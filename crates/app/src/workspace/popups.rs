//! The window's popups: one at a time, anchored to the note, and closed properly.
//!
//! A popup is the checker's note about one word, in the checker's own words, with
//! the things it can do about it underneath. Both ways of asking for one go
//! through [`Workspace::open_popup_for`], and every way out goes through
//! [`Workspace::close_popup`], which is the only place the keyboard is handed
//! back. That is what keeps the two rules the user cares about true:
//!
//! - **They never stack.** The window has one popup slot, so opening a second
//!   popup replaces the first rather than laying it on top. A prompt or a picker
//!   — which own the keyboard and are modal in a way a popup is not — closes the
//!   popup on the way in, and no popup opens while one of them is up.
//! - **They close.** `escape`, choosing a row, the pointer leaving the word, the
//!   pointer walking off the bottom of the panel, an edit under the cursor,
//!   opening a note, opening a prompt or a picker: each drops the popup and
//!   refocuses what had the keyboard before it.

use gpui::{px, AppContext, Bounds, Context, Entity, Focusable, Pixels, Point, Window};

use crate::acts::ShowNote;
use crate::lsp::{Diagnostic, Fix};
use crate::popup::{Popup, PopupEvent};

use super::Workspace;

/// How far below the line the panel starts.
const GAP: Pixels = px(4.);

/// How far past the panel's outer edge the pointer may go before the panel is
/// dismissed.
///
/// The pointer has to cross the gap between the word and the panel to reach a
/// row, so the edge cannot be exact; but a panel that survives the pointer
/// walking away from it survives forever, because the panel swallows the moves
/// over its own rows and there is nothing under it to report one. The note
/// column's mouse-move handler ([`Workspace::pointer_moved`]) is what notices.
const DISMISS_LEEWAY: Pixels = px(14.);

/// How far from the window's own edge a panel may be drawn.
const EDGE_MARGIN: Pixels = px(8.);

/// Where the panel is, and which side of its line it hangs on.
///
/// The side matters because the edge that ends a panel is the one *away* from the
/// note: a panel that flipped above its line is walked away from upwards, and
/// watching its bottom edge would close it the moment the pointer moved down onto
/// its own word.
#[derive(Debug, PartialEq)]
pub(crate) struct PlacedPopup {
    /// The panel's bounds, in window coordinates.
    pub(crate) bounds: Bounds<Pixels>,
    /// Whether the panel hangs below the line it belongs to.
    pub(crate) below: bool,
}

impl Workspace {
    // ----- asking for a popup -------------------------------------------

    /// vim's `K`: what does the checker say about the word the cursor is on?
    ///
    /// Nothing is asked of the server: the notes are already in hand (they are
    /// what drew the wavy underlines), which is what makes this work while the
    /// server is thinking and with no server at all.
    pub(super) fn show_note(&mut self, _: &ShowNote, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompt.is_some() || self.picker.is_some() {
            return;
        }
        let offset = self
            .editor
            .read_with(cx, |editor, _cx| editor.cursor_offset());
        let line = self
            .editor
            .read_with(cx, |editor, _cx| editor.display_position().0);
        self.open_popup_for(offset, line, false, window, cx);
    }

