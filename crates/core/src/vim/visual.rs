//! Visual mode: `v` and `V`, the selection they keep, and what motions do to it.
//!
//! The selection lives on [`Buffer`]; the engine only keeps the anchor honest,
//! which is what [`Vim::reanchor`] is for after a motion has moved the cursor.

use crate::buffer::Buffer;

use super::{Key, Mode, Outcome, Pending, Register, Vim};

impl Vim {
    /// Visual mode: motions extend the selection, operators act on it.
    pub(super) fn handle_visual_key(&mut self, key: Key, buf: &mut Buffer) -> Outcome {
        // A digit extends the count, unless it is the argument of a half-typed
        // command (`f0`).
        if let Key::Char(c @ '0'..='9') = key {
            if !self.waiting_for_literal() {
                if c == '0' && self.count.is_none() {
                    self.zero_motion(buf);
                    return self.reanchor(buf);
                }
                self.count = Some(self.count.unwrap_or(0) * 10 + (c as usize - '0' as usize));
                return Outcome::Stay;
            }
        }

        // A half-typed command takes the next key as its argument, whatever it
        // is: `fv` finds a literal `v` even though `v` is a bound key.
        if self.waiting_for_literal() {
            return self.visual_motion(key, buf);
        }

        match key {
            Key::Escape => {
                self.exit_visual(buf);
                Outcome::Stay
            }
            // `v` switches charwise <-> linewise; typing the form the mode
            // already has ends the selection.
            Key::Char('v') | Key::Char('V') => {
                let line_wise = key == Key::Char('V');
                if self.mode == Mode::Visual && line_wise {
                    self.enter_visual(true, buf);
                } else if self.mode == Mode::VisualLine && !line_wise {
                    self.enter_visual(false, buf);
                } else {
                    self.exit_visual(buf);
                }
                Outcome::Stay
            }
            // `o` jumps to the other end of the selection.
            Key::Char('o') | Key::Char('O') => {
                if let Some((a, h)) = buf.selection() {
                    buf.set_selection(h, a);
                }
                Outcome::Stay
            }
            // `i`/`a` start a text object, which extends the selection.
            Key::Char('i') | Key::Char('a') => {
                let count = self.count.take().unwrap_or(1);
                self.pending = Pending::TextObject {
                    op: None,
                    count,
                    around: key == Key::Char('a'),
                };
                Outcome::Stay
            }
            // `I`/`A` insert at the first non-blank / last character of the
            // selection's near and far lines.
            Key::Char('I') | Key::Char('A') => {
                let Some((lo, hi)) = self.selection_range(buf) else {
                    return Outcome::Unhandled;
                };
                self.mode = Mode::Normal;
                buf.clear_selection();
                if key == Key::Char('I') {
                    buf.set_cursor(lo);
                    buf.row_first_non_blank();
                } else {
                    buf.set_cursor(hi.saturating_sub(1));
                    buf.row_last_char_motion();
                    buf.move_right_stay_on_line();
                }
                Outcome::EnterInsert
            }
            // Operators apply to the selection itself.
            Key::Char('d')
            | Key::Char('x')
            | Key::Char('c')
            | Key::Char('s')
            | Key::Char('y')
            | Key::Char('p') => {
                let Some((lo, hi)) = self.selection_range(buf) else {
                    return Outcome::Unhandled;
                };
                let linewise = self.mode == Mode::VisualLine;
                let text = buf.text()[lo..hi].to_string();
                self.exit_visual(buf);
                match key {
                    Key::Char('y') => self.register = Some(Register { text, linewise }),
                    Key::Char('p') => {
                        buf.delete_range(lo, hi);
                        let reg = self.register.clone().map(|r| r.text).unwrap_or_default();
                        buf.insert_at(lo, &reg);
                    }
                    Key::Char('c') | Key::Char('s') => {
                        self.register = Some(Register { text, linewise });
                        buf.delete_range(lo, hi);
                        return Outcome::EnterInsert;
                    }
                    _ => {
                        self.register = Some(Register { text, linewise });
                        buf.delete_range(lo, hi);
                    }
                }
                Outcome::Stay
            }
            // Everything else is a motion.
            _ => self.visual_motion(key, buf),
        }
    }

    /// Runs a motion while in visual mode. Motions measure from the cursor, so
    /// the selection is cleared first and then re-anchored where it was.
    fn visual_motion(&mut self, key: Key, buf: &mut Buffer) -> Outcome {
        let anchor = buf.selection().map(|(a, _)| a);
        buf.clear_selection();
        let outcome = self.handle_motion_key(key, buf);
        // A text object sets its own selection; a plain motion only moves the
        // head.
        if buf.selection().is_none() {
            if let Some(anchor) = anchor {
                buf.set_selection(anchor, buf.cursor());
            }
        }
        outcome
    }

    /// Enter visual mode: `v` starts the selection at the cursor, `V` takes
    /// whole lines.
    pub(super) fn enter_visual(&mut self, line_wise: bool, buf: &mut Buffer) {
        self.pending = Pending::None;
        self.count = None;
        self.mode = if line_wise {
            Mode::VisualLine
        } else {
            Mode::Visual
        };
        let cursor = buf.cursor();
        if line_wise {
            let row = buf.row_col(cursor).0;
            buf.set_selection(buf.line_start(row), buf.line_end(row));
        } else {
            buf.set_selection(cursor, cursor);
        }
    }

    /// Leave visual mode, leaving the cursor on the selection's far end.
    fn exit_visual(&mut self, buf: &mut Buffer) {
        self.mode = Mode::Normal;
        self.pending = Pending::None;
        self.count = None;
        buf.clear_selection();
    }

    /// The byte range `[lo, hi)` a visual selection covers. Charwise visual is
    /// *inclusive* of the character under the cursor, so the end sits one
    /// character past the head; linewise takes whole lines, line breaks
    /// included.
    pub fn selection_range(&self, buf: &Buffer) -> Option<(usize, usize)> {
        let (anchor, head) = buf.selection()?;
        let (lo, hi) = (anchor.min(head), anchor.max(head));
        if self.mode == Mode::VisualLine {
            let end = buf.line_end(buf.row_col(hi).0);
            let end = if end < buf.text().len() {
                buf.next_char_boundary(end)
            } else {
                end
            };
            Some((buf.line_start(buf.row_col(lo).0), end))
        } else {
            let end = if hi < buf.text().len() {
                buf.next_char_boundary(hi)
            } else {
                buf.text().len()
            };
            Some((lo, end))
        }
    }

    /// After a motion in visual mode the anchor stays put and the head follows
    /// the cursor.
    fn reanchor(&self, buf: &mut Buffer) -> Outcome {
        if let Some((anchor, _)) = buf.selection() {
            buf.set_selection(anchor, buf.cursor());
        }
        Outcome::Stay
    }
}
