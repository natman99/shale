//! The review strip: the agent edits that are waiting for the user (roadmap P2.5a).
//!
//! It sits at the bottom of the window, **above the status bar**, and only exists when
//! the queue is non-empty. The status bar owns facts about the whole note and stays
//! untouched; the note never shifts under the strip; and a line of text at the bottom
//! of the window is where vim users already look for news about the buffer.
//!
//! One row per pending hunk, because the decision the user makes is per hunk: accept
//! this change, reject that one. A collapsed edit is one row for all of its hunks,
//! which is what makes a twelve-hunk proposal navigable.
//!
//! **The keys are two layers** (ruled in brainstorm.md). While the strip has the
//! keyboard, bare `j`/`k` hop the marker, `y`/`x` accept and reject, `o` opens the
//! note, `za` collapses the edit, and `escape` hands the keyboard back. While the note
//! has the keyboard, only `]a`/`[a` reach the marker — so `y` and `x` never shadow
//! yank and delete in prose. Every one of them is a registry command; the window runs
//! them, and this widget never learns what accepting a hunk *does*, exactly as the
//! popup never learns what a checker's fix is.
//!
//! The queue itself is the files in `.shale/pending-edits/` (see
//! [`shale_core::pending`]); what is here is the window's view of them, pushed in by
//! [`crate::workspace`] on a beat.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    div, prelude::*, px, App, Context, CursorStyle, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, SharedString, StatefulInteractiveElement, Styled,
    Window,
};
use shale_core::pending::PendingEdit;
use shale_core::theme::Palette;

/// The key context the strip's bare keys live in. Like the tree, the prompt and the
/// pickers it is a sibling of the note, so the vim contexts cannot swallow the keys
/// that drive it — and its own keys cannot swallow vim's.
pub(crate) const CONTEXT: &str = "review";

/// The height of one row of the strip. The same order as the status bar's, so the two
/// read as one area at the bottom of the window rather than two widgets.
const ROW: Pixels = px(20.);
/// Space between the strip's edge and its rows: the spacing scale's first step
/// (`design-language.md` §2), the same one a picker row and a file-tree row take.
const PAD: Pixels = crate::style::SPACE_1;
/// How many hunk rows the strip draws at once.
///
/// More than that and the strip is a panel that pushes the note off screen, which is
/// the opposite of what a command-area-style strip is for. [`crate::list::window`]
/// slides the window to keep the marker on it, which is the strip's scrolling: a
/// twelve-hunk proposal is still six rows, and `j` walks it.
///
/// `pub(crate)` so the tests can assert against the cap rather than a copy of it.
pub(crate) const VISIBLE_ROWS: usize = 6;

/// One row of the strip: one hunk, or one collapsed edit shown as a single row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Row {
    /// Which edit in the queue this row belongs to.
    pub(crate) edit: usize,
    /// Which hunk of it, or `None` for a collapsed edit's summary row — which is the
    /// edit as a whole, and is what `y`/`x` act on together.
    pub(crate) hunk: Option<usize>,
}

/// The strip's state: what the queue holds, how each hunk stands, and where the
/// keyboard's marker is.
pub(crate) struct Review {
    focus_handle: FocusHandle,
    /// The queue, oldest first, exactly as the files read back.
    edits: Vec<PendingEdit>,
    /// Which hunks no longer fit the note they were proposed against, as
    /// `(edit, hunk)`. Accepting one does nothing but say so; the moves left are
    /// reject, or let the agent re-propose.
    stale: HashSet<(usize, usize)>,
    /// Which hunks overlap another pending hunk on the same note. Both may apply
    /// cleanly one after the other — and the second goes stale when the first lands —
    /// so this is a warning rather than a verdict.
    clash: HashSet<(usize, usize)>,
    /// The edits drawn as one row rather than one row per hunk, by **id**: the queue
    /// is re-read on a beat, and an index would collapse a different edit each time
    /// one above it was accepted.
    collapsed: HashSet<String>,
    /// The row the keyboard is on, counted over [`Review::rows`].
    current: usize,
    /// The colours the strip is drawn with, from the window.
    palette: Rc<Palette>,
}

