//! Running a saved query's SQL against the index (roadmap P4.3).
//!
//! This is the one place the app hands SQL to SQLite on the user's behalf, so it owns
//! both halves of the safety story:
//!
//! - **The connection is read-only.** [`Db::read`] sets `query_only`, so a body that
//!   tries to write fails inside SQLite. That is the enforcement the plan asks for —
//!   the connection refusing, not the text being sanitized first. A query note can
//!   break a report; it cannot break a vault.
//! - **The body is one question.** It must start with `SELECT` or `WITH`, and it may
//!   not hold a second statement. Not because a second statement could do damage — it
//!   could not, the connection would refuse it — but because a note that silently runs
//!   two statements answers a different question than the one the reader wrote, and
//!   SQLite's own message for the case names neither.
//!
//! ## No cache yet, on purpose
//!
//! The plan wrote "results cached by query-hash + vault mtime" before the index existed
//! and before any query had been measured. A `GROUP BY` over `props` in a 5k-note vault
//! is a single indexed scan, and a cache is a second answer to keep in step — with the
//! anchor path in its key once relative queries land, or a query cached for one session
//! answers for the next. Measure a slow query, then cache that query; the key rule is
//! written here so the cache knows what it owes when it arrives.

use super::Db;
use anyhow::{anyhow, Context, Result};
use rusqlite::types::ValueRef;
use rusqlite::Row;

/// One cell of a result.
///
/// SQLite has four storage classes and the surface has to tell them apart: a NULL is
/// not an empty string, and a number is not the text that spells it — a column of
/// counts should not sort as text because it was read as one.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Text(String),
    Integer(i64),
    Real(f64),
    Null,
}

impl Cell {
    pub fn is_null(&self) -> bool {
        matches!(self, Cell::Null)
    }

    /// How the cell reads on one line, spelled so the reader can tell the three
    /// kinds of nothing apart. A NULL is `∅` and an empty string is `''`, because a
    /// query's own `NULL` is a hole in the data while `''` is a value the data
    /// actually holds — and a table that draws both as a gap hides the difference
    /// the query went to the trouble of making.
    pub fn display(&self) -> String {
        match self {
            Cell::Text(text) if text.is_empty() => "''".to_string(),
            Cell::Text(text) => text.clone(),
            Cell::Integer(number) => number.to_string(),
            Cell::Real(number) => number.to_string(),
            Cell::Null => "∅".to_string(),
        }
    }
}

/// A result set: the column names as SQLite reports them, and the rows.
///
/// Every row carries one cell per column, so a surface laying out a table never has to
/// wonder whether a short row means a missing value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

impl Rows {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The column's index by name, for a surface that wants one named column (a lens
    /// reads the note-path column, a ledger reads the amount).
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    /// Order the answer by a column named in a query note's declaration: `cr`, or
    /// `-cr` for descending. An unknown column leaves the order alone rather than
    /// guessing, because a declaration that names a column the query does not return
    /// is the author's mistake to notice, not a reason to shuffle the answer.
    ///
    /// This is the reader's override, applied to what came back rather than spliced
    /// into the statement: a query that sorts itself and a note that sorts it are one
    /// sort, and the picker and the in-file table both call this, so they cannot
    /// disagree about the order of the same answer.
    pub fn sort_by(&mut self, spec: &str) {
        let (name, descending) = match spec.strip_prefix('-') {
            Some(rest) => (rest, true),
            None => (spec, false),
        };
        let Some(column) = self.column(name) else {
            return;
        };
        self.rows.sort_by(|a, b| {
            let order = compare_cells(a.get(column), b.get(column));
            if descending {
                order.reverse()
            } else {
                order
            }
        });
    }
}

/// Order two cells. Numbers order as numbers, so a column of counts does not put 10
/// before 9; anything else orders as the text the reader can see.
fn compare_cells(a: Option<&Cell>, b: Option<&Cell>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let number = |cell: &Cell| match cell {
        Cell::Integer(n) => Some(*n as f64),
        Cell::Real(n) => Some(*n),
        _ => None,
    };
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, _) => Ordering::Less,
        (_, None) => Ordering::Greater,
        (Some(x), Some(y)) => match (number(x), number(y)) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            _ => x.display().cmp(&y.display()),
        },
    }
}

/// How many answer rows a surface draws. Both doors — the picker and the in-file
/// table — cut at this number and say how many it left out, so one answer cannot be
/// two different lengths on two different screens. The runner itself is uncapped: what
/// the query answers is what the query answers.
pub const MAX_ROWS: usize = 200;

/// The one parameter the app ever binds: the note the reader is standing in.
pub const CURRENT: &str = ":current";

/// A named binding, spelled as the body spells it (`:current`).
///
/// Text only for now: the one binding the app makes is the anchor path of a relative
/// query, and a body that wants a number can write it. Adding a typed binding is a
/// wider door than it looks — a value bound from a note's frontmatter is a value the
/// index already decided the kind of, and this is not the place to re-decide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

