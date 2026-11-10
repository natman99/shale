//! A small text buffer with cursor/selection and undo, designed for
//! markdown notes (whole-file `String` storage is fine at this scale).

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    text: String,
    cursor: usize,
    selection: Option<(usize, usize)>,
}

pub use view::ViewSlot;

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
    /// True while insert-mode edits are being coalesced into one undo step.
    group_open: bool,
    /// Bumped on every mutation, so the UI can cache per-text computations
    /// (like soft-wrap, styles and quote substitution) across frames.
    ///
    /// The high bits are an epoch unique to *this* buffer, so the id is a safe key
    /// for those caches across a note switch. Counting edits from zero — which this
    /// used to do — hands every note nobody has edited the same id, and a cache
    /// keyed on it then answers for a note that is not the one on screen: the
    /// spans of a long note applied to a short one hide bytes that belong to other
    /// words, and a span past the short note's end indexes a row that does not
    /// exist. The low bits still move with this buffer's edits.
    change_id: u64,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new(String::new())
    }
}

mod display;
mod edits;
// The per-view half of the display model lives here, outside the text.
mod folds;
mod lines;
mod motions;
mod view;
mod words;

/// The word boundary before an offset, shared by the buffer's backward word
/// motion and by anything outside the buffer that has to agree with it (the
/// one-line prompt and picker fields).
pub use words::{big_word_back, indent_cut, word_back};

/// One epoch per buffer, so no two buffers share a `change_id` however many times
/// each is edited afterwards. 2^32 edits to one buffer is not a thing this editor
/// will see.
static EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Buffer {
    pub fn new(text: String) -> Self {
        Self {
            text,
            cursor: 0,
            selection: None,
            undo: Vec::new(),
            redo: Vec::new(),
            desired_col: None,
            group_open: false,
            change_id: EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed) << 32,
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
        // The vertical target column is *not* recomputed here: it is a display
        // column, and a display column belongs to a view, which this method does
        // not know. [`Self::move_display_to`] latches it from the view driving the
        // caret the first time a vertical motion needs it, which is the same number
        // this used to store eagerly — and it means a typed edit, which moves the
        // caret without a motion, re-aims at the column the caret is actually on
        // (what vim does) instead of the one the caret was on before typing.
        self.desired_col = None;
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
    /// Changes to the buffer text, for caching UI-side derived data. Unique to
    /// this buffer for as long as the process lives: two notes nobody has edited
    /// still answer with different ids.
    pub fn change_id(&self) -> u64 {
        self.change_id
    }
}

#[cfg(test)]
mod tests;
