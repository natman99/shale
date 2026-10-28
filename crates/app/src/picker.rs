//! A searchable list: quick-open of notes today, and the widget the command
//! palette and vault search will reuse.
//!
//! The same shape as the prompt — a [`Field`] on top, keys handled through the
//! same actions — plus the two things a prompt does not need: the rows are
//! re-ranked with [`shale_core::fuzzy`] on every keystroke, and one row is
//! highlighted so `enter` or `j`/`k` can choose it without the mouse.
//!
//! The list is a *window* over the ranked rows rather than a scrolling element:
//! the window slides to keep the selection inside it, which is all a keyboard
//! user needs and needs no scroll state of its own.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    div, prelude::*, px, App, Bounds, Context, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, ParentElement, Pixels, Point, Render, SharedString,
    Styled, UTF16Selection, Window,
};
use shale_core::theme::Palette;

use crate::acts::{
    FieldBackspace, FieldCancel, FieldDelete, FieldLeft, FieldRight, FieldSubmit, PickerDown,
    PickerUp,
};
use crate::field::{approx_char_width, Field, FieldElement, FieldOwner};

/// The key context the picker's bindings live in. Like the prompt it is drawn as
/// a *sibling* of the tree and the editor, because gpui enables a context's
/// bindings whenever that context is in the focused element's stack: nested
/// inside either one, their `j`/`k` would swallow the letters being typed.
pub(crate) const CONTEXT: &str = "picker";

/// How many rows are visible at once.
const VISIBLE_ROWS: usize = 12;

/// How many ranked rows to keep. A vault with more notes than this is only
/// searched for the best matches, which is also what keeps a keystroke cheap.
const RANK_LIMIT: usize = 200;

/// What the user did with the picker.
#[derive(Clone)]
pub(crate) enum PickerEvent {
    /// Enter on the highlighted row: its index into the list the picker was
    /// built from, which is the order the workspace passed in.
    Choose(usize),
    /// Escape: nothing was chosen.
    Cancel,
}

/// One row the picker can offer.
#[derive(Clone)]
pub(crate) struct PickerItem {
    /// The text the row leads with: a note's name, a command's name.
    pub label: SharedString,
    /// Dimmer text after it: the folder a note lives in, the key a command is
    /// bound to, the line a search hit is on.
    pub detail: SharedString,
    /// What the query is matched against, which is usually wider than the label:
    /// a note's whole path, so `no/an` reaches `notes/Another Note.md`.
    pub search: SharedString,
}

/// One row in ranked order.
struct Row {
    /// Which item of the caller's list this row is.
    item: usize,
    /// Which characters of the *label* the query matched. The query is ranked
    /// against the wider search text, so these are computed separately: a query
    /// that only matched the folder part of a path highlights nothing in the
    /// name, which is honest about why the row is there.
    label_ranges: Vec<(usize, usize)>,
}

/// A searchable list with one row highlighted.
pub(crate) struct Picker {
    focus_handle: FocusHandle,
    /// The question shown above the field.
    label: SharedString,
    /// What has been typed so far, and where the caret is.
    field: Field,
    /// Everything that could be chosen, in the order the caller passed it.
    items: Vec<PickerItem>,
    /// `(row, index into items)`, best first, recomputed whenever the query
    /// changes. With an empty query this is every item in the caller's order.
    ranked: Vec<Row>,
    /// Which entry of `ranked` is highlighted.
    selected: usize,
    /// The colours the picker is drawn with, taken from the workspace when it
    /// opens.
    palette: Rc<Palette>,
    /// The editor's resolved font family, so the rows match the note text.
    family: SharedString,
    /// Text size in points.
    size: f32,
}

impl Picker {
    pub(crate) fn new(
        label: impl Into<SharedString>,
        items: Vec<PickerItem>,
        palette: Rc<Palette>,
        family: SharedString,
        size: f32,
        cx: &mut App,
    ) -> Self {
        let mut picker = Self {
            focus_handle: cx.focus_handle(),
            label: label.into(),
            field: Field::new(""),
            items,
            ranked: Vec::new(),
            selected: 0,
            palette,
            family,
            size,
        };
        picker.rerank();
        picker
    }

    /// The index into the caller's list that `enter` would choose, or `None` when
    /// there is nothing to choose.
    pub(crate) fn chosen(&self) -> Option<usize> {
        self.ranked.get(self.selected).map(|row| row.item)
    }

    /// How many rows matched.
    pub(crate) fn matches(&self) -> usize {
        self.ranked.len()
    }

