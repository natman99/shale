//! `shale-mcp`: a Model Context Protocol server exposing vault reading,
//! writing, and searching over stdio JSON-RPC 2.0.
//!
//! Protocol reference: https://modelcontextprotocol.io
//! This implements the subset needed for tool use: `initialize`,
//! `notifications/initialized`, `ping`, `tools/list`, and `tools/call`.

use std::io::{BufRead, Write};

use anyhow::{Context as _, Result};
use clap::Parser;
use serde_json::{json, Value};
use shale_core::search::SearchOptions;
use shale_core::Vault;

/// MCP protocol version implemented by this server.
const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Parser, Debug)]
#[command(name = "shale-mcp", about = "MCP server for a Shale vault")]
struct Args {
    /// Path to the vault directory.
    #[arg(long)]
    vault: std::path::PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let vault = Vault::open(&args.vault).context("opening vault")?;

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

        let response = match dispatch(&vault, &method, &params) {
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

fn dispatch(vault: &Vault, method: &str, params: &Value) -> Result<Value> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "shale-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .context("tools/call missing name")?;
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            call_tool(vault, name, &arguments)
        }
        other => Ok(json!({ "error": format!("unknown method: {other}") })),
    }
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "list_notes",
            "description": "List all markdown notes in the vault, by vault-relative path.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "read_note",
            "description": "Read a markdown note by vault-relative path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Vault-relative path, e.g. notes/idea.md" }
                },
                "required": ["path"]
            }
        },
        {
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
        },
        {
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
        },
        {
            "name": "backlinks",
            "description": "List notes that contain wiki-links to the given note.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Vault-relative path of the note" }
                },
                "required": ["path"]
            }
        }
    ])
}

fn call_tool(vault: &Vault, name: &str, arguments: &Value) -> Result<Value> {
    let text_result = |text: String| json!({ "content": [{ "type": "text", "text": text }] });

    match name {
        "list_notes" => {
            let entries = vault.list_markdown()?;
            let lines: Vec<String> = entries
                .iter()
                .map(|e| e.path.display().to_string())
                .collect();
            Ok(text_result(lines.join("\n")))
        }
        "read_note" => {
            let path = arg_str(arguments, "path")?;
            Ok(text_result(vault.read(path)?))
        }
        "write_note" => {
            let path = arg_str(arguments, "path")?;
            let content = arg_str(arguments, "content")?;
            vault.write(path, content)?;
            Ok(text_result(format!("wrote {}", path)))
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
            Ok(text_result(rendered.join("\n")))
        }
        "backlinks" => {
            let path = arg_str(arguments, "path")?;
            let entries = vault.list_markdown()?;
            let links = vault.backlinks(path, &entries)?;
            let lines: Vec<String> = links.iter().map(|e| e.path.display().to_string()).collect();
            Ok(text_result(lines.join("\n")))
        }
        other => anyhow::bail!("unknown tool: {other}"),
    }
}

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
    fn initialize_handshake() -> Result<()> {
        let (_d, vault) = fixture()?;
        let result = dispatch(&vault, "initialize", &json!({}))?;
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], "shale-mcp");
        Ok(())
    }

    #[test]
    fn tools_list_has_all_tools() -> Result<()> {
        let (_d, vault) = fixture()?;
        let result = dispatch(&vault, "tools/list", &json!({}))?;
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 5);
        Ok(())
    }

    #[test]
    fn read_and_write_tools() -> Result<()> {
        let (_d, vault) = fixture()?;
        let result = call_tool(&vault, "read_note", &json!({ "path": "a.md" }))?;
        assert_eq!(result["content"][0]["text"], "Hello [[b]]");

        call_tool(
            &vault,
            "write_note",
            &json!({ "path": "c.md", "content": "new" }),
        )?;
        assert_eq!(vault.read("c.md")?, "new");
        Ok(())
    }

    #[test]
    fn search_tool() -> Result<()> {
        let (_d, vault) = fixture()?;
        let result = call_tool(&vault, "search_notes", &json!({ "query": "world" }))?;
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("b.md:1: World"));
        Ok(())
    }

    #[test]
    fn backlinks_tool() -> Result<()> {
        let (_d, vault) = fixture()?;
        let result = call_tool(&vault, "backlinks", &json!({ "path": "b.md" }))?;
        let text = result["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "a.md");
        Ok(())
    }

    #[test]
    fn path_traversal_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let result = call_tool(&vault, "read_note", &json!({ "path": "../secret" }));
        assert!(result.is_err());
    }
}
