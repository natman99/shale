//! The window's test-only surface: the accessors the app's tests read to see
//! what the window decided, and the hooks that stand in for a timer or a
//! checker. Compiled only for tests, so the binary carries none of it.
//!
//! They live apart from the window itself because they are read by tests and
//! never by the window's own code, and they are a lot of lines. A child module
//! sees its parent's private fields, so they ask for what they need directly.

use super::bar::{mode_label, save_indicator, word_badge};
use super::*;

impl Workspace {
    /// The editor entity. Only the tests ask for it, to look at what the window
    /// handed the checker.
    #[cfg(test)]
    pub(crate) fn editor(&self) -> &Entity<MarkdownEditor> {
        &self.editor
    }
    /// The checker's notes on the open note, as the editor draws them.
    #[cfg(test)]
    pub(crate) fn spell_note_count(&self) -> usize {
        self.checker_notes.len()
    }
    /// The popup's rows, and whether one is up at all. Only the tests ask.
    #[cfg(test)]
    pub(crate) fn popup_rows(&self, cx: &gpui::App) -> Vec<String> {
        let Some(popup) = self.popup.as_ref() else {
            return Vec::new();
        };
        popup.read_with(cx, |popup, _cx| {
            popup.rows().iter().map(|row| row.to_string()).collect()
        })
    }
    /// Whether a popup is up. Only the tests ask.
    #[cfg(test)]
    pub(crate) fn has_popup(&self) -> bool {
        self.popup.is_some()
    }
    /// Which of the popup's offer rows it is drawing. Only the tests ask, to check
    /// that a long list scrolls without the panel changing height.
    #[cfg(test)]
    pub(crate) fn popup_drawn(&self, cx: &gpui::App) -> std::ops::Range<usize> {
        self.popup
            .as_ref()
            .map(|popup| popup.read_with(cx, |popup, _cx| popup.drawn()))
            .unwrap_or(0..0)
    }
    /// The complaint the popup is showing. Only the tests ask, to check that the
    /// panel is about the right word.
    #[cfg(test)]
    pub(crate) fn popup_message(&self, cx: &gpui::App) -> Option<String> {
        self.popup
            .as_ref()
            .map(|popup| popup.read_with(cx, |popup, _cx| popup.message().to_string()))
    }
    /// Offer the popup a list of fixes without a server to produce them, so the
    /// path from a row to the buffer can be tested. Only the tests call it.
    #[cfg(test)]
    pub(crate) fn offer_test_fixes(&mut self, fixes: Vec<Fix>, cx: &mut Context<Self>) {
        let Some(popup) = self.popup.clone() else {
            return;
        };
        self.popup_fixes = fixes;
        let rows: Vec<String> = self
            .popup_fixes
            .iter()
            .map(|fix| fix.title.clone())
            .collect();
        popup.update(cx, |popup, cx| popup.set_rows(rows, cx));
    }
    /// Put the checker's notes in place without a server, so the popup's paths
    /// can be tested headlessly. Only the tests call it.
    #[cfg(test)]
    pub(crate) fn set_checker_notes(&mut self, notes: Vec<Diagnostic>, cx: &mut Context<Self>) {
        self.show_notes(notes, cx);
    }
    /// Whether a checker process is running. Only the tests ask: the window's
    /// behaviour must not depend on the answer, and this is how that is checked.
    #[cfg(test)]
    pub(crate) fn has_checker(&self) -> bool {
        self.spell.is_some()
    }
    /// What the countdown and the blur hook do, without waiting for either: how a
    /// test reaches the write. The two session exits (`quit`, and the window
    /// manager's close) go through `save` instead, so that a note can be written
    /// with autosave off.
    #[cfg(test)]
    pub(crate) fn autosave_now(&mut self, cx: &mut Context<Self>) {
        self.autosave(cx);
    }
    /// What the watcher does on its timer — re-walk the vault and fold any
    /// difference into the window — without waiting for the timer.
    #[cfg(test)]
    pub(crate) fn check_vault_now(&mut self, cx: &mut Context<Self>) {
        let after = self.vault.signature().unwrap_or_default();
        self.apply_disk_changes(after, cx);
    }
    /// The open note's text as the editor holds it, and whether that text is
    /// ahead of the disk: how a test checks what a disk change did to a note.
    #[cfg(test)]
    pub(crate) fn buffer_text<C: gpui::AppContext>(&self, cx: &C) -> String {
        self.editor.read_with(cx, |editor, _cx| editor.text())
    }
    #[cfg(test)]
    pub(crate) fn buffer_is_dirty<C: gpui::AppContext>(&self, cx: &C) -> bool {
        self.editor.read_with(cx, |editor, _cx| editor.is_dirty())
    }
    /// What the status bar shows for the save state: how a test checks that the
    /// dot is lit while the note is ahead of the disk and gone once it is not.
    #[cfg(test)]
    pub(crate) fn dirty_indicator(&self, cx: &gpui::App) -> &'static str {
        save_indicator(self.editor.read(cx).is_dirty())
    }
    /// What the status bar shows for the open note's word count: how a test checks
    /// that the badge follows the buffer as the user types, and stays out of the bar
    /// when there is no note open.
    #[cfg(test)]
    pub(crate) fn word_badge(&self, cx: &gpui::App) -> String {
        word_badge(self.active.is_some(), self.editor.read(cx).word_count())
    }
    /// The cursor's buffer position (row, column), 0-based: how a test checks
    /// that choosing a search hit lands on the line that row was showing.
    #[cfg(test)]
    pub(crate) fn cursor_position<C: gpui::AppContext>(&self, cx: &C) -> (usize, usize) {
        self.editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
    }
    /// Whether the cursor's display line is inside the viewport as it was last
    /// painted. What the typewriter-scroll tests ask: a caret the reader cannot
    /// see is the whole complaint the mode exists to answer.
    #[cfg(test)]
    pub(crate) fn cursor_line_is_visible<C: gpui::AppContext>(&self, cx: &C) -> bool {
        let line = self
            .editor
            .read_with(cx, |editor, _cx| editor.display_position().0);
        self.display_line_is_visible(cx, line)
    }
    /// Whether the display line `line` is inside the viewport as it was last
    /// painted. The typewriter pair asks about a line *below* the caret: plain
    /// scroll-into-view stops at the caret, so a line a few rows down is only on
    /// screen when the frame centred the caret in the window.
    #[cfg(test)]
    pub(crate) fn display_line_is_visible<C: gpui::AppContext>(&self, cx: &C, line: usize) -> bool {
        self.editor
            .read_with(cx, |editor, _cx| editor.line_is_visible(line))
    }
    #[cfg(test)]
    pub(crate) fn tree_rows(&self) -> Vec<PathBuf> {
        self.tree_rows.clone()
    }
    #[cfg(test)]
    pub(crate) fn tree_selection(&self) -> Option<PathBuf> {
        self.tree_selection.clone()
    }
    #[cfg(test)]
    pub(crate) fn active_path(&self) -> Option<PathBuf> {
        self.active.clone()
    }
    /// The palette the window is drawn with. Only the tests ask, to check that a
    /// theme preview changed it and that `escape` put it back.
    #[cfg(test)]
    pub(crate) fn palette(&self) -> Rc<Palette> {
        Rc::clone(&self.palette)
    }
    #[cfg(test)]
    pub(crate) fn status(&self) -> String {
        self.status.to_string()
    }
    /// The mode the status bar shows, which is how a test checks that the prompt
    /// really did take the keyboard.
    #[cfg(test)]
    pub(crate) fn mode_label(&self, cx: &gpui::App) -> &'static str {
        mode_label(self.editor.read(cx).mode())
    }
    /// What the open prompt holds, or `None` while no prompt is open.
    #[cfg(test)]
    pub(crate) fn prompt_text(&self, cx: &gpui::App) -> Option<String> {
        self.prompt
            .as_ref()
            .map(|(_, prompt)| prompt.read(cx).text().to_string())
    }
    /// The note the open picker would choose, or `None` while no picker is open:
    /// how a test checks what a typed query narrowed the list to. A command row
    /// reads as its label, and a search hit as `path:line`.
    #[cfg(test)]
    pub(crate) fn picker_choice(&self, cx: &gpui::App) -> Option<String> {
        self.picker
            .as_ref()
            .and_then(|picker| picker.read(cx).chosen())
            .map(|target| match target {
                PickerTarget::Note(path) => path.display().to_string(),
                PickerTarget::Hit { path, line } => format!("{}:{}", path.display(), line),
                PickerTarget::Command(command) => command.label().to_string(),
                PickerTarget::Theme(name) => name,
            })
    }
    /// Where the note's first display line sits, in window coordinates: how a
    /// test checks the editor's margins, and that a picker floats over the text
    /// instead of moving it. `(top edge, x of the first character)`.
    #[cfg(test)]
    pub(crate) fn note_origin<C: gpui::AppContext>(
        &self,
        cx: &C,
    ) -> Option<(gpui::Pixels, gpui::Pixels)> {
        self.editor.read_with(cx, |editor, _cx| {
            let top = editor.hit_line_edges(0)?.0;
            // `hit_line_x` is the painted line's left edge in window
            // coordinates; `x_for_offset` is relative to the line, so it would
            // always report 0 and prove nothing about the margins.
            let left = editor.hit_line_x(0)?.0;
            Some((top, left))
        })
    }
    /// How many rows the open picker is offering.
    #[cfg(test)]
    pub(crate) fn picker_matches(&self, cx: &gpui::App) -> Option<usize> {
        self.picker.as_ref().map(|picker| picker.read(cx).matches())
    }
}
