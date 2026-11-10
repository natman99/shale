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
    pub(super) fold_cache: Option<(u64, Option<String>, bool, Vec<fold::FoldRegion>)>,
    /// The file stem when this view's buffer is a query note (`.shq`), and
    /// `None` for everything else. What makes [`fold::query_region`] a region at
    /// all: a `.shq`'s headings are not sections, because the file is not
    /// markdown, and its program is one fold that starts shut.
    pub(super) query_note: Option<String>,
    /// Whether this view folds a query note's program (false in raw mode).
    pub(super) fold_program: bool,
    /// Whether *this view* refuses edits — preview mode. Motions, the selection,
    /// folding and the undo history keep working; only the text is off limits.
    ///
    /// Per view, not per buffer, because the same note can be read one way and
    /// edited another at the same time: a preview tab of a note and a live tab of
    /// it share one buffer (P4.17), and a flag on the buffer would let the
    /// reading view lock the editing one out.
    read_only: bool,
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
        self.query_note = None;
        self.fold_program = true;
    }

    /// Tell this view what kind of buffer it is looking at: a query note titled
    /// `stem` (or `None` for an ordinary note), and whether this view is allowed
    /// to fold its program. Raw mode says no — the raw view is the source, so
    /// there is nothing to fold there, and a `.shq`'s `#`-looking lines are not
    /// sections either, so it has no regions at all. Setting the same values
    /// again changes nothing, which is what lets the render loop ask every frame.
    pub fn set_query_note(&mut self, stem: Option<&str>, fold_program: bool) {
        if self.query_note.as_deref() == stem && self.fold_program == fold_program {
            return;
        }
        self.query_note = stem.map(str::to_string);
        self.fold_program = fold_program;
        // The regions are a different question for a different kind of file, so
        // the cached answer is not reusable even though the text has not moved.
        self.fold_cache = None;
    }

    // ----- folds ---------------------------------------------------------

    /// Whether this view refuses edits (preview mode).
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Put this view into or out of preview mode.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// The closed sections in this view.
    pub fn folds(&self) -> &fold::FoldState {
        &self.folds
    }

    /// Whether this view has any fold closed, which lets the UI skip
    /// [`Self::fold_view`] for the common case of a note with no folds.
    pub fn has_closed_folds(&self) -> bool {
        self.folds.any_closed()
    }

    /// The foldable regions of `text`, recomputed when the text or the kind of
    /// file changed. For a note they are headings and the rows they own; for a
    /// query note they are its program, which is one region and starts shut.
    pub fn fold_regions(&mut self, text: &str, change_id: u64) -> &[fold::FoldRegion] {
        let query = self.query_note.clone();
        let fold_program = self.fold_program;
        if self.fold_cache.as_ref().is_none_or(|(id, kind, fold, _)| {
            *id != change_id || *kind != query || *fold != fold_program
        }) {
            let regions = match query.as_deref() {
                // A query note's own text is a program, not a document: its
                // `#`-looking lines are not sections, so the query region is the
                // only region it has — and in raw mode there are none at all. A
                // file that does not parse as a query gets no region either, and
                // so shows its source.
                Some(stem) if fold_program => fold::query_region(text, stem).into_iter().collect(),
                Some(_) => Vec::new(),
                None => fold::fold_regions(text),
            };
            self.fold_cache = Some((change_id, query, fold_program, regions));
        }
        &self.fold_cache.as_ref().expect("just filled").3
    }
}
