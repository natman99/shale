//! Per-view state about one buffer.
//!
//! `Buffer` owns the text, the caret and the undo history. What the *view* owns
//! is how that text is laid out on screen: where the UI broke each long row into
//! soft-wrapped display lines, and (in a later step) which heading sections the
//! user folded shut. Neither changes the text, and the same text can be laid out
//! two ways at once — a note in a narrow pane and the same note in a wide one,
//! or a note in a tab and the same note inside an embed.
//!
//! That is why this lives outside `Buffer` rather than inside it: a question
//! about display lines has to name the view it was asked of, or two views of one
//! note quietly answer it for each other. The signature of every display-aware
//! buffer method takes a `&ViewSlot` for exactly that reason — the compiler is
//! the guard, not a convention about remembering to swap state.

/// How one view lays out one buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewSlot {
    /// Per buffer row, the absolute byte offsets where wrapped display lines
    /// begin (supplied by the UI, which knows the pixel width). An empty vec
    /// means the row draws no display line at all — a collapsed fence line, a
    /// table's delimiter row; no entry means the row is a single unwrapped line.
    segments: Vec<Vec<usize>>,
}

impl ViewSlot {
    /// A slot with nothing measured yet: every row answers as one unwrapped line.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record where the UI wrapped each row for this view. Called every frame the
    /// text or the width changed; the buffer's `change_id` is what lets the UI
    /// skip the recomputation when neither did.
    pub fn set_segments(&mut self, segments: Vec<Vec<usize>>) {
        self.segments = segments;
    }

    /// The wrap points this view measured, if any.
    pub fn segments(&self) -> &[Vec<usize>] {
        &self.segments
    }

    /// Forget the measurement — a different note, or a width change that
    /// invalidates every break point.
    pub fn clear(&mut self) {
        self.segments.clear();
    }
}
