//! The graph tab's filter strip (roadmap P3.4 Task 8): the three chips, the search
//! line, and the field's side of gpui's input handler.
//!
//! The strip is a sibling of the canvas rather than a child of the element that carries
//! the `graph` key context, and that is the whole design: gpui's context stack is the
//! focused element's ancestors, so a field inside the canvas's context could never
//! contain an `h`, a `j`, an `f` or a `+`. It is also why the strip has its own focus
//! handle and its own context name.
//!
//! What is *not* here is the filter itself. The window owns the [`GraphFilter`] and the
//! mask it produces, because the mask decides which nodes the simulation holds as well
//! as which get painted; this module draws the window's answer and asks for changes to
//! it.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    div, prelude::*, px, Bounds, Context, EntityInputHandler, FocusHandle, MouseButton, Pixels,
    Point, SharedString, UTF16Selection, Window,
};
use shale_core::graph::filter::GraphFilter;
use shale_core::theme::Palette;

use crate::acts::{
    FieldBackspace, FieldDelete, FieldDeleteWordBack, FieldLeft, FieldRight, GraphFilterClose,
    GraphFilterMissing, GraphFilterOpen, GraphFilterOrphans, GraphFilterTags,
};
use crate::field::{approx_char_width, Field, FieldElement, FieldOwner};

use super::GraphView;

/// The key context the filter strip lives in (`graph-filter`).
///
/// It is a context of its own because the strip holds a text field, and gpui's context
/// stack is the focused element's *ancestors*: if the field shared the `graph` context,
/// `hjkl`, `f`, `+` and `-` would be live while a query was being typed and the query
/// could not contain a single one of those letters. The field's div is a sibling of the
/// canvas rather than a child of the element that carries `graph`, which is what keeps
/// the two stacks apart.
pub(crate) const FILTER_CONTEXT: &str = "graph-filter";

/// Which of the filter strip's three chips a click or a key is asking about. They are
/// named for what they *show*: "missing" is on while the holes in the vault are drawn,
/// which is the inverse of core's `existing_only`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chip {
    /// Notes that link nothing and are linked by nothing.
    Orphans,
    /// Links to files that are not there.
    Missing,
    /// `#tag` nodes.
    Tags,
    /// The neighbourhood of the note on screen (M2b). Its label carries the depth.
    Local,
}

/// The depth the local view starts at when it is switched on rather than stepped: two
/// hops, because one hop is the note and its links, which reads as a list rather than a
/// picture.
pub(crate) const DEFAULT_LOCAL_DEPTH: usize = 2;

impl GraphView {
    /// The filter the window is working with, pushed in on every refresh so the chips
    /// draw the truth rather than what the view last asked for.
    ///
    /// The search line follows it, which is what makes a filter restored from the session
    /// file show up in the field as well as in the picture. A query that already matches
    /// what is typed is left alone, so the echo of the view's own keystroke does not
    /// throw the caret away.
    pub(crate) fn set_filter(&mut self, filter: GraphFilter, cx: &mut Context<Self>) {
        let query = filter.search.clone().unwrap_or_default();
        if query != self.filter_field.text() {
            self.filter_field = Field::new(&query);
        }
        self.adopt_filter(filter);
        cx.notify();
    }

    // ----- the filter strip --------------------------------------------------

    /// `/`: the strip opens and takes the keyboard, which is what makes the
    /// `graph-filter` context the active one and puts the graph's own letters out of the
    /// way. The focus is deferred because the field this is asking for does not exist in
    /// the tree until the frame that the keystroke opens.
    pub(super) fn open_panel(
        &mut self,
        _: &GraphFilterOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.panel = true;
        cx.notify();
        let focus = self.panel_focus.clone();
        window.defer(cx, move |window, cx| focus.focus(window, cx));
    }

    /// `escape`, and `enter`: the strip closes and the graph has the keyboard back.
    ///
    /// One action for both keys, because the filter took effect as it was typed: there is
    /// nothing for `enter` to accept that `escape` would be undoing, and the query stays
    /// in force either way. What closes is the strip, not the graph — a second `escape`
    /// is the one that leaves the tab.
    fn close_panel(&mut self, _: &GraphFilterClose, window: &mut Window, cx: &mut Context<Self>) {
        self.panel = false;
        cx.notify();
        self.focus_handle.focus(window, cx);
    }

