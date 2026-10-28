//! The element that paints one display line: shaped text with markdown
//! styling, the selection and cursor quads, and the record the editor keeps for
//! hit-testing.
//!
//! A "display line" is a segment of a buffer row: soft wrap splits a long row
//! into several of these, and each one is painted and measured separately.
//!
//! Live preview hides the markdown syntax of a row the cursor is not on, so the
//! painted string is generally *not* one contiguous range of the buffer: the
//! [`PieceMap`] it is painted from is what keeps clicks, the cursor quad and the
//! IME anchored to the right bytes.
//!
//! A row that belongs to a markdown table is the exception: it is never wrapped
//! and its source pipes are not painted. Instead each cell's content is drawn at
//! its column's offset (see [`crate::table_view`]) with grid lines between the
//! columns, so the user sees a table. A table's `---` delimiter row is collapsed
//! out of the display model unless the cursor is on it or on the header, and
//! when it is drawn its cells (the alignment markers) are painted like any
//! other's. Because the painted text is no longer one contiguous string, a
//! painted line is recorded as a list of [`HitSegment`]s — one for an ordinary
//! row, one per cell for a table row — and every x <-> offset question is
//! answered against them.

use std::rc::Rc;

use gpui::{
    fill, font, px, relative, rgb, App, Bounds, Corners, Element, ElementId, ElementInputHandler,
    Entity, Font, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine,
    SharedString, Style as ElementStyle, TextRun, Window,
};
use shale_core::mdstyle::{CalloutKind, Style};
use shale_core::theme::Palette;

use crate::editor::MarkdownEditor;
use crate::image::ImagePaint;
use crate::preview::PieceMap;
use crate::table_view::TablePaint;
use crate::theme::{self, RunStyle};

/// One painted row: a segment of a buffer row when the row soft-wraps.
#[derive(Clone, Copy)]
pub(crate) struct DisplayLine {
    /// Buffer row this segment comes from.
    pub(crate) row: usize,
    /// First buffer byte of the segment.
    pub(crate) start: usize,
    /// End of the segment, exclusive.
    pub(crate) end: usize,
    /// Text size this segment is shaped and wrapped at (headings are larger).
    pub(crate) size: f32,
}

/// One painted piece of a display line: the whole row for ordinary text, or one
/// cell of a table row.
#[derive(Clone)]
pub(crate) struct HitSegment {
    /// Where the segment was painted, measured from the line's left edge.
    pub(crate) x: Pixels,
    /// The shaped text, which answers x <-> byte-index questions.
    pub(crate) line: ShapedLine,
    /// Which bytes of the note this segment's text came from: one piece for a
    /// table cell, and more than one when live preview hid markers in the middle
    /// of a row.
    pub(crate) map: PieceMap,
}

/// A display line captured during paint, for hit-testing and IME placement.
#[derive(Clone)]
pub(crate) struct HitLine {
    /// Flat display-line index.
    pub(crate) index: usize,
    /// First buffer byte of the segment.
    pub(crate) start: usize,
    /// End of the segment, exclusive.
    pub(crate) end: usize,
    /// The painted pieces, left to right: one for an ordinary row, one per cell
    /// for a table row.
    pub(crate) segments: Vec<HitSegment>,
    /// How wide the painted line is. Not read yet: it is what horizontal
    /// scrolling (for a table wider than the window) will need, and it is what
    /// the wrap-accuracy diagnostics compare against.
    #[allow(dead_code)]
    pub(crate) width: Pixels,
    /// Where it was painted.
    pub(crate) bounds: Bounds<Pixels>,
    /// Whether the line was painted as a thematic break's rule. Not read by the
    /// editor itself — the rule is a quad, not text — but it is what the preview
    /// tests check, and what horizontal scrolling will need alongside `width`.
    #[allow(dead_code)]
    pub(crate) rule: bool,
    /// Whether the line was painted inside a callout's box. Not read by the
    /// editor (the box is a quad, not text), but it is what the preview tests
    /// check.
    #[allow(dead_code)]
    pub(crate) callout: bool,
    /// The fold marker this line painted after its text, if any: where it starts
    /// (measured from the line's left edge) and what it says. A click inside it
    /// toggles the fold instead of moving the cursor.
    pub(crate) fold_marker: Option<FoldMarker>,
}

