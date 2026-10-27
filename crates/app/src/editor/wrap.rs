//! The frame's wrap model: which markdown markers a row hides, what is left of
//! the row once they are gone, and where each row breaks into display lines.
//!
//! This is the part of the editor that has to agree exactly with the paint: the
//! buffer is handed the same break points the line elements draw, so vim's `j`
//! and `k` step over wrapped rows and over hidden markers the way the user sees
//! them.

use gpui::{px, IndentAdjustment, LineFragment, LineWrapperHandle, Pixels};
use shale_core::mdlayout;
use shale_core::mdstyle::{CalloutKind, Style, StyleSpan};
use shale_core::table::Table;

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
    /// The row is a thematic break (`---`, `***`, `___`), which is drawn as a
    /// rule across the editor instead of as its dashes.
    pub(super) rule: bool,
    /// The row is part of an Obsidian callout (`> [!note]`), whose box the line
    /// element paints behind the text. The kind decides the accent colour.
    pub(super) callout: Option<CalloutKind>,
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
    /// The row was collapsed (drawn as no line at all) when this was measured,
    /// so its empty list of starts must not be reused once it is visible again.
    collapsed: bool,
    /// The row was drawn as one line whatever its width (a table row, or a row
    /// painted as an embedded image), so it was never measured for breaks.
    no_wrap: bool,
    /// Absolute byte offsets where the row's display lines start, beginning with
    /// the row itself.
    starts: Vec<usize>,
}
impl MarkdownEditor {
    /// Whether the cursor or the selection touches `start..=end` in buffer
    /// offsets. Live preview shows the syntax of anything it touches and hides
    /// the syntax of everything else, and the same rule decides whether a
    /// collapsed row (a fence line, a table's delimiter) comes back.
    fn touches(&self, selection: Option<(usize, usize)>, start: usize, end: usize) -> bool {
        let cursor = self.buffer.cursor();
        (cursor >= start && cursor <= end)
            || selection
                .map(|(a, h)| a.max(h) >= start && a.min(h) <= end)
                .unwrap_or(false)
    }
    /// Which buffer rows the cursor or the selection touches. Live preview draws
    /// those exactly as the file holds them — markers, fence lines, image
    /// sources — and every other row as its reader sees it.
    pub(super) fn active_rows(&self, selection: Option<(usize, usize)>) -> Vec<bool> {
        (0..self.buffer.line_count())
            .map(|row| {
                self.touches(
                    selection,
                    self.buffer.line_start(row),
                    self.buffer.line_end(row),
                )
            })
            .collect()
    }

    /// The markdown markers hidden on each buffer row, as merged buffer ranges:
    /// empty on the cursor's row, on a selected row, and on a row with no
    /// markers. Obsidian's live preview rule, and what lets the user see and edit
    /// the syntax of the line they are working on.
    pub(super) fn hidden_markers(
        &self,
        selection: Option<(usize, usize)>,
    ) -> Vec<Vec<(usize, usize)>> {
        let line_count = self.buffer.line_count();
        let Some((_, spans)) = self.style_cache.as_ref() else {
            return vec![Vec::new(); line_count];
        };
        let active = self.active_rows(selection);
        let mut out = Vec::with_capacity(line_count);
        // Spans and rows are both in offset order, so one forward walk finds the
        // spans that touch each row.
        let mut first = 0usize;
        for (row, is_active) in active.iter().enumerate() {
            let row_start = self.buffer.line_start(row);
            let row_end = self.buffer.line_end(row);
            if *is_active {
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
            let rule = span.style == Style::Rule;
            let callout = match span.style {
                Style::Callout { kind } => Some(kind),
                _ => None,
            };
            let mut r = row;
            while r < line_count && self.buffer.line_start(r) < span.end {
                let row = &mut rows[r];
                row.wide |= wide;
                row.heading = row.heading.max(heading);
                row.block |= block;
                row.rule |= rule;
                if callout.is_some() {
                    row.callout = callout;
                }
                r += 1;
            }
        }
        rows
    }
    /// The rows live preview does not draw at all, so they take up no vertical
    /// space: the ``` lines of a fenced block the cursor is not inside, and a
    /// table's `---` delimiter row, which is painted as a rule and has no cell
    /// to edit. Either comes back as soon as the cursor is on it (or, for a
    /// fence, anywhere in its block), which is what lets the user keep editing
    /// the syntax — including typing a new table's delimiter row.
    pub(super) fn collapsed_rows(
        &self,
        selection: Option<(usize, usize)>,
        tables: &[Table],
    ) -> Vec<bool> {
        let line_count = self.buffer.line_count();
        let mut out = vec![false; line_count];
        for table in tables {
            let row = match table.rows.get(table.delimiter) {
                Some(row) => row.row,
                None => continue,
            };
            if row >= line_count
                || self.touches(
                    selection,
                    self.buffer.line_start(row),
                    self.buffer.line_end(row),
                )
            {
                continue;
            }
            out[row] = true;
        }

        let Some((_, spans)) = self.style_cache.as_ref() else {
            return out;
        };
        // The scanner emits a `CodeFence` span for the opening and the closing
        // line of every block, in order, so they pair up as they are walked. An
        // unterminated block runs to the end of the note.
        let mut row = 0usize;
        let mut open: Option<usize> = None;
        for span in spans {
            while row < line_count && self.buffer.line_end(row) <= span.start {
                row += 1;
            }
            if span.style != Style::CodeFence {
                continue;
            }
            match open.take() {
                None => open = Some(row),
                Some(first) => {
                    if !self.touches(
                        selection,
                        self.buffer.line_start(first),
                        self.buffer.line_end(row),
                    ) {
                        out[first] = true;
                        out[row] = true;
                    }
                }
            }
        }
        if let Some(first) = open.filter(|_| line_count > 0) {
            let last = line_count - 1;
            if !self.touches(
                selection,
                self.buffer.line_start(first),
                self.buffer.line_end(last),
            ) {
                out[first] = true;
            }
        }
        out
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
        no_wrap: &[bool],
        pieces: &[Vec<(usize, usize)>],
        collapsed: &[bool],
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
            // A collapsed row is not drawn, so it contributes no display line and
            // the buffer's index space steps over it. It is never cached: whether
            // a row is collapsed follows the cursor, which the cache key does not
            // include.
            if collapsed.get(row).copied().unwrap_or(false) {
                segments.push(Vec::new());
                measured.push(CachedRow {
                    preview,
                    collapsed: true,
                    no_wrap: false,
                    starts: Vec::new(),
                });
                continue;
            }
            // A table row is painted as one grid row, and a row drawn as an
            // embedded image is one picture, so neither soft-wraps: the buffer's
            // display model must agree or j/k and the paint fall out of step.
            if no_wrap.get(row).copied().unwrap_or(false) {
                segments.push(starts);
                measured.push(CachedRow {
                    preview,
                    collapsed: false,
                    no_wrap: true,
                    starts: vec![row_start],
                });
                continue;
            }
            let cached_row = cached.as_ref().and_then(|rows| rows.get(row));
            if cached_row.is_some_and(|cached| {
                !cached.collapsed && !cached.no_wrap && cached.preview == preview
            }) {
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
                collapsed: false,
                no_wrap: false,
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
