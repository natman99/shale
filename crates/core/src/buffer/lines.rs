//! Where the lines are: the row and offset geometry of the text.
//!
//! The buffer is one string; this is what makes it addressable by row, which is
//! the language the vim engine and the paint both work in.

use super::Buffer;

impl Buffer {
    pub fn line_count(&self) -> usize {
        self.text.matches('\n').count() + 1
    }
    /// Byte offset of the first character of `row`.
    pub fn line_start(&self, row: usize) -> usize {
        if row == 0 {
            return 0;
        }
        let mut lines_seen = 0;
        for (idx, b) in self.text.char_indices() {
            if b == '\n' {
                lines_seen += 1;
                if lines_seen == row {
                    return idx + 1;
                }
            }
        }
        self.text.len()
    }
    /// Byte offset just past the last character of `row` (the newline or EOF).
    pub fn line_end(&self, row: usize) -> usize {
        let start = self.line_start(row);
        match self.text[start..].find('\n') {
            Some(len) => start + len,
            None => self.text.len(),
        }
    }
    /// The text of `count` whole lines from `row`, with a trailing newline when
    /// the range does not end at EOF (the linewise yank text).
    pub fn yank_lines(&self, row: usize, count: usize) -> String {
        let last_row = self.line_count() - 1;
        let row = row.min(last_row);
        let last = row.saturating_add(count.saturating_sub(1)).min(last_row);
        let mut text = self.text()[self.line_start(row)..self.line_end(last)].to_string();
        if last < last_row {
            text.push('\n');
        }
        text
    }

    // ----- folding ------------------------------------------------------
    /// Byte range covering `count` whole lines from `row`, including the
    /// trailing newline when there is one (the linewise operator range).
    pub fn line_range(&self, row: usize, count: usize) -> (usize, usize) {
        let last_row = self.line_count() - 1;
        let row = row.min(last_row);
        let last = row.saturating_add(count.saturating_sub(1)).min(last_row);
        let mut start = self.line_start(row);
        let mut end = self.line_end(last);
        if end < self.text.len() {
            end += 1; // include the trailing newline
        } else if start > 0 {
            // Final line of the buffer: take the preceding newline instead.
            start = start.saturating_sub(1);
        }
        (start, end)
    }

    // ----- motions (never modify text) ----------------------------------
    pub fn row_col(&self, offset: usize) -> (usize, usize) {
        let offset = self.clamp(offset);
        let row = self.text[..offset].matches('\n').count();
        let col = offset - self.line_start(row);
        (row, col)
    }
    pub fn offset_of(&self, row: usize, col: usize) -> usize {
        let row = row.min(self.line_count() - 1);
        let start = self.line_start(row);
        let end = self.line_end(row);
        let target = start + col;
        if target >= end {
            end
        } else {
            self.clamp(target)
        }
    }
}
