//! The sidebar's keyboard state and the rows it draws.
//!
//! `push_node` walks the built tree, emits one row per visible node and records
//! the same paths in the same order, which is what lets `j`/`k` step through
//! exactly what the user can see: a collapsed folder's children are skipped for
//! free, because they were never rows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui::{
    div, prelude::*, px, Context, InteractiveElement, ParentElement, StatefulInteractiveElement,
    Styled, Window,
};
use shale_core::theme::Palette;

use crate::acts::*;
use crate::style::{CHIP_RADIUS, GLYPH_COLLAPSED, GLYPH_EXPANDED};
use crate::tree;

use super::Workspace;

/// What the sidebar rows are drawn from, bundled so `push_node` does not take
/// an argument per question it asks.
pub(super) struct TreeState<'a> {
    /// The note the sidebar marks as current.
    pub(super) active: &'a Option<PathBuf>,
    /// The row the keyboard has selected, or `None` while the sidebar does not
    /// have the keyboard.
    pub(super) selected: Option<&'a PathBuf>,
    /// The directories the user has opened.
    pub(super) expanded: &'a HashSet<PathBuf>,
    /// The theme's colours.
    pub(super) palette: &'a Palette,
}

/// Draw one node of the tree as a sidebar row, and record its path so the
/// keyboard motions can reach it.
pub(super) fn push_node(
    rows: &mut Vec<gpui::AnyElement>,
    paths: &mut Vec<PathBuf>,
    node: &tree::TreeNode,
    depth: usize,
    state: &TreeState,
    idx: &mut usize,
    cx: &mut Context<Workspace>,
) {
    let indent = px(12. * depth as f32);
    let is_dir = !node.is_file;
    let is_open = state.expanded.contains(&node.path);
    let is_active = state.active.as_deref() == Some(node.path.as_path());
    let is_selected = state.selected == Some(&node.path);
    let label = format!(
        "{} {}",
        if is_dir {
            if is_open {
                GLYPH_EXPANDED
            } else {
                GLYPH_COLLAPSED
            }
        } else {
            " "
        },
        node.name
    );
    let my_idx = *idx;
    *idx += 1;
    paths.push(node.path.clone());

    let row = div()
        .id(("tree-item", my_idx))
        .cursor_pointer()
        .pl(indent)
        .pr_1()
        .py_0p5()
        // The highlight is a chip, not a band: the sidebar pads its rows, so the
        // rounded corners are visible against the sidebar's own background.
        .rounded(CHIP_RADIUS)
        // Every row carries a rule, and only its colour changes: a border that
        // appeared on the selected row alone would push every row below it down
        // by two pixels as the keyboard moved. It is transparent rather than the
        // sidebar's own colour because a row also sits over a hover or active
        // fill, which an opaque rule would cut into.
        //
        // This is the window's state grammar (`design-language.md` §4): the
        // pointer gets `ui_hover_bg`, the open note gets `ui_active_bg`, and the
        // row the keyboard is on gets the focus rule.
        .border_1()
        .border_color(if is_selected {
            gpui::rgb(state.palette.ui_active_border)
        } else {
            // `rgba`, not `rgb`: an alpha of zero, where `rgb` would be opaque black.
            gpui::rgba(0x0000_0000)
        })
        .when(is_active, |d| d.bg(gpui::rgb(state.palette.ui_active_bg)))
        .hover(|s| s.bg(gpui::rgb(state.palette.ui_hover_bg)))
        .child(label)
        .on_click({
            let path = node.path.clone();
            cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                tracing::debug!(path = %path.display(), is_dir, "tree click");
                this.tree_selection = Some(path.clone());
                // A query note opens in the same editor as a note: it is a plain text
                // file the author owns and edits (P4.3 ruling). It is not indexed as a
                // note, and it is not in quick-open — the tree is where the author's own
                // files live, and `space f` stays a note list.
                let opens = path
                    .extension()
                    .map(|e| e == "md" || e == "shq")
                    .unwrap_or(false);
                if !is_dir && opens {
                    this.open_note(&path, cx);
                } else {
                    this.toggle_dir(path.clone());
                }
            })
        });
    rows.push(row.into_any());

    if is_dir && is_open {
        for child in &node.children {
            push_node(rows, paths, child, depth + 1, state, idx, cx);
        }
    }
}

impl Workspace {
    // ----- the file tree, from the keyboard ----------------------------

    /// Open every folder on the way to `path`, so the note the window came back with is
    /// a row the user can see rather than something inside a folder nobody opened.
    pub(super) fn expand_ancestors(&mut self, path: &Path) {
        let mut parent = path.parent();
        while let Some(dir) = parent.filter(|dir| !dir.as_os_str().is_empty()) {
            self.expanded.insert(dir.to_path_buf());
            parent = dir.parent();
        }
    }

    /// Whether `path` is one of the vault's folders.
    pub(super) fn is_dir(&self, path: &Path) -> bool {
        self.dirs.iter().any(|dir| dir.path == *path)
    }

    /// The folder a new note or folder belongs in: the selected folder, the
    /// folder holding the selected note, the folder of the note that is open, or
    /// the vault root.
    pub(super) fn target_dir(&self) -> PathBuf {
        match self.tree_selection.as_ref().or(self.note().as_ref()) {
            Some(path) if self.is_dir(path) => path.clone(),
            Some(path) => path.parent().map(Path::to_path_buf).unwrap_or_default(),
            None => PathBuf::new(),
        }
    }