impl<'a> Binding<'a> {
    pub fn new(name: &'a str, text: &'a str) -> Self {
        Self { name, text }
    }
}

impl Db {
    /// Run one query against the index on a read-only connection.
    pub fn run(&self, sql: &str, bindings: &[Binding<'_>]) -> Result<Rows> {
        check(sql)?;
        let conn = self.read()?;
        let mut stmt = conn
            .prepare(sql)
            .with_context(|| format!("the query could not be prepared: {sql}"))?;
        let columns: Vec<String> = stmt.column_names().map(str::to_string).collect();
        let width = columns.len();
        // The body's own list of parameters decides what gets bound. rusqlite refuses
        // a name the statement does not hold, and the surface offers an anchor whether
        // or not this query asked for one — so an unasked-for binding is dropped rather
        // than reported. The other direction is the one worth a message: a body that
        // asks for `:curent` is a typo, and SQLite's answer for it names neither the
        // intent nor the anchor that was sitting right there.
        let wanted = parameters(sql);
        if let Some(missing) = wanted
            .iter()
            .find(|name| !bindings.iter().any(|bound| bound.name == **name))
        {
            return Err(anyhow!(
                "the query asks for `{missing}`, and nothing supplies it"
            ));
        }

        let named: Vec<(&str, &str)> = bindings
            .iter()
            .filter(|bound| wanted.iter().any(|name| name == bound.name))
            .map(|b| (b.name, b.text))
            .collect();
        let mut out = Rows {
            columns,
            rows: Vec::new(),
        };
        let mapped = stmt
            .query_map(named.as_slice(), |row| read_row(row, width))
            .with_context(|| format!("the query failed to run: {sql}"))?;
        for row in mapped {
            out.rows
                .push(row.context("a result row could not be read")?);
        }
        Ok(out)
    }
}

/// One row's cells, in column order. The width comes from the statement's columns, so
/// a row is always as wide as the header even if SQLite hands back something odd.
fn read_row(row: &Row<'_>, width: usize) -> rusqlite::Result<Vec<Cell>> {
    let mut out = Vec::with_capacity(width);
    for index in 0..width {
        out.push(match row.get_ref(index)? {
            ValueRef::Null => Cell::Null,
            ValueRef::Integer(number) => Cell::Integer(number),
            ValueRef::Real(number) => Cell::Real(number),
            ValueRef::Text(text) => Cell::Text(String::from_utf8_lossy(text).into_owned()),
            // The index holds no blobs, but `CAST(x AS BLOB)` can produce one. Say what
            // it is rather than pass it off as text.
            ValueRef::Blob(bytes) => Cell::Text(format!("<{} bytes>", bytes.len())),
        });
    }
    Ok(out)
}

/// Refuse a body that is not one question, before SQLite gets the chance to.
///
/// This is the one place the shape rule lives: the runner calls it before it
/// prepares, and the editor asks it (`is_runnable`) whether a query note is worth
/// folding at all. Two answers to that question would mean a file the view folds
/// shut and the door refuses to run — a refusal with nothing to read.
pub fn check(sql: &str) -> Result<()> {
    let Some(word) = first_word(sql) else {
        return Err(anyhow!("a query note's body is empty"));
    };
    if !matches!(word.as_str(), "select" | "with") {
        return Err(anyhow!(
            "a query note asks a question: this body starts with `{word}`, \
             not `SELECT` or `WITH`"
        ));
    }
    let mut extra: Option<&str> = None;
    scan(sql, &mut |event| {
        if let Event::StatementBreak(after) = event {
            extra.get_or_insert(after);
        }
    });
    if let Some(after) = extra {
        return Err(anyhow!(
            "a query note holds one statement; this one continues after a `;` \
             with `{}`",
            clipped(after)
        ));
    }
    Ok(())
}

/// The first word of the body, lowercased, skipping whitespace and comments.
fn first_word(sql: &str) -> Option<String> {
    let bytes = sql.as_bytes();
    let mut at = 0;
    loop {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if bytes.get(at..at + 2) == Some(b"--") {
            at += 2;
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if bytes.get(at..at + 2) == Some(b"/*") {
            at += 2;
            while at < bytes.len() && bytes.get(at..at + 2) != Some(b"*/") {
                at += 1;
            }
            at = (at + 2).min(bytes.len());
            continue;
        }
        break;
    }
    let start = at;
    while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
        at += 1;
    }
    (at > start).then(|| sql[start..at].to_ascii_lowercase())
}

/// What matters inside a statement, found by one walk.
#[derive(Debug)]
enum Event<'a> {
    /// A `;` with more text after it — a second statement.
    StatementBreak(&'a str),
    /// A `:name` parameter, spelled as the body spells it.
    Parameter(&'a str),
}

/// Walk a statement and report what matters, skipping quoted text and comments.
///
/// One walker rather than two, because a `;` and a `:name` are both ordinary inside a
/// string (`'a; b'`), inside a quoted identifier and inside a comment — and two walkers
/// that disagree about where a quote ends would leave one rule a hole the other does not
/// have.
fn scan<'a>(sql: &'a str, found: &mut impl FnMut(Event<'a>)) {
    let bytes = sql.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() {
                    if bytes[at] == quote {
                        // Doubled is an escape, not the end.
                        if bytes.get(at + 1) == Some(&quote) {
                            at += 2;
                            continue;
                        }
                        break;
                    }
                    at += 1;
                }
                at += 1;
            }
            b'[' => {
                at += 1;
                while at < bytes.len() && bytes[at] != b']' {
                    at += 1;
                }
                at += 1;
            }
            b'-' if bytes.get(at + 1) == Some(&b'-') => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at += 2;
                while at < bytes.len() && bytes.get(at..at + 2) != Some(b"*/") {
                    at += 1;
                }
                at = (at + 2).min(bytes.len());
            }
            b';' => {
                let rest = sql[at + 1..].trim();
                if !rest.is_empty() {
                    // Named up to the next statement break, so the message quotes the
                    // reader's own text rather than the whole file.
                    let end = rest.find(';').unwrap_or(rest.len());
                    found(Event::StatementBreak(rest[..end].trim()));
                }
                at += 1;
            }
            b':' | b'@' | b'$'
                if bytes
                    .get(at + 1)
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') =>
            {
                let start = at;
                at += 1;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                found(Event::Parameter(&sql[start..at]));
            }
            _ => at += 1,
        }
    }
}

