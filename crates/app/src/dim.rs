//! The frame's dimming system: what it draws at full strength, and how far the
//! rest is pulled toward the background.
//!
//! Focus mode (roadmap P0.8) is the first source, and the point of putting the
//! answer here rather than in the view is that there will be others: the `/`
//! search will light its matches, and a pinned note will light its own rows. A
//! source only has to say *what it lights*, in buffer bytes; this module turns
//! that into one alpha per buffer row, and the paint fades whatever it was going
//! to draw by that much.
//!
//! Fading is a blend toward the background rather than a per-glyph alpha because
//! every palette colour is a 0xRRGGBB integer and every paint site already has a
//! colour in hand — and because the same blend serves a run's glyphs and a
//! background quad alike.

use shale_core::mdstyle::{
    object::{range as object_range, Object},
    Style, StyleSpan,
};

/// How much of a colour survives, by how many blocks a row sits from the lit one.
/// The lit block keeps everything; the ramp falls off over the next two and then
/// holds at [`FLOOR`], so the rest of the note goes quiet without becoming a
/// spotlight the reader cannot look away from.
pub(crate) const RAMP: [f32; 3] = [1.0, 0.7, 0.45];

/// What is left of a colour further from the lit block than [`RAMP`] reaches.
pub(crate) const FLOOR: f32 = 0.3;

/// How much of the note stays at full strength around the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The markdown block the cursor is in: a paragraph, a list item, a quote, a
    /// callout, a fenced block, a table row, or a heading on its own.
    #[default]
    Block,
    /// The whole section the cursor is in, from its heading down to the next
    /// heading of the same or a higher level — the same span `i#` selects.
    Section,
}

impl Scope {
    /// The scope a word names — as the session file (roadmap P1.8) will spell it,
    /// or as a status line says it. Anything unrecognised is [`Scope::Block`],
    /// which is also what a window that never changed it has.
    pub(crate) fn from_word(word: &str) -> Self {
        match word.trim().to_ascii_lowercase().as_str() {
            "section" => Scope::Section,
            _ => Scope::Block,
        }
    }

    /// The word for it, which is what the session file and the status line say.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Scope::Block => "block",
            Scope::Section => "section",
        }
    }
}

/// The alpha each buffer row is drawn at: 1.0 is the colour the palette gave it,
/// less is that colour pulled toward the background by [`fade`].
///
/// `spans` is the style scan the frame already ran — a heading opens a section
/// rather than continuing the paragraph above it, which is the one thing about
/// markdown this needs to know. With [`Scope::Section`] the lit span is the
/// heading section the cursor is in; a cursor above the first heading has no
/// section, and falls back to its block.
pub(crate) fn row_alphas(
    text: &str,
    spans: &[StyleSpan],
    cursor: usize,
    scope: Scope,
    line_count: usize,
) -> Vec<f32> {
    let rows = row_ranges(text, line_count);
    let blocks = block_of_row(text, &rows, spans);
    let cursor_row = rows
        .iter()
        .position(|&(_, end)| cursor <= end)
        .unwrap_or(rows.len().saturating_sub(1));
    let cursor_block = blocks.get(cursor_row).copied().unwrap_or(0);

    // What the source lights, as buffer bytes. A section the cursor is not inside
    // is not an answer, so the block stands in.
    let lit = match scope {
        Scope::Section => object_range(text, cursor, Object::Section, false),
        Scope::Block => None,
    }
    .unwrap_or_else(|| block_span(&rows, &blocks, cursor_block));

    // The ramp counts blocks, so the lit span only has to be reduced to the
    // blocks it touches — and everything numbered between them is lit too, which
    // is exactly what lighting a section means.
    let mut lo: Option<usize> = None;
    let mut hi: Option<usize> = None;
    for (row, &block) in blocks.iter().enumerate() {
        if touches(rows[row], lit) {
            lo = Some(lo.unwrap_or(block).min(block));
            hi = Some(hi.unwrap_or(block).max(block));
        }
    }
    let (lo, hi) = (lo.unwrap_or(cursor_block), hi.unwrap_or(cursor_block));

    blocks
        .iter()
        .map(|&block| {
            let distance = if block >= lo && block <= hi {
                0
            } else {
                lo.abs_diff(block).min(hi.abs_diff(block))
            };
            alpha(distance)
        })
        .collect()
}

