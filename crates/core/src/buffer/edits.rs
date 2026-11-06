//! Edits, and the undo history they leave behind.
//!
//! Every edit takes a [`Snapshot`] first; the undo-group calls decide whether a
//! run of typing is one step or many, which is what makes `u` undo a typed word
//! rather than a character.
//!
//! Every method here that can change the text starts by asking whether the buffer
//! is read-only ([`Buffer::set_read_only`], which is preview mode) and does
//! nothing if it is. `undo` and `redo` are on that list too: undoing is an edit.

use super::{Buffer, Snapshot};

impl Buffer {
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
        if self.read_only {
            return;
        }
        self.snapshot_for_edit();
        let (start, end) = self.selection_range();
        self.text.replace_range(start..end, text);
        let new_cursor = start + text.len();
        self.selection = None;
        self.cursor = new_cursor;
    }
    pub fn delete_selected(&mut self) {
        if self.read_only || self.selection.is_none() {
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
        if self.read_only {
            return;
        }
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
        if self.read_only {
            return;
        }
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
    /// Delete the word before the cursor — insert-mode `ctrl-backspace`, and the
    /// same pair normal-mode `db` makes. Any blanks just before the cursor go with
    /// the word, which is what vim's `i_CTRL-W` does, so deleting after typing a
    /// space takes the word too rather than leaving it behind the cursor.
    pub fn delete_word_back(&mut self) {
        if self.read_only {
            return;
        }
        if self.selection.is_some() {
            self.delete_selected();
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let head = self.cursor;
        let start = super::word_back(&self.text, head);
        self.delete_range(start, head);
    }
    /// Delete the `count` characters at the cursor (vim `x`, `2x`).
    pub fn delete_char(&mut self, count: usize) {
        if self.read_only {
            return;
        }
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
        if self.read_only {
            return;
        }
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
        if self.read_only {
            return;
        }
        let (start, end) = self.line_range(self.row_col(self.cursor).0, count.max(1));
        if start >= end {
            return;
        }
        self.snapshot();
        self.text.replace_range(start..end, "");
        self.cursor = self.clamp(start.min(self.text.len()));
        self.selection = None;
    }
    /// vim's `J`: pull the line below up onto the cursor's line, `count - 1`
    /// times (`3J` joins two breaks). Read off vim, line by line:
    ///
    /// - The break becomes a space, and the next line's leading whitespace goes
    ///   with it: `one\n   two` is `one two`, not `one    two`.
    /// - A next line that is blank once its whitespace is stripped adds nothing at
    ///   all — no space: `one\n\ntwo` is `onetwo` up to the break that follows.
    /// - A line that already ends in whitespace does not get a second separator:
    ///   `one  \ntwo` is `one  two`.
    /// - After `.`, `?` or `!` vim's default `joinspaces` puts *two* spaces, and
    ///   Shale keeps that: it is what vim does, and mid-line it costs Markdown
    ///   nothing (only trailing double spaces mean a break).
    ///
    /// The cursor ends on the character the join happened at — the separator, or
    /// the last character of the line when nothing was inserted — which is where
    /// vim leaves it and what makes a repeated `J` keep walking.
    ///
    /// Returns whether anything was joined; on the last line there is nothing to
    /// join and the caller leaves the buffer alone.
    pub fn join_lines(&mut self, count: usize) -> bool {
        if self.read_only {
            return false;
        }
        let mut joined = false;
        // vim leaves the cursor on the *first* join, not the last one, so a `3J`
        // parks it where the line started being merged.
        let mut first_target: Option<usize> = None;
        // `J` is one join, `3J` is two: the count is the number of lines the
        // result spans, not the number of breaks that go.
        for _ in 1..count.max(2) {
            let row = self.row_col(self.cursor).0;
            // A note that ends with a line break does not have an empty line after
            // it — that break ends the file — so `J` on the last real line has
            // nothing to join, exactly as in vim.
            let phantom_last = usize::from(self.text.ends_with('\n'));
            if row + 1 + phantom_last >= self.line_count() {
                break;
            }
            let line_start = self.line_start(row);
            let line_end = self.line_end(row);
            let next_start = self.line_start(row + 1);
            let next_end = self.line_end(row + 1);
            let current = self.text[line_start..line_end].to_string();
            let next_raw = self.text[next_start..next_end].to_string();
            let lead = next_raw
                .char_indices()
                .find(|(_, c)| !c.is_whitespace())
                .map(|(i, _)| i)
                .unwrap_or(next_raw.len());
            let blank_next = lead >= next_raw.len();
            let separator = if blank_next || current.ends_with(char::is_whitespace) {
                ""
            } else if current
                .trim_end()
                .chars()
                .next_back()
                .is_some_and(|c| matches!(c, '.' | '?' | '!'))
            {
                "  "
            } else {
                " "
            };
            self.snapshot();
            // The break and the next line's leading whitespace both go; what is
            // left of that line follows the current one directly.
            self.text.replace_range(line_end..(next_start + lead), separator);
            self.selection = None;
            let target = if separator.is_empty() {
                line_end.max(line_start + 1) - 1
            } else {
                line_end
            };
            self.cursor = self.clamp(target);
            first_target.get_or_insert(target);
            joined = true;
        }
        if let Some(target) = first_target {
            self.cursor = self.clamp(target);
        }
        joined
    }
    /// Empty `count` lines for a linewise change (`cc`, `S`), leaving the line
    /// itself behind.
    ///
    /// [`Self::delete_line`] takes the line break with the line, which is what a
    /// delete wants and what a change does not: vim's `cc` leaves an empty line and
    /// starts inserting on it, rather than joining the change onto the line below.
    pub fn change_lines(&mut self, row: usize, count: usize) {
        if self.read_only {
            return;
        }
        let (start, end) = self.line_range(row, count.max(1));
        if start >= end {
            return;
        }
        // `line_range` takes the break *after* the lines when it can, and the break
        // *before* them when they are the buffer's last line. Asked of the range
        // before it goes away, that says which end of the replacement the surviving
        // break sits at.
        let break_came_first = self.text[start..end].starts_with('\n');
        self.snapshot();
        self.text.replace_range(start..end, "\n");
        // Either way the cursor ends up on the emptied line: before the break when
        // the break trails it, after it when the break leads.
        self.cursor = self.clamp(if break_came_first { start + 1 } else { start });
        self.selection = None;
    }

    /// Insert `text` at `offset` without touching the selection semantics of
    /// [`Self::insert`] (used by linewise paste and by table splices, which run
    /// inside an undo group so one edit stays one undo step).
    pub fn insert_at(&mut self, offset: usize, text: &str) {
        if self.read_only {
            return;
        }
        let offset = self.clamp(offset);
        self.snapshot_for_edit();
        self.text.insert_str(offset, text);
        self.cursor = self.clamp(offset + text.len());
        self.selection = None;
    }
    /// Open `count` new lines below the cursor and place the cursor on the last
    /// of them (vim `o`, `2o`).
    pub fn open_line_below(&mut self, count: usize) {
        if self.read_only {
            return;
        }
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
        if self.read_only {
            return;
        }
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
        if self.read_only {
            return;
        }
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
        if self.read_only {
            return;
        }
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
