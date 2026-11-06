//! A phantom line: text a queued proposal would put in the note, painted where it would
//! land without being in the buffer at all (roadmap P2.5b).
//!
//! It is a separate element rather than a [`super::LineElement`] because a LineElement is
//! a piece of the buffer: it carries byte ranges for the cursor, the selection, clicks,
//! the IME and the wrap model to be answered against. A phantom line has no bytes to
//! answer with — it is not in the note, and `j`/`k`, search and folds must keep skipping
//! it — so it shapes and paints a string and asks nothing of the buffer.
//!
//! It is shaped at the note's own family and size, which is what lets a proposal read as
//! part of the note rather than as a panel pasted over it.

use std::rc::Rc;

use gpui::{
    fill, font, px, relative, rgb, App, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Style as ElementStyle,
    Window,
};
use shale_core::theme::Palette;

use crate::theme::{self, RunStyle};

/// What kind of phantom row this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhantomKind {
    /// A line the proposal would add.
    Added,
    /// The `−N +M` a collapsed hunk shows instead of its lines.
    Badge,
}

pub(crate) struct PhantomLine {
    /// The line the proposal would put there, or the count a collapsed hunk shows.
    pub(crate) text: SharedString,
    /// The note's own font family and size, so the row measures like the text beside it.
    pub(crate) family: SharedString,
    pub(crate) size: f32,
    pub(crate) palette: Rc<Palette>,
    pub(crate) kind: PhantomKind,
    /// The hunk the review strip's marker is on: a wider bar, so the strip and the note
    /// point at the same change rather than at two nearby ones.
    pub(crate) current: bool,
}

impl PhantomLine {
    fn height(&self) -> Pixels {
        px(theme::line_height(self.size))
    }

    /// The bar's colour: the palette's green for an addition, its amber for a count.
    /// A proposed change is not an error, and neither is a collapsed one, which is why
    /// neither takes the red — `design-language.md` §1.
    fn bar(&self) -> u32 {
        match self.kind {
            PhantomKind::Added => self.palette.diff_add,
            PhantomKind::Badge => self.palette.diff_changed,
        }
    }
}

impl IntoElement for PhantomLine {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PhantomLine {
    type RequestLayoutState = ();
    type PrepaintState = ShapedLine;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = ElementStyle::default();
        style.size.width = relative(1.).into();
        style.size.height = self.height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let base_font = font(self.family.clone());
        let style = RunStyle {
            color: match self.kind {
                PhantomKind::Added => self.palette.diff_add,
                PhantomKind::Badge => self.palette.diff_changed,
            },
            ..RunStyle::plain(&self.palette)
        };
        window.text_system().shape_line(
            self.text.clone(),
            px(self.size),
            &[style.text_run(self.text.len(), &base_font)],
            None,
        )
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        line: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // The hunk's bar, continued down the phantom rows so the proposal reads as one
        // block: struck lines, then the lines that would replace them.
        let width = if self.current { px(3.) } else { px(2.) };
        window.paint_quad(fill(
            Bounds::from_corners(
                gpui::point(bounds.left(), bounds.top()),
                gpui::point(bounds.left() + width, bounds.bottom()),
            ),
            rgb(self.bar()),
        ));
        if let Err(err) = line.paint(
            gpui::point(bounds.left(), bounds.top()),
            self.height(),
            gpui::TextAlign::default(),
            None,
            window,
            cx,
        ) {
            tracing::warn!("failed to paint a proposed line: {err:#}");
        }
    }
}
