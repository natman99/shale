//! The operators (`d`, `c`, `y`), the text objects they range over, and the
//! unnamed register.
//!
//! A motion decides *where*; this decides *what happens* to the bytes in between,
//! and records them so `.` and `p` can use them again.

use crate::buffer::Buffer;
use crate::table;

use super::{repeat, Key, Operator, Outcome, Pending, Register, Vim};

impl Vim {
    /// Apply `op` over `count` repetitions of the motion `key`.
    pub(super) fn apply_operator(
        &mut self,
        op: Operator,
        count: usize,
        key: Key,
        buf: &mut Buffer,
    ) -> Outcome {
        // Where the cursor sits in a table, if anywhere: a linewise delete
        // removes rows and must leave the cursor in the cell that took their
        // place, and a charwise motion must not sweep out of the cell.
        let located = table::locate(&table::tables(buf.text()), buf.cursor());
        // Linewise forms: the doubled operator, and line-crossing motions.
        let linewise = match key {
            Key::Char(c) if c == op.char() => Some(count),
            Key::Char('j') | Key::Char('k') => Some(count + 1),
            _ => None,
        };

        if let Some(lines) = linewise {
            let row = buf.row_col(buf.cursor()).0;
            match op {
                Operator::Yank => {
                    self.register = Some(Register {
                        text: buf.yank_lines(row, lines),
                        linewise: true,
                    });
                    return Outcome::Stay;
                }
                Operator::Delete => {
                    buf.delete_line(lines);
                    // `dd` on a table row removes the row; the cell that moved
                    // up into its place becomes the cursor's cell.
                    if let Some(loc) = located {
                        if let Some(offset) =
                            table::cursor_for(buf, loc.first_row, loc.row, loc.column)
                        {
                            buf.set_cursor(offset);
                        }
                    }
                }
                Operator::Change => {
                    buf.delete_line(lines);
                    return Outcome::EnterInsert;
                }
            }
            return Outcome::Stay;
        }

        // Charwise: run the motion, then act on the swept range.
        let start = buf.cursor();
        match key {
            Key::Char('w') => repeat(count, || buf.word_forward()),
            Key::Char('e') => repeat(count, || buf.word_end()),
            Key::Char('b') => repeat(count, || buf.word_backward()),
            Key::Char('h') => repeat(count, || buf.move_left()),
            Key::Char('l') => repeat(count, || buf.move_right_stay_on_line()),
            Key::Char(' ') => repeat(count, || buf.move_right_stay_on_line()),
            Key::Char('0') => buf.line_start_motion(),
            Key::Char('^') => buf.first_non_blank(),
            Key::Char('$') => buf.line_last_char_motion(),
            Key::Char('G') => buf.goto_line(count.saturating_sub(1)),
            Key::Char('f') | Key::Char('F') | Key::Char('t') | Key::Char('T') => {
                // The operator waits for the search character, e.g. `dfw`.
                self.pending = Pending::Find {
                    op: Some(op),
                    count,
                    forward: matches!(key, Key::Char('f') | Key::Char('t')),
                    inclusive: matches!(key, Key::Char('f') | Key::Char('F')),
                };
                return Outcome::Stay;
            }
            Key::Char('i') | Key::Char('a') => {
                // The operator waits for the object's scope, e.g. `diw`.
                self.pending = Pending::TextObject {
                    op: Some(op),
                    count,
                    around: key == Key::Char('a'),
                };
                return Outcome::Stay;
            }
            _ => {
                // Unknown motion: cancel the operator rather than guessing.
                return Outcome::Unhandled;
            }
        }
        let mut end = buf.cursor();
        // `$` and `e` are inclusive motions: they land *on* a character, which
        // the operator must include. `w` is exclusive, so it stops at the start
        // of the next word and that character is left alone.
        if matches!(key, Key::Char('$') | Key::Char('e')) && end < buf.text().len() {
            end = buf.next_char_boundary(end);
        }
        // Inside a cell the sweep stops at the cell's content edge, so `dw` at
        // the end of a cell deletes the rest of that cell and not the next one.
        if let Some(loc) = located {
            end = end.clamp(loc.cell.nav_start(), loc.cell.end);
        }
        let (lo, hi) = (start.min(end), start.max(end));
        self.apply_range(op, lo, hi, buf)
    }

    /// The range of the text object with the given scope under `offset`.
    pub(super) fn object_range(
        buf: &Buffer,
        offset: usize,
        scope: char,
        inner: bool,
    ) -> (usize, usize) {
        match scope {
            'p' => buf.paragraph_object(offset, inner),
            _ => buf.word_object(offset, inner),
        }
    }

    /// Apply `op` to the byte range `[lo, hi)`.
    pub(super) fn apply_range(
        &mut self,
        op: Operator,
        lo: usize,
        hi: usize,
        buf: &mut Buffer,
    ) -> Outcome {
        match op {
            Operator::Yank => {
                self.register = Some(Register {
                    text: buf.text()[lo..hi].to_string(),
                    linewise: false,
                });
            }
            Operator::Delete => {
                self.register = Some(Register {
                    text: buf.text()[lo..hi].to_string(),
                    linewise: false,
                });
                buf.delete_range(lo, hi);
            }
            Operator::Change => {
                buf.delete_range(lo, hi);
                return Outcome::EnterInsert;
            }
        }
        Outcome::Stay
    }

    /// Paste the register after (`p`) or before (`P`) the cursor.
    pub(super) fn paste(&mut self, after: bool, buf: &mut Buffer) {
        let Some(reg) = self.register.clone() else {
            return;
        };
        let len = buf.text().len();
        if reg.linewise {
            let row = buf.row_col(buf.cursor()).0;
            let text = if reg.text.ends_with('\n') {
                reg.text
            } else {
                format!("{}\n", reg.text)
            };
            let offset = if after {
                (buf.line_end(row) + 1).min(len)
            } else {
                buf.line_start(row)
            };
            buf.insert_at(offset, &text);
        } else {
            let offset = if after {
                buf.next_char_boundary(buf.cursor()).min(len)
            } else {
                buf.cursor()
            };
            buf.insert_at(offset, &reg.text);
        }
    }
}
