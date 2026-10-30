//! The painted form of a display line: the buffer's text with live-preview
//! markers left out and a few characters drawn as something else, plus the map
//! back to buffer offsets.
//!
//! Live preview (see [`shale_core::mdlayout`]) removes characters from what is
//! drawn: `## Title` is painted as `Title`, `a **b** c` as `a b c`. The buffer
//! still holds every byte, so the two coordinate spaces have to be translated
//! constantly — a click's x into a buffer offset, the cursor's offset into an x,
//! a soft-wrap break into a buffer boundary. [`PieceMap`] is that translation,
//! and [`LineLayout`] bundles it with the string that actually gets shaped.
//!
//! One character is not merely dropped but *replaced*: a straight quote drawn
//! curly (`"hi"` painted as `“hi”`). That breaks the obvious arithmetic, because
//! the curly glyph is one character and three UTF-8 bytes where the file has one.
//! The rule that keeps this tractable is that **a substituted character is always
//! a piece of its own**, so every other piece still maps with a single
//! subtraction — and nothing has to be mapped from the middle of a run.

use gpui::SharedString;
use shale_core::mdstyle::Style;

/// One run of painted text: where it came from in the note, and where it lands in
/// the painted string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Piece {
    /// The range of the note this run came from.
    buf: (usize, usize),
    /// The range of the painted string it occupies.
    painted: (usize, usize),
    /// The character to draw for this run instead of the note's own text. Only a
    /// substituted character has one, and it is always a run of exactly one byte.
    glyph: Option<char>,
}

impl Piece {
    /// Whether the painted run is a different length from the note's, which is
    /// true only of a substituted character.
    fn expanded(&self) -> bool {
        self.painted.1 - self.painted.0 != self.buf.1 - self.buf.0
    }

    /// The painted index a note offset inside this run stands for. An offset at
    /// the end of a substituted character lands *after* its glyph, which is what
    /// puts the cursor beside the quote rather than inside it.
    fn painted_at(&self, offset: usize) -> usize {
        if self.expanded() {
            if offset <= self.buf.0 {
                self.painted.0
            } else {
                self.painted.1
            }
        } else {
            self.painted.0 + (offset - self.buf.0)
        }
    }

    /// The note offset a painted index inside this run stands for. Every byte
    /// index of a substituted glyph is the same character, so they all answer with
    /// that character's own offset.
    fn offset_at(&self, index: usize) -> usize {
        if self.expanded() {
            self.buf.0
        } else {
            self.buf.0 + (index - self.painted.0)
        }
    }
}

/// The note ranges a painted string is made of, in paint order, each with the
/// place it holds in the painted string.
///
/// A painted line is only one contiguous note range when nothing was hidden and
/// nothing was substituted. Otherwise it is one piece per run of visible text,
/// and the gaps between the pieces are exactly the hidden markers.
#[derive(Clone, Default)]
pub(crate) struct PieceMap {
    pieces: Vec<Piece>,
}

impl PieceMap {
    /// A line painted exactly as the note holds it.
    pub(crate) fn identity(start: usize, end: usize) -> Self {
        PieceMap {
            pieces: vec![Piece {
                buf: (start, end),
                painted: (start, end),
                glyph: None,
            }],
        }
    }

    /// The map for `pieces` with `subs` applied. `pieces` are absolute note ranges
    /// in order, which is also the order they are painted in; a substitution is a
    /// `(note offset, glyph)` pair for a character drawn as something else — a
    /// straight quote drawn curly — and each one becomes a piece of its own.
    ///
    /// Substitutions outside a piece's range, and any that fall inside a hidden
    /// marker, are simply not painted, so they need no special case here.
    pub(crate) fn with_substitutions(pieces: &[(usize, usize)], subs: &[(usize, char)]) -> Self {
        let mut out: Vec<Piece> = Vec::new();
        let mut painted = 0usize;
        for &(start, end) in pieces {
            if end <= start {
                continue;
            }
            let mut at = start;
            for &(offset, glyph) in subs {
                if offset < start {
                    continue;
                }
                if offset >= end {
                    break;
                }
                // A substitution covers exactly one byte, so one that names a byte
                // already painted (a duplicate) would make a piece run backwards.
                if offset < at {
                    continue;
                }
                if offset > at {
                    out.push(Piece {
                        buf: (at, offset),
                        painted: (painted, painted + (offset - at)),
                        glyph: None,
                    });
                    painted += offset - at;
                }
                out.push(Piece {
                    buf: (offset, offset + 1),
                    painted: (painted, painted + glyph.len_utf8()),
                    glyph: Some(glyph),
                });
                painted += glyph.len_utf8();
                at = offset + 1;
            }
            if at < end {
                out.push(Piece {
                    buf: (at, end),
                    painted: (painted, painted + (end - at)),
                    glyph: None,
                });
                painted += end - at;
            }
        }
        PieceMap { pieces: out }
    }

    /// The string this map describes: the note's own ranges, with each substituted
    /// character drawn in place of the one in the file.
    pub(crate) fn painted_text(&self, text: &str) -> String {
        self.painted_from(text, 0)
    }

