//! A phantom line: text painted inside the note that is not in the note's buffer.
//!
//! Two things put rows here. A queued proposal's additions, where they would land
//! (roadmap P2.5b). And a `![[transclusion]]`'s body, the lines of another note shown
//! under the line that asks for them (roadmap P4.0a) — which is the same rendering
//! problem wearing a different hat: the rows must take the note's font and styling, must
//! not enter the buffer, and must stay invisible to `j`/`k`, search, folds and the ruler.
//!
//! It is a separate element rather than a [`super::LineElement`] because a LineElement is
//! a piece of the buffer: it carries byte ranges for the cursor, the selection, clicks,
//! the IME and the wrap model to be answered against. A phantom line has no bytes to
//! answer with, so it shapes and paints a string and asks nothing of the buffer.
//!
//! It is shaped at the note's own family and size, which is what lets it read as part of
//! the note rather than as a panel pasted over it. An embed's rows also carry the source
//! note's markdown styling, because a heading inside an embedded note is still a heading.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    fill, font, px, relative, rgb, App, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString,
    Style as ElementStyle, Window,
};
use shale_core::theme::Palette;
use shale_core::transclude::Origin;

use crate::line::marks;
use crate::theme::{self, RunStyle};

/// How far right each level of embed steps. Deliberately smaller than a list
/// indent: this is a seam between notes, not a nesting structure inside one.
const INDENT_STEP: f32 = 14.0;

/// Where a phantom row was painted, and what it is a line *of*.
///
/// A phantom row is content the buffer does not hold, so nothing in the display model
/// can answer for it — but it is still on screen, and a reader who clicks it means
/// something by that. This record is the only way back from the pixel to the note: the
/// row's address is an `Origin`, a place in another buffer, which is what entering an
/// embed starts from (roadmap P4.0b).
#[derive(Debug, Clone)]
pub(crate) struct PhantomHit {
    /// The band the row was painted in, gutter to window edge.
    #[allow(dead_code)]
    pub(crate) bounds: Bounds<Pixels>,
    /// Where in the vault this row is a line of.
    #[allow(dead_code)]
    pub(crate) origin: Option<Origin>,
}

/// What kind of phantom row this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhantomKind {
    /// A line the proposal would add.
    Added,
    /// The `−N +M` a collapsed hunk shows instead of its lines.
    Badge,
    /// A line of another note, shown under the `![[…]]` that asks for it. Styled by
    /// its own markdown, in the note's own colours: an embed is content, not a proposal,
    /// so it takes none of the diff hues.
    Embed,
    /// An embed that stopped expanding: its name, and nothing else.
    EmbedBadge,
}

pub(crate) struct PhantomLine {
    /// The line the proposal would put there, the count a collapsed hunk shows, or a
    /// line of an embedded note.
    pub(crate) text: SharedString,
    /// The note's own font family and size, so the row measures like the text beside it.
    pub(crate) family: SharedString,
    pub(crate) size: f32,
    pub(crate) palette: Rc<Palette>,
    pub(crate) kind: PhantomKind,
    /// The hunk the review strip's marker is on: a wider bar, so the strip and the note
    /// point at the same change rather than at two nearby ones.
    pub(crate) current: bool,
    /// How many levels of embed deep this row is. 0 for a proposal's rows, which belong
    /// at the note's own margin; 1 for the body of the host's own `![[…]]`, deeper for
    /// an embed inside that. The indent is what makes a nested embed readable without a
    /// box around each level.
    pub(crate) indent: usize,
    /// Markdown spans over `text`, in byte offsets, for an embed's rows: the source
    /// note's own styling, so a heading inside an embedded note is still a heading.
    ///
    /// The caller computes these over the **whole embed body** and slices them per row,
    /// because a fence that opens on one row and closes on another is only visible to a
    /// scan that sees both. An empty list means "plain", which is what a proposal's rows
    /// are: proposed text is not styled, it is a change being offered.
    pub(crate) spans: Vec<(usize, usize, shale_core::mdstyle::Style)>,
    /// Which note this row is a line of, and which row. `None` for a proposal's rows.
    pub(crate) origin: Option<Origin>,
    /// The editor's list of painted phantom rows, which this element adds itself to
    /// while painting.
    ///
    /// Not an `Entity<MarkdownEditor>` updated from `paint`, the way `LineElement`
    /// records its hit line: the phantom rows are built while the editor is already
    /// borrowed, and a shared cell keeps that borrow from having to be handed around.
    pub(crate) hits: Rc<RefCell<Vec<PhantomHit>>>,
}