    /// The pointer moved over the note and landed on buffer offset `offset` of
    /// display line `line`, with the pointer itself at the window position `at`.
    ///
    /// Hovering is a request, not a command: it only opens a popup when the
    /// pointer is over a word the checker marked, and it only closes one it
    /// opened itself. A popup asked for with `K` is the user asking, and moving
    /// the mouse is not their answer to it. And a popup the pointer opened keeps
    /// the height it occupies to itself, so the walk from its word down to one of
    /// its rows survives passing over other marked words on the way.
    pub(super) fn pointer_hovered(
        &mut self,
        offset: usize,
        line: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Insert mode is typing. A popup that took the keyboard the moment the
        // pointer brushed a word would be worse than no popup at all.
        if !self
            .editor
            .read_with(cx, |editor, _cx| editor.is_normal_mode())
        {
            return;
        }
        let note = self.note_at(offset);
        let Some(note) = note else {
            // Nothing marked under the pointer. That is usually the end of the
            // panel's subject — unless the pointer is down at the panel's own
            // height, which is the trip from the word to one of its rows. Closing
            // there would make the rows unclickable, and being clickable is the
            // whole point of having them. The panel's place is worked out again
            // rather than read off the last frame, because near the bottom of the
            // window it is drawn above the line instead.
            if self.popup_from_pointer && !self.pointer_on_panel(at, window, cx) {
                self.close_popup(window, cx);
            }
            return;
        };
        // Already showing exactly this note: leave it alone, so the panel does not
        // reopen on every mouse-move event.
        if self.popup_note == Some((note.start, note.end)) {
            return;
        }
        // A popup asked for with `K` is the user asking; brushing past another
        // marked word on the way by is not their answer to it, so it is left
        // alone. A popup the pointer opened follows the pointer.
        if self.popup.is_some() && !self.popup_from_pointer {
            return;
        }
        // ...but only as far as the panel's own height. The panel hangs below its
        // word, so the way down to one of its rows crosses the line underneath,
        // and when that line holds another marked word the pointer lands on it
        // mid-trip. Reopening there moves the panel out from under the row the
        // user is reaching for, which is the bug this guards: inside the band the
        // panel occupies, the pointer is still travelling to a row rather than
        // asking a new question.
        if self.popup_from_pointer && self.pointer_on_panel(at, window, cx) {
            return;
        }
        self.open_popup_for(offset, line, true, window, cx);
    }

    /// Whether the pointer is at the height the panel occupies.
    ///
    /// The panel swallows the mouse-move events over its own rows, so the window
    /// never hears about the pointer once it is on top of them; what it does hear
    /// about is the strip between the word and the panel, which belongs to the line
    /// below. Only the height is compared, not the column: a marked word near the
    /// right end of a long line sits above the space beside the panel, and a user
    /// walking straight down from it is on their way to a row just as much as
    /// someone above the panel's own text.
    ///
    /// Two questions are answered by this one band: whether a move over unmarked
    /// text is still inside the panel (keep it), and whether a move that lands on
    /// *another* marked word is a new question or the same trip (see
    /// [`Workspace::pointer_hovered`]).
    fn pointer_on_panel(&self, at: Point<Pixels>, window: &Window, cx: &gpui::App) -> bool {
        let Some(placed) = self.popup_place(window, cx) else {
            return false;
        };
        let slack = GAP + px(3.);
        at.y >= placed.bounds.origin.y - (GAP + slack) && at.y <= placed.bounds.bottom() + slack
    }

    /// The pointer moved anywhere in the note column, including the empty space
    /// below the last line, where no line reports a hover.
    ///
    /// This is the only thing that ends a panel the pointer walked away from
    /// *downwards*: the panel itself swallows the moves over its rows, and once
    /// the pointer is below it there may be no text there to report the move, so a
    /// panel left like that never goes away. Anything past the panel's outer edge
    /// — the edge away from the note, which is its top when it had to be drawn
    /// above its line — plus [`DISMISS_LEEWAY`] is a way away from it.
    ///
    /// Only that one edge is decided here. Everywhere else the line the pointer is
    /// over answers (see [`Workspace::pointer_hovered`]), which is what lets a
    /// panel follow the pointer from word to word.
    pub(super) fn pointer_moved(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.popup_from_pointer {
            return;
        }
        // Worked out again rather than read off the last frame: the pointer may
        // have just moved onto another marked word, which moved the panel with it,
        // and a stale place would dismiss the panel the pointer is now on.
        let Some(placed) = self.popup_place(window, cx) else {
            return;
        };
        let away = if placed.below {
            at.y > placed.bounds.bottom() + DISMISS_LEEWAY
        } else {
            at.y < placed.bounds.top() - DISMISS_LEEWAY
        };
        if away {
            self.close_popup(window, cx);
        }
    }