    /// One chip, by click or by key. The keyboard's forms are `alt`-modified because a
    /// bare letter has to be able to reach the field as text.
    fn toggle(&mut self, chip: Chip, cx: &mut Context<Self>) {
        let mut filter = self.filter.clone();
        match chip {
            Chip::Orphans => filter.orphans = !filter.orphans,
            Chip::Tags => filter.tags = !filter.tags,
            // The chip says what is on screen, so "missing" off is core's
            // `existing_only` on.
            Chip::Missing => filter.existing_only = !filter.existing_only,
            // The local chip is not a toggle the strip can compute: the centre belongs to
            // the window, so it goes through its own door.
            Chip::Local => return self.toggle_local(cx),
        }
        self.apply_filter(filter, cx);
    }

    /// The local chip: the graph as the neighbourhood of the note on screen, or the whole
    /// vault. The window owns the centre, so this asks rather than computes — and if the
    /// window says nothing happened (no note has been shown, so there is nothing to be a
    /// neighbourhood around), the chip does not light up.
    pub(super) fn toggle_local(&mut self, cx: &mut Context<Self>) {
        let depth = if self.filter.local.is_some() {
            None
        } else {
            Some(DEFAULT_LOCAL_DEPTH)
        };
        let Ok(Some((summary, frame))) =
            self.workspace.update(cx, move |ws, cx| ws.graph_local(depth, cx))
        else {
            return;
        };
        self.summary = Some(summary);
        self.store_frame(frame);
        if let Some(filter) = self
            .workspace
            .update(cx, |ws, _cx| ws.graph_filter_state())
            .ok()
        {
            self.adopt_filter(filter);
        }
        cx.notify();
    }

    /// `alt-o`.
    fn toggle_orphans(
        &mut self,
        _: &GraphFilterOrphans,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle(Chip::Orphans, cx);
    }

    /// `alt-x`.
    fn toggle_missing(
        &mut self,
        _: &GraphFilterMissing,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle(Chip::Missing, cx);
    }

    /// `alt-t`.
    fn toggle_tags(&mut self, _: &GraphFilterTags, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle(Chip::Tags, cx);
    }

    /// The search line changed. Every path that edits the field ends here — the input
    /// handler and each of the five keys bound in the strip — so the picture follows the
    /// query as it is typed rather than waiting for `enter`, which is what makes the line
    /// a filter and not a command.
    fn sync_search(&mut self, cx: &mut Context<Self>) {
        let mut filter = self.filter.clone();
        filter.search = Some(self.filter_field.text().to_string());
        self.apply_filter(filter, cx);
    }

    /// Send a filter to the window and take back the counts and the picture it made.
    ///
    /// The window owns the filter, because its mask decides which nodes the simulation
    /// holds as well as which get painted. The change starts here, though, and the view is
    /// already borrowed while it is being handled — which is why this door hands the
    /// result back to the caller instead of pushing it in the way a note change's is.
    fn apply_filter(&mut self, filter: GraphFilter, cx: &mut Context<Self>) {
        self.adopt_filter(filter.clone());
        if let Ok(Some((summary, frame))) = self
            .workspace
            .update(cx, move |ws, cx| ws.graph_set_filter(filter, cx))
        {
            self.summary = Some(summary);
            self.store_frame(frame);
        }
        cx.notify();
    }

