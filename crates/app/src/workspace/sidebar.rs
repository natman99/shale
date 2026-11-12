//! The right-hand strip's keyboard state and the rows it draws.
//!
//! The strip is a **chrome surface** hosted the way the file tree is (sidebar-plan.md
//! ruling 13): its own `FocusHandle`, its own keymap context, its own visibility flag,
//! session-restored, and a stop on the window's `ctrl-w h/j/k/l` path rather than a pane
//! in the tiling. It answers about one **note** (EDGE-S1), and it shows one **view** at a
//! time (ruling 11).

use std::path::PathBuf;

use gpui::{Context, Window};

use crate::acts::*;
use crate::sidebar::{self, View};

use super::Workspace;

/// What the outline rows are drawn from, bundled the way `TreeState` is.
pub(super) struct RowState<'a> {
    /// The row the keyboard has selected, or `None` while the strip does not have the
    /// keyboard — a stale selection must not look like a cursor.
    pub(super) selected: Option<usize>,
    /// The section the caret is in. A different fact from the selection, and it gets a
    /// different visual (ruling 14).
    pub(super) mark: Option<usize>,
    pub(super) palette: &'a shale_core::theme::Palette,
}

/// Draw one Outline row, mirroring the tree's `push_node`: the rows the keyboard steps
/// through are the rows the last frame laid out, in the same order, so a folded
/// heading's children are skipped for free.
pub(super) fn push_row(
    rows: &mut Vec<gpui::AnyElement>,
    row: &sidebar::Row,
    index: usize,
    state: &RowState,
    cx: &mut Context<Workspace>,
) {
    use gpui::prelude::*;
    let indent = gpui::px(12. * row.depth.saturating_sub(1) as f32);
    let is_selected = state.selected == Some(row.line);
    let is_mark = state.mark == Some(row.line);
    let line = row.line;
    let label = format!(
        "{} {}",
        if row.foldable {
            if row.closed {
                crate::style::GLYPH_COLLAPSED
            } else {
                crate::style::GLYPH_EXPANDED
            }
        } else {
            " "
        },
        row.text
    );
    let row_widget = gpui::div()
        .id(("sidebar-row", index))
        .cursor_pointer()
        .pl(indent)
        .pr_1()
        .py_0p5()
        .rounded(crate::style::CHIP_RADIUS)
        // The window's state grammar (`design-language.md` §4), and the tree's: every
        // row carries a rule and only its colour changes, so the keyboard moving does
        // not push the rows below it down. The selection is the focus rule; the mark is
        // the same fill the tree gives the note you are in.
        .border_1()
        .border_color(if is_selected {
            gpui::rgb(state.palette.ui_active_border)
        } else {
            gpui::rgba(0x0000_0000)
        })
        .when(is_mark, |d| d.bg(gpui::rgb(state.palette.ui_active_bg)))
        .hover(|s| s.bg(gpui::rgb(state.palette.ui_hover_bg)))
        .child(label)
        .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
            // A click selects and scrubs; it does not move the caret. The strip is a
            // surface you read, and the pointer should not cost the reader the place
            // their caret was — `enter` is the door that commits to a section.
            this.sidebar_selection = Some(line);
            this.sidebar_scrub(line, cx);
            cx.notify();
        }));
    rows.push(row_widget.into_any());
}

impl Workspace {
    // ----- the strip, from the keyboard --------------------------------

    /// `space b` / `:sidebar`: put the strip away, or bring it back.
    ///
    /// A toggle rather than an on and an off, for the tree's reason: the way out has to
    /// be the same two keys as the way in.
    pub(super) fn toggle_sidebar(
        &mut self,
        _: &SidebarToggle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let show = !self.sidebar_visible;
        self.set_sidebar_visible(show, window, cx);
    }

    /// The visibility change behind `space b`, `:sidebar`, the palette row and the MCP
    /// command, so those doors cannot drift apart.
    ///
    /// The note gives the strip the width it takes, which is the tree's rule pointed the
    /// other way; both chrome surfaces are there to be put away when the note wants the
    /// whole window.
    pub(crate) fn set_sidebar_visible(
        &mut self,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let word = if show { "shown" } else { "hidden" };
        if self.sidebar_visible == show {
            self.say(format!("sidebar is already {word}"), cx);
            return;
        }
        self.sidebar_visible = show;
        if !show && self.sidebar_focus.is_focused(window) {
            let focus = self.active_focus(cx);
            window.focus(&focus, cx);
        }
        self.say(format!("sidebar {word}"), cx);
        cx.notify();
    }

