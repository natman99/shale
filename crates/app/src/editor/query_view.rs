//! The query note's own view: the answer, hung under the program that computed it.
//!
//! The fold hides the program (see [`crate::fold`]'s query region); this file draws what
//! the program was for. The rows are phantom rows — they own no bytes in the buffer, so
//! `/` never finds one and the caret never lands in one (EDGE-23). They hang under the
//! last visible row of the program, which is the marker row when the fold is shut and the
//! last line of the SQL when it is open: one rule, so the answer is always below whatever
//! of the program is showing.
//!
//! The table is the same answer `:query` draws, from the same `Db::run` on the same
//! read-only connection, sorted by the same declaration. Two surfaces with two answers
//! would be two files to believe.

use shale_core::db::{Cell, MAX_ROWS};
use shale_core::mdstyle::Style;
use shale_core::vault::VaultEntry;

use super::overlay::BlockLine;
use super::Answer;
use crate::line::phantom::PhantomKind;
use crate::table_view::GridPaint;

/// How much a query's answer came back with, with no name in front of it.
///
/// A refusal says *refused* rather than `0 rows`, because a count that quietly reads
/// zero would let the reader believe the query asked and got an answer. (In the query
/// note itself a run-time refusal opens the fold instead, so a marker never carries this
/// wording; an embed has no fold to open.)
pub(crate) fn answer_count(answer: &Answer) -> String {
    if answer.error.is_some() {
        return "refused".into();
    }
    let rows = answer.rows.rows.len();
    format!("{rows} {}", if rows == 1 { "row" } else { "rows" })
}

/// What a query's answer is *called*: its name, and how much came back. The query note's
/// marker row says this — there, the fold has hidden the whole program, so the marker is
/// the only thing left saying what the file is. A note that *embeds* the query says only
/// [`answer_count`]: the row above the block already names the query, and a name printed
/// twice is one name too many.
pub(crate) fn answer_shape(name: &str, answer: &Answer) -> String {
    format!("{name} · {}", answer_count(answer))
}