impl Review {
    pub(crate) fn new(palette: Rc<Palette>, cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            edits: Vec::new(),
            stale: HashSet::new(),
            clash: HashSet::new(),
            collapsed: HashSet::new(),
            current: 0,
            palette,
        }
    }

    /// Replace what the strip shows with what the queue now holds.
    ///
    /// The marker follows the hunk it was on when that hunk is still pending: the
    /// queue is re-read from disk on a beat, and a marker that jumped because a file
    /// above it was accepted would send the next `y` to a hunk the user was never
    /// looking at. Returns whether anything the user can see changed, so the window
    /// repaints only when there is something new to see.
    pub(crate) fn set_queue(
        &mut self,
        edits: Vec<PendingEdit>,
        stale: HashSet<(usize, usize)>,
        clash: HashSet<(usize, usize)>,
        cx: &mut Context<Self>,
    ) -> bool {
        let was = self.marked();
        let same = edits == self.edits && stale == self.stale && clash == self.clash;
        self.edits = edits;
        self.stale = stale;
        self.clash = clash;
        if !same {
            self.restore_marker(was.as_ref(), cx);
        }
        !same
    }

    /// Whether there is nothing waiting, which is when the strip is not drawn at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// How many hunks are waiting, for the strip's own header.
    pub(crate) fn hunk_count(&self) -> usize {
        self.edits.iter().map(|edit| edit.hunks.len()).sum()
    }

    /// The edit a row belongs to.
    pub(crate) fn edit(&self, row: &Row) -> &PendingEdit {
        &self.edits[row.edit]
    }

    /// Whether a hunk's context no longer matches the note.
    pub(crate) fn is_stale(&self, edit: usize, hunk: usize) -> bool {
        self.stale.contains(&(edit, hunk))
    }

    /// Whether another pending hunk is proposed over the same lines.
    pub(crate) fn is_clashing(&self, edit: usize, hunk: usize) -> bool {
        self.clash.contains(&(edit, hunk))
    }

    /// The rows the strip draws: one per hunk, or one for a whole collapsed edit.
    ///
    /// An edit with no hunks at all — a hand-edited queue file, which is a thing that
    /// happens — still gets a row, because a proposal the user cannot see is a proposal
    /// they cannot get rid of.
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (edit, pending) in self.edits.iter().enumerate() {
            if pending.hunks.is_empty() || self.collapsed.contains(&pending.id) {
                rows.push(Row { edit, hunk: None });
                continue;
            }
            for hunk in 0..pending.hunks.len() {
                rows.push(Row {
                    edit,
                    hunk: Some(hunk),
                });
            }
        }
        rows
    }

    /// Which rows the strip draws, as the window [`crate::list`] puts around the
    /// marker. The render draws these, and the strip's height is their count, so the
    /// marker is never off the strip the user is looking at.
    pub(crate) fn drawn(&self) -> std::ops::Range<usize> {
        let rows = self.rows().len();
        crate::list::window(self.current, rows, rows.min(VISIBLE_ROWS))
    }

    /// The row the marker is on, if there is one.
    pub(crate) fn current_row(&self) -> Option<Row> {
        self.rows().into_iter().nth(self.current)
    }

    /// Move the marker, wrapping at the ends: a queue is usually a handful of rows,
    /// and wrapping is quicker than counting them.
    pub(crate) fn move_marker(&mut self, by: isize, cx: &mut Context<Self>) {
        let rows = self.rows().len();
        if rows == 0 {
            self.current = 0;
            return;
        }
        self.current = (self.current as isize + by).rem_euclid(rows as isize) as usize;
        cx.notify();
    }

    /// The pointer's row becomes the marked one, so the mouse and the keyboard always
    /// agree about what `y` would do.
    pub(crate) fn select_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if row != self.current && row < self.rows().len() {
            self.current = row;
            cx.notify();
        }
    }

    /// Collapse or expand the edit the marker is on: one row for the whole edit
    /// instead of one per hunk.
    pub(crate) fn toggle_collapse(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.current_row() else {
            return;
        };
        let id = self.edits[row.edit].id.clone();
        if !self.collapsed.remove(&id) {
            self.collapsed.insert(id);
        }
        self.current = self.current.min(self.rows().len().saturating_sub(1));
        cx.notify();
    }

    /// What the marker is on, as `(edit id, hunk)`, which is what survives the queue
    /// being re-read under it.
    fn marked(&self) -> Option<(String, Option<usize>)> {
        let row = self.current_row()?;
        Some((self.edits[row.edit].id.clone(), row.hunk))
    }

    /// Put the marker back where it was after the queue changed: the same hunk of the
    /// same edit where it still exists, otherwise the nearest row that does.
    fn restore_marker(&mut self, was: Option<&(String, Option<usize>)>, cx: &mut Context<Self>) {
        let rows = self.rows();
        if let Some((id, hunk)) = was {
            let found = rows
                .iter()
                .position(|row| &self.edits[row.edit].id == id && row.hunk == *hunk);
            if let Some(index) = found {
                self.current = index;
                cx.notify();
                return;
            }
        }
        self.current = self.current.min(rows.len().saturating_sub(1));
        cx.notify();
    }
}