/// The `:name` parameters the body asks for, in the order they appear, deduplicated.
///
/// Public because the *declaration* has to be checked against what the body actually
/// asks for (roadmap P4.3 M5): a query labelled relative in the picker while its body
/// never asks for the anchor, or a body that leans on the anchor while the label says
/// nothing, are the same hidden state everywhere else in this app refuses.
pub fn parameters(sql: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    scan(sql, &mut |event| {
        if let Event::Parameter(name) = event {
            if !out.iter().any(|seen| seen == name) {
                out.push(name.to_string());
            }
        }
    });
    out
}

/// One line, whatever the database sent.
///
/// A syntax error arrives quoting the statement — newlines and all. Every surface
/// that shows an answer shows one line per row, so the message is flattened where it
/// reaches the surface rather than at its source: the MCP tool still reports the whole
/// thing, and a picker row or a table row never has to wonder whether its text fits.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Shorten a quote of the reader's own text to something a status line can carry,
/// cutting on a character boundary — the text is theirs and may hold `—` or `[[…]]`.
fn clipped(text: &str) -> String {
    if text.len() <= 40 {
        return text.to_string();
    }
    let mut at = 40;
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    format!("{}…", &text[..at])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;

    /// A vault with three notes and the properties a monster vault would have.
    fn vault() -> Result<(tempfile::TempDir, Vault, Db)> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        vault.write("Goblin.md", "---\ncr: 5\ntype: monster\n---\n# Goblin\n")?;
        vault.write("Wisp.md", "---\ncr: 2\ntype: monster\n---\n# Wisp\n")?;
        vault.write("Session 15.md", "---\ntype: session\n---\n# Session 15\n")?;
        let db = Db::open(dir.path())?;
        db.reconcile(&vault)?;
        Ok((dir, vault, db))
    }

    /// One column, as text, in the order the query returned it.
    fn column(db: &Db, sql: &str) -> Result<Vec<String>> {
        Ok(db
            .run(sql, &[])?
            .rows
            .into_iter()
            .map(|row| row[0].display())
            .collect())
    }

    #[test]
    fn a_query_answers_from_the_index() -> Result<()> {
        let (_d, _v, db) = vault()?;
        assert_eq!(
            column(&db, "SELECT path FROM notes ORDER BY path")?,
            ["Goblin.md", "Session 15.md", "Wisp.md"],
            "the order the query asked for is the order the rows arrive in"
        );
        assert_eq!(
            column(
                &db,
                "SELECT value FROM props WHERE key = 'cr' ORDER BY CAST(value AS REAL)"
            )?
            .len(),
            2
        );
        Ok(())
    }

    #[test]
    fn a_result_keeps_its_columns_and_its_kinds() -> Result<()> {
        let (_d, _v, db) = vault()?;
        let rows = db.run(
            "SELECT note, COUNT(*) AS n FROM props WHERE key = 'type' GROUP BY note",
            &[],
        )?;
        assert_eq!(rows.columns, ["note", "n"]);
        assert_eq!(rows.len(), 3);
        let counts: Vec<&Cell> = rows.rows.iter().map(|row| &row[1]).collect();
        assert!(
            counts.iter().all(|cell| matches!(cell, Cell::Integer(1))),
            "a count comes back as a number, not as the text of a number: {counts:?}"
        );
        Ok(())
    }

    #[test]
    fn a_null_is_a_null_and_not_an_empty_string() -> Result<()> {
        let (_d, _v, db) = vault()?;
        let rows = db.run("SELECT NULL AS missing, '' AS empty", &[])?;
        assert!(rows.rows[0][0].is_null());
        assert_eq!(rows.rows[0][1], Cell::Text(String::new()));
        Ok(())
    }

    #[test]
    fn a_relative_query_gets_its_anchor_bound() -> Result<()> {
        let (_d, _v, db) = vault()?;
        let rows = db.run(
            "SELECT path FROM notes WHERE path <> :current ORDER BY path",
            &[Binding::new(":current", "Wisp.md")],
        )?;
        assert_eq!(
            rows.rows
                .iter()
                .map(|row| row[0].display())
                .collect::<Vec<_>>(),
            ["Goblin.md", "Session 15.md"],
            "the anchor is a value the body compares against, not text spliced into it"
        );
        Ok(())
    }

    #[test]
    fn a_body_that_is_not_a_question_is_refused_before_it_runs() -> Result<()> {
        let (_d, _v, db) = vault()?;
        for body in [
            "UPDATE notes SET note = 'x'",
            "DELETE FROM notes",
            "INSERT INTO tags (note, tag) VALUES ('a.md', 'x')",
            "PRAGMA table_info(notes)",
            "DROP TABLE props",
            "  -- a comment first\nUPDATE notes SET note = 'x'",
            "",
            "   \n",
        ] {
            let err = db.run(body, &[]).unwrap_err().to_string();
            assert!(
                err.contains("asks a question") || err.contains("body is empty"),
                "{body} was accepted: {err}"
            );
        }
        // A `WITH` is a question too, and the shape check must not refuse it.
        assert!(db.run("WITH x AS (SELECT 1) SELECT * FROM x", &[]).is_ok());
        Ok(())
    }

    #[test]
    fn a_second_statement_in_the_body_is_named_rather_than_run() -> Result<()> {
        let (_d, _v, db) = vault()?;
        let err = db
            .run("SELECT path FROM notes; SELECT 1", &[])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("one statement") && err.contains("SELECT 1"),
            "{err}"
        );
        // A trailing semicolon is punctuation, not a second statement.
        assert!(db.run("SELECT path FROM notes;\n", &[]).is_ok());
        // A semicolon inside text, a quoted identifier or a comment is not a break.
        for body in [
            "SELECT 'a; b' AS x",
            "SELECT \"path;\" FROM notes LIMIT 0",
            "SELECT path FROM notes -- ends here; not a break\n",
            "SELECT path FROM notes /* ; not a break */ LIMIT 1",
        ] {
            assert!(db.run(body, &[]).is_ok(), "{body} was refused");
        }
        Ok(())
    }

    /// A binding the body never asks for is not an error: the surface offers an anchor
    /// whether or not this particular query wants one, and rusqlite would refuse the
    /// surplus. The other direction is the one that earns a message — a body asking for
    /// `:curent` is a typo, and SQLite's answer for an unbound parameter names neither
    /// the intent nor the anchor that was sitting right there.
    #[test]
    fn an_unasked_for_binding_is_dropped_and_an_unsupplied_one_is_named() -> Result<()> {
        let (_d, _v, db) = vault()?;
        let anchor = [Binding::new(":current", "Wisp.md")];
        let rows = db.run("SELECT path FROM notes ORDER BY path", &anchor)?;
        assert!(
            !rows.rows.is_empty(),
            "an anchor the body never asked for broke the query"
        );
        let err = db
            .run("SELECT path FROM notes WHERE path = :curent", &anchor)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains(":curent") && err.contains("nothing supplies"),
            "{err}"
        );
        // A `:name` inside a string or a comment is text, not a request for a binding.
        assert!(
            db.run("SELECT ':current' AS literal -- :current too", &[])
                .is_ok(),
            "a parameter shape inside text was read as a parameter"
        );
        Ok(())
    }

    /// The connection is the enforcement, not the shape check: a body that gets past
    /// the check still cannot write.
    #[test]
    fn the_read_connection_refuses_to_write_even_past_the_shape_check() -> Result<()> {
        let (_d, _v, db) = vault()?;
        let conn = db.read()?;
        let err = conn
            .execute("INSERT INTO tags (note, tag) VALUES ('a.md', 'nope')", [])
            .unwrap_err();
        assert!(
            err.to_string().contains("readonly database"),
            "the read connection is not a courtesy: {err}"
        );
        Ok(())
    }
}
