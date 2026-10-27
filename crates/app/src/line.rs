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
//! and its source pipes and dashes are not painted. Instead each cell's content
//! is drawn at its column's offset (see [`crate::table_view`]) with grid lines
//! between the columns, so the user sees a table. Because the painted text is no
//! longer one contiguous string, a painted line is recorded as a list of
//! [`HitSegment`]s — one for an ordinary row, one per cell for a table row — and
//! every x <-> offset question is answered against them.

use gpui::{
    fill, font, px, relative, rgb, App, Bounds, Element, ElementId, ElementInputHandler, Entity,
    Font, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine,
    SharedString, Style as ElementStyle, TextRun, Window,
};
use shale_core::mdstyle::Style;

use crate::editor::MarkdownEditor;
use crate::preview::PieceMap;
use crate::table_view::TablePaint;
use crate::theme::{self, RunStyle, CURSOR, LINK, LINK_UNRESOLVED, SELECTION};

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
    /// Text size to shape and paint at; the row's height follows it.
    pub(crate) size: f32,
    /// Whether the row belongs to a fenced code block, whose background spans
    /// the full width of the editor rather than just its text.
    pub(crate) block: bool,
    /// Set when this row is a table row: the grid to paint instead of the raw
    /// markdown, whose pipes and dashes are then hidden.
    pub(crate) table: Option<TablePaint>,
    /// The width of one character cell, which turns the grid's character
    /// geometry into pixels.
    pub(crate) char_width: Pixels,
}