    /// The strip itself: three chips and the search line, in the `graph-filter` context
    /// and tracking the strip's own focus.
    ///
    /// The field's keys are the same actions the prompt and the pickers bind, because it
    /// is the same field; the three toggles are `alt`-modified, because every bare letter
    /// this context receives is a letter of the query.
    pub(super) fn filter_panel(&mut self, cx: &mut Context<Self>) -> gpui::Div {
        let palette = Rc::clone(&self.palette);
        let (orphans, missing, tags) = (
            self.filter.orphans,
            !self.filter.existing_only,
            self.filter.tags,
        );
        let local = self.filter.local.as_ref().map(|scope| scope.depth);
        let local_label = match local {
            Some(depth) => format!("local {depth}"),
            None => "local".to_string(),
        };
        div()
            .key_context(FILTER_CONTEXT)
            .track_focus(&self.panel_focus)
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .text_color(gpui::rgb(palette.ui_fg))
            // A click anywhere in the strip is a click on the strip's keyboard. The
            // window already aims the keyboard at the field when the frame moves (see
            // [`GraphView::focus_target`]), but a press that lands on the strip's
            // background — between the chips, or in the gap beside the field — has to
            // take the keyboard too, or the field looks ready and hears nothing.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.panel_focus.focus(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &FieldBackspace, _window, cx| {
                this.filter_field.backspace();
                this.sync_search(cx);
            }))
            .on_action(cx.listener(|this, _: &FieldDelete, _window, cx| {
                this.filter_field.delete_forward();
                this.sync_search(cx);
            }))
            .on_action(cx.listener(|this, _: &FieldDeleteWordBack, _window, cx| {
                this.filter_field.delete_word_back();
                this.sync_search(cx);
            }))
            .on_action(cx.listener(|this, _: &FieldLeft, _window, cx| {
                this.filter_field.step_caret(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FieldRight, _window, cx| {
                this.filter_field.step_caret(1);
                cx.notify();
            }))
            .on_action(cx.listener(Self::close_panel))
            .on_action(cx.listener(Self::toggle_orphans))
            .on_action(cx.listener(Self::toggle_missing))
            .on_action(cx.listener(Self::toggle_tags))
            .child(
                chip(&palette, "orphans", orphans)
                    .id("graph-filter-orphans")
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.toggle(Chip::Orphans, cx);
                    })),
            )
            .child(
                chip(&palette, "missing", missing)
                    .id("graph-filter-missing")
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.toggle(Chip::Missing, cx);
                    })),
            )
            .child(
                chip(&palette, "tags", tags)
                    .id("graph-filter-tags")
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.toggle(Chip::Tags, cx);
                    })),
            )
            .child(
                chip(&palette, &local_label, local.is_some())
                    .id("graph-filter-local")
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.toggle(Chip::Local, cx);
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(FieldElement::new(cx.entity())),
            )
    }

    /// Whether the filter strip is showing, for the tests that open and close it.
    #[cfg(test)]
    pub(crate) fn panel_open(&self) -> bool {
        self.panel
    }

    /// What the search line holds, for the tests that type into it.
    #[cfg(test)]
    pub(crate) fn query(&self) -> &str {
        self.filter_field.text()
    }
}

/// The strip's search line is the same field the prompt and the pickers draw, so the
/// view has to be a [`FieldOwner`] over it and answer for it to gpui's input handler.
/// The shape is [`crate::prompt::Prompt`]'s.
impl FieldOwner for GraphView {
    fn field(&self) -> &Field {
        &self.filter_field
    }

    fn palette(&self) -> &Rc<Palette> {
        &self.palette
    }

    fn family(&self) -> SharedString {
        self.font.clone()
    }

    fn size(&self) -> f32 {
        self.font_size
    }

    fn focus(&self) -> FocusHandle {
        self.panel_focus.clone()
    }
}

impl EntityInputHandler for GraphView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let (text, used) = self.filter_field.text_in_utf16(range_utf16);
        actual_range.replace(used);
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.filter_field.caret_selection())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.filter_field.replace_utf16(range_utf16, new_text);
        self.sync_search(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Composition is not tracked separately, exactly as in the prompt; the second
        // range is the part of the composing text that is now committed, which the field
        // has no use for.
        let _ = new_selected_range;
        self.replace_text_in_range(range_utf16, new_text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let offset = self.filter_field.offset_from_utf16(range_utf16.start);
        let x = crate::field::PAD + (offset as f32 * approx_char_width());
        Some(Bounds::from_corners(
            gpui::point(bounds.left() + px(x), bounds.top()),
            gpui::point(bounds.left() + px(x + 8.), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(
            self.filter_field
                .utf16_index_for_x(f32::from(point.x), approx_char_width()),
        )
    }
}

/// One chip in the filter strip: a word naming what is being shown, filled and legible
/// while it is, quiet and outlined while it is not, at the radius every other control in
/// the window uses.
///
/// The three states are the window's state grammar (`design-language.md` §4): a chip that
/// is on carries `ui_active_bg`, one the pointer is over carries `ui_hover_bg`, and one
/// that is off is drawn in `ui_disabled` -- the role for "this is here, you are not
/// getting it", which is quieter than a label and says something a label does not.
fn chip(palette: &Palette, label: impl Into<gpui::SharedString>, on: bool) -> gpui::Div {
    div()
        .flex_none()
        .px_2()
        .text_size(crate::style::TEXT_XS)
        .rounded(crate::style::CHIP_RADIUS)
        .border_1()
        .border_color(gpui::rgb(palette.ui_border))
        .cursor(gpui::CursorStyle::PointingHand)
        .text_color(gpui::rgb(if on {
            palette.ui_fg
        } else {
            palette.ui_disabled
        }))
        .when(on, |d| d.bg(gpui::rgb(palette.ui_active_bg)))
        // Only an off chip answers the pointer with a background: on one, the hover
        // colour would be quieter than the state it is already in.
        .when(!on, |d| d.hover(|s| s.bg(gpui::rgb(palette.ui_hover_bg))))
        .child(label.into())
}
