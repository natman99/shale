//! The frame: sidebar rows, the prompt, the note and the status bar.

use std::path::PathBuf;

use gpui::{anchored, div, prelude::*, px, AnchoredPositionMode, Context, MouseButton, Window};

use crate::acts::*;
use crate::editor::View;
use crate::prompt;
use crate::style::CHIP_RADIUS;
use crate::tree;

use super::bar::{mode_label, save_indicator, word_badge};
use super::tree::{push_node, TreeState};
use super::Workspace;

/// The height of the tab bar's row, and of one tab within it. A little taller than
/// the status bar because it is something the pointer aims at, not something it
/// reads.
const TAB_BAR: gpui::Pixels = gpui::px(28.);
/// One tab's height inside the bar. The four pixels of bar colour above and below it
/// are the point: with one tab open, a chip that fills the bar reads as the bar, and
/// a chip the bar frames reads as a tab.
const TAB_ROW: gpui::Pixels = gpui::px(20.);
/// The bar's own colour left at its ends and between two tabs.
const TAB_GAP: gpui::Pixels = gpui::px(6.);

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A tab change that had no window to rehook the hover subscription with gets
        // it here, one frame later, rather than never: the pointer can only hover over
        // the note that is drawn, so the subscription has to be the drawn tab's. The
        // keyboard follows the same rule, for the same reason.
        if self.hover_stale {
            cx.on_next_frame(window, |ws, window, cx| ws.watch_hover(window, cx));
        }
        if self.focus_stale {
            cx.on_next_frame(window, |ws, window, cx| ws.focus_active_tab(window, cx));
        }
        let tree = tree::build_tree(&self.entries, &self.dirs);
        let active = self.note();
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
        let prompt = self
            .prompt
            .as_ref()
            .filter(|(kind, _)| {
                !matches!(
                    kind,
                    prompt::PromptKind::Command | prompt::PromptKind::Search
                )
            })
            .map(|(_, prompt)| prompt.clone());
        // vim's command line and its `/` search both ask for something to be typed
        // at the bottom of the note rather than a name for a file, so both sit where
        // vim puts them — over the bottom of the note, not in the sidebar with the
        // prompts that name a note.
        let bottom_field = self
            .prompt
            .as_ref()
            .filter(|(kind, _)| {
                matches!(
                    kind,
                    prompt::PromptKind::Command | prompt::PromptKind::Search
                )
            })
            .map(|(_, prompt)| prompt.clone());
        let picker = self.picker.clone();
        // The popup is drawn last, over the note, where its line was painted.
        // Asking each frame is what lets it follow the note as it scrolls; a line
        // that is no longer on screen means no panel, which is the right answer
        // rather than a panel floating over the wrong word. The hover path asks the
        // same question rather than reading what this frame drew, because a pointer
        // move can arrive between frames — and near the bottom of the window the
        // panel has moved to above its line by then.
        let placed = self.popup_place(window, cx).map(|placed| placed.bounds);
        let popup = self.popup_entity().zip(placed);
        // The one path that had to close a popup without a window to hand the
        // keyboard back with (the file watcher) left the refocus for here, which
        // is the first place there is a window. `defer` keeps the change out of
        // the draw itself.
        if let Some(focus) = self.pending_focus.take() {
            window.defer(cx, move |window, cx| focus.focus(window, cx));
        }

        let mode = mode_label(self.editor().read(cx).mode());
        let dirty = save_indicator(self.editor().read(cx).is_dirty());
        let pending = self.editor().read(cx).pending_hint();
        let (line, col) = self.editor().read(cx).display_position();
        // Which of the three views the note is in, for the badge below. All three
        // are named, so the slot does not appear and disappear as the user moves
        // between them; the default one is drawn quiet, because a badge for the
        // state the note is nearly always in should not compete with the two that
        // mean something is different.
        let view = self.editor().read(cx).view();
        // Focus mode gets a badge only while it is on, and names its scope with
        // it, because "dimmed" and "dimmed to the whole section" are different
        // things to see. Unlike the view badge this slot comes and goes: a mode
        // that is nearly always off would only add a word to every status line.
        let focus = if self.editor().read(cx).focus_mode() {
            format!("focus {}", self.editor().read(cx).focus_scope().word())
        } else {
            String::new()
        };
        // The checker's count, so "no underlines" can be told apart from "the
        // checker has not answered yet". Preview draws no underlines at all, so it
        // has nothing to count either.
        let spell = if view == View::Preview {
            String::new()
        } else {
            match self.checker_notes.len() {
                0 => String::new(),
                1 => "1 spellcheck note".into(),
                n => format!("{n} spellcheck notes"),
            }
        };
        // Which match of how many the cursor is on, while a `/` search is up.
        let search = self.search_label();
        // The open note's own word count, live from the buffer. It is state, not a
        // message: it is true for exactly as long as it is shown, and it moves with
        // the typing rather than a save later.
        let words = word_badge(self.note().is_some(), self.editor().read(cx).word_count());

        // The tab bar's rows. Built here rather than in a widget of its own because a
        // tab is not a thing that can be drawn without the window: clicking one moves
        // the keyboard, and closing one writes its note.
        let tabs: Vec<gpui::Stateful<gpui::Div>> = self
            .tab_state(cx)
            .into_iter()
            .enumerate()
            .map(|(index, tab)| {
                let label = tab
                    .path
                    .as_ref()
                    .map(|path| {
                        path.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    })
                    .unwrap_or_else(|| "no note".to_string());
                let entity = cx.entity().clone();
                let close = cx.entity().clone();
                let id = tab.id;
                let active = index == self.tab;
                div()
                    .id(("tab", id))
                    .flex_1()
                    .min_w_0()
                    .h(TAB_ROW)
                    .px_2()
                    .rounded(CHIP_RADIUS)
                    .flex()
                    .items_center()
                    .gap_1()
                    // A tab is a thing to act on, so the pointer says so rather than
                    // offering the caret it inherits from the note.
                    .cursor(gpui::CursorStyle::PointingHand)
                    .text_color(gpui::rgb(if active {
                        self.palette.ui_fg
                    } else {
                        self.palette.ui_dim
                    }))
                    .child(
                        // The same dot the status bar uses, on the tab it belongs to:
                        // with tabs open, a note ahead of its file is not necessarily
                        // the one on screen.
                        div()
                            .text_color(gpui::rgb(self.palette.accent))
                            .child(save_indicator(tab.dirty)),
                    )
                    .child(div().flex_1().min_w_0().text_ellipsis().child(label))
                    .child(
                        div()
                            .id(("tab-close", id))
                            .text_color(gpui::rgb(self.palette.ui_dim))
                            .cursor(gpui::CursorStyle::PointingHand)
                            .child("\u{00d7}")
                            // The `\u{00d7}` is the tab's own click. Without stopping
                            // the propagation the row behind it would take the same
                            // click and switch to the tab it is closing.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(move |_, window, cx| {
                                close.update(cx, |ws, cx| ws.click_tab_close(id, window, cx));
                            }),
                    )
                    .when(active, |d| d.bg(gpui::rgb(self.palette.ui_active_bg)))
                    .when(!active, |d| {
                        d.hover(|s| s.bg(gpui::rgb(self.palette.ui_hover_bg)))
                    })
                    .on_click(move |_, window, cx| {
                        entity.update(cx, |ws, cx| ws.click_tab(id, window, cx));
                    })
            })
            .collect();

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(self.palette.ui_bg))
            .text_color(gpui::rgb(self.palette.ui_fg))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::quit_forced))
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
            .on_action(cx.listener(Self::tab_next))
            .on_action(cx.listener(Self::tab_prev))
            .on_action(cx.listener(Self::tab_new))
            .on_action(cx.listener(Self::tab_close))
            .on_action(cx.listener(Self::tab_only))
            .on_action(cx.listener(Self::quick_open))
            .on_action(cx.listener(Self::jump_back))
            .on_action(cx.listener(Self::jump_forward))
            .on_action(cx.listener(Self::search_vault))
            .on_action(cx.listener(Self::search_buffer))
            .on_action(cx.listener(Self::search_next))
            .on_action(cx.listener(Self::search_prev))
            .on_action(cx.listener(Self::show_note))
            .on_action(cx.listener(Self::show_raw))
            .on_action(cx.listener(Self::show_live))
            .on_action(cx.listener(Self::show_preview))
            .on_action(cx.listener(Self::toggle_focus))
            .on_action(cx.listener(Self::toggle_typewriter))
            .on_action(cx.listener(Self::command_palette))
            .on_action(cx.listener(Self::command_line))
            .on_action(cx.listener(Self::change_theme))
            .on_action(cx.listener(|ws, _: &CheckerOn, _window, cx| ws.set_checker(true, cx)))
            .on_action(cx.listener(|ws, _: &CheckerOff, _window, cx| ws.set_checker(false, cx)))
            .on_action(cx.listener(Self::copy_text))
            .on_action(cx.listener(Self::show_wordcount))
            // Agent edit review (roadmap P2.5a). Handled here rather than on the strip
            // itself because these commands are reachable from three doors at once: the
            // strip's own keys, `]a`/`[a` in the note, and the palette.
            .on_action(cx.listener(Self::review_focus))
            .on_action(cx.listener(Self::review_next))
            .on_action(cx.listener(Self::review_prev))
            .on_action(cx.listener(Self::review_accept))
            .on_action(cx.listener(Self::review_reject))
            .on_action(cx.listener(Self::review_open_note))
            .on_action(cx.listener(Self::review_collapse))
            // Clicking off the picker or the command line closes it. Both float over
            // the note, and a click under them used to belong to nothing at all, so
            // they simply stayed up until `escape`. A click inside a panel never
            // reaches here: the panel occludes everything behind it, this handler
            // among them, and stops the click on top of that.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &gpui::MouseDownEvent, window, cx| {
                    this.dismiss_overlays(window, cx);
                }),
            )
            // The tabs (roadmap P3.1). Top of the window, one row per open note, where
            // a window's tabs already live in every other editor the user has used.
            // It is `flex_none`, so it takes its height and the note takes what is
            // left; the tabs share that width and ellipsis rather than scrolling, which
            // is the part still to do.
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .h(TAB_BAR)
                    .px(TAB_GAP)
                    .flex()
                    .items_center()
                    .gap(TAB_GAP)
                    .text_xs()
                    .bg(gpui::rgb(self.palette.ui_status_bg))
                    .text_color(gpui::rgb(self.palette.ui_dim))
                    .children(tabs),
            )
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
                            // the first and last lines look cut off. The padding is on
                            // this column and its background is the note's, so the air
                            // belongs to the note: the band around the text is the same
                            // colour as the text's own background rather than a strip of
                            // chrome, and the note reads as one rectangle from the
                            // sidebar to the edge of the window.
                            .px_4()
                            .pt_3()
                            .pb_2()
                            .bg(gpui::rgb(self.palette.bg))
                            // The column, not just the text, has to report the
                            // pointer: the space below the last line belongs to no
                            // line, and a panel the pointer walked away downwards
                            // would otherwise never be told to go.
                            .on_mouse_move(cx.listener(
                                |this, event: &gpui::MouseMoveEvent, window, cx| {
                                    this.pointer_moved(event.position, window, cx);
                                },
                            ))
                            .child(div().flex_1().min_h_0().child(self.editor().clone()))
                            // A picker *overlays* the note rather than pushing it
                            // down: it is painted after the text, so opening one does
                            // not re-wrap every line underneath. Like the prompt it is
                            // a sibling of the editor, so the vim contexts stay out of
                            // its way.
                            //
                            // Both overlays occlude what is behind them. gpui's hit
                            // test walks the hitboxes under the pointer from the
                            // topmost down and stops at one that blocks the mouse, so
                            // this is what keeps the note *under* the panel from
                            // answering the pointer: a harper panel used to open
                            // itself underneath the picker — two panels at once, and
                            // neither answering for the other — and the note's IBeam
                            // showed through the panel.
                            //
                            // A click inside a panel stops there too. The window's own
                            // mouse-down handler (below) dismisses an overlay, and it
                            // sits *behind* this one, so the occlusion already keeps it
                            // from seeing a click inside the panel; the explicit stop is
                            // here so that rule does not depend on how the occlusion is
                            // spelled. Either one alone is enough — the test for it
                            // fails only when both are off.
                            .when_some(picker, |d, picker| {
                                d.child(
                                    div()
                                        .absolute()
                                        .top_2()
                                        .left_4()
                                        .right_4()
                                        .occlude()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .child(picker),
                                )
                            })
                            .when_some(bottom_field, |d, field| {
                                d.child(
                                    div()
                                        .absolute()
                                        .bottom_2()
                                        .left_4()
                                        .right_4()
                                        .occlude()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .child(field),
                                )
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
                    .child(format!("{mode}{pending}"))
                    // The view gets its own badge, and it is the only place the view
                    // is named: the command line deliberately leaves the status line
                    // alone, so `:preview` is one accent label rather than the same
                    // word twice. It matters most for preview, which looks almost
                    // exactly like live preview and simply refuses to type — `space l`
                    // or `:live` is how the user gets back. Live is the resting state,
                    // so it is drawn in the chrome's quiet colour rather than the
                    // accent, which is reserved for the two views that change what the
                    // note will do.
                    .child(
                        div()
                            .text_color(gpui::rgb(if view == View::Live {
                                self.palette.ui_dim
                            } else {
                                self.palette.accent
                            }))
                            .child(view.word().to_string()),
                    )
                    .when(!focus.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.accent))
                                .child(focus),
                        )
                    })
                    // The save state gets its own element and the palette's
                    // accent rather than riding along in the mode string: it is
                    // the one part of the bar the user has to read without
                    // looking for it, and it is what tells them an autosave
                    // happened.
                    .when(!dirty.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.accent))
                                .child(dirty),
                        )
                    })
                    .when(!self.status.is_empty(), |d| {
                        // A message only takes a slot when there is one. An empty
                        // child is still a flex item, so leaving it in put a gap on
                        // both sides of nothing and split the bar with a wider space
                        // than anywhere else in it.
                        d.child(self.status.clone())
                    })
                    .when(!spell.is_empty(), |d| d.child(spell))
                    // The search's own readout, so the count does not overwrite the
                    // status line the way a transient message would.
                    .when(!search.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.accent))
                                .child(search),
                        )
                    })
                    .when(!words.is_empty(), |d| {
                        // The count sits next to the cursor position at the right end
                        // of the bar, and in the chrome's quiet colour: it is a number
                        // to glance at, not news, and the accent is reserved for the
                        // segments that mean something is different.
                        d.child(
                            div()
                                .text_color(gpui::rgb(self.palette.ui_dim))
                                .child(words),
                        )
                    })
                    .child(format!("{}:{}", line + 1, col + 1)),
            )
            // The review strip: bottom of the window, above the status bar, and only
            // when something is waiting (roadmap P2.5a). The status bar owns facts
            // about the whole note and is left alone, and the note never shifts once
            // the strip is up: the strip is `flex_none`, and the note column above it
            // takes what is left.
            .when(self.review_active(cx), |d| {
                d.child(
                    div()
                        .flex_none()
                        .w_full()
                        .border_t_1()
                        .border_color(gpui::rgb(self.palette.ui_border))
                        .child(self.review.clone()),
                )
            })
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
