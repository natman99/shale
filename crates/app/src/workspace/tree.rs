//! The sidebar's keyboard state and the rows it draws.
//!
//! `push_node` walks the built tree, emits one row per visible node and records
//! the same paths in the same order, which is what lets `j`/`k` step through
//! exactly what the user can see: a collapsed folder's children are skipped for
//! free, because they were never rows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui::{
    div, prelude::*, px, Context, Focusable, InteractiveElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use shale_core::theme::Palette;

use crate::acts::*;
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
                "▾"
            } else {
                "▸"
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
        .when(is_active, |d| d.bg(gpui::rgb(state.palette.ui_active_bg)))
        .when(is_selected, |d| d.bg(gpui::rgb(state.palette.ui_hover_bg)))
        .hover(|s| s.bg(gpui::rgb(state.palette.ui_hover_bg)))
        .child(label)
        .on_click({
            let path = node.path.clone();
            cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                tracing::debug!(path = %path.display(), is_dir, "tree click");
                this.tree_selection = Some(path.clone());
                if !is_dir && path.extension().map(|e| e == "md").unwrap_or(false) {
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

    /// Whether `path` is one of the vault's folders.
    pub(super) fn is_dir(&self, path: &Path) -> bool {
        self.dirs.iter().any(|dir| dir.path == *path)
    }

    /// The folder a new note or folder belongs in: the selected folder, the
    /// folder holding the selected note, the folder of the note that is open, or
    /// the vault root.
    pub(super) fn target_dir(&self) -> PathBuf {
        match self.tree_selection.as_ref().or(self.active.as_ref()) {
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

    /// Give the sidebar the keyboard.
    pub(super) fn focus_tree(
        &mut self,
        _: &FocusTree,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tree_selection.is_none() {
            self.tree_selection = self
                .active
                .clone()
                .or_else(|| self.tree_rows.first().cloned());
        }
        window.focus(&self.tree_focus, cx);
        cx.notify();
    }

    /// Hand the keyboard back to the note.
    pub(crate) fn focus_editor(
        &mut self,
        _: &FocusEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = self.editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }
}