/// A fold marker drawn after a line's text. It is not buffer text, so it lives
/// outside the [`PieceMap`]: it only needs a position and a width.
#[derive(Clone)]
pub(crate) struct FoldMarker {
    /// First x of the marker, measured from the line's left edge.
    pub(crate) x: Pixels,
    /// The marker's shaped text; its width gives the right edge of the click
    /// target, and its string is what the paint tests check.
    pub(crate) line: ShapedLine,
}

impl HitLine {
    /// The segment that owns a buffer offset: the one whose text covers it, or
    /// else the last one to its left. A table's padding and pipes, and a hidden
    /// markdown marker, sit between pieces, so they resolve to the piece on
    /// their left.
    fn segment_index_for(&self, offset: usize) -> usize {
        self.segments
            .iter()
            .position(|s| s.map.covers(offset))
            // Nothing covers it (a click past the end of the line): the last
            // segment that starts before it, or the first one if the line is
            // empty.
            .or_else(|| {
                self.segments
                    .iter()
                    .rposition(|s| s.map.first_offset().is_some_and(|start| start <= offset))
            })
            .unwrap_or(0)
    }

    /// The x of a buffer offset, measured from this line's left edge. The offset
    /// is clamped into the line, because a caller (IME bounds, for one) may ask
    /// for a position just past its end.
    pub(crate) fn x_for_offset(&self, offset: usize) -> Pixels {
        let offset = offset.clamp(self.start, self.end);
        let segment = &self.segments[self.segment_index_for(offset)];
        let column = segment.map.index_for_offset(offset);
        segment.x + segment.line.x_for_index(column)
    }

    /// The buffer offset closest to an x measured from this line's left edge.
    pub(crate) fn offset_for_x(&self, x: Pixels) -> usize {
        let index = self.segments.iter().rposition(|s| s.x <= x).unwrap_or(0);
        let segment = &self.segments[index];
        let column = segment.line.closest_index_for_x(x - segment.x);
        segment
            .map
            .offset_for_index(column)
            .clamp(self.start, self.end)
    }
}

