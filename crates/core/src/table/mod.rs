//! Markdown tables: structure, cell lookup, and row/column editing.
//!
//! A table is the GFM form: a header row, a delimiter row (`| --- | :--: |`),
//! then body rows. Detection is shared with [`crate::mdstyle`] so the styling
//! and the editing model can never disagree about which lines are a table.
//!
//! Everything here works on byte offsets into the whole buffer text, always on
//! character boundaries. A cell carries three ranges:
//!
//! - the **content** (`start..end`, padding and pipes excluded),
//! - the **region** (`span_start..span_end`, the text between the two pipes,
//!   padding included),
//! - the **pipes** that open and close it (`open_pipe` / `close_pipe`).
//!
//! Navigation uses the pipes: a cursor sitting on padding, or on the pipe that
//! *closes* a cell, belongs to that cell, while the pipe that *opens* it belongs
//! to the cell on its left. That partitions every row exactly, so a cursor can
//! always be resolved to a column — which is what keeps movement from "skipping
//! around" when a row is ragged or a cell is empty.
//!
//! The delimiter row is the one exception: it is painted as a rule and has no
//! text to edit, so it is not a cell, the motions skip it, and a click on it is
//! snapped into a real cell.
//!
//! The row and column edits live in [`edit`]; this module is the model: parsing,
//! cell lookup, and turning an offset into a cell.

mod edit;

pub use edit::{
    cursor_for, delete_column, delete_row, insert_column_after, insert_column_before,
    insert_row_after, insert_row_before, move_column, move_row,
};

use crate::mdstyle::{is_table_delimiter_row, table_row_flags};

/// One cell of a table row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// First byte of the cell's content, leading padding excluded.
    pub start: usize,
    /// End of the content, exclusive.
    pub end: usize,
    /// First byte of the region between the delimiters, padding included.
    pub span_start: usize,
    /// End of that region, exclusive (equal to `close_pipe` when there is one).
    pub span_end: usize,
    /// The pipe that opens this cell, if the row has one.
    pub open_pipe: Option<usize>,
    /// The pipe that closes this cell, if the row has one.
    pub close_pipe: Option<usize>,
    /// Column index, 0-based.
    pub column: usize,
}

impl Cell {
    /// Lowest offset that belongs to this cell: just after its opening pipe, or
    /// the start of the region when the cell is the row's first region.
    pub fn nav_start(&self) -> usize {
        self.open_pipe.map_or(self.span_start, |pipe| pipe + 1)
    }

    /// Highest offset that belongs to this cell: its closing pipe, or the end of
    /// the region when there is none.
    pub fn nav_end(&self) -> usize {
        self.close_pipe.unwrap_or(self.span_end)
    }

    /// Whether the cursor offset belongs to this cell. Padding and the pipe that
    /// closes the cell are included; the pipe that opens it belongs to the cell
    /// on its left.
    pub fn contains(&self, offset: usize) -> bool {
        self.nav_start() <= offset && offset <= self.nav_end()
    }

    /// Where the cursor should sit when this cell is selected: on its first
    /// content character, or on the padding when the cell is empty.
    pub fn cursor(&self) -> usize {
        if self.start < self.end {
            self.start
        } else {
            self.span_start
        }
    }

    /// Whether this cell was synthesized to pad a ragged row, i.e. there is no
    /// text for it at all.
    pub fn is_synthetic(&self, row_end: usize) -> bool {
        self.start == row_end && self.end == row_end && self.close_pipe.is_none()
    }
}

/// One line of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    /// Buffer row index.
    pub row: usize,
    /// Start of the line.
    pub start: usize,
    /// End of the line, excluding the newline.
    pub end: usize,
    /// Cells left to right. Always exactly the table's column count: a short
    /// row gets empty cells at its end, a long row's extras are dropped.
    pub cells: Vec<Cell>,
}

