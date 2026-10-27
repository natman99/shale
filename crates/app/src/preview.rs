//! The painted form of a display line: the buffer's text with live-preview
//! markers left out, plus the map back to buffer offsets.
//!
//! Live preview (see [`shale_core::mdlayout`]) removes characters from what is
//! drawn: `## Title` is painted as `Title`, `a **b** c` as `a b c`. The buffer
//! still holds every byte, so the two coordinate spaces have to be translated
//! constantly — a click's x into a buffer offset, the cursor's offset into an x,
//! a soft-wrap break into a buffer boundary. [`PieceMap`] is that translation,
//! and [`LineLayout`] bundles it with the string that actually gets shaped.

use gpui::SharedString;
use shale_core::mdstyle::Style;

/// The buffer ranges a painted string is made of, in paint order, each with the
/// byte index in the painted string where it starts.
///
/// A painted line is only one contiguous buffer range when nothing was hidden.
/// Otherwise it is one piece per run of visible text, and the gaps between the
/// pieces are exactly the hidden markers.
#[derive(Clone, Default)]
pub(crate) struct PieceMap {
    /// `(buffer start, buffer end, painted index)`, sorted, non-overlapping and
    /// contiguous in the painted string. Empty when the line paints nothing.
    pieces: Vec<(usize, usize, usize)>,
}

impl PieceMap {
    /// A line painted exactly as the buffer holds it.
    pub(crate) fn identity(start: usize, end: usize) -> Self {
        PieceMap {
            pieces: vec![(start, end, 0)],
        }
    }

    /// The map for `pieces`: absolute buffer ranges in order, which is also the
    /// order they are painted in.
    pub(crate) fn from_pieces(pieces: &[(usize, usize)]) -> Self {
        let mut out = Vec::with_capacity(pieces.len());
        let mut painted = 0usize;
        for &(start, end) in pieces {
            if end <= start {
                continue;
            }
            out.push((start, end, painted));
            painted += end - start;
        }
        PieceMap { pieces: out }
    }

    /// Whether `offset` is inside one of the painted pieces.
    pub(crate) fn covers(&self, offset: usize) -> bool {
        self.pieces
            .iter()
            .any(|&(start, end, _)| start <= offset && offset <= end)
    }

    /// The buffer offset where this line's painted text begins.
    pub(crate) fn first_offset(&self) -> Option<usize> {
        self.pieces.first().map(|&(start, _, _)| start)
    }

    /// The painted-string index of a buffer offset, clamped into the line. An
    /// offset inside a hidden marker resolves to the character just after it,
    /// which is what puts the cursor next to visible text rather than inside a
    /// gap.
    pub(crate) fn index_for_offset(&self, offset: usize) -> usize {
        let mut last_end = 0usize;
        for &(start, end, painted) in &self.pieces {
            if offset < start {
                return painted;
            }
            if offset <= end {
                return painted + (offset - start);
            }
            last_end = painted + (end - start);
        }
        last_end
    }

    /// The buffer offset a painted-string index stands for. An index past the
    /// end of the painted text gives the end of the last piece, so a caller that
    /// asks for "one past the last character" still lands inside the line.
    pub(crate) fn offset_for_index(&self, index: usize) -> usize {
        for &(start, end, painted) in &self.pieces {
            if index < painted + (end - start) {
                return start + index.saturating_sub(painted);
            }
        }
        self.pieces.last().map(|&(_, end, _)| end).unwrap_or(0)
    }

    /// The pieces, for tests and diagnostics.
    #[cfg(test)]
    pub(crate) fn pieces(&self) -> &[(usize, usize, usize)] {
        &self.pieces
    }

    /// The markdown spans that overlap the painted text, clipped to it and
    /// re-based onto it. Input spans are buffer offsets.
    ///
    /// A span that straddles a hidden marker becomes two, and the scanner's
    /// order is preserved so a later span still wins over an earlier one.
    pub(crate) fn rebase_spans(
        &self,
        spans: &[(usize, usize, Style)],
    ) -> Vec<(usize, usize, Style)> {
        let mut out = Vec::new();
        for &(start, end, style) in spans {
            for &(piece_start, piece_end, painted) in &self.pieces {
                let (lo, hi) = (start.max(piece_start), end.min(piece_end));
                if hi > lo {
                    out.push((
                        painted + (lo - piece_start),
                        painted + (hi - piece_start),
                        style,
                    ));
                }
            }
        }
        out
    }

    /// The wiki-link spans that overlap the painted text, clipped and re-based
    /// the same way.
    pub(crate) fn rebase_links(&self, links: &[(usize, usize, bool)]) -> Vec<(usize, usize, bool)> {
        let mut out = Vec::new();
        for &(start, end, resolved) in links {
            for &(piece_start, piece_end, painted) in &self.pieces {
                let (lo, hi) = (start.max(piece_start), end.min(piece_end));
                if hi > lo {
                    out.push((
                        painted + (lo - piece_start),
                        painted + (hi - piece_start),
                        resolved,
                    ));
                }
            }
        }
        out
    }
}

