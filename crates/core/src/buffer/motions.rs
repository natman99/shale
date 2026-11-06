//! Cursor motions, in the terms the vim engine thinks in: sideways, by screen
//! line, by word, and `f`/`t`/`F`/`T`.
//!
//! The vertical motions carry `desired_col` — the column they are aiming for — which
//! is what makes `j`/`k` walk a wrapped paragraph back to the same column instead of
//! hugging the margin. It is latched from the [`ViewSlot`] driving the caret, because a
//! column is a position on a *display* line and a display line belongs to a view: the
//! methods that need one take it as an argument, so a display question can never be
//! answered from another view's wrap points.

use super::view::ViewSlot;
use super::words::{class_of, WordClass};
use super::Buffer;

impl Buffer {
    pub fn move_left(&mut self) {
        let pos = self.selection.map(|(a, _)| a).unwrap_or(self.cursor);
        let prev = self.prev_char_boundary(pos);
        self.set_cursor(prev);
    }
    pub fn move_right(&mut self) {
        let pos = self
            .selection
            .map(|(_, h)| h.max(self.cursor))
            .unwrap_or(self.cursor);
        let next = self.next_char_boundary(pos);
        self.set_cursor(next);
    }
    /// Move right but never past the end of the current line (vim `l`).
    pub fn move_right_stay_on_line(&mut self) {
        let (row, _) = self.row_col(self.cursor);
        let line_end = self.line_end(row);
        if self.cursor < line_end {
            self.set_cursor(self.next_char_boundary(self.cursor));
        }
    }
    pub fn move_up(&mut self, view: &ViewSlot) {
        let index = self.display_index_for(view, self.cursor);
        if index == 0 {
            return;
        }
        self.move_display_to(view, index - 1);
    }
    pub fn move_down(&mut self, view: &ViewSlot) {
        let index = self.display_index_for(view, self.cursor);
        if index + 1 >= self.display_line_count(view) {
            return;
        }
        self.move_display_to(view, index + 1);
    }
    /// Move to display line `index`, keeping the column we are aiming for and
    /// clamping to that line's width (vim's behaviour across soft wraps).
    ///
    /// The aim is latched on the way through the first time it is needed, then
    /// kept: that is what lets `jj` walk a wrapped paragraph back to the column it
    /// started from instead of hugging the margin after the first step lands on a
    /// short row.
    fn move_display_to(&mut self, view: &ViewSlot, index: usize) {
        let Some((start, end)) = self.display_range(view, index) else {
            return;
        };
        let target = match self.desired_col {
            Some(col) => col,
            None => {
                let col = self.display_col(view, self.cursor);
                self.desired_col = Some(col);
                col
            }
        };
        let offset = self.offset_for_char_col(start, end, target);
        self.set_cursor_keep_desired(offset);
    }
    /// Move to the first character of the current *display* line (vim `0`,
    /// which is a screen-line motion when the row wraps).
    pub fn line_start_motion(&mut self, view: &ViewSlot) {
        let start = self.display_start(view, self.cursor);
        self.set_cursor(start);
    }
    /// Move to the first non-blank of the current *display* line (vim `^`).
    pub fn first_non_blank(&mut self, view: &ViewSlot) {
        let start = self.display_start(view, self.cursor);
        let end = self.display_end(view, self.cursor);
        let rel = self.text[start..end]
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(0);
        self.set_cursor(start + rel);
    }
    /// Move to the first non-blank of the whole buffer row, ignoring soft wraps
    /// (the `I` command, which is line-wise in vim).
    pub fn row_first_non_blank(&mut self) {
        let (row, _) = self.row_col(self.cursor);
        let start = self.line_start(row);
        let end = self.line_end(row);
        let rel = self.text[start..end]
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(0);
        self.set_cursor(start + rel);
    }
    pub fn line_end_motion(&mut self) {
        let (row, _) = self.row_col(self.cursor);
        self.set_cursor(self.line_end(row));
    }
    /// Move to the last character of the current *display* line (vim `$`), which
    /// unlike [`Self::line_end_motion`] does not land past the final character.
    pub fn line_last_char_motion(&mut self, view: &ViewSlot) {
        let start = self.display_start(view, self.cursor);
        let end = self.display_end(view, self.cursor);
        if end > start {
            self.set_cursor(self.prev_char_boundary(end));
        } else {
            self.set_cursor(start);
        }
    }
    /// Move to the last character of the whole buffer row, ignoring soft wraps
    /// (the `A` command, which is line-wise in vim).
    pub fn row_last_char_motion(&mut self) {
        let (row, _) = self.row_col(self.cursor);
        let start = self.line_start(row);
        let end = self.line_end(row);
        if end > start {
            self.set_cursor(self.prev_char_boundary(end));
        } else {
            self.set_cursor(start);
        }
    }
    pub fn buffer_start(&mut self) {
        self.set_cursor(0);
    }
    /// Move to the first character of `row` (0-based, clamped).
    pub fn goto_line(&mut self, row: usize) {
        let row = row.min(self.line_count() - 1);
        self.set_cursor(self.line_start(row));
    }
    pub fn buffer_end(&mut self) {
        self.set_cursor(self.text.len());
    }
    /// Move to the first non-blank of the next line (vim Enter).
    pub fn next_line_first_non_blank(&mut self, view: &ViewSlot) {
        let (row, _) = self.row_col(self.cursor);
        if row + 1 < self.line_count() {
            self.set_cursor(self.line_start(row + 1));
            self.first_non_blank(view);
        }
    }
    /// Move to the start of the next word (vim `w`).
    ///
    /// Words are runs of one of three classes: keyword characters
    /// (alphanumeric + `_`), punctuation runs, and whitespace. `w` consumes the
    /// current run (unless it is whitespace) and then any whitespace, landing on
    /// the start of the next run — crossing line boundaries like vim does.
    /// Vim's `w`: an exclusive motion to the start of the next word, skipping
    /// the gap after the current one. `e` is the inclusive form.
    pub fn word_forward(&mut self) {
        let len = self.text.len();
        let mut i = self.clamp(self.cursor);
        if i >= len {
            return;
        }
        let cls = class_of(self.char_at(i).unwrap());
        if cls != WordClass::Whitespace && cls != WordClass::Newline {
            i = self.skip_run(i, cls);
        }
        i = self.skip_blanks(i);
        self.set_cursor(i);
    }
    /// Move to the start of the next WORD (vim `W`).
    ///
    /// A WORD is delimited only by blanks, so `[[link]]`, `**bold**` and
    /// `code.rs` are each one WORD. The line break is a blank, so a WORD never
    /// crosses it — but `W` at the end of a line skips to the first WORD of the
    /// next, as vim's does.
    pub fn big_word_forward(&mut self) {
        let len = self.text.len();
        let mut i = self.clamp(self.cursor);
        if i >= len {
            return;
        }
        if !self.char_at(i).unwrap().is_whitespace() {
            i = self.skip_word_run(i);
        }
        i = self.skip_blanks(i);
        self.set_cursor(i);
    }
    /// Move to the end of the current/next WORD (vim `E`), the inclusive form of
    /// `W`: it lands *on* the WORD's last character.
    pub fn big_word_end(&mut self) {
        let len = self.text.len();
        if len == 0 {
            return;
        }
        let i = self.cursor.min(len - 1);
        let here = self.char_at(i).unwrap();
        let run_end = self.skip_word_run(i);
        // Strictly inside a WORD: finish that WORD.
        if !here.is_whitespace() && run_end > i + here.len_utf8() {
            self.set_cursor(self.prev_char_boundary(run_end));
            return;
        }
        // Otherwise step to the next WORD and take its last character. With no
        // WORD left the cursor stays where it is: vim's `E` at the end of the last
        // word does not step onto the line break.
        let j = self.skip_blanks(run_end);
        if j >= len {
            let last = self.prev_char_boundary(len);
            self.set_cursor(self.cursor.min(last));
            return;
        }
        let end = self.skip_word_run(j);
        self.set_cursor(self.prev_char_boundary(end));
    }
    /// Move to the start of the previous word (vim `b`).
    ///
    /// The boundary itself is [`word_back`], which the buffer shares with the
    /// one-line fields so a "word" means the same thing in a note and in a query.
    pub fn word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let to = super::word_back(&self.text, self.cursor);
        self.set_cursor(to);
    }
    /// Move to the start of the previous WORD (vim `B`).
    pub fn big_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let to = super::big_word_back(&self.text, self.cursor);
        self.set_cursor(to);
    }
    /// Move to the end of the current/next word (vim `e`).
    pub fn word_end(&mut self) {
        let len = self.text.len();
        if len == 0 {
            return;
        }
        let i = self.cursor.min(len - 1);
        let cls = class_of(self.char_at(i).unwrap());
        let run_end = self.skip_run(i, cls);
        // Strictly inside a keyword/punctuation run: finish that run.
        let here = self.char_at(i).unwrap().len_utf8();
        if matches!(cls, WordClass::Word | WordClass::Punct) && run_end > i + here {
            self.set_cursor(self.prev_char_boundary(run_end));
            return;
        }
        // Otherwise step to the next non-blank run and take its last char. With
        // none left the cursor stays put rather than stepping onto the break that
        // ends the buffer — vim's `e` on the last word of a note stays on it.
        let j = self.skip_blanks(run_end);
        if j >= len {
            let last = self.prev_char_boundary(len);
            self.set_cursor(self.cursor.min(last));
            return;
        }
        let end = self.skip_run(j, class_of(self.char_at(j).unwrap()));
        self.set_cursor(self.prev_char_boundary(end));
    }
    /// Move to the `count`-th occurrence of `c` on the current row: vim's `f`
    /// (forward, lands on it), `t` (forward, stops just before it), `F` and `T`
    /// (backwards). A character that is not on the rest of the row leaves the
    /// cursor where it is, as in vim.
    pub fn find_char(&mut self, c: char, forward: bool, inclusive: bool, count: usize) {
        let (row, _) = self.row_col(self.cursor);
        let row_start = self.line_start(row);
        let row_end = self.line_end(row);
        let mut remaining = count.max(1);
        let mut found = None;

        if forward {
            let mut i = self.next_char_boundary(self.cursor);
            while i < row_end {
                if self.text[i..].starts_with(c) {
                    remaining -= 1;
                    if remaining == 0 {
                        found = Some(i);
                        break;
                    }
                }
                i = self.next_char_boundary(i);
            }
        } else {
            let mut i = self.prev_char_boundary(self.cursor);
            loop {
                if i < row_start || i >= self.cursor {
                    break;
                }
                if self.text[i..].starts_with(c) {
                    remaining -= 1;
                    if remaining == 0 {
                        found = Some(i);
                        break;
                    }
                }
                if i == row_start {
                    break;
                }
                i = self.prev_char_boundary(i);
            }
        }

        let Some(found) = found else { return };
        let target = match (forward, inclusive) {
            (true, true) | (false, true) => found,
            // `t` stops on the character before the match, `T` just after it.
            (true, false) => self.prev_char_boundary(found),
            (false, false) => self.next_char_boundary(found),
        };
        self.set_cursor(target);
    }
}
