//! The `.shq` query note: a saved question, kept as a plain text file (roadmap P4.3).
//!
//! A query note is not a note. It is a file in the vault whose body is one SQL
//! statement against the index, with a small declaration above it saying what to call
//! the result and how to show it. Everything about the format follows from the two
//! rulings it inherits: the body is **SQL** because the GM-toolkit rulings ask for
//! aggregates and joins a shorthand could never reach ("the party ledger is a saved
//! `.shq` over those tables"), and the *vocabulary* is shared with vault search because
//! P4.14 ruled one grammar — `tag:`, `prop:`, `path:` mean the same columns there and
//! here, so a search can be saved and a saved query can be read by a reader who already
//! knows the search.
//!
//! ## The fence is `+++`, not `---`
//!
//! A note's properties block is YAML between `---` fences and [`crate::frontmatter`]
//! owns that shape. A query note's declaration is TOML, and its body is SQL — where
//! `--` opens a line comment, so a `---` fence sitting above a SQL body would be read
//! as a comment by the very language below it. `+++` is the TOML fence other tools
//! already use, and it means the reader never guesses which language a file is in.
//!
//! ## The declaration is optional
//!
//! A file that opens with `+++` carries a declaration; a file that does not is a query
//! with the defaults and its whole text as the body. Defaults are the common case — a
//! saved search wants no declaration — so the format does not force one.
//!
//! ## Unknown declaration keys are kept, not refused
//!
//! A key this version does not know (`columns = [...]` written by a newer Shale) is not
//! a typo, and refusing a file the index can still answer would turn a forward
//! compatibility question into a broken vault. A key that is *misspelled* is caught
//! where it is visible — the surface shows the declaration it read — rather than by
//! failing the run.

use anyhow::{anyhow, Result};
use std::path::Path;

/// The fence a query note's declaration opens and closes with.
pub const FENCE: &str = "+++";

/// The extension that makes a file a query note.
pub const EXTENSION: &str = "shq";

/// What a query note declares about itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Decl {
    /// The name the surface shows. Defaults to the file's stem.
    pub name: Option<String>,
    /// A column to order by; a leading `-` means descending. The surface applies it,
    /// because a query that sorts itself and a note that sorts it are one sort.
    pub sort: Option<String>,
    /// How to show the rows. Unknown spellings are kept rather than refused, so a
    /// newer view name does not break an older reader.
    pub view: Option<String>,
    /// `relative = "current"`: the query reads in relation to the note the reader is
    /// standing in, and `:current` is bound at run time. The anchor path belongs in the
    /// cache key when results are cached — a query named `at the table` cached for one
    /// session must not answer for the next.
    pub relative: bool,
}

/// One parsed query note: what it declares, and the SQL it asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryNote {
    pub decl: Decl,
    /// The body, exactly as written. Trimming and the one-statement rule belong to the
    /// runner, so what the reader sees in the file and what runs are the same text.
    pub sql: String,
}

impl QueryNote {
    /// Read a `.shq` file's text.
    ///
    /// The declaration is TOML; a body is anything after it. A declaration that will
    /// not parse is an error naming the file's own text, because the alternative —
    /// running the query and quietly ignoring the half that sets its name and sort —
    /// answers a question the reader did not ask.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let Some(rest) = text.strip_prefix(FENCE) else {
            return Ok(Self {
                decl: Decl::default(),
                sql: text.to_string(),
            });
        };
        // The opening fence's own line ends the prefix; the block runs to the next
        // fence line.
        let after_open = rest.strip_prefix('\n').unwrap_or(rest);
        let Some(close) = find_fence(after_open) else {
            return Err(anyhow!(
                "a query note's declaration opens with `{FENCE}` and never closes it"
            ));
        };
        let (toml, body) = after_open.split_at(close);
        let body = body[FENCE.len()..]
            .strip_prefix('\n')
            .unwrap_or(&body[FENCE.len()..]);
        let decl = read_decl(toml)?;
        Ok(Self {
            decl,
            sql: body.to_string(),
        })
    }

    /// The name to show: the declared one, or the file's stem.
    pub fn display_name<'a>(&'a self, stem: &'a str) -> &'a str {
        self.decl.name.as_deref().unwrap_or(stem)
    }

    /// What this query needs from the surface that is running it (roadmap P4.3 M5).
    ///
    /// `host` is the note the reader is standing in: the active note for `:query`, the
    /// note holding the embed for a `![[query]]` block. A query note is not a note, so a
    /// relative query *viewed in its own file* has no host — the same nothing a graph tab
    /// has — and says so rather than binding its own path and answering a question about
    /// a file the `links` table has no row for.
    ///
    /// The declaration and the body have to agree. The picker labels a query "relative"
    /// from the declaration, so a body that answers differently depending on where the
    /// reader stands while the label says otherwise is hidden state, and a declaration
    /// that claims relativity the body never uses costs nothing and is left alone.
    pub fn anchor(&self, host: Option<&Path>) -> Anchor {
        let asked = crate::db::parameters(&self.sql)
            .iter()
            .any(|name| name == crate::db::CURRENT);
        if !asked {
            return Anchor::NotAsked;
        }
        if !self.decl.relative {
            return Anchor::Refused(format!(
                "the query asks for `{}` but does not declare `relative = \"current\"`",
                crate::db::CURRENT
            ));
        }
        match host.filter(|path| path.extension().is_none_or(|ext| ext != EXTENSION)) {
            Some(note) => Anchor::Bound(note.to_string_lossy().into_owned()),
            None => Anchor::Refused(
                "a relative query needs a note to be relative to, and this surface has none"
                    .to_string(),
            ),
        }
    }
}

