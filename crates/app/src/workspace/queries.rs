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

use gpui::{Context, Window};
use shale_core::db::Cell;
use shale_core::vault::VaultEntry;
use shale_core::QueryNote;

use crate::acts::*;
use crate::picker::PickerItem;

use super::open::{notice, PickerTarget};
use super::Workspace;

/// How many rows one query's answer may draw. Named in the core, because the in-file
/// table cuts at the same number and says how many it left out: one answer, one length.
use shale_core::db::MAX_ROWS;

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
        let note = match QueryNote::parse(&text) {
            Ok(note) => note,
            Err(err) => {
                self.open_picker(
                    &label,
                    vec![notice(format!("this is not a query note: {err}"))],
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
        // The anchor for a relative query is the note the reader is standing in. M5 is
        // where the declaration's `relative` decides whether an anchor is *required*;
        // binding it whenever there is one is the same mechanism, and a body that never
        // asks for `:current` never sees it.
        let anchor = self.note().map(|path| path.to_string_lossy().into_owned());
        let bindings = match anchor.as_deref() {
            Some(current) => vec![shale_core::db::Binding::new(":current", current)],
            None => Vec::new(),
        };
        // The declaration's `sort` is the reader's override, applied to what came back
        // rather than spliced into the statement: a query that sorts itself and a note
        // that sorts it are one sort.
        let answer = db.run(&note.sql, &bindings).map(|mut rows| {
            if let Some(sort) = note.decl.sort.as_deref() {
                rows.sort_by(sort);
            }
            rows
        });
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let name = note.display_name(&stem).to_string();
        let (label, rows) = match answer {
            Err(err) => (
                format!("Query {name}"),
                vec![notice(format!(
                    "the query could not be answered: {}",
                    shale_core::db::one_line(&format!("{err:#}"))
                ))],
            ),
            Ok(rows) if rows.rows.is_empty() => {
                // EDGE-1: an answer of nothing is still an answer.
                (
                    format!("Query {name}"),
                    vec![notice("no note answers this")],
                )
            }
            Ok(rows) => (format!("Query {name}"), result_rows(&rows, &self.entries)),
        };
        self.open_picker(&label, rows, window, cx);
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
) -> Vec<(PickerItem, PickerTarget)> {
    let mut rows = Vec::with_capacity(answer.rows.len() + 2);
    rows.push(notice(format!("columns: {}", answer.columns.join(" · "))));

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