/// One line: text, optional selection and cursor quads.
pub(crate) struct LineElement {
    pub(crate) editor: Entity<MarkdownEditor>,
    pub(crate) row: usize,
    /// Flat display-line index of this row's segment.
    pub(crate) index: usize,
    /// Buffer byte offsets this segment covers (`end` is exclusive).
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// The text actually painted: `start..end` with any hidden markers left out.
    pub(crate) text: SharedString,
    /// Which bytes of the note `text` came from, so an x can be turned back into
    /// a buffer offset. Identity for a table row, whose `text` is the raw row:
    /// the grid paint hides the markers itself, cell by cell, from `hidden`.
    pub(crate) map: PieceMap,
    /// The markdown markers hidden on this row, as buffer ranges. Empty for an
    /// ordinary row, where they are already left out of `text`; a table row needs
    /// them because the grid paint builds each cell's text on its own.
    pub(crate) hidden: Vec<(usize, usize)>,
    /// Selection within the painted text, as painted-string byte indices.
    pub(crate) line_sel: Option<(usize, usize)>,
    /// The cursor, as a painted-string byte index.
    pub(crate) cursor_col: Option<usize>,
    pub(crate) insert: bool,
    /// (start, end, resolved) **buffer** byte spans of wiki-links overlapping
    /// this display line.
    pub(crate) links: Vec<(usize, usize, bool)>,
    /// (start, end, style) **buffer** byte markdown spans overlapping this
    /// display line, in the order the scanner emitted them (block spans first,
    /// so later spans win). Re-based onto the painted text before shaping.
    pub(crate) spans: Vec<(usize, usize, Style)>,
    /// (start, end) **buffer** byte ranges the spell and grammar checker has
    /// something to say about, drawn as wavy underlines. Re-based onto the
    /// painted text the same way the spans are.
    pub(crate) marks: Vec<(usize, usize)>,
    /// Text size to shape and paint at; the row's height follows it.
    pub(crate) size: f32,
    /// Whether the row belongs to a fenced code block, whose background spans
    /// the full width of the editor rather than just its text.
    pub(crate) block: bool,
    /// Whether the row is a thematic break whose dashes live preview hid: the
    /// element draws a rule across its width in their place.
    pub(crate) rule: bool,
    /// Set when the row belongs to an Obsidian callout: the element paints a
    /// box behind the line and an accent bar on its left edge, in the colour
    /// that matches the callout's type.
    pub(crate) callout: Option<CalloutKind>,
    /// Set when this row is a table row: the grid to paint instead of the raw
    /// markdown, whose pipes are then left out because they are not cell
    /// content.
    pub(crate) table: Option<TablePaint>,
    /// Set when this row is drawn as an embedded image instead of as text: the
    /// picture and the rectangle it fits in, which is also the line's height.
    pub(crate) image: Option<ImagePaint>,
    /// The width of one character cell, which turns the grid's character
    /// geometry into pixels.
    pub(crate) char_width: Pixels,
    /// The palette this line is painted with, shared from the editor. Every
    /// colour in the element (text runs, the cursor and selection quads, the
    /// table grid, a callout's box) comes from it, so a theme change is one
    /// assignment on the editor rather than a change here.
    pub(crate) palette: Rc<Palette>,
    /// Set when this row is a heading whose section is folded shut: how many
    /// rows the fold hides. The element paints that count after the row's text
    /// and records where, so a click on it opens the section again.
    pub(crate) fold: Option<usize>,
}

/// The text a folded heading carries: a marker and the number of rows the fold
/// hides, so the user can see how much is out of view.
fn fold_label(hidden: usize) -> String {
    format!(
        " \u{22ef} {hidden} more {}",
        if hidden == 1 { "line" } else { "lines" }
    )
}

pub(crate) struct LinePrepaint {
    /// The whole line's shaped text, for an ordinary row.
    line: Option<ShapedLine>,
    /// One entry per cell, for a table row.
    cells: Vec<CellShaped>,
    /// The fold marker (`⋯ 4 more lines`), shaped separately because it is not
    /// part of the buffer text and so has no offset in the piece map.
    marker: Option<ShapedLine>,
}

/// One table cell as it is painted: its shaped text, and the buffer ranges that
/// text came from, which the cursor quad, the selection and hit-testing all need.
#[derive(Clone)]
pub(crate) struct CellShaped {
    /// The cell's content, with any hidden markers left out.
    line: ShapedLine,
    /// The pieces of `line`.
    map: PieceMap,
}

/// Build the `TextRun`s that paint `text`.
///
/// Every byte of the line gets a style — the markdown spans applied in order,
/// so a span that starts later in the source wins, then the resolved/unresolved
/// colour of any wiki-link over the top, then the checker's wavy underline over
/// both — and neighbouring bytes that share a style are coalesced into one run.
/// Run lengths are utf-8 byte counts and must add up to exactly `text.len()`:
/// gpui walks runs against glyphs, so a map that does not tile the text styles
/// the wrong characters.
fn build_runs(
    text: &str,
    spans: &[(usize, usize, Style)],
    links: &[(usize, usize, bool)],
    marks: &[(usize, usize)],
    base_font: &Font,
    palette: &Palette,
) -> Vec<TextRun> {
    let total = text.len();
    let plain = RunStyle::plain(palette);
    let mut styles = vec![plain; total];
    for (start, end, style) in spans {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        for slot in &mut styles[start..end] {
            *slot = RunStyle::for_style(*style, palette);
        }
    }
    for (start, end, resolved) in links {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        let color = if *resolved {
            palette.link
        } else {
            palette.link_unresolved
        };
        for slot in &mut styles[start..end] {
            slot.color = color;
        }
    }
    // The checker's notes are drawn last, over whatever styling the text already
    // had: a misspelled word inside a heading still needs its squiggle. The
    // colour is the palette's error red, the one role every theme already reads
    // as "something is wrong"; a dedicated palette role is a later refinement.
    for (start, end) in marks {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        for slot in &mut styles[start..end] {
            slot.spell = Some(palette.callout_danger);
        }
    }

    let mut runs: Vec<TextRun> = Vec::new();
    let mut i = 0usize;
    while i < total {
        let style = styles[i];
        let mut j = i + 1;
        while j < total && styles[j] == style {
            j += 1;
        }
        runs.push(style.text_run(j - i, base_font));
        i = j;
    }
    if runs.is_empty() {
        // An empty display line still gets one (empty) run.
        runs.push(plain.text_run(0, base_font));
    }
    runs
}

