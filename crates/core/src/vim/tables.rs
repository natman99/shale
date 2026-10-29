//! The table commands, and the cell motions that keep a cursor inside a cell.
//!
//! Consulted before the ordinary motions, because inside a table `h`/`j`/`k`/`l`
//! mean cells rather than characters and rows, and `|`, `\`, `o`, `O`, `D` and
//! `S` mean columns and rows of the grid. The free functions below are the
//! stepping: they move the cursor from cell to cell without ever landing in the
//! padding or on the delimiter row.

use crate::buffer::Buffer;
use crate::table::{self, Cell, Located, Table};

use super::{repeat, Key, Outcome, Vim};

impl Vim {
    /// Table commands, consulted before the ordinary motions. Returns `None`
    /// when the key is not a table command, so its normal binding applies.
    ///
    /// `tables` and `loc` describe the buffer as it was when the key arrived; the
    /// edit helpers re-parse it and reposition the cursor themselves.
    pub(super) fn table_command(
        &mut self,
        key: Key,
        count: usize,
        tables: &[Table],
        loc: Located,
        buf: &mut Buffer,
    ) -> Option<Outcome> {
        let Key::Char(c) = key else {
            return None;
        };
        let table = tables.get(loc.table)?;
        let count = count as isize;
        match c {
            // Cell-to-cell motions. `h`/`l` walk the cell's content and step into
            // the neighbouring cell at its edge; the shifted forms jump a whole
            // column. `J`/`K` step through the grid's rows, skipping the
            // delimiter row and walking out of the table at its top or bottom.
            // Plain `j`/`k` are not table commands at all: they stay the ordinary
            // line motions, which follow the display model and so can stop on the
            // delimiter row while live preview is drawing it.
            'H' => step_cell(table, loc, 0, -count, buf),
            'L' => step_cell(table, loc, 0, count, buf),
            'K' => match step_cell_rows(table, loc, -count, buf) {
                // Walking off the top of the table leaves it: whatever part of the
                // count the table did not use becomes the ordinary up motion.
                Some(leftover) if leftover > 0 => {
                    repeat(leftover, || buf.move_up());
                    step_past_rule(table, buf, false);
                }
                Some(_) => {}
                None => buf.move_up(),
            },
            'J' => match step_cell_rows(table, loc, count, buf) {
                Some(leftover) if leftover > 0 => {
                    repeat(leftover, || buf.move_down());
                    step_past_rule(table, buf, true);
                }
                Some(_) => {}
                None => buf.move_down(),
            },
            'D' => {
                // Each delete re-locates the cursor, which can shift the table's
                // indices, so the table is re-read for every column.
                for _ in 0..count.max(1) {
                    let tables = table::tables(buf.text());
                    let Some(loc) = table::locate(&tables, buf.cursor()) else {
                        break;
                    };
                    table::delete_column(buf, &tables[loc.table], loc.row, loc.column);
                }
            }
            '|' => table::insert_column_after(buf, table, loc.row, loc.column),
            '\\' => table::insert_column_before(buf, table, loc.row, loc.column),
            'I' => {
                // Shift+I empties the cell and puts the cursor in it; outside a
                // table `I` keeps its vim meaning of row-first-non-blank.
                buf.delete_range(loc.cell.start, loc.cell.end);
                buf.set_cursor(loc.cell.start);
                return Some(Outcome::EnterInsert);
            }
            'A' => {
                // Append at the end of the cell's content rather than the row's.
                buf.set_cursor(loc.cell.end);
                return Some(Outcome::EnterInsert);
            }
            'o' => {
                table::insert_row_after(buf, table, loc.row, loc.column);
                return Some(Outcome::EnterInsert);
            }
            'O' => {
                table::insert_row_before(buf, table, loc.row, loc.column);
                return Some(Outcome::EnterInsert);
            }
            _ => return None,
        }
        Some(Outcome::Stay)
    }
}