    /// Move the tree's keyboard selection by `delta` rows. The rows are the ones
    /// the last frame drew, so a collapsed folder is stepped over.
    pub(super) fn move_tree_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.tree_rows.is_empty() {
            return;
        }
        let last = self.tree_rows.len() - 1;
        let current = self
            .tree_selection
            .as_ref()
            .and_then(|path| self.tree_rows.iter().position(|row| row == path));
        let next = match current {
            Some(index) => (index as isize + delta).clamp(0, last as isize) as usize,
            None => 0,
        };
        self.tree_selection = Some(self.tree_rows[next].clone());
        // The rows the keyboard steps through are the rows the last frame laid out,
        // in the same order, so the row's index is the scroll container's item index:
        // `j` past the bottom edge brings the row back rather than leaving the
        // selection somewhere off screen.
        self.tree_scroll.scroll_to_item(next);
        cx.notify();
    }

    /// Open the selected note, or open and close the selected folder.
    ///
    /// A note row also hands the keyboard to the note: the tree is where you go to
    /// *find* a note, so the next thing typed is almost always meant for it. A
    /// folder row keeps the sidebar's keyboard, because the next key is another
    /// tree motion.
    pub(super) fn open_tree_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.tree_selection.clone() else {
            return;
        };
        if self.is_dir(&path) {
            self.toggle_dir(path);
            cx.notify();
        } else {
            self.open_note(&path, cx);
            self.focus_editor(&FocusEditor, window, cx);
        }
    }

    /// `h`: close a folder, or step out to the folder that holds this row.
    pub(super) fn tree_collapse(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.tree_selection.clone() else {
            return;
        };
        if self.is_dir(&path) && self.expanded.remove(&path) {
            // Closing it is the action; the selection stays where it was.
        } else if let Some(parent) = path.parent() {
            self.tree_selection = Some(parent.to_path_buf());
        }
        cx.notify();
    }

    /// `l`: open a folder, or move into the first row inside it.
    pub(super) fn tree_expand(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.tree_selection.clone() else {
            return;
        };
        if self.is_dir(&path) && !self.expanded.insert(path.clone()) {
            let child = self
                .tree_rows
                .iter()
                .find(|row| row.starts_with(&path) && *row != &path);
            if let Some(child) = child {
                self.tree_selection = Some(child.clone());
            }
        }
        cx.notify();
    }

    /// `space e` / `:tree`: put the file tree away, or bring it back.
    ///
    /// It is a toggle rather than an on and an off because the way out has to be the
    /// same two keys as the way in, the same rule focus mode follows.
    pub(super) fn toggle_tree(
        &mut self,
        _: &TreeToggle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let show = !self.tree_visible;
        self.set_tree_visible(show, window, cx);
    }

    /// The visibility change behind `space e`, `:tree`, the palette row and the MCP
    /// command, so those doors cannot drift apart.
    ///
    /// The note takes the width the tree had, which is the point: hiding the sidebar
    /// to read the note at full width costs the user nothing.
    ///
    /// Two things are said out loud. A sidebar that vanishes with no other change is
    /// easy to mistake for a key that did nothing; and an `on` that was already true
    /// reports itself, the way `:typewriter on` does, so the second of two identical
    /// commands is not a mystery.
    ///
    /// The keyboard comes off the tree when it goes away. A tree that loses its rows
    /// while it holds the keyboard would leave the keyboard sitting on a row that is
    /// no longer drawn, and every key from there would be aimed at nothing.
    pub(crate) fn set_tree_visible(
        &mut self,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let word = if show { "shown" } else { "hidden" };
        if self.tree_visible == show {
            self.say(format!("tree is already {word}"), cx);
            return;
        }
        self.tree_visible = show;
        if !show && self.tree_focus.is_focused(window) {
            let focus = self.active_focus(cx);
            window.focus(&focus, cx);
        }
        self.say(format!("tree {word}"), cx);
        cx.notify();
    }

    /// Give the sidebar the keyboard.
    ///
    /// A sidebar that is put away is brought back first: `ctrl-w h` aimed at a
    /// window that is not drawn is a dead key otherwise, and the user asked to be
    /// looking at the tree. This does not go through `set_tree_visible` because the
    /// tree appearing on screen is its own report; a status line saying so would be
    /// noise on a command the user can see working.
    pub(super) fn focus_tree(
        &mut self,
        _: &FocusTree,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tree_selection.is_none() {
            self.tree_selection = self.note().or_else(|| self.tree_rows.first().cloned());
        }
        self.tree_visible = true;
        window.focus(&self.tree_focus, cx);
        cx.notify();
    }

    /// Hand the keyboard to whatever tab is on screen.
    ///
    /// This used to carry a graph branch that moved the frame back to the last note tab
    /// first — a shortcut from before panes and tab pages existed. The graph is a tab
    /// like any other now, so the keyboard goes to the graph itself (its canvas, or its
    /// filter strip when that is what the user is working in), and leaving the tab is
    /// tab paging: `g t`/`g T`, `:tabnext`, or closing it.
    pub(crate) fn focus_editor(
        &mut self,
        _: &FocusEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = self.active_focus(cx);
        window.focus(&focus, cx);
        cx.notify();
    }
}
