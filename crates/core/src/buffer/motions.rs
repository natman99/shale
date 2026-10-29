//! Cursor motions, in the terms the vim engine thinks in: sideways, by screen
//! line, by word, and `f`/`t`/`F`/`T`.
//!
//! Every motion leaves `desired_col` behind, which is what makes `j`/`k` walk a
//! wrapped paragraph back to the same column instead of hugging the margin.

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
    pub fn move_up(&mut self) {
        let index = self.display_index_for(self.cursor);
        if index == 0 {
            return;
        }
        self.move_display_to(index - 1);
    }
    pub fn move_down(&mut self) {
        let index = self.display_index_for(self.cursor);
        if index + 1 >= self.display_line_count() {
            return;
        }
        self.move_display_to(index + 1);
    }
    /// Move to display line `index`, keeping the column we are aiming for and
    /// clamping to that line's width (vim's behaviour across soft wraps).
    fn move_display_to(&mut self, index: usize) {
        let Some((start, end)) = self.display_range(index) else {
            return;
        };
        let target = self
            .desired_col
            .unwrap_or_else(|| self.display_col(self.cursor));
        let offset = self.offset_for_char_col(start, end, target);
        self.set_cursor_keep_desired(offset);
    }
    /// Move to the first character of the current *display* line (vim `0`,
    /// which is a screen-line motion when the row wraps).
    pub fn line_start_motion(&mut self) {
        let start = self.display_start(self.cursor);
        self.set_cursor(start);
    }
    /// Move to the first non-blank of the current *display* line (vim `^`).
    pub fn first_non_blank(&mut self) {
        let start = self.display_start(self.cursor);
        let end = self.display_end(self.cursor);
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
    pub fn line_last_char_motion(&mut self) {
        let start = self.display_start(self.cursor);
        let end = self.display_end(self.cursor);
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
    pub fn next_line_first_non_blank(&mut self) {
        let (row, _) = self.row_col(self.cursor);
        if row + 1 < self.line_count() {
            self.set_cursor(self.line_start(row + 1));
            self.first_non_blank();
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
        // Otherwise step to the next non-blank run and take its last char.
        let j = self.skip_blanks(run_end);
        if j >= len {
            self.set_cursor(self.prev_char_boundary(len));
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
