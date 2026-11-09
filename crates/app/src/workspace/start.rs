//! The blank tab's start block.
//!
//! A blank tab used to be an empty buffer: a caret at the top of a void, with
//! every door out of it living only in the docs. The block is the same surface
//! with its doors on it -- what the vault holds, what was open last, and the
//! keys -- and it steps aside the moment the tab starts being a note, so the
//! writing surface is never behind a menu.
//!
//! This amends one ruling in `design-language.md` §13. "Every empty surface
//! gets exactly one line" still holds for an empty *list*: a picker with no
//! matches, a folder with no files. A blank tab is not a list with nothing in
//! it -- it is the front door of the window, and a front door gets a handle.
//! What the ruling still forbids here, and this file honours: no illustration,
//! no second voice, no animation (§7: chrome does not animate), no new
//! elevation (§5: the block sits on the note's own surface, unraised).

use std::path::PathBuf;

use gpui::{div, prelude::*, AnyElement, App, Context};
use shale_core::vault::rel_text;
use shale_core::vim::Mode;

use crate::acts::*;
use crate::style::*;

use super::tabs::TabContent;
use super::Workspace;

/// Recent notes the block lists on screen. Past a screenful the answer is the
/// picker, not a longer menu.
const RECENT_ROWS: usize = 6;

/// `1 note` / `7 notes`. Every other count in the app spells its own number
/// word (§13's unit formats); `1 notes` would be the one place it does not.
fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

impl Workspace {
    /// Whether pane `pix` shows the block: a tab with no note, and only while
    /// it is still that. Text in the buffer or insert mode means the tab is
    /// becoming a note, and the block is gone -- the block never stands
    /// between a thought and the page.
    pub(crate) fn show_start_block(&self, pix: usize, cx: &App) -> bool {
        let TabContent::Note { editor, path, .. } = &self.panes[pix].active_tab().content else {
            return false;
        };
        if path.is_some() {
            return false;
        }
        let editor = editor.read(cx);
        editor.text().is_empty() && editor.mode() == Mode::Normal
    }

    /// The recents the block offers: still on disk, newest first, a screenful.
    /// A note deleted from the vault drops out, because every row here is a
    /// promise that clicking takes you somewhere.
    pub(crate) fn start_recents(&self, cx: &App) -> Vec<PathBuf> {
        self._services
            .read(cx)
            .recent()
            .iter()
            .filter(|path| self.entries.iter().any(|entry| entry.path == **path))
            .take(RECENT_ROWS)
            .cloned()
            .collect()
    }

    /// The body of a row click. The click names its own pane, so a recent
    /// clicked in a pane without the keyboard opens *there* -- the same rule
    /// the tab bar's rows follow for the same reason.
    pub(crate) fn open_recent_in(&mut self, pix: usize, path: &PathBuf, cx: &mut Context<Self>) {
        self.active_pane = pix;
        self.hover_stale = true;
        self.open_note(path, cx);
    }

    /// The block itself. Left-aligned in the note's own column, at the note's
    /// text indent, so it reads as the note's first lines rather than a panel
    /// parked in front of them.
    pub(crate) fn start_block(&self, pix: usize, cx: &mut Context<Self>) -> AnyElement {
        let palette = &self.palette;
        let notes = self.entries.len();

        // The vault line, in the status line's voice: lowercase, no period.
        // Link counts come from the graph index, which is built on demand --
        // until it exists the line says what it actually knows.
        let vault_line = if notes == 0 {
            "the vault holds no notes yet".to_string()
        } else {
            match self.graph.as_ref() {
                Some(index) => format!(
                    "{} · {}",
                    counted(notes, "note", "notes"),
                    counted(index.graph().edge_count(), "link", "links")
                ),
                None => counted(notes, "note", "notes"),
            }
        };

        let recent = self.start_recents(cx);

        let block = div()
            .flex()
            .flex_col()
            .gap(SPACE_2)
            .text_color(gpui::rgb(palette.ui_fg))
            .child(div().child(vault_line));

        if notes > 0 {
            let list = div()
                .flex()
                .flex_col()
                .gap(SPACE_1)
                .when(!recent.is_empty(), |d| {
                    d.child(
                        div()
                            .text_color(gpui::rgb(palette.ui_dim))
                            .text_size(TEXT_SM)
                            .child("recent"),
                    )
                });
            let has_recent = !recent.is_empty();
            let list = recent.into_iter().enumerate().fold(list, |d, (ix, path)| {
                d.child(
                    div()
                        // One id per row per pane: the same `ix` exists in two
                        // panes of a split window, and a shared id would make
                        // the two rows one control.
                        .id(("start-recent", pix * RECENT_ROWS + ix))
                        .cursor_pointer()
                        .px(SPACE_2)
                        .py(SPACE_1)
                        .rounded(CHIP_RADIUS)
                        // A rule on every row, transparent until something
                        // needs it: a border only on hover would shift the rows
                        // below it as the pointer moves (the same no-shift rule
                        // the tree's rows carry).
                        .border_1()
                        .border_color(gpui::rgba(0x0000_0000))
                        .hover(|s| s.bg(gpui::rgb(palette.ui_hover_bg)))
                        .text_color(gpui::rgb(palette.ui_fg))
                        .child(rel_text(&path))
                        .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                            this.open_recent_in(pix, &path, cx);
                        })),
                )
            });
            let block = block.child(list).when(!has_recent, |d| {
                // A vault with notes and no history yet: the line is the
                // one-line empty state the design language asks for, and the
                // legend below is the way in.
                d.child(
                    div()
                        .text_color(gpui::rgb(palette.ui_dim))
                        .child("nothing opened here yet"),
                )
            });
            return self.start_legend(block, notes);
        }

        // The first-run moment: the same screen, one different line, and the
        // one door that matters when there is nothing to open.
        let block = block.child(
            div()
                .id(("start-new", pix))
                .cursor_pointer()
                .w_full()
                .px(SPACE_2)
                .py(SPACE_1)
                .rounded(CHIP_RADIUS)
                .border_1()
                .border_color(gpui::rgba(0x0000_0000))
                .hover(|s| s.bg(gpui::rgb(palette.ui_hover_bg)))
                .text_color(gpui::rgb(palette.ui_fg))
                .child("new note")
                .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                    this.active_pane = pix;
                    this.hover_stale = true;
                    this.new_note(&NewNote, window, cx);
                })),
        );
        self.start_legend(block, notes).into_any_element()
    }

    /// The doors, as key-chips: the sanctioned way to show a key
    /// (`style::key_chip`), the same legend the graph strip and the review
    /// strip use. An empty vault has nothing to open, search, or draw, so it
    /// gets only the door that works.
    fn start_legend(&self, block: gpui::Div, notes: usize) -> AnyElement {
        let palette = &self.palette;
        let legend: &[(&str, &str)] = if notes == 0 {
            &[("ctrl-n", "new note")]
        } else {
            &[
                ("space f", "open"),
                ("ctrl-shift-f", "search"),
                ("space g", "graph"),
                ("ctrl-n", "new note"),
            ]
        };
        block
            .child(div().h_1())
            // The chips follow the surface they sit on: this one is the note's
            // own background, so the chips take the chrome's rule, not a panel's.
            .child(crate::style::key_legend(
                legend,
                palette.ui_border,
                palette.ui_fg,
                palette.ui_dim,
            ))
            .into_any_element()
    }
}
