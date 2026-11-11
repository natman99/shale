//! `shale-mcp`: a Model Context Protocol server over stdio JSON-RPC 2.0.
//!
//! Protocol reference: https://modelcontextprotocol.io
//! The subset implemented here: `initialize`, `notifications/initialized`, `ping`,
//! `tools/list`, `tools/call`, `resources/list`, `resources/read`.
//!
//! Two modes, joined by a probe (roadmap P2.3):
//!
//! - **The vault**, which this process answers for itself — list, read, write,
//!   search, backlinks ([`vault_tools`]).
//! - **The live window**, which it can only ask about. If a Shale window is
//!   answering the loopback port named in `.shale/mcp.lock`, the live tools are
//!   offered and proxy that answer ([`live`]); if not, they are not offered at all
//!   and [`resources`] says so. The lockfile is a hint, never an authority: a pid can
//!   be reused and a port taken over, so the probe is what decides.
//!
//! The vault and its index stay useful either way, which is what the "fall back with
//! an explicit *live tools unavailable*" rule is for.
//!
//! One scope, decided at startup and enforced here (roadmap P2.4): `--profile` picks
//! a `[mcp.profile."<name>"]` from the vault's config, and that profile decides which
//! tools exist and which paths inside the vault are visible at all. A connection with
//! no profile has the whole vault, which is what this binary has always had.
//! Enforcement is server-side only — a client claiming more gets what the profile
//! says — and every path still goes through [`Vault`]'s confinement.
//!
//! Diagnostics go to stderr. Stdout is the protocol.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::Parser;
use serde_json::{json, Value};
use shale_core::config::McpProfile;
use shale_core::Vault;

mod live;
mod pending_tools;
mod query_tools;
mod resources;
mod vault_tools;

/// MCP protocol version implemented by this server.
const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Parser, Debug)]
#[command(name = "shale-mcp", about = "MCP server for a Shale vault")]
struct Args {
    /// Path to the vault directory.
    #[arg(long)]
    vault: PathBuf,
    /// A `[mcp.profile."<name>"]` from the vault's config: the paths and tools this
    /// connection may reach. Without it the connection has the whole vault.
    #[arg(long)]
    profile: Option<String>,
    /// The generated command table (`docs/commands.md`), served as
    /// `shale://commands`. Found relative to the repo by default; pass this when the
    /// binary has been installed somewhere the repo is not.
    #[arg(long)]
    docs: Option<PathBuf>,
}

/// What this server can answer with: the vault, the scope this connection was
/// started in, and the generated command table if one can be found.
struct Server {
    /// The vault every path is confined to.
    vault: Vault,
    /// `docs/commands.md`, when it was found. `None` means `shale://commands` is not
    /// offered rather than offered and failing.
    docs: Option<PathBuf>,
    /// What this connection may touch, which is what every tool call is checked
    /// against before it does anything.
    profile: McpProfile,
    /// The profile's name, for the messages that have to say who refused. `None` is
    /// an unprofiled connection.
    profile_name: Option<String>,
}

impl Server {
    /// Who to name in a refusal. Naming the profile is what makes the refusal
    /// actionable: the agent (or the human reading its log) knows which scope to
    /// change rather than guessing which rule fired.
    fn who(&self) -> String {
        match &self.profile_name {
            Some(name) => format!("the profile \"{name}\""),
            None => "this connection (no profile)".to_string(),
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let vault = Vault::open(&args.vault).context("opening vault")?;
    // The vault's config is where the profiles are, so the server reads the same
    // file the app does. A vault with no config has no profiles, which is the
    // whole-vault scope this binary has always had.
    let config = shale_core::Config::load(&args.vault).context("loading config")?;
    let profile = chosen_profile(&config, args.profile.as_deref())?;
    let profile_name = args.profile.clone();
    if let Some(name) = &profile_name {
        eprintln!(
            "shale-mcp: running as profile \"{name}\" over {}",
            vault.root().display()
        );
    }
    let server = Server {
        vault,
        docs: docs_path(&args),
        profile,
        profile_name,
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line.context("reading stdin")?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(err) => {
                let msg = format!("parse error: {err}");
                eprintln!("shale-mcp: {msg}");
                continue;
            }
        };

        // Notifications (no `id`) get no response.
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let params = request.get("params").cloned().unwrap_or(json!({}));

        let response = match dispatch(&server, &method, &params) {
            Ok(result) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            }),
            Err(err) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32603, "message": err.to_string() },
            }),
        };
        writeln!(stdout, "{response}").context("writing stdout")?;
        stdout.flush().context("flushing stdout")?;
    }
    Ok(())
}