/// A table: header row, delimiter row, then body rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// Rows in order.
    pub rows: Vec<TableRow>,
    /// Column count, taken from the delimiter row.
    pub columns: usize,
    /// Index (within `rows`) of the delimiter row, normally 1.
    pub delimiter: usize,
}

impl Table {
    /// The widest content per column, in characters, used to lay the table out
    /// as an aligned grid. The delimiter row counts too: live preview usually
    /// draws it as a rule, but the row comes back as its own text when the
    /// cursor is on it or on the header, and a column narrower than its `:---:`
    /// would have nowhere to put that text. `text` is the buffer text the
    /// offsets refer to.
    pub fn column_widths(&self, text: &str) -> Vec<usize> {
        let mut widths = vec![0usize; self.columns];
        for row in &self.rows {
            for cell in &row.cells {
                let width = content_chars(text, cell);
                if width > widths[cell.column] {
                    widths[cell.column] = width;
                }
            }
        }
        widths
    }

    /// The row index containing `offset`.
    pub fn row_at(&self, offset: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| r.start <= offset && offset <= r.end)
    }

    /// The `(row index, column)` containing `offset`. A cursor on a pipe or on
    /// padding resolves to the cell that owns that region.
    ///
    /// The delimiter row has no cells: it is painted as a rule and its dashes
    /// are not cell content, so an offset on it resolves to no cell at all and
    /// the vim engine treats that line as an ordinary one. The shifted vertical
    /// motions never come to rest on it (see [`Table::navigable_rows`]); plain
    /// `j`/`k` do, whenever live preview is drawing the row (which is when the
    /// cursor is already on it or on the header above it).
    pub fn cell_at(&self, offset: usize) -> Option<(usize, usize)> {
        let row = self.row_at(offset)?;
        if self.is_delimiter(row) {
            return None;
        }
        let cells = &self.rows[row].cells;
        let column = cells
            .iter()
            .find(|c| c.contains(offset))
            .map(|c| c.column)
            // `offset` is on the row's leading pipe: take the first cell.
            .or_else(|| cells.first().map(|c| c.column))?;
        Some((row, column))
    }

    /// The cell at `(row index, column)`, clamped into the table so callers can
    /// ask for a position that an edit may have removed. A request for the
    /// delimiter row is answered with the nearest row that has cells, which is
    /// what keeps an edit's cursor off the rule.
    pub fn cell_clamped(&self, row: usize, column: usize) -> Option<&Cell> {
        if self.rows.is_empty() || self.columns == 0 {
            return None;
        }
        let index = self.row_beyond_delimiter(row.min(self.rows.len() - 1));
        let row = self.rows.get(index)?;
        let column = column.min(self.columns - 1);
        row.cells.get(column)
    }

    /// Whether `row_index` is the delimiter row.
    pub fn is_delimiter(&self, row_index: usize) -> bool {
        row_index == self.delimiter
    }

    /// The rows the shifted vertical motions (`J`/`K`) walk: every row but the
    /// delimiter, which is painted as a rule and has no cell to edit, so they
    /// skip the rule and can leave the table. Plain `j`/`k` are ordinary line
    /// motions and follow the display model instead, which drops the delimiter
    /// row only while nothing is on it.
    pub fn navigable_rows(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.rows.len()).filter(move |index| !self.is_delimiter(*index))
    }

    /// The nearest row index to `row` that has cells. The row below the
    /// delimiter is preferred, because that is where a cursor that was sent to
    /// the rule most likely wanted to go; the row above is the fallback when the
    /// delimiter is the table's last row.
    fn row_beyond_delimiter(&self, row: usize) -> usize {
        if row != self.delimiter {
            return row;
        }
        if self.delimiter + 1 < self.rows.len() {
            self.delimiter + 1
        } else {
            self.delimiter.saturating_sub(1)
        }
    }
}

