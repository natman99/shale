//! The door to a saved query (roadmap P4.3, M2): pick a `.shq`, run it, read the rows.
//!
//! The rows go to the picker, which is the ruling for M2: the picker already draws a
//! label, a dim detail and brought-forward bytes, and it already knows what to do with a
//! row that names a note. The results region — the table under the SQL in the file's own
//! view — is M3, and none of this is thrown away to build it: `result_rows` is the one
//! producer of what a row says, and a region would draw the same rows differently.
//!
//! Two rules hold the surface honest:
//!
//! - **A refusal is a row, not a silence.** A picker with no rows reads as an empty vault,
//!   so every failure — no index, a file that will not parse, SQL that SQLite refuses, a
//!   query that answers nothing — opens the picker with one row saying which it is
//!   ([`PickerTarget::Notice`]). "Zero" and "I cannot look" are different answers, the
//!   posture EDGE-25 set for `:props`.
//! - **The core reads what the query answers; the surface decides what a person can
//!   scan.** The cut and the row that admits it are produced together, so they cannot
//!   disagree about how many rows there were.

use std::path::Path;

use anyhow::anyhow;
use gpui::{Context, Window};
use shale_core::db::Cell;
use shale_core::vault::{rel_text, VaultEntry};
use shale_core::QueryNote;

use crate::acts::*;
use crate::picker::PickerItem;
use crate::prompt;

use super::open::{notice, PickerTarget};
use super::Workspace;

/// How many rows one query's answer may draw. Named in the core, because the in-file
/// table cuts at the same number and says how many it left out: one answer, one length.
use shale_core::db::MAX_ROWS;

/// Where a saved query lives (ruling I). One folder, named for what is in it, so a
/// reader who finds one note finds the rest, and the tree shows them together.
const QUERIES_DIR: &str = "queries";

impl Workspace {
    /// `:query` and the palette's **Run a saved query**: the vault's `.shq` files, each
    /// labelled with the name it declares rather than the stem of the file it lives in.
    pub(super) fn show_queries(
        &mut self,
        _: &RunQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.queries.is_empty() {
            self.open_picker(
                "Run a saved query",
                vec![notice("this vault holds no saved queries (.shq)")],
                window,
                cx,
            );
            return;
        }
        let rows = self
            .queries
            .iter()
            .map(|entry| {
                let parsed = self
                    .vault
                    .read(&entry.path)
                    .map_err(|err| anyhow::anyhow!("{err}"))
                    .and_then(|text| QueryNote::parse(&text));
                query_row(entry, &parsed)
            })
            .collect();
        self.open_picker("Run a saved query", rows, window, cx);
    }

    /// Read one `.shq`, run it against the index, and show what came back.
    ///
    /// Every step reports itself. The file is re-read here rather than carried over from
    /// the picker's row because the file is the authority and it may have been edited —
    /// by the reader, or by an agent — since the list was built.
    pub(super) fn run_query(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let label = format!("Query {}", path.display());
        let text = match self.vault.read(path) {
            Ok(text) => text,
            Err(err) => {
                self.open_picker(
                    &label,
                    vec![notice(format!("the query note could not be read: {err}"))],
                    window,
                    cx,
                );
                return;
            }
        };
        // EDGE-14: with the index off there is nothing to answer from, and "no note
        // answers this" would be a lie about the vault rather than about the setting.
        let Some(db) = self.db.as_ref() else {
            self.open_picker(
                &label,
                vec![notice(
                    "there is no vault index to read (index.enabled = false)",
                )],
                window,
                cx,
            );
            return;
        };
        let conn = match db.read() {
            Ok(conn) => conn,
            Err(err) => {
                self.open_picker(
                    &label,
                    vec![notice(format!("the vault index could not be read: {err}"))],
                    window,
                    cx,
                );
                return;
            }
        };
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        // The one chain (ruling 44): parse, anchor, ask, sort. The picker's job is to draw
        // what that answered, not to re-decide what a refusal says — so the sentences for
        // EDGE-1, EDGE-2, EDGE-3/4 and EDGE-12 are written once, in core, and the agent
        // door reads the same ones.
        //
        // The anchor's journey (M5): the note the reader is standing in, handed over the
        // same way the editor's table and the query's own view hand it. A relative query
        // run from a surface with no note under the reader — a graph tab, an empty window,
        // or the query open in its own tab — is refused out loud, as a row (EDGE-12).
        let host = self.note();
        let outcome = shale_core::query_note::ask(&text, &stem, host.as_deref(), &conn);
        let (label, rows) = match outcome {
            shale_core::query_note::Outcome::Refused { why } => (label, vec![notice(why)]),
            shale_core::query_note::Outcome::Empty { name } => (
                format!("Query {name}"),
                // EDGE-1: an answer of nothing is still an answer.
                vec![notice("no note answers this")],
            ),
            shale_core::query_note::Outcome::Answered { name, rows } => (
                format!("Query {name}"),
                result_rows(&rows, &self.entries, path),
            ),
        };
        self.open_picker(&label, rows, window, cx);
    }