/// One table cell's worth of styling: the markdown spans, wiki-links and
/// checker's ranges that fall inside it, re-based onto its painted text.
type CellStyles = (
    Vec<(usize, usize, Style)>,
    Vec<(usize, usize, bool)>,
    Vec<(usize, usize)>,
);

/// The markdown spans, wiki-links and checker's ranges that overlap one table
/// cell, clipped to it and re-based onto the cell's painted text. The spans in
/// are buffer offsets; `map` describes the text they are being mapped onto.
fn cell_styles(
    spans: &[(usize, usize, Style)],
    links: &[(usize, usize, bool)],
    marks: &[(usize, usize)],
    map: &PieceMap,
) -> CellStyles {
    (
        map.rebase_spans(spans),
        map.rebase_links(links),
        map.rebase_marks(marks),
    )
}

impl LineElement {
    /// The x of a cell's content, measured from the line's left edge.
    fn cell_x(&self, cell_x: usize) -> Pixels {
        px(cell_x as f32 * f32::from(self.char_width))
    }

    /// How tall this line is: as tall as its picture when it has one, and
    /// otherwise the height its text size implies.
    fn height(&self) -> f32 {
        self.image
            .as_ref()
            .map(|image| f32::from(image.height))
            .unwrap_or_else(|| theme::line_height(self.size))
    }