/// The content width of a cell in characters (not bytes, so multi-byte text
/// still lines up in the aligned grid). Cell bounds are always character
/// boundaries, so the slice is safe.
fn content_chars(text: &str, cell: &Cell) -> usize {
    text.get(cell.start..cell.end)
        .map(|content| content.chars().count())
        .unwrap_or(0)
}

/// Every table in `text`, in order.
pub fn tables(text: &str) -> Vec<Table> {
    if !text.contains('|') {
        // No row can be a table row without a pipe, and the editor asks this on
        // every keystroke.
        return Vec::new();
    }
    let lines = line_starts_and_content(text);
    let flags = table_row_flags(&lines);
    let mut out: Vec<Table> = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        if !flags[index] {
            index += 1;
            continue;
        }
        let mut rows: Vec<TableRow> = Vec::new();
        while index < lines.len() && flags[index] {
            let (start, content) = lines[index];
            rows.push(TableRow {
                row: index,
                start,
                end: start + content.len(),
                cells: split_cells(start, content),
            });
            index += 1;
        }
        // The delimiter row decides the column count; a group without one (only
        // possible if the flags ever disagree) is not a table.
        let Some(delimiter) = rows
            .iter()
            .position(|r| is_table_delimiter_row(&text[r.start..r.end]))
        else {
            continue;
        };
        let columns = rows[delimiter].cells.len().max(1);
        for row in &mut rows {
            normalize_cells(row, columns);
        }
        out.push(Table {
            rows,
            columns,
            delimiter,
        });
    }
    out
}

/// Where the cursor sits inside a table: everything the editor needs to move it
/// and to edit the table it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    /// Index of the table within the buffer.
    pub table: usize,
    /// Buffer row the table starts on, which identifies it again after an edit.
    pub first_row: usize,
    /// Row index within the table.
    pub row: usize,
    /// Column index within the table.
    pub column: usize,
    /// The cell under the cursor.
    pub cell: Cell,
}

/// The cell under `offset`, given the buffer's tables, or `None` when the cursor
/// is not in a table.
pub fn locate(tables: &[Table], offset: usize) -> Option<Located> {
    for (index, table) in tables.iter().enumerate() {
        let Some((row, column)) = table.cell_at(offset) else {
            continue;
        };
        let cell = *table.rows[row].cells.get(column)?;
        return Some(Located {
            table: index,
            first_row: table.rows.first()?.row,
            row,
            column,
            cell,
        });
    }
    None
}

/// `(line start, content without the newline)` for every line.
fn line_starts_and_content(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let content_len = line.trim_end_matches(['\n', '\r']).len();
        out.push((offset, &line[..content_len]));
        offset += line.len();
    }
    out
}

/// Byte offsets of the cell delimiters in a line. A `\|` is content, not a
/// delimiter.
fn pipe_offsets(content: &str) -> Vec<usize> {
    let mut pipes = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            // Skip the escaped character whatever it is, so `\|` stays content.
            b'\\' => {
                i += 1;
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'|' => {
                pipes.push(i);
                i += 1;
            }
            _ => i += 1,
        }
    }
    pipes
}

/// Split a line into cells at its unescaped `|` delimiters, following GFM: the
/// row is split at every pipe, and the empty margins outside a leading or
/// trailing pipe are not cells.
fn split_cells(line_start: usize, content: &str) -> Vec<Cell> {
    let pipes = pipe_offsets(content);
    let mut cells: Vec<Cell> = Vec::new();
    for region in 0..=pipes.len() {
        let span_start = if region == 0 {
            0
        } else {
            pipes[region - 1] + 1
        };
        let span_end = if region == pipes.len() {
            content.len()
        } else {
            pipes[region]
        };
        let open_pipe = if region == 0 {
            None
        } else {
            Some(line_start + pipes[region - 1])
        };
        let close_pipe = if region == pipes.len() {
            None
        } else {
            Some(line_start + pipes[region])
        };
        let text = &content[span_start..span_end];
        // The margin before a leading pipe, and the margin after a trailing
        // one, are not cells.
        if text.trim().is_empty() {
            let before_leading_pipe = region == 0 && pipes.first() == Some(&0);
            let after_trailing_pipe = region == pipes.len() && !cells.is_empty();
            if before_leading_pipe || after_trailing_pipe {
                continue;
            }
        }
        // `end = start + trimmed length` keeps the content range empty-but-
        // well-ordered for a whitespace-only region, where the leading and
        // trailing trims would otherwise cross over.
        let lead = text.len() - text.trim_start().len();
        let start = line_start + span_start + lead;
        cells.push(Cell {
            start,
            end: start + text.trim().len(),
            span_start: line_start + span_start,
            span_end: line_start + span_end,
            open_pipe,
            close_pipe,
            column: cells.len(),
        });
    }
    cells
}

