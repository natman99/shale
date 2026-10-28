//! The frame: sidebar rows, the prompt, the note and the status bar.

use std::path::PathBuf;

use gpui::{anchored, div, prelude::*, px, AnchoredPositionMode, Context, Window};

use crate::acts::*;
use crate::tree;

use super::tree::{push_node, TreeState};
use super::{mode_label, Workspace};

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tree = tree::build_tree(&self.entries, &self.dirs);
        let active = self.active.clone();
        let selected = self.tree_selection.clone();
        let expanded = &self.expanded;
        // The keyboard selection is only drawn while the sidebar has the
        // keyboard, so a stale selection does not look like the cursor.
        let tree_focused = self.tree_focus.is_focused(window);
        let selected_row = if tree_focused {
            selected.as_ref()
        } else {
            None
        };

        // Build sidebar rows.
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        // The paths of the rows in the order they are drawn, which is what the
        // keyboard motions step through.
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut idx = 0usize;
        let state = TreeState {
            active: &active,
            selected: selected_row,
            expanded,
            palette: &self.palette,
        };
        for node in &tree {
            push_node(&mut rows, &mut paths, node, 0, &state, &mut idx, cx);
        }
        self.tree_rows = paths;
        let prompt = self.prompt.as_ref().map(|(_, prompt)| prompt.clone());
        let picker = self.picker.clone();
        // The popup is drawn last, over the note, where its line was painted.
        // Asking each frame is what lets it follow the note as it scrolls; a line
        // that is no longer on screen means no panel, which is the right answer
        // rather than a panel floating over the wrong word. The same bounds go on
        // record, because the hover path has to know where the panel is to let the
        // pointer travel from the word onto a row without closing it.
        let placed = self.popup_place(cx);
        self.popup_placed = placed;
        let popup = self.popup_entity().zip(placed);
        // The one path that had to close a popup without a window to hand the
        // keyboard back with (the file watcher) left the refocus for here, which
        // is the first place there is a window. `defer` keeps the change out of
        // the draw itself.
        if let Some(focus) = self.pending_focus.take() {
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }

        let mode = mode_label(self.editor.read(cx).mode());
        let dirty = if self.editor.read(cx).is_dirty() {
            "●"
        } else {
            ""
        };
        let pending = self.editor.read(cx).pending_hint();
        let (line, col) = self.editor.read(cx).display_position();
        // The checker's count, so "no underlines" can be told apart from "the
        // checker has not answered yet".
        let spell = match self.checker_notes.len() {
            0 => String::new(),
            1 => "1 spellcheck note".into(),
            n => format!("{n} spellcheck notes"),
        };

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(self.palette.ui_bg))
            .text_color(gpui::rgb(self.palette.ui_fg))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            // The vault edits are available from anywhere: the tree's selection
            // says what they act on, and the open note is the fallback.
            .on_action(cx.listener(Self::new_note))
            .on_action(cx.listener(Self::new_folder))
            .on_action(cx.listener(Self::rename_note))
            .on_action(cx.listener(Self::delete_note))
            .on_action(cx.listener(Self::focus_tree))
            .on_action(cx.listener(Self::focus_editor))
            .on_action(cx.listener(Self::window_left))
            .on_action(cx.listener(Self::window_right))
            .on_action(cx.listener(Self::quick_open))
            .on_action(cx.listener(Self::search_vault))
            .on_action(cx.listener(Self::show_note))
            .on_action(cx.listener(Self::command_palette))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .flex_none()
                            .w(px(260.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .border_r_1()
                            .border_color(gpui::rgb(self.palette.ui_border))
                            .child(
                                div()
                                    .p_2()
                                    .pb_1()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(
                                        self.vault
                                            .root()
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .to_string(),
                                    ),
                            )
                            // The prompt is a sibling of the tree, not a child of
                            // it: gpui enables a context's bindings whenever that
                            // context is in the focused element's stack, so a prompt
                            // *inside* the tree would still take the tree's `j`/`k`.
                            .when_some(prompt, |d, prompt| {
                                d.child(div().px_2().pb_2().child(prompt))
                            })
                            .child(
                                div()
                                    .id("tree-scroll")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_scroll()
                                    .key_context(tree::CONTEXT)
                                    .track_focus(&self.tree_focus)
                                    .on_action(cx.listener(|this, _: &TreeDown, _window, cx| {
                                        this.move_tree_selection(1, cx)
                                    }))
                                    .on_action(cx.listener(|this, _: &TreeUp, _window, cx| {
                                        this.move_tree_selection(-1, cx)
                                    }))
                                    .on_action(cx.listener(|this, _: &TreeOpen, window, cx| {
                                        this.open_tree_row(window, cx)
                                    }))
                                    .on_action(cx.listener(
                                        |this, _: &TreeCollapse, _window, cx| {
                                            this.tree_collapse(cx)
                                        },
                                    ))
                                    .on_action(cx.listener(|this, _: &TreeExpand, _window, cx| {
                                        this.tree_expand(cx)
                                    }))
                                    .on_action(cx.listener(Self::focus_editor))
                                    .on_action(cx.listener(Self::new_note))
                                    .on_action(cx.listener(Self::new_folder))
                                    .on_action(cx.listener(Self::rename_note))
                                    .on_action(cx.listener(Self::delete_note))
                                    .p_2()
                                    .pt_1()
                                    .text_sm()
                                    .children(rows),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            // The note gets some air: text hard against the sidebar,
                            // the top of the window and the status bar is cramped, and
                            // the first and last lines look cut off.
                            .px_4()
                            .pt_3()
                            .pb_2()
                            // The column, not just the text, has to report the
                            // pointer: the space below the last line belongs to no
                            // line, and a panel the pointer walked away downwards
                            // would otherwise never be told to go.
                            .on_mouse_move(cx.listener(
                                |this, event: &gpui::MouseMoveEvent, window, cx| {
                                    this.pointer_moved(event.position, window, cx);
                                },
                            ))
                            .child(div().flex_1().min_h_0().child(self.editor.clone()))
                            // A picker *overlays* the note rather than pushing it
                            // down: it is painted after the text, so opening one does
                            // not re-wrap every line underneath. Like the prompt it is
                            // a sibling of the editor, so the vim contexts stay out of
                            // its way.
                            .when_some(picker, |d, picker| {
                                d.child(div().absolute().top_2().left_4().right_4().child(picker))
                            }),
                    ),
            )
            .child(
                div()
                    .h(px(24.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_4()
                    .text_xs()
                    .bg(gpui::rgb(self.palette.ui_status_bg))
                    .text_color(gpui::rgb(self.palette.ui_fg))
                    .child(format!("{mode}{pending}{dirty}"))
                    .child(self.status.clone())
                    .child(spell)
                    .child(format!("{}:{}", line + 1, col + 1)),
            )
            .when_some(popup, |d, (popup, bounds)| {
                // `anchored` paints at a window coordinate rather than in the
                // layout, which is what lets the panel sit over the note without
                // pushing the note down; snapping to the window's edge keeps a
                // popup near the bottom or the right edge on screen.
                d.child(
                    anchored()
                        .position(bounds.origin)
                        .position_mode(AnchoredPositionMode::Window)
                        .snap_to_window()
                        .child(popup),
                )
            })
    }
}
