//! Live preview: which markdown markers to hide, and what is left to paint.
//!
//! Obsidian's live preview hides the syntax of a line the user is not editing:
//! the `#` of a heading, the `**` around bold text, a blockquote's `>`, the
//! brackets and url of a link. Nothing here changes the buffer — every byte stays
//! in the file, in the undo history and in the cursor's world. This module only
//! answers the presentation question: given a row's [`StyleSpan`]s, which ranges
//! of it should not be drawn ([`hidden_ranges`]), and which ranges remain
//! ([`visible_pieces`]).
//!
//! The caller decides *when* to hide: a row the cursor or selection is on stays
//! fully visible, which is what lets the user read and edit the syntax they are
//! working on. Everything downstream — shaping, soft wrap, hit-testing, the
//! cursor quad — then works on the visible pieces rather than the raw row, so
//! what the user sees and where they click stay in step.

use crate::mdstyle::{Style, StyleSpan};

/// Whether a style is markdown syntax rather than content, and so hidden in
/// live preview.
///
/// Deliberately *not* hidden: list bullets and numbers, task checkboxes, `#tags`
/// and thematic breaks, all of which Obsidian keeps on screen, and the fenced
/// block's ``` lines, which are the only sign of where the block starts and
/// ends.
pub fn is_marker(style: Style) -> bool {
    matches!(
        style,
        Style::HeadingMarker
            | Style::QuoteMarker
            | Style::Punctuation
            | Style::LinkUrl
            | Style::LinkTarget
    )
}