#[cfg(test)]
impl super::MarkdownEditor {
    /// What the answer table would paint, one string per row: for the tests, the same
    /// rows the frame hangs under the program.
    pub(crate) fn query_shape(&mut self) -> Vec<String> {
        let Some(answer) = self.query_answer() else {
            return Vec::new();
        };
        let entries = self.embeds.entries().to_vec();
        super::query_view::answer_lines(&answer, &entries)
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    /// The answer's rows as the frame would break them: one string per **painted
    /// piece**, so a test can see a refusal that runs past the pane arrive in pieces
    /// rather than off the screen. Same `row_ranges` the frame calls.
    /// The rows that wear a grid: the row's text, the rule positions in character cells,
    /// the grid's width, and whether the row is the shaded header. A test can put a rule
    /// and a glyph side by side and see that they never share a cell.
    pub(crate) fn query_grid(&mut self) -> Vec<(String, Vec<usize>, usize, bool)> {
        let Some(answer) = self.query_answer() else {
            return Vec::new();
        };
        let entries = self.embeds.entries().to_vec();
        answer_lines(&answer, &entries)
            .into_iter()
            .filter_map(|line| {
                let grid = line.grid?;
                Some((line.text, grid.boundaries, grid.total, grid.header))
            })
            .collect()
    }
    pub(crate) fn query_pieces(&mut self, window: &gpui::Window) -> Vec<String> {
        let Some(answer) = self.query_answer() else {
            return Vec::new();
        };
        let entries = self.embeds.entries().to_vec();
        let width = self.wrap_width();
        let palette = std::rc::Rc::clone(&self.palette);
        let mut wrappers = self.body_wrappers(window);
        answer_lines(&answer, &entries)
            .iter()
            .flat_map(|line| {
                row_ranges(line, width, &mut wrappers, &palette)
                    .into_iter()
                    .map(move |(lo, hi)| line.text[lo..hi].to_string())
            })
            .collect()
    }
}

/// How wide one cell may be before it is clipped. A table row that runs past the pane
/// has nowhere to go — phantom rows do not soft-wrap the way the note's own rows do —
/// and a clipped cell says so with `…` rather than running off the screen unseen.
const CELL_WIDTH: usize = 60;

/// The gap between columns, in spaces. Two, because the font is monospace and one
/// would let a narrow column's text sit against the next column's.
const GAP: usize = 2;

/// A cell's worth of space inside the grid's outer edges, so a vertical rule never sits
/// against a value. The same gutter a markdown table's grid keeps.
const GUT: usize = 1;

/// The table's rows: a header, one line per answer, and whatever the answer wants to
/// say about itself (a refusal, an empty result, the rows it did not draw).
///
/// `entries` is the vault's file list, and it decides one thing only: whether a cell
/// *is* a note. The rule is the picker's rule — the cell spelled exactly as a vault
/// path — so the same answer cannot be a door in one surface and a dead end in the
/// other.
pub(crate) fn answer_lines(answer: &Answer, entries: &[VaultEntry]) -> Vec<BlockLine> {
    let mut out = Vec::new();
    if let Some(err) = answer.error.as_deref() {
        out.push(line(
            format!(
                "the query could not be answered: {}",
                shale_core::db::one_line(err)
            ),
            PhantomKind::Refusal,
            Vec::new(),
            None,
        ));
        return out;
    }
    if answer.rows.rows.is_empty() {
        out.push(line(
            "the query answers nothing".to_string(),
            PhantomKind::Caption,
            Vec::new(),
            None,
        ));
        return out;
    }

    let shown: Vec<&Vec<Cell>> = answer.rows.rows.iter().take(MAX_ROWS).collect();
    // Every cell spelled once, so the widths and the rows are built from one reading.
    let cells: Vec<Vec<String>> = shown
        .iter()
        .map(|row| row.iter().map(spelled).collect())
        .collect();
    // One width per column: as wide as its name, as wide as its widest answer cell.
    let widths: Vec<usize> = answer
        .rows
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let answers = cells
                .iter()
                .map(|row| row.get(index).map_or(0, |cell| cell.chars().count()));
            answers.max().unwrap_or(0).max(column.chars().count())
        })
        .collect();

    out.push(BlockLine {
        grid: Some(GridPaint {
            header: true,
            ..grid_paint(&widths)
        }),
        ..line(
            join(&answer.rows.columns, &widths),
            PhantomKind::Header,
            Vec::new(),
            None,
        )
    });
    for cells in &cells {
        // The first cell is the one the picker treats as the row's identity, so it is
        // the one that can be a door here too.
        let door = cells
            .first()
            .and_then(|cell| entry_named(entries, cell))
            .map(|path| shale_core::transclude::EmbedRef {
                target: path,
                anchor: None,
                alias: None,
            });
        let spans = door
            .as_ref()
            .map(|_| vec![(GUT, GUT + cells[0].len(), Style::WikiLink)])
            .unwrap_or_default();
        let text = join(cells, &widths);
        let grid = grid_paint(&widths);
        // The door's click belongs to the cell the link is painted in. Bounding it to
        // the row's band made a click past the table's right edge — over the note the
        // answer hangs under — open the link instead (the reader's report, 2026-11-10).
        let door_span = door.as_ref().map(|_| cell_around(&grid, GUT));
        out.push(BlockLine {
            door_span,
            grid: Some(grid),
            ..line(text, PhantomKind::Row, spans, door)
        });
    }
    let hidden = answer.rows.rows.len() - shown.len();
    if hidden > 0 {
        out.push(line(
            format!("…{hidden} more rows not shown"),
            PhantomKind::Caption,
            Vec::new(),
            None,
        ));
    }
    out
}

/// The grid cell that holds the character cell `at`: the pair of boundaries enclosing
/// it, in character cells from the grid's left edge. A door's hit box is its cell, so a
/// click in the padding of a neighbouring column — or past the table entirely — is the
/// note's, not the answer's.
fn cell_around(grid: &GridPaint, at: usize) -> (usize, usize) {
    // The cell is the one whose left edge is the last edge at or before `at`. Past the
    // grid's right edge that is its last cell, which is what a clipped column's door
    // should still answer for.
    let index = grid
        .boundaries
        .iter()
        .position(|&edge| edge > at)
        .map(|edge| edge.saturating_sub(1))
        .unwrap_or(grid.boundaries.len().saturating_sub(2));
    let from = grid.boundaries[index];
    let to = grid
        .boundaries
        .get(index + 1)
        .copied()
        .unwrap_or(grid.total);
    (from, to)
}

/// The grid over a row: the outer edges, a rule in the middle of each gap between
/// columns, and the row's top and bottom edges. Measured in character cells, which is what
/// the padded text is measured in, so a rule lands in the gap and never on a value.
fn grid_paint(widths: &[usize]) -> GridPaint {
    let mut boundaries = vec![0usize];
    let mut start = GUT;
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            boundaries.push(start - GAP / 2);
        }
        start += width + GAP;
    }
    let total = start - GAP + GUT;
    boundaries.push(total);
    GridPaint {
        boundaries,
        total,
        header: false,
        top_rule: true,
        bottom_rule: true,
    }
}

/// Whether a row of the answer is prose the pane can run out of room for.
///
/// A table row keeps its columns: breaking one mid-cell would leave the continuation
/// lines with nothing to align to, and the cell clip already bounds each one. A sentence
/// the answer says about itself — a refusal, an admission about the rows it left out — is
/// prose, and a database message quoting the statement it refused is longer than any pane.
pub(crate) fn is_prose(line: &BlockLine) -> bool {
    matches!(
        line.phantom,
        Some(PhantomKind::Refusal) | Some(PhantomKind::Caption)
    )
}

