//! The vault's own tools: what `shale-mcp` can answer without asking a running
//! window anything (roadmap P2.3), and what the chosen **profile** lets it answer
//! (roadmap P2.4).
//!
//! Every path here goes through [`Vault`], which is what confines an agent to the
//! vault and normalises what it asks for, and then through the profile, which is what
//! confines it *inside* the vault. There is deliberately no second path logic in this
//! file — the app's file code and these tools answer for the same rules.
//!
//! Enforcement is server-side only: a client that claims a wider scope than the
//! profile grants gets the profile. And a refusal is an answer the agent can read
//! (`isError` plus a sentence), not a protocol failure, because an agent that learns
//! *why* is one that stops trying.

use anyhow::{Context as _, Result};
use serde_json::{json, Value};
use shale_core::search::SearchOptions;

use crate::refusal;
use crate::Server;

/// The tools, in the order an agent sees them in `tools/list`. The profile's
/// allowlist is applied by the caller, which is also where the live tools are
/// decided, so one list is filtered one way.
pub(crate) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "list_notes",
            "description": "List all markdown notes in the vault, by vault-relative path.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "read_note",
            "description": "Read a markdown note by vault-relative path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Vault-relative path, e.g. notes/idea.md" }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "write_note",
            "description": "Create or overwrite a file in the vault: a markdown note, or a saved query's .shq (see list_queries) — there is one write door and the profile's paths and the review queue govern it. Parent directories are created automatically.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Vault-relative path" },
                    "content": { "type": "string", "description": "Full markdown content" }
                },
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "search_notes",
            "description": "Search all notes for a query, returning matching lines with file and line numbers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "whole_word": { "type": "boolean" },
                    "case_sensitive": { "type": "boolean" },
                    "max_hits": { "type": "integer" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "backlinks",
            "description": "List notes that contain wiki-links to the given note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Vault-relative path of the note" }
                },
                "required": ["path"]
            }
        }),
    ]
}

/// Run one of them. `Ok(None)` means the name is not a vault tool, which lets the
/// caller try the live ones before calling the tool unknown.
pub(crate) fn call(server: &Server, name: &str, arguments: &Value) -> Result<Option<Value>> {
    let vault = &server.vault;
    let profile = &server.profile;
    let text_result = |text: String| json!({ "content": [{ "type": "text", "text": text }] });

    match name {
        "list_notes" => {
            let entries = profile.visible(&vault.list_markdown()?);
            let lines: Vec<String> = entries
                .iter()
                .map(|e| e.path.display().to_string())
                .collect();
            Ok(Some(text_result(lines.join("\n"))))
        }
        "read_note" => {
            let path = arg_str(arguments, "path")?;
            if !profile.can_read(path.as_ref()) {
                return Ok(Some(refusal(&format!(
                    "{} may not read {path}",
                    server.who()
                ))));
            }
            Ok(Some(text_result(vault.read(path)?)))
        }
        "write_note" => {
            let path = arg_str(arguments, "path")?;
            let content = arg_str(arguments, "content")?;
            if !profile.can_write(path.as_ref()) {
                return Ok(Some(refusal(&format!(
                    "{} may not write {path}",
                    server.who()
                ))));
            }
            // A profile that asked for review does not write: the change becomes a
            // proposal in the queue, and the user decides. This is the flip of the
            // fail-closed refusal P2.4 shipped with, now that P2.5a can keep the
            // promise.
            if profile.review {
                return Ok(Some(crate::pending_tools::propose(server, path, content)?));
            }
            vault.write(path, content)?;
            Ok(Some(text_result(format!("wrote {}", path))))
        }
        "search_notes" => {
            let query = arg_str(arguments, "query")?;
            let opts = SearchOptions {
                whole_word: arguments
                    .get("whole_word")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                case_sensitive: arguments
                    .get("case_sensitive")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                max_hits: arguments
                    .get("max_hits")
                    .and_then(Value::as_u64)
                    .map(|n| n as usize),
            };
            // The scope is applied to the entries *before* they are searched, so a
            // hidden note cannot contribute a hit — and `max_hits` counts the hits
            // the agent is allowed to see rather than the ones it is not.
            let entries = profile.visible(&vault.list_markdown()?);
            let hits = shale_core::search::search(vault, &entries, query, &opts)?;
            let rendered: Vec<String> = hits
                .iter()
                .map(|h| format!("{}:{}: {}", h.path.display(), h.line_number, h.line_text))
                .collect();
            Ok(Some(text_result(rendered.join("\n"))))
        }
        "backlinks" => {
            let path = arg_str(arguments, "path")?;
            if !profile.can_read(path.as_ref()) {
                return Ok(Some(refusal(&format!(
                    "{} may not read {path}",
                    server.who()
                ))));
            }
            let entries = profile.visible(&vault.list_markdown()?);
            let links = vault.backlinks(path, &entries)?;
            let lines: Vec<String> = links.iter().map(|e| e.path.display().to_string()).collect();
            Ok(Some(text_result(lines.join("\n"))))
        }
        _ => Ok(None),
    }
}

