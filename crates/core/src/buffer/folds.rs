//! Folds: the closed heading sections, and the rules that keep a cursor from
//! coming to rest inside one.
//!
//! Which regions exist is `crate::fold`'s answer about the markdown; what the
//! buffer owns is the *closed* set, keyed on heading text so a fold survives an
//! edit to the rows it owns.

use crate::fold;

use super::Buffer;

impl Buffer {
    /// The foldable regions of the current text, recomputed when the text
    /// changed. A region is a heading and the rows it owns.
    pub fn fold_regions(&mut self) -> &[fold::FoldRegion] {
        let change_id = self.change_id;
        if self
            .fold_cache
            .as_ref()
            .is_none_or(|(id, _)| *id != change_id)
        {
            self.fold_cache = Some((change_id, fold::fold_regions(&self.text)));
        }
        &self.fold_cache.as_ref().expect("just filled").1
    }
    /// The innermost region that contains `row` — a heading row is inside its
    /// own region — which is the section the cursor is in.
    pub fn fold_containing(&mut self, row: usize) -> Option<fold::FoldRegion> {
        self.fold_regions()
            // Regions are in buffer order, so the last one containing `row` is
            // the one that opens deepest into it.
            .iter()
            .rfind(|region| region.start_row <= row && row <= region.end_row)
            .cloned()
    }
    /// The regions that are closed right now, in buffer order.
    pub fn closed_regions(&mut self) -> Vec<fold::FoldRegion> {
        let state = self.folds.clone();
        self.fold_regions()
            .iter()
            .filter(|region| state.is_closed(region))
            .cloned()
            .collect()
    }
    /// Whether any fold is closed, which lets the UI skip [`Self::fold_view`]
    /// for the common case of a note with no folds.
    pub fn has_closed_folds(&self) -> bool {
        self.folds.any_closed()
    }
    /// How the closed folds change the display model: which buffer rows take no
    /// display line, and how many rows each folded heading hides (what the UI
    /// paints as its marker). The UI ORs `collapsed` into the rows live preview
    /// already hides, so `j`/`k`, the ruler and the scroll position follow.
    pub fn fold_view(&mut self) -> fold::FoldView {
        let regions = self.fold_regions().to_vec();
        fold::fold_view(&regions, &self.folds, self.line_count())
    }
    /// `za`: close the section the cursor is in, or open it if it is closed.
    pub fn fold_toggle_at(&mut self, row: usize) {
        let Some(region) = self.fold_containing(row) else {
            return;
        };
        if self.folds.is_closed(&region) {
            self.folds.open(&region);
        } else {
            self.folds.close(&region);
            self.clamp_cursor_out_of_folds();
        }
    }
    /// `zc`: close the section the cursor is in.
    pub fn fold_close_at(&mut self, row: usize) {
        let Some(region) = self.fold_containing(row) else {
            return;
        };
        self.folds.close(&region);
        self.clamp_cursor_out_of_folds();
    }
    /// `zo`: open the section the cursor is in.
    pub fn fold_open_at(&mut self, row: usize) {
        let Some(region) = self.fold_containing(row) else {
            return;
        };
        self.folds.open(&region);
    }
    /// `zM`: close every section in the note.
    pub fn fold_close_all(&mut self) {
        let regions = self.fold_regions().to_vec();
        self.folds.close_all(&regions);
        self.clamp_cursor_out_of_folds();
    }
    /// `zR`, and vim's `zE`: open every section. The two coincide here because
    /// a fold is a closed region rather than a stored definition — once the rows
    /// are visible again there is nothing left to remove.
    pub fn fold_open_all(&mut self) {
        self.folds.open_all();
    }
    /// `za` at the cursor's row.
    pub fn fold_toggle(&mut self) {
        let row = self.cursor_row();
        self.fold_toggle_at(row);
    }
    /// `zc` at the cursor's row.
    pub fn fold_close(&mut self) {
        let row = self.cursor_row();
        self.fold_close_at(row);
    }
    /// `zo` at the cursor's row.
    pub fn fold_open(&mut self) {
        let row = self.cursor_row();
        self.fold_open_at(row);
    }
    /// The buffer row the cursor is on.
    fn cursor_row(&self) -> usize {
        self.row_col(self.cursor).0
    }
    /// A closed fold must never hide the cursor: put it on the heading that
    /// opens the innermost fold covering it, then walk outward until the row it
    /// lands on is drawn. Each step moves strictly upwards, so this terminates.
    fn clamp_cursor_out_of_folds(&mut self) {
        loop {
            let row = self.cursor_row();
            if !self
                .fold_view()
                .collapsed
                .get(row)
                .is_some_and(|hidden| *hidden)
            {
                return;
            }
            let Some(start) = self
                .closed_regions()
                .iter()
                .filter(|region| region.hides(row))
                .map(|region| region.start_row)
                .next_back()
            else {
                return;
            };
            self.set_cursor(self.line_start(start));
            self.row_first_non_blank();
        }
    }

    // ----- display lines (soft wrap) ------------------------------------
}
