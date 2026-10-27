//! The element that paints one display line: shaped text with markdown
//! styling, the selection and cursor quads, and the record the editor keeps for
//! hit-testing.
//!
//! A "display line" is a segment of a buffer row: soft wrap splits a long row
//! into several of these, and each one is painted and measured separately.

use gpui::{
    fill, px, relative, rgb, App, Bounds, Element, ElementId, ElementInputHandler, Entity, Font,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString,
    Style as ElementStyle, TextRun, Window,
};
use shale_core::mdstyle::Style;

use crate::editor::MarkdownEditor;
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

/// A display line captured during paint, for hit-testing and IME placement.
#[derive(Clone)]
pub(crate) struct HitLine {
    /// Flat display-line index.
    pub(crate) index: usize,
    /// First buffer byte of the segment.
    pub(crate) start: usize,
    /// End of the segment, exclusive.
    pub(crate) end: usize,
    /// The shaped line, which answers x <-> byte-index questions.
    pub(crate) line: ShapedLine,
    /// Where it was painted.
    pub(crate) bounds: Bounds<Pixels>,
}

/// One painted line: text, optional selection and cursor quads.
pub(crate) struct LineElement {
    pub(crate) editor: Entity<MarkdownEditor>,
    pub(crate) row: usize,
    /// Flat display-line index of this row's segment.
    pub(crate) index: usize,
    /// Buffer byte offsets this segment covers (`end` is exclusive).
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) text: SharedString,
    pub(crate) line_sel: Option<(usize, usize)>,
    pub(crate) cursor_col: Option<usize>,
    pub(crate) insert: bool,
    /// (start, end, resolved) byte spans of wiki-links within `text`.
    pub(crate) links: Vec<(usize, usize, bool)>,
    /// (start, end, style) markdown spans within `text`, in the order the
    /// scanner emitted them (block spans first, so later spans win).
    pub(crate) spans: Vec<(usize, usize, Style)>,
    /// Text size to shape and paint at; the row's height follows it.
    pub(crate) size: f32,
    /// Whether the row belongs to a fenced code block, whose background spans
    /// the full width of the editor rather than just its text.
    pub(crate) block: bool,
}

pub(crate) struct LinePrepaint {
    line: ShapedLine,
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
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let font = window.text_style().font();
        let runs = build_runs(&self.text, &self.spans, &self.links, &font);
        let line = window
            .text_system()
            .shape_line(self.text.clone(), px(self.size), &runs, None);
        LinePrepaint { line }
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
        let line = &prepaint.line;

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

        // Run backgrounds (inline code and highlights) are a separate pass over
        // the decoration runs, painted under the glyphs.
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
            let width = if self.insert {
                px(1.5)
            } else {
                let next = line.x_for_index((col + 1).min(self.text.len()));
                (next - x).max(px(7.))
            };
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(bounds.left() + x, bounds.top()),
                    gpui::point(bounds.left() + x + width, bounds.bottom()),
                ),
                rgb(CURSOR),
            ));
        }

        // Record for hit-testing.
        self.editor.update(cx, |editor, _cx| {
            editor.hit_lines.push(HitLine {
                index: self.index,
                start: self.start,
                end: self.end,
                line: line.clone(),
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
}
