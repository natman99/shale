//! The vault's own tools: what `shale-mcp` can answer without asking a running
//! window anything (roadmap P2.3).
//!
//! Every path here goes through [`Vault`], which is what confines an agent to the
//! vault and normalises what it asks for. There is deliberately no second path
//! logic in this file — the app's file code and these tools answer for the same
//! rules, and P2.4's profiles will enforce their allowlists in the same place.

use anyhow::{Context as _, Result};
use serde_json::{json, Value};
use shale_core::search::SearchOptions;
use shale_core::Vault;

/// The tools, in the order an agent sees them in `tools/list`.
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
            "description": "Create or overwrite a markdown note. Parent directories are created automatically.",
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
pub(crate) fn call(vault: &Vault, name: &str, arguments: &Value) -> Result<Option<Value>> {
    let text_result = |text: String| json!({ "content": [{ "type": "text", "text": text }] });

    match name {
        "list_notes" => {
            let entries = vault.list_markdown()?;
            let lines: Vec<String> = entries
                .iter()
                .map(|e| e.path.display().to_string())
                .collect();
            Ok(Some(text_result(lines.join("\n"))))
        }
        "read_note" => {
            let path = arg_str(arguments, "path")?;
            Ok(Some(text_result(vault.read(path)?)))
        }
        "write_note" => {
            let path = arg_str(arguments, "path")?;
            let content = arg_str(arguments, "content")?;
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
            let entries = vault.list_markdown()?;
            let hits = shale_core::search::search(vault, &entries, query, &opts)?;
            let rendered: Vec<String> = hits
                .iter()
                .map(|h| format!("{}:{}: {}", h.path.display(), h.line_number, h.line_text))
                .collect();
            Ok(Some(text_result(rendered.join("\n"))))
        }
        "backlinks" => {
            let path = arg_str(arguments, "path")?;
            let entries = vault.list_markdown()?;
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

    fn fixture() -> Result<(tempfile::TempDir, Vault)> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        vault.write("a.md", "Hello [[b]]")?;
        vault.write("b.md", "World")?;
        Ok((dir, vault))
    }

    #[test]
    fn read_and_write_tools() -> Result<()> {
        let (_d, vault) = fixture()?;
        let result = call(&vault, "read_note", &json!({ "path": "a.md" }))?
            .expect("read_note is a vault tool");
        assert_eq!(result["content"][0]["text"], "Hello [[b]]");

        call(
            &vault,
            "write_note",
            &json!({ "path": "c.md", "content": "new" }),
        )?
        .expect("write_note is a vault tool");
        assert_eq!(vault.read("c.md")?, "new");
        Ok(())
    }

    #[test]
    fn search_tool() -> Result<()> {
        let (_d, vault) = fixture()?;
        let result = call(&vault, "search_notes", &json!({ "query": "world" }))?
            .expect("search_notes is a vault tool");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("b.md:1: World"));
        Ok(())
    }

    #[test]
    fn backlinks_tool() -> Result<()> {
        let (_d, vault) = fixture()?;
        let result = call(&vault, "backlinks", &json!({ "path": "b.md" }))?
            .expect("backlinks is a vault tool");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "a.md");
        Ok(())
    }

    /// A name that is not one of these is not an error here: it is the caller's
    /// chance to try the live tools before giving up.
    #[test]
    fn a_name_that_is_not_a_vault_tool_is_left_for_the_caller() -> Result<()> {
        let (_d, vault) = fixture()?;
        assert!(call(&vault, "live_note", &json!({}))?.is_none());
        Ok(())
    }

    #[test]
    fn path_traversal_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let result = call(&vault, "read_note", &json!({ "path": "../secret" }));
        assert!(result.is_err());
    }
}