/// A required string argument, named in the error when it is missing.
fn arg_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("missing string argument: {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shale_core::config::McpProfile;

    /// A server over a vault with a public note, a draft and a journal entry, so a
    /// scope can be shown narrowing something rather than asserted in the abstract.
    fn server(profile: McpProfile) -> Result<(tempfile::TempDir, Server)> {
        let dir = tempfile::tempdir()?;
        let vault = shale_core::Vault::open(dir.path())?;
        vault.write("inbox/idea.md", "Hello [[draft]]")?;
        vault.write("drafts/draft.md", "A draft about [[idea]]")?;
        vault.write("journal/february.md", "Private thoughts about [[idea]]")?;
        Ok((
            dir,
            Server {
                vault,
                docs: None,
                profile,
                profile_name: None,
            },
        ))
    }

    fn fixture() -> Result<(tempfile::TempDir, Server)> {
        server(McpProfile::default())
    }

    /// The scoped profile P2.4 exists for: free rein over `inbox` and `drafts`,
    /// nothing near the journal.
    fn writing_help() -> McpProfile {
        McpProfile {
            read: vec!["inbox/".into(), "drafts/".into()],
            write: vec!["inbox/".into(), "drafts/".into()],
            deny: Vec::new(),
            tools: Vec::new(),
            review: false,
        }
    }

    fn text(result: &Value) -> String {
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn read_and_write_tools() -> Result<()> {
        let (_d, server) = fixture()?;
        let result = call(&server, "read_note", &json!({ "path": "inbox/idea.md" }))?
            .expect("read_note is a vault tool");
        assert_eq!(text(&result), "Hello [[draft]]");

        call(
            &server,
            "write_note",
            &json!({ "path": "inbox/new.md", "content": "new" }),
        )?
        .expect("write_note is a vault tool");
        assert_eq!(server.vault.read("inbox/new.md")?, "new");
        Ok(())
    }

    #[test]
    fn search_tool() -> Result<()> {
        let (_d, server) = fixture()?;
        let result = call(&server, "search_notes", &json!({ "query": "draft" }))?
            .expect("search_notes is a vault tool");
        assert!(text(&result).contains("inbox/idea.md:1: Hello [[draft]]"));
        Ok(())
    }

    #[test]
    fn backlinks_tool() -> Result<()> {
        let (_d, server) = fixture()?;
        let result = call(&server, "backlinks", &json!({ "path": "drafts/draft.md" }))?
            .expect("backlinks is a vault tool");
        assert_eq!(text(&result), "inbox/idea.md");
        Ok(())
    }

    /// The whole point of a profile: the same five tools, a smaller vault.
    #[test]
    fn a_scope_hides_what_it_should_and_says_so() -> Result<()> {
        let (_d, server) = server(writing_help())?;

        let listed = call(&server, "list_notes", &json!({}))?.expect("a vault tool");
        assert_eq!(
            text(&listed),
            "drafts/draft.md\ninbox/idea.md",
            "the journal is not in the list at all"
        );

        let refused = call(
            &server,
            "read_note",
            &json!({ "path": "journal/february.md" }),
        )?
        .expect("read_note is a vault tool");
        assert_eq!(refused["isError"], true);
        assert!(
            text(&refused).contains("may not read journal/february.md"),
            "and it says which path: {refused}"
        );

        let refused = call(
            &server,
            "write_note",
            &json!({ "path": "journal/february.md", "content": "edited" }),
        )?
        .expect("write_note is a vault tool");
        assert_eq!(refused["isError"], true);
        assert_eq!(
            server.vault.read("journal/february.md")?,
            "Private thoughts about [[idea]]",
            "a refused write writes nothing"
        );
        Ok(())
    }

    /// A search that runs over everything and filters afterwards would leak both hits
    /// and hit counts, so the scope is applied to the entries first.
    #[test]
    fn a_search_never_reports_a_note_the_scope_hides() -> Result<()> {
        let (_d, server) = server(writing_help())?;
        let result = call(&server, "search_notes", &json!({ "query": "idea" }))?
            .expect("search_notes is a vault tool");
        let hits = text(&result);
        assert!(hits.contains("drafts/draft.md"));
        assert!(
            !hits.contains("journal"),
            "the journal also mentions it: {hits}"
        );
        Ok(())
    }

    /// Backlinks is the same leak in a different shape: the answer is a list of
    /// paths, and the target itself is a path.
    #[test]
    fn backlinks_are_scoped_both_ways() -> Result<()> {
        let (_d, server) = server(writing_help())?;
        // Asking who links to a hidden note is refused…
        let refused = call(
            &server,
            "backlinks",
            &json!({ "path": "journal/february.md" }),
        )?
        .expect("backlinks is a vault tool");
        assert_eq!(refused["isError"], true);

        // …and a hidden note never appears in the answer about a visible one.
        let result = call(&server, "backlinks", &json!({ "path": "drafts/draft.md" }))?
            .expect("backlinks is a vault tool");
        assert_eq!(text(&result), "inbox/idea.md");
        Ok(())
    }

    /// P2.5a: a profile that asked for review gets its write queued, not refused and
    /// not written.
    #[test]
    fn a_profile_that_asks_for_review_has_its_writes_queued() -> Result<()> {
        let mut profile = writing_help();
        profile.review = true;
        let (_d, server) = server(profile)?;

        let queued = call(
            &server,
            "write_note",
            &json!({ "path": "inbox/idea.md", "content": "overwritten" }),
        )?
        .expect("write_note is a vault tool");
        assert_eq!(queued["isError"], Value::Null);
        assert!(
            text(&queued).starts_with("queued for review as "),
            "{queued}"
        );
        assert_eq!(server.vault.read("inbox/idea.md")?, "Hello [[draft]]");

        // Reading is still fine: review is about writes, not about seeing.
        let read = call(&server, "read_note", &json!({ "path": "inbox/idea.md" }))?
            .expect("read_note is a vault tool");
        assert_eq!(read["isError"], Value::Null);
        Ok(())
    }

    /// A name that is not one of these is not an error here: it is the caller's
    /// chance to try the live tools before giving up.
    #[test]
    fn a_name_that_is_not_a_vault_tool_is_left_for_the_caller() -> Result<()> {
        let (_d, server) = fixture()?;
        assert!(call(&server, "live_note", &json!({}))?.is_none());
        Ok(())
    }

    #[test]
    fn path_traversal_rejected() {
        let (_dir, server) = fixture().unwrap();
        let result = call(&server, "read_note", &json!({ "path": "../secret" }));
        assert!(
            result.is_err(),
            "the vault's own wall still holds: {result:?}"
        );
    }
}
