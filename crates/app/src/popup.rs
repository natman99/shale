//! The window's one popup: what the spell and grammar checker has to say about
//! the word under the cursor, and what can be done about it.
//!
//! It is an **overlay inside the window**, not an OS window: it is drawn as a
//! sibling of the note, positioned over it, and it takes the keyboard while it is
//! up — which is what makes `escape` close it and `enter` act, exactly as the
//! prompt and the pickers work.
//!
//! Two rules the window enforces, and the widget is built to make easy:
//!
//! - **Only one popup at a time.** The window holds one `Option<Entity<Popup>>`,
//!   and opening another replaces it. The widget has no idea how many exist.
//! - **It closes properly.** Every way out — `escape`, choosing a row, the pointer
//!   leaving the word it is about, an edit, opening a picker or a prompt — goes
//!   through the window's single `close_popup`, which also hands the keyboard back
//!   to whatever had it. A popup that outlives its subject is a bug the user has
//!   to mouse out of.
//!
//! The rows are the server's own actions, in the server's own words; the widget
//! never learns what choosing one *does*, it only says which row was chosen.

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, App, Context, CursorStyle, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, SharedString, Size, Styled,
    Window,
};
use shale_core::theme::Palette;

use crate::acts::{ApplyRow, FieldCancel, FieldSubmit, PickerDown, PickerUp};

/// The key context the popup's bindings live in. Like the prompt and the pickers
/// it is drawn as a *sibling* of the note, so the vim contexts cannot swallow the
/// keys that drive it.
pub(crate) const CONTEXT: &str = "popup";

/// How wide the panel is. Wide enough for a checker's sentence and an action's
/// title on one line each, which is what keeps [`panel_size`] — and therefore the
/// window's idea of where the panel is — exact.
pub(crate) const WIDTH: Pixels = px(430.);
/// The height of one line of the panel: the message, then one per action.
///
/// It is the note's own line height at the note's own size, so the panel reads as
/// part of the note, and [`panel_size`] can promise the render delivers.
fn row_height(size: f32) -> Pixels {
    px(crate::theme::line_height(size))
}
/// Space between the border and the first line.
const PAD: Pixels = px(6.);
/// The border, counted twice in the height.
const BORDER: Pixels = px(1.);
/// How many characters of a message or a title fit on one line. Longer ones are
/// clipped, because a wrapped line would make [`panel_size`] a guess.
const LINE_CHARS: usize = 48;

/// The panel's size for `rows` action rows drawn at the note's text `size`.
///
/// The window uses this both to place the panel and to decide whether the pointer
/// is over it, and the render below builds the panel out of the same constants, so
/// the two cannot drift apart.
pub(crate) fn panel_size(rows: usize, size: f32) -> Size<Pixels> {
    gpui::size(
        WIDTH,
        PAD * 2. + BORDER * 2. + row_height(size) * (rows as f32 + 1.),
    )
}

/// What the user did with the popup.
#[derive(Clone)]
pub(crate) enum PopupEvent {
    /// Choose row `n` (0-based). What it means is the window's business.
    Apply(usize),
    /// `escape`, or the pointer left the word the popup is about.
    Cancel,
}

/// A note from the checker, and the things that can be done about it.
pub(crate) struct Popup {
    focus_handle: FocusHandle,
    /// The complaint, in the server's words: `Did you mean to spell \`thier\`
    /// this way?`.
    message: SharedString,
    /// The server's name for the kind of complaint (`SpellCheck`, `UseTitleCase`),
    /// drawn dim after the message so a grammar rule is not mistaken for a typo.
    code: SharedString,
    /// What can be done, in the server's words. Empty until the server answers —
    /// the panel is worth showing then too, since the complaint is the point.
    rows: Vec<SharedString>,
    /// Which row the keyboard is on.
    selected: usize,
    /// The colours the panel is drawn with, from the window.
    palette: Rc<Palette>,
    /// The editor's resolved font family, so the panel reads like the note.
    family: SharedString,
    /// Text size in points.
    size: f32,
}