/// The ranges of `row_start..row_end` that live preview hides, sorted and
/// merged, each non-empty and inside the row.
///
/// `spans` may be the whole buffer's spans; only the ones that touch the row are
/// considered. A heading's `#` markers take the spaces after them along, so a
/// heading does not begin with a stray indent; a blockquote's `>` does *not*
/// take its space, which leaves the quoted text indented by one cell.
pub fn hidden_ranges(
    text: &str,
    spans: &[StyleSpan],
    row_start: usize,
    row_end: usize,
) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut found: Vec<(usize, usize)> = spans
        .iter()
        .filter(|span| is_marker(span.style))
        .filter_map(|span| {
            let start = span.start.max(row_start);
            let mut end = span.end.min(row_end);
            if end <= start {
                return None;
            }
            if span.style == Style::HeadingMarker {
                while end < row_end && (bytes[end] == b' ' || bytes[end] == b'\t') {
                    end += 1;
                }
            }
            Some((start, end))
        })
        .collect();
    found.sort_unstable();

    // Merge overlapping and adjacent ranges: nested spans (a quote holding
    // emphasis, a heading holding a link) produce many small ones, and the
    // caller walks this list once.
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(found.len());
    for (start, end) in found {
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

/// The ranges of `row_start..row_end` that get painted: the row with `hidden`
/// removed, in order, each non-empty.
///
/// An empty row, or one that is entirely markers, produces no pieces at all —
/// there is nothing to draw, but the row still occupies a display line.
pub fn visible_pieces(
    row_start: usize,
    row_end: usize,
    hidden: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = row_start;
    for &(start, end) in hidden {
        if end <= at {
            continue;
        }
        if start >= row_end {
            break;
        }
        if start > at {
            out.push((at, start.min(row_end)));
        }
        at = end.max(at);
        if at >= row_end {
            break;
        }
    }
    if at < row_end {
        out.push((at, row_end));
    }
    out
}

/// The visible pieces of one row, in one call: [`hidden_ranges`] then
/// [`visible_pieces`].
pub fn row_pieces(
    text: &str,
    spans: &[StyleSpan],
    row_start: usize,
    row_end: usize,
) -> Vec<(usize, usize)> {
    let hidden = hidden_ranges(text, spans, row_start, row_end);
    visible_pieces(row_start, row_end, &hidden)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdstyle::style_spans;

    /// The source each visible piece of the whole text covers. Only valid for
    /// single-row test inputs, where the row is the whole text.
    fn shown(text: &str) -> String {
        let spans = style_spans(text);
        row_pieces(text, &spans, 0, text.len())
            .iter()
            .map(|&(s, e)| &text[s..e])
            .collect()
    }

    #[test]
    fn heading_markers_and_their_space_go_away() {
        assert_eq!(shown("## Title"), "Title");
        // No trailing space: the marker is the whole prefix.
        assert_eq!(shown("#Title"), "#Title", "not a heading at all");
        // A heading that is only a marker has nothing left to paint.
        assert!(row_pieces("# ", &style_spans("# "), 0, 2).is_empty());
    }

    #[test]
    fn inline_markers_go_away() {
        assert_eq!(shown("a **b** c"), "a b c");
        assert_eq!(shown("`code`"), "code");
        assert_eq!(shown("~~s~~ and ==h=="), "s and h");
        assert_eq!(shown(r"\* literal"), "* literal");
    }

    #[test]
    fn quotes_keep_their_indent() {
        // The `>` is hidden but its space is not, which is what leaves the quote
        // visibly indented.
        assert_eq!(shown("> quoted"), " quoted");
        assert_eq!(shown("> > nested"), "  nested");
    }

    #[test]
    fn links_show_their_text_only() {
        assert_eq!(shown("[label](https://example.com)"), "label");
        assert_eq!(shown("[[Some Note]]"), "Some Note");
        assert_eq!(shown("[[Some Note|An Alias]]"), "An Alias");
        assert_eq!(shown("![[diagram.png]]"), "diagram.png");
    }

    #[test]
    fn structure_stays_visible() {
        assert_eq!(shown("- a bullet"), "- a bullet");
        assert_eq!(shown("1. a number"), "1. a number");
        assert_eq!(shown("- [x] done"), "- [x] done");
        assert_eq!(shown("a #tag here"), "a #tag here");
        assert_eq!(shown("---"), "---");
        assert_eq!(shown("```"), "```");
    }

    #[test]
    fn hidden_ranges_are_merged_and_inside_the_row() {
        let text = "# **bold** title";
        let spans = style_spans(text);
        let hidden = hidden_ranges(text, &spans, 0, text.len());
        assert_eq!(
            hidden,
            vec![(0, 4), (8, 10)],
            "the heading marker, the space it takes with it and the opening `**` merge into one range"
        );
        assert_eq!(shown(text), "bold title");
        for window in hidden.windows(2) {
            assert!(window[0].1 < window[1].0, "ranges must not touch");
        }
    }

    #[test]
    fn pieces_tile_the_row_minus_the_hidden_ranges() {
        let text = "plain **bold** `code` [[Note|Alias]] tail";
        let spans = style_spans(text);
        let hidden = hidden_ranges(text, &spans, 0, text.len());
        let pieces = visible_pieces(0, text.len(), &hidden);

        let mut covered = 0usize;
        let mut previous_end = 0usize;
        for &(start, end) in &pieces {
            assert!(start < end, "no empty pieces");
            assert!(start >= previous_end, "pieces must not overlap");
            // No piece may contain a hidden byte.
            for &(h_start, h_end) in &hidden {
                assert!(
                    h_end <= start || h_start >= end,
                    "{:?} in {:?}",
                    (start, end),
                    (h_start, h_end)
                );
            }
            covered += end - start;
            previous_end = end;
        }
        let hidden_bytes: usize = hidden.iter().map(|(s, e)| e - s).sum();
        assert_eq!(covered + hidden_bytes, text.len());
    }

    #[test]
    fn only_the_requested_row_is_considered() {
        let text = "# one\n**two**\n";
        let spans = style_spans(text);
        let second = text.find("**two**").unwrap();
        let row_end = second + text[second..].find('\n').unwrap();
        let pieces = row_pieces(text, &spans, second, row_end);
        assert_eq!(
            pieces.iter().map(|&(s, e)| &text[s..e]).collect::<String>(),
            "two"
        );
    }
}