/// One display line as it is painted.
pub(crate) struct LineLayout {
    /// The text that gets shaped and painted.
    pub(crate) text: SharedString,
    /// Which bytes of the note that text came from.
    pub(crate) map: PieceMap,
}

impl LineLayout {
    /// The painted text for `pieces` of `text`, and the map from it back to the
    /// buffer. With one piece this is the whole display line unchanged.
    pub(crate) fn build(text: &str, pieces: &[(usize, usize)]) -> Self {
        let map = PieceMap::from_pieces(pieces);
        let painted = pieces
            .iter()
            .map(|&(start, end)| &text[start..end])
            .collect::<String>();
        LineLayout {
            text: painted.into(),
            map,
        }
    }
}

/// The buffer offset a byte index into a row's *visible* text stands for, where
/// `pieces` are that row's visible ranges. Used to turn a soft-wrap break in the
/// painted text into a boundary in the buffer.
///
/// A break that lands exactly on a piece boundary belongs to the piece that
/// follows, so the hidden gap before it stays on the line above.
pub(crate) fn offset_for_visible_index(pieces: &[(usize, usize)], index: usize) -> usize {
    let mut painted = 0usize;
    for &(start, end) in pieces {
        if end <= start {
            continue;
        }
        if index < painted + (end - start) {
            return start + index.saturating_sub(painted);
        }
        painted += end - start;
    }
    pieces.last().map(|&(_, end)| end).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `hello **wor**ld` with the `**` markers hidden: the painted text is
    /// `hello world`, made of the pieces `(0,6)`, `(8,11)` and `(13,15)`.
    fn layout() -> LineLayout {
        let text = "hello **wor**ld";
        LineLayout::build(text, &[(0, 6), (8, 11), (13, 15)])
    }

    #[test]
    fn painted_text_and_map() {
        let l = layout();
        assert_eq!(l.text.as_str(), "hello world");
        assert_eq!(l.map.pieces(), &[(0, 6, 0), (8, 11, 6), (13, 15, 9)]);
    }

    #[test]
    fn offsets_and_indices_round_trip() {
        let l = layout();
        // Inside painted text the mapping is exact.
        for (offset, index) in [
            (0, 0),
            (5, 5),
            (6, 6),
            (8, 6),
            (10, 8),
            (11, 9),
            (13, 9),
            (14, 10),
            (15, 11),
        ] {
            assert_eq!(l.map.index_for_offset(offset), index, "offset {offset}");
        }
        // A hidden marker's offset resolves to the character after it.
        assert_eq!(l.map.index_for_offset(7), 6);
        assert_eq!(l.map.index_for_offset(12), 9);
        // And an offset past the end clamps to the end of the painted text.
        assert_eq!(l.map.index_for_offset(99), 11);

        assert_eq!(l.map.offset_for_index(0), 0);
        assert_eq!(l.map.offset_for_index(5), 5);
        assert_eq!(l.map.offset_for_index(6), 8);
        assert_eq!(l.map.offset_for_index(9), 13);
        assert_eq!(l.map.offset_for_index(11), 15, "one past the end");
        assert_eq!(l.map.offset_for_index(40), 15, "clamped");
    }

    #[test]
    fn spans_are_split_around_a_hidden_marker() {
        let l = layout();
        // One span over the whole line becomes one per piece, in order.
        let spans = l.map.rebase_spans(&[(0, 15, Style::Quote)]);
        assert_eq!(
            spans,
            vec![
                (0, 6, Style::Quote),
                (6, 9, Style::Quote),
                (9, 11, Style::Quote)
            ]
        );
        // A span inside the hidden gap contributes nothing.
        assert!(l.map.rebase_spans(&[(6, 8, Style::Punctuation)]).is_empty());
        // And a span that overlaps a piece edge is clipped to it.
        assert_eq!(
            l.map.rebase_spans(&[(4, 12, Style::Strong)]),
            vec![(4, 6, Style::Strong), (6, 9, Style::Strong)]
        );
    }

    #[test]
    fn an_empty_line_maps_to_nothing() {
        let l = LineLayout::build("## ", &[]);
        assert_eq!(l.text.as_str(), "");
        assert_eq!(l.map.index_for_offset(0), 0);
        assert_eq!(l.map.offset_for_index(0), 0);
        assert!(!l.map.covers(1));
    }

    #[test]
    fn wrap_breaks_map_onto_piece_boundaries() {
        let pieces = [(0usize, 6), (8, 11), (13, 15)];
        assert_eq!(offset_for_visible_index(&pieces, 0), 0);
        assert_eq!(offset_for_visible_index(&pieces, 3), 3);
        assert_eq!(offset_for_visible_index(&pieces, 7), 9);
        // A break at the seam goes to the piece that follows, leaving the hidden
        // gap on the line above.
        assert_eq!(offset_for_visible_index(&pieces, 6), 8);
        assert_eq!(offset_for_visible_index(&pieces, 9), 13);
        assert_eq!(offset_for_visible_index(&pieces, 10), 14);
        assert_eq!(offset_for_visible_index(&pieces, 12), 15);
    }
}
