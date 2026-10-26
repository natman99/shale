//! A small text buffer with cursor/selection and undo, designed for
//! markdown notes (whole-file `String` storage is fine at this scale).
//! whole-file `String` storage is fine at this scale).

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    text: String,
    cursor: usize,
    selection: Option<(usize, usize)>,
}

/// Byte offsets are always clamped to character boundaries.
#[derive(Debug, Clone)]
pub struct Buffer {
    text: String,
    /// Cursor byte offset (the head of the selection, if any).
    cursor: usize,
    /// Active selection as (anchor, head); cursor == head.
    selection: Option<(usize, usize)>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl Buffer {
    pub fn new(text: String) -> Self {
        Self {
            text,
            cursor: 0,
            selection: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: String) {
        self.undo.clear();
        self.redo.clear();
        self.text = text;
        self.cursor = 0;
        self.selection = None;
        self.cursor = self.clamp(self.cursor);
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn selection(&self) -> Option<(usize, usize)> {
        self.selection
    }

    pub fn set_cursor(&mut self, offset: usize) {
        self.cursor = self.clamp(offset);
        self.selection = None;
    }

    pub fn set_selection(&mut self, anchor: usize, head: usize) {
        let anchor = self.clamp(anchor);
        let head = self.clamp(head);
        self.selection = Some((anchor, head));
        self.cursor = head;
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Snap an offset to the nearest valid char boundary within the text.
    fn clamp(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        if self.text.is_char_boundary(offset) {
            offset
        } else {
            let mut o = offset;
            while o > 0 && !self.text.is_char_boundary(o) {
                o -= 1;
            }
            o
        }
    }

    // ----- line helpers -------------------------------------------------

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

    // ----- motions (never modify text) ----------------------------------

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
        let (row, col) = self.row_col(self.cursor);
        if row > 0 {
            self.set_cursor(self.offset_of(row - 1, col));
        }
    }

    pub fn move_down(&mut self) {
        let (row, col) = self.row_col(self.cursor);
        if row + 1 < self.line_count() {
            self.set_cursor(self.offset_of(row + 1, col));
        }
    }

    pub fn line_start_motion(&mut self) {
        let (row, _) = self.row_col(self.cursor);
        self.set_cursor(self.line_start(row));
    }

    pub fn first_non_blank(&mut self) {
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

    pub fn buffer_start(&mut self) {
        self.set_cursor(0);
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
    pub fn word_forward(&mut self) {
        let mut i = self.cursor;
        let len = self.text.len();
        // Advance past the current word (or newline), then past whitespace.
        i = self.skip_word(i);
        while i < len {
            let c = self.text[i..].chars().next().unwrap();
            if c == '\n' {
                i += 1;
                break;
            }
            if !c.is_whitespace() {
                break;
            }
            i += c.len_utf8();
        }
        self.set_cursor(i);
    }

    /// Move to the start of the previous word (vim `b`).
    pub fn word_backward(&mut self) {
        let mut i = self.cursor;
        if i > 0 {
            i = self.prev_char_boundary(i);
        }
        // Back over whitespace (crossing at most one newline).
        let mut crossed_newline = false;
        while i > 0 {
            let c = self.prev_char(i).unwrap();
            if c == '\n' {
                if crossed_newline {
                    break;
                }
                crossed_newline = true;
            } else if !c.is_whitespace() {
                break;
            }
            i = self.prev_char_boundary(i);
        }
        // Back over the word.
        while i > 0 {
            let c = self.prev_char(i).unwrap();
            if !(c.is_alphanumeric() || c == '_') {
                break;
            }
            i = self.prev_char_boundary(i);
        }
        self.set_cursor(i);
    }

    /// Move to the end of the current/next word (vim `e`).
    pub fn word_end(&mut self) {
        let len = self.text.len();
        let mut i = self.cursor;
        // Step at least one char forward.
        if i < len {
            i = self.next_char_boundary(i);
        }
        // Past whitespace.
        while i < len {
            let c = self.text[i..].chars().next().unwrap();
            if c == '\n' {
                i = self.next_char_boundary(i);
                break;
            }
            if !c.is_whitespace() {
                break;
            }
            i += c.len_utf8();
        }
        // To the last char of the word.
        let mut last = i;
        while i < len {
            let c = self.text[i..].chars().next().unwrap();
            if !(c.is_alphanumeric() || c == '_') {
                break;
            }
            last = i;
            i += c.len_utf8();
        }
        self.set_cursor(last);
    }

    fn skip_word(&self, mut i: usize) -> usize {
        let len = self.text.len();
        if i < len {
            let c = self.text[i..].chars().next().unwrap();
            if c == '\n' {
                return i + 1;
            }
            if c.is_whitespace() {
                while i < len {
                    let c = self.text[i..].chars().next().unwrap();
                    if c == '\n' || !c.is_whitespace() {
                        break;
                    }
                    i += c.len_utf8();
                }
                return i;
            }
            while i < len {
                let c = self.text[i..].chars().next().unwrap();
                if !(c.is_alphanumeric() || c == '_') {
                    break;
                }
                i += c.len_utf8();
            }
        }
        i
    }

    fn prev_char_boundary(&self, offset: usize) -> usize {
        let mut o = self.clamp(offset).saturating_sub(1);
        while o > 0 && !self.text.is_char_boundary(o) {
            o -= 1;
        }
        o
    }

    fn next_char_boundary(&self, offset: usize) -> usize {
        let mut o = self.clamp(offset) + 1;
        while o < self.text.len() && !self.text.is_char_boundary(o) {
            o += 1;
        }
        o.min(self.text.len())
    }

    fn prev_char(&self, offset: usize) -> Option<char> {
        let start = self.prev_char_boundary(offset);
        self.text[start..offset].chars().next()
    }

    // ----- edits (each pushes one undo snapshot) ------------------------

    fn snapshot(&mut self) {
        self.undo.push(Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            selection: self.selection,
        });
        self.redo.clear();
    }

    /// Replace the selection (or insert at the cursor) with `text`.
    pub fn insert(&mut self, text: &str) {
        self.snapshot();
        let (start, end) = self.selection_range();
        self.text.replace_range(start..end, text);
        let new_cursor = start + text.len();
        self.selection = None;
        self.cursor = new_cursor;
    }

    pub fn delete_selected(&mut self) {
        if self.selection.is_none() {
            return;
        }
        self.snapshot();
        let (start, end) = self.selection_range();
        self.text.replace_range(start..end, "");
        self.selection = None;
        self.cursor = start;
    }

    /// Delete backwards from the cursor (or the selection, if any).
    pub fn backspace(&mut self) {
        if self.selection.is_some() {
            self.delete_selected();
            return;
        }
        if self.cursor == 0 {
            return;
        }
        self.snapshot();
        let prev = self.prev_char_boundary(self.cursor);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    /// Delete the character after the cursor (or the selection, if any).
    pub fn delete_forward(&mut self) {
        if self.selection.is_some() {
            self.delete_selected();
            return;
        }
        let end = self.next_char_boundary(self.cursor);
        if end == self.cursor {
            return;
        }
        self.snapshot();
        self.text.replace_range(self.cursor..end, "");
    }

    /// Delete the character at the cursor (vim `x`).
    pub fn delete_char(&mut self) {
        let end = self.next_char_boundary(self.cursor);
        if end == self.cursor {
            return;
        }
        self.snapshot();
        self.text.replace_range(self.cursor..end, "");
        self.selection = None;
        self.cursor = self.clamp(self.cursor);
    }

    /// Delete the whole line containing the cursor, including its newline
    /// (vim `dd`).
    pub fn delete_line(&mut self) {
        self.snapshot();
        let (row, _) = self.row_col(self.cursor);
        let mut start = self.line_start(row);
        let mut end = self.line_end(row);
        if end < self.text.len() {
            end += 1; // include the trailing newline
        } else if start > 0 {
            // Last line: take the preceding newline instead.
            start -= 1;
        }
        self.text.replace_range(start..end, "");
        self.cursor = self.clamp(start.min(self.text.len()));
        self.selection = None;
    }

    /// Open a new line below the cursor and place the cursor on it (vim `o`).
    pub fn open_line_below(&mut self) {
        self.snapshot();
        let (row, _) = self.row_col(self.cursor);
        let end = self.line_end(row);
        self.text.insert(end, '\n');
        self.cursor = end + 1;
        self.selection = None;
    }

    /// Open a new line above the cursor (vim `O`).
    pub fn open_line_above(&mut self) {
        self.snapshot();
        let (row, _) = self.row_col(self.cursor);
        let start = self.line_start(row);
        self.text.insert(start, '\n');
        self.cursor = start;
        self.selection = None;
    }

    fn selection_range(&self) -> (usize, usize) {
        match self.selection {
            Some((a, h)) => (a.min(h), a.max(h)),
            None => (self.cursor, self.cursor),
        }
    }

    pub fn undo(&mut self) {
        if let Some(snap) = self.undo.pop() {
            self.redo.push(Snapshot {
                text: self.text.clone(),
                cursor: self.cursor,
                selection: self.selection,
            });
            self.text = snap.text;
            self.cursor = self.clamp(snap.cursor);
            self.selection = snap.selection;
        }
    }

    pub fn redo(&mut self) {
        if let Some(snap) = self.redo.pop() {
            self.undo.push(Snapshot {
                text: self.text.clone(),
                cursor: self.cursor,
                selection: self.selection,
            });
            self.text = snap.text;
            self.cursor = self.clamp(snap.cursor);
            self.selection = snap.selection;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf() -> Buffer {
        Buffer::new("hello world\nsecond line\nthird".to_string())
    }

    #[test]
    fn line_bounds() {
        let b = buf();
        assert_eq!(b.line_count(), 3);
        assert_eq!(b.line_start(1), 12);
        assert_eq!(b.line_end(1), 23);
        assert_eq!(b.line_end(2), 29);
    }

    #[test]
    fn row_col_roundtrip() {
        let b = buf();
        assert_eq!(b.row_col(12), (1, 0));
        assert_eq!(b.offset_of(1, 6), 18);
        // Column beyond line end clamps to line end.
        assert_eq!(b.offset_of(0, 100), 11);
    }

    #[test]
    fn word_motions() {
        let mut b = Buffer::new("foo bar  baz\nnext".to_string());
        b.word_forward(); // at 'f' -> start of "bar"
        assert_eq!(b.cursor(), 4);
        b.word_forward(); // -> start of "baz"
        assert_eq!(b.cursor(), 9);
        b.word_forward(); // -> start of "next"
        assert_eq!(b.cursor(), 13);
        b.word_backward(); // -> start of "baz"
        assert_eq!(b.cursor(), 9);

        let mut b2 = Buffer::new("hello world".to_string());
        b2.word_end();
        assert_eq!(b2.cursor(), 4); // 'o' of hello
        b2.word_end();
        assert_eq!(b2.cursor(), 10); // 'd' of world
    }

    #[test]
    fn line_motions() {
        let mut b = buf();
        b.set_cursor(15);
        b.line_start_motion();
        assert_eq!(b.cursor(), 12);
        b.line_end_motion();
        assert_eq!(b.cursor(), 23);
        b.move_up();
        assert_eq!(b.row_col(b.cursor()), (0, 11));
        b.first_non_blank();
        assert_eq!(b.cursor(), 0);
    }

    #[test]
    fn enter_motion() {
        let mut b = Buffer::new("aaa\n  indented\nzzz".to_string());
        b.set_cursor(0);
        b.next_line_first_non_blank();
        assert_eq!(b.cursor(), 6); // 'i' of "indented"
        b.next_line_first_non_blank();
        assert_eq!(b.cursor(), 15); // 'z' of "zzz"
        b.next_line_first_non_blank(); // stays at last line
        assert_eq!(b.cursor(), 15);
    }

    #[test]
    fn insert_and_undo_redo() {
        let mut b = Buffer::new("ab".to_string());
        b.set_cursor(1);
        b.insert("X");
        assert_eq!(b.text(), "aXb");
        assert_eq!(b.cursor(), 2);
        b.undo();
        assert_eq!(b.text(), "ab");
        assert_eq!(b.cursor(), 1);
        b.redo();
        assert_eq!(b.text(), "aXb");
    }

    #[test]
    fn delete_line_middle_and_last() {
        let mut b = buf();
        b.set_cursor(13);
        b.delete_line();
        assert_eq!(b.text(), "hello world\nthird");

        let mut b2 = buf();
        b2.set_cursor(25);
        b2.delete_line();
        assert_eq!(b2.text(), "hello world\nsecond line");
    }

    #[test]
    fn open_lines() {
        let mut b = buf();
        b.set_cursor(5);
        b.open_line_below();
        assert_eq!(b.text(), "hello world\n\nsecond line\nthird");
        assert_eq!(b.cursor(), 12);

        let mut b2 = buf();
        b2.set_cursor(13);
        b2.open_line_above();
        assert_eq!(&b2.text()[11..13], "\n\n");
        assert_eq!(b2.cursor(), 12);
    }

    #[test]
    fn delete_char_and_selection() {
        let mut b = Buffer::new("héllo".to_string());
        b.set_cursor(1); // on the é (2 bytes)
        b.delete_char();
        assert_eq!(b.text(), "hllo");

        let mut b2 = Buffer::new("hello".to_string());
        b2.set_selection(0, 3);
        b2.delete_selected();
        assert_eq!(b2.text(), "lo");
        assert_eq!(b2.cursor(), 0);
    }

    #[test]
    fn cursor_clamps_to_char_boundaries() {
        let mut b = Buffer::new("héllo".to_string());
        b.set_cursor(2); // mid-char: é starts at 1, ends at 3
        assert_eq!(b.cursor(), 1);
        b.set_cursor(999);
        assert_eq!(b.cursor(), 6);
    }
}
