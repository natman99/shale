//! The chrome's tooltip: one line of text, in the colours of the surface that owns the
//! control it hangs off. gpui's `.tooltip()` wants a view, and Zed's own `Tooltip::text`
//! lives in the `ui` crate Shale does not depend on, so the view is ours. Small on
//! purpose: it has no actions, no selection and no focus, and gpui dismisses it when the
//! pointer leaves the control.

use gpui::{
    AnyView, App, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div,
    prelude::*,
};
use shale_core::theme::Palette;
use std::rc::Rc;

pub(crate) struct ChromeTooltip {
    /// The line to show: what the control is, in the words the docs use for it.
    text: SharedString,
    /// The colours of the surface the control sits on, so a tooltip over the note and
    /// one over the tab bar do not disagree about what chrome looks like.
    palette: Rc<Palette>,
}

impl ChromeTooltip {
    /// Build the view gpui paints when the pointer rests on the control.
    pub(crate) fn build(
        text: impl Into<SharedString>,
        palette: &Rc<Palette>,
        _window: &mut Window,
        cx: &mut App,
    ) -> AnyView {
        let palette = Rc::clone(palette);
        cx.new(|_cx| Self {
            text: text.into(),
            palette,
        })
        .into()
    }
}

impl Render for ChromeTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .border_1()
            .border_color(gpui::rgb(self.palette.ui_border))
            .bg(gpui::rgb(self.palette.ui_status_bg))
            .text_color(gpui::rgb(self.palette.ui_fg))
            .rounded(crate::style::CHIP_RADIUS)
            .px_2()
            .py_1()
            .text_size(crate::style::TEXT_XS)
            .child(self.text.clone())
    }
}
