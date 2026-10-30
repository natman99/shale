//! Typographic quotes: what each straight `"` and `'` in a note stands for.
//!
//! The file keeps straight quotes. They are what the keyboard types, what grep
//! and git and every other tool expects to see, and what the user said they want
//! to keep — so it is the *rendering* that turns them into “curly” ones, and only
//! in the views that render.
//!
//! That means the decision has to come from context, because a straight mark does
//! not say which end it is: `say "hi"` has an opener and a closer that are the
//! same byte, and `don't` has neither. The same answer serves two questions —
//! which glyph to draw, and what `diq` selects — and both need to know that an
//! apostrophe is not a quotation.
//!
//! Code is the exception that matters: a `"` inside a code span or a fenced block
//! is a string delimiter in somebody's program, so it gets no reading at all and
//! is drawn exactly as typed.

use super::{Style, StyleSpan};

/// What a straight quote mark means where it sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// The opening end of a quotation.
    Open,
    /// The closing end of one.
    Close,
    /// Inside a word (`don't`, `rockin'`): an apostrophe. It is drawn like a
    /// closing mark and is never the end of a quoted object.
    Apostrophe,
}

/// One straight quote mark, and what it stands for there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    /// The byte offset of the mark in the note.
    pub offset: usize,
    /// The mark itself: `"` or `'`.
    pub quote: char,
    /// What it means in this position.
    pub reading: Reading,
}

impl Mark {
    /// The glyph the mark is drawn as in a rendering view. An apostrophe and a
    /// closing mark are the same glyph, which is exactly why they are different
    /// readings: one ends a quotation, the other belongs to a word.
    pub fn glyph(&self) -> char {
        match (self.quote, self.reading) {
            ('"', Reading::Open) => '\u{201c}',
            ('"', _) => '\u{201d}',
            (_, Reading::Open) => '\u{2018}',
            (_, _) => '\u{2019}',
        }
    }
}

/// Every straight quote mark in `text`, in order, each with its reading.
///
/// `spans` is the style scan the view already has: anything inside
/// [`Style::Code`], [`Style::CodeBlock`] or [`Style::CodeFence`] is skipped, so a
/// `"` in code is left alone and does not flip the state of the quotes around it.
///
/// A double quote opens unless a letter or digit sits directly before it, which is
/// the rule that tells `say "hi"` its two ends apart. A single quote between two
/// word characters is an apostrophe, and otherwise follows the same open/close
/// rule. A line break counts as neither, so the first quote on a line is an opener:
/// the rule is local to the line rather than a state machine that one unbalanced
/// mark could carry through the rest of the note.
pub fn marks(text: &str, spans: &[StyleSpan]) -> Vec<Mark> {
    let code = code_ranges(spans);
    let mut out: Vec<Mark> = Vec::new();
    let mut chars = text.char_indices().peekable();
    // Whether the character just seen makes the quote next to it part of a word.
    // The end of a code span counts, which is what makes `"x"; and "real"` read
    // the second `"` as an opener rather than a closer.
    let mut prev_word = false;
    while let Some((offset, ch)) = chars.next() {
        if ch == '\n' {
            // A line break is not a word character, and it ends the line the
            // quotation would have been opened on. Without this, the first quote
            // on any line whose predecessor ended in a word — the line under a
            // list item, a callout, a fence — read as a closer, and the pair it
            // belonged to never existed.
            prev_word = false;
            continue;
        }
        if in_code(&code, offset) {
            prev_word = true;
            continue;
        }
        if ch != '"' && ch != '\'' {
            prev_word = ch.is_alphanumeric() || ch == '_';
            continue;
        }
        let next_word = chars
            .peek()
            .is_some_and(|&(_, c)| c.is_alphanumeric() || c == '_');
        let reading = if ch == '\'' && prev_word && next_word {
            Reading::Apostrophe
        } else if prev_word {
            Reading::Close
        } else {
            Reading::Open
        };
        out.push(Mark {
            offset,
            quote: ch,
            reading,
        });
        prev_word = true;
    }
    out
}

/// The quoted span under `offset`, or `None` when the cursor is not inside one.
///
/// The inner object is the text between the marks; the outer one is the marks with
/// it, which is what `daq` removes. When two quotations nest (a single-quoted
/// phrase inside a double-quoted one) the **innermost** one wins, because that is
/// the smaller thing the cursor was pointing at.
pub fn range(
    text: &str,
    offset: usize,
    inner: bool,
    spans: &[StyleSpan],
) -> Option<(usize, usize)> {
    let marks = marks(text, spans);
    let mut best: Option<(usize, usize)> = None;
    let mut open: Vec<Mark> = Vec::new();
    for mark in marks {
        if mark.reading == Reading::Apostrophe {
            continue;
        }
        if mark.reading == Reading::Open {
            open.push(mark);
            continue;
        }
        // A closer pairs with the nearest open mark of the same character.
        let Some(at) = open.iter().rposition(|m| m.quote == mark.quote) else {
            continue;
        };
        let opener = open.remove(at);
        if opener.offset > offset || mark.offset < offset {
            continue;
        }
        let pair = (opener.offset, mark.offset + 1);
        if best.is_none_or(|(lo, hi)| pair.1 - pair.0 < hi - lo) {
            best = Some(pair);
        }
    }
    best.map(|(lo, hi)| if inner { (lo + 1, hi - 1) } else { (lo, hi) })
}

