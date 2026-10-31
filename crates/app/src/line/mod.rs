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
//!
//! The run builder — the part that turns markdown spans, wiki-links, checker's
//! notes and search matches into the styled [`gpui::TextRun`]s a line is painted
//! from — lives in [`runs`].

mod runs;
#[cfg(test)]
mod tests;

use runs::{build_runs, cell_styles, CellShaped, LineStyles};

use std::rc::Rc;

use gpui::{
    fill, font, px, relative, rgb, App, Bounds, Corners, Element, ElementId, ElementInputHandler,
    Entity, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine,
    SharedString, Style as ElementStyle, Window,
};
use shale_core::mdstyle::{CalloutKind, Style};
use shale_core::theme::Palette;

use crate::dim::fade;
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
    /// How much of the line's colour survived the dimming system: 1.0 is the
    /// palette's own. Recorded so the focus-mode tests can see what the frame
    /// actually drew rather than what the model was asked for.
    #[allow(dead_code)]
    pub(crate) dim: f32,
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
    /// The straight quotes drawn curly on this line, as `(buffer offset, glyph)` in
    /// note order. Already applied to `text` and `map` for an ordinary line; a
    /// table row needs them because each cell's text is built on its own.
    pub(crate) quotes: Vec<(usize, char)>,
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
    /// (start, end) **buffer** byte ranges the `/` search matched, drawn with a
    /// background under the text. Re-based like the marks.
    pub(crate) hits: Vec<(usize, usize)>,
    /// The one `/` match the cursor is on, as a **buffer** byte range: drawn over
    /// the others so the current match in a run of `n`/`N` stands out. `None`
    /// when there is no current match.
    pub(crate) active: Option<(usize, usize)>,
    /// Text size to shape and paint at; the line's height follows it.
    pub(crate) size: f32,
    /// How much of this line's colour survives: 1.0 is the palette's own, less is
    /// every colour it draws pulled toward the background by the dimming system.
    /// The cursor, the selection and a search match's background are never faded,
    /// because those are the user's marks on the text rather than the text.
    pub(crate) dim: f32,
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
            let styles = LineStyles {
                spans: self.map.rebase_spans(&self.spans),
                links: self.map.rebase_links(&self.links),
                marks: self.map.rebase_marks(&self.marks),
                hits: self.map.rebase_marks(&self.hits),
                active: self
                    .active
                    .map(|range| self.map.rebase_marks(&[range]))
                    .unwrap_or_default(),
            };
            let runs = build_runs(&self.text, &styles, &base_font, &self.palette, self.dim);
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
                let map = PieceMap::with_substitutions(&pieces, &self.quotes);
                let painted = map.painted_text_from(&text, self.start);
                let styles = cell_styles(
                    &self.spans,
                    &self.links,
                    &self.marks,
                    &self.hits,
                    self.active,
                    &map,
                );
                let runs = build_runs(&painted, &styles, &base_font, &self.palette, self.dim);
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
        // The block's own colours fade with the rest of the row, or a dimmed code
        // block would still shout at the reader from across the note.
        if self.block {
            window.paint_quad(fill(
                bounds,
                rgb(fade(self.palette.code_block_bg, self.palette.bg, self.dim)),
            ));
        }

        // A callout is a box: a background across the whole line and an accent
        // bar on its left edge in the colour of its type. The `>` markers and
        // the `[!type]` header are hidden by live preview, so what is left of
        // the line reads as the box's content.
        if let Some(kind) = self.callout {
            window.paint_quad(fill(
                bounds,
                rgb(fade(self.palette.callout_bg, self.palette.bg, self.dim)),
            ));
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(bounds.left(), bounds.top()),
                    gpui::point(bounds.left() + px(2.), bounds.bottom()),
                ),
                rgb(fade(
                    self.palette.callout_accent(kind),
                    self.palette.bg,
                    self.dim,
                )),
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
                rgb(fade(self.palette.table_line, self.palette.bg, self.dim)),
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
                dim: self.dim,
                fold_marker,
            });
        });
    }
}