impl PhantomLine {
    fn height(&self) -> Pixels {
        px(theme::line_height(self.size))
    }

    /// The bar's colour: the palette's green for an addition, its amber for a count.
    /// A proposed change is not an error, and neither is a collapsed one, which is why
    /// neither takes the red — `design-language.md` §1.
    ///
    /// An embed takes neither: its rows are the note's content, not a change to it. Its
    /// rule is the quiet border colour, whose only job is to show where borrowed text
    /// starts and stops.
    fn bar(&self) -> u32 {
        match self.kind {
            PhantomKind::Added => self.palette.diff_add,
            PhantomKind::Badge => self.palette.diff_changed,
            PhantomKind::Embed | PhantomKind::EmbedBadge => self.palette.ui_border,
        }
    }

    /// Where the row's text starts: the note's column, stepped right for each level of
    /// embed. The rule stays in the gutter whatever the indent, so a nested embed reads
    /// as one block with one edge rather than a stack of boxes.
    fn text_x(&self, bounds: Bounds<Pixels>) -> Pixels {
        bounds.left() + px(self.indent as f32 * INDENT_STEP)
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
        let runs = if self.kind == PhantomKind::Embed {
            // The source note's markdown, in the note's own colours. `build_runs` is the
            // same function the note's own lines go through, which is the point: an
            // embedded heading and a local heading must not be two different renderings
            // of the same thing.
            super::runs::build_runs(
                self.text.as_ref(),
                &super::runs::LineStyles {
                    spans: self.spans.clone(),
                    links: Vec::new(),
                    marks: Vec::new(),
                    hits: Vec::new(),
                    active: Vec::new(),
                    diff: None,
                },
                &base_font,
                &self.palette,
                1.0,
            )
        } else {
            let style = RunStyle {
                color: match self.kind {
                    PhantomKind::Added => self.palette.diff_add,
                    PhantomKind::Badge => self.palette.diff_changed,
                    // A stopped embed is a name the reader can act on, so it wears the
                    // link colour rather than a diff colour.
                    PhantomKind::EmbedBadge => self.palette.link,
                    PhantomKind::Embed => self.palette.ui_fg,
                },
                ..RunStyle::plain(&self.palette)
            };
            vec![style.text_run(self.text.len(), &base_font)]
        };
        window
            .text_system()
            .shape_line(self.text.clone(), px(self.size), &runs, None)
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
        // The bar, continued down the phantom rows so the block reads as one thing: for
        // a proposal, the struck lines and their replacements; for an embed, every row of
        // the borrowed body however deeply nested. It sits in the gutter whatever the
        // row's indent, so nesting steps the text and not the edge it hangs from.
        //
        // The hunk's bar, continued down the phantom rows so the proposal reads as one
        // block: struck lines, then the lines that would replace them. It sits in the
        // gutter, at the same x the struck rows put theirs, and the phantom text stays in
        // the note's own text column — an addition that is indented away from the
        // column reads as a quote rather than as a line the note would gain.
        let width = if self.current { px(3.) } else { px(2.) };
        let x = marks::glyph_x(bounds.left() - crate::style::GUTTER, width);
        window.paint_quad(fill(
            Bounds::from_corners(
                gpui::point(x, bounds.top()),
                gpui::point(x + width, bounds.bottom()),
            ),
            rgb(self.bar()),
        ));
        if let Err(err) = line.paint(
            gpui::point(self.text_x(bounds), bounds.top()),
            self.height(),
            gpui::TextAlign::default(),
            None,
            window,
            cx,
        ) {
            tracing::warn!("failed to paint a proposed line: {err:#}");
        }

        // Record where it landed. The bounds are the row's own band, so a click
        // anywhere in it — including the gutter bar and the empty space past the
        // text — belongs to this row and not to the note row it happens to sit
        // beside.
        if self.origin.is_some() {
            self.hits.borrow_mut().push(PhantomHit {
                bounds,
                origin: self.origin.clone(),
            });
        }
    }
}
