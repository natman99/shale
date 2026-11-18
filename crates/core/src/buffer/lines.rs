//! Where the lines are: the row and offset geometry of the text.
//!
//! The buffer is one string; this is what makes it addressable by row, which is
//! the language the vim engine and the paint both work in.

use super::Buffer;

impl Buffer {
    /// Refresh the line-start table when the text changed since the last
    /// build. The change id is the whole invalidation rule: every path
    /// that mutates `text` (edits, undo, redo, `set_text`) moves it.
    fn refresh_line_index(&self) {
        let stale = self
            .line_index
            .borrow()
            .as_ref()
            .is_none_or(|(id, _)| *id != self.change_id);
        if stale {
            let mut starts = Vec::with_capacity(self.text.matches('\n').count() + 1);
            starts.push(0usize);
            // `\n` is one byte and never appears inside a multi-byte
            // character, so a byte scan finds every line boundary.
            for (i, b) in self.text.bytes().enumerate() {
                if b == b'\n' {
                    starts.push(i + 1);
                }
            }
            *self.line_index.borrow_mut() = Some((self.change_id, starts));
        }
    }
    /// Answer from the current line-start table.
    fn with_line_starts<T>(&self, f: impl FnOnce(&[usize]) -> T) -> T {
        self.refresh_line_index();
        let cell = self.line_index.borrow();
        f(&cell.as_ref().expect("refresh_line_index filled it").1)
    }
    pub fn line_count(&self) -> usize {
        self.with_line_starts(|starts| starts.len())
    }
    /// Byte offset of the first character of `row`; a row past the end
    /// answers at the end of the text, as the scan did.
    pub fn line_start(&self, row: usize) -> usize {
        self.with_line_starts(|starts| *starts.get(row).unwrap_or(&self.text.len()))
    }
    /// Byte offset just past the last character of `row` (the newline or EOF).
    pub fn line_end(&self, row: usize) -> usize {
        self.with_line_starts(|starts| match starts.get(row + 1) {
            // The next row starts one byte past this row's newline.
            Some(&next) => next - 1,
            None => self.text.len(),
        })
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