    /// Paints one row of a table: the header's background, the grid lines, the
    /// rule under the header, then each cell's text with its own selection and
    /// cursor quads.
    fn paint_table(
        &self,
        bounds: Bounds<Pixels>,
        prepaint: &LinePrepaint,
        table: &TablePaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let left = bounds.left();
        let grid_width = px(table.total as f32 * f32::from(self.char_width));

        // The header row is shaded so the table reads as a table even before
        // the rules are drawn.
        if table.header {
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(left, bounds.top()),
                    gpui::point(left + grid_width, bounds.bottom()),
                ),
                rgb(self.palette.table_header_bg),
            ));
        }
        // A vertical rule on every row, at each column boundary, so the columns
        // line up across the whole table.
        for boundary in &table.boundaries {
            let x = left + self.cell_x(*boundary);
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(x - px(0.5), bounds.top()),
                    gpui::point(x + px(0.5), bounds.bottom()),
                ),
                rgb(self.palette.table_line),
            ));
        }
        // The rule under the header. It is painted once: along the delimiter
        // row's top edge while live preview draws that row, and along the
        // header's bottom edge when the delimiter row is collapsed away (or has
        // scrolled out of view, which would leave the rule unpainted otherwise).
        let rule_y = if table.delimiter {
            Some(bounds.top())
        } else if table.rule_below {
            Some(bounds.bottom() - px(1.))
        } else {
            None
        };
        if let Some(y) = rule_y {
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(left, y),
                    gpui::point(left + grid_width, y + px(1.)),
                ),
                rgb(self.palette.table_line),
            ));
        }

        for (cell, shaped) in table.cells.iter().zip(prepaint.cells.iter()) {
            let x = left + self.cell_x(cell.x);
            // The selection is relative to the start of the painted line, and a
            // table row's line is the raw row, so it can be turned back into
            // buffer offsets and mapped into the cell's own painted text.
            if let Some((s, e)) = self.line_sel {
                let (lo, hi) = (s.max(cell.start - self.start), e.min(cell.end - self.start));
                if hi > lo {
                    let x0 = x + shaped
                        .line
                        .x_for_index(shaped.map.index_for_offset(self.start + lo));
                    let x1 = x + shaped
                        .line
                        .x_for_index(shaped.map.index_for_offset(self.start + hi));
                    window.paint_quad(fill(
                        Bounds::from_corners(
                            gpui::point(x0, bounds.top()),
                            gpui::point(x1.max(x0 + px(1.)), bounds.bottom()),
                        ),
                        rgb(self.palette.selection),
                    ));
                }
            }
            if let Err(err) = shaped.line.paint_background(
                gpui::point(x, bounds.top()),
                px(theme::line_height(self.size)),
                gpui::TextAlign::default(),
                None,
                window,
                cx,
            ) {
                tracing::warn!(row = self.row, "failed to paint cell background: {err:#}");
            }
            if let Err(err) = shaped.line.paint(
                gpui::point(x, bounds.top()),
                px(theme::line_height(self.size)),
                gpui::TextAlign::default(),
                None,
                window,
                cx,
            ) {
                tracing::warn!(row = self.row, "failed to paint cell: {err:#}");
            }
        }

        // The cursor sits on a cell, which the grid geometry places exactly.
        if let Some(col) = self.cursor_col {
            let offset = self.start + col;
            if let Some(cell) = table.cell_for(offset) {
                let shaped = table
                    .cells
                    .iter()
                    .position(|c| c.column == cell.column)
                    .and_then(|i| prepaint.cells.get(i))
                    .cloned()
                    .unwrap_or_else(|| CellShaped {
                        line: window.text_system().shape_line(
                            SharedString::from(""),
                            px(self.size),
                            &[],
                            None,
                        ),
                        map: PieceMap::identity(cell.start, cell.end),
                    });
                let index = shaped.map.index_for_offset(self.start + col);
                let x = left + self.cell_x(cell.x) + shaped.line.x_for_index(index);
                let width = if self.insert {
                    px(1.5)
                } else {
                    self.char_width.max(px(7.))
                };
                window.paint_quad(fill(
                    Bounds::from_corners(
                        gpui::point(x, bounds.top()),
                        gpui::point(x + width, bounds.bottom()),
                    ),
                    rgb(self.palette.cursor),
                ));
            }
        }
    }
}