pub(crate) struct LinePrepaint {
    /// The whole line's shaped text, for an ordinary row.
    line: Option<ShapedLine>,
    /// One entry per cell, for a table row. The delimiter row's cells are empty:
    /// its dashes are replaced by a rule.
    cells: Vec<CellShaped>,
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
/// colour of any wiki-link over the top — and neighbouring bytes that share a
/// style are coalesced into one run. Run lengths are utf-8 byte counts and must
/// add up to exactly `text.len()`: gpui walks runs against glyphs, so a map that
/// does not tile the text styles the wrong characters.
fn build_runs(
    text: &str,
    spans: &[(usize, usize, Style)],
    links: &[(usize, usize, bool)],
    base_font: &Font,
) -> Vec<TextRun> {
    let total = text.len();
    let mut styles = vec![RunStyle::PLAIN; total];
    for (start, end, style) in spans {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        for slot in &mut styles[start..end] {
            *slot = RunStyle::for_style(*style);
        }
    }
    for (start, end, resolved) in links {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        let color = if *resolved { LINK } else { LINK_UNRESOLVED };
        for slot in &mut styles[start..end] {
            slot.color = color;
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
        runs.push(RunStyle::PLAIN.text_run(0, base_font));
    }
    runs
}

/// The markdown spans and wiki-links that overlap one table cell, clipped to it
/// and re-based onto the cell's painted text. The spans in are buffer offsets;
/// `map` describes the text they are being mapped onto.
fn cell_styles(
    spans: &[(usize, usize, Style)],
    links: &[(usize, usize, bool)],
    map: &PieceMap,
) -> (Vec<(usize, usize, Style)>, Vec<(usize, usize, bool)>) {
    (map.rebase_spans(spans), map.rebase_links(links))
}

impl LineElement {
    /// The x of a cell's content, measured from the line's left edge.
    fn cell_x(&self, cell_x: usize) -> Pixels {
        px(cell_x as f32 * f32::from(self.char_width))
    }

    /// Paints one row of a table: the header's background, the grid lines, the
    /// delimiter's rule, then each cell's text with its own selection and cursor
    /// quads.
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
                rgb(theme::TABLE_HEADER_BG),
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
                rgb(theme::TABLE_LINE),
            ));
        }
        if table.delimiter {
            // The delimiter row is the rule under the header, not its dashes.
            let y = bounds.top() + px(f32::from(bounds.size.height) / 2.);
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(left, y - px(0.5)),
                    gpui::point(left + grid_width, y + px(0.5)),
                ),
                rgb(theme::TABLE_LINE),
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
                        rgb(SELECTION),
                    ));
                }
            }
            if table.delimiter {
                continue;
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
                    rgb(CURSOR),
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
        style.size.height = px(theme::line_height(self.size)).into();
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
                &base_font,
            );
            let line = text_system.shape_line(self.text.clone(), px(self.size), &runs, None);
            return LinePrepaint {
                line: Some(line),
                cells: Vec::new(),
            };
        };
        // A table row is shaped cell by cell, each with only the styles that
        // fall inside it, because the cells are painted at column offsets rather
        // than as one string. A cell's text leaves out the row's hidden markers,
        // exactly as an ordinary row's does.
        let text = self.text.to_string();
        let cells = table
            .cells
            .iter()
            .map(|cell| {
                let pieces =
                    shale_core::mdlayout::visible_pieces(cell.start, cell.end, &self.hidden);
                let map = PieceMap::from_pieces(&pieces);
                if table.delimiter {
                    return CellShaped {
                        line: text_system.shape_line(
                            SharedString::from(""),
                            px(self.size),
                            &build_runs("", &[], &[], &base_font),
                            None,
                        ),
                        map,
                    };
                }
                let painted: String = pieces
                    .iter()
                    .map(|&(s, e)| &text[s - self.start..e - self.start])
                    .collect();
                let (spans, links) = cell_styles(&self.spans, &self.links, &map);
                let runs = build_runs(&painted, &spans, &links, &base_font);
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
        LinePrepaint { line: None, cells }
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
            window.paint_quad(fill(bounds, rgb(theme::CODE_BLOCK_BG)));
        }

        let width;
        let segments: Vec<HitSegment>;
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
            width = line.width();
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
                    rgb(SELECTION),
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
                    rgb(CURSOR),
                ));
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
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::font;

    /// One paint input: the line's text, its markdown spans and its wiki-link
    /// spans (start, end, resolved).
    type Case = (
        &'static str,
        Vec<(usize, usize, Style)>,
        Vec<(usize, usize, bool)>,
    );

    /// The runs must tile the line exactly: gpui matches runs against glyphs by
    /// byte count, so a run map that is short or long styles the wrong text (and
    /// in debug builds trips gpui's own assertions).
    #[test]
    fn runs_tile_the_line() {
        let font = font("monospace");
        let cases: Vec<Case> = vec![
            ("plain text", vec![], vec![]),
            ("", vec![], vec![]),
            (
                "# Heading **bold**",
                vec![
                    (0, 1, Style::HeadingMarker),
                    (0, 18, Style::Heading { level: 1 }),
                    (10, 16, Style::Strong),
                ],
                vec![],
            ),
            ("héllo 🎉 `code` wörld", vec![(13, 19, Style::Code)], vec![]),
            (
                "[[link]] and [[gone]]",
                vec![(0, 8, Style::WikiLink)],
                vec![(0, 8, true), (13, 21, false)],
            ),
        ];
        for (text, spans, links) in cases {
            let runs = build_runs(text, &spans, &links, &font);
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

    /// A cell only sees the styles that fall inside it, re-based onto its own
    /// painted text: a bold span that starts in another column must not leak in,
    /// and one that starts inside must not be clipped away.
    #[test]
    fn cell_styles_clip_and_rebase() {
        // The line is `| **a** | b |`: the strong span covers bytes 2..7 and the
        // first cell's content is bytes 1..7.
        let spans = vec![(2, 7, Style::Strong)];
        let links = vec![(12, 18, true)];
        let map = PieceMap::from_pieces(&[(1, 7)]);
        let (cell_spans, cell_links) = cell_styles(&spans, &links, &map);
        assert_eq!(cell_spans, vec![(1, 6, Style::Strong)]);
        assert!(cell_links.is_empty(), "a link in another cell is dropped");
    }

    /// A cell whose markers are hidden is painted from the pieces that are left,
    /// so its spans have to be re-based onto those pieces rather than onto the
    /// cell's raw content.
    #[test]
    fn cell_styles_follow_the_hidden_markers() {
        // The cell's content is bytes 10..17, `**a** b`, with both `**` hidden.
        let map = PieceMap::from_pieces(&[(12, 13), (15, 17)]);
        let (cell_spans, _) = cell_styles(&[(10, 15, Style::Strong)], &[], &map);
        assert_eq!(
            cell_spans,
            vec![(0, 1, Style::Strong)],
            "the span follows the painted `a`"
        );
    }
}
