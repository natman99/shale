//! The window's test-only surface: the accessors the app's tests read to see
//! what the window decided, and the hooks that stand in for a timer or a
//! checker. Compiled only for tests, so the binary carries none of it.
//!
//! They live apart from the window itself because they are read by tests and
//! never by the window's own code, and they are a lot of lines. A child module
//! sees its parent's private fields, so they ask for what they need directly.

use super::bar::{mode_label, save_indicator, word_badge};
use super::*;
use shale_core::vault::rel_text;

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
        let tab = self.pane().active;
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
    /// Whether the sidebar is drawn, for the `space e` and session tests.
    #[cfg(test)]
    pub(crate) fn tree_visible(&self) -> bool {
        self.tree_visible
    }
    /// The sidebar's keyboard, for the tests that check where the keys go when the
    /// tree is put away.
    #[cfg(test)]
    pub(crate) fn tree_focus(&self) -> gpui::FocusHandle {
        self.tree_focus.clone()
    }
    #[cfg(test)]
    /// How many display lines the note on screen is laid out as. The overlay's phantom
    /// rows are deliberately **not** in this number (roadmap P2.5b).
    #[cfg(test)]
    pub(crate) fn display_line_count<C: gpui::AppContext>(&self, cx: &C) -> usize {
        self.editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.display_line_count()))
            .unwrap_or_default()
    }
    /// The inline overlay of the note on screen, as that note would paint it
    /// (roadmap P2.5b): struck rows, phantom rows, and which hunk is marked.
    #[cfg(test)]
    pub(crate) fn overlay_shape<C: gpui::AppContext>(&self, cx: &C) -> Vec<String> {
        self.editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.overlay_shape()))
            .unwrap_or_default()
    }
    /// What the note on screen would paint for its transclusions (roadmap P4.0a).
    /// `&mut C` because resolving fills the editor's body cache, which is the point of
    /// the test: the cache is the difference between a view and a disk read per frame.
    #[cfg(test)]
    /// Where the last paint put the phantom rows of embedded notes, and what each one
    /// is a line of — the seam's clickable half, for the tests that click on it.
    /// The status bar's embed segment (roadmap P4.0b): the note the caret is inside,
    /// or empty when it is in the tab's own note.
    pub(crate) fn embed_badge<C: gpui::AppContext>(&self, cx: &C) -> String {
        self.pane()
            .active_tab()
            .editor()
            .and_then(|editor| editor.read_with(cx, |editor, _cx| editor.entered_name()))
            .map(|stem| format!("in ![[{stem}]]"))
            .unwrap_or_default()
    }

    /// A note this window holds, by name: its text and whether it has unsaved edits.
    /// Independent of which tab is on screen, which is the whole point of an embed —
    /// the note being typed into may not have a tab at all.
    pub(crate) fn note_state(&self, name: &str) -> Option<(String, bool)> {
        // By stem, across every form of path the window may have opened the note
        // under: a tab opened from a test's relative path and an embed reached
        // through the vault's absolute one are the same note to whoever is looking at
        // the screen.
        let (_, note) = self.notes.held().into_iter().find(|(path, _)| {
            path.file_stem()
                .is_some_and(|stem| stem.to_string_lossy() == name)
        })?;
        let text = note.buffer.borrow().text().to_string();
        Some((text, note.dirty.get()))
    }

    pub(crate) fn phantom_rows<C: gpui::AppContext>(&self, cx: &C) -> Vec<(String, f32, f32, f32)> {
        self.editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.phantom_rows()))
            .unwrap_or_default()
    }

    /// Where the entered embed's rows were painted this frame: `(row in the embedded
    /// note, left, top, bottom)`. Empty when no embed is being edited.
    pub(crate) fn embed_rows<C: gpui::AppContext>(&self, cx: &C) -> Vec<(usize, f32, f32, f32)> {
        self.editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.embed_rows()))
            .unwrap_or_default()
    }

    /// What note and row a point on the screen belongs to, or `None` when the point
    /// is not on an embedded note's row.
    pub(crate) fn origin_at<C: gpui::AppContext>(&self, at: (f32, f32), cx: &C) -> Option<String> {
        self.editor().and_then(|editor| {
            editor.read_with(cx, |editor, _cx| {
                editor
                    .origin_at(gpui::point(gpui::px(at.0), gpui::px(at.1)))
                    .map(|o| format!("{}:{}", o.path.display(), o.row))
            })
        })
    }

    /// Where the last paint put one of the note's own display lines.
    pub(crate) fn line_band<C: gpui::AppContext>(
        &self,
        index: usize,
        cx: &C,
    ) -> Option<(f32, f32, f32)> {
        self.editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.line_band(index)))
            .unwrap_or_default()
    }

    pub(crate) fn embed_shape<C: gpui::AppContext>(&self, cx: &mut C) -> Vec<String> {
        self.editor()
            .map(|editor| editor.update(cx, |editor, _cx| editor.embed_shape()))
            .unwrap_or_default()
    }

    /// The buffer rows the vim engine currently treats as under review.
    #[cfg(test)]
    pub(crate) fn review_lines<C: gpui::AppContext>(&self, cx: &C) -> Vec<(usize, usize)> {
        self.editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.review_lines()))
            .unwrap_or_default()
    }

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
        self.pane().active
    }
    /// How many graph tabs the window holds. The one-per-window rule is what a test
    /// checks, and `:graph` twice is how it would break.
    #[cfg(test)]
    pub(crate) fn graph_tabs(&self) -> usize {
        self.panes
            .iter()
            .flat_map(|pane| pane.tabs.iter())
            .filter(|tab| tab.is_graph())
            .count()
    }
    /// One pane's first display line, as painted: `(top, bottom, left)`, or
    /// `None` when the pane's editor painted no line at all — which is what a
    /// collapsed viewport looks like from the hit-test side.
    #[cfg(test)]
    pub(crate) fn pane_hit_box(
        &self,
        pix: usize,
        cx: &gpui::App,
    ) -> Option<(gpui::Pixels, gpui::Pixels, gpui::Pixels)> {
        let editor = self.panes[pix].active_tab().editor()?;
        editor.read_with(cx, |editor, _cx| {
            let (top, bottom) = editor.hit_line_edges(0)?;
            let left = editor.hit_line_x(0)?.0;
            Some((top, bottom, left))
        })
    }
    /// The height the pane's editor was actually given by the layout. A split
    /// whose wrapper failed to size the pane gives zero (the P3.2 bug: gpui
    /// does not stretch flex items, so the editor collapsed under the tab bar
    /// and painted nothing).
    #[cfg(test)]
    pub(crate) fn pane_editor_height(&self, pix: usize, cx: &gpui::App) -> f32 {
        let editor = self.panes[pix].active_tab().editor().unwrap();
        editor.read_with(cx, |editor, _cx| f32::from(editor.wrap_height()))
    }
    /// Whether the pane's editor cursor is drawn faded (the pane does not have
    /// the keyboard).
    #[cfg(test)]
    pub(crate) fn pane_cursor_dim(&self, pix: usize, cx: &gpui::App) -> bool {
        self.panes[pix]
            .active_tab()
            .editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.cursor_dim))
            .unwrap_or(false)
    }
    /// How many panes the window is tiled into (roadmap P3.2).
    #[cfg(test)]
    pub(crate) fn pane_count(&self) -> usize {
        self.panes.len()
    }
    /// Which pane has the keyboard.
    #[cfg(test)]
    pub(crate) fn active_pane_ix(&self) -> usize {
        self.active_pane
    }
    /// The notes of one pane's tab bar, in order: `None` is the empty tab,
    /// `Some("")` would be a graph tab's missing path.
    #[cfg(test)]
    pub(crate) fn pane_tab_paths(&self, pix: usize, _cx: &gpui::App) -> Vec<Option<PathBuf>> {
        self.panes[pix]
            .tabs
            .iter()
            .map(|tab| tab.path().map(|p| p.to_path_buf()))
            .collect()
    }
    /// Which tab one pane shows.
    #[cfg(test)]
    pub(crate) fn pane_tab_index(&self, pix: usize) -> usize {
        self.panes[pix].active
    }
    /// The root split's shape: `leaf`, `row` or `col` — enough for a test to
    /// check that `s` stacked and `v` tiled sideways.
    #[cfg(test)]
    pub(crate) fn layout_shape(&self) -> &'static str {
        match &self.layout {
            crate::workspace::panes::Node::Leaf(_) => "leaf",
            crate::workspace::panes::Node::Split { dir, .. } => match dir {
                crate::workspace::panes::SplitDir::Row => "row",
                crate::workspace::panes::SplitDir::Column => "col",
            },
        }
    }
    /// Where every pane actually sits, as fractions of the editing area: what a
    /// resize is supposed to change, and the honest way for a test to see it.
    #[cfg(test)]
    pub(crate) fn pane_rects(&self) -> Vec<(usize, f32, f32, f32, f32)> {
        self.layout.rects()
    }

    /// Whether the tab on screen is the graph.
    #[cfg(test)]
    pub(crate) fn showing_graph(&self) -> bool {
        self.pane().active_tab().is_graph()
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
    /// Open the graph the way `space g` does, but without a keystroke. A simulated
    /// keystroke pumps gpui's test executor, which lands the background scan before the
    /// test can see the gap between the tab opening and the picture arriving.
    #[cfg(test)]
    pub(crate) fn open_graph_now(&mut self, cx: &mut Context<Self>) {
        self.open_graph(cx);
    }
    /// Whether an index scan is in flight: the gap between the tab opening and the
    /// picture arriving, which is what the tab's "building" line is drawn from.
    #[cfg(test)]
    pub(crate) fn graph_building(&self) -> bool {
        self.graph_build.is_some()
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
            self.palette.graph_tag,
            self.palette.graph_ghost,
        )
    }
    /// Whether the note editor on screen holds the keyboard. The graph tab has no
    /// editor, so this answers `false` for it — which is how a test can tell "the
    /// keyboard is on the graph view" from "the keyboard is nowhere".
    #[cfg(test)]
    pub(crate) fn editor_is_focused(&self, window: &mut gpui::Window, cx: &gpui::App) -> bool {
        use gpui::Focusable as _;
        self.editor()
            .is_some_and(|editor| editor.read(cx).focus_handle(cx).is_focused(window))
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
    /// Whether the view is moving its own camera right now (the local graph's drift).
    #[cfg(test)]
    pub(crate) fn graph_reframing(&self, cx: &gpui::App) -> bool {
        self.graph_view()
            .is_some_and(|view| view.read(cx).reframing())
    }
    /// One tick of that drift, by hand.
    #[cfg(test)]
    pub(crate) fn graph_drift_target(&self, cx: &gpui::App) -> Option<([f32; 2], f32, f32)> {
        self.graph_view()?.read(cx).drift_target()
    }

    #[cfg(test)]
    pub(crate) fn graph_drift_step(&mut self, cx: &mut Context<Workspace>) -> bool {
        self.graph_view()
            .is_some_and(|view| view.update(cx, |view, _cx| view.drift_step()))
    }
    /// Which node carries the "this is the note the window is showing" mark, if any.
    #[cfg(test)]
    pub(crate) fn graph_centre_mark(&self, cx: &gpui::App) -> Option<usize> {
        self.graph_view()
            .and_then(|view| view.read(cx).centre_mark())
    }
    /// Where a node is on screen, in window px: the way a test aims the pointer at a dot
    /// without working the camera out for itself.
    #[cfg(test)]
    pub(crate) fn graph_node_screen(&self, slot: usize, cx: &gpui::App) -> Option<[f32; 2]> {
        let view = self.graph_view()?;
        let view = view.read(cx);
        match view.mode() {
            crate::graph::Dim::Two => {
                let camera = view.camera()?;
                let position = view.frame()?.node_position(slot)?;
                Some(camera.to_screen(position))
            }
            // The projected view answers from the projection the last frame built —
            // the same points the paint used, which is the only answer a pointer test
            // can aim at.
            crate::graph::Dim::Three => view.projection_slot(slot),
        }
    }
    /// What the graph would draw for a note right now: `(screen centre, drawn radius)`.
    #[cfg(test)]
    pub(crate) fn graph_drawn_node(&self, path: &str, cx: &gpui::App) -> Option<([f32; 2], f32)> {
        self.graph_view()?.read(cx).drawn_node(path)
    }

    /// The width of the whole picture on screen, in px.
    #[cfg(test)]
    pub(crate) fn graph_drawn_spread(&self, cx: &gpui::App) -> Option<f32> {
        self.graph_view()?.read(cx).drawn_spread()
    }

    /// How many times this window has published itself to the live channel.
    #[cfg(test)]
    pub(crate) fn live_publishes(&self) -> usize {
        self.live_publishes
    }

    /// Ask the projected picture the view is holding about a frame it is NOT holding.
    #[cfg(test)]
    pub(crate) fn graph_projection_hit(
        &self,
        frame: &crate::graph::GraphFrame,
        screen: [f32; 2],
        cx: &gpui::App,
    ) -> Option<usize> {
        self.graph_view()?.read(cx).projection_hit(frame, screen)
    }

    /// Ship a frame to the graph view without drawing it, the way the layout's beat
    /// does. A test that wants the window between a frame arriving and the frame being
    /// painted — where the projection on the view still belongs to the graph before it —
    /// cannot get there through a keystroke, because a simulated keystroke paints.
    #[cfg(test)]
    pub(crate) fn graph_ship_frame(
        &self,
        frame: Rc<crate::graph::GraphFrame>,
        cx: &mut Context<Workspace>,
    ) {
        if let Some(view) = self.graph_view().cloned() {
            view.update(cx, |view, cx| view.set_frame(frame, cx));
        }
    }

    /// Run the layout for `ticks` ticks and ship the frame. `settle_graph` runs the sim
    /// to rest in one go, which is what a paint test wants and what a drift test does
    /// not: the drift needs to be watched while the layout is still moving.
    #[cfg(test)]
    pub(crate) fn graph_run_for(&mut self, ticks: usize, cx: &mut Context<Workspace>) {
        let Some(sim) = self.graph_sim.as_mut() else {
            return;
        };
        sim.advance(ticks);
        let frame = std::rc::Rc::new(sim.frame());
        if let Some(view) = self.graph_view().cloned() {
            view.update(cx, |view, cx| view.set_frame(frame, cx));
        }
    }

    /// The projected camera as `(yaw, pitch, pan, zoom)`, for the mode tests: the angle
    /// an orbit made, and the framing a fit made.
    #[cfg(test)]
    pub(crate) fn graph_camera3(&self, cx: &gpui::App) -> Option<(f32, f32, [f32; 2], f32)> {
        self.graph_view()?.read(cx).camera3().map(|camera| {
            (
                camera.yaw,
                camera.pitch,
                [camera.pan[0], camera.pan[1]],
                camera.zoom,
            )
        })
    }
    /// How far the projected camera's eye sits from the origin, in world units. The
    /// dolly's whole contract is this number: travel, bounded by the cloud's radius.
    #[cfg(test)]
    pub(crate) fn graph_eye_distance(&self, cx: &gpui::App) -> Option<f32> {
        self.graph_view()?
            .read(cx)
            .camera3()
            .map(|camera| camera.dist)
    }
    /// The layout's third axis, straight off the window's sim: how a test tells a graph
    /// that opened up from one that stayed flat.
    #[cfg(test)]
    pub(crate) fn graph_depths(&self) -> Vec<f32> {
        self.graph_sim
            .as_ref()
            .map(|sim| sim.positions3().iter().map(|p| p[2]).collect())
            .unwrap_or_default()
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
            .map(|sim| sim.pinned())
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
    pub(crate) fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let index = self.pane().active;
        self.close_tab(index, window, cx)
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
                PickerTarget::Note(path) => rel_text(&path),
                PickerTarget::Hit { path, line } => format!("{}:{}", rel_text(&path), line),
                PickerTarget::Command(command) => command.label().to_string(),
                PickerTarget::Theme(name) => name,
                PickerTarget::Vault(path) => path.display().to_string(),
                PickerTarget::VaultPrompt => "prompt".into(),
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
