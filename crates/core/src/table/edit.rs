//! Editing a markdown table: rows and columns come and go, and the cursor ends
//! up where the user expects.
//!
//! Every edit is a splice rather than a rewrite, so a table the user aligned by
//! hand stays aligned where they left it: dropping a column removes exactly that
//! cell's region plus the pipe that closes it, and inserting one adds `  |` (or
//! ` --- |` on the delimiter row, which keeps the row a valid delimiter). Moving
//! a row or column rotates the ones it travels through, and the delimiter row
//! never moves, so the rule stays under the header.
//!
//! Each edit re-positions the cursor through [`cursor_for`], which re-identifies
//! the table by the buffer row it starts on and clamps the requested position:
//! deleting the last row or column selects the cell that took its place, and if
//! the table is gone entirely the cursor is left where the edit put it rather
//! than jumping to some other table. Edits that touch every row run inside an
//! undo group, so one `u` undoes one command.

use super::{tables, Cell, Table, TableRow};
use crate::buffer::Buffer;

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
    use crate::table::tests::TABLE;

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
