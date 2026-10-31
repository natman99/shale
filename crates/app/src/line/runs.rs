//! Turning one line's text and its overlapping annotations into the styled
//! [`gpui::TextRun`]s gpui paints: the markdown spans, wiki-link colours,
//! checker's underlines and search-match backgrounds, applied in order and then
//! coalesced into as few runs as will still tile the text.
//!
//! Run lengths are utf-8 byte counts and must add up to exactly the painted
//! text's length, so the re-basing here is done through the line's [`PieceMap`]
//! rather than by clipping buffer ranges directly.

use gpui::{Font, ShapedLine, TextRun};
use shale_core::mdstyle::Style;
use shale_core::theme::Palette;

use crate::preview::PieceMap;
use crate::theme::RunStyle;

/// One table cell as it is painted: its shaped text, and the buffer ranges that
/// text came from, which the cursor quad, the selection and hit-testing all need.
#[derive(Clone)]
pub(crate) struct CellShaped {
    /// The cell's content, with any hidden markers left out.
    pub(crate) line: ShapedLine,
    /// The pieces of `line`.
    pub(crate) map: PieceMap,
}

/// Build the `TextRun`s that paint `text`.
///
/// Every byte of the line gets a style — the markdown spans applied in order,
/// so a span that starts later in the source wins, then the resolved/unresolved
/// colour of any wiki-link over the top, then the checker's wavy underline over
/// both, then the search matches' background over everything — and neighbouring
/// bytes that share a style are coalesced into one run.
/// Run lengths are utf-8 byte counts and must add up to exactly `text.len()`:
/// gpui walks runs against glyphs, so a map that does not tile the text styles
/// the wrong characters.
pub(super) fn build_runs(
    text: &str,
    styles: &LineStyles,
    base_font: &Font,
    palette: &Palette,
    dim: f32,
) -> Vec<TextRun> {
    let total = text.len();
    let plain = RunStyle::plain(palette);
    let mut painted = vec![plain; total];
    for (start, end, style) in &styles.spans {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        for slot in &mut painted[start..end] {
            *slot = RunStyle::for_style(*style, palette);
        }
    }
    for (start, end, resolved) in &styles.links {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        let color = if *resolved {
            palette.link
        } else {
            palette.link_unresolved
        };
        for slot in &mut painted[start..end] {
            slot.color = color;
        }
    }
    // The checker's notes are drawn last, over whatever styling the text already
    // had: a misspelled word inside a heading still needs its squiggle. The
    // colour is the palette's own `spell` role, which a theme can set apart from
    // its error red -- a checker's note is a remark about the text, not an error
    // in it.
    for (start, end) in &styles.marks {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        for slot in &mut painted[start..end] {
            slot.spell = Some(palette.spell);
        }
    }
    // A search match only paints a background, so the text keeps whatever colour
    // markdown gave it. The roles are the ones the palette already has:
    // `highlight_bg` for `==highlight==`, and the selection colour for the match
    // under the cursor, which is what makes "you are on the third of 17" visible
    // without a second panel.
    for (start, end) in &styles.hits {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        for slot in &mut painted[start..end] {
            slot.background = Some(palette.highlight_bg);
        }
    }
    for (start, end) in &styles.active {
        let start = (*start).min(total);
        let end = (*end).clamp(start, total);
        for slot in &mut painted[start..end] {
            slot.background = Some(palette.selection);
        }
    }

    let mut runs: Vec<TextRun> = Vec::new();
    let mut i = 0usize;
    while i < total {
        let style = painted[i];
        let mut j = i + 1;
        while j < total && painted[j] == style {
            j += 1;
        }
        // Faded after coalescing: two runs that shared a style still share it once
        // dimmed, and dimming first would compare the wrong values above.
        runs.push(style.faded(dim, palette.bg).text_run(j - i, base_font));
        i = j;
    }
    if runs.is_empty() {
        // An empty display line still gets one (empty) run.
        runs.push(plain.faded(dim, palette.bg).text_run(0, base_font));
    }
    runs
}

/// Everything that styles one line's runs: the markdown spans, wiki-links,
/// checker's ranges and search matches, all in the same coordinate space — the
/// text being painted, once re-based off the buffer.
#[derive(Default)]
pub(super) struct LineStyles {
    /// Markdown spans, in the order the scanner emitted them.
    pub(super) spans: Vec<(usize, usize, Style)>,
    /// Wiki-links with whether their target names a note that exists.
    pub(super) links: Vec<(usize, usize, bool)>,
    /// The spell and grammar checker's notes.
    pub(super) marks: Vec<(usize, usize)>,
    /// Every `/` match.
    pub(super) hits: Vec<(usize, usize)>,
    /// The `/` match the cursor is on. A match that straddles a hidden marker is
    /// painted in pieces, so this is a list rather than one range.
    pub(super) active: Vec<(usize, usize)>,
}

/// The markdown spans, wiki-links, checker's ranges and search matches that
/// overlap one table cell, clipped to it and re-based onto the cell's painted
/// text. The ranges in are buffer offsets; `map` describes the text they are
/// being mapped onto.
pub(super) fn cell_styles(
    spans: &[(usize, usize, Style)],
    links: &[(usize, usize, bool)],
    marks: &[(usize, usize)],
    hits: &[(usize, usize)],
    active: Option<(usize, usize)>,
    map: &PieceMap,
) -> LineStyles {
    let current = active.map(|range| vec![range]).unwrap_or_default();
    LineStyles {
        spans: map.rebase_spans(spans),
        links: map.rebase_links(links),
        marks: map.rebase_marks(marks),
        hits: map.rebase_marks(hits),
        active: map.rebase_marks(&current),
    }
}