    /// Re-rank the rows for the current query and put the selection back on the
    /// best one: typing narrows the list, so the row the user wants is the one
    /// that just moved to the top.
    fn rerank(&mut self) {
        let query = self.field.text().to_string();
        let ranked = if query.is_empty() {
            self.items
                .iter()
                .enumerate()
                .map(|(idx, _)| Row {
                    item: idx,
                    label_ranges: Vec::new(),
                })
                .collect()
        } else {
            shale_core::ranked(&query, &self.items, RANK_LIMIT, |item| item.search.as_str())
                .into_iter()
                // The score is what put the row here; the list is already in order.
                .map(|(_, idx)| Row {
                    item: idx,
                    label_ranges: shale_core::fuzzy_match(&query, self.items[idx].label.as_str())
                        .map(|in_label| in_label.ranges)
                        .unwrap_or_default(),
                })
                .collect()
        };
        self.ranked = ranked;
        self.selected = 0;
    }

    /// Move the highlighted row, clamped: a list that wraps is easier to lose.
    fn move_selection(&mut self, delta: isize) {
        if self.ranked.is_empty() {
            self.selected = 0;
            return;
        }
        let last = self.ranked.len() - 1;
        let moved = self.selected as isize + delta;
        self.selected = moved.clamp(0, last as isize) as usize;
    }

    /// The first ranked row to draw, so the selection is always inside the window.
    fn window_start(&self) -> usize {
        self.selected.saturating_sub(VISIBLE_ROWS - 1)
    }
}

impl Focusable for Picker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FieldOwner for Picker {
    fn field(&self) -> &Field {
        &self.field
    }

    fn palette(&self) -> &Rc<Palette> {
        &self.palette
    }

    fn family(&self) -> SharedString {
        self.family.clone()
    }

    fn size(&self) -> f32 {
        self.size
    }

    fn focus(&self) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PickerEvent> for Picker {}

impl EntityInputHandler for Picker {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let (text, used) = self.field.text_in_utf16(range_utf16);
        actual_range.replace(used);
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.field.caret_selection())
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
        self.field.replace_utf16(range_utf16, new_text);
        // Every keystroke narrows the list, which is the whole point of a picker.
        self.rerank();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range_utf16, new_text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let offset = self.field.offset_from_utf16(range_utf16.start);
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
            self.field
                .utf16_index_for_x(f32::from(point.x), approx_char_width()),
        )
    }
}

/// The label split into runs at the characters the query matched, so a row shows
/// *why* it matched. The ranges are byte ranges the fuzzy matcher produced, so
/// they land on character boundaries and never overlap.
fn runs(label: &str, matched: &[(usize, usize)], palette: &Palette) -> Vec<gpui::Div> {
    let mut out = Vec::new();
    let mut from = 0;
    for &(start, end) in matched {
        if start > from {
            out.push(div().child(label[from..start].to_string()));
        }
        out.push(
            div()
                .bg(gpui::rgb(palette.highlight_bg))
                .child(label[start..end].to_string()),
        );
        from = end;
    }
    if from < label.len() {
        out.push(div().child(label[from..].to_string()));
    }
    out
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let palette = Rc::clone(&self.palette);
        let start = self.window_start();
        let end = (start + VISIBLE_ROWS).min(self.ranked.len());
        let rows: Vec<gpui::Div> = (start..end)
            .map(|n| {
                let row = &self.ranked[n];
                let item = &self.items[row.item];
                div()
                    .flex()
                    .gap_2()
                    .px_1()
                    .py_0p5()
                    .when(n == self.selected, |d| {
                        d.bg(gpui::rgb(palette.ui_active_bg))
                    })
                    .children(runs(item.label.as_str(), &row.label_ranges, &palette))
                    .child(
                        div()
                            .text_color(gpui::rgb(palette.dim))
                            .child(item.detail.clone()),
                    )
            })
            .collect();

        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &FieldSubmit, _window, cx| {
                if let Some(idx) = this.chosen() {
                    cx.emit(PickerEvent::Choose(idx));
                }
            }))
            .on_action(cx.listener(|_, _: &FieldCancel, _window, cx| {
                cx.emit(PickerEvent::Cancel);
            }))
            .on_action(cx.listener(|this, _: &FieldBackspace, _window, cx| {
                this.field.backspace();
                this.rerank();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FieldDelete, _window, cx| {
                this.field.delete_forward();
                this.rerank();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FieldLeft, _window, cx| {
                this.field.step_caret(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FieldRight, _window, cx| {
                this.field.step_caret(1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PickerUp, _window, cx| {
                this.move_selection(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PickerDown, _window, cx| {
                this.move_selection(1);
                cx.notify();
            }))
            .border_1()
            .border_color(gpui::rgb(palette.ui_border))
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .pt_1()
                    .text_color(gpui::rgb(palette.dim))
                    .child(self.label.clone()),
            )
            .child(div().px_2().py_1().child(FieldElement::new(entity)))
            .children(rows)
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .py_1()
                    .text_color(gpui::rgb(palette.dim))
                    .child(format!(
                        "{} of {} matching",
                        if self.ranked.is_empty() {
                            0
                        } else {
                            self.selected + 1
                        },
                        self.matches()
                    )),
            )
    }
}
