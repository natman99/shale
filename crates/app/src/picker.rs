//! A searchable list: quick-open of notes and the command palette today, and the
//! widget vault search reuses.
//!
//! The same shape as the prompt — a [`Field`] on top, keys handled through the
//! same actions — plus the two things a prompt does not need: the rows are
//! re-ranked with [`shale_core::fuzzy`] on every keystroke, and one row is
//! highlighted so `enter` or `j`/`k` can choose it without the mouse.
//!
//! A picker is generic over what a row stands for (`PickerTarget` in the
//! workspace), and the payload travels *inside* the row list rather than in a
//! parallel array the caller has to keep in step: a picker whose rows are
//! re-derived from the query changes their number and their order on every
//! keystroke, which is exactly when a parallel array goes wrong.
//!
//! The list is a *window* over the ranked rows rather than a scrolling element:
//! the window slides to keep the selection inside it, which is all a keyboard
//! user needs and needs no scroll state of its own.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    div, font, prelude::*, px, rgb, App, Bounds, Context, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, Font, InteractiveElement, IntoElement, ParentElement, Pixels, Point,
    Render, SharedString, Styled, StyledText, TextRun, UTF16Selection, Window,
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
    /// Enter on the highlighted row. Nothing more is needed: the row carries
    /// what it stands for.
    Choose,
    /// The highlighted row moved — by `j`/`k`, by typing, or by the pointer
    /// resting on a row. The window decides what a row being pointed at means;
    /// the theme picker previews it, and nothing else answers.
    Highlight,
    /// Escape: nothing was chosen.
    Cancel,
}

/// Rows that cannot be listed up front, given the query: vault search has to read
/// every note, so its rows only exist once something has been typed. The rows are
/// shown in the order the source returns them — a search already decided what is
/// worth showing, and re-ranking it as a subsequence would only muddy that.
pub(crate) type RowSource<T> = Rc<dyn Fn(&str) -> Vec<(PickerItem, T)>>;

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
    /// Which item of the picker's current list this row is.
    item: usize,
    /// Which characters of the *label* the query matched. The query is ranked
    /// against the wider search text, so these are computed separately: a query
    /// that only matched the folder part of a path highlights nothing in the
    /// name, which is honest about why the row is there.
    label_ranges: Vec<(usize, usize)>,
}

/// A searchable list with one row highlighted.
pub(crate) struct Picker<T: Clone + 'static> {
    focus_handle: FocusHandle,
    /// The question shown above the field.
    label: SharedString,
    /// What has been typed so far, and where the caret is.
    field: Field,
    /// Everything that could be chosen: the row's text and what it stands for.
    /// For a sourced picker this is rebuilt from the query on every keystroke.
    items: Vec<(PickerItem, T)>,
    /// `Some` when the rows come from the query rather than from a list the
    /// caller passed in.
    source: Option<RowSource<T>>,
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