/// What a query needs from the surface running it, decided in one place so every
/// surface — the picker, a note's embedded table, the query's own view, and later the
/// MCP verb — asks the same question and refuses the same way (EDGE-12, EDGE-20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// Bind `:current` to this vault-relative path.
    Bound(String),
    /// The body never asked for an anchor; the surface owes it nothing.
    NotAsked,
    /// The query needs a note and this surface cannot name one, or the body and the
    /// declaration disagree about whether one is needed. The string is what the reader
    /// reads — no surface invents its own wording for this.
    Refused(String),
}

/// The byte offset of the closing fence's line start, if there is one.
fn find_fence(text: &str) -> Option<usize> {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == FENCE {
            return Some(at);
        }
        at += line.len();
    }
    None
}

/// The declaration's TOML, read into the keys this version knows.
///
/// Values are read as strings wherever a string is what they are (`name`, `sort`,
/// `view`), so a number or a table in one of those slots is a clear error rather than
/// a silent `to_string()` of something the reader did not write.
fn read_decl(toml: &str) -> Result<Decl> {
    let table: std::collections::BTreeMap<String, toml::Value> =
        toml::from_str(toml).map_err(|err| anyhow!("the query note's declaration: {err}"))?;
    let mut decl = Decl::default();
    for (key, value) in table {
        match key.as_str() {
            "name" => decl.name = Some(text_of(&key, &value)?),
            "sort" => decl.sort = Some(text_of(&key, &value)?),
            "view" => decl.view = Some(text_of(&key, &value)?),
            "relative" => {
                let word = text_of(&key, &value)?;
                // The ruling spells one anchor: the note you are standing in. A second
                // spelling (`folder`, `vault`) is a different feature, and accepting the
                // word now would promise an answer it does not give.
                if word != "current" {
                    return Err(anyhow!(
                        "`relative = \"{word}\"` is not a thing a query can be relative to; \
                         the one anchor is `current`"
                    ));
                }
                decl.relative = true;
            }
            // Unknown: kept out of the struct, not out of the file. See the module doc.
            _ => continue,
        }
    }
    Ok(decl)
}

fn text_of(key: &str, value: &toml::Value) -> Result<String> {
    match value {
        toml::Value::String(s) => Ok(s.clone()),
        other => Err(anyhow!(
            "`{key}` takes a string, and this is {} in the declaration",
            kind_word(other)
        )),
    }
}

