//! The run builder's own tests: that the styled runs tile the painted text
//! exactly, that the annotations win in the right order, and that a table cell's
//! ranges re-base onto the cell's painted text.

use super::*;
use crate::line::runs::{build_runs, cell_styles, LineStyles};
use gpui::{font, TextRun};

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
        let styles = LineStyles {
            spans,
            links,
            marks,
            ..LineStyles::default()
        };
        let runs = build_runs(text, &styles, &font, &Palette::DARK, 1.0);
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

/// The checker's ranges come out as wavy underlines in the palette's spell
/// colour, over whatever styling the same bytes already had.
#[test]
fn a_checker_range_is_a_wavy_underline() {
    let font = font("monospace");
    let text = "Thier spelling";
    let styles = LineStyles {
        marks: vec![(0, 5)],
        ..LineStyles::default()
    };
    let runs = build_runs(text, &styles, &font, &Palette::DARK, 1.0);
    let total: usize = runs.iter().map(|r| r.len).sum();
    assert_eq!(total, text.len());
    assert_eq!(runs[0].len, 5, "the marked word is its own run");
    let underline = runs[0].underline.expect("underlined");
    assert!(underline.wavy, "a checker's note is a squiggle");
    assert_eq!(underline.color, Some(rgb(Palette::DARK.spell).into()));
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
    let map = PieceMap::with_substitutions(&[(1, 7)], &[]);
    let styles = cell_styles(&spans, &links, &marks, &[], None, &map);
    let (cell_spans, cell_links, cell_marks) = (styles.spans, styles.links, styles.marks);
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
    let map = PieceMap::with_substitutions(&[(12, 13), (15, 17)], &[]);
    let styles = cell_styles(&[(10, 15, Style::Strong)], &[], &[], &[], None, &map);
    assert_eq!(
        styles.spans,
        vec![(0, 1, Style::Strong)],
        "the span follows the painted `a`"
    );
}

/// The background painted under one byte of the line, found by walking the
/// runs, whose lengths are byte counts.
fn background_at(runs: &[TextRun], byte: usize) -> Option<gpui::Hsla> {
    let mut at = 0usize;
    for run in runs {
        if byte < at + run.len {
            return run.background_color;
        }
        at += run.len;
    }
    None
}

/// A `/` match is a background, drawn over whatever the markdown gave the same
/// bytes; the current match is the selection colour on top of the others, so
/// the match the cursor is on is visible without a second panel.
#[test]
fn a_search_match_is_a_background() {
    let font = font("monospace");
    // `` `word` and word ``: the first match is inside inline code, which has a
    // background of its own, so this also proves the match wins over it.
    let text = "`word` and word";
    let styles = LineStyles {
        spans: vec![(0, 6, Style::Code)],
        hits: vec![(1, 5), (11, 15)],
        active: vec![(11, 15)],
        ..LineStyles::default()
    };
    let runs = build_runs(text, &styles, &font, &Palette::DARK, 1.0);
    let total: usize = runs.iter().map(|r| r.len).sum();
    assert_eq!(total, text.len(), "runs must cover the line exactly");
    assert_eq!(
        background_at(&runs, 0),
        Some(rgb(Palette::DARK.code_bg).into()),
        "the fence marker the match does not cover keeps its own background"
    );
    assert_eq!(
        background_at(&runs, 1),
        Some(rgb(Palette::DARK.highlight_bg).into()),
        "a match wins over the code background"
    );
    assert_eq!(
        background_at(&runs, 11),
        Some(rgb(Palette::DARK.selection).into()),
        "the current match is drawn over the others"
    );
    assert_eq!(
        background_at(&runs, 7),
        None,
        "text between matches is left alone"
    );
}
