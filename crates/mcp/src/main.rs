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
//! Diagnostics go to stderr. Stdout is the protocol.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::Parser;
use serde_json::{json, Value};
use shale_core::Vault;

mod live;
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
    /// The generated command table (`docs/commands.md`), served as
    /// `shale://commands`. Found relative to the repo by default; pass this when the
    /// binary has been installed somewhere the repo is not.
    #[arg(long)]
    docs: Option<PathBuf>,
}

/// What this server can answer with: the vault, and the generated command table if
/// one can be found.
struct Server {
    /// The vault every path is confined to.
    vault: Vault,
    /// `docs/commands.md`, when it was found. `None` means `shale://commands` is not
    /// offered rather than offered and failing.
    docs: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let vault = Vault::open(&args.vault).context("opening vault")?;
    let server = Server {
        vault,
        docs: docs_path(&args),
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
            // Resources as well as tools: the status contract and the command table
            // are context to read, not actions to take.
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

/// The tools on offer right now. The live ones depend on a window answering at this
/// moment, which is why they are asked about per list rather than decided once at
/// startup: an agent's session outlives the window it started against.
fn tool_definitions(server: &Server) -> Vec<Value> {
    let mut tools = vault_tools::definitions();
    if live::available(server.vault.root()) {
        tools.extend(live::definitions());
    }
    tools
}

/// Run one tool: the vault's own first, then the live ones, then say it is unknown.
fn call_tool(server: &Server, name: &str, arguments: &Value) -> Result<Value> {
    if let Some(result) = vault_tools::call(&server.vault, name, arguments)? {
        return Ok(result);
    }
    if shale_core::live::LIVE_TOOLS.contains(&name) {
        return live::call(server.vault.root(), name);
    }
    anyhow::bail!("unknown tool: {name}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(docs: Option<PathBuf>) -> Result<(tempfile::TempDir, Server)> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        vault.write("a.md", "Hello [[b]]")?;
        vault.write("b.md", "World")?;
        Ok((dir, Server { vault, docs }))
    }

    #[test]
    fn initialize_handshake() -> Result<()> {
        let (_d, server) = fixture(None)?;
        let result = dispatch(&server, "initialize", &json!({}))?;
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], "shale-mcp");
        assert!(
            result["capabilities"]["resources"].is_object(),
            "the resources half is advertised, or a client will never ask for it"
        );
        Ok(())
    }

    /// Without a window answering, the five vault tools and nothing else.
    #[test]
    fn tools_list_without_a_window_is_the_vault_half() -> Result<()> {
        let (_d, server) = fixture(None)?;
        let result = dispatch(&server, "tools/list", &json!({}))?;
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 5);
        assert!(
            !tools
                .iter()
                .any(|t| t["name"].as_str().unwrap().starts_with("live_")),
            "no live tool is offered that cannot be called"
        );
        Ok(())
    }

    #[test]
    fn resources_list_and_read() -> Result<()> {
        let (_d, server) = fixture(None)?;
        let listed = dispatch(&server, "resources/list", &json!({}))?;
        assert_eq!(listed["resources"].as_array().unwrap().len(), 1);

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
        let (_dir, server) = fixture(None).unwrap();
        let err = format!(
            "{:#}",
            call_tool(&server, "make_coffee", &json!({})).unwrap_err()
        );
        assert!(err.contains("unknown tool"), "{err}");
    }

    /// The docs lookup: an explicit path that is not there costs a warning and a
    /// missing resource, not a server that will not start.
    #[test]
    fn a_missing_command_table_is_survivable() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let args = Args {
            vault: dir.path().to_path_buf(),
            docs: Some(dir.path().join("nope.md")),
        };
        assert!(docs_path(&args).is_none());

        let docs = dir.path().join("commands.md");
        std::fs::write(&docs, "# Commands\n")?;
        let args = Args {
            vault: dir.path().to_path_buf(),
            docs: Some(docs.clone()),
        };
        assert_eq!(docs_path(&args).as_deref(), Some(docs.as_path()));
        Ok(())
    }
}
