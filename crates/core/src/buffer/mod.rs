//! A small text buffer with cursor/selection and undo, designed for
//! markdown notes (whole-file `String` storage is fine at this scale).

use crate::fold;

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
    /// Which heading sections the user has folded shut. View state like
    /// `display_segments`: it changes what is drawn, never the text.
    folds: fold::FoldState,
    /// The foldable regions of the current text, cached until the text changes
    /// (they are derived from the markdown scan, so recomputing them per key
    /// press would be wasteful).
    fold_cache: Option<(u64, Vec<fold::FoldRegion>)>,
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

mod display;
mod edits;
mod folds;
mod lines;
mod motions;
mod words;

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
            folds: fold::FoldState::default(),
            fold_cache: None,
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
        // Another note, another set of folds: they are keyed on heading text, so
        // carrying them over would fold unrelated rows.
        self.folds.clear();
        self.fold_cache = None;
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
    /// Changes to the buffer text, for caching UI-side derived data.
    pub fn change_id(&self) -> u64 {
        self.change_id
    }
}

#[cfg(test)]
mod tests;