    /// `:sidebar <word>`: show the strip on one view.
    ///
    /// The keyboard stays where it is. A command that moves the keyboard is a command the
    /// user did not ask to be moved; `ctrl-w l` is how the strip gets the keys, and the
    /// strip appearing on screen is its own report, so this does not go through
    /// [`Self::set_sidebar_visible`] for the showing half.
    pub(crate) fn show_sidebar_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.sidebar_view = view;
        self.sidebar_visible = true;
        // The rows are a different question now, so the old selection does not carry:
        // a heading row and a backlink row are not the same kind of thing.
        self.sidebar_selection = None;
        self.say(format!("sidebar: {}", view.label()), cx);
        cx.notify();
    }

    /// `[` and `]` inside the strip: one view either way.
    pub(super) fn sidebar_step_view(&mut self, delta: isize, cx: &mut Context<Self>) {
        let view = self.sidebar_view.step(delta);
        self.sidebar_view = view;
        self.sidebar_selection = None;
        self.say(format!("sidebar: {}", view.label()), cx);
        cx.notify();
    }

    /// `ctrl-w l` at the right edge: give the strip the keyboard.
    ///
    /// A strip that is put away is brought back first — the tree's rule, and for the same
    /// reason: `ctrl-w l` aimed at something that is not drawn is otherwise a dead key.
    pub(super) fn focus_sidebar(
        &mut self,
        _: &FocusSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The strip is a stop on the `ctrl-w` path only when it has a note to describe.
        // Handing the keyboard to a surface with nothing in it is the dead-key problem
        // pointed the other way: the key does something, and the something is empty. A
        // graph tab, a blank tab and a query tab have no subject (EDGE-S1), so the
        // keyboard stays where it is and the status line says why.
        if self.sidebar_subject(cx).is_none() {
            self.say("the strip has no note to describe", cx);
            cx.notify();
            return;
        }
        self.sidebar_visible = true;
        if self.sidebar_selection.is_none() {
            self.sidebar_selection = self.sidebar_rows.first().map(|row| row.line);
        }
        window.focus(&self.sidebar_focus, cx);
        cx.notify();
    }

    /// `j`/`k`: move the strip's selection by `delta` rows.
    ///
    /// The rows are the ones the last frame drew, so a folded heading's children are
    /// stepped over for free. The selection is the heading's **buffer line**, not a row
    /// index: typing changes how many rows there are, and a line survives a row being
    /// added above it.
    pub(super) fn move_sidebar_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.sidebar_rows.is_empty() {
            return;
        }
        let last = self.sidebar_rows.len() - 1;
        let current = self
            .sidebar_selection
            .and_then(|line| self.sidebar_rows.iter().position(|row| row.line == line));
        let next = match current {
            Some(index) => (index as isize + delta).clamp(0, last as isize) as usize,
            None => 0,
        };
        let line = self.sidebar_rows[next].line;
        self.sidebar_selection = Some(line);
        // The row index is the scroll container's item index, as in the tree: `j` past
        // the bottom edge brings the row back rather than leaving the selection off
        // screen.
        self.sidebar_scroll.scroll_to_item(next);
        // The scrubber (ruling 18): the strip has the keyboard, so its selection shows
        // the note where the selection is — scroll only, caret untouched.
        self.sidebar_scrub(line, cx);
        cx.notify();
    }

    /// Show the note where buffer row `line` is, without moving the caret.
    ///
    /// Only when the strip's note is the one on screen: a row from an embedded note has
    /// no position in the host's scroll to reveal, and scrolling the host to a row number
    /// it did not ask for is a lie.
    pub(crate) fn sidebar_scrub(&mut self, line: usize, cx: &mut Context<Self>) {
        if !self.sidebar_view.built() || self.sidebar_subject(cx) != self.note() {
            return;
        }
        if let Some(editor) = self.editor().cloned() {
            editor.update(cx, |editor, cx| editor.preview_line(line, cx));
        }
    }

    /// `enter`: act on the selected row.
    ///
    /// For an outline row that means jumping the note to that heading and following the
    /// keyboard into the note — exactly what the tree's `enter` does with a file row
    /// (`tree.rs:186`), which is what "mirror the tree's bindings" buys.
    ///
    /// The jump never duplicates a tab and never re-reads a note (ruling 19). When the
    /// subject is the note already on screen this is a **pure reveal**: no `open_note`,
    /// no tab, no read. When it is not, `open_note` finds the tab that holds it or opens
    /// one, and `load_target` still refuses to overwrite the only view of a dirty note.
    pub(super) fn open_sidebar_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(line) = self.sidebar_selection else {
            return;
        };
        let Some(row) = self
            .sidebar_rows
            .iter()
            .find(|row| row.line == line)
            .cloned()
        else {
            return;
        };
        let subject = self.sidebar_subject(cx);
        let on_screen = self.note();
        if subject != on_screen {
            if let Some(path) = subject {
                self.open_note(&path, cx);
            }
        }
        let found = self.editor().is_some_and(|editor| {
            editor.update(cx, |editor, cx| editor.reveal_section(line, &row.text, cx))
        });
        if found {
            self.focus_editor(&FocusEditor, window, cx);
        } else {
            // The heading moved out from under the strip between frames — the note was
            // edited while the row was drawn. Say so rather than sit on a stale row.
            self.say(format!("no “{}” section to jump to", row.text), cx);
            cx.notify();
        }
    }

    /// `h`/`l`: close or open the section the selected heading opens.
    ///
    /// This is the note's own fold, not a second one kept by the strip (ruling 7): the
    /// outline mirrors the note, so folding there hides the rows here and `zc`/`zo` in
    /// the note hide them too. One decision, two surfaces.
    pub(super) fn sidebar_fold(&mut self, close: bool, cx: &mut Context<Self>) {
        let Some(line) = self.sidebar_selection else {
            return;
        };
        let Some(row) = self.sidebar_rows.iter().find(|row| row.line == line) else {
            return;
        };
        if !row.foldable {
            // A heading with nothing under it has no section to fold, which is the same
            // answer the note's gutter gives on that line.
            self.say(format!("“{}” has nothing under it", row.text), cx);
            cx.notify();
            return;
        }
        if let Some(editor) = self.editor().cloned() {
            editor.update(cx, |editor, cx| editor.fold_section_at(line, close, cx));
        }
        cx.notify();
    }

    /// `enter` in the strip's keymap: the jump.
    pub(super) fn sidebar_open(
        &mut self,
        _: &SidebarOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_sidebar_row(window, cx);
    }

    /// `h` in the strip's keymap.
    pub(super) fn sidebar_collapse(
        &mut self,
        _: &SidebarCollapse,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_fold(true, cx);
    }

    /// `l` in the strip's keymap.
    pub(super) fn sidebar_expand(
        &mut self,
        _: &SidebarExpand,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_fold(false, cx);
    }

    /// `[` in the strip's keymap.
    pub(super) fn sidebar_prev_view(
        &mut self,
        _: &SidebarPrevView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_step_view(-1, cx);
    }

    /// `]` in the strip's keymap.
    pub(super) fn sidebar_next_view(
        &mut self,
        _: &SidebarNextView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_step_view(1, cx);
    }

    // ----- what the strip is about -------------------------------------
    /// The note the strip describes: the note the caret is inside (an entered embed's
    /// outline is the one on screen), or the tab's. `None` is a tab with no note — the
    /// blank tab or the graph — which leaves the strip showing nothing rather than
    /// guessing (EDGE-S1).
    pub(crate) fn sidebar_subject(&self, cx: &mut Context<Self>) -> Option<PathBuf> {
        self.caret_note(cx)
            .or_else(|| self.pane().active_tab().path().map(|p| p.to_path_buf()))
    }

    /// The caret's buffer row in the strip's note, which is what the current-section
    /// mark follows. `None` when the strip's note is not the one on screen: a caret in
    /// another note says nothing about this one.
    pub(crate) fn sidebar_caret_line(&self, cx: &mut Context<Self>) -> Option<usize> {
        let subject = self.sidebar_subject(cx)?;
        if self.note().as_deref() != Some(subject.as_path()) {
            return None;
        }
        let editor = self.editor()?.clone();
        Some(editor.read_with(cx, |editor, _cx| editor.caret_row()))
    }

    /// The rows the Outline view would draw for the strip's note, from its **buffer**
    /// (EDGE-S5) and the note's own fold decisions (ruling 7).
    pub(crate) fn sidebar_outline(&mut self, cx: &mut Context<Self>) -> Vec<sidebar::Row> {
        let Some(path) = self.sidebar_subject(cx) else {
            return Vec::new();
        };
        let Some(note) = self.note_entry(&path, cx) else {
            return Vec::new();
        };
        let text = note.buffer.borrow().text().to_string();
        let folds = self
            .editor()
            .map(|editor| editor.read_with(cx, |editor, _cx| editor.folds()))
            .unwrap_or_default();
        sidebar::outline(&text, &folds)
    }
}
