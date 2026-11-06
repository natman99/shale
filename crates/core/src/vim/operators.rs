//! The operators (`d`, `c`, `y`), the text objects they range over, and the
//! unnamed register.
//!
//! A motion decides *where*; this decides *what happens* to the bytes in between,
//! and records them so `.` and `p` can use them again.

use crate::buffer::{Buffer, ViewSlot};
use crate::mdstyle;
use crate::table;
use crate::table::Located;

use super::{repeat, Key, OpKind, Operator, Outcome, Pending, Register, Vim};

impl Vim {
    /// Apply `op` over `count` repetitions of the motion `key`, then file the
    /// text it took into the registers vim would have used.
    pub(super) fn apply_operator(
        &mut self,
        op: Operator,
        count: usize,
        key: Key,
        buf: &mut Buffer,
        view: &mut ViewSlot,
    ) -> Outcome {
        let before = self.begin_register_op();
        // A yank does not move the cursor: `0ywp` pastes where the yank started,
        // not where its motion ended. Verified against vim.
        let mark = buf.cursor();
        let outcome = self.apply_operator_inner(op, count, key, buf, view);
        if op == Operator::Yank {
            buf.set_cursor(mark);
        }
        let kind = match op {
            Operator::Yank => OpKind::Yank,
            Operator::Delete => OpKind::Delete,
            Operator::Change => OpKind::Change,
        };
        self.commit_register_op(kind, before);
        outcome
    }

