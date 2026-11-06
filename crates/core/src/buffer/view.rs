//! Per-view state about one buffer.
//!
//! `Buffer` owns the text, the caret and the undo history. What the *view* owns
//! is how that text is laid out on screen: where the UI broke each long row into
//! soft-wrapped display lines, and (in a later step) which heading sections the
//! user folded shut. Neither changes the text, and the same text can be laid out
//! two ways at once — a note in a narrow pane and the same note in a wide one,
//! or a note in a tab and the same note inside an embed.
//!
//! Folds are here for the same reason: `zM` in a tab should not fold the
//! embed of the same note sitting beside it.
//!
//! That is why this lives outside `Buffer` rather than inside it: a question
//! about display lines has to name the view it was asked of, or two views of one
//! note quietly answer it for each other. The signature of every display-aware
//! buffer method takes a `&ViewSlot` for exactly that reason — the compiler is
//! the guard, not a convention about remembering to swap state.

use crate::fold;

/// How one view lays out one buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewSlot {
    /// Per buffer row, the absolute byte offsets where wrapped display lines
    /// begin (supplied by the UI, which knows the pixel width). An empty vec
    /// means the row draws no display line at all — a collapsed fence line, a
    /// table's delimiter row; no entry means the row is a single unwrapped line.
    segments: Vec<Vec<usize>>,
    /// Which heading sections this view has folded shut. Folded, not stored:
    /// the regions themselves are re-derived from the markdown, and a closed
    /// fold is keyed on its heading text so it survives an edit to the rows it
    /// owns. See `buffer/folds.rs` for the caret half of the story.
    pub(super) folds: fold::FoldState,
    /// The regions of the text this slot last measured, with the `change_id`
    /// they were measured against — the same invalidation trick as `segments`.
    pub(super) fold_cache: Option<(u64, Vec<fold::FoldRegion>)>,
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

    /// Forget everything measured or folded — a different note is in this view.
    pub fn clear(&mut self) {
        self.segments.clear();
        self.folds.clear();
        self.fold_cache = None;
    }

    // ----- folds ---------------------------------------------------------

    /// The closed sections in this view.
    pub fn folds(&self) -> &fold::FoldState {
        &self.folds
    }

    /// Whether this view has any fold closed, which lets the UI skip
    /// [`Self::fold_view`] for the common case of a note with no folds.
    pub fn has_closed_folds(&self) -> bool {
        self.folds.any_closed()
    }

    /// The foldable regions of `text`, recomputed when the text changed. A
    /// region is a heading and the rows it owns.
    pub fn fold_regions(&mut self, text: &str, change_id: u64) -> &[fold::FoldRegion] {
        if self
            .fold_cache
            .as_ref()
            .is_none_or(|(id, _)| *id != change_id)
        {
            self.fold_cache = Some((change_id, fold::fold_regions(text)));
        }
        &self.fold_cache.as_ref().expect("just filled").1
    }
}
