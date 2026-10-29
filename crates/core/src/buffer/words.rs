//! Words and paragraphs: the ranges the text objects take.
//!
//! The character classes live with the motions that step over them, so `w` and
//! `iw` cannot disagree about what a word is.

use super::Buffer;

impl Buffer {
    /// The byte range of the class run (word / punctuation / whitespace) that
    /// contains `i`. Runs never cross a newline, since `\n` is its own class.
    pub(super) fn run_of(&self, i: usize) -> (usize, usize) {
        let Some(cls) = self.char_at(i).map(class_of) else {
            return (i, i);
        };
        let end = self.skip_run(i, cls);
        let mut start = i;
        while start > 0 && self.prev_char(start).map(class_of) == Some(cls) {
            start = self.prev_char_boundary(start);
        }
        (start, end)
    }
    /// The word text object under `offset` (vim `iw` / `aw`). `inner` is the run
    /// itself; `around` also takes the surrounding whitespace, preferring the
    /// trailing run and falling back to the leading one, the way `aw` does. On
    /// whitespace, `around` takes that whitespace plus the word after it.
    pub fn word_object(&self, offset: usize, inner: bool) -> (usize, usize) {
        let offset = self.clamp(offset);
        let (row, _) = self.row_col(offset);
        let row_end = self.line_end(row);
        if offset >= row_end {
            // Sitting on the line break: the break itself is the object.
            return (offset, self.next_char_boundary(offset).min(offset + 1));
        }
        let (start, end) = self.run_of(offset);
        if inner {
            return (start, end);
        }
        let cls = class_of(self.char_at(offset).unwrap());
        if cls == WordClass::Whitespace {
            let after = self
                .skip_run(end, WordClass::Word)
                .max(self.skip_run(end, WordClass::Punct));
            let after = if after > end { after } else { end };
            return (start, after.max(end));
        }
        // Trailing whitespace, if the run is followed by any.
        let ws_end = self.skip_run(end, WordClass::Whitespace);
        if ws_end > end {
            return (start, ws_end);
        }
        // Otherwise leading whitespace.
        let mut ws_start = start;
        while ws_start > 0 && self.prev_char(ws_start).map(class_of) == Some(WordClass::Whitespace)
        {
            ws_start = self.prev_char_boundary(ws_start);
        }
        (ws_start, end)
    }
    /// The paragraph text object containing `offset` (vim `ip` / `ap`): a
    /// maximal run of non-blank lines. `around` adds the following blank lines
    /// (or the preceding ones when there are none after).
    pub fn paragraph_object(&self, offset: usize, inner: bool) -> (usize, usize) {
        let (row, _) = self.row_col(offset);
        let last_row = self.line_count() - 1;
        let blank = |r: usize| {
            self.text[self.line_start(r)..self.line_end(r)]
                .trim()
                .is_empty()
        };
        let on_blank = blank(row);

        let mut first = row;
        while first > 0 && blank(first - 1) == on_blank {
            first -= 1;
        }
        let mut last = row;
        while last < last_row && blank(last + 1) == on_blank {
            last += 1;
        }

        let include_break = |end: usize| if end < self.text.len() { end + 1 } else { end };
        let mut start = self.line_start(first);
        let mut end = if on_blank {
            include_break(self.line_end(last))
        } else {
            self.line_end(last)
        };
        if inner || on_blank {
            return (start, end);
        }

        // `ap`: swallow the blank lines after the paragraph, including the break
        // that ends it.
        let mut next = last + 1;
        let plain_end = end;
        end = include_break(end);
        while next <= last_row && blank(next) {
            end = include_break(self.line_end(next));
            next += 1;
        }
        if end == plain_end {
            // Nothing after: take the blank lines before it instead.
            let mut prev = first;
            while prev > 0 && blank(prev - 1) {
                prev -= 1;
            }
            if prev < first {
                start = self.line_start(prev);
            } else {
                start = start.saturating_sub(1); // the break that ends the previous paragraph
            }
        }
        (start, end)
    }
    pub(super) fn char_at(&self, i: usize) -> Option<char> {
        self.text[i..].chars().next()
    }
    /// Advance past a run of `cls` starting at `i`.
    pub(super) fn skip_run(&self, mut i: usize, cls: WordClass) -> usize {
        let len = self.text.len();
        while i < len {
            match self.char_at(i) {
                Some(c) if class_of(c) == cls => i += c.len_utf8(),
                _ => break,
            }
        }
        i
    }
    /// Advance past any run of blanks (spaces/tabs and newlines).
    pub(super) fn skip_blanks(&self, mut i: usize) -> usize {
        let len = self.text.len();
        while i < len {
            match self.char_at(i).map(class_of) {
                Some(WordClass::Whitespace) => i = self.skip_run(i, WordClass::Whitespace),
                Some(WordClass::Newline) => i = self.skip_run(i, WordClass::Newline),
                _ => break,
            }
        }
        i
    }
    pub(crate) fn prev_char_boundary(&self, offset: usize) -> usize {
        let mut o = self.clamp(offset).saturating_sub(1);
        while o > 0 && !self.text.is_char_boundary(o) {
            o -= 1;
        }
        o
    }
    pub(crate) fn next_char_boundary(&self, offset: usize) -> usize {
        let mut o = self.clamp(offset) + 1;
        while o < self.text.len() && !self.text.is_char_boundary(o) {
            o += 1;
        }
        o.min(self.text.len())
    }
    pub(super) fn prev_char(&self, offset: usize) -> Option<char> {
        let start = self.prev_char_boundary(offset);
        self.text[start..offset].chars().next()
    }

