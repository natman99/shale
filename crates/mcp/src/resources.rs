//! The resources `shale-mcp` offers: context an agent reads rather than a change it
//! asks for (roadmap P2.3).
//!
//! [`STATUS_URI`] is the pollable contract for "is a Shale window up, and which live
//! tools does that make available", so an agent can notice the app open or close
//! mid-session without guessing from a failing call. [`COMMANDS_URI`] is the command
//! table P2.2 generates into `docs/commands.md` — a file rather than only a function
//! precisely because this binary cannot link against the app.

use anyhow::{Context as _, Result};
use serde_json::{json, Value};

use crate::live;
use crate::Server;

/// Whether a Shale window is up and what that makes available.
pub(crate) const STATUS_URI: &str = "shale://status";
/// Every command Shale answers to, with its keys and its `:` spellings.
pub(crate) const COMMANDS_URI: &str = "shale://commands";
/// What this connection is allowed to touch, so an agent can read its own scope
/// instead of probing for it one refusal at a time (roadmap P2.4).
pub(crate) const ACCESS_URI: &str = "shale://access";

/// What `resources/list` offers. The command table is only there when the generated
/// file can be found: a resource that fails when read is worse than one that is not
/// mentioned.
pub(crate) fn list(server: &Server) -> Vec<Value> {
    let mut items = vec![
        json!({
            "uri": STATUS_URI,
            "name": "status",
            "title": "Shale's live status",
            "description": "Whether a Shale window is answering, its version, and the live tools that makes available.",
            "mimeType": "application/json",
        }),
        json!({
            "uri": ACCESS_URI,
            "name": "access",
            "title": "This connection's scope",
            "description": "The profile this server is running as: the paths it may read and write, what is denied, and the tools it may call.",
            "mimeType": "application/json",
        }),
    ];
    if server.docs.is_some() {
        items.push(json!({
            "uri": COMMANDS_URI,
            "name": "commands",
            "title": "Shale's commands",
            "description": "The command table: every id, its keys, and its command-line spellings.",
            "mimeType": "text/markdown",
        }));
    }
    items
}

/// What `resources/read` answers with. An unknown URI is an error rather than an
/// empty answer, so an agent that invented a URI learns that it did.
pub(crate) fn read(server: &Server, uri: &str) -> Result<Value> {
    match uri {
        STATUS_URI => {
            let mut status = live::status(server.vault.root());
            // The status contract has to agree with `tools/list`, which the profile
            // already narrowed; an agent that read the two differently would chase a
            // tool the server would refuse.
            status
                .live_tools
                .retain(|tool| server.profile.allows_tool(tool));
            let text = serde_json::to_string_pretty(&status).context("rendering the status")?;
            Ok(json!({
                "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }]
            }))
        }
        ACCESS_URI => {
            let text = serde_json::to_string_pretty(&json!({
                "profile": server.profile_name,
                "read": server.profile.read,
                "write": server.profile.write,
                "deny": server.profile.deny,
                "tools": server.profile.tools,
                "review": server.profile.review,
                "tools_offered": tool_names(server),
            }))
            .context("rendering the scope")?;
            Ok(json!({
                "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }]
            }))
        }
        COMMANDS_URI => {
            let Some(path) = &server.docs else {
                anyhow::bail!("no command table to serve: pass --docs <path to commands.md>")
            };
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading the command table at {}", path.display()))?;
            Ok(json!({
                "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": text }]
            }))
        }
        other => anyhow::bail!("no such resource: {other}"),
    }
}

/// The tools this server will actually run, which is what `shale://access` reports
/// beside the profile's own lists: the offer, not just the rule.
fn tool_names(server: &Server) -> Vec<String> {
    crate::tool_definitions(server)
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shale_core::Vault;

    fn server(docs: Option<std::path::PathBuf>) -> Result<(tempfile::TempDir, Server)> {
        scoped(docs, shale_core::config::McpProfile::default(), None)
    }

    fn scoped(
        docs: Option<std::path::PathBuf>,
        profile: shale_core::config::McpProfile,
        name: Option<String>,
    ) -> Result<(tempfile::TempDir, Server)> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        Ok((
            dir,
            Server {
                vault,
                docs,
                profile,
                profile_name: name,
            },
        ))
    }

    /// With no window up, the status resource still answers — it says so plainly,
    /// which is the point of it being a resource rather than a failing tool call.
    #[test]
    fn the_status_resource_answers_when_no_window_is_up() -> Result<()> {
        let (_d, server) = server(None)?;
        let result = read(&server, STATUS_URI)?;
        let text = result["contents"][0]["text"].as_str().unwrap();
        let status: shale_core::live::Status = serde_json::from_str(text).unwrap();
        assert!(!status.app_running);
        assert!(status.live_tools.is_empty());
        Ok(())
    }

    /// The command table is offered only when there is a file behind it, and reads
    /// back as markdown when there is.
    #[test]
    fn the_command_table_is_offered_only_when_it_exists() -> Result<()> {
        let (_d, without) = server(None)?;
        assert_eq!(list(&without).len(), 2, "status and access");
        let err = format!("{:#}", read(&without, COMMANDS_URI).unwrap_err());
        assert!(err.contains("--docs"), "and says how to get one: {err}");

        let dir = tempfile::tempdir()?;
        let docs = dir.path().join("commands.md");
        std::fs::write(&docs, "# Commands\n\n`note.save`\n")?;
        let (_d, with) = server(Some(docs))?;
        assert_eq!(list(&with).len(), 3);
        let result = read(&with, COMMANDS_URI)?;
        assert_eq!(result["contents"][0]["mimeType"], "text/markdown");
        assert!(result["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("note.save"));
        Ok(())
    }

    /// An agent's first question is what it may do, so the scope it was started with
    /// is readable rather than something it has to discover by being refused.
    #[test]
    fn a_connection_can_read_its_own_scope() -> Result<()> {
        let profile = shale_core::config::McpProfile {
            read: vec!["**".into()],
            write: vec!["inbox/".into()],
            deny: vec!["journal/".into()],
            tools: vec!["read_note".into(), "write_note".into()],
            review: false,
        };
        let (_d, server) = scoped(None, profile, Some("writing-help".into()))?;
        let result = read(&server, ACCESS_URI)?;
        let text = result["contents"][0]["text"].as_str().unwrap();
        let access: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(access["profile"], "writing-help");
        assert_eq!(access["write"], json!(["inbox/"]));
        assert_eq!(access["deny"], json!(["journal/"]));
        assert_eq!(
            access["tools_offered"],
            json!(["read_note", "write_note"]),
            "the offer, not just the rule"
        );
        Ok(())
    }

    #[test]
    fn a_resource_that_was_not_invented_is_refused() {
        let (_dir, server) = server(None).unwrap();
        let err = format!("{:#}", read(&server, "shale://secrets").unwrap_err());
        assert!(err.contains("no such resource"), "{err}");
    }
}