/// The profile this connection runs as.
///
/// A name that is not in the config is a startup failure rather than a quiet
/// fallback: a typo in a permission flag must not turn into a wider scope than the
/// user meant. The names that *are* there get listed, because that is the question
/// the user is about to ask.
fn chosen_profile(config: &shale_core::Config, name: Option<&str>) -> Result<McpProfile> {
    let Some(name) = name else {
        return Ok(McpProfile::default());
    };
    match config.mcp.profile.get(name) {
        Some(profile) => Ok(profile.clone()),
        None => {
            let known: Vec<&String> = config.mcp.profile.keys().collect();
            if known.is_empty() {
                anyhow::bail!(
                    "no profile \"{name}\": this vault defines none. Add \
                     [mcp.profile.\"{name}\"] to .shale/config.toml"
                );
            }
            anyhow::bail!(
                "no profile \"{name}\". This vault defines: {}",
                known
                    .iter()
                    .map(|known| format!("\"{known}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

/// A tool's "no": readable, and `isError` so a client knows the call did not do what
/// it asked. Refusals are answers rather than protocol failures, because an agent
/// that can read *why* stops trying instead of retrying the same path.
pub(crate) fn refusal(why: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": why }],
        "isError": true,
    })
}

/// Where the generated command table is, if it can be found at all.
///
/// The two default candidates cover how this binary gets run in practice: from a
/// build directory inside the repo, and from a shell sitting in the repo. A copy
/// installed somewhere else can see neither, so it is given `--docs` — and a table
/// that is simply missing costs a resource, not the vault tools.
fn docs_path(args: &Args) -> Option<PathBuf> {
    if let Some(path) = &args.docs {
        if path.is_file() {
            return Some(path.clone());
        }
        eprintln!(
            "shale-mcp: no command table at {}, so shale://commands is not offered",
            path.display()
        );
        return None;
    }
    let candidates = [
        // Built here: `<repo>/target/...` cannot be assumed, but the crate directory
        // is known at compile time, and it is two steps from `docs/`.
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/commands.md"
        )),
        PathBuf::from("docs/commands.md"),
    ];
    candidates.into_iter().find(|path| path.is_file())
}

/// One request's answer.
fn dispatch(server: &Server, method: &str, params: &Value) -> Result<Value> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            // Resources as well as tools: the status contract, the command table and
            // the scope this connection has are context to read, not actions to take.
            "capabilities": { "tools": {}, "resources": {} },
            "serverInfo": {
                "name": "shale-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions(server) })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .context("tools/call missing name")?;
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            call_tool(server, name, &arguments)
        }
        "resources/list" => Ok(json!({ "resources": resources::list(server) })),
        "resources/read" => {
            let uri = params
                .get("uri")
                .and_then(Value::as_str)
                .context("resources/read missing uri")?;
            resources::read(server, uri)
        }
        other => Ok(json!({ "error": format!("unknown method: {other}") })),
    }
}