    /// The palette row and the bare `:save-query`: neither has a search in hand, and
    /// inventing one would answer a question nobody asked. The two doors that do carry
    /// one — the row at the end of a search, and `:save-query tag:monster` — say so.
    pub(super) fn save_query_action(
        &mut self,
        _: &SaveQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ask_save_query("", window, cx);
    }

    /// Ask for a name for a search the index can answer again (P4.3 M6).
    pub(super) fn ask_save_query(
        &mut self,
        search: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if search.trim().is_empty() {
            self.say(
                "save-query needs the search to keep: `:save-query tag:monster` — or run a \
                 search and choose \"save this search as a query\" at the end of its rows",
                cx,
            );
            return;
        }
        // Asked before the name is typed rather than after: a search that cannot be kept
        // should say which part of it cannot before the reader has spent a name on it.
        // The compiler is asked again when the name arrives — it is the authority on what
        // the file will hold, and the answer is what gets written.
        if let Err(reason) = shale_core::db::compile_sql(&shale_core::query::parse(search)) {
            self.say(reason, cx);
            return;
        }
        self.open_prompt(
            prompt::PromptKind::SaveQuery(search.to_string()),
            "name this query",
            &Self::query_name(search),
            window,
            cx,
        );
    }

    /// Write the file: the name is the reader's, the SQL is the search's.
    pub(super) fn write_query(
        &mut self,
        search: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow!("a saved query needs a name"));
        }
        // One name, in one folder. A name holding a separator is the reader reaching for
        // the tree, and a path spliced together out of a prompt is how a file ends up
        // somewhere nobody meant to write to.
        if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
            return Err(anyhow!(
                "a query name is one name — move it into a folder with the tree afterwards"
            ));
        }
        let sql = shale_core::db::compile_sql(&shale_core::query::parse(search))
            .map_err(anyhow::Error::msg)?;
        let path =
            Path::new(QUERIES_DIR).join(format!("{name}.{}", shale_core::query_note::EXTENSION));
        if self.vault.exists(&path) {
            // Ruling I: never overwrite silently. And not loudly either — the file a
            // reader already wrote is worth more than the search they just typed.
            return Err(anyhow!(
                "{} already holds a query — Shale will not overwrite it",
                rel_text(&path)
            ));
        }
        // The search is kept above the SQL as a comment: the file says the question in the
        // words the reader typed, and the statement below is what those words mean.
        let text = format!(
            "+++\nname = \"{}\"\n+++\n-- Saved from the search: {search}\n{sql}\n",
            name.replace('"', "\\\"")
        );
        self.vault.write(&path, &text)?;
        self.refresh_entries(cx);
        self.tree_selection = Some(path.clone());
        // Open what was just written: the reader asked what the search means, and the
        // file's own table is that answer, drawn from the statement they can now read.
        self.open_note(&path, cx);
        self.say(format!("saved {}", rel_text(&path)), cx);
        Ok(())
    }

    /// A name to start the prompt with, from what the search asks for:
    /// `tag:monster prop:cr<=5` becomes `monster-cr`. The reader edits it — this is a
    /// starting point, not a decision about what their question is called.
    fn query_name(search: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        for token in search.split_whitespace() {
            // The operator is the question's shape, not its name, and the comparison is
            // not part of the thing being named.
            let value = token.split_once(':').map_or(token, |(_, value)| value);
            let value = value
                .split(['<', '>', '='])
                .next()
                .unwrap_or("")
                .trim_matches('"');
            if !value.is_empty() {
                parts.push(value.to_lowercase());
            }
        }
        if parts.is_empty() {
            return "query".to_string();
        }
        parts.join("-").chars().take(40).collect()
    }
}