/// The buffer ranges the style scan says are code.
fn code_ranges(spans: &[StyleSpan]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = spans
        .iter()
        .filter(|span| {
            matches!(
                span.style,
                Style::Code | Style::CodeBlock | Style::CodeFence
            )
        })
        .map(|span| (span.start, span.end))
        .collect();
    out.sort_unstable();
    out
}

/// Whether `offset` falls in one of the (sorted) code ranges.
fn in_code(ranges: &[(usize, usize)], offset: usize) -> bool {
    ranges
        .binary_search_by(|&(start, end)| {
            if end <= offset {
                std::cmp::Ordering::Less
            } else if start > offset {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdstyle::style_spans;

    /// The readings, as `(offset, quote, reading)` triples spelled out for the
    /// assertions below, so a failure says which mark went which way.
    fn readings(text: &str) -> Vec<(usize, char, Reading)> {
        marks(text, &style_spans(text))
            .into_iter()
            .map(|m| (m.offset, m.quote, m.reading))
            .collect()
    }

    /// The text a `q` object covers.
    fn cut(text: &str, offset: usize, inner: bool) -> String {
        let (lo, hi) = range(text, offset, inner, &style_spans(text)).unwrap();
        text[lo..hi].to_string()
    }

    #[test]
    fn a_quotation_opens_after_a_space_and_closes_after_a_word() {
        assert_eq!(
            readings("say \"hi\" now"),
            vec![(4, '"', Reading::Open), (7, '"', Reading::Close),]
        );
    }

    #[test]
    fn an_apostrophe_is_not_a_quotation() {
        // Both of these read as apostrophes: one between letters, one at the end
        // of a word the g has been dropped from.
        assert_eq!(
            readings("don't rockin'"),
            vec![(3, '\'', Reading::Apostrophe), (12, '\'', Reading::Close)]
        );
    }

    #[test]
    fn a_single_quote_opens_a_quoted_word() {
        assert_eq!(
            readings("the 'x' mark"),
            vec![(4, '\'', Reading::Open), (6, '\'', Reading::Close)]
        );
    }

    #[test]
    fn code_keeps_its_quotes_straight() {
        // The `"` inside the code span is not a quotation, and does not make the
        // one after it read as a closer.
        assert_eq!(
            readings("`let s = \"x\";` and \"real\""),
            vec![(19, '"', Reading::Open), (24, '"', Reading::Close)]
        );
    }

    #[test]
    fn a_fence_keeps_every_quote_inside_it() {
        let text = "```\nlet s = \"x\";\n```\nsaid \"hi\"\n";
        let found = readings(text);
        assert_eq!(
            found
                .iter()
                .map(|(offset, _, _)| *offset)
                .collect::<Vec<_>>(),
            vec![26, 29],
            "only the quotes outside the fence are marks"
        );
    }

    #[test]
    fn an_unbalanced_quote_does_not_flip_the_next_paragraph() {
        let text = "oops \"\n\nand \"here\"\n";
        assert_eq!(
            readings(text),
            vec![
                (5, '"', Reading::Open),
                (12, '"', Reading::Open),
                (17, '"', Reading::Close),
            ]
        );
    }

    #[test]
    fn a_quotation_at_the_start_of_a_line_opens() {
        // The line under a list item is ordinary text, and its first quote is an
        // opener: what sits before it is a line break, not a word.
        let text = "- one\nsay \"hello\" now\n";
        assert_eq!(cut(text, 11, true), "hello");
        let text = "- one\n\"hello\" now\n";
        assert_eq!(cut(text, 7, true), "hello");
        // And under a fence, and under a callout, the same.
        let text = "```\ncode\n```\nsaid \"hi\"\n";
        assert_eq!(cut(text, 19, true), "hi");
        let text = "> [!note] box\nsaid \"hi\"\n";
        assert_eq!(cut(text, 20, true), "hi");
    }

    #[test]
    fn the_object_takes_the_words_between_the_marks() {
        let text = "say \"hello there\" now\n";
        // From inside the quotation, and from either mark.
        for offset in 4..=16 {
            assert_eq!(cut(text, offset, true), "hello there", "offset {offset}");
            assert_eq!(
                cut(text, offset, false),
                "\"hello there\"",
                "offset {offset}"
            );
        }
        // Off the quotation there is no object, which cancels the operator.
        assert!(range(text, 0, true, &style_spans(text)).is_none());
    }

    #[test]
    fn the_innermost_quotation_wins() {
        let text = "say \"it's 'x' here\" now\n";
        // The cursor on the quoted `x` asks about the single quotes; the cursor in
        // the surrounding words asks about the double ones.
        assert_eq!(cut(text, 11, true), "x");
        assert_eq!(cut(text, 8, true), "it's 'x' here");
    }

    #[test]
    fn an_apostrophe_never_makes_an_object() {
        let text = "don't do this\n";
        assert!(range(text, 3, true, &style_spans(text)).is_none());
    }

    #[test]
    fn the_glyphs_are_the_curly_pair() {
        let found = marks("say \"hi\" don't", &style_spans("say \"hi\" don't"));
        let glyphs: String = found.iter().map(Mark::glyph).collect();
        assert_eq!(glyphs, "\u{201c}\u{201d}\u{2019}");
    }
}