impl Focusable for Review {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Review {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Rc::clone(&self.palette);
        let entity = cx.entity();
        let rows = self.rows();
        let drawn = self.drawn();

        let cells: Vec<gpui::Stateful<gpui::Div>> = drawn
            .clone()
            .map(|n| {
                let row = rows[n];
                let clicked = entity.clone();
                let (label, badge, badge_color) = self.row_text(row);
                let is_marked = n == self.current;
                div()
                    .id(("review-row", n))
                    .h(ROW)
                    .px_1()
                    .flex()
                    .gap_2()
                    .text_ellipsis()
                    // The strip is a thing to act on, so the pointer should say so
                    // rather than offer the caret it inherits from the window.
                    .cursor(CursorStyle::PointingHand)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(gpui::StyledText::new(label)),
                    )
                    .when(!badge.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(gpui::rgb(badge_color))
                                .child(gpui::StyledText::new(badge)),
                        )
                    })
                    .when(is_marked, |d| d.bg(gpui::rgb(palette.ui_active_bg)))
                    .when(!is_marked, |d| {
                        d.hover(|s| s.bg(gpui::rgb(palette.ui_hover_bg)))
                    })
                    // A marked row is a chip, as a file-tree row and a picker row
                    // are: the strip pads its rows, so the corner shows against the
                    // strip's own background.
                    .rounded(crate::style::CHIP_RADIUS)
                    .on_click(move |_, window, cx| {
                        // The strip takes the keyboard on a click, so the bare keys
                        // work straight after pointing at a row.
                        let handle = clicked.read(cx).focus_handle.clone();
                        window.focus(&handle, cx);
                        clicked.update(cx, |review, cx| review.select_row(n, cx));
                    })
            })
            .collect();

        let hunks = self.hunk_count();
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .w_full()
            .px(PAD)
            .py(PAD)
            .text_size(crate::style::TEXT_XS)
            .bg(gpui::rgb(palette.ui_status_bg))
            .text_color(gpui::rgb(palette.ui_fg))
            .child(
                div()
                    .h(ROW)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_color(gpui::rgb(palette.accent))
                            .text_ellipsis()
                            .child(format!(
                                "{hunks} pending {} from {} {}",
                                if hunks == 1 { "hunk" } else { "hunks" },
                                self.edits.len(),
                                if self.edits.len() == 1 {
                                    "proposal"
                                } else {
                                    "proposals"
                                }
                            )),
                    )
                    // The keys are named where the user is already looking, because
                    // the strip only appears when there is something to review and
                    // nobody has read the docs by then.
                    .child(
                        div()
                            .text_color(gpui::rgb(palette.ui_dim))
                            .text_ellipsis()
                            .child("j/k hop · y accept · x reject · o note · za collapse"),
                    ),
            )
            .child(div().children(cells))
    }
}

impl Review {
    /// One row's text, its badge, and the colour that badge is drawn in.
    ///
    /// Split out so the render above stays about layout and the words a row is made of
    /// can be read in one place.
    fn row_text(&self, row: Row) -> (SharedString, SharedString, u32) {
        let edit = &self.edits[row.edit];
        let note = edit.note.display().to_string();
        let Some(index) = row.hunk else {
            // A collapsed edit: the note, who proposed it, and how much is waiting.
            return (
                format!("{note} · {} · {} hunks", edit.profile, edit.hunks.len()).into(),
                "collapsed".into(),
                self.palette.ui_dim,
            );
        };
        let hunk = &edit.hunks[index];
        let label = format!(
            "{note} · {} · line {} · −{} +{}",
            edit.profile,
            hunk.old_start,
            hunk.removed(),
            hunk.added()
        );
        // Stale is the harder state — the hunk cannot be applied at all — so it wins
        // the badge when a hunk is both.
        if self.is_stale(row.edit, index) {
            return (label.into(), "stale".into(), self.palette.callout_failure);
        }
        if self.is_clashing(row.edit, index) {
            return (label.into(), "clash".into(), self.palette.callout_warning);
        }
        (label.into(), "".into(), self.palette.ui_dim)
    }
}