/// Where a row of the answer breaks at `width`. The frame and the tests ask this one
/// function, or the test measures a break the paint never makes.
pub(crate) fn row_ranges(
    line: &BlockLine,
    width: gpui::Pixels,
    wrappers: &mut [gpui::LineWrapperHandle],
    palette: &shale_core::theme::Palette,
) -> Vec<(usize, usize)> {
    if !is_prose(line) {
        return vec![(0, line.text.len())];
    }
    super::embeds::wrap_text(&line.text, &line.spans, None, width, wrappers, palette)
}

/// One row of the table as the reader sees it: each cell clipped to the column's
/// allowance, then padded to the column's width.
fn join(cells: &[String], widths: &[usize]) -> String {
    let mut out = " ".repeat(GUT);
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            out.push_str(&" ".repeat(GAP));
        }
        out.push_str(cell);
        let width = widths.get(index).copied().unwrap_or(0);
        let used = cell.chars().count();
        if used < width {
            out.push_str(&" ".repeat(width - used));
        }
    }
    out.trim_end().to_string()
}

/// One cell, spelled the way the picker spells it (a NULL is `∅`, an empty string is
/// `''`), on one line, and clipped if it is enormous. A cell may hold a newline — an
/// excerpt, a description — and a table row may not: phantom rows do not soft-wrap the
/// way the note's own rows do, so the newline becomes a space here.
fn spelled(cell: &Cell) -> String {
    let text = shale_core::db::one_line(&cell.display());
    if text.chars().count() <= CELL_WIDTH {
        return text;
    }
    let clipped: String = text.chars().take(CELL_WIDTH).collect();
    format!("{clipped}…")
}

/// The vault path a cell names, exactly as the picker decides it: the cell spelled as
/// a path this vault holds. Anything else stays text — a row that looks openable and
/// then is not is the visible plot hole the lens ruling already refused.
fn entry_named(entries: &[VaultEntry], cell: &str) -> Option<String> {
    let cell = cell.trim();
    if cell.is_empty() {
        return None;
    }
    entries
        .iter()
        .find(|entry| entry.path.to_string_lossy() == cell)
        .map(|entry| entry.path.to_string_lossy().into_owned())
}

/// One phantom row.
fn line(
    text: String,
    kind: PhantomKind,
    spans: Vec<(usize, usize, Style)>,
    door: Option<shale_core::transclude::EmbedRef>,
) -> BlockLine {
    BlockLine {
        door_span: None,
        text,
        depth: 0,
        badge: false,
        door,
        spans,
        phantom: Some(kind),
        grid: None,
        origin: None,
        gutter: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A door's hit box is its cell, not the row: the boundaries enclosing the first
    /// column's text, and — for a link in a later column — that column's own cell.
    #[test]
    fn a_door_is_bounded_to_the_cell_its_link_is_painted_in() {
        // Two columns, 8 and 5 wide: the outer edges, and a rule in the middle of the
        // gap between them.
        let grid = grid_paint(&[8, 5]);
        assert_eq!(grid.boundaries, vec![0, 10, 17], "the grid's own edges");
        assert_eq!(cell_around(&grid, GUT), (0, 10), "the first column's cell");
        assert_eq!(cell_around(&grid, 12), (10, 17), "the second column's cell");
        assert_eq!(
            cell_around(&grid, 99),
            (10, 17),
            "past the grid is still its last cell"
        );
    }

    /// The row itself carries that cell, not the grid: a click in the next column over
    /// is a click on a number, and a number does not open a note.
    #[test]
    fn a_rows_door_covers_its_own_cell_and_not_the_next_column() {
        let answer = Answer {
            rows: shale_core::db::Rows {
                columns: vec!["note".to_string(), "cr".to_string()],
                rows: vec![vec![
                    shale_core::db::Cell::Text("Goblin.md".to_string()),
                    shale_core::db::Cell::Integer(5),
                ]],
            },
            error: None,
        };
        let entries = [shale_core::vault::VaultEntry {
            path: std::path::PathBuf::from("Goblin.md"),
        }];
        let lines = answer_lines(&answer, &entries);
        let row = lines
            .iter()
            .find(|line| line.door.is_some())
            .expect("the row that names a note is a door");
        let grid = row.grid.clone().expect("an answer row wears the grid");
        assert_eq!(
            row.door_span,
            Some((0, grid.boundaries[1])),
            "the door is the first cell, ending at the rule between the columns"
        );
        assert!(
            row.door_span.unwrap().1 < grid.total,
            "and does not reach across the whole table: {:?}",
            row.door_span
        );
    }
}