impl IntoElement for LineElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LineElement {
    type RequestLayoutState = ();
    type PrepaintState = LinePrepaint;

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
        style.size.height = px(self.height()).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let text_system = window.text_system();
        let base_font = font(self.editor.read(cx).font_family());
        let Some(table) = self.table.clone() else {
            // The spans are buffer offsets and the runs are painted-text offsets,
            // so they are clipped to the painted pieces and re-based first.
            let runs = build_runs(
                &self.text,
                &self.map.rebase_spans(&self.spans),
                &self.map.rebase_links(&self.links),
                &self.map.rebase_marks(&self.marks),
                &base_font,
                &self.palette,
            );
            let line = text_system.shape_line(self.text.clone(), px(self.size), &runs, None);
            // The fold marker is not buffer text, so it is shaped on its own and
            // painted after the line rather than being spliced into it.
            let marker = self.fold.map(|hidden| {
                let label = fold_label(hidden);
                let style = RunStyle {
                    color: self.palette.dim,
                    ..RunStyle::plain(&self.palette)
                };
                text_system.shape_line(
                    SharedString::from(label.clone()),
                    px(self.size),
                    &[style.text_run(label.len(), &base_font)],
                    None,
                )
            });
            return LinePrepaint {
                line: Some(line),
                cells: Vec::new(),
                marker,
            };
        };
        // A table row is shaped cell by cell, each with only the styles that
        // fall inside it, because the cells are painted at column offsets rather
        // than as one string. A cell's text leaves out the row's hidden markers,
        // exactly as an ordinary row's does. The delimiter row is shaped like any
        // other: its cells are the alignment markers, which is what the user
        // edits when the row is drawn.
        let text = self.text.to_string();
        let cells = table
            .cells
            .iter()
            .map(|cell| {
                let pieces =
                    shale_core::mdlayout::visible_pieces(cell.start, cell.end, &self.hidden);
                let map = PieceMap::from_pieces(&pieces);
                let painted: String = pieces
                    .iter()
                    .map(|&(s, e)| &text[s - self.start..e - self.start])
                    .collect();
                let (spans, links, marks) =
                    cell_styles(&self.spans, &self.links, &self.marks, &map);
                let runs = build_runs(&painted, &spans, &links, &marks, &base_font, &self.palette);
                CellShaped {
                    line: text_system.shape_line(
                        SharedString::from(painted),
                        px(self.size),
                        &runs,
                        None,
                    ),
                    map,
                }
            })
            .collect();
        LinePrepaint {
            line: None,
            cells,
            marker: None,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // gpui only routes typed characters to an input handler that was
        // registered during the current frame's paint, so exactly one line
        // element per frame registers it (with the bounds of the first painted
        // line, which is also what IME candidate windows anchor to).
        let register = self.editor.update(cx, |editor, _| {
            if editor.input_registered {
                false
            } else {
                editor.input_registered = true;
                true
            }
        });
        if register {
            let focus_handle = self.editor.read(cx).focus_handle.clone();
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.editor.clone()),
                cx,
            );
        }

        // A fenced block's background covers the whole line, not just its text.
        if self.block {
            window.paint_quad(fill(bounds, rgb(self.palette.code_block_bg)));
        }

        // A callout is a box: a background across the whole line and an accent
        // bar on its left edge in the colour of its type. The `>` markers and
        // the `[!type]` header are hidden by live preview, so what is left of
        // the line reads as the box's content.
        if let Some(kind) = self.callout {
            window.paint_quad(fill(bounds, rgb(self.palette.callout_bg)));
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(bounds.left(), bounds.top()),
                    gpui::point(bounds.left() + px(2.), bounds.bottom()),
                ),
                rgb(self.palette.callout_accent(kind)),
            ));
        }

        // A thematic break is a rule across the editor, not the dashes the file
        // holds: `prepaint` shaped an empty line because live preview hid them,
        // and they come back on the row the cursor is on.
        if self.rule {
            let y = bounds.top() + px(f32::from(bounds.size.height) / 2.);
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(bounds.left(), y - px(0.5)),
                    gpui::point(bounds.right(), y + px(0.5)),
                ),
                rgb(self.palette.table_line),
            ));
        }

        // An embedded image fills the line's height, and its source text is not
        // painted while the picture is: clicking the picture puts the cursor on
        // the row, which is what brings the source back.
        if let Some(image) = self.image.clone() {
            let image_bounds = Bounds::from_corners(
                bounds.origin,
                gpui::point(bounds.left() + image.width, bounds.top() + image.height),
            );
            if let Err(err) = window.paint_image(
                bounds,
                image_bounds,
                Corners::all(px(0.)),
                image.image,
                0,
                false,
            ) {
                tracing::warn!(row = self.row, "failed to paint image: {err:#}");
            }
        }

        let width;
        let segments: Vec<HitSegment>;
        // Where this line painted its fold marker, if it has one, so a click on
        // the marker can be told from a click on the text.
        let mut fold_marker = None;
        if let Some(table) = self.table.clone() {
            width = px(table.total as f32 * f32::from(self.char_width));
            segments = table
                .cells
                .iter()
                .zip(prepaint.cells.iter())
                .map(|(cell, shaped)| HitSegment {
                    x: self.cell_x(cell.x),
                    line: shaped.line.clone(),
                    map: shaped.map.clone(),
                })
                .collect();
            self.paint_table(bounds, prepaint, &table, window, cx);
        } else {
            let line = prepaint
                .line
                .as_ref()
                .expect("an ordinary line is shaped in prepaint");
            width = self
                .image
                .as_ref()
                .map(|image| image.width)
                .unwrap_or_else(|| line.width());
            segments = vec![HitSegment {
                x: px(0.),
                line: line.clone(),
                map: self.map.clone(),
            }];

            // Selection quad.
            if let Some((s, e)) = self.line_sel {
                let x0 = line.x_for_index(s);
                let x1 = line.x_for_index(e.max(s + 1));
                window.paint_quad(fill(
                    Bounds::from_corners(
                        gpui::point(bounds.left() + x0, bounds.top()),
                        gpui::point(bounds.left() + x1, bounds.bottom()),
                    ),
                    rgb(self.palette.selection),
                ));
            }

            // Run backgrounds (inline code and highlights) are a separate pass
            // over the decoration runs, painted under the glyphs.
            if let Err(err) = line.paint_background(
                bounds.origin,
                px(theme::line_height(self.size)),
                gpui::TextAlign::default(),
                None,
                window,
                cx,
            ) {
                tracing::warn!(row = self.row, "failed to paint line background: {err:#}");
            }

            if let Err(err) = line.paint(
                bounds.origin,
                px(theme::line_height(self.size)),
                gpui::TextAlign::default(),
                None,
                window,
                cx,
            ) {
                tracing::warn!(row = self.row, "failed to paint line: {err:#}");
            }

            // Cursor quad.
            if let Some(col) = self.cursor_col {
                let x = line.x_for_index(col);
                let cursor_width = if self.insert {
                    px(1.5)
                } else {
                    let next = line.x_for_index((col + 1).min(self.text.len()));
                    (next - x).max(px(7.))
                };
                window.paint_quad(fill(
                    Bounds::from_corners(
                        gpui::point(bounds.left() + x, bounds.top()),
                        gpui::point(bounds.left() + x + cursor_width, bounds.bottom()),
                    ),
                    rgb(self.palette.cursor),
                ));
            }

            // The fold marker goes after the row's text, dimmed, and its x range
            // is recorded so clicking it can open the section again.
            if let Some(marker) = prepaint.marker.as_ref() {
                let x = bounds.left() + line.width() + self.char_width;
                if let Err(err) = marker.paint(
                    gpui::point(x, bounds.top()),
                    px(theme::line_height(self.size)),
                    gpui::TextAlign::default(),
                    None,
                    window,
                    cx,
                ) {
                    tracing::warn!(row = self.row, "failed to paint fold marker: {err:#}");
                }
                fold_marker = Some(FoldMarker {
                    x: x - bounds.left(),
                    line: marker.clone(),
                });
            }
        }

        // Record for hit-testing.
        self.editor.update(cx, |editor, _cx| {
            editor.hit_lines.push(HitLine {
                index: self.index,
                start: self.start,
                end: self.end,
                segments,
                width,
                bounds,
                rule: self.rule,
                callout: self.callout.is_some(),
                fold_marker,
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::font;

    /// One paint input: the line's text, its markdown spans, its wiki-link spans
    /// (start, end, resolved) and the checker's ranges.
    type Case = (
        &'static str,
        Vec<(usize, usize, Style)>,
        Vec<(usize, usize, bool)>,
        Vec<(usize, usize)>,
    );

    /// The runs must tile the line exactly: gpui matches runs against glyphs by
    /// byte count, so a run map that is short or long styles the wrong text (and
    /// in debug builds trips gpui's own assertions).
    #[test]
    fn runs_tile_the_line() {
        let font = font("monospace");
        let cases: Vec<Case> = vec![
            ("plain text", vec![], vec![], vec![]),
            ("", vec![], vec![], vec![]),
            (
                "# Heading **bold**",
                vec![
                    (0, 1, Style::HeadingMarker),
                    (0, 18, Style::Heading { level: 1 }),
                    (10, 16, Style::Strong),
                ],
                vec![],
                vec![],
            ),
            (
                "héllo 🎉 `code` wörld",
                vec![(13, 19, Style::Code)],
                vec![],
                vec![],
            ),
            (
                "[[link]] and [[gone]]",
                vec![(0, 8, Style::WikiLink)],
                vec![(0, 8, true), (13, 21, false)],
                vec![],
            ),
            // A checker's note over styled text must still tile the line: it
            // splits runs rather than adding length.
            (
                "Thier heading **is wrogn**",
                vec![(14, 26, Style::Strong)],
                vec![],
                vec![(0, 5), (20, 26)],
            ),
        ];
        for (text, spans, links, marks) in cases {
            let runs = build_runs(text, &spans, &links, &marks, &font, &Palette::DARK);
            let total: usize = runs.iter().map(|r| r.len).sum();
            assert_eq!(total, text.len(), "runs must cover {text:?} exactly");
            if text.is_empty() {
                assert_eq!(runs.len(), 1, "an empty line gets one empty run");
            } else {
                assert!(
                    runs.iter().all(|r| r.len > 0),
                    "no run may be empty for {text:?}"
                );
            }
            // Adjacent runs must differ, or the coalescing pass is broken.
            let key = |r: &TextRun| {
                (
                    r.color,
                    r.font.weight,
                    r.font.style,
                    r.background_color,
                    r.underline.is_some(),
                    r.strikethrough.is_some(),
                )
            };
            for pair in runs.windows(2) {
                assert_ne!(
                    key(&pair[0]),
                    key(&pair[1]),
                    "adjacent runs in {text:?} should have been coalesced"
                );
            }
        }
    }

    /// The checker's ranges come out as wavy underlines in the palette's error
    /// colour, over whatever styling the same bytes already had.
    #[test]
    fn a_checker_range_is_a_wavy_underline() {
        let font = font("monospace");
        let text = "Thier spelling";
        let runs = build_runs(text, &[], &[], &[(0, 5)], &font, &Palette::DARK);
        let total: usize = runs.iter().map(|r| r.len).sum();
        assert_eq!(total, text.len());
        assert_eq!(runs[0].len, 5, "the marked word is its own run");
        let underline = runs[0].underline.expect("underlined");
        assert!(underline.wavy, "a checker's note is a squiggle");
        assert_eq!(
            underline.color,
            Some(rgb(Palette::DARK.callout_danger).into())
        );
        assert!(
            runs[1].underline.is_none(),
            "the bytes past the range are left alone"
        );
    }

    /// A cell only sees the styles that fall inside it, re-based onto its own
    /// painted text: a bold span that starts in another column must not leak in,
    /// and one that starts inside must not be clipped away.
    #[test]
    fn cell_styles_clip_and_rebase() {
        // The line is `| **a** | b |`: the strong span covers bytes 2..7 and the
        // first cell's content is bytes 1..7.
        let spans = vec![(2, 7, Style::Strong)];
        let links = vec![(12, 18, true)];
        let marks = vec![(2, 4), (12, 15)];
        let map = PieceMap::from_pieces(&[(1, 7)]);
        let (cell_spans, cell_links, cell_marks) = cell_styles(&spans, &links, &marks, &map);
        assert_eq!(cell_spans, vec![(1, 6, Style::Strong)]);
        assert!(cell_links.is_empty(), "a link in another cell is dropped");
        assert_eq!(
            cell_marks,
            vec![(1, 3)],
            "the mark inside the cell is re-based, the one in another cell dropped"
        );
    }

    /// A cell whose markers are hidden is painted from the pieces that are left,
    /// so its spans have to be re-based onto those pieces rather than onto the
    /// cell's raw content.
    #[test]
    fn cell_styles_follow_the_hidden_markers() {
        // The cell's content is bytes 10..17, `**a** b`, with both `**` hidden.
        let map = PieceMap::from_pieces(&[(12, 13), (15, 17)]);
        let (cell_spans, _, _) = cell_styles(&[(10, 15, Style::Strong)], &[], &[], &map);
        assert_eq!(
            cell_spans,
            vec![(0, 1, Style::Strong)],
            "the span follows the painted `a`"
        );
    }
}
