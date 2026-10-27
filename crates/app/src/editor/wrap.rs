//! The frame's wrap model: which markdown markers a row hides, what is left of
//! the row once they are gone, and where each row breaks into display lines.
//!
//! This is the part of the editor that has to agree exactly with the paint: the
//! buffer is handed the same break points the line elements draw, so vim's `j`
//! and `k` step over wrapped rows and over hidden markers the way the user sees
//! them.

use gpui::{px, IndentAdjustment, LineFragment, LineWrapperHandle, Pixels};
use shale_core::mdlayout;
use shale_core::mdstyle::{Style, StyleSpan};

use crate::line::DisplayLine;
use crate::preview::offset_for_visible_index;
use crate::theme;

use super::MarkdownEditor;

/// How a buffer row will be presented, derived from its markdown style spans.
/// The wrap model has to measure a row with the face and size it will be painted
/// with, or the break points stop matching what the user sees.
#[derive(Clone, Copy, Default)]
pub(super) struct RowStyle {
    /// The row is painted with a face wider than the regular one (a heading,
    /// strong or quoted text).
    pub(super) wide: bool,
    /// The largest heading level on the row; 0 is body text.
    pub(super) heading: u8,
    /// The row belongs to a fenced code block, which gets a full-width
    /// background.
    pub(super) block: bool,
}

/// Which line wrapper a row needs: one per heading level, regular and wide face.
fn wrapper_index(row: RowStyle) -> usize {
    row.heading as usize * 2 + usize::from(row.wide)
}

/// Cached soft-wrap boundaries for one buffer row, together with the live-preview
/// state they were measured in. Hiding a row's markers changes what has to fit,
/// so a row is only re-wrapped when that flag changes rather than every time the
/// cursor moves.
pub(super) struct CachedRow {
    /// Whether the row's markdown markers were hidden when it was measured.
    preview: bool,
    /// Absolute byte offsets where the row's display lines start, beginning with
    /// the row itself.
    starts: Vec<usize>,
}
impl MarkdownEditor {
    /// The markdown markers hidden on each buffer row, as merged buffer ranges:
    /// empty on the cursor's row, on a selected row, and on a row with no
    /// markers. Obsidian's live preview rule, and what lets the user see and edit
    /// the syntax of the line they are working on.
    pub(super) fn hidden_markers(
        &self,
        selection: Option<(usize, usize)>,
    ) -> Vec<Vec<(usize, usize)>> {
        let line_count = self.buffer.line_count();
        let cursor = self.buffer.cursor();
        let Some((_, spans)) = self.style_cache.as_ref() else {
            return vec![Vec::new(); line_count];
        };
        let mut out = Vec::with_capacity(line_count);
        // Spans and rows are both in offset order, so one forward walk finds the
        // spans that touch each row.
        let mut first = 0usize;
        for row in 0..line_count {
            let row_start = self.buffer.line_start(row);
            let row_end = self.buffer.line_end(row);
            let active = (cursor >= row_start && cursor <= row_end)
                || selection
                    .map(|(a, h)| {
                        let (lo, hi) = (a.min(h), a.max(h));
                        hi >= row_start && lo <= row_end
                    })
                    .unwrap_or(false);
            if active {
                out.push(Vec::new());
                continue;
            }
            while first < spans.len() && spans[first].end <= row_start {
                first += 1;
            }
            let touching: Vec<StyleSpan> = spans[first..]
                .iter()
                .take_while(|s| s.start < row_end)
                .copied()
                .collect();
            out.push(mdlayout::hidden_ranges(
                self.buffer.text(),
                &touching,
                row_start,
                row_end,
            ));
        }
        out
    }