impl<T: Clone + 'static> Picker<T> {
    /// A picker over a list that already exists — every note, every command.
    pub(crate) fn new(
        label: impl Into<SharedString>,
        items: Vec<(PickerItem, T)>,
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
            source: None,
            ranked: Vec::new(),
            selected: 0,
            palette,
            family,
            size,
        };
        picker.rerank();
        picker
    }

    /// A picker whose rows are built from the query, for a list too expensive to
    /// produce before anything has been typed.
    pub(crate) fn sourced(
        label: impl Into<SharedString>,
        source: RowSource<T>,
        palette: Rc<Palette>,
        family: SharedString,
        size: f32,
        cx: &mut App,
    ) -> Self {
        let mut picker = Self {
            focus_handle: cx.focus_handle(),
            label: label.into(),
            field: Field::new(""),
            items: Vec::new(),
            source: Some(source),
            ranked: Vec::new(),
            selected: 0,
            palette,
            family,
            size,
        };
        picker.rerank();
        picker
    }

    /// What `enter` would choose, or `None` when there is nothing to choose.
    pub(crate) fn chosen(&self) -> Option<T> {
        self.ranked
            .get(self.selected)
            .map(|row| self.items[row.item].1.clone())
    }

    /// How many rows matched.
    pub(crate) fn matches(&self) -> usize {
        self.ranked.len()
    }

    /// The row the pointer is over becomes the highlighted one, so the pointer
    /// and the keyboard always agree about what `enter` would choose.
    pub(crate) fn hover_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if row >= self.ranked.len() || row == self.selected {
            return;
        }
        self.selected = row;
        cx.notify();
        cx.emit(PickerEvent::Highlight);
    }

    /// Repaint the panel with `palette`. The theme picker previews the theme its
    /// rows name, and the panel is drawn in the theme it is offering, so the
    /// preview is not hidden by the panel that shows it.
    pub(crate) fn set_palette(&mut self, palette: Rc<Palette>, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    /// Re-rank the rows for the current query and put the selection back on the
    /// best one: typing narrows the list, so the row the user wants is the one
    /// that just moved to the top.
    fn rerank(&mut self) {
        let query = self.field.text().to_string();
        if let Some(source) = self.source.clone() {
            // The source did the finding; its order is the order to show.
            self.items = source(&query);
            self.ranked = self
                .items
                .iter()
                .enumerate()
                .map(|(idx, (item, _))| Row {
                    item: idx,
                    // A search hit is there because the query is *in* the line,
                    // so the highlight is that substring, not a fuzzy alignment.
                    label_ranges: substring_ranges(item.label.as_str(), &query),
                })
                .collect();
            self.selected = 0;
            return;
        }
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
            shale_core::ranked(&query, &self.items, RANK_LIMIT, |(item, _)| {
                item.search.as_str()
            })
            .into_iter()
            // The score is what put the row here; the list is already in order.
            .map(|(_, idx)| Row {
                item: idx,
                label_ranges: shale_core::fuzzy_match(&query, self.items[idx].0.label.as_str())
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

impl<T: Clone + 'static> Focusable for Picker<T> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl<T: Clone + 'static> FieldOwner for Picker<T> {
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

impl<T: Clone + 'static> EventEmitter<PickerEvent> for Picker<T> {}

impl<T: Clone + 'static> EntityInputHandler for Picker<T> {
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
        // The best row may have changed under the selection, so a picker that
        // previews what it points at has to be told.
        cx.emit(PickerEvent::Highlight);
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

/// The byte range of the query inside `label`, ignoring case: how a sourced
/// picker explains its rows. Only the first occurrence is marked, and a query
/// that is not in the label (it matched elsewhere in the row) marks nothing.
fn substring_ranges(label: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let lower = label.to_lowercase();
    match lower.find(&query.to_lowercase()) {
        Some(start) => vec![(start, start + query.len())],
        None => Vec::new(),
    }
}

/// The label split into `gpui::TextRun`s at the characters the query matched, so
/// a row shows *why* it matched. Matched characters are drawn in
/// [`Palette::match_text`] rather than on a box behind them: one run per stretch
/// of matched or unmatched text, in one text element, so the letters keep their
/// normal spacing.
///
/// The ranges are byte ranges the fuzzy matcher produced, so they land on
/// character boundaries and never overlap. They are one character long each -- a
/// matched word arrives as several adjacent ranges -- which is why they are
/// coalesced before the runs are built: a run per letter would break kerning and
/// line-breaking inside the word the user typed.
fn runs(
    label: &str,
    matched: &[(usize, usize)],
    base_font: &Font,
    palette: &Palette,
) -> Vec<TextRun> {
    // (range, is a match) first, so neighbours of the same kind can merge.
    let mut parts: Vec<(Range<usize>, bool)> = Vec::new();
    let mut from = 0;
    for &(start, end) in matched {
        if start > from {
            parts.push((from..start, false));
        }
        match parts.last_mut() {
            Some(last) if last.1 && last.0.end == start => last.0.end = end,
            _ => parts.push((start..end, true)),
        }
        from = end;
    }
    if from < label.len() {
        parts.push((from..label.len(), false));
    }
    parts
        .into_iter()
        .map(|(range, is_match)| TextRun {
            len: range.len(),
            font: base_font.clone(),
            color: rgb(if is_match {
                palette.match_text
            } else {
                palette.ui_fg
            })
            .into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        })
        .collect()
}

impl<T: Clone + 'static> Render for Picker<T> {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let palette = Rc::clone(&self.palette);
        // Unlike the plain text children this replaces, a `TextRun` carries its
        // own font, so the rows ask for the editor's family -- the same one the
        // query field above them is drawn in. Their size still comes from the
        // surrounding text style.
        let base_font = font(self.family.clone());
        let start = self.window_start();
        let end = (start + VISIBLE_ROWS).min(self.ranked.len());
        let rows: Vec<gpui::Stateful<gpui::Div>> = (start..end)
            .map(|n| {
                let row = &self.ranked[n];
                let (item, _target) = &self.items[row.item];
                let hovered = entity.clone();
                let clicked = entity.clone();
                div()
                    .id(("picker-row", n))
                    .flex()
                    .gap_2()
                    .px_1()
                    .py_0p5()
                    .when(n == self.selected, |d| {
                        d.bg(gpui::rgb(palette.ui_active_bg))
                    })
                    .child(StyledText::new(item.label.clone()).with_runs(runs(
                        item.label.as_str(),
                        &row.label_ranges,
                        &base_font,
                        &palette,
                    )))
                    .child(
                        div()
                            .text_color(gpui::rgb(palette.ui_dim))
                            .child(item.detail.clone()),
                    )
                    // The pointer picks the row it is over, which is what makes a
                    // preview (the theme picker's) follow the mouse, and what makes
                    // a click mean the same thing `enter` would.
                    .on_mouse_move(move |_, _, cx| {
                        hovered.update(cx, |picker, cx| picker.hover_row(n, cx));
                    })
                    .on_click(move |_, _, cx| {
                        clicked.update(cx, |picker, cx| {
                            picker.hover_row(n, cx);
                            cx.emit(PickerEvent::Choose);
                        });
                    })
            })
            .collect();

        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &FieldSubmit, _window, cx| {
                if this.chosen().is_some() {
                    cx.emit(PickerEvent::Choose);
                }
            }))
            .on_action(cx.listener(|_, _: &FieldCancel, _window, cx| {
                cx.emit(PickerEvent::Cancel);
            }))
            .on_action(cx.listener(|this, _: &FieldBackspace, _window, cx| {
                this.field.backspace();
                this.rerank();
                cx.notify();
                cx.emit(PickerEvent::Highlight);
            }))
            .on_action(cx.listener(|this, _: &FieldDelete, _window, cx| {
                this.field.delete_forward();
                this.rerank();
                cx.notify();
                cx.emit(PickerEvent::Highlight);
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
                cx.emit(PickerEvent::Highlight);
            }))
            .on_action(cx.listener(|this, _: &PickerDown, _window, cx| {
                this.move_selection(1);
                cx.notify();
                cx.emit(PickerEvent::Highlight);
            }))
            .border_1()
            .border_color(gpui::rgb(palette.ui_border))
            // Opaque, because the picker *overlays* the note: a panel that lets
            // the text through is unreadable, which is the price of not pushing
            // the note down.
            .bg(gpui::rgb(palette.ui_bg))
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .pt_1()
                    .text_color(gpui::rgb(palette.ui_dim))
                    .child(self.label.clone()),
            )
            .child(div().px_2().py_1().child(FieldElement::new(entity)))
            .children(rows)
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .py_1()
                    .text_color(gpui::rgb(palette.ui_dim))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What each run is: how many bytes it covers and whether it was painted as
    /// a match rather than as ordinary row text.
    fn painted(label: &str, matched: &[(usize, usize)], palette: &Palette) -> Vec<(usize, bool)> {
        runs(label, matched, &font("monospace"), palette)
            .into_iter()
            .map(|r| (r.len, r.color == rgb(palette.match_text).into()))
            .collect()
    }

    /// An empty query leaves the row as one run of ordinary text, which is also
    /// the case for a row that matched only in its search text, not its label.
    #[test]
    fn a_row_that_matched_nothing_in_its_label_is_one_run() {
        let palette = Palette::DEFAULT;
        assert_eq!(painted("Welcome.md", &[], &palette), vec![(10, false)]);
    }

    /// The matcher returns one range per character, so a matched word arrives as
    /// a heap of adjacent ones. They have to become one run: a run per letter is
    /// what put gaps inside matched words.
    #[test]
    fn adjacent_matches_are_merged_into_one_run() {
        let palette = Palette::DEFAULT;
        assert_eq!(
            painted("New note", &[(0, 1), (1, 2), (2, 3)], &palette),
            vec![(3, true), (5, false)]
        );
    }

    /// Matches that are not next to each other stay separate, so the letters in
    /// between keep the row's own colour.
    #[test]
    fn a_match_in_the_middle_splits_the_row_into_three_runs() {
        let palette = Palette::DEFAULT;
        assert_eq!(
            painted("Note Book", &[(0, 1), (5, 6)], &palette),
            vec![(1, true), (4, false), (1, true), (3, false)]
        );
    }

    /// Runs are counted in bytes, like the ranges they come from, so a match on a
    /// multi-byte letter covers the whole letter.
    #[test]
    fn a_match_on_a_multibyte_letter_is_whole() {
        let palette = Palette::DEFAULT;
        assert_eq!(
            painted("caf\u{e9} note", &[(3, 5)], &palette),
            vec![(3, false), (2, true), (5, false)]
        );
    }
}