    fn apply_operator_inner(
        &mut self,
        op: Operator,
        count: usize,
        key: Key,
        buf: &mut Buffer,
        view: &mut ViewSlot,
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
                    // The lines go in the register before they go away, as vim's `dd`
                    // does, so `p` can move them: a delete is a cut. With the system
                    // clipboard mirrored that cut is visible to other programs too,
                    // which is why the register has to be right here.
                    let text = buf.yank_lines(row, lines);
                    buf.delete_line(lines);
                    self.register = Some(Register {
                        text,
                        linewise: true,
                    });
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
                    // A change is a cut, as it is in vim: what it replaces goes in
                    // the register, so `cc` then `p` puts the line back somewhere
                    // else. The app mirrors the register, so the mirror sees it too.
                    // The line itself stays: `cc` empties a line rather than removing
                    // it, which is the one place a linewise change and a linewise
                    // delete part company.
                    let text = buf.yank_lines(row, lines);
                    buf.change_lines(row, lines);
                    self.register = Some(Register {
                        text,
                        linewise: true,
                    });
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
            Key::Char('W') => repeat(count, || buf.big_word_forward()),
            Key::Char('E') => repeat(count, || buf.big_word_end()),
            Key::Char('B') => repeat(count, || buf.big_word_backward()),
            Key::Char('h') => repeat(count, || buf.move_left()),
            Key::Char('l') => repeat(count, || buf.move_right_stay_on_line()),
            Key::Char(' ') => repeat(count, || buf.move_right_stay_on_line()),
            Key::Char('0') => buf.line_start_motion(view),
            Key::Char('^') => buf.first_non_blank(view),
            Key::Char('$') => buf.line_last_char_motion(view),
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
            Key::Char(';') | Key::Char(',') => {
                // `d;` sweeps to where the last find would land again, with the
                // same inclusivity — `d;` after an `f` includes the character it
                // lands on, exactly as `df<char>` would.
                let Some(last) = self.last_find else {
                    return Outcome::Unhandled;
                };
                let (c, forward, inclusive) = last.replay(key == Key::Char(','));
                let start = buf.cursor();
                buf.find_char(c, forward, inclusive, count);
                if located.is_some_and(|loc| !loc.cell.contains(buf.cursor())) {
                    buf.set_cursor(start);
                    return Outcome::Stay;
                }
                let end = buf.cursor();
                if start == end {
                    // Nothing found: the operator is cancelled, as in vim.
                    return Outcome::Stay;
                }
                let (lo, hi) = if end > start {
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
            _ => {
                // Unknown motion: cancel the operator rather than guessing.
                return Outcome::Unhandled;
            }
        }
        let mut end = buf.cursor();
        // `$` and `e` are inclusive motions: they land *on* a character, which
        // the operator must include. `w` is exclusive, so it stops at the start
        // of the next word and that character is left alone.
        if matches!(key, Key::Char('$') | Key::Char('e') | Key::Char('E')) && end < buf.text().len()
        {
            end = buf.next_char_boundary(end);
        }
        // A forward word motion that runs off the end of the buffer stops at the
        // end of the *line*, not past it: vim's cursor cannot sit beyond the last
        // character, so `d4w` on a note's last line empties the line rather than
        // taking the line break with it and joining the line below.
        if matches!(key, Key::Char('w' | 'W' | 'e' | 'E')) && end >= buf.text().len() {
            end = buf.line_end(buf.row_col(start).0);
        }
        // Inside a cell the sweep stops at the cell's content edge, so `dw` at
        // the end of a cell deletes the rest of that cell and not the next one.
        if let Some(loc) = located {
            end = end.clamp(loc.cell.nav_start(), loc.cell.end);
        }
        let (lo, hi) = (start.min(end), start.max(end));
        self.apply_range(op, lo, hi, buf)
    }

    /// The range of the text object with the given scope under `offset`, or
    /// `None` when the cursor is not inside one.
    ///
    /// `w` and `p` are the buffer's own words and paragraphs. `t` is answered
    /// from the tables the caller already parsed for its motions, and the markdown
    /// objects from the style scan — which is only run for the scopes that need
    /// it, so an ordinary `dw` never pays for it.
    pub(super) fn object_range(
        buf: &Buffer,
        located: Option<Located>,
        offset: usize,
        scope: char,
        inner: bool,
    ) -> Option<(usize, usize)> {
        match scope {
            'p' => Some(buf.paragraph_object(offset, inner)),
            'w' => Some(buf.word_object(offset, inner)),
            'W' => Some(buf.big_word_object(offset, inner)),
            't' => {
                let cell = located?.cell;
                Some(if inner {
                    // The cell's content, its padding left out.
                    (cell.start, cell.end)
                } else {
                    // The cell, its padding and the pipe that closes it: `dat`
                    // takes the separator with the content, so the row loses the
                    // cell rather than leaving an empty one behind.
                    (
                        cell.nav_start(),
                        cell.close_pipe.map_or(cell.span_end, |pipe| pipe + 1),
                    )
                })
            }
            _ => mdstyle::object::Object::from_char(scope)
                .and_then(|object| mdstyle::object::range(buf.text(), offset, object, inner)),
        }
    }

    /// Apply `op` to the byte range `[lo, hi)`.
    ///
    /// A read-only buffer (preview) refuses the edit, and refusing it *here* is
    /// what keeps `d` from filling the register with text it never deleted and `c`
    /// from promising an insert mode the view will not give. Yank is the one
    /// operator that asks nothing of the buffer, so it still works: a note being
    /// read is a note whose text the user may well want to copy.
    pub(super) fn apply_range(
        &mut self,
        op: Operator,
        lo: usize,
        hi: usize,
        buf: &mut Buffer,
    ) -> Outcome {
        if buf.is_read_only() && !matches!(op, Operator::Yank) {
            return Outcome::Stay;
        }
        if lo == hi {
            // Nothing was swept, so nothing is registered. This is what keeps `X` at
            // the start of a line from throwing away the last yank, and `c0` at
            // column zero from opening an insert mode over text it did not remove.
            return Outcome::Stay;
        }
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
                // The same cut as Delete, for the same reason: `cw` then `p` puts
                // the old word back, and the clipboard sees the change as a cut.
                self.register = Some(Register {
                    text: buf.text()[lo..hi].to_string(),
                    linewise: false,
                });
                buf.delete_range(lo, hi);
                return Outcome::EnterInsert;
            }
        }
        Outcome::Stay
    }

    /// Paste the register after (`p`) or before (`P`) the cursor — the register a
    /// `"` prefix chose if there was one, the unnamed one otherwise.
    pub(super) fn paste(&mut self, after: bool, buf: &mut Buffer) {
        let Some(reg) = self.paste_register() else {
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