/// The tools on offer right now: what a window answering makes possible, narrowed by
/// what the profile allows. Both halves are asked about per list rather than decided
/// once at startup, because an agent's session outlives the window it started
/// against — and because a tool that is not listed is a tool the client will not try.
fn tool_definitions(server: &Server) -> Vec<Value> {
    let allowed = |tools: Vec<Value>| {
        tools
            .into_iter()
            .filter(|tool| {
                server
                    .profile
                    .allows_tool(tool["name"].as_str().unwrap_or_default())
            })
            .collect::<Vec<_>>()
    };
    let mut tools = allowed(vault_tools::definitions());
    tools.extend(allowed(query_tools::definitions()));
    tools.extend(allowed(pending_tools::definitions()));
    if live::available(server.vault.root()) {
        tools.extend(allowed(live::definitions()));
    }
    tools
}

/// Run one tool: the profile's allowlist first, then the vault's own, then the live
/// ones, then say it is unknown.
fn call_tool(server: &Server, name: &str, arguments: &Value) -> Result<Value> {
    if !server.profile.allows_tool(name) {
        return Ok(refusal(&format!(
            "{} may not call {name}. Ask for it in the profile, or call one of: {}",
            server.who(),
            tool_definitions(server)
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if let Some(result) = vault_tools::call(server, name, arguments)? {
        return Ok(result);
    }
    if let Some(result) = query_tools::call(server, name, arguments)? {
        return Ok(result);
    }
    match name {
        "pending_edits" => return pending_tools::list(server, arguments),
        "retract_pending" => return pending_tools::retract(server, arguments),
        _ => {}
    }
    if shale_core::live::LIVE_TOOLS.contains(&name) {
        return live::call(server.vault.root(), &server.profile, name);
    }
    anyhow::bail!("unknown tool: {name}");
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn fixture(
        docs: Option<PathBuf>,
        profile: McpProfile,
        name: Option<String>,
    ) -> (tempfile::TempDir, Server) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        vault.write("a.md", "Hello [[b]]").unwrap();
        vault.write("b.md", "World").unwrap();
        (
            dir,
            Server {
                vault,
                docs,
                profile,
                profile_name: name,
            },
        )
    }

    #[test]
    fn initialize_handshake() -> Result<()> {
        let (_d, server) = fixture(None, McpProfile::default(), None);
        let result = dispatch(&server, "initialize", &json!({}))?;
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], "shale-mcp");
        assert!(
            result["capabilities"]["resources"].is_object(),
            "the resources half is advertised, or a client will never ask for it"
        );
        Ok(())
    }

    /// Without a window answering: the vault tools, the query tools, the queue tools,
    /// and nothing else. Named rather than counted — a count cannot say *which* door is
    /// missing, and a door quietly disappearing from `tools/list` is a feature an agent
    /// never learns to ask for.
    #[test]
    fn tools_list_without_a_window_is_the_vault_half() -> Result<()> {
        let (_d, server) = fixture(None, McpProfile::default(), None);
        let result = dispatch(&server, "tools/list", &json!({}))?;
        let names: Vec<String> = result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            [
                "list_notes",
                "read_note",
                "write_note",
                "search_notes",
                "backlinks",
                "list_queries",
                "read_query",
                "run_query",
                "pending_edits",
                "retract_pending",
            ],
            "the vault half, in the order an agent sees it"
        );
        assert!(
            !names.iter().any(|name| name.starts_with("live_")),
            "no live tool is offered that cannot be called"
        );
        Ok(())
    }

    /// The allowlist narrows `tools/list` as well as `tools/call`, so a client is
    /// never shown a tool that will only refuse.
    #[test]
    fn a_tool_list_narrows_the_list_as_well_as_the_call() -> Result<()> {
        let profile = McpProfile {
            tools: vec!["read_note".into(), "search_notes".into()],
            ..McpProfile::default()
        };
        let (_d, server) = fixture(None, profile, Some("quiet-reader".into()));

        let listed = dispatch(&server, "tools/list", &json!({}))?;
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["read_note", "search_notes"]);

        let refused = dispatch(
            &server,
            "tools/call",
            &json!({ "name": "write_note", "arguments": { "path": "a.md", "content": "x" } }),
        )?;
        assert_eq!(refused["isError"], true);
        assert!(
            refused["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("quiet-reader"),
            "a refusal names the scope that refused: {refused}"
        );
        assert_eq!(server.vault.read("a.md")?, "Hello [[b]]");
        Ok(())
    }

    #[test]
    fn resources_list_and_read() -> Result<()> {
        let (_d, server) = fixture(None, McpProfile::default(), None);
        let listed = dispatch(&server, "resources/list", &json!({}))?;
        assert_eq!(listed["resources"].as_array().unwrap().len(), 2);

        let read = dispatch(
            &server,
            "resources/read",
            &json!({ "uri": resources::STATUS_URI }),
        )?;
        assert!(read["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("app_running"));
        Ok(())
    }

    #[test]
    fn a_tool_neither_half_knows_is_an_error() {
        let (_dir, server) = fixture(None, McpProfile::default(), None);
        let err = format!(
            "{:#}",
            call_tool(&server, "make_coffee", &json!({})).unwrap_err()
        );
        assert!(err.contains("unknown tool"), "{err}");
    }

    /// A typo in `--profile` must not become a wider scope than the user meant, and
    /// the error has to say what the vault does define.
    #[test]
    fn a_profile_that_is_not_there_stops_the_server_starting() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir_all(dir.path().join(".shale"))?;
        std::fs::write(
            dir.path().join(".shale/config.toml"),
            "[mcp.profile.\"writing-help\"]\nwrite = [\"inbox/\"]\n\
             [mcp.profile.\"reader\"]\ntools = [\"read_note\"]\n",
        )?;
        let config = shale_core::Config::load(dir.path())?;

        let chosen = chosen_profile(&config, Some("writing-help"))?;
        assert_eq!(chosen.write, vec!["inbox/"]);

        let err = format!(
            "{:#}",
            chosen_profile(&config, Some("writting-help")).unwrap_err()
        );
        assert!(err.contains("\"writing-help\""), "and it lists: {err}");
        assert!(err.contains("\"reader\""), "both of them: {err}");

        // A vault with no profiles at all says how to add one.
        let empty = shale_core::Config::load(tempfile::tempdir()?.path())?;
        let err = format!(
            "{:#}",
            chosen_profile(&empty, Some("anything")).unwrap_err()
        );
        assert!(err.contains("defines none"), "{err}");
        Ok(())
    }

    /// The docs lookup: an explicit path that is not there costs a warning and a
    /// missing resource, not a server that will not start.
    #[test]
    fn a_missing_command_table_is_survivable() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let args = Args {
            vault: dir.path().to_path_buf(),
            profile: None,
            docs: Some(dir.path().join("nope.md")),
        };
        assert!(docs_path(&args).is_none());

        let docs = dir.path().join("commands.md");
        std::fs::write(&docs, "# Commands\n")?;
        let args = Args {
            vault: dir.path().to_path_buf(),
            profile: None,
            docs: Some(docs.clone()),
        };
        assert_eq!(docs_path(&args).as_deref(), Some(docs.as_path()));
        Ok(())
    }

    /// An unprofiled connection is named honestly in a refusal, so a log line does
    /// not send a reader looking for a profile that was never set.
    #[test]
    fn a_refusal_says_who_refused() {
        let (_dir, server) = fixture(None, McpProfile::default(), None);
        assert_eq!(server.who(), "this connection (no profile)");
        let (_dir, server) = fixture(None, McpProfile::default(), Some("reader".into()));
        assert_eq!(server.who(), "the profile \"reader\"");
    }

    /// The confinement is the vault's, not the profile's: a path that escapes the
    /// vault is refused whatever the scope says.
    #[test]
    fn a_scope_never_widens_the_vaults_own_wall() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let outside = Path::new(dir.path()).join("..").join("secret.md");
        assert!(vault
            .read(outside.strip_prefix(dir.path()).unwrap())
            .is_err());
    }
}
