//! The saved-query door (roadmap P4.3 M7): an agent can see the vault's `.shq` files,
//! read one, and ask it — so "make me a dashboard of X" ends as a file the reader can
//! open, edit and re-run, not as a line of SQL in a chat transcript.
//!
//! Three rules hold the door honest:
//!
//! - **The same runner, on a connection that cannot write** (ruling 41). There is no
//!   second executor for agents: [`shale_core::db::Db::reader`] opens the index the vault
//!   already has — never one this door creates — and the statement goes through the same
//!   shape rule and the same binding rules the editor's table goes through.
//! - **The sentences are the editor's** (ruling 44). `query_note::ask` is the one chain,
//!   so "no note answers this", a body that is not one question, and a relative query with
//!   no host read the same here as in the picker.
//! - **No second write door** (ruling 43). A `.shq` is written with `write_note`, which
//!   already honours the profile's paths and the review queue (EDGE-18). What this module
//!   adds is visibility: `list_notes` walks markdown, so before M7 an agent could not see
//!   a saved query at all.

use std::path::Path;

use anyhow::{Context as _, Result};
use serde_json::{json, Value};
use shale_core::db::{Db, MAX_ROWS};
use shale_core::query_note::{self, Outcome};
use shale_core::vault::rel_text;

use crate::refusal;
use crate::Server;

/// The tools, in the order an agent sees them in `tools/list`.
pub(crate) fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "list_queries",
            "description": "List the vault's saved queries (.shq files), by vault-relative path. These are not notes: `list_notes` does not include them.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "read_query",
            "description": "Read a saved query: its `+++` declaration and its SQL body, exactly as the file holds them.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Vault-relative path, e.g. queries/monsters.shq" }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "run_query",
            "description": "Run a saved query against the vault index and return its rows as text. The statement runs on a read-only connection and is refused before SQLite if it is not exactly one SELECT. A query whose declaration says `relative = \"current\"` needs `note`: the note to be relative to. NULL cells render as \u{2205} and an empty string as ''.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Vault-relative path of the .shq" },
                    "note": { "type": "string", "description": "Vault-relative path of the note the query is relative to; required only for a query that declares `relative = \"current\"`" }
                },
                "required": ["path"]
            }
        }),
    ]
}

/// Run one of them. `Ok(None)` means the name is not a query tool, which lets the
/// caller try the other doors before calling the tool unknown.
pub(crate) fn call(server: &Server, name: &str, arguments: &Value) -> Result<Option<Value>> {
    let vault = &server.vault;
    let profile = &server.profile;
    let text_result = |text: String| json!({ "content": [{ "type": "text", "text": text }] });

    match name {
        "list_queries" => {
            let entries = profile.visible(&vault.list_queries()?);
            let lines: Vec<String> = entries.iter().map(|e| rel_text(&e.path)).collect();
            if lines.is_empty() {
                return Ok(Some(text_result(
                    "this vault holds no saved queries (.shq)".to_string(),
                )));
            }
            Ok(Some(text_result(lines.join("\n"))))
        }
        "read_query" => {
            let path = arg_str(arguments, "path")?;
            if !profile.can_read(path.as_ref()) {
                return Ok(Some(refusal(&format!(
                    "{} may not read {path}",
                    server.who()
                ))));
            }
            Ok(Some(text_result(vault.read(path)?)))
        }
        "run_query" => {
            let path = arg_str(arguments, "path")?;
            if !profile.can_read(path.as_ref()) {
                return Ok(Some(refusal(&format!(
                    "{} may not read {path}",
                    server.who()
                ))));
            }
            let text = vault.read(path)?;
            // EDGE-14, the door's own wording: this server never builds an index, so the
            // honest answer is that the vault has none to read — never zero rows.
            let Some(conn) = Db::reader(vault.root())? else {
                return Ok(Some(refusal(
                    "there is no vault index to read: this vault has no .shale/shale.db. \
                     Open it in Shale with indexing on, then ask again.",
                )));
            };
            // Ruling 42: an agent has no caret, so it names the host instead of the door
            // guessing one. A `note` that is not a note the profile can see is refused,
            // not bound — binding a path the index has no row for answers a question
            // about a file that does not exist.
            let host = match arguments.get("note").and_then(Value::as_str) {
                Some(note) => match host_note(server, note)? {
                    Some(resolved) => Some(resolved),
                    None => {
                        return Ok(Some(refusal(&format!(
                            "`{note}` is not a note this profile can see, so a relative \
                             query has nothing to be relative to"
                        ))));
                    }
                },
                None => None,
            };
            let stem = Path::new(path)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string());
            match query_note::ask(&text, &stem, host.as_deref().map(Path::new), &conn) {
                Outcome::Refused { why } => Ok(Some(refusal(&why))),
                Outcome::Empty { name } => Ok(Some(text_result(format!(
                    "{name} \u{b7} no note answers this"
                )))),
                Outcome::Answered { name, rows } => Ok(Some(text_result(render(&name, &rows)))),
            }
        }
        _ => Ok(None),
    }
}

