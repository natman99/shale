//! Folds: the closed heading sections, and the rules that keep a cursor from
//! coming to rest inside one.
//!
//! Which regions exist is `crate::fold`'s answer about the markdown; the
//! *closed* set belongs to the view (`buffer/view.rs`), keyed on heading text so
//! a fold survives an edit to the rows it owns.
//!
//! What stays on `Buffer` is the half that touches the caret. Closing a fold can
//! hide the row the caret is sitting on, and only the buffer knows where the
//! caret is — so each command below asks the view to change its fold state and
//! then clamps the caret against what the view now draws. The two halves meet
//! here, in the one file that is allowed to hold both.

use crate::fold;

use super::{Buffer, ViewSlot};

impl ViewSlot {
    /// How the closed folds change this view's display model: which buffer rows
    /// take no display line, and how many rows each folded heading hides (what
    /// the UI paints as its marker). The UI ORs `collapsed` into the rows live
    /// preview already hides, so `j`/`k`, the ruler and the scroll position
    /// follow.
    pub fn fold_view(&mut self, text: &str, change_id: u64, line_count: usize) -> fold::FoldView {
        let regions = self.fold_regions(text, change_id).to_vec();
        fold::fold_view(&regions, &self.folds, line_count)
    }

    /// The innermost region that contains `row` — a heading row is inside its
    /// own region — which is the section the cursor is in.
    pub fn fold_containing(
        &mut self,
        text: &str,
        change_id: u64,
        row: usize,
    ) -> Option<fold::FoldRegion> {
        self.fold_regions(text, change_id)
            // Regions are in buffer order, so the last one containing `row` is
            // the one that opens deepest into it.
            .iter()
            .rfind(|region| region.start_row <= row && row <= region.end_row)
            .cloned()
    }

    /// The regions this view has closed, in buffer order.
    pub fn closed_regions(&mut self, text: &str, change_id: u64) -> Vec<fold::FoldRegion> {
        let state = self.folds.clone();
        self.fold_regions(text, change_id)
            .iter()
            .filter(|region| state.is_closed(region))
            .cloned()
            .collect()
    }

    /// `za`: close the section at `row`, or open it if this view has it closed.
    pub fn fold_toggle_at(&mut self, text: &str, change_id: u64, row: usize) {
        let Some(region) = self.fold_containing(text, change_id, row) else {
            return;
        };
        if self.folds.is_closed(&region) {
            self.folds.open(&region);
        } else {
            self.folds.close(&region);
        }
    }

    /// `zc`: close the section at `row`.
    pub fn fold_close_at(&mut self, text: &str, change_id: u64, row: usize) {
        let Some(region) = self.fold_containing(text, change_id, row) else {
            return;
        };
        self.folds.close(&region);
    }

    /// `zo`: open the section at `row`.
    pub fn fold_open_at(&mut self, text: &str, change_id: u64, row: usize) {
        let Some(region) = self.fold_containing(text, change_id, row) else {
            return;
        };
        self.folds.open(&region);
    }

    /// `zM`: close every section in the note.
    pub fn fold_close_all(&mut self, text: &str, change_id: u64) {
        let regions = self.fold_regions(text, change_id).to_vec();
        self.folds.close_all(&regions);
    }

    /// `zR`, and vim's `zE`: open every section. The two coincide here because
    /// a fold is a closed region rather than a stored definition — once the rows
    /// are visible again there is nothing left to remove.
    pub fn fold_open_all(&mut self) {
        self.folds.open_all();
    }
}

impl Buffer {
    /// This view's fold answer for the current text: what is hidden, and what
    /// each folded heading shows instead.
    pub fn fold_view(&mut self, view: &mut ViewSlot) -> fold::FoldView {
        let (count, id) = (self.line_count(), self.change_id);
        view.fold_view(&self.text, id, count)
    }

    /// Whether this view has any fold closed — the cheap question the UI asks
    /// before it bothers asking the expensive one.
    pub fn has_closed_folds(&self, view: &ViewSlot) -> bool {
        view.has_closed_folds()
    }

    /// `za` at the cursor's row.
    pub fn fold_toggle(&mut self, view: &mut ViewSlot) {
        let row = self.cursor_row();
        self.fold_toggle_at(view, row);
    }

    /// `za` on `row`: the fold marker the UI drew is a click target, and the
    /// click can land on a row the cursor is not on.
    pub fn fold_toggle_at(&mut self, view: &mut ViewSlot, row: usize) {
        let id = self.change_id;
        view.fold_toggle_at(&self.text, id, row);
        self.clamp_cursor_out_of_folds(view);
    }

    /// `zc` at the cursor's row.
    pub fn fold_close(&mut self, view: &mut ViewSlot) {
        let row = self.cursor_row();
        let id = self.change_id;
        view.fold_close_at(&self.text, id, row);
        self.clamp_cursor_out_of_folds(view);
    }

    /// `zo` at the cursor's row. Opening cannot hide the caret, so there is
    /// nothing to clamp.
    pub fn fold_open(&mut self, view: &mut ViewSlot) {
        let row = self.cursor_row();
        let id = self.change_id;
        view.fold_open_at(&self.text, id, row);
    }

    /// `zM`: close every section.
    pub fn fold_close_all(&mut self, view: &mut ViewSlot) {
        let id = self.change_id;
        view.fold_close_all(&self.text, id);
        self.clamp_cursor_out_of_folds(view);
    }

    /// `zR` / `zE`: open every section.
    pub fn fold_open_all(&mut self, view: &mut ViewSlot) {
        view.fold_open_all();
    }

    /// The buffer row the cursor is on.
    fn cursor_row(&self) -> usize {
        self.row_col(self.cursor).0
    }

    /// A closed fold must never hide the cursor: put it on the heading that
    /// opens the innermost fold covering it, then walk outward until the row it
    /// lands on is drawn. Each step moves strictly upwards, so this terminates.
    fn clamp_cursor_out_of_folds(&mut self, view: &mut ViewSlot) {
        loop {
            let row = self.cursor_row();
            let hidden = self
                .fold_view(view)
                .collapsed
                .get(row)
                .is_some_and(|hidden| *hidden);
            if !hidden {
                return;
            }
            let id = self.change_id;
            let Some(start) = view
                .closed_regions(&self.text, id)
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