/// Pad or truncate a row's cells so every row has `columns` of them.
fn normalize_cells(row: &mut TableRow, columns: usize) {
    while row.cells.len() < columns {
        let column = row.cells.len();
        row.cells.push(Cell {
            start: row.end,
            end: row.end,
            span_start: row.end,
            span_end: row.end,
            open_pipe: None,
            close_pipe: None,
            column,
        });
    }
    if row.cells.len() > columns {
        row.cells.truncate(columns);
    }
    for (index, cell) in row.cells.iter_mut().enumerate() {
        cell.column = index;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cell contents of every table: table -> row -> cell.
    pub(super) fn grid(text: &str) -> Vec<Vec<Vec<String>>> {
        tables(text)
            .into_iter()
            .map(|t| {
                t.rows
                    .iter()
                    .map(|r| {
                        r.cells
                            .iter()
                            .map(|c| text[c.start..c.end].to_string())
                            .collect()
                    })
                    .collect()
            })
            .collect()
    }

    pub(super) const TABLE: &str = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n";

    #[test]
    fn parses_rows_and_cells() {
        let ts = tables(TABLE);
        assert_eq!(ts.len(), 1);
        let t = &ts[0];
        assert_eq!(t.columns, 2);
        assert_eq!(t.rows.len(), 4);
        assert_eq!(t.delimiter, 1);
        assert_eq!(
            t.column_widths(TABLE),
            vec![3, 3],
            "the rule's dashes are the floor"
        );
        assert_eq!(
            grid(TABLE)[0],
            vec![
                vec!["a", "bb"],
                vec!["---", "---"],
                vec!["1", "2"],
                vec!["3", "4"],
            ]
        );
    }

    #[test]
    fn a_cursor_on_a_pipe_or_padding_still_belongs_to_a_cell() {
        let text = "| 1 | 2 |\n| --- | --- |\n";
        let t = &tables(text)[0];
        let row = &t.rows[0];
        // `| 1 | 2 |`: leading pipe, padding, content, padding, separator pipe,
        // padding, content, trailing pipe.
        assert_eq!(t.cell_at(row.start), Some((0, 0)), "leading pipe");
        assert_eq!(t.cell_at(row.start + 1), Some((0, 0)), "padding");
        assert_eq!(t.cell_at(row.start + 2), Some((0, 0)), "content");
        assert_eq!(t.cell_at(row.start + 3), Some((0, 0)), "padding");
        assert_eq!(t.cell_at(row.start + 4), Some((0, 0)), "separator pipe");
        assert_eq!(t.cell_at(row.start + 5), Some((0, 1)), "padding");
        assert_eq!(t.cell_at(row.start + 6), Some((0, 1)), "content");
        assert_eq!(t.cell_at(row.start + 7), Some((0, 1)), "trailing pipe");
    }

    #[test]
    fn every_offset_in_a_table_row_resolves_to_a_cell() {
        let text = "| a | a longer cell | c |\n| --- | --- | --- |\n";
        let t = &tables(text)[0];
        let row = &t.rows[0];
        for offset in row.start..=row.end {
            assert!(t.cell_at(offset).is_some(), "offset {offset} is unowned");
        }
    }

    #[test]
    fn ragged_rows_get_empty_cells() {
        let text = "| a | b | c |\n| --- | --- | --- |\n| 1 |\n";
        let t = &tables(text)[0];
        assert_eq!(t.columns, 3);
        let row = &t.rows[2];
        assert_eq!(row.cells.len(), 3);
        assert_eq!(row.cells[2].start, row.cells[2].end, "empty");
        assert_eq!(row.cells[2].cursor(), row.end, "sits at the line end");
        assert!(row.cells[2].is_synthetic(row.end));
    }

    #[test]
    fn escaped_pipes_are_content() {
        let text = "| a \\| b | c |\n| --- | --- |\n| 1 | 2 |\n";
        let t = &tables(text)[0];
        assert_eq!(t.columns, 2);
        assert_eq!(grid(text)[0][0], vec!["a \\| b", "c"]);
    }

    #[test]
    fn a_row_of_only_pipes_terminates() {
        // Every `|` is a delimiter, so a scanner that forgets to step past one
        // spins forever and grows its boundary list until the process is OOM
        // killed. Keep this case cheap and exact.
        let text = "|| ||\n| --- | --- |\n| 1 | 2 |\n";
        let ts = tables(text);
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].columns, 2);
        assert_eq!(grid(text)[0][0], vec!["", ""]);
    }

    #[test]
    fn a_row_without_outer_pipes_is_still_cells() {
        let text = "a | b\n--- | ---\n1 | 2\n";
        let ts = tables(text);
        assert_eq!(ts.len(), 1);
        assert_eq!(grid(text)[0][0], vec!["a", "b"]);
    }

    #[test]
    fn no_delimiter_row_is_not_a_table() {
        assert!(tables("| a | b |\n| 1 | 2 |\n").is_empty());
    }

    #[test]
    fn locate_finds_the_cell_under_the_cursor() {
        let text = "prose\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let tables = tables(text);
        let cursor = text.find('2').unwrap();
        let loc = locate(&tables, cursor).unwrap();
        assert_eq!(
            (loc.table, loc.first_row, loc.row, loc.column),
            (0, 2, 2, 1)
        );
        assert_eq!(loc.cell.start, cursor);
        assert!(locate(&tables, text.find("prose").unwrap()).is_none());
    }

    #[test]
    fn the_delimiter_row_is_not_a_cell() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n";
        let table = &tables(text)[0];
        // Every offset of the rule line resolves to no cell, so the vim engine
        // treats it as an ordinary line rather than as dashes to edit.
        let rule = text.find("---").unwrap();
        for offset in rule..rule + 3 {
            assert!(table.cell_at(offset).is_none(), "offset {offset}");
            assert!(locate(&tables(text), offset).is_none());
        }
        // It is still a row of the table, and it is the one the shifted
        // vertical motions skip.
        assert_eq!(table.row_at(rule), Some(1));
        assert_eq!(table.navigable_rows().collect::<Vec<_>>(), vec![0, 2]);
    }

    #[test]
    fn the_delimiter_row_has_cells_that_set_the_column_floor() {
        let text = "| a | bb |\n| :--- | ---: |\n| 1 | 2 |\n";
        let table = &tables(text)[0];
        // When live preview draws the rule row it draws its cells, so a column
        // must be wide enough for the alignment markers it holds.
        assert_eq!(grid(text)[0][1], vec![":---", "---:"]);
        assert_eq!(table.column_widths(text), vec![4, 4]);
    }

    #[test]
    fn tables_outside_a_table_are_left_alone() {
        let text = "prose | not | a table\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let ts = tables(text);
        assert_eq!(ts.len(), 1);
        assert_eq!(
            ts[0].rows[0].row, 2,
            "the table starts after the blank line"
        );
    }
}
