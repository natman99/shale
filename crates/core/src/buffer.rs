//! A small text buffer with cursor/selection and undo, designed for
//! markdown notes (whole-file `String` storage is fine at this scale).

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    text: String,
    cursor: usize,
    selection: Option<(usize, usize)>,
}

/// Vim's word classes: keyword runs, punctuation runs, whitespace, and
/// newlines (tracked separately so motions can reason about line ends).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Newline,
    Whitespace,
    Word,
    Punct,
}

fn class_of(c: char) -> WordClass {
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
    /// Character column the cursor is aiming for on vertical motions, so moving
    /// down onto a short line and back restores the original column (vim).
    /// Counted in characters (not bytes) from the start of the current *display*
    /// line, which is the buffer row when nothing wraps: gpui gives one cell to
    /// every character in our monospace font, so a character column is the
    /// column the user sees, while a byte column is not.
    desired_col: Option<usize>,
    /// Per buffer row, the absolute byte offsets where wrapped display lines
    /// begin (supplied by the UI, which knows the pixel width). Empty means the
    /// row is a single display line.
    display_segments: Vec<Vec<usize>>,
    /// True while insert-mode edits are being coalesced into one undo step.
    group_open: bool,
    /// Bumped on every mutation, so the UI can cache per-text computations
    /// (like soft-wrap boundaries) across frames.
    change_id: u64,
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
            desired_col: None,
            display_segments: Vec::new(),
            group_open: false,
            change_id: 0,
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
        self.desired_col = None;
        self.display_segments = Vec::new();
        self.group_open = false;
        self.change_id += 1;
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
        self.desired_col = Some(self.display_col(self.cursor));
    }

    /// Move the cursor without disturbing the vertical target column, for
    /// motions that clamp against a shorter line.
    fn set_cursor_keep_desired(&mut self, offset: usize) {
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

    // ----- display lines (soft wrap) ------------------------------------

    /// Record where the UI wrapped each row. `segments[row]` holds the absolute
    /// byte offsets that begin each display line of that row (the first is the
    /// row's own start). A row the UI does not draw at all — a collapsed fence
    /// line, a table's delimiter row — gets an empty vec and so contributes no
    /// display line; a row with no entry (the text changed since the last frame)
    /// counts as one unwrapped line.
    pub fn set_display_segments(&mut self, segments: Vec<Vec<usize>>) {
        self.display_segments = segments;
    }

    /// The absolute offsets at which row `row`'s display lines begin. Empty when
    /// the row is collapsed: it has no display line, so the vertical motions step
    /// over it and an offset inside it resolves to the line that follows.
    fn segment_starts(&self, row: usize) -> Vec<usize> {
        self.display_segments
            .get(row)
            .cloned()
            .unwrap_or_else(|| vec![self.line_start(row)])
    }

    /// The byte offset where the display line containing `offset` begins.
    pub fn display_start(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        let (row, _) = self.row_col(offset);
        let fallback = self.line_start(row);
        *self
            .segment_starts(row)
            .iter()
            .rev()
            .find(|&&s| s <= offset)
            .unwrap_or(&fallback)
    }

    /// The byte range `[start, end)` of the display line at flat index `index`.
    pub fn display_range(&self, index: usize) -> Option<(usize, usize)> {
        let mut i = 0usize;
        for row in 0..self.line_count() {
            let segs = self.segment_starts(row);
            let line_end = self.line_end(row);
            for (k, start) in segs.iter().enumerate() {
                let end = segs.get(k + 1).copied().unwrap_or(line_end);
                if i == index {
                    return Some((*start, end));
                }
                i += 1;
            }
        }
        None
    }

    /// Total number of display lines, wrapped rows included.
    pub fn display_line_count(&self) -> usize {
        (0..self.line_count())
            .map(|row| self.segment_starts(row).len())
            .sum()
    }

    /// Flat display-line index containing `offset`.
    pub fn display_index_for(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        let (row, _) = self.row_col(offset);
        let mut index = 0usize;
        for r in 0..row {
            index += self.segment_starts(r).len();
        }
        let segs = self.segment_starts(row);
        let within = segs
            .iter()
            .filter(|&&s| s <= offset)
            .count()
            .saturating_sub(1);
        index + within
    }

    /// The byte offset just past the end of the display line containing
    /// `offset` (the next segment start, or the end of the buffer row).
    pub fn display_end(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        let (row, _) = self.row_col(offset);
        let start = self.display_start(offset);
        self.segment_starts(row)
            .iter()
            .filter(|&&s| s > start)
            .copied()
            .next()
            .unwrap_or_else(|| self.line_end(row))
    }

    /// The character column of `offset` within its display line.
    pub fn display_col(&self, offset: usize) -> usize {
        let offset = self.clamp(offset);
        let start = self.display_start(offset);
        self.text[start..offset].chars().count()
    }

    /// The byte offset `col` characters into `[start, end)`, clamped to `end`.
    fn offset_for_char_col(&self, start: usize, end: usize, col: usize) -> usize {
        let mut offset = self.clamp(start).min(end);
        for _ in 0..col {
            if offset >= end {
                break;
            }
            offset = self.next_char_boundary(offset);
        }
        offset.min(end)
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
    pub fn word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let len = self.text.len();
        let mut i = self.cursor;

        // In the middle of a run: back up to that run's start.
        if i < len {
            if let Some(cls) = self.char_at(i).map(class_of) {
                if cls != WordClass::Whitespace && cls != WordClass::Newline {
                    while i > 0 && self.prev_char(i).map(class_of) == Some(cls) {
                        i = self.prev_char_boundary(i);
                    }
                    if i < self.cursor {
                        self.set_cursor(i);
                        return;
                    }
                }
            }
        }

        // At a run start (or on a blank): cross blanks, then back over the
        // previous run.
        while i > 0 {
            match self.prev_char(i).map(class_of) {
                Some(WordClass::Whitespace) | Some(WordClass::Newline) => {
                    i = self.prev_char_boundary(i);
                }
                _ => break,
            }
        }
        if i == 0 {
            self.set_cursor(0);
            return;
        }
        if let Some(cls) = self.prev_char(i).map(class_of) {
            while i > 0 && self.prev_char(i).map(class_of) == Some(cls) {
                i = self.prev_char_boundary(i);
            }
        }
        self.set_cursor(i);
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

    /// The byte range of the class run (word / punctuation / whitespace) that
    /// contains `i`. Runs never cross a newline, since `\n` is its own class.
    fn run_of(&self, i: usize) -> (usize, usize) {
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

    fn char_at(&self, i: usize) -> Option<char> {
        self.text[i..].chars().next()
    }

    /// Advance past a run of `cls` starting at `i`.
    fn skip_run(&self, mut i: usize, cls: WordClass) -> usize {
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
    fn skip_blanks(&self, mut i: usize) -> usize {
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

    fn prev_char(&self, offset: usize) -> Option<char> {
        let start = self.prev_char_boundary(offset);
        self.text[start..offset].chars().next()
    }

    // ----- edits (each pushes one undo snapshot) ------------------------

    /// Changes to the buffer text, for caching UI-side derived data.
    pub fn change_id(&self) -> u64 {
        self.change_id
    }

    fn snapshot(&mut self) {
        self.change_id += 1;
        // Skip snapshots that would record the state we are already in; an undo
        // step that appears to do nothing is worse than no step at all.
        if let Some(last) = self.undo.last() {
            if last.text == self.text && last.cursor == self.cursor {
                self.redo.clear();
                self.group_open = false;
                return;
            }
        }
        self.undo.push(Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            selection: self.selection,
        });
        self.redo.clear();
        self.group_open = false;
    }

    /// Start coalescing edits into one undo step, remembering the current state
    /// first (call when entering insert mode without having edited anything).
    pub fn begin_undo_group(&mut self) {
        self.snapshot();
        self.group_open = true;
    }

    /// Join an undo group that was already started by the operator that moved us
    /// into insert mode (`cw`, `c$`), so one `u` restores the whole change.
    pub fn continue_undo_group(&mut self) {
        self.group_open = true;
    }

    /// Stop coalescing (leaving insert mode). If nothing actually changed, the
    /// group's snapshot is dropped so undo never has a dead step.
    pub fn end_undo_group(&mut self) {
        if self.group_open {
            let unchanged = self
                .undo
                .last()
                .is_some_and(|last| last.text == self.text && last.cursor == self.cursor);
            if unchanged {
                self.undo.pop();
            }
        }
        self.group_open = false;
    }

    /// Snapshot unless an undo group is already open, so a run of insert-mode
    /// keystrokes becomes a single undo step.
    fn snapshot_for_edit(&mut self) {
        // The text is about to change even when the snapshot is suppressed, so
        // the change id has to move for UI-side caches to stay honest.
        self.change_id += 1;
        if !self.group_open {
            self.snapshot();
        }
    }

    /// Replace the selection (or insert at the cursor) with `text`.
    pub fn insert(&mut self, text: &str) {
        self.snapshot_for_edit();
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
        self.snapshot_for_edit();
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
        self.snapshot_for_edit();
        self.text.replace_range(self.cursor..end, "");
    }

    /// Delete the `count` characters at the cursor (vim `x`, `2x`).
    pub fn delete_char(&mut self, count: usize) {
        let count = count.max(1);
        let mut end = self.cursor;
        for _ in 0..count {
            end = self.next_char_boundary(end);
        }
        if end == self.cursor {
            return;
        }
        self.snapshot();
        self.text.replace_range(self.cursor..end, "");
        self.selection = None;
        self.cursor = self.clamp(self.cursor);
    }

    /// Delete the half-open byte range `[start, end)` and park the cursor at
    /// `start` (used by vim operators over a motion range). Joins an open undo
    /// group, which is how a table edit that splices several rows stays one step.
    pub fn delete_range(&mut self, start: usize, end: usize) {
        let (start, end) = (self.clamp(start), self.clamp(end));
        if start >= end {
            return;
        }
        self.snapshot_for_edit();
        self.text.replace_range(start..end, "");
        self.selection = None;
        self.cursor = self.clamp(start);
    }

    /// Delete `count` whole lines starting at the cursor's row, including their
    /// newlines (vim `dd`, `2dd`).
    pub fn delete_line(&mut self, count: usize) {
        let (start, end) = self.line_range(self.row_col(self.cursor).0, count.max(1));
        if start >= end {
            return;
        }
        self.snapshot();
        self.text.replace_range(start..end, "");
        self.cursor = self.clamp(start.min(self.text.len()));
        self.selection = None;
    }

    /// Insert `text` at `offset` without touching the selection semantics of
    /// [`Self::insert`] (used by linewise paste and by table splices, which run
    /// inside an undo group so one edit stays one undo step).
    pub fn insert_at(&mut self, offset: usize, text: &str) {
        let offset = self.clamp(offset);
        self.snapshot_for_edit();
        self.text.insert_str(offset, text);
        self.cursor = self.clamp(offset + text.len());
        self.selection = None;
    }

    /// Open `count` new lines below the cursor and place the cursor on the last
    /// of them (vim `o`, `2o`).
    pub fn open_line_below(&mut self, count: usize) {
        self.snapshot();
        let (row, _) = self.row_col(self.cursor);
        let end = self.line_end(row);
        let n = count.max(1);
        self.text.replace_range(end..end, &"\n".repeat(n));
        self.cursor = end + n;
        self.selection = None;
    }

    /// Open `count` new lines above the cursor (vim `O`, `2O`).
    pub fn open_line_above(&mut self, count: usize) {
        self.snapshot();
        let (row, _) = self.row_col(self.cursor);
        let start = self.line_start(row);
        let n = count.max(1);
        self.text.replace_range(start..start, &"\n".repeat(n));
        self.cursor = start;
        self.selection = None;
    }

    fn selection_range(&self) -> (usize, usize) {
        match self.selection {
            Some((a, h)) => (a.min(h), a.max(h)),
            None => (self.cursor, self.cursor),
        }
    }

    /// How many undo / redo steps are available (for diagnostics).
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    pub fn undo(&mut self) {
        self.group_open = false;
        self.change_id += 1;
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
        self.group_open = false;
        self.change_id += 1;
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

    /// `w`/`b`/`e` must treat punctuation runs as words instead of stalling.
    /// Like vim, a run of non-blank non-keyword characters counts as one word,
    /// so `).` is a single word.
    #[test]
    fn word_motions_punctuation() {
        let mut b = Buffer::new("a, b). c".to_string());
        b.word_forward(); // 'a' -> ','
        assert_eq!(b.cursor(), 1);
        b.word_forward(); // ',' -> 'b'
        assert_eq!(b.cursor(), 3);
        b.word_forward(); // 'b' -> ')'
        assert_eq!(b.cursor(), 4);
        b.word_forward(); // '). ' -> 'c'
        assert_eq!(b.cursor(), 7);
        b.word_forward(); // at end: stays
        assert_eq!(b.cursor(), 8);

        b.word_backward(); // end -> 'c'
        assert_eq!(b.cursor(), 7);
        b.word_backward(); // 'c' -> start of the `).` run
        assert_eq!(b.cursor(), 4);
        b.word_backward(); // '). ' -> 'b'
        assert_eq!(b.cursor(), 3);
        b.word_backward(); // 'b' -> ','
        assert_eq!(b.cursor(), 1);

        // `e` walks run ends, including punctuation runs.
        let mut b = Buffer::new("foo,bar".to_string());
        b.word_end(); // end of "foo"
        assert_eq!(b.cursor(), 2);
        b.word_end(); // end of ","
        assert_eq!(b.cursor(), 3);
        b.word_end(); // end of "bar"
        assert_eq!(b.cursor(), 6);
    }

    /// `w` crosses a line end to the first word of the next line.
    #[test]
    fn word_motion_crosses_lines() {
        let mut b = Buffer::new("end\n   next".to_string());
        b.set_cursor(0);
        b.word_forward(); // 'end' -> 'next'
        assert_eq!(b.cursor(), 7);
        b.word_backward(); // 'next' -> 'end'
        assert_eq!(b.cursor(), 0);
    }

    #[test]
    fn word_text_objects() {
        let b = buf(); // "hello world\nsecond line\nthird"
        assert_eq!(
            b.word_object(0, true),
            (0, 5),
            "iw is the run under the cursor"
        );
        assert_eq!(
            b.word_object(3, true),
            (0, 5),
            "from anywhere inside the word"
        );
        assert_eq!(
            b.word_object(0, false),
            (0, 6),
            "aw takes the trailing space"
        );
        assert_eq!(
            b.word_object(10, false),
            (5, 11),
            "and the leading one at the row end"
        );
        assert_eq!(b.word_object(5, true), (5, 6), "the space is its own run");
        assert_eq!(
            b.word_object(5, false),
            (5, 11),
            "aw on whitespace takes the word after"
        );

        let b = Buffer::new("a.b".to_string());
        assert_eq!(
            b.word_object(1, true),
            (1, 2),
            "punctuation is its own word"
        );
        assert_eq!(
            b.word_object(0, false),
            (0, 1),
            "aw does not span a run that is not whitespace"
        );
    }

    #[test]
    fn paragraph_text_objects() {
        let b = Buffer::new("one\ntwo\n\nthree\n\n\nfour".to_string());
        // rows: one(0) two(4) blank(8) three(9) blank(15) blank(16) four(17)
        assert_eq!(
            b.paragraph_object(0, true),
            (0, 7),
            "a run of non-blank lines"
        );
        assert_eq!(b.paragraph_object(5, true), (0, 7), "from any line in it");
        assert_eq!(
            b.paragraph_object(9, true),
            (9, 14),
            "a lone line is a paragraph"
        );
        assert_eq!(
            b.paragraph_object(8, true),
            (8, 9),
            "a blank line is its own object"
        );
        assert_eq!(
            b.paragraph_object(0, false),
            (0, 9),
            "ap swallows the blank line after"
        );
        assert_eq!(
            b.paragraph_object(9, false),
            (9, 17),
            "ap swallows every blank line after"
        );
        assert_eq!(
            b.paragraph_object(17, false),
            (15, 21),
            "ap takes the blank lines before when nothing follows"
        );
    }

    /// `f`/`t`/`F`/`T` search within the row only, and leave the cursor alone
    /// when the character is not there.
    #[test]
    fn find_char_motions() {
        let mut b = buf(); // "hello world\nsecond line\nthird"
        b.set_cursor(0);
        b.find_char('o', true, true, 1);
        assert_eq!(b.cursor(), 4, "f lands on the character");
        b.find_char('o', true, true, 1);
        assert_eq!(b.cursor(), 7, "a second f finds the next one");
        b.find_char('z', true, true, 1);
        assert_eq!(b.cursor(), 7, "a missing character leaves the cursor alone");

        b.set_cursor(0);
        b.find_char('o', true, false, 1);
        assert_eq!(b.cursor(), 3, "t stops before the character");

        b.set_cursor(7);
        b.find_char('w', false, true, 1);
        assert_eq!(b.cursor(), 6, "F searches backwards");
        b.set_cursor(7);
        b.find_char('w', false, false, 1);
        assert_eq!(
            b.cursor(),
            7,
            "T stops just after it, which is where we were"
        );
        b.set_cursor(10);
        b.find_char('e', false, false, 1);
        assert_eq!(b.cursor(), 2, "T lands after the match");

        b.set_cursor(0);
        b.find_char('o', true, true, 2);
        assert_eq!(b.cursor(), 7, "2f finds the second occurrence");

        b.set_cursor(8);
        b.find_char('s', true, true, 1);
        assert_eq!(b.cursor(), 8, "the search stops at the end of the row");
    }

    /// `$` is the last character of the line, not past it.
    #[test]
    fn dollar_is_last_char() {
        let mut b = Buffer::new("abcd\nxy".to_string());
        b.set_cursor(0);
        b.line_last_char_motion();
        assert_eq!(b.cursor(), 3);
        b.move_down();
        b.line_last_char_motion();
        assert_eq!(b.cursor(), 6);
        // An empty line stays at the line start.
        let mut b = Buffer::new("a\n\nb".to_string());
        b.set_cursor(2);
        b.line_last_char_motion();
        assert_eq!(b.cursor(), 2);
    }

    /// Vertical motions remember the column they were aiming for.
    #[test]
    fn vertical_motions_keep_desired_column() {
        let mut b = Buffer::new("aaaaaaaa\nb\ncccccccc".to_string());
        b.set_cursor(7); // col 7 of the first line
        b.move_down(); // clamped onto the short line
        assert_eq!(b.row_col(b.cursor()), (1, 1));
        b.move_down(); // restores col 7 on the third line
        assert_eq!(b.row_col(b.cursor()), (2, 7));
        // A horizontal motion resets the target column.
        b.line_start_motion();
        b.move_up();
        assert_eq!(b.row_col(b.cursor()), (1, 0));
    }

    #[test]
    fn vertical_motions_cross_wrapped_display_lines() {
        // One buffer row wrapped into three display lines, then a normal row.
        let mut b = Buffer::new("aaa bbb ccc\nd".to_string());
        b.set_display_segments(vec![vec![0, 4, 8], vec![12]]);
        assert_eq!(b.display_line_count(), 4);
        assert_eq!(b.display_range(1), Some((4, 8)));
        assert_eq!(b.display_index_for(9), 2);

        b.set_cursor(0);
        b.move_down();
        assert_eq!(b.cursor(), 4, "j steps onto the wrapped segment");
        b.move_down();
        assert_eq!(b.cursor(), 8);
        b.move_down();
        assert_eq!(b.cursor(), 12, "and then onto the next buffer row");
        b.move_up();
        assert_eq!(b.cursor(), 8);

        // The column being aimed for is relative to the display line.
        b.set_cursor(9); // one byte into the "ccc" segment
        assert_eq!(b.desired_col, Some(1));
        b.move_down();
        assert_eq!(b.cursor(), 13, "clamped to the short row");
        b.move_up();
        assert_eq!(b.cursor(), 9, "and restored when coming back");
    }

    #[test]
    fn a_collapsed_row_takes_no_display_line() {
        // A fenced block whose ``` lines live preview does not draw at all: five
        // buffer rows, three display lines.
        let mut b = Buffer::new("one\n```\ncode\n```\ntwo".to_string());
        b.set_display_segments(vec![vec![0], vec![], vec![8], vec![], vec![17]]);
        assert_eq!(b.display_line_count(), 3);
        assert_eq!(
            b.display_range(1),
            Some((8, 12)),
            "the code line follows `one`"
        );
        assert_eq!(b.display_index_for(17), 2);
        assert_eq!(
            b.display_index_for(4),
            1,
            "an offset inside a collapsed row resolves to the line after it"
        );

        b.set_cursor(0);
        b.move_down();
        assert_eq!(b.cursor(), 8, "j steps over the collapsed fence");
        b.move_down();
        assert_eq!(b.cursor(), 17, "and over the closing one");
        b.move_up();
        assert_eq!(b.cursor(), 8);
    }

    #[test]
    fn desired_column_counts_characters_not_bytes() {
        // 'é' is two bytes and one cell, so a byte column and a character column
        // disagree from the second character onwards.
        let mut b = Buffer::new("a\u{e9}\u{e9}\nxy".to_string());
        // row 0: a(0) \u{e9}(1..3) \u{e9}(3..5), newline at 5; row 1 starts at 6.
        b.set_cursor(3);
        assert_eq!(b.display_col(3), 2, "the second \u{e9} is the third cell");
        assert_eq!(b.desired_col, Some(2));

        b.move_down();
        assert_eq!(b.cursor(), 8, "two cells into the short row");
        b.move_up();
        assert_eq!(
            b.cursor(),
            3,
            "the cell column is restored, not a byte offset"
        );
    }

    #[test]
    fn screen_line_motions_respect_soft_wrap() {
        // "aaaa   bbbb" wrapped after the first space run, so the second display
        // line starts with blanks, plus a tab-indented second row.
        let mut b = Buffer::new("aaaa   bbbb\n\tzz".to_string());
        b.set_display_segments(vec![vec![0, 5], vec![12]]);
        assert_eq!(b.display_range(1), Some((5, 11)));

        b.set_cursor(9);
        b.line_start_motion();
        assert_eq!(b.cursor(), 5, "0 goes to the display line start");

        b.set_cursor(9);
        b.line_last_char_motion();
        assert_eq!(b.cursor(), 10, "$ goes to the display line's last char");

        b.set_cursor(9);
        b.first_non_blank();
        assert_eq!(b.cursor(), 7, "^ skips blanks inside the display line");

        b.set_cursor(9);
        b.row_first_non_blank();
        assert_eq!(b.cursor(), 0, "the line-wise `I` target ignores the wrap");

        b.set_cursor(9);
        b.row_last_char_motion();
        assert_eq!(b.cursor(), 10, "the line-wise `A` target is the row end");

        // On a row's last display line the screen motions are the row bounds.
        b.set_cursor(13);
        b.line_last_char_motion();
        assert_eq!(b.cursor(), 14);
        b.line_start_motion();
        assert_eq!(b.cursor(), 12);
        b.first_non_blank();
        assert_eq!(b.cursor(), 13, "^ skips the leading tab");
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

    /// A run of insert-mode edits is one undo step, like vim's insert session.
    #[test]
    fn insert_group_is_one_undo_step() {
        let mut b = Buffer::new("x".to_string());
        b.begin_undo_group();
        b.insert("a");
        b.insert("b");
        b.backspace();
        b.insert("c");
        b.end_undo_group();
        assert_eq!(b.text(), "acx");
        assert_eq!(b.undo_depth(), 1, "the whole session is one step");
        b.undo();
        assert_eq!(b.text(), "x");
        b.redo();
        assert_eq!(b.text(), "acx");
    }

    /// Entering insert mode without editing must not leave a dead undo step.
    #[test]
    fn empty_insert_group_is_dropped() {
        let mut b = Buffer::new("x".to_string());
        b.begin_undo_group();
        b.end_undo_group();
        assert_eq!(b.undo_depth(), 0);
    }

    /// Edits outside a group (normal-mode operators) still snapshot per edit.
    #[test]
    fn ungrouped_edits_snapshot_each() {
        let mut b = Buffer::new("x".to_string());
        b.insert("a");
        b.insert("b");
        assert_eq!(b.undo_depth(), 2);
        b.undo();
        assert_eq!(b.text(), "ax");
    }

    #[test]
    fn delete_line_middle_and_last() {
        let mut b = buf();
        b.set_cursor(13);
        b.delete_line(1);
        assert_eq!(b.text(), "hello world\nthird");

        let mut b2 = buf();
        b2.set_cursor(25);
        b2.delete_line(1);
        assert_eq!(b2.text(), "hello world\nsecond line");
    }

    #[test]
    fn open_lines() {
        let mut b = buf();
        b.set_cursor(5);
        b.open_line_below(1);
        assert_eq!(b.text(), "hello world\n\nsecond line\nthird");
        assert_eq!(b.cursor(), 12);

        let mut b2 = buf();
        b2.set_cursor(13);
        b2.open_line_above(1);
        assert_eq!(&b2.text()[11..13], "\n\n");
        assert_eq!(b2.cursor(), 12);
    }

    #[test]
    fn delete_char_and_selection() {
        let mut b = Buffer::new("héllo".to_string());
        b.set_cursor(1); // on the é (2 bytes)
        b.delete_char(1);
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

    #[test]
    fn counted_deletes() {
        let mut b = Buffer::new("abcdef\nsecond\nthird".to_string());
        b.set_cursor(0);
        b.delete_char(3);
        assert_eq!(b.text(), "def\nsecond\nthird");

        b.set_cursor(4); // on 'e' of "second"
        b.delete_line(2);
        assert_eq!(b.text(), "def");

        let mut b2 = Buffer::new("one two three four".to_string());
        b2.set_cursor(3);
        b2.delete_range(0, 7);
        assert_eq!(b2.text(), " three four");
        assert_eq!(b2.cursor(), 0);
    }

    #[test]
    fn linewise_helpers() {
        let b = Buffer::new("aa\nb\nccc".to_string());
        assert_eq!(b.yank_lines(0, 2), "aa\nb\n");
        assert_eq!(b.yank_lines(2, 5), "ccc");
        // Deleting the final line takes the preceding newline instead.
        assert_eq!(b.line_range(1, 1), (3, 5));
        assert_eq!(b.line_range(2, 1), (4, 8));

        let mut b = Buffer::new("aa\nb\nccc".to_string());
        b.goto_line(1);
        assert_eq!(b.cursor(), 3);
        b.goto_line(99);
        assert_eq!(b.cursor(), 5);
        b.insert_at(3, "d\n");
        assert_eq!(b.text(), "aa\nd\nb\nccc");
        assert_eq!(b.cursor(), 5);
    }

    #[test]
    fn counted_open_lines() {
        let mut b = Buffer::new("a\nb".to_string());
        b.set_cursor(0);
        b.open_line_below(2);
        assert_eq!(b.text(), "a\n\n\nb");
        assert_eq!(b.cursor(), 3);

        let mut b2 = Buffer::new("a\nb".to_string());
        b2.set_cursor(2);
        b2.open_line_above(2);
        assert_eq!(b2.text(), "a\n\n\nb");
        assert_eq!(b2.cursor(), 2);
    }
}