/// How much of a colour survives `distance` blocks away from what the frame
/// lights.
pub(crate) fn alpha(distance: usize) -> f32 {
    *RAMP.get(distance).unwrap_or(&FLOOR)
}

/// Pull `color` back toward `bg` until only `alpha` of it is left: 1.0 keeps it,
/// 0.0 replaces it with the background. Per channel, because the palette is
/// 0xRRGGBB.
pub(crate) fn fade(color: u32, bg: u32, alpha: f32) -> u32 {
    if alpha >= 1.0 {
        return color;
    }
    let a = alpha.clamp(0.0, 1.0);
    let channel = |v: u32, shift: u32| (v >> shift) & 0xff;
    let mix = |c: u32, b: u32| ((c as f32 * a + b as f32 * (1.0 - a)).round() as u32) & 0xff;
    (mix(channel(color, 16), channel(bg, 16)) << 16)
        | (mix(channel(color, 8), channel(bg, 8)) << 8)
        | mix(channel(color, 0), channel(bg, 0))
}

/// The `(start, end)` byte range of each buffer row, `line_count` of them.
fn row_ranges(text: &str, line_count: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(line_count);
    let mut at = 0usize;
    for line in text.split('\n') {
        if out.len() == line_count {
            break;
        }
        out.push((at, at + line.len()));
        at += line.len() + 1;
    }
    // A buffer that counts more rows than the text has (an empty last row is the
    // usual case) gets zero-width rows at the end rather than a panic.
    out.resize(line_count, (text.len(), text.len()));
    out
}

/// Whether `range` is inside the lit span. A blank row paints nothing, so it is
/// lit by where it sits rather than by what it holds — which is what keeps a
/// section's blank lines from coming back dimmed between two lit paragraphs.
fn touches(range: (usize, usize), lit: (usize, usize)) -> bool {
    if range.0 == range.1 {
        range.0 >= lit.0 && range.0 <= lit.1
    } else {
        range.0 < lit.1 && range.1 > lit.0
    }
}

/// The block index of every row: a maximal run of consecutive non-blank rows is
/// one block, except that a heading row is a block of its own (it opens a section
/// rather than continuing the paragraph above it) and a blank row is a block of
/// its own, which is what makes the ramp count paragraphs rather than lines.
fn block_of_row(text: &str, rows: &[(usize, usize)], spans: &[StyleSpan]) -> Vec<usize> {
    let headings = heading_rows(rows, spans);
    let blank = |row: usize| {
        text[rows[row].0..rows[row].1]
            .chars()
            .all(char::is_whitespace)
    };
    let mut out = vec![0usize; rows.len()];
    let mut block = 0usize;
    for row in 1..rows.len() {
        if headings[row] || headings[row - 1] || blank(row) || blank(row - 1) {
            block += 1;
        }
        out[row] = block;
    }
    out
}

/// Which rows hold a heading, marker or text. Spans and rows are both in offset
/// order, so one forward walk is enough.
fn heading_rows(rows: &[(usize, usize)], spans: &[StyleSpan]) -> Vec<bool> {
    let mut out = vec![false; rows.len()];
    let mut row = 0usize;
    for span in spans {
        if !matches!(span.style, Style::Heading { .. } | Style::HeadingMarker) {
            continue;
        }
        while row < rows.len() && rows[row].1 <= span.start {
            row += 1;
        }
        let mut at = row;
        while at < rows.len() && rows[at].0 < span.end {
            out[at] = true;
            at += 1;
        }
    }
    out
}

