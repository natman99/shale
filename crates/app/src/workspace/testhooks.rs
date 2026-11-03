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
    /// The editor entity of the tab on screen. Only the tests ask for it, to look at
    /// what the window handed the checker.
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
        let tab = self.tab;
        self.autosave(tab, cx);
    }
    /// Turn autosave on or off in the window's own config, for the tests that check
    /// what an exit owes a note the user did not save by hand. The window holds its
    /// config by `Rc`, so this replaces it rather than editing one both share.
    #[cfg(test)]
    pub(crate) fn set_autosave(&mut self, on: bool) {
        let mut config = (*self.config).clone();
        config.editor.autosave = on;
        self.config = Rc::new(config);
    }
    /// Every tab's countdown-driven write, for the tests that check that a quit or a
    /// blur owes more than the note on screen.
    #[cfg(test)]
    pub(crate) fn autosave_every_tab(&mut self, cx: &mut Context<Self>) {
        self.autosave_all(cx);
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
        self.editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.text()))
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub(crate) fn buffer_is_dirty<C: gpui::AppContext>(&self, cx: &C) -> bool {
        self.editor()
            .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty()))
    }
    /// Whether any tab is ahead of its note on disk.
    #[cfg(test)]
    pub(crate) fn any_tab_is_dirty(&self, cx: &gpui::App) -> bool {
        self.any_dirty(cx)
    }
    /// What the status bar shows for the save state: how a test checks that the
    /// dot is lit while the note is ahead of the disk and gone once it is not.
    #[cfg(test)]
    pub(crate) fn dirty_indicator(&self, cx: &gpui::App) -> &'static str {
        save_indicator(
            self.editor()
                .is_some_and(|editor| editor.read(cx).is_dirty()),
        )
    }
    /// What the status bar shows for the open note's word count: how a test checks
    /// that the badge follows the buffer as the user types, and stays out of the bar
    /// when there is no note open.
    #[cfg(test)]
    pub(crate) fn word_badge(&self, cx: &gpui::App) -> String {
        word_badge(
            self.note().is_some(),
            self.editor()
                .map(|editor| editor.read(cx).word_count())
                .unwrap_or(0),
        )
    }
    /// The cursor's buffer position (row, column), 0-based: how a test checks
    /// that choosing a search hit lands on the line that row was showing.
    #[cfg(test)]
    pub(crate) fn cursor_position<C: gpui::AppContext>(&self, cx: &C) -> (usize, usize) {
        self.editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.cursor_position()))
            .unwrap_or((0, 0))
    }
    /// Whether the cursor's display line is inside the viewport as it was last
    /// painted. What the typewriter-scroll tests ask: a caret the reader cannot
    /// see is the whole complaint the mode exists to answer.
    #[cfg(test)]
    pub(crate) fn cursor_line_is_visible<C: gpui::AppContext>(&self, cx: &C) -> bool {
        let line = self
            .editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.display_position().0))
            .unwrap_or(0);
        self.display_line_is_visible(cx, line)
    }
    /// Whether the display line `line` is inside the viewport as it was last
    /// painted. The typewriter pair asks about a line *below* the caret: plain
    /// scroll-into-view stops at the caret, so a line a few rows down is only on
    /// screen when the frame centred the caret in the window.
    #[cfg(test)]
    pub(crate) fn display_line_is_visible<C: gpui::AppContext>(&self, cx: &C, line: usize) -> bool {
        self.editor()
            .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.line_is_visible(line)))
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
        self.note()
    }
    /// The tabs as the bar sees them: `(note, dirty)`, left to right, for the tests
    /// that check what `ctrl-t`, `gt` and `:tabclose` did to the bar.
    #[cfg(test)]
    pub(crate) fn tab_paths(&self, cx: &gpui::App) -> Vec<(Option<PathBuf>, bool)> {
        self.tab_state(cx)
            .into_iter()
            .map(|tab| (tab.path, tab.dirty))
            .collect()
    }
    /// Which tab is on screen, by index.
    #[cfg(test)]
    pub(crate) fn tab_index(&self) -> usize {
        self.tab
    }
    /// How many graph tabs the window holds. The one-per-window rule is what a test
    /// checks, and `:graph` twice is how it would break.
    #[cfg(test)]
    pub(crate) fn graph_tabs(&self) -> usize {
        self.tabs.iter().filter(|tab| tab.is_graph()).count()
    }
    /// Whether the tab on screen is the graph.
    #[cfg(test)]
    pub(crate) fn showing_graph(&self) -> bool {
        self.tabs[self.tab].is_graph()
    }
    /// The counts the graph panel was last told: how a test checks the panel is
    /// showing the vault rather than a placeholder.
    #[cfg(test)]
    pub(crate) fn graph_counts(&self, cx: &gpui::App) -> Option<crate::graph::GraphSummary> {
        self.graph_view().and_then(|view| view.read(cx).summary())
    }
    /// Whether the window has built its graph index at all — as distinct from having a
    /// tab showing it, which is what `:graph` on a second call must not rebuild.
    #[cfg(test)]
    pub(crate) fn graph_built(&self) -> bool {
        self.graph.is_some()
    }
    /// The filter the window is working with: what the chips are answering about, and
    /// what the session file has to remember.
    #[cfg(test)]
    pub(crate) fn graph_filter(&self) -> shale_core::graph::filter::GraphFilter {
        self.graph_filter.clone()
    }
    /// Whether the filter strip is showing and what its search line holds. The strip is
    /// the view's, not the window's, so this asks the view.
    #[cfg(test)]
    pub(crate) fn graph_panel(&self, cx: &gpui::App) -> (bool, String) {
        self.graph_view()
            .map(|view| {
                let view = view.read(cx);
                (view.panel_open(), view.query().to_string())
            })
            .unwrap_or((false, String::new()))
    }
    /// What the session file would be written as right now, for the tests that check a
    /// decision made inside a window survives a restart. Writing it is the beats'
    /// business; this is only what they would write.
    #[cfg(test)]
    pub(crate) fn session_snapshot(&self, cx: &mut Context<Self>) -> shale_core::Session {
        self.capture_session(cx)
    }
    /// The positions the graph tab is painting, in slot order. A test uses these to
    /// tell "the layout moved" from "the layout was carried over", which is the
    /// difference between a re-heat and a graph that saw no reason to move.
    #[cfg(test)]
    pub(crate) fn graph_positions(&self, cx: &gpui::App) -> Vec<[f32; 2]> {
        self.graph_view()
            .and_then(|view| view.read(cx).frame())
            .map(|frame| frame.nodes.iter().map(|node| node.position).collect())
            .unwrap_or_default()
    }
    /// Whether the window's simulation has cooled, which is whether the beat has any
    /// right to keep asking for frames.
    #[cfg(test)]
    pub(crate) fn graph_settled(&self) -> bool {
        self.graph_sim.as_ref().is_none_or(|sim| sim.settled())
    }
    /// Take the layout's beat down without settling it, so a test can read the frame a
    /// change shipped rather than whatever the next tick would have made of it.
    #[cfg(test)]
    pub(crate) fn stop_graph_beat(&mut self) {
        self.graph_beat = None;
    }
    /// The colours the graph paints with: a note's dot, a tag's, and a ghost's. A paint
    /// test counts the dots it expects by the colour they were drawn in, which is the
    /// only way to tell a graph's nodes from the rest of the window's quads.
    #[cfg(test)]
    pub(crate) fn graph_colors(&self) -> (u32, u32, u32) {
        (
            self.palette.graph_node,
            self.palette.tag,
            self.palette.ui_dim,
        )
    }
    /// The graph tab's camera as `(zoom, pan)`, for the tests that check what a wheel,
    /// a drag or an arrow did to it. `None` while the view is still framing the whole
    /// graph itself — which is also the answer after `f`.
    #[cfg(test)]
    pub(crate) fn graph_camera(&self, cx: &gpui::App) -> Option<(f32, [f32; 2])> {
        let view = self.graph_view()?;
        view.read(cx)
            .camera()
            .map(|camera| (camera.zoom, [camera.pan[0], camera.pan[1]]))
    }
    /// Where a node is on screen, in window px: the way a test aims the pointer at a dot
    /// without working the camera out for itself.
    #[cfg(test)]
    pub(crate) fn graph_node_screen(&self, slot: usize, cx: &gpui::App) -> Option<[f32; 2]> {
        let view = self.graph_view()?;
        let view = view.read(cx);
        let camera = view.camera()?;
        let position = view.frame()?.node_position(slot)?;
        Some(camera.to_screen(position))
    }
    /// The canvas's bounds, for the tests that click in the middle of the graph.
    #[cfg(test)]
    pub(crate) fn graph_canvas(&self, cx: &gpui::App) -> Option<[f32; 4]> {
        let bounds = self.graph_view()?.read(cx).canvas()?;
        Some([
            bounds.origin.x.as_f32(),
            bounds.origin.y.as_f32(),
            bounds.size.width.as_f32(),
            bounds.size.height.as_f32(),
        ])
    }
    /// Which bodies the simulation is holding against the forces: a drag pins, letting
    /// go unpins, and a shift-drag does not.
    #[cfg(test)]
    pub(crate) fn graph_pinned(&self) -> Vec<bool> {
        self.graph_sim
            .as_ref()
            .map(|sim| sim.pinned().to_vec())
            .unwrap_or_default()
    }
    /// The simulation's positions, straight off the window rather than from the frame
    /// the tab is painting: a drag moves the sim, and the frame follows on the next
    /// tick, so a test that wants to know what the pointer did asks the sim.
    #[cfg(test)]
    pub(crate) fn graph_sim_positions(&self) -> Vec<[f32; 2]> {
        self.graph_sim
            .as_ref()
            .map(|sim| sim.positions())
            .unwrap_or_default()
    }
    /// Whether the layout's beat is running — which it is not, once the sim has come
    /// to rest, and must not be.
    #[cfg(test)]
    pub(crate) fn graph_beat_running(&self) -> bool {
        self.graph_beat.is_some()
    }
    /// The node the pointer is over, and the one the keyboard has chosen.
    #[cfg(test)]
    pub(crate) fn graph_hover(&self, cx: &gpui::App) -> Option<usize> {
        self.graph_view()?.read(cx).hover()
    }
    /// What the graph tab's nodes stand for, in slot order: the way a test finds the
    /// slot of a note it means to click, since the index renumbers its nodes on every
    /// rebuild.
    #[cfg(test)]
    pub(crate) fn graph_paths(&self, cx: &gpui::App) -> Vec<Option<String>> {
        let Some(view) = self.graph_view() else {
            return Vec::new();
        };
        let Some(frame) = view.read(cx).frame() else {
            return Vec::new();
        };
        frame
            .nodes
            .iter()
            .map(|node| node.path.as_ref().map(|path| path.to_string()))
            .collect()
    }
    #[cfg(test)]
    pub(crate) fn graph_selected(&self, cx: &gpui::App) -> Option<usize> {
        self.graph_view()?.read(cx).selected()
    }
    /// Close the tab on screen, as the tab bar's `×` and `:tabclose` do. The graph tab
    /// has no command line of its own — `:` is bound in the note's context — so the
    /// tests that want a tab gone ask the window rather than typing a word it cannot
    /// hear.
    #[cfg(test)]
    pub(crate) fn close_active_tab(&mut self, cx: &mut Context<Self>) -> bool {
        let index = self.tab;
        self.close_tab(index, cx)
    }
    /// Move the frame to tab `index`, for the same reason: a test that needs the
    /// keyboard on a tab no key reaches from where the keyboard already is.
    #[cfg(test)]
    pub(crate) fn show_tab_at(&mut self, index: usize, cx: &mut Context<Self>) {
        self.show_tab(index, cx);
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
        self.editor()
            .map(|editor| mode_label(editor.read(cx).mode()))
            .unwrap_or("")
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
        let editor = self.editor()?;
        editor.read_with(cx, |editor, _cx| {
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