/// One saved query as a row: the name it declares, and the folder it lives in.
///
/// A file that will not parse still gets a row — the vault holds it, and a list that
/// silently drops it would leave the author wondering where their query went. The reason
/// it will not parse rides in the detail, and choosing it says the same thing again.
pub(crate) fn query_row(
    entry: &VaultEntry,
    parsed: &Result<QueryNote, anyhow::Error>,
) -> (PickerItem, PickerTarget) {
    let stem = entry
        .path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| entry.path.display().to_string());
    let folder = folder_of(&entry.path);
    let (label, detail) = match parsed {
        Ok(note) => (
            note.display_name(&stem).to_string(),
            // The declaration's hints, only when they are real: a query that sorts
            // itself or reads from the note the reader is standing in is a different
            // kind of row than a plain list.
            match (note.decl.sort.as_deref(), note.decl.relative) {
                (None, false) => folder,
                (None, true) => format!("{folder} · relative"),
                (Some(sort), false) => format!("{folder} · sorted by {sort}"),
                (Some(sort), true) => format!("{folder} · relative · sorted by {sort}"),
            },
        ),
        Err(err) => (stem, format!("{folder} · {err}")),
    };
    (
        PickerItem {
            label: label.clone().into(),
            detail: detail.clone().into(),
            detail_ranges: Vec::new(),
            search: format!("{label} {}", entry.path.display()).into(),
        },
        PickerTarget::RunQuery(entry.path.clone()),
    )
}

/// What a query's answer looks like as rows: the columns once, then one row per answer.
///
/// A row whose first cell names a note in this vault *is* that note — choosing it goes
/// there, which is what makes a saved query a door rather than a printout. A first cell
/// that ends in `.md` and names no note is left as text: the lens ruling wants a visible
/// plot hole, not a row that looks openable and then fails.
pub(crate) fn result_rows(
    answer: &shale_core::db::Rows,
    entries: &[VaultEntry],
    query: &Path,
) -> Vec<(PickerItem, PickerTarget)> {
    let mut rows = Vec::with_capacity(answer.rows.len() + 2);
    // The row naming the columns is the way back to the file that computed them. The
    // picker is the surface where an answer is readable, and the reader who wants to
    // see or edit the program should reach it from the answer instead of going hunting
    // through the tree.
    rows.push((
        PickerItem {
            label: format!("columns: {}", answer.columns.join(" · ")).into(),
            detail: format!("open {}", query.display()).into(),
            detail_ranges: Vec::new(),
            search: format!("columns {}", query.display()).into(),
        },
        PickerTarget::Note(query.to_path_buf()),
    ));

    for cells in answer.rows.iter().take(MAX_ROWS) {
        let first = cells.first().map(Cell::display).unwrap_or_default();
        let rest = cells
            .iter()
            .skip(1)
            .map(Cell::display)
            .collect::<Vec<_>>()
            .join(" · ");
        // A cell that names a note in this vault is that note. One that does not is
        // left as text: the lens ruling wants a visible plot hole, not a row that looks
        // openable and then fails.
        let target = entries
            .iter()
            .find(|entry| entry.path.to_string_lossy() == first)
            .map(|entry| PickerTarget::Note(entry.path.clone()))
            .unwrap_or_else(|| PickerTarget::Notice(first.clone().into()));
        rows.push((
            PickerItem {
                label: first.clone().into(),
                detail: rest.clone().into(),
                detail_ranges: Vec::new(),
                search: format!("{first} {rest}").into(),
            },
            target,
        ));
    }
    if answer.rows.len() > MAX_ROWS {
        rows.push(notice(format!(
            "…{} more rows not shown",
            answer.rows.len() - MAX_ROWS
        )));
    }
    rows
}

/// The folder a vault path sits in, spelled the way a note row spells it. `""` for a
/// file at the vault root, which the caller folds into its detail.
fn folder_of(path: &Path) -> String {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.display().to_string(),
        _ => "the vault's top level".to_string(),
    }
}
