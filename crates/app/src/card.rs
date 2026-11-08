//! The hover preview card: what a link points at, without leaving the note.
//!
//! A card is **opened by the pointer and dismissed by the pointer**. Unlike
//! [`crate::popup::Popup`] it never takes the keyboard: a surface that ate the keys the
//! moment the mouse brushed a link would be worse than no card at all, and the reader is
//! reading rather than being asked a question. That is why there is no `FocusHandle` and
//! no `key_context` here — there is no state in the card for a key to act on.
//!
//! The window owns the *content* decision (which link, which section, which note the
//! caret is in) and this widget draws what it is handed. The size comes from the same
//! constants the render below builds the panel out of, so the window's idea of where the
//! card is and the card's idea of how big it is cannot drift apart — the rule
//! [`crate::popup::panel_size`] follows for the same reason.

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, Context, IntoElement, ParentElement, Pixels, Render, SharedString, Size,
    Styled, Window,
};
use shale_core::theme::Palette;

use crate::style::CHIP_RADIUS;

/// How wide the card is: narrower than the checker's panel, because it carries the
/// target's own lines rather than a sentence and a list of things to do about it.
pub(crate) const WIDTH: Pixels = px(380.);

/// Space between the border and the first line: the spacing scale's second step
/// (`design-language.md` §2).
const PAD: Pixels = crate::style::SPACE_2;
/// The border, counted twice in the height.
const BORDER: Pixels = px(1.);

/// How many of the target's lines a card shows before saying how many it left out.
///
/// The same ceiling the checker's panel keeps: a card taller than this runs off the
/// bottom of the window, and unlike that panel it cannot scroll.
pub(crate) const VISIBLE_LINES: usize = 8;

/// One row of the card at the note's text size — the note's own line height, so the card
/// reads as part of the note rather than as a dialog laid over it.
fn row_height(size: f32) -> Pixels {
    px(crate::theme::line_height(size))
}

/// What the card says about the link under the pointer.
pub(crate) struct Preview {
    /// The note the card is about, named the way the vault names it. A section link
    /// names the section too: the card is showing that section, and the reader should be
    /// able to tell which half of the note they are looking at.
    title: SharedString,
    /// The target's lines, with markdown markers already hidden — the rule the note's own
    /// rows and an embed's rows follow, so a heading in the card is a heading and not a
    /// `#`.
    lines: Vec<SharedString>,
    /// How many lines the card left out, said out loud: a preview that looks complete is
    /// a lie about the note.
    hidden: usize,
    palette: Rc<Palette>,
    family: SharedString,
    size: f32,
}

impl Preview {
    pub(crate) fn new(
        title: impl Into<SharedString>,
        lines: Vec<String>,
        hidden: usize,
        palette: Rc<Palette>,
        family: SharedString,
        size: f32,
    ) -> Self {
        Self {
            title: title.into(),
            lines: lines.into_iter().map(SharedString::from).collect(),
            hidden,
            palette,
            family,
            size,
        }
    }

    /// What the card says, in the same words it was handed: how a test checks the card
    /// without reading pixels off it.
    #[cfg(test)]
    pub(crate) fn shape(&self) -> (String, Vec<String>, usize) {
        (
            self.title.to_string(),
            self.lines.iter().map(|line| line.to_string()).collect(),
            self.hidden,
        )
    }

    /// How many rows the card draws: the title, the target's lines, and the count of what
    /// was left out when anything was.
    fn rows(&self) -> usize {
        1 + self.lines.len() + usize::from(self.hidden > 0)
    }

    /// The size the render will produce. The window places the card with this before it
    /// draws the card, so the two agree by construction.
    pub(crate) fn panel_size(&self) -> Size<Pixels> {
        gpui::size(
            WIDTH,
            PAD * 2. + BORDER * 2. + row_height(self.size) * self.rows() as f32,
        )
    }
}

impl Render for Preview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette.clone();
        div()
            .w(WIDTH)
            .px(PAD)
            .py(PAD)
            .text_size(px(self.size))
            .font_family(self.family.clone())
            .bg(gpui::rgb(palette.popup_bg))
            .text_color(gpui::rgb(palette.ui_fg))
            .border_1()
            .border_color(gpui::rgb(palette.popup_border))
            // The raised triple, identical to the checker's panel and the picker: every
            // floating thing in this app is shaped the same way (`design-language.md` §5).
            .rounded(CHIP_RADIUS)
            .child(
                div().h(row_height(self.size)).flex().gap_2().child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_ellipsis()
                        .child(gpui::StyledText::new(self.title.clone())),
                ),
            )
            .children(self.lines.iter().map(|line| {
                div()
                    .h(row_height(self.size))
                    .text_color(gpui::rgb(palette.ui_dim))
                    .text_ellipsis()
                    .child(gpui::StyledText::new(line.clone()))
            }))
            .when(self.hidden > 0, |d| {
                d.child(
                    div()
                        .h(row_height(self.size))
                        .text_color(gpui::rgb(palette.ui_disabled))
                        .child(gpui::StyledText::new(format!("{} more lines", self.hidden))),
                )
            })
    }
}
