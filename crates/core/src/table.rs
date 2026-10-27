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
//! Edits are splices, not rewrites: dropping a column removes exactly the region
//! and its closing pipe, and inserting one adds `  |` (or ` --- |` on the
//! delimiter row, which keeps the row a valid delimiter). A hand-aligned table
//! therefore stays aligned where the user left it.

use crate::buffer::Buffer;
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
    /// as an aligned grid. The delimiter row is skipped: it is drawn as a rule,
    /// not as text. `text` is the buffer text the offsets refer to.
    pub fn column_widths(&self, text: &str) -> Vec<usize> {
        let mut widths = vec![0usize; self.columns];
        for (index, row) in self.rows.iter().enumerate() {
            if index == self.delimiter {
                continue;
            }
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
    /// The delimiter row has no cells: it is painted as a rule and has no text
    /// worth editing, so an offset on it resolves to no cell at all and the vim
    /// engine treats that line as an ordinary one. Motions never come to rest on
    /// it either (see [`Table::navigable_rows`]), and a click on it is snapped
    /// into a real cell by [`clickable_offset`].
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

    /// The rows a cursor can stand on: every row but the delimiter, which is
    /// painted as a rule and has no text to edit. This is what the vertical
    /// motions walk, so `j`/`k` skip the rule and can leave the table.
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

/// The offset a click on `offset` was meant for. A click on a table's delimiter
/// row — painted as a rule, with no text under the pointer — belongs to the cell
/// below it in the same column, or the one above when the delimiter is the
/// table's last row. Every other offset comes back unchanged.
pub fn clickable_offset(tables: &[Table], offset: usize) -> usize {
    for table in tables {
        let Some(row) = table.row_at(offset) else {
            continue;
        };
        if !table.is_delimiter(row) {
            return offset;
        }
        // The column the pointer was over, from the delimiter row's own cells,
        // which partition the row exactly like any other row's do.
        let Some(column) = table.rows[row]
            .cells
            .iter()
            .find(|c| c.contains(offset))
            .or_else(|| nearest_cell_left(&table.rows[row], offset))
            .map(|c| c.column)
        else {
            return offset;
        };
        let index = table.row_beyond_delimiter(row);
        let Some(target) = table.rows.get(index) else {
            return offset;
        };
        return target
            .cells
            .get(column)
            .map(|c| c.cursor())
            .unwrap_or(offset);
    }
    offset
}

/// The last cell that starts at or before `offset`, for an offset that no cell
/// claims (a row whose cells were synthesized for a ragged table).
fn nearest_cell_left(row: &TableRow, offset: usize) -> Option<&Cell> {
    row.cells
        .iter()
        .rev()
        .find(|c| c.span_start <= offset)
        .or_else(|| row.cells.first())
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

/// The text that makes one new column on a row: a dash run on the delimiter row
/// (so the row stays a valid delimiter), empty padding elsewhere. Returned as
/// `(insert_after, insert_before)` so the caller picks the side.
fn new_column_text(table: &Table, row_index: usize) -> (&'static str, &'static str) {
    if table.is_delimiter(row_index) {
        (" --- |", "| --- ")
    } else {
        ("  |", "|  ")
    }
}

/// The byte range that makes up column `column` of `row` — its region plus the
/// pipe that closes it — or `None` when the cell is padding synthesized for a
/// ragged row and there is nothing to remove.
fn column_cut(row: &TableRow, cell: &Cell) -> Option<(usize, usize)> {
    if cell.is_synthetic(row.end) {
        return None;
    }
    let end = cell.close_pipe.map_or(row.end, |pipe| pipe + 1);
    if end <= cell.span_start {
        return None;
    }
    Some((cell.span_start, end))
}

/// Where a new column goes on `row`: just after the closing pipe of `after`
/// (inserting `after_text`), or just before its opening pipe (inserting
/// `before_text`) for an insert-before.
fn column_splice(row: &TableRow, cell: &Cell, after: bool, text: &str) -> Option<(usize, String)> {
    if after {
        let at = cell.close_pipe.map_or(row.end, |pipe| pipe + 1);
        Some((at, text.to_string()))
    } else {
        let at = cell.open_pipe.unwrap_or(cell.span_start);
        Some((at, text.to_string()))
    }
}

/// Where the cursor belongs after an edit: the cell at `(first_row, row,
/// column)` in the *current* text, clamped so deleting the last row or column
/// selects the one that took its place. `first_row` is the buffer row the table
/// started on, which identifies it after the edit; if the table is gone (its
/// delimiter row was deleted, say) this returns `None` and the caller leaves the
/// cursor where the edit put it.
pub fn cursor_for(buf: &Buffer, first_row: usize, row: usize, column: usize) -> Option<usize> {
    let table = tables(buf.text())
        .into_iter()
        .find(|t| t.rows.first().is_some_and(|r| r.row == first_row))?;
    let cell = table.cell_clamped(row, column)?;
    Some(cell.cursor())
}

/// The offset the cursor should land on after an edit.
fn landing(buf: &Buffer, first_row: usize, row: usize, column: usize) -> Option<usize> {
    cursor_for(buf, first_row, row, column)
}

/// Delete the table row at `row_index`, then report where the cursor should go:
/// the cell in the same column of the row that took its place, or the row above
/// when the last row was deleted. One undo step.
pub fn delete_row(buf: &mut Buffer, table: &Table, row_index: usize, column: usize) {
    let Some(row) = table.rows.get(row_index) else {
        return;
    };
    let first_row = table.rows.first().map(|r| r.row).unwrap_or(row.row);
    let (start, end) = buf.line_range(row.row, 1);
    buf.delete_range(start, end);
    if let Some(offset) = landing(buf, first_row, row_index, column) {
        buf.set_cursor(offset);
    }
}

/// Delete column `column` from every row of the table, as one undo step.
pub fn delete_column(buf: &mut Buffer, table: &Table, row_index: usize, column: usize) {
    if table.columns < 2 {
        // A one-column table is not a table any more; leave it alone rather
        // than leaving a line of bare pipes behind.
        return;
    }
    let first_row = table.rows.first().map(|r| r.row).unwrap_or(0);
    buf.begin_undo_group();
    // Bottom-up so the offsets of the rows above stay valid.
    for row in table.rows.iter().rev() {
        let Some(cell) = row.cells.get(column) else {
            continue;
        };
        if let Some((start, end)) = column_cut(row, cell) {
            buf.delete_range(start, end);
        }
    }
    buf.end_undo_group();
    if let Some(offset) = landing(buf, first_row, row_index, column) {
        buf.set_cursor(offset);
    }
}

/// Insert an empty row after `row_index`, keeping every column.
pub fn insert_row_after(buf: &mut Buffer, table: &Table, row_index: usize, column: usize) {
    insert_row(buf, table, row_index, column, true);
}

/// Insert an empty row before `row_index`, keeping every column.
pub fn insert_row_before(buf: &mut Buffer, table: &Table, row_index: usize, column: usize) {
    insert_row(buf, table, row_index, column, false);
}

fn insert_row(buf: &mut Buffer, table: &Table, row_index: usize, column: usize, after: bool) {
    let Some(row) = table.rows.get(row_index) else {
        return;
    };
    let first_row = table.rows.first().map(|r| r.row).unwrap_or(row.row);
    let mut line = String::from('|');
    for _ in 0..table.columns {
        line.push_str("  |");
    }
    // The new line goes after `row`'s end or before `row`'s start, and carries
    // its own newline on the correct side so it never merges with `row`.
    let mut new_index = if after { row_index + 1 } else { row_index };
    let mut target = if after { row.end } else { row.start };
    let mut text = if after {
        format!("\n{line}")
    } else {
        format!("{line}\n")
    };
    // A row cannot go between the header and its delimiter row: that would stop
    // the block from being a table at all, so it lands below the delimiter
    // instead.
    if new_index == table.delimiter {
        let Some(delimiter) = table.rows.get(table.delimiter) else {
            return;
        };
        target = delimiter.end;
        new_index = table.delimiter + 1;
        text = format!("\n{line}");
    }
    buf.insert_at(target, &text);
    buf.end_undo_group();
    if let Some(offset) = landing(buf, first_row, new_index, column) {
        buf.set_cursor(offset);
    }
}

/// Insert an empty column after `column`.
pub fn insert_column_after(buf: &mut Buffer, table: &Table, row_index: usize, column: usize) {
    insert_column(buf, table, row_index, column, true);
}

/// Insert an empty column before `column`.
pub fn insert_column_before(buf: &mut Buffer, table: &Table, row_index: usize, column: usize) {
    insert_column(buf, table, row_index, column, false);
}

fn insert_column(buf: &mut Buffer, table: &Table, row_index: usize, column: usize, after: bool) {
    let first_row = table.rows.first().map(|r| r.row).unwrap_or(0);
    buf.begin_undo_group();
    for (index, row) in table.rows.iter().enumerate().rev() {
        let Some(cell) = row.cells.get(column) else {
            continue;
        };
        let (after_text, before_text) = new_column_text(table, index);
        let text = if after { after_text } else { before_text };
        if let Some((at, text)) = column_splice(row, cell, after, text) {
            buf.insert_at(at, &text);
        }
    }
    buf.end_undo_group();
    let landing_column = if after { column + 1 } else { column };
    if let Some(offset) = landing(buf, first_row, row_index, landing_column) {
        buf.set_cursor(offset);
    }
}

/// The text of one table row's line, without its newline.
fn line_text(buf: &Buffer, row: &TableRow) -> String {
    buf.text()[row.start..row.end].to_string()
}

/// Move the table row at `row_index` `delta` rows down (negative moves it up),
/// as one undo step. The delimiter row stays where it is: moving the header down
/// swaps it with the first body row rather than pushing it under the rule. The
/// rows in between shift the other way by one, each keeping its own text exactly
/// as the user left it, and the cursor follows the row that moved.
pub fn move_row(buf: &mut Buffer, table: &Table, row_index: usize, column: usize, delta: isize) {
    let rows: Vec<usize> = table.navigable_rows().collect();
    if rows.len() < 2 {
        return;
    }
    let Some(from) = rows.iter().position(|r| *r == row_index) else {
        return;
    };
    let to = (from as isize + delta).clamp(0, rows.len() as isize - 1) as usize;
    if to == from {
        return;
    }
    let first_row = table.rows.first().map(|r| r.row).unwrap_or(0);
    // The block of rows the moved row travels through, rotated by one.
    let block: Vec<usize> = if to > from {
        rows[from..=to].to_vec()
    } else {
        rows[to..=from].to_vec()
    };
    let mut texts: Vec<String> = block
        .iter()
        .map(|&index| line_text(buf, &table.rows[index]))
        .collect();
    if to > from {
        texts.rotate_left(1);
    } else {
        texts.rotate_right(1);
    }
    buf.begin_undo_group();
    // Bottom-up so the offsets of the rows above stay valid. Each line keeps its
    // newline: only the content between `start` and `end` is replaced.
    for (slot, &index) in block.iter().enumerate().rev() {
        let row = &table.rows[index];
        buf.delete_range(row.start, row.end);
        buf.insert_at(row.start, &texts[slot]);
    }
    buf.end_undo_group();
    if let Some(offset) = landing(buf, first_row, rows[to], column) {
        buf.set_cursor(offset);
    }
}

/// The text of columns `lo..=hi` of `row`, rotated by one column (the column at
/// `lo` moves to the end of the block when moving right, the one at `hi` moves
/// to the front when moving left), with the byte range that block occupies. The
/// columns of a row are adjacent — each one's region runs up to the next one's
/// start — so the whole block is a single splice. `None` when the row does not
/// have every column in the block, i.e. a ragged row, which is left as it is.
fn rotated_columns(
    buf: &Buffer,
    row: &TableRow,
    lo: usize,
    hi: usize,
    right: bool,
) -> Option<(usize, usize, String)> {
    let mut start = usize::MAX;
    let mut end = 0usize;
    let mut chunks: Vec<String> = Vec::with_capacity(hi - lo + 1);
    for index in lo..=hi {
        let cell = row.cells.get(index)?;
        let (from, to) = column_cut(row, cell)?;
        start = start.min(from);
        end = end.max(to);
        chunks.push(buf.text()[from..to].to_string());
    }
    if chunks.len() < 2 {
        return None;
    }
    if right {
        chunks.rotate_left(1);
    } else {
        chunks.rotate_right(1);
    }
    Some((start, end, chunks.concat()))
}

/// Move table column `column` `delta` columns to the right (negative moves it
/// left), as one undo step. The delimiter row's cell travels with the column, so
/// the row stays a valid delimiter, the columns in between shift the other way
/// by one, and the cursor follows the column that moved.
pub fn move_column(buf: &mut Buffer, table: &Table, row_index: usize, column: usize, delta: isize) {
    if table.columns < 2 {
        return;
    }
    let last = table.columns - 1;
    let to = (column as isize + delta).clamp(0, last as isize) as usize;
    if to == column {
        return;
    }
    let (lo, hi) = if to > column {
        (column, to)
    } else {
        (to, column)
    };
    let first_row = table.rows.first().map(|r| r.row).unwrap_or(0);
    buf.begin_undo_group();
    // Bottom-up so the offsets of the rows above stay valid.
    for row in table.rows.iter().rev() {
        if let Some((start, end, text)) = rotated_columns(buf, row, lo, hi, to > column) {
            buf.delete_range(start, end);
            buf.insert_at(start, &text);
        }
    }
    buf.end_undo_group();
    if let Some(offset) = landing(buf, first_row, row_index, to) {
        buf.set_cursor(offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cell contents of every table: table -> row -> cell.
    fn grid(text: &str) -> Vec<Vec<Vec<String>>> {
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

    const TABLE: &str = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n";

    #[test]
    fn parses_rows_and_cells() {
        let ts = tables(TABLE);
        assert_eq!(ts.len(), 1);
        let t = &ts[0];
        assert_eq!(t.columns, 2);
        assert_eq!(t.rows.len(), 4);
        assert_eq!(t.delimiter, 1);
        assert_eq!(t.column_widths(TABLE), vec![1, 2]);
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
        // It is still a row of the table, and it is the one the motions skip.
        assert_eq!(table.row_at(rule), Some(1));
        assert_eq!(table.navigable_rows().collect::<Vec<_>>(), vec![0, 2]);
    }

    #[test]
    fn a_click_on_the_rule_lands_in_a_cell() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n";
        let tables = tables(text);
        let rule = text.find("---").unwrap();
        // The rule line is `| --- | --- |`, so an offset nine bytes into it sits
        // over the second column.
        let over_second_column = rule + 9;
        let snapped = clickable_offset(&tables, over_second_column);
        assert_eq!(
            &text[snapped..snapped + 1],
            "2",
            "the click belongs to the body row's second cell"
        );
        // A click on an ordinary row is left exactly where it was.
        let on_content = text.find('1').unwrap();
        assert_eq!(clickable_offset(&tables, on_content), on_content);
        // A click outside a table is untouched, too.
        assert_eq!(clickable_offset(&tables, 0), 0);
    }

    #[test]
    fn delete_row_selects_the_cell_that_took_its_place() {
        let mut buf = Buffer::new(TABLE.to_string());
        // Cursor on `3` (the last body row, column 0): deleting that row should
        // land on the row that took its place, not wander off.
        let cursor = buf.text().find('3').unwrap();
        buf.set_cursor(cursor);
        let t = &tables(buf.text())[0];
        let (row, column) = t.cell_at(cursor).unwrap();
        delete_row(&mut buf, t, row, column);
        assert_eq!(buf.text(), "| a | bb |\n| --- | --- |\n| 1 | 2 |\n");
        let after = tables(buf.text())[0].cell_at(buf.cursor()).unwrap();
        assert_eq!(after, (2, 0), "the row that moved up is selected");
    }

    #[test]
    fn deleting_the_last_row_falls_back_to_the_row_above() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n";
        let mut buf = Buffer::new(text.to_string());
        let cursor = buf.text().find('1').unwrap();
        buf.set_cursor(cursor);
        let (row, column) = tables(buf.text())[0].cell_at(cursor).unwrap();
        let t = tables(buf.text())[0].clone();
        delete_row(&mut buf, &t, row, column);
        let after = tables(buf.text())[0].cell_at(buf.cursor()).unwrap();
        assert_eq!(
            after,
            (0, 0),
            "only the header and the rule are left, and a cursor never lands on the rule"
        );
    }

    #[test]
    fn delete_column_keeps_the_rest_of_the_row_verbatim() {
        let text = "| a | b | c |\n| --- | --- | --- |\n| 1 | 2 | 3 |\n";
        let mut buf = Buffer::new(text.to_string());
        let cursor = buf.text().find('2').unwrap();
        buf.set_cursor(cursor);
        let t = tables(buf.text())[0].clone();
        let (row, column) = t.cell_at(cursor).unwrap();
        assert_eq!((row, column), (2, 1));
        delete_column(&mut buf, &t, row, column);
        assert_eq!(buf.text(), "| a | c |\n| --- | --- |\n| 1 | 3 |\n");
        let after = tables(buf.text())[0].cell_at(buf.cursor()).unwrap();
        assert_eq!(after, (2, 1), "the cell that took the column's place");
        assert_eq!(&buf.text()[buf.cursor()..buf.cursor() + 1], "3");
    }

    #[test]
    fn deleting_the_first_column_keeps_the_leading_pipe() {
        let text = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(buf.text())[0].clone();
        delete_column(&mut buf, &t, 2, 0);
        assert_eq!(buf.text(), "| b |\n| --- |\n| 2 |\n");
    }

    #[test]
    fn deleting_the_last_column_clamps_to_what_is_left() {
        let text = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let mut buf = Buffer::new(text.to_string());
        let cursor = buf.text().find('2').unwrap();
        buf.set_cursor(cursor);
        let t = tables(buf.text())[0].clone();
        let (row, column) = t.cell_at(cursor).unwrap();
        assert_eq!(column, 1);
        delete_column(&mut buf, &t, row, column);
        assert_eq!(buf.text(), "| a |\n| --- |\n| 1 |\n");
        let after = tables(buf.text())[0].cell_at(buf.cursor()).unwrap();
        assert_eq!(after, (2, 0), "clamped to the only column");
    }

    #[test]
    fn a_one_column_table_is_not_broken_by_deleting_its_only_column() {
        let text = "| a |\n| --- |\n| 1 |\n";
        let mut buf = Buffer::new(text.to_string());
        let cursor = buf.text().find('1').unwrap();
        buf.set_cursor(cursor);
        let t = tables(buf.text())[0].clone();
        delete_column(&mut buf, &t, 2, 0);
        assert_eq!(buf.text(), text, "nothing to delete");
    }

    #[test]
    fn an_aligned_table_stays_aligned_after_dropping_a_column() {
        let text = "| a   | bb | c |\n| --- | -- | - |\n| 111 | 2  | 3 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(buf.text())[0].clone();
        delete_column(&mut buf, &t, 2, 1);
        assert_eq!(buf.text(), "| a   | c |\n| --- | - |\n| 111 | 3 |\n");
    }

    #[test]
    fn insert_row_after_keeps_every_column() {
        let text = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let mut buf = Buffer::new(text.to_string());
        let cursor = buf.text().find('1').unwrap();
        buf.set_cursor(cursor);
        let t = tables(buf.text())[0].clone();
        insert_row_after(&mut buf, &t, 2, 1);
        assert_eq!(buf.text(), "| a | b |\n| --- | --- |\n| 1 | 2 |\n|  |  |\n");
        let after = tables(buf.text())[0].cell_at(buf.cursor()).unwrap();
        assert_eq!(after, (3, 1), "the new row, same column");
    }

    #[test]
    fn insert_row_before_keeps_every_column() {
        let text = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(buf.text())[0].clone();
        insert_row_before(&mut buf, &t, 2, 0);
        assert_eq!(buf.text(), "| a | b |\n| --- | --- |\n|  |  |\n| 1 | 2 |\n");
        let after = tables(buf.text())[0].cell_at(buf.cursor()).unwrap();
        assert_eq!(after, (2, 0), "the new row, same column");
    }

    #[test]
    fn insert_column_after_keeps_the_table_valid() {
        let text = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let mut buf = Buffer::new(text.to_string());
        let cursor = buf.text().find('1').unwrap();
        buf.set_cursor(cursor);
        let t = tables(buf.text())[0].clone();
        insert_column_after(&mut buf, &t, 2, 0);
        assert_eq!(
            buf.text(),
            "| a |  | b |\n| --- | --- | --- |\n| 1 |  | 2 |\n"
        );
        let after = tables(buf.text())[0].cell_at(buf.cursor()).unwrap();
        assert_eq!(after, (2, 1), "the new column is selected");
    }

    #[test]
    fn insert_column_before_keeps_the_table_valid() {
        let text = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(buf.text())[0].clone();
        insert_column_before(&mut buf, &t, 2, 1);
        assert_eq!(
            buf.text(),
            "| a |  | b |\n| --- | --- | --- |\n| 1 |  | 2 |\n"
        );
        let after = tables(buf.text())[0].cell_at(buf.cursor()).unwrap();
        assert_eq!(after, (2, 1), "the new column is selected");
    }

    #[test]
    fn inserting_a_column_after_the_last_one_appends_it() {
        let text = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(buf.text())[0].clone();
        insert_column_after(&mut buf, &t, 2, 1);
        assert_eq!(
            buf.text(),
            "| a | b |  |\n| --- | --- | --- |\n| 1 | 2 |  |\n"
        );
        assert_eq!(tables(buf.text())[0].columns, 3);
    }

    #[test]
    fn a_ragged_row_is_padded_when_a_column_is_inserted() {
        let text = "| a | b | c |\n| --- | --- | --- |\n| 1 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(buf.text())[0].clone();
        insert_column_after(&mut buf, &t, 2, 1);
        assert_eq!(
            buf.text(),
            "| a | b |  | c |\n| --- | --- | --- | --- |\n| 1 |  |\n"
        );
        assert_eq!(tables(buf.text())[0].columns, 4);
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

    #[test]
    fn move_row_down_swaps_it_with_the_row_below() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        // The cursor is on `1`, the first body row; one step down swaps it with
        // `3 | 4`, and the cursor follows the row that moved.
        move_row(&mut buf, &t, 2, 0, 1);
        assert_eq!(
            buf.text(),
            "| a | bb |\n| --- | --- |\n| 3 | 4 |\n| 1 | 2 |\n"
        );
        assert_eq!(&buf.text()[buf.cursor()..buf.cursor() + 1], "1");
    }

    #[test]
    fn move_row_up_from_a_body_row_lands_above_it() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        move_row(&mut buf, &t, 3, 1, -1);
        assert_eq!(
            buf.text(),
            "| a | bb |\n| --- | --- |\n| 3 | 4 |\n| 1 | 2 |\n"
        );
        assert_eq!(&buf.text()[buf.cursor()..buf.cursor() + 1], "4");
    }

    #[test]
    fn moving_the_header_keeps_the_rule_under_it() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        // The delimiter row never moves: the header swaps with the first body
        // row rather than being pushed under the rule.
        move_row(&mut buf, &t, 0, 0, 1);
        assert_eq!(
            buf.text(),
            "| 1 | 2 |\n| --- | --- |\n| a | bb |\n| 3 | 4 |\n"
        );
        let after = &tables(buf.text())[0];
        assert_eq!(after.delimiter, 1, "still a valid table");
        assert_eq!(
            &buf.text()[buf.cursor()..buf.cursor() + 1],
            "a",
            "the cursor follows the row that moved"
        );
    }

    #[test]
    fn a_counted_row_move_rotates_the_rows_it_travels_through() {
        let text = "| a |\n| --- |\n| 1 |\n| 2 |\n| 3 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        move_row(&mut buf, &t, 2, 0, 2);
        assert_eq!(
            buf.text(),
            "| a |\n| --- |\n| 2 |\n| 3 |\n| 1 |\n",
            "the rows it passed shift up by one"
        );
    }

    #[test]
    fn a_row_move_at_the_edge_of_the_table_does_nothing() {
        let text = "| a |\n| --- |\n| 1 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        move_row(&mut buf, &t, 2, 0, 5);
        move_row(&mut buf, &t, 0, 0, -5);
        assert_eq!(buf.text(), text);
        assert_eq!(buf.undo_depth(), 0, "a no-op leaves no undo step");
    }

    #[test]
    fn move_column_right_swaps_it_with_the_column_next_to_it() {
        let text = "| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        move_column(&mut buf, &t, 2, 0, 1);
        assert_eq!(
            buf.text(),
            "| bb | a | ccc |\n| --- | --- | --- |\n| 22 | 1 | 333 |\n"
        );
        // The cursor follows the column that moved, so it is still on `1`.
        assert_eq!(&buf.text()[buf.cursor()..buf.cursor() + 1], "1");
    }

    #[test]
    fn move_column_left_takes_the_rule_cell_with_it() {
        let text = "| a | bb | ccc |\n| --- | :---: | --- |\n| 1 | 22 | 333 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        move_column(&mut buf, &t, 2, 1, -1);
        assert_eq!(
            buf.text(),
            "| bb | a | ccc |\n| :---: | --- | --- |\n| 22 | 1 | 333 |\n",
            "the alignment marker travels with its column"
        );
        let after = &tables(buf.text())[0];
        assert_eq!(after.columns, 3, "still a valid table");
    }

    #[test]
    fn a_counted_column_move_rotates_the_columns_it_travels_through() {
        let text = "| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        move_column(&mut buf, &t, 2, 0, 2);
        assert_eq!(
            buf.text(),
            "| bb | ccc | a |\n| --- | --- | --- |\n| 22 | 333 | 1 |\n"
        );
    }

    #[test]
    fn a_ragged_row_is_left_alone_by_a_column_move() {
        let text = "| a | b | c |\n| --- | --- | --- |\n| 1 |\n";
        let mut buf = Buffer::new(text.to_string());
        let t = tables(text)[0].clone();
        move_column(&mut buf, &t, 2, 0, 1);
        assert_eq!(
            buf.text(),
            "| b | a | c |\n| --- | --- | --- |\n| 1 |\n",
            "the short row has no text to move"
        );
    }

    /// The undo depth after one edit of each kind: a table edit splices every
    /// row, but the user must be able to undo it in one step.
    #[test]
    fn a_table_edit_is_one_undo_step() {
        let text = "| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n";
        let edits: [fn(&mut Buffer, &Table); 5] = [
            |buf, t| delete_column(buf, t, 2, 1),
            |buf, t| insert_column_after(buf, t, 2, 1),
            |buf, t| insert_column_before(buf, t, 2, 1),
            |buf, t| move_column(buf, t, 2, 1, 1),
            |buf, t| move_row(buf, t, 2, 1, -1),
        ];
        for edit in edits {
            let mut buf = Buffer::new(text.to_string());
            let t = tables(text)[0].clone();
            edit(&mut buf, &t);
            assert_ne!(buf.text(), text, "the edit did nothing");
            assert_eq!(buf.undo_depth(), 1, "one step: {text:?}");
            buf.undo();
            assert_eq!(buf.text(), text, "one undo puts the table back");
            buf.redo();
            assert_ne!(buf.text(), text, "and one redo takes it back off");
        }
    }
}