/// Moves the cursor to the cell `d_row` rows and `d_col` columns away, clamped
/// inside the table, landing on its first content character. Clamping is what
/// keeps the cursor from jumping to another table or wandering off the grid when
/// it is already at an edge. Used for the column jumps (`H`/`L`); the vertical
/// motions go through [`step_cell_rows`], which can leave the table.
fn step_cell(table: &Table, loc: Located, d_row: isize, d_col: isize, buf: &mut Buffer) {
    let last_row = table.rows.len().saturating_sub(1) as isize;
    let last_col = table.columns.saturating_sub(1) as isize;
    let row = (loc.row as isize + d_row).clamp(0, last_row) as usize;
    let column = (loc.column as isize + d_col).clamp(0, last_col) as usize;
    if let Some(cell) = table.cell_clamped(row, column) {
        let at = cell.cursor();
        buf.set_cursor(at);
    }
}

/// The shifted vertical motion (`J`/`K`): `delta` rows down (or up, when
/// negative) in the same column, skipping the delimiter row, landing on the
/// cell's first content character. Returns the part of the count the table could
/// not use — the steps left over after walking off its top or bottom, which the
/// caller spends on the
/// ordinary line motion so the cursor escapes the table — and `Some(0)` when the
/// motion stayed inside. `None` means the cursor was not on a navigable row at
/// all, so the caller should just use the ordinary motion.
fn step_cell_rows(table: &Table, loc: Located, delta: isize, buf: &mut Buffer) -> Option<usize> {
    let rows: Vec<usize> = table.navigable_rows().collect();
    let pos = rows.iter().position(|r| *r == loc.row)? as isize;
    let target = pos + delta;
    // Walking off the table comes to rest on its edge first, so the steps that
    // are left over continue from there instead of re-crossing the grid.
    let (index, leftover) = if target < 0 {
        (0, (-target) as usize)
    } else if target as usize >= rows.len() {
        (rows.len() - 1, target as usize - (rows.len() - 1))
    } else {
        (target as usize, 0)
    };
    let cell = table.rows[rows[index]].cells.get(loc.column)?;
    buf.set_cursor(cell.cursor());
    Some(leftover)
}

/// A vertical motion that left a table must not come to rest on its rule line:
/// if the ordinary line motion landed there, step once more in the same
/// direction. `table` describes the buffer as the motion saw it (no edits are
/// involved), so its row offsets are still exact.
fn step_past_rule(table: &Table, buf: &mut Buffer, down: bool) {
    if table
        .row_at(buf.cursor())
        .is_some_and(|row| table.is_delimiter(row))
    {
        if down {
            buf.move_down();
        } else {
            buf.move_up();
        }
    }
}

/// Where a character motion (`h`/`l`/`w`/`b`/`e`) may come to rest inside a
/// table: on a cell's content, never on its padding or pipes. A motion that
/// walks off the end of a cell steps into the next one in the same row (the
/// previous one for a backward motion); at the edge of the row it stays on the
/// cell's own content instead of escaping into the gutter.
///
/// Which end of the neighbour it lands on follows the motion: `l`, `w` and `b`
/// arrive at a cell's first content character, while `h` and the inclusive `e`
/// arrive at its last. An empty cell has no content, so its padding is the only
/// place to sit and `Cell::cursor()` is used.
pub(super) fn snap_cell_motion(table: &Table, loc: Located, key: Key, buf: &mut Buffer) {
    let cell = loc.cell;
    let cursor = buf.cursor();
    // Already on the cell's content: the motion was ordinary, leave it alone.
    if cell.start <= cursor && cursor < cell.end {
        return;
    }
    let forward = matches!(key, Key::Char('l' | ' ' | 'w' | 'e'));
    let land_on_end = matches!(key, Key::Char('h' | 'e'));
    let Some(row) = table.rows.get(loc.row) else {
        return;
    };
    let neighbour = if forward {
        row.cells.get(loc.column + 1)
    } else if loc.column > 0 {
        row.cells.get(loc.column - 1)
    } else {
        None
    };
    let at = match neighbour {
        Some(cell) => cell_edge(buf, cell, land_on_end),
        // At the edge of the row there is nowhere to step, so the cursor stays on
        // this cell's content at the end the motion was travelling towards.
        None => cell_edge(buf, &cell, forward),
    };
    buf.set_cursor(at);
}

/// The offset of a cell's first or last content character. An empty cell has no
/// content, so its padding is the only place a cursor can sit.
fn cell_edge(buf: &Buffer, cell: &Cell, last: bool) -> usize {
    if last && cell.start < cell.end {
        buf.prev_char_boundary(cell.end)
    } else {
        cell.cursor()
    }
}