    /// The visible pieces of every buffer row: the row minus its hidden markers.
    /// A row with nothing hidden is one piece, painted as the buffer holds it.
    pub(super) fn row_pieces(&self, hidden: &[Vec<(usize, usize)>]) -> Vec<Vec<(usize, usize)>> {
        (0..self.buffer.line_count())
            .map(|row| {
                mdlayout::visible_pieces(
                    self.buffer.line_start(row),
                    self.buffer.line_end(row),
                    hidden.get(row).map(|h| h.as_slice()).unwrap_or(&[]),
                )
            })
            .collect()
    }
    /// Per-buffer-row presentation facts the wrap model needs, gathered from the
    /// style spans in one pass (spans and rows are both in offset order, so a
    /// single forward walk is enough).
    pub(super) fn row_styles(&self) -> Vec<RowStyle> {
        let line_count = self.buffer.line_count();
        let mut rows = vec![RowStyle::default(); line_count];
        let Some((_, spans)) = self.style_cache.as_ref() else {
            return rows;
        };
        let mut row = 0usize;
        for span in spans {
            while row < line_count && self.buffer.line_end(row) <= span.start {
                row += 1;
            }
            let style = theme::RunStyle::for_style(span.style);
            let wide = style.is_wide();
            let heading = match span.style {
                Style::Heading { level } => level,
                Style::HeadingMarker => 1,
                _ => 0,
            };
            let block = matches!(span.style, Style::CodeBlock | Style::CodeFence);
            let mut r = row;
            while r < line_count && self.buffer.line_start(r) < span.end {
                let row = &mut rows[r];
                row.wide |= wide;
                row.heading = row.heading.max(heading);
                row.block |= block;
                r += 1;
            }
        }
        rows
    }
    /// Build this frame's display lines: buffer rows split at the wrap points
    /// gpui computes for the current width. The exact same boundaries are handed
    /// to the buffer, so vim's `j`/`k` step over wrapped rows like neovim's and
    /// the buffer's display-line index space is identical to the painted one (if
    /// they ever disagreed the cursor would be drawn on the wrong row).
    ///
    /// A row is measured with the face and size it will be painted with —
    /// `wrappers` holds one line wrapper per (heading level, wide or not), which
    /// is what keeps break points matching the paint — and against the text live
    /// preview actually leaves visible (`pieces[row]`), so hidden markers do not
    /// take up room. Boundaries are cached until the text, the width, or a row's
    /// live-preview state changes.
    pub(super) fn display_lines(
        &mut self,
        width: Pixels,
        wrappers: &mut [LineWrapperHandle],
        rows: &[RowStyle],
        table_rows: &[bool],
        pieces: &[Vec<(usize, usize)>],
    ) -> Vec<DisplayLine> {
        let line_count = self.buffer.line_count();
        let change_id = self.buffer.change_id();
        let cached = self
            .wrap_cache
            .as_ref()
            .filter(|(id, w, _)| *id == change_id && *w == width)
            .map(|(_, _, rows)| rows);

        let mut segments: Vec<Vec<usize>> = Vec::with_capacity(line_count);
        let mut measured: Vec<CachedRow> = Vec::with_capacity(line_count);
        for row in 0..line_count {
            let row_start = self.buffer.line_start(row);
            let row_end = self.buffer.line_end(row);
            let mut starts = vec![row_start];
            let row_pieces = pieces.get(row).cloned().unwrap_or_default();
            // Whether this row is painted with its markers hidden, which is what
            // the wrap cache is keyed on.
            let preview = row_pieces.as_slice() != [(row_start, row_end)];
            // A table row is painted as one grid row and never soft-wraps, so
            // the buffer's display model must agree or j/k and the painted lines
            // fall out of step.
            if table_rows.get(row).copied().unwrap_or(false) {
                segments.push(starts);
                measured.push(CachedRow {
                    preview,
                    starts: vec![row_start],
                });
                continue;
            }
            let cached_row = cached.as_ref().and_then(|rows| rows.get(row));
            if cached_row.is_some_and(|cached| cached.preview == preview) {
                // Re-validate even a cached boundary against the current row, so
                // a cache key that ever fails to notice an edit cannot push a
                // segment past the end of the row.
                for &s in &cached_row.expect("checked above").starts {
                    if s > row_start && s < row_end && s > *starts.last().unwrap() {
                        starts.push(s);
                    }
                }
            } else if width > px(0.) {
                // The wrapper sees the painted text, and its break points are
                // mapped back onto buffer offsets in the visible pieces.
                let visible: String = row_pieces
                    .iter()
                    .map(|&(s, e)| &self.buffer.text()[s..e])
                    .collect();
                let row_style = rows.get(row).copied().unwrap_or_default();
                let wrapper = &mut wrappers[wrapper_index(row_style)];
                for boundary in wrapper.wrap_line(
                    &[LineFragment::text(&visible)],
                    width,
                    IndentAdjustment::NoIndent,
                ) {
                    let s = offset_for_visible_index(&row_pieces, boundary.ix);
                    if s > row_start && s < row_end && s > *starts.last().unwrap() {
                        starts.push(s);
                    }
                }
            }
            // width == 0 means no measured width yet (the first frame): one
            // display line per row.
            measured.push(CachedRow {
                preview,
                starts: starts.clone(),
            });
            segments.push(starts);
        }
        if cached.is_none() {
            self.wrap_cache = Some((change_id, width, measured));
        }

        self.buffer.set_display_segments(segments.clone());
        let mut out = Vec::new();
        for (row, starts) in segments.iter().enumerate() {
            let row_end = self.buffer.line_end(row);
            let size = theme::font_size(rows.get(row).copied().unwrap_or_default().heading);
            for (i, &start) in starts.iter().enumerate() {
                let end = starts.get(i + 1).copied().unwrap_or(row_end);
                out.push(DisplayLine {
                    row,
                    start,
                    end,
                    size,
                });
            }
        }
        out
    }
}