/// The byte range covering every row in block `want`.
fn block_span(rows: &[(usize, usize)], blocks: &[usize], want: usize) -> (usize, usize) {
    let mut span = (usize::MAX, 0usize);
    for (row, &block) in blocks.iter().enumerate() {
        if block == want {
            span.0 = span.0.min(rows[row].0);
            span.1 = span.1.max(rows[row].1);
        }
    }
    if span.0 == usize::MAX {
        (0, 0)
    } else {
        span
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alphas of every row of `text` with the caret at `cursor`, running the
    /// same style scan the frame hands in.
    fn alphas(text: &str, cursor: usize, scope: Scope) -> Vec<f32> {
        let spans = shale_core::mdstyle::style_spans(text);
        row_alphas(text, &spans, cursor, scope, text.split('\n').count())
    }

    #[test]
    fn a_colour_fades_toward_the_background_channel_by_channel() {
        assert_eq!(fade(0x000000, 0xffffff, 1.0), 0x000000);
        assert_eq!(fade(0x000000, 0xffffff, 0.0), 0xffffff);
        assert_eq!(fade(0x000000, 0xffffff, 0.5), 0x808080);
        // Halfway between two hues, and a colour already at the background stays.
        assert_eq!(fade(0xff0000, 0x0000ff, 0.5), 0x800080);
        assert_eq!(fade(0x101010, 0x101010, 0.2), 0x101010);
    }

    #[test]
    fn the_ramp_holds_at_its_floor_rather_than_reaching_zero() {
        assert_eq!(alpha(0), 1.0);
        assert!(alpha(1) < alpha(0) && alpha(2) < alpha(1));
        assert_eq!(alpha(3), FLOOR);
        assert_eq!(alpha(900), FLOOR);
        assert!(alpha(900) > 0.0, "the rest of the note stays readable");
    }

    #[test]
    fn a_paragraph_is_one_block_and_a_blank_line_starts_the_next() {
        let a = alphas("one\ntwo\n\nthree\n", 0, Scope::Block);
        // Rows: `one`, `two`, the blank line, `three`, and the empty row the
        // trailing newline leaves.
        assert_eq!(a.len(), 5);
        assert_eq!(a[0], 1.0);
        assert_eq!(a[1], 1.0, "the rest of the paragraph is lit too");
        assert!(a[3] < 1.0, "the next paragraph is dimmed");
        assert!(
            a[3] > FLOOR,
            "one paragraph away is on the ramp, not at the floor"
        );
    }

    #[test]
    fn a_heading_is_a_block_of_its_own() {
        let a = alphas("para\n# Title\nunder\n", 0, Scope::Block);
        assert_eq!(a[0], 1.0);
        assert!(a[1] < 1.0, "the heading is one block away");
        assert!(a[2] < 1.0, "and the line under it one further");
        assert!(a[2] <= a[1], "and never brighter than the row above it");
    }

    #[test]
    fn a_fence_is_one_block() {
        let text = "before\n\n```\nlet x = 1;\nlet y = 2;\n```\n\nafter\n";
        let a = alphas(text, 0, Scope::Block);
        // The caret is in `before`. The fence's four rows are one block, so they
        // all sit at the same distance rather than ramping away line by line.
        assert_eq!(a[3], a[4], "the two code lines agree");
        assert_eq!(a[4], a[5], "and so does the closing fence");
    }

    #[test]
    fn section_scope_lights_the_whole_of_the_current_section() {
        let text = "# Top\n\nfirst\n\nmore\n\n# Other\n\ntail\n";
        let block = alphas(text, 2, Scope::Block);
        let section = alphas(text, 2, Scope::Section);
        assert!(block[4] < 1.0, "block scope lights only the heading");
        assert_eq!(section[2], 1.0, "the section's first paragraph is lit");
        assert_eq!(section[4], 1.0, "and its second");
        assert!(section[6] < 1.0, "the next section is not");
    }

    #[test]
    fn a_cursor_above_the_first_heading_falls_back_to_its_block() {
        let a = alphas("intro line\n\n# Top\n\nbody\n", 0, Scope::Section);
        assert_eq!(a[0], 1.0);
        assert!(a[3] < 1.0);
    }

    #[test]
    fn the_scope_words_round_trip() {
        assert_eq!(Scope::from_word("section"), Scope::Section);
        assert_eq!(Scope::from_word("Section"), Scope::Section);
        assert_eq!(Scope::from_word("block"), Scope::Block);
        assert_eq!(Scope::from_word(""), Scope::Block);
        // Not a scope yet: a word that asks for it gets the default rather than
        // an error, and the roadmap records what is still owed.
        assert_eq!(Scope::from_word("sentence"), Scope::Block);
        assert_eq!(Scope::Section.word(), "section");
        assert_eq!(Scope::Block.word(), "block");
    }

    #[test]
    fn an_empty_note_lights_itself_rather_than_panicking() {
        assert_eq!(alphas("", 0, Scope::Block), vec![1.0]);
        assert_eq!(alphas("\n\n", 1, Scope::Section).len(), 3);
        // A caret past the end of the note is a real question: the buffer answers
        // with its last offset, and the last row is what gets lit.
        let a = alphas("one\ntwo", 99, Scope::Block);
        assert_eq!(a[1], 1.0);
    }
}