    /// The checker's note that covers `offset`, if there is one.
    ///
    /// The end is inclusive so a cursor sitting just past a word still counts as
    /// being on it, which is how vim's `K` behaves. Grammar rules and spelling
    /// notes can overlap, so the tightest note wins: the more specific one is the
    /// one the user meant.
    fn note_at(&self, offset: usize) -> Option<&Diagnostic> {
        self.checker_notes
            .iter()
            .filter(|note| note.start <= offset && offset <= note.end)
            .min_by_key(|note| note.end.saturating_sub(note.start))
    }

    /// Put a popup about the note covering `offset` on screen, anchored below
    /// display line `line`, and point the keyboard at it.
    fn open_popup_for(
        &mut self,
        offset: usize,
        line: usize,
        from_pointer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(note) = self.note_at(offset).cloned() else {
            return;
        };
        let (palette, family, size) = self.picker_style(cx);
        let popup = cx.new(|cx| {
            Popup::new(
                note.message.clone(),
                note.code.clone(),
                palette,
                family,
                size,
                cx,
            )
        });
        self._popup_events = Some(cx.subscribe_in(
            &popup,
            window,
            |ws, _popup, event, window, cx| match event {
                PopupEvent::Apply(row) => ws.apply_popup_row(*row, window, cx),
                PopupEvent::Cancel => ws.close_popup(window, cx),
            },
        ));
        // Ask the checker what can be done about this word. The answer arrives
        // through the checker's own observer, which fills the rows in; until then
        // the panel shows the complaint, which is the part the user asked for.
        self.popup_fixes = Vec::new();
        if let Some(spell) = self.spell.clone() {
            spell.update(cx, |spell, cx| spell.fixes_at(note.start, note.end, cx));
        }
        self.popup_return_focus = window.focused(cx);
        self.popup = Some(popup.clone());
        self.popup_anchor = Some(line);
        self.popup_note = Some((note.start, note.end));
        self.popup_from_pointer = from_pointer;
        window.focus(&popup.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    // ----- getting out of one -------------------------------------------

    /// Drop the popup and hand the keyboard back to whatever had it.
    pub(super) fn close_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.take().is_none() {
            return;
        }
        self._popup_events = None;
        self.popup_anchor = None;
        self.popup_note = None;
        self.popup_from_pointer = false;
        self.popup_fixes.clear();
        let focus = match self.popup_return_focus.take() {
            Some(focus) => focus,
            None => self.editor.read(cx).focus_handle.clone(),
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Drop the popup when there is no window to hand the keyboard back with —
    /// the file watcher's path, which runs outside a frame.
    ///
    /// The panel goes away at once; the refocus is remembered and done by the next
    /// frame, which is the first place there is a window to ask. A popup left on
    /// screen with the keyboard stuck to it is the failure this avoids.
    pub(super) fn drop_popup(&mut self, cx: &mut Context<Self>) {
        if self.popup.take().is_none() {
            return;
        }
        self._popup_events = None;
        self.popup_anchor = None;
        self.popup_note = None;
        self.popup_from_pointer = false;
        self.popup_fixes.clear();
        if let Some(focus) = self.popup_return_focus.take() {
            self.pending_focus = Some(focus);
        }
        cx.notify();
    }

    /// The row `enter`, a digit, or a click chose: what the checker offered, in
    /// the order it offered it.
    fn apply_popup_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(fix) = self.popup_fixes.get(row).cloned() else {
            self.close_popup(window, cx);
            return;
        };
        // The edit first, then the command: an offer to replace a word means the
        // replacement, and the command is the server remembering it (which is also
        // what makes the new text come back marked clean).
        if !fix.edits.is_empty() {
            let applied = self
                .editor
                .update(cx, |editor, cx| editor.apply_edits(&fix.edits, cx));
            self.status = if applied {
                "applied the checker's fix".into()
            } else {
                "the note changed under that fix".into()
            };
        }
        if let Some((command, args)) = &fix.command {
            if let Some(spell) = self.spell.clone() {
                spell.update(cx, |spell, cx| spell.run(command, args, cx));
            }
        }
        self.close_popup(window, cx);
    }

    // ----- the popup's contents -----------------------------------------

    /// The rows of the popup that is up, from the checker's latest answer. Called
    /// whenever the checker notifies, so a popup that opens before the server has
    /// thought fills itself in when it does.
    pub(super) fn refresh_popup_rows(&mut self, cx: &mut Context<Self>) {
        let Some(popup) = self.popup.clone() else {
            return;
        };
        // The word the panel is about may have been fixed by hand while it was up.
        // With nothing marked there any more, the panel has nothing left to say.
        let marked = self
            .popup_note
            .is_some_and(|(start, _)| self.checker_notes.iter().any(|note| note.start == start));
        if !marked {
            self.drop_popup(cx);
            return;
        }
        let fixes: Vec<Fix> = match self.spell.as_ref() {
            Some(spell) => spell.read_with(cx, |spell, _cx| spell.fixes().to_vec()),
            None => Vec::new(),
        };
        let rows: Vec<String> = fixes.iter().map(|fix| fix.title.clone()).collect();
        self.popup_fixes = fixes;
        popup.update(cx, |popup, cx| popup.set_rows(rows, cx));
    }

    /// Where the popup panel goes: its bounds in window coordinates and which side
    /// of its line it hangs on, or `None` when there is no popup or the line it
    /// belongs to is not on screen.
    ///
    /// Asked each frame, and answered from the line's bounds *as they were painted*,
    /// which is what lets a popup follow the note as it scrolls and vanish when its
    /// line scrolls away. The render draws the panel here and hands the same bounds
    /// back to the hover path, so "where the panel is" is one answer rather than two
    /// that can disagree.
    ///
    /// The panel hangs below its line, because that is where the pointer came from.
    /// Near the bottom of the window there is no room there, and a panel that runs
    /// off the edge has rows nobody can reach — so it goes above the line instead.
    /// That is also why the render's `snap_to_window` is only a last resort: the
    /// place is already inside the window, and a snap the corridor does not know
    /// about is a corridor over empty space.
    pub(crate) fn popup_place(&self, window: &Window, cx: &gpui::App) -> Option<PlacedPopup> {
        let anchor = self.popup_anchor?;
        let (line, size, visible) = self.editor.read_with(cx, |editor, _cx| {
            (
                editor.hit_line_bounds(anchor),
                editor.font_size(),
                editor.line_is_visible(anchor),
            )
        });
        let line = line?;
        if !visible {
            return None;
        }
        // The panel's height comes from the panel itself: how many offer rows it
        // is drawing. Asking it, rather than counting the window's own copy of the
        // fixes, is what keeps the click corridor on the rows the user can see.
        let rows = self
            .popup
            .as_ref()
            .map(|popup| popup.read_with(cx, |popup, _cx| popup.drawn().len()))
            .unwrap_or(0);
        let size = crate::popup::panel_size(rows, size);
        let mut x = line.left();
        if x + size.width > line.right().max(x) {
            // A note near the right edge of a narrow window: keep the panel inside
            // the text column rather than off the side of the window.
            x = (line.right() - size.width).max(line.left());
        }
        let below = line.bottom() + GAP + size.height <= window.bounds().bottom() - EDGE_MARGIN;
        let y = if below {
            line.bottom() + GAP
        } else {
            // Above the line. Clamped to the window rather than left off the top:
            // a panel with nowhere to go is still better than unreachable rows,
            // and the note scrolls, so this is a momentary state.
            (line.top() - GAP - size.height).max(EDGE_MARGIN)
        };
        Some(PlacedPopup {
            bounds: Bounds::new(Point::new(x, y), size),
            below,
        })
    }

    /// The popup entity, for the render.
    pub(super) fn popup_entity(&self) -> Option<Entity<Popup>> {
        self.popup.clone()
    }
}