    // ----- edits (each pushes one undo snapshot) ------------------------
}

/// The offset vim's `b` motion lands on coming back from `at`: the start of the
/// word before the cursor, crossing blanks first.
///
/// This is the one definition of "the word before an offset". The buffer's
/// backward word motion, insert-mode `ctrl-backspace` and the one-line fields'
/// delete-word all ask it, so a word means the same thing everywhere text is
/// edited — and it is a free function over `&str` so the fields, which hold a
/// `SharedString` rather than a [`Buffer`](super::Buffer), can ask it too.
pub fn word_back(text: &str, at: usize) -> usize {
    if at == 0 {
        return 0;
    }
    let len = text.len();
    let at = at.min(len);
    // The character just before `i`, and the byte offset of its start. Walking
    // back to a char boundary keeps this safe on text whose offsets came from
    // somewhere else.
    let start_of_prev = |i: usize| {
        let mut s = i.saturating_sub(1);
        while s > 0 && !text.is_char_boundary(s) {
            s -= 1;
        }
        s
    };
    let prev_class = |i: usize| text[start_of_prev(i)..i].chars().next().map(class_of);

    let mut i = at;
    // In the middle of a run: back up to that run's start.
    if i < len {
        if let Some(cls) = text[i..].chars().next().map(class_of) {
            if cls != WordClass::Whitespace && cls != WordClass::Newline {
                while i > 0 && prev_class(i) == Some(cls) {
                    i = start_of_prev(i);
                }
                if i < at {
                    return i;
                }
            }
        }
    }
    // At a run start (or on a blank): cross blanks, then back over the previous
    // run.
    while i > 0 {
        match prev_class(i) {
            Some(WordClass::Whitespace) | Some(WordClass::Newline) => i = start_of_prev(i),
            _ => break,
        }
    }
    if i == 0 {
        return 0;
    }
    if let Some(cls) = prev_class(i) {
        while i > 0 && prev_class(i) == Some(cls) {
            i = start_of_prev(i);
        }
    }
    i
}

/// Vim's word classes: keyword runs, punctuation runs, whitespace, and
/// newlines (tracked separately so motions can reason about line ends).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WordClass {
    Newline,
    Whitespace,
    Word,
    Punct,
}

pub(super) fn class_of(c: char) -> WordClass {
    if c == '\n' {
        WordClass::Newline
    } else if c.is_whitespace() {
        WordClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        WordClass::Word
    } else {
        WordClass::Punct
    }
}