/// The answer as text: the shape the marker row carries, then a header line, then one
/// line per row. The surface caps what it draws and says how many it left out (ruling
/// 16), at the same number the in-file table cuts at, so the two doors admit the same.
fn render(name: &str, rows: &shale_core::db::Rows) -> String {
    let total = rows.rows.len();
    let shown = total.min(MAX_ROWS);
    let mut out = format!("{name} \u{b7} {total} rows\n");
    out.push_str(&rows.columns.join(" | "));
    for row in rows.rows.iter().take(shown) {
        out.push('\n');
        out.push_str(
            &row.iter()
                .map(|cell| cell.display())
                .collect::<Vec<_>>()
                .join(" | "),
        );
    }
    if total > shown {
        out.push_str(&format!(
            "\n\u{2026} and {} more rows ({shown} shown)",
            total - shown
        ));
    }
    out
}

/// Resolve the `note` argument to a vault-relative note path this profile may see.
///
/// The path is asked through the same resolver a wiki-link goes through, so `Lair` and
/// `lair.md` and `Lair.md` all name the note they name in the editor — and a `.shq`, an
/// attachment or nothing at all names none.
fn host_note(server: &Server, note: &str) -> Result<Option<String>> {
    let entries = server.profile.visible(&server.vault.list_markdown()?);
    let resolver = shale_core::vault::LinkResolver::new(&entries);
    Ok(resolver
        .resolve_link(note)
        .map(|entry| rel_text(&entry.path)))
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

    /// A vault with one note, one monster note and one saved query that answers over the
    /// index, so the door has something real to list, read and ask.
    fn vault_with(query: &str, index: bool) -> Result<(tempfile::TempDir, Server)> {
        let dir = tempfile::tempdir()?;
        let vault = shale_core::Vault::open(dir.path())?;
        vault.write(
            "notes/Goblin.md",
            "---\ncr: 5\ntags: [goblinoid]\n---\nSmall and rude.\n",
        )?;
        vault.write("notes/Lair.md", "Guards the [[Goblin]].\n")?;
        vault.write("queries/monsters.shq", query)?;
        if index {
            // Opening applies the schema, which is all the door needs to exist; the
            // writer thread ends when the handle is dropped.
            let db = shale_core::db::Db::open(dir.path())?;
            drop(db);
        }
        Ok((
            dir,
            Server {
                vault,
                docs: None,
                profile: McpProfile::default(),
                profile_name: None,
            },
        ))
    }

    const PLAIN: &str = "+++\nname = \"Monsters\"\n+++\nSELECT path FROM notes ORDER BY path\n";
    const RELATIVE: &str = "+++\nname = \"Where I point\"\nrelative = \"current\"\n+++\nSELECT target FROM links WHERE source = :current ORDER BY target\n";

    fn text(result: &Value) -> String {
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn a_saved_query_is_visible_to_an_agent() -> Result<()> {
        let (_d, server) = vault_with(PLAIN, true)?;
        let listed = call(&server, "list_queries", &json!({}))?.expect("a query tool");
        assert_eq!(text(&listed), "queries/monsters.shq");

        // …and it is not in the notes list, which is why this verb exists.
        let notes =
            crate::vault_tools::call(&server, "list_notes", &json!({}))?.expect("a vault tool");
        assert!(
            !text(&notes).contains(".shq"),
            "list_notes walks markdown: {}",
            text(&notes)
        );

        let read = call(
            &server,
            "read_query",
            &json!({ "path": "queries/monsters.shq" }),
        )?
        .expect("a query tool");
        assert_eq!(text(&read), PLAIN);
        Ok(())
    }

    /// The door runs the same runner: an empty index answers zero rows, and the answer
    /// says so in the editor's words (EDGE-1).
    #[test]
    fn running_a_query_answers_with_the_editors_sentences() -> Result<()> {
        let (_d, server) = vault_with(PLAIN, true)?;
        let answer = call(
            &server,
            "run_query",
            &json!({ "path": "queries/monsters.shq" }),
        )?
        .expect("a query tool");
        assert_eq!(answer["isError"], Value::Null);
        assert_eq!(text(&answer), "Monsters \u{b7} no note answers this");
        Ok(())
    }

    /// EDGE-12 through this door: a relative query with no `note` refuses in the sentence
    /// ruling 38 wrote, and with one it answers.
    #[test]
    fn a_relative_query_names_its_host_or_is_refused() -> Result<()> {
        let (_d, server) = vault_with(RELATIVE, true)?;

        let refused = call(
            &server,
            "run_query",
            &json!({ "path": "queries/monsters.shq" }),
        )?
        .expect("a query tool");
        assert_eq!(refused["isError"], true);
        assert_eq!(
            text(&refused),
            "a relative query needs a note to be relative to, and this surface has none"
        );

        // A `note` that names nothing is refused too, rather than bound to a path the
        // index has no row for.
        let refused = call(
            &server,
            "run_query",
            &json!({ "path": "queries/monsters.shq", "note": "Nowhere.md" }),
        )?
        .expect("a query tool");
        assert_eq!(refused["isError"], true);
        assert!(
            text(&refused).contains("is not a note this profile can see"),
            "{}",
            text(&refused)
        );
        Ok(())
    }

    /// EDGE-14: a vault with no index gets the sentence, never an empty table.
    #[test]
    fn a_vault_with_no_index_says_so_instead_of_answering_zero() -> Result<()> {
        let (_d, server) = vault_with(PLAIN, false)?;
        let refused = call(
            &server,
            "run_query",
            &json!({ "path": "queries/monsters.shq" }),
        )?
        .expect("a query tool");
        assert_eq!(refused["isError"], true);
        assert!(
            text(&refused).starts_with("there is no vault index to read"),
            "{}",
            text(&refused)
        );
        Ok(())
    }

    /// EDGE-3/4 at the door: a body that is not one question is refused before SQLite,
    /// and the file is left alone — which is EDGE-18's posture for a write too.
    #[test]
    fn a_body_that_is_not_a_question_is_refused_at_the_door() -> Result<()> {
        let (_d, server) = vault_with("+++\n+++\nDELETE FROM notes;\n", true)?;
        let refused = call(
            &server,
            "run_query",
            &json!({ "path": "queries/monsters.shq" }),
        )?
        .expect("a query tool");
        assert_eq!(refused["isError"], true);
        assert!(
            text(&refused).starts_with("the query could not be answered: "),
            "{}",
            text(&refused)
        );
        Ok(())
    }

    /// Ruling 43: the write door is `write_note`, and it already reaches a `.shq` — with
    /// a review profile, that write is queued rather than applied (EDGE-18).
    #[test]
    fn a_query_is_written_through_the_note_door_and_review_governs_it() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let vault = shale_core::Vault::open(dir.path())?;
        let profile = McpProfile {
            review: true,
            ..McpProfile::default()
        };
        let server = Server {
            vault,
            docs: None,
            profile,
            profile_name: None,
        };

        let queued = crate::vault_tools::call(
            &server,
            "write_note",
            &json!({ "path": "queries/monsters.shq", "content": PLAIN }),
        )?
        .expect("a vault tool");
        assert!(
            text(&queued).starts_with("queued for review as "),
            "{queued}"
        );
        Ok(())
    }
}