impl Popup {
    pub(crate) fn new(
        message: impl Into<SharedString>,
        code: impl Into<SharedString>,
        palette: Rc<Palette>,
        family: SharedString,
        size: f32,
        cx: &mut App,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            message: clip(&message.into()),
            code: code.into(),
            rows: Vec::new(),
            selected: 0,
            palette,
            family,
            size,
        }
    }

    /// The actions the server offered, replacing whatever was there. The
    /// highlight stays on the same index where it can, so a popup that fills in
    /// under the pointer does not jump.
    pub(crate) fn set_rows(&mut self, rows: Vec<String>, cx: &mut Context<Self>) {
        let rows: Vec<SharedString> = rows.into_iter().map(|row| clip(&row)).collect();
        if rows == self.rows {
            return;
        }
        self.selected = self.selected.min(rows.len().saturating_sub(1));
        self.rows = rows;
        cx.notify();
    }

    /// The highlighted row, if there is one to highlight.
    pub(crate) fn chosen(&self) -> Option<usize> {
        (!self.rows.is_empty()).then_some(self.selected)
    }

    /// What the panel offers, in order. Only the tests ask, to check that the
    /// checker's answer reached the panel.
    #[cfg(test)]
    pub(crate) fn rows(&self) -> &[SharedString] {
        &self.rows
    }

    /// The complaint the panel is about. Only the tests ask.
    #[cfg(test)]
    pub(crate) fn message(&self) -> &SharedString {
        &self.message
    }

    /// Move the highlight, wrapping at the ends: a popup has a handful of rows and
    /// wrapping is quicker than counting them.
    pub(crate) fn move_selection(&mut self, by: isize) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len() as isize;
        let next = (self.selected as isize + by).rem_euclid(len);
        self.selected = next as usize;
    }

    /// The row the pointer is over becomes the highlighted one, so the mouse and
    /// the keyboard always agree about what `enter` would do.
    pub(crate) fn select(&mut self, row: usize, cx: &mut Context<Self>) {
        if row < self.rows.len() && row != self.selected {
            self.selected = row;
            cx.notify();
        }
    }
}

impl Focusable for Popup {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PopupEvent> for Popup {}

impl Render for Popup {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Rc::clone(&self.palette);
        let entity = cx.entity();
        let rows: Vec<gpui::Stateful<gpui::Div>> = self
            .rows
            .iter()
            .enumerate()
            .map(|(n, row)| {
                let clicked = entity.clone();
                let hovered = entity.clone();
                div()
                    .id(("popup-row", n))
                    // A row is a button, and the pointer should say so. Without
                    // this the note's IBeam (set on the editor's root) shows
                    // through the panel, because the panel is drawn over the note
                    // rather than inside it.
                    .cursor(CursorStyle::PointingHand)
                    .h(row_height(self.size))
                    .px_1()
                    .flex()
                    .gap_2()
                    .text_ellipsis()
                    .child(div().w(px(14.)).child(format!("{}.", n + 1)))
                    .child(gpui::StyledText::new(row.clone()))
                    .when(n == self.selected, |d| {
                        d.bg(gpui::rgb(palette.popup_hover_bg))
                    })
                    .on_click(move |_, _, cx| {
                        clicked.update(cx, |_, cx| cx.emit(PopupEvent::Apply(n)));
                    })
                    .on_mouse_move(move |_, _, cx| {
                        hovered.update(cx, |popup, cx| popup.select(n, cx));
                    })
            })
            .collect();

        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            // The panel itself is not a thing to click — only its rows are — so
            // the message line gets the ordinary arrow rather than a hand, and
            // certainly not the note's text caret.
            .cursor(CursorStyle::Arrow)
            .w(WIDTH)
            .px(PAD)
            .py(PAD)
            .text_size(px(self.size))
            .font_family(self.family.clone())
            .bg(gpui::rgb(palette.popup_bg))
            .text_color(gpui::rgb(palette.ui_fg))
            .border_1()
            .border_color(gpui::rgb(palette.popup_border))
            .on_action(cx.listener(|_, _: &FieldCancel, _window, cx| {
                cx.emit(PopupEvent::Cancel);
            }))
            .on_action(cx.listener(|this, _: &FieldSubmit, _window, cx| {
                if let Some(row) = this.chosen() {
                    cx.emit(PopupEvent::Apply(row));
                }
            }))
            .on_action(cx.listener(|this, _: &PickerUp, _window, cx| {
                this.move_selection(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PickerDown, _window, cx| {
                this.move_selection(1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, action: &ApplyRow, _window, cx| {
                // The digit rows are bound 1-based; the list is 0-based.
                if action.0 < this.rows.len() {
                    cx.emit(PopupEvent::Apply(action.0));
                }
            }))
            .child(
                div()
                    .h(row_height(self.size))
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .child(gpui::StyledText::new(self.message.clone())),
                    )
                    .child(
                        div()
                            .text_color(gpui::rgb(palette.ui_dim))
                            .child(gpui::StyledText::new(self.code.clone())),
                    ),
            )
            .child(div().children(rows))
    }
}

/// Shorten to one line's worth of characters, marking that something was left off.
/// Never cuts through the middle of a character.
fn clip(text: &str) -> SharedString {
    if text.chars().count() <= LINE_CHARS {
        return text.into();
    }
    let kept: String = text.chars().take(LINE_CHARS).collect();
    SharedString::from(format!("{kept}…"))
}
