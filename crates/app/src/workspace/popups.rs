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
//!   opening a note, opening a prompt or a picker, and coming into the preview
//!   view: each drops the popup and refocuses what had the keyboard before it.
//!
//! The hover preview card lives here too, because it is the other thing the pointer
//! opens and the same placement and dismissal rules decide it — with one difference
//! worth naming: the popup takes the keyboard and gives it back, and the card never
//! takes it, so closing a card has nothing to undo.

use gpui::{px, AppContext, Bounds, Context, Entity, Focusable, Pixels, Point, Window};

use crate::acts::ShowNote;
use crate::editor::View;
use crate::lsp::{Diagnostic, Fix};
use crate::popup::{Popup, PopupEvent};

use super::Workspace;

/// How far below the line the panel starts: the spacing scale's first step
/// (`design-language.md` §2).
const GAP: Pixels = crate::style::SPACE_1;

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

    /// vim's `K`: what does the app say about the word the cursor is on?
    ///
    /// Nothing is asked of the server: the notes are already in hand (they are
    /// what drew the wavy underlines), which is what makes this work while the
    /// server is thinking and with no server at all.
    ///
    /// The checker answers **first**. A word with a squiggle under it has a specific
    /// answer, and `K` over it is asking that question; the card — the note a link points
    /// at, the distribution a `!p` line asks for — is the fallback for the word the
    /// checker had nothing to say about. The pointer runs the other way round (a link is
    /// one thing to hover, and the squiggle is already on screen telling you the checker
    /// has something), which is the difference between asking and being told.
    pub(super) fn show_note(&mut self, _: &ShowNote, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompt.is_some() || self.picker.is_some() {
            return;
        }
        // Preview mode shows no checker at all: its underlines are not drawn, and
        // every row a panel offers is an edit the view refuses to make. The graph tab
        // has no note to ask about, and answers the same way.
        let Some(handle) = self.editor().cloned() else {
            return;
        };
        if handle.read(cx).view() == View::Preview {
            return;
        }
        let offset = handle.read_with(cx, |editor, _cx| editor.cursor_offset());
        // The row the caret is on, in the space that row is counted in: inside a
        // block being edited it is a row of the other note, and a panel anchored
        // by number alone would hang it off this window's line of the same number.
        let line = handle.read_with(cx, |editor, _cx| editor.caret_line());
        if self.note_at(offset).is_some() {
            self.open_popup_for(offset, line, false, window, cx);
            return;
        }
        // Nothing marked: ask the other question the same cursor position can raise.
        // `open_card_for` answers nothing when the cursor is on neither a link nor a
        // request line, which is `K`'s long-standing behaviour on a plain word.
        self.open_card_for(offset, line, window, cx);
    }

    /// The pointer moved over the note and landed on buffer offset `offset` of
    /// line `line`, with the pointer itself at the window position `at`.
    ///
    /// `line` carries which note's rows it counts: a row inside the block being
    /// edited belongs to the other buffer, and the panel has to hang off that row.
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
        line: crate::editor::LineRef,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A picker or a prompt owns the window, so the note is not what the pointer
        // is over. The panel that floats over the note occludes the note under it,
        // so a move inside it should not reach this at all; the guard is the same
        // rule stated where it cannot be forgotten, and it also covers a prompt in
        // the sidebar, which does not occlude the note.
        if self.picker.is_some() || self.prompt.is_some() {
            return;
        }
        // Insert mode is typing. A popup that took the keyboard the moment the
        // pointer brushed a word would be worse than no popup at all. Preview mode
        // is reading: it draws no underlines, so a panel appearing over the word
        // the pointer happens to cross would be a surprise with nothing to offer.
        if !self.editor().is_some_and(|editor| {
            editor.read_with(cx, |editor, _cx| {
                editor.is_normal_mode() && editor.view() != View::Preview
            })
        }) {
            // A card that was up when insert mode started is now describing a view that
            // did not ask for it, and one over a preview is a panel over the word the
            // pointer happens to cross with nothing to offer.
            self.close_card(cx);
            return;
        }
        // The card does not swallow the pointer: a click on the text it covers should
        // still put the caret in that text. So the window keeps hearing about moves over
        // it, and this is where the card's own band is accounted for — the same band rule
        // the panel uses, for the same reason. The card hangs below its link, so the way
        // down into it crosses the line underneath, and a reader who settles there to read
        // is not asking a new question.
        let on_card = self.card.is_some() && self.pointer_on_card(at, window, cx);
        // A link is a bigger construct than a marked word written inside it, so the card
        // gets first offer at the pointer: `[[Thier Note]]` is one thing to ask about, not
        // a spelling complaint about half of it. A `!p` line is the same kind of thing —
        // one question, whose answer is a distribution rather than a note.
        let on_surface = self.editor().is_some_and(|editor| {
            editor.read_with(cx, |editor, _cx| {
                editor.link_at(offset).is_some() || editor.request_at(offset).is_some()
            })
        });
        if on_surface && !on_card {
            self.open_card_for(offset, line, window, cx);
            return;
        }
        if on_card {
            return;
        }
        // Off the link and off the card: the card goes. It has no rows to reach for, so
        // unlike the panel it does not have to survive the trip to anything.
        if self.card.is_some() {
            self.close_card(cx);
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

    /// Show the card for the link under the pointer, replacing any card already up.
    ///
    /// The content is asked for once, when the pointer arrives on a new link, rather than
    /// every frame: a card rebuilt on each mouse-move event would re-read the target note
    /// dozens of times a second, and the reader would watch it flicker.
    fn open_card_for(
        &mut self,
        offset: usize,
        line: crate::editor::LineRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Already showing this link: leave it alone.
        if self.card_anchor == Some(line) {
            return;
        }
        let Some(editor) = self.editor() else {
            return;
        };
        let text = editor.read_with(cx, |editor, _cx| editor.preview_at(offset));
        // Where the thing under the pointer begins, so the card hangs under it and not
        // under the first word of the line it happens to be written on.
        let link_start = editor.read_with(cx, |editor, _cx| editor.preview_anchor(offset));
        let Some(text) = text else {
            // A link to a note the vault does not have has nothing to preview. Showing a
            // card for it would promise a note that is not there; the unresolved link's
            // own story is P6.9's, not this card's.
            self.close_card(cx);
            return;
        };
        // `links.hover_aliased` off: a link whose shown text is not the note's name —
        // `[[masked|anon]]`, or `[[anon]]` where a note declares that alias — keeps its
        // card to itself. The card exists to answer "where does this go?", and a note
        // written in aliases would answer it on every other line. Following the link is
        // untouched: the setting is only about the surface that appears unasked.
        if text.aliased && !self.config.links.hover_aliased {
            self.close_card(cx);
            return;
        }
        let (palette, family, size) = self.picker_style(cx);
        // Read the scroll position before the card is stored: `editor` is a handle into
        // the window's state, and assigning the card is a write to that same state.
        let scroll = editor.read_with(cx, |editor, _cx| editor.scroll_y());
        let card = cx.new(|_cx| {
            crate::card::Preview::new(text.title, text.lines, text.hidden, palette, family, size)
        });
        self.card = Some(card);
        self.card_anchor = Some(line);
        self.card_link_start = link_start;
        self.surface_scroll = scroll;
        // One floating thing at a time.
        if self.popup.is_some() {
            self.close_popup(window, cx);
        }
        window.refresh();
    }

    /// Take the card away. It never had the keyboard, so there is nothing to hand back.
    pub(super) fn close_card(&mut self, cx: &mut Context<Self>) {
        if self.card.take().is_none() {
            return;
        }
        self.card_anchor = None;
        self.card_link_start = None;
        self.surface_reask = false;
        cx.notify();
    }

    /// Where the card goes: the same placement as the checker's panel — below the line it
    /// is about, flipped above it near the bottom of the window, kept inside the text
    /// column near the right edge — worked out from the card's own height each frame.
    pub(crate) fn card_place(&self, window: &Window, cx: &gpui::App) -> Option<PlacedPopup> {
        let anchor = self.card_anchor?;
        let editor = self.editor()?;
        let (line, visible) = editor.read_with(cx, |editor, _cx| {
            (
                editor.hit_line_bounds(anchor),
                editor.line_is_visible(anchor),
            )
        });
        let line = line?;
        if !visible {
            return None;
        }
        let card = self.card.as_ref()?;
        let size = card.read_with(cx, |card, _cx| card.panel_size());
        // Under the link, not under the line: a card at the line's left edge names a
        // note in the middle of a sentence and points at the sentence's first word.
        let mut x = self
            .card_link_start
            .and_then(|start| {
                editor.read_with(cx, |editor, _cx| editor.hit_line_x_at(anchor, start))
            })
            .unwrap_or_else(|| line.left());
        if x + size.width > line.right().max(x) {
            x = (line.right() - size.width).max(line.left());
        }
        let below = line.bottom() + GAP + size.height <= window.bounds().bottom() - EDGE_MARGIN;
        let y = if below {
            line.bottom() + GAP
        } else {
            (line.top() - GAP - size.height).max(window.bounds().top() + EDGE_MARGIN)
        };
        Some(PlacedPopup {
            bounds: gpui::Bounds {
                origin: gpui::point(x, y),
                size,
            },
            below,
        })
    }

    /// The note scrolled under a pointer that did not move.
    ///
    /// gpui's wheel handler dirties the view that owns the scroller and no view above it,
    /// so a frame after a wheel tick can draw the note at its new offset without the
    /// window ever rendering — and the window is what places a card or a panel and what
    /// re-runs the pointer's collision. This makes the window take part in the frame the
    /// scroll asked for; the render notices the offset moved and queues the re-ask.
    pub(super) fn note_scrolled(&mut self, cx: &mut Context<Self>) {
        #[cfg(test)]
        {
            self.scroll_reports += 1;
        }
        if self.card.is_none() && !(self.popup.is_some() && self.popup_from_pointer) {
            return;
        }
        cx.notify();
    }

    /// Ask the pointer's question again against the current paint.
    ///
    /// Scrolling is the case: the pointer did not move, so gpui reports no move, and a
    /// surface is left describing a word the note has slid out from under it. The frame
    /// that scrolled has already placed the surface at its word's new position, so this is
    /// the second half of the rule: the pointer, which did not move, is asked what it is on
    /// now. A pointer that ends up on the surface, or on the word it is about, keeps it;
    /// one that lands on other text ends it; one that lands on a different marked word or
    /// link moves the surface to that.
    /// The frame after the note scrolled: ask the pointer where the surface's word
    /// went, and mark the window dirty so this frame also *places* the surface again.
    ///
    /// The placement half is not a formality. A surface is placed from the note's
    /// last painted line positions, and within a frame the window renders before the
    /// note paints — so the frame that noticed the scroll placed the surface where
    /// its line *used to be*, and a surface that survives the collision keeps that
    /// stale place until something else redraws the window. This callback runs at the
    /// start of the frame after that paint, so its records are the scrolled ones.
    pub(super) fn surface_scroll_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reask_pointer(window, cx);
        #[cfg(test)]
        {
            self.surface_replace += 1;
        }
        cx.notify();
    }

    pub(super) fn reask_pointer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor() else {
            return;
        };
        // The question is answered from the painted rows of the note itself, which cannot
        // speak for a surface anchored inside an entered block.
        let anchored_in_block = matches!(
            (self.card_anchor, self.popup_anchor),
            (Some(crate::editor::LineRef::Embed(_)), _)
                | (_, Some(crate::editor::LineRef::Embed(_)))
        );
        if anchored_in_block {
            return;
        }
        let at = window.mouse_position();
        match editor.read_with(cx, |editor, _cx| editor.hover_under(at)) {
            Some((offset, line)) => self.pointer_hovered(offset, line, at, window, cx),
            None => {
                self.close_card(cx);
                if self.popup_from_pointer {
                    self.close_popup(window, cx);
                }
            }
        }
    }

    /// How many wheel scrolls the note has reported, for the tests.
    #[cfg(test)]
    pub(crate) fn scroll_reports(&self) -> usize {
        self.scroll_reports
    }

    #[cfg(test)]
    pub(crate) fn surface_replace(&self) -> usize {
        self.surface_replace
    }

    /// How far the note had scrolled the last time a floating surface asked about the
    /// pointer, for the tests: it only moves inside `render`, so it also says whether the
    /// window took part in a frame at all.
    #[cfg(test)]
    pub(crate) fn surface_scroll(&self) -> f32 {
        self.surface_scroll
    }

    /// What the card says, for the tests: the title, the lines, and what it left out.
    #[cfg(test)]
    pub(crate) fn card_shape(&self, cx: &gpui::App) -> Option<(String, Vec<String>, usize)> {
        let card = self.card.as_ref()?;
        Some(card.read_with(cx, |card, _cx| card.shape()))
    }

    /// Whether the pointer is at the height the card occupies — the same band rule the
    /// panel uses, for the same reason: the card swallows the moves over itself, and what
    /// the window hears about is the strip between the link and the card.
    fn pointer_on_card(&self, at: Point<Pixels>, window: &Window, cx: &gpui::App) -> bool {
        let Some(placed) = self.card_place(window, cx) else {
            return false;
        };
        let slack = GAP + px(3.);
        at.y >= placed.bounds.origin.y - (GAP + slack)
            && at.y <= placed.bounds.bottom() + slack
            && at.x >= placed.bounds.left() - slack
            && at.x <= placed.bounds.right() + slack
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
        if at.y < placed.bounds.origin.y - (GAP + slack) || at.y > placed.bounds.bottom() + slack {
            return false;
        }
        // The column counts too. A band across the whole window meant the panel survived
        // a pointer anywhere at its height — the far side of the note included, which is
        // not on the way to anything. What is on the way: the panel's own span, and the
        // word's column, which is where the pointer came from. A word near the right end
        // of a long line sits beside the panel rather than above it, and walking straight
        // down from it is still the way to a row.
        let in_panel =
            at.x >= placed.bounds.left() - slack && at.x <= placed.bounds.right() + slack;
        let in_word = self
            .word_span(cx)
            .is_some_and(|(left, right)| at.x >= left - slack && at.x <= right + slack);
        in_panel || in_word
    }

    /// The window x range the popup's marked word was painted across.
    fn word_span(&self, cx: &gpui::App) -> Option<(Pixels, Pixels)> {
        let anchor = self.popup_anchor?;
        let (start, end) = self.popup_note?;
        let editor = self.editor()?;
        let left = editor.read_with(cx, |editor, _cx| editor.hit_line_x_at(anchor, start))?;
        let right = editor.read_with(cx, |editor, _cx| editor.hit_line_x_at(anchor, end))?;
        Some((left.min(right), left.max(right)))
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
    /// line `line`, and point the keyboard at it.
    fn open_popup_for(
        &mut self,
        offset: usize,
        line: crate::editor::LineRef,
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
        if let Some(active) = self.editor() {
            self.surface_scroll = active.read_with(cx, |editor, _cx| editor.scroll_y());
        }
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
            None => self.active_focus(cx),
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
        // The keyboard has to land somewhere. Whatever had it before the panel is
        // the first answer; if the panel took it from nothing at all, the note on
        // screen is. A panel dropped with the keyboard still pointed at it is the
        // whole failure this path exists to avoid, and it is total: no key, in any
        // context, reaches anything.
        let focus = self
            .popup_return_focus
            .take()
            .or_else(|| Some(self.active_focus(cx)));
        if let Some(focus) = focus {
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
            let applied = self.editor().is_some_and(|editor| {
                editor.update(cx, |editor, cx| editor.apply_edits(&fix.edits, cx))
            });
            self.say(
                if applied {
                    "applied the checker's fix"
                } else {
                    "the note changed under that fix"
                },
                cx,
            );
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
        let editor = self.editor()?;
        let (line, size, visible) = editor.read_with(cx, |editor, _cx| {
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
