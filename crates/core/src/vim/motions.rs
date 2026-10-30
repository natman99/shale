//! The normal-mode state machine: what a bare key does to the cursor.
//!
//! Motions, the pending-command prefixes (`g`, `f`, `i`, `z`) and the insert
//! transitions. The state it reads and writes is [`Vim`]'s, and the vocabulary it
//! matches on lives with the struct in `vim/mod.rs`; the operators it hands off to
//! are in [`crate::vim::operators`], and the table commands in
//! [`crate::vim::tables`].

use crate::buffer::Buffer;
use crate::table;

use super::tables::snap_cell_motion;
use super::{char_range, repeat, Key, Operator, Outcome, Pending, Register, Vim};

impl Vim {
    /// The normal-mode state machine: motions, operators and insert transitions.
    pub(super) fn handle_motion_key(&mut self, key: Key, buf: &mut Buffer) -> Outcome {
        // The table under the cursor, if any. Parsed per key: the scan is cheap
        // (it bails out at once when the text has no pipe) and every table edit
        // below invalidates it anyway.
        let tables = table::tables(buf.text());
        let located = table::locate(&tables, buf.cursor());

        // A digit extends the count. A bare `0` (with no count in progress) is
        // the line-start motion instead. A digit that is the argument of a
        // half-typed command (`f0`) is a literal, not a count.
        if let Key::Char(c @ '0'..='9') = key {
            if !self.waiting_for_literal() {
                if c == '0' && self.count.is_none() && self.pending == Pending::None {
                    self.zero_motion(buf);
                    return Outcome::Stay;
                }
                self.count = Some(self.count.unwrap_or(0) * 10 + (c as usize - '0' as usize));
                return Outcome::Stay;
            }
        }

        // A find is waiting for its character.
        if let Pending::Find {
            op,
            count,
            forward,
            inclusive,
        } = self.pending
        {
            self.pending = Pending::None;
            let Key::Char(c) = key else {
                return Outcome::Unhandled;
            };
            let start = buf.cursor();
            buf.find_char(c, forward, inclusive, count);
            // Inside a cell, a find that would land outside it counts as "not
            // found": the cursor stays put and an operator is cancelled.
            if located.is_some_and(|loc| !loc.cell.contains(buf.cursor())) {
                buf.set_cursor(start);
                return Outcome::Stay;
            }
            let Some(op) = op else {
                return Outcome::Stay;
            };
            let end = buf.cursor();
            if start == end {
                // Nothing found: the operator is cancelled, as in vim.
                return Outcome::Stay;
            }
            let (lo, hi) = if end > start {
                // A forward `f` lands *on* the character, which the operator
                // must include.
                (
                    start,
                    if inclusive {
                        buf.next_char_boundary(end)
                    } else {
                        end
                    },
                )
            } else {
                (end, start)
            };
            return self.apply_range(op, lo, hi, buf);
        }

        // A text object is waiting for its scope: `diw`, `dap`, `2aw`, `diL`,
        // `ci*`, `` da` ``, `dat`.
        if let Pending::TextObject { op, count, around } = self.pending {
            self.pending = Pending::None;
            let Key::Char(scope) = key else {
                return Outcome::Unhandled;
            };
            let inner = !around;
            let cursor = buf.cursor();
            let Some((lo, mut hi)) = Self::object_range(buf, located, cursor, scope, inner) else {
                // Nothing of that kind under the cursor: the operator is
                // cancelled rather than let loose on a range nobody asked for,
                // exactly as a find that matches nothing is.
                return Outcome::Stay;
            };
            // A count typed before a word or paragraph object extends it over the
            // following objects, as `d2aw` does; the gap between them is absorbed
            // so the sweep stays contiguous. The markdown objects and a table cell
            // are single constructs, so a count there is ignored rather than
            // guessed at.
            if matches!(scope, 'w' | 'p') {
                for _ in 1..count {
                    let mut at = hi;
                    while at < buf.text().len()
                        && buf.text()[at..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_whitespace())
                    {
                        at = buf.next_char_boundary(at);
                    }
                    let (_, next_hi) =
                        Self::object_range(buf, located, at, scope, inner).unwrap_or((hi, hi));
                    if next_hi <= hi {
                        break;
                    }
                    hi = next_hi;
                }
            }
            let Some(op) = op else {
                // Bare `iw`/`ip`: a motion. In visual mode it *is* the
                // selection; in normal mode it just moves the cursor.
                if self.mode.is_visual() {
                    let head = if hi > lo {
                        buf.prev_char_boundary(hi)
                    } else {
                        lo
                    };
                    buf.set_selection(lo, head);
                } else {
                    buf.set_cursor(lo);
                }
                return Outcome::Stay;
            };
            return self.apply_range(op, lo, hi, buf);
        }

        // An operator in progress consumes the next key as its motion.
        if let Pending::Operator { op, count } = self.pending {
            self.pending = Pending::None;
            let motion_count = self.count.take().unwrap_or(1);
            // Vim multiplies a count given on both sides of the operator.
            return self.apply_operator(op, count * motion_count, key, buf);
        }

        if let Pending::Goto { count } = self.pending {
            self.pending = Pending::None;
            let count = self.count.take().unwrap_or(count);
            return match key {
                Key::Char('g') => {
                    if count > 1 {
                        buf.goto_line(count - 1);
                    } else {
                        buf.buffer_start();
                    }
                    Outcome::Stay
                }
                Key::Char('x') => Outcome::FollowLink,
                // `g f` is Shale's quick-open. vim's `gf` edits the file named
                // under the cursor; here the name is typed, so the chord opens the
                // picker instead — and it is one of the ways in now that `ctrl-o`
                // belongs to the jump chain.
                Key::Char('f') => Outcome::QuickOpen,
                _ => Outcome::Unhandled,
            };
        }

        // A fold command: `z` plus the command letter. The second key arrives
        // through the action it is already bound to (`a`, `c`, `o`, `d`) or one
        // added for the shifted forms, so the engine only has to interpret it.
        if let Pending::Fold = self.pending {
            self.pending = Pending::None;
            let Key::Char(c) = key else {
                return Outcome::Unhandled;
            };
            match c {
                'a' => buf.fold_toggle(),
                'c' => buf.fold_close(),
                'o' => buf.fold_open(),
                // `zd` removes the fold the cursor is on. A fold is a closed
                // region here, not a stored definition, so that is the same as
                // opening it.
                'd' => buf.fold_open(),
                'M' => buf.fold_close_all(),
                'R' | 'E' => buf.fold_open_all(),
                _ => return Outcome::Unhandled,
            }
            return Outcome::Stay;
        }

        let count = self.count.take().unwrap_or(1);
        // `alt+hjkl` moves a whole row or column of the table the cursor is in.
        // Outside a table the keys do nothing, so they stay unhandled.
        if let Key::AltChar(c) = key {
            let Some(loc) = located else {
                return Outcome::Unhandled;
            };
            let Some(table) = tables.get(loc.table) else {
                return Outcome::Unhandled;
            };
            let count = count as isize;
            match c {
                'h' => table::move_column(buf, table, loc.row, loc.column, -count),
                'l' => table::move_column(buf, table, loc.row, loc.column, count),
                'k' => table::move_row(buf, table, loc.row, loc.column, -count),
                'j' => table::move_row(buf, table, loc.row, loc.column, count),
                _ => return Outcome::Unhandled,
            }
            return Outcome::Stay;
        }
        // Inside a cell the table commands come first: `H`/`L` jump columns,
        // `J`/`K` step through cells, and `o`/`I`/`A` act on the cell rather
        // than the whole row. Plain `j`/`k` fall through to the ordinary line
        // motions below, which is what lets the cursor reach a table's delimiter
        // row when live preview is drawing it.
        if let Some(loc) = located {
            if let Some(outcome) = self.table_command(key, count, &tables, loc, buf) {
                return outcome;
            }
        }
        // The motions that follow are the ordinary (non-table) ones; a table row
        // adjusts where they come to rest below.
        match key {
            // Motions
            Key::Char('h') => repeat(count, || buf.move_left()),
            Key::Char('j') => repeat(count, || buf.move_down()),
            Key::Char('k') => repeat(count, || buf.move_up()),
            Key::Char('l') => repeat(count, || buf.move_right_stay_on_line()),
            // Vim binds space to the same motion as `l`; as the argument of
            // `f `/`t ` it arrives through the pending find instead. The motion
            // also arms the leader chords (`space f` quick-open, `space s` vault
            // search, and the note's three views).
            Key::Char(' ') => {
                self.after_space = true;
                repeat(count, || buf.move_right_stay_on_line())
            }
            Key::Char('w') => repeat(count, || buf.word_forward()),
            Key::Char('b') => repeat(count, || buf.word_backward()),
            Key::Char('e') => repeat(count, || buf.word_end()),
            Key::Char('f') | Key::Char('F') | Key::Char('t') | Key::Char('T') => {
                self.pending = Pending::Find {
                    op: None,
                    count,
                    forward: matches!(key, Key::Char('f') | Key::Char('t')),
                    inclusive: matches!(key, Key::Char('f') | Key::Char('F')),
                };
                return Outcome::Stay;
            }
            Key::Char('^') => buf.first_non_blank(),
            Key::Char('$') => buf.line_last_char_motion(),
            Key::Char('G') => {
                if count > 1 {
                    buf.goto_line(count - 1);
                } else {
                    buf.buffer_end();
                }
            }
            Key::Char('g') => {
                self.pending = Pending::Goto { count };
                return Outcome::Stay;
            }
            Key::Char('z') => {
                self.pending = Pending::Fold;
                return Outcome::Stay;
            }
            // Operators
            Key::Char('d') | Key::Char('c') | Key::Char('y') => {
                let op = match key {
                    Key::Char('d') => Operator::Delete,
                    Key::Char('c') => Operator::Change,
                    _ => Operator::Yank,
                };
                self.pending = Pending::Operator { op, count };
                return Outcome::Stay;
            }
            Key::Char('x') => {
                let (start, end) = char_range(buf, buf.cursor(), count);
                self.register = Some(Register {
                    text: buf.text()[start..end].to_string(),
                    linewise: false,
                });
                buf.delete_range(start, end);
            }
            Key::Char('p') => self.paste(true, buf),
            Key::Char('P') => self.paste(false, buf),
            // Visual mode entry
            Key::Char('v') => {
                self.enter_visual(false, buf);
                return Outcome::Stay;
            }
            Key::Char('V') => {
                self.enter_visual(true, buf);
                return Outcome::Stay;
            }
            // Insert transitions
            Key::Char('i') => return Outcome::EnterInsert,
            Key::Char('a') => {
                buf.move_right();
                return Outcome::EnterInsert;
            }
            Key::Char('I') => {
                // `I` is line-wise, unlike `^` (which is screen-line).
                buf.row_first_non_blank();
                return Outcome::EnterInsert;
            }
            Key::Char('A') => {
                // Likewise `A` appends at the end of the whole row. The step off
                // the last character has to stay on the row: an empty row has no
                // last character, and a plain `move_right` would cross the
                // newline and start appending on the line below.
                buf.row_last_char_motion();
                buf.move_right_stay_on_line();
                return Outcome::EnterInsert;
            }
            Key::Char('o') => {
                buf.open_line_below(count);
                return Outcome::EnterInsert;
            }
            Key::Char('O') => {
                buf.open_line_above(count);
                return Outcome::EnterInsert;
            }
            // Undo (redo is bound to ctrl-r by the UI).
            Key::Char('u') => buf.undo(),
            Key::Escape => buf.clear_selection(),
            // vim's "look this up", handed to the window: see [`Outcome::Hover`].
            Key::Char('K') => return Outcome::Hover,
            _ => return Outcome::Unhandled,
        }
        // Inside a cell the ordinary motions stay on the table's content: one
        // that would come to rest on padding, a pipe or outside the row steps
        // into the neighbouring cell instead. The column-edge motions stop at the
        // cell's content edges rather than the line's.
        if let Some(loc) = located {
            let cell = loc.cell;
            match key {
                Key::Char('h' | 'l' | ' ' | 'w' | 'b' | 'e') => {
                    if let Some(table) = tables.get(loc.table) {
                        snap_cell_motion(table, loc, key, buf);
                    }
                }
                Key::Char('0') => buf.set_cursor(cell.cursor()),
                Key::Char('^') => buf.set_cursor(cell.cursor()),
                Key::Char('$') => {
                    buf.set_cursor(if cell.start < cell.end {
                        buf.prev_char_boundary(cell.end)
                    } else {
                        cell.nav_start()
                    });
                }
                _ => {}
            }
        }
        Outcome::Stay
    }

    /// The `0` motion: the start of the screen line, or the first content
    /// character of the cell when the cursor is inside a table (its padding is
    /// not a place a cursor should come to rest).
    pub(super) fn zero_motion(&self, buf: &mut Buffer) {
        match table::locate(&table::tables(buf.text()), buf.cursor()) {
            Some(loc) => buf.set_cursor(loc.cell.cursor()),
            None => buf.line_start_motion(),
        }
    }
}