    /// The same, reading `text` as the note's bytes from `base` on. A table row is
    /// painted cell by cell from a string that holds only its own row, so its
    /// pieces name offsets that start at the row rather than at the note.
    pub(crate) fn painted_text_from(&self, text: &str, base: usize) -> String {
        self.painted_from(text, base)
    }

    fn painted_from(&self, text: &str, base: usize) -> String {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece.glyph {
                Some(glyph) => out.push(glyph),
                None => out.push_str(&text[piece.buf.0 - base..piece.buf.1 - base]),
            }
        }
        out
    }

    /// Whether `offset` is inside one of the painted pieces.
    pub(crate) fn covers(&self, offset: usize) -> bool {
        self.pieces
            .iter()
            .any(|piece| piece.buf.0 <= offset && offset <= piece.buf.1)
    }

    /// The note offset where this line's painted text begins.
    pub(crate) fn first_offset(&self) -> Option<usize> {
        self.pieces.first().map(|piece| piece.buf.0)
    }

    /// The painted-string index of a note offset, clamped into the line. An offset
    /// inside a hidden marker resolves to the character just after it, which is
    /// what puts the cursor next to visible text rather than inside a gap.
    pub(crate) fn index_for_offset(&self, offset: usize) -> usize {
        let mut last_end = 0usize;
        for piece in &self.pieces {
            if offset < piece.buf.0 {
                return piece.painted.0;
            }
            if offset <= piece.buf.1 {
                return piece.painted_at(offset);
            }
            last_end = piece.painted.1;
        }
        last_end
    }

    /// The note offset a painted-string index stands for. An index past the end of
    /// the painted text gives the end of the last piece, so a caller that asks for
    /// "one past the last character" still lands inside the line.
    pub(crate) fn offset_for_index(&self, index: usize) -> usize {
        for piece in &self.pieces {
            if index < piece.painted.1 {
                return piece.offset_at(index);
            }
        }
        self.pieces.last().map(|piece| piece.buf.1).unwrap_or(0)
    }

    /// The pieces, for tests and diagnostics: `(note start, note end, painted
    /// start)`.
    #[cfg(test)]
    pub(crate) fn pieces(&self) -> Vec<(usize, usize, usize)> {
        self.pieces
            .iter()
            .map(|piece| (piece.buf.0, piece.buf.1, piece.painted.0))
            .collect()
    }

    /// The markdown spans that overlap the painted text, clipped to it and
    /// re-based onto it. Input spans are note offsets.
    ///
    /// A span that straddles a hidden marker becomes two, and the scanner's order
    /// is preserved so a later span still wins over an earlier one. A span that
    /// covers a substituted character is widened to the glyph's bytes, which is
    /// what keeps a curly quote styled like the quote it replaced.
    pub(crate) fn rebase_spans(
        &self,
        spans: &[(usize, usize, Style)],
    ) -> Vec<(usize, usize, Style)> {
        let mut out = Vec::new();
        for &(start, end, style) in spans {
            for piece in &self.pieces {
                let (lo, hi) = (start.max(piece.buf.0), end.min(piece.buf.1));
                if hi > lo {
                    out.push((piece.painted_at(lo), piece.painted_at(hi), style));
                }
            }
        }
        out
    }

    /// The wiki-link spans that overlap the painted text, clipped and re-based
    /// the same way: a link whose `[[` the view hid comes back as the runs that are
    /// painted, and a link over a substituted character widens to the glyph.
    pub(crate) fn rebase_links(&self, links: &[(usize, usize, bool)]) -> Vec<(usize, usize, bool)> {
        let mut out = Vec::new();
        for &(start, end, resolved) in links {
            for piece in &self.pieces {
                let (lo, hi) = (start.max(piece.buf.0), end.min(piece.buf.1));
                if hi > lo {
                    out.push((piece.painted_at(lo), piece.painted_at(hi), resolved));
                }
            }
        }
        out
    }

    /// The checker's ranges that overlap the painted text, clipped and re-based
    /// the same way. A range that falls entirely in a hidden marker (a heading's
    /// `#`, say) contributes nothing, which is right: nothing is painted there to
    /// underline. Any other list of note byte ranges maps the same way, which is
    /// how the `/` search matches get onto the painted text.
    pub(crate) fn rebase_marks(&self, marks: &[(usize, usize)]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for &(start, end) in marks {
            for piece in &self.pieces {
                let (lo, hi) = (start.max(piece.buf.0), end.min(piece.buf.1));
                if hi > lo {
                    out.push((piece.painted_at(lo), piece.painted_at(hi)));
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
    /// The painted text for `pieces` of `text`, with `subs` drawn in place of the
    /// characters they name, and the map from it back to the note. With one piece
    /// and no substitutions this is the whole display line unchanged.
    pub(crate) fn build(text: &str, pieces: &[(usize, usize)], subs: &[(usize, char)]) -> Self {
        let map = PieceMap::with_substitutions(pieces, subs);
        let painted = map.painted_text(text);
        LineLayout {
            text: painted.into(),
            map,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `hello **wor**ld` with the `**` markers hidden: the painted text is
    /// `hello world`, made of the pieces `(0,6)`, `(8,11)` and `(13,15)`.
    fn layout() -> LineLayout {
        let text = "hello **wor**ld";
        LineLayout::build(text, &[(0, 6), (8, 11), (13, 15)], &[])
    }

    #[test]
    fn painted_text_and_map() {
        let l = layout();
        assert_eq!(l.text.as_str(), "hello world");
        assert_eq!(l.map.pieces(), vec![(0, 6, 0), (8, 11, 6), (13, 15, 9)]);
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
        let l = LineLayout::build("## ", &[], &[]);
        assert_eq!(l.text.as_str(), "");
        assert_eq!(l.map.index_for_offset(0), 0);
        assert_eq!(l.map.offset_for_index(0), 0);
        assert!(!l.map.covers(1));
    }

    #[test]
    fn wrap_breaks_map_onto_piece_boundaries() {
        let pieces = [(0usize, 6), (8, 11), (13, 15)];
        let map = PieceMap::with_substitutions(&pieces, &[]);
        assert_eq!(map.offset_for_index(0), 0);
        assert_eq!(map.offset_for_index(3), 3);
        assert_eq!(map.offset_for_index(7), 9);
        // A break at the seam goes to the piece that follows, leaving the hidden
        // gap on the line above.
        assert_eq!(map.offset_for_index(6), 8);
        assert_eq!(map.offset_for_index(9), 13);
        assert_eq!(map.offset_for_index(10), 14);
        assert_eq!(map.offset_for_index(12), 15);
    }

    /// `say "hi"` with the opening quote drawn curly: the painted text is one byte
    /// longer than the note's, and every mapping has to agree.
    fn curly() -> LineLayout {
        let text = "say \"hi\"";
        LineLayout::build(text, &[(0, 8)], &[(4, '\u{201c}')])
    }

    #[test]
    fn a_substituted_character_is_painted_longer_than_it_is_in_the_note() {
        let l = curly();
        assert_eq!(l.text.as_str(), "say \u{201c}hi\"");
        // The substitution is a piece of its own: one byte of note, three painted.
        assert_eq!(l.map.pieces(), vec![(0, 4, 0), (4, 5, 4), (5, 8, 7)]);
    }

    #[test]
    fn a_substituted_character_maps_to_itself() {
        let l = curly();
        // The quote's own offset, and the offset just past it, bracket the glyph.
        assert_eq!(l.map.index_for_offset(4), 4);
        assert_eq!(l.map.index_for_offset(5), 7);
        // Anywhere inside the glyph's three bytes answers with the quote's offset,
        // so a click on a curly quote lands on the straight one in the file.
        for index in [4, 5, 6] {
            assert_eq!(l.map.offset_for_index(index), 4, "index {index}");
        }
        // And the text after it is where it should be, three bytes along.
        assert_eq!(l.map.index_for_offset(6), 8);
        assert_eq!(l.map.offset_for_index(8), 6);
        assert_eq!(l.map.offset_for_index(10), 8);
    }

    #[test]
    fn a_span_over_a_substituted_character_widens_to_the_glyph() {
        let l = curly();
        // A quotation styled over `"hi"` covers the curly glyph's bytes, not just
        // the one byte the note holds.
        assert_eq!(
            l.map.rebase_marks(&[(4, 8)]),
            vec![(4, 7), (7, 10)],
            "the painted ranges are three bytes for the glyph and three for hi\""
        );
    }

    #[test]
    fn substitutions_and_hidden_markers_agree() {
        // `**say "hi"**` with both `**` markers hidden and the opening quote
        // curly: the painted text is `say “hi"`, four bytes shorter than the note
        // and two longer than what the pieces alone would give.
        let text = "**say \"hi\"**";
        let l = LineLayout::build(text, &[(2, 10)], &[(6, '\u{201c}')]);
        assert_eq!(l.text.as_str(), "say \u{201c}hi\"");
        assert_eq!(l.map.pieces(), vec![(2, 6, 0), (6, 7, 4), (7, 10, 7)]);
        assert_eq!(
            l.map.index_for_offset(0),
            0,
            "a hidden marker maps to the front"
        );
        assert_eq!(l.map.index_for_offset(6), 4);
        assert_eq!(l.map.index_for_offset(7), 7);
        assert_eq!(l.map.offset_for_index(4), 6);
        assert_eq!(l.map.offset_for_index(9), 9, "the straight quote is itself");
        assert_eq!(
            l.map.index_for_offset(11),
            10,
            "the hidden markers at the end clamp to the painted end"
        );
    }

    #[test]
    fn a_substitution_inside_a_hidden_marker_is_not_painted() {
        // A quote inside a hidden marker has nothing painted for it, so the
        // substitution is simply not in any piece and changes nothing.
        let text = "\"x\"";
        let l = LineLayout::build(text, &[(1, 2)], &[(0, '\u{201c}'), (2, '\u{201d}')]);
        assert_eq!(l.text.as_str(), "x");
        assert_eq!(l.map.pieces(), vec![(1, 2, 0)]);
    }
}
