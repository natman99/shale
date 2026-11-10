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