/// What a declaration value is, in the words a reader would use for it.
fn kind_word(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "a string",
        toml::Value::Integer(_) | toml::Value::Float(_) => "a number",
        toml::Value::Boolean(_) => "a yes/no",
        toml::Value::Array(_) => "a list",
        toml::Value::Table(_) => "a table",
        toml::Value::Datetime(_) => "a date",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A relative query: the host note's own outgoing links.
    const RELATIVE: &str = "+++\nname = \"Where I point\"\nrelative = \"current\"\n+++\nSELECT target FROM links WHERE source = :current ORDER BY target\n";

    /// The anchor's journey (M5): a relative query run from a note gets that note.
    #[test]
    fn a_relative_query_asks_for_the_note_the_reader_stands_in() {
        let note = QueryNote::parse(RELATIVE).unwrap();
        assert_eq!(
            note.anchor(Some(Path::new("Lair.md"))),
            Anchor::Bound("Lair.md".into()),
            "the host note is the anchor"
        );
    }

    /// EDGE-12: a surface with no note under the reader refuses out loud, in words that
    /// name the missing thing rather than SQLite's missing parameter.
    #[test]
    fn a_relative_query_with_no_host_says_what_it_is_missing() {
        let note = QueryNote::parse(RELATIVE).unwrap();
        let Anchor::Refused(why) = note.anchor(None) else {
            panic!("a host-less surface must refuse");
        };
        assert!(
            why.contains("needs a note to be relative to"),
            "the refusal names the missing note: {why}"
        );
    }

    /// A query note is not a note, so viewing a relative query in its own file is the
    /// same host-less surface as a graph tab. Binding `:current` to the `.shq` would
    /// answer a question about a file the `links` table has no row for.
    #[test]
    fn a_relative_query_viewed_in_its_own_file_has_no_host() {
        let note = QueryNote::parse(RELATIVE).unwrap();
        assert!(
            matches!(
                note.anchor(Some(Path::new("queries/where.shq"))),
                Anchor::Refused(_)
            ),
            "the query's own path is not a note to be relative to"
        );
    }

    /// The declaration and the body have to agree. The picker labels a query "relative"
    /// from the declaration, so a body that answers differently depending on where the
    /// reader stands while the label says nothing about it is hidden state.
    #[test]
    fn a_body_that_uses_the_anchor_without_declaring_it_is_refused_by_name() {
        let note = QueryNote::parse("SELECT target FROM links WHERE source = :current\n").unwrap();
        let Anchor::Refused(why) = note.anchor(Some(Path::new("Lair.md"))) else {
            panic!("an undeclared anchor is a mismatch, not a silent binding");
        };
        assert!(
            why.contains("relative = \"current\""),
            "the refusal names the declaration that is missing: {why}"
        );
    }

    /// The other direction costs nothing: a declaration that claims relativity a body
    /// never uses runs, the same way an unknown `view` is kept rather than refused.
    #[test]
    fn a_declaration_the_body_never_needs_is_left_alone() {
        let note =
            QueryNote::parse("+++\nrelative = \"current\"\n+++\nSELECT note FROM notes\n").unwrap();
        assert_eq!(note.anchor(None), Anchor::NotAsked);
    }

    /// The question is asked with the runner's own walker, so an anchor spelled inside a
    /// string literal or a comment is not a parameter and asks for nothing.
    #[test]
    fn an_anchor_spelled_in_a_string_is_not_asked_for() {
        let note = QueryNote::parse("SELECT ':current' AS literal -- :current too\n").unwrap();
        assert_eq!(note.anchor(None), Anchor::NotAsked);
    }

    #[test]
    fn a_query_note_without_a_declaration_is_all_body() {
        let note = QueryNote::parse("SELECT note FROM notes ORDER BY note\n").unwrap();
        assert_eq!(note.decl, Decl::default());
        assert_eq!(note.sql, "SELECT note FROM notes ORDER BY note\n");
        assert_eq!(note.display_name("recent"), "recent");
    }

    #[test]
    fn a_declaration_is_read_and_the_body_starts_after_it() {
        let note = QueryNote::parse(concat!(
            "+++\n",
            "name = \"The party\"\n",
            "sort = \"-cr\"\n",
            "view = \"table\"\n",
            "relative = \"current\"\n",
            "+++\n",
            "SELECT note, value FROM props WHERE key = 'cr'\n",
        ))
        .unwrap();
        assert_eq!(
            note.decl,
            Decl {
                name: Some("The party".into()),
                sort: Some("-cr".into()),
                view: Some("table".into()),
                relative: true,
            }
        );
        assert_eq!(note.sql, "SELECT note, value FROM props WHERE key = 'cr'\n");
        assert_eq!(note.display_name("party"), "The party");
    }

    #[test]
    fn a_sql_comment_line_is_not_a_fence_and_a_fence_line_is_not_a_comment() {
        // The reason the fence is `+++`: `--` is SQL's comment, so a body may open a
        // line with `-- anything`, and a `---` fence would be ambiguous with it.
        let note = QueryNote::parse(concat!(
            "+++\n",
            "name = \"x\"\n",
            "+++\n",
            "-- a comment that looks like a fence: ---\n",
            "SELECT 1\n",
        ))
        .unwrap();
        assert!(note
            .sql
            .contains("-- a comment that looks like a fence: ---"));
    }

    #[test]
    fn an_unknown_declaration_key_is_kept_out_of_the_struct_not_out_of_the_file() {
        let note = QueryNote::parse(concat!(
            "+++\n",
            "columns = [\"note\", \"cr\"]\n",
            "name = \"x\"\n",
            "+++\n",
            "SELECT 1\n",
        ))
        .unwrap();
        assert_eq!(note.decl.name.as_deref(), Some("x"));
        assert_eq!(note.sql, "SELECT 1\n");
    }

    #[test]
    fn a_declaration_that_cannot_be_read_says_so() {
        for text in [
            "+++\nname = \n+++\nSELECT 1\n",
            "+++\nname = 5\n+++\nSELECT 1\n",
            "+++\nname = \"x\"\nSELECT 1\n",
            "+++\nrelative = \"folder\"\n+++\nSELECT 1\n",
        ] {
            let err = QueryNote::parse(text).unwrap_err().to_string();
            assert!(!err.is_empty(), "{text} was accepted");
        }
    }
}
