//! The markdown text objects: the ranges `i` and `a` act on when the scope names
//! a markdown construct rather than a word or a paragraph.
//!
//! The vim engine asks for a byte range and puts its operator on it, so this
//! module answers one question: what is the object under this offset, and what is
//! inside it. `inner` is the object without its own markup — the callout's text,
//! the code in the block, the name in the link — and the outer range is the whole
//! construct, markup included, which is what `da#` removes.
//!
//! Everything here reads the [`crate::mdstyle::style_spans`] scan instead of
//! parsing again: that scanner already knows where a fence ends, which blockquote
//! opened a callout, and that a `#` inside a fenced block is not a heading.
//! Duplicating that state is how the two would drift apart. Links are the one
//! object that needs a construct's *extent* rather than its styled ranges, so
//! they reuse the inline scanner's own link functions.
//!
//! A table cell (`it` / `at`) is deliberately **not** here: the engine has the
//! tables parsed already for its cell motions, and [`crate::table`] answers the
//! range from that parse.

use super::inline::{find_subslice, md_link, next_char_len, wiki_link};
use super::{block_at, block_map, quotes, style_spans, Style, StyleSpan};

/// Which markdown construct the character after `i` or `a` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Object {
    /// A `[[wiki-link]]`, a `[text](url)`, or an `![[embed]]`: `iL` / `aL`.
    Link,
    /// The section a heading opens, running to the heading that starts the next
    /// section of the same or a higher level: `i#` / `a#`.
    Section,
    /// An Obsidian callout box, header line and `>` markers included:
    /// `i*` / `a*`.
    Callout,
    /// A fenced code block, or — when the cursor is not inside one — an inline
    /// code span on its line: `` i` `` / `` a` ``.
    Code,
    /// A quoted span: the text between a matching pair of `"` or `'`, and the pair
    /// with it. `iq` / `aq`. An apostrophe inside a word is never one of these,
    /// and neither is a quote mark inside code.
    Quote,
    /// A bracket pair: vim's `i(`/`a(`, `i[`/`a[`, `i{`/`a{`, `i<`/`a<`. The
    /// opening and the closing character name the same object (`i(` and `i)` are
    /// one pair), and `b` / `B` are vim's letters for parens and braces.
    Pair(Pair),
    /// One markdown block — a paragraph, one list item with its children, one whole
    /// quote, callout, table or code fence, one heading line: `i^` / `a^`. The caret is
    /// the character a block id is written with, so the object and the anchor a block
    /// answers to are named by the same mark and are the same atom (P4.9 ruling 10).
    /// `ib` / `ab` were the first choice and are **taken**: `b` and `B` are vim's
    /// historical letters for parens and braces, and an object that shadows a motion the
    /// user already knows is a worse trade than an unfamiliar scope character.
    Block,
}

/// Which bracket pair. Kept separate from [`Object`] so the pair's two
/// characters stay together in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pair {
    Paren,
    Bracket,
    Brace,
    Angle,
}

impl Pair {
    /// The characters that open and close it.
    fn chars(self) -> (char, char) {
        match self {
            Self::Paren => ('(', ')'),
            Self::Bracket => ('[', ']'),
            Self::Brace => ('{', '}'),
            Self::Angle => ('<', '>'),
        }
    }
}

impl Object {
    /// The scope character that names the object, as typed after `i` or `a`.
    /// `None` for anything else, including `w`, `p` and `t`, which the vim layer
    /// answers from the buffer's words, paragraphs and tables.
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'L' => Some(Self::Link),
            '#' => Some(Self::Section),
            '*' => Some(Self::Callout),
            '`' => Some(Self::Code),
            'q' => Some(Self::Quote),
            // vim's own letters: the pair answers to either of its characters, and
            // `b` / `B` are the historical names for parens and braces.
            '(' | ')' | 'b' => Some(Self::Pair(Pair::Paren)),
            '[' | ']' => Some(Self::Pair(Pair::Bracket)),
            '{' | '}' | 'B' => Some(Self::Pair(Pair::Brace)),
            '<' | '>' => Some(Self::Pair(Pair::Angle)),
            // The quote object answers to the quote marks themselves as well as to
            // `q`, because that is what a vim user's fingers type.
            '"' | '\'' => Some(Self::Quote),
            // The block object is named by the mark a block id is written with. `b` and
            // `B` — the letters ruling 10 first proposed — stay vim's parens and braces.
            '^' => Some(Self::Block),
            _ => None,
        }
    }
}

/// The byte range `[lo, hi)` of the object under `offset`, or `None` when the
/// cursor is not inside one — which is what cancels the operator, exactly as a
/// find that matches nothing does.
pub fn range(text: &str, offset: usize, object: Object, inner: bool) -> Option<(usize, usize)> {
    match object {
        Object::Link => link_range(text, offset, inner),
        Object::Section => section_range(text, offset, inner),
        Object::Callout => callout_range(text, offset, inner),
        Object::Code => code_range(text, offset, inner),
        Object::Quote => quotes::range(text, offset, inner, &style_spans(text)),
        Object::Pair(pair) => pair_range(text, offset, pair, inner),
        Object::Block => block_range(text, offset, inner),
    }
}

/// The bracket pair under `offset`, innermost first.
///
/// Brackets inside a code span or a fenced block are text, not brackets — the
/// same rule the quote object follows, and for the same reason: `di(` inside a
/// code block should take the code's parentheses, and everywhere else it should
/// not see the ones the block happens to contain. An unclosed pair matches
/// nothing, as in vim. Backslashes are *not* escapes here: vim's pair matching
/// does not honour them either, and `di(` on `(a\)b)` closes on the backslashed
/// paren in both.
///
/// Pairs may cross lines (a list of coordinates written over three lines is one
/// pair), which is why the stack outlives the line loop.
fn pair_range(text: &str, offset: usize, pair: Pair, inner: bool) -> Option<(usize, usize)> {
    let (open, close) = pair.chars();
    let mut stack: Vec<usize> = Vec::new();
    let mut fence: Option<&str> = None;

    let mut start = 0;
    while start <= text.len() {
        let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        let line = &text[start..end];
        let body = line.trim_start();
        if let Some(marker) = fence {
            // Inside a block: opaque until the fence that closes it.
            if body.starts_with(marker) {
                fence = None;
            }
            start = end + 1;
            continue;
        }
        if body.starts_with("```") || body.starts_with("~~~") {
            fence = Some(if body.starts_with("```") {
                "```"
            } else {
                "~~~"
            });
            start = end + 1;
            continue;
        }
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = line[i..].chars().next().unwrap();
            if c == '`' {
                if let Some(stop) = code_span_end(bytes, i) {
                    i = stop;
                    continue;
                }
            }
            if c == open {
                stack.push(start + i);
            } else if c == close {
                if let Some(opener) = stack.pop() {
                    if opener <= offset && offset <= start + i {
                        return Some(if inner {
                            (opener + open.len_utf8(), start + i)
                        } else {
                            (opener, start + i + close.len_utf8())
                        });
                    }
                }
            }
            i += c.len_utf8();
        }
        start = end + 1;
    }
    None
}

/// The line containing `offset`, as its start and the end of its content with
/// the line break excluded. An offset sitting on a line break belongs to the line
/// it ends, which is where a cursor at the end of a line is.
fn line_at(text: &str, offset: usize) -> Option<(usize, usize)> {
    if offset > text.len() {
        return None;
    }
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    Some((start, end))
}

/// The offset just past the line break that ends the line containing `at`, or the
/// end of the text when that line is the last one. An object that covers whole
/// lines takes its end here, so `da#` removes the section's last line rather than
/// leaving an empty one behind.
fn after_line(text: &str, at: usize) -> usize {
    let (_, end) = line_at(text, at).unwrap_or((at, at));
    if text.as_bytes().get(end) == Some(&b'\n') {
        end + 1
    } else {
        end
    }
}

/// The link construct under `offset`. The walk is `inline_spans`' own, over the
/// cursor's line: the same three shapes, the same functions, and the same skip
/// over an inline code span so a `[[name]]` written inside backticks stays text.
fn link_range(text: &str, offset: usize, inner: bool) -> Option<(usize, usize)> {
    let (start, end) = line_at(text, offset)?;
    let line = &text[start..end];
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if offset < start + i {
            // The cursor is in the plain text before the next construct.
            return None;
        }
        if let Some(stop) = code_span_end(bytes, i) {
            i = stop;
            continue;
        }
        let mut out: Vec<StyleSpan> = Vec::new();
        let stop = match bytes[i] {
            b'!' if bytes.get(i + 1) == Some(&b'[') => {
                if bytes.get(i + 2) == Some(&b'[') {
                    wiki_link(start, line, i, &mut out)
                } else {
                    md_link(start, line, i, &mut out)
                }
            }
            b'[' if bytes.get(i + 1) == Some(&b'[') => wiki_link(start, line, i, &mut out),
            b'[' => md_link(start, line, i, &mut out),
            _ => None,
        };
        let Some(stop) = stop else {
            i += next_char_len(bytes, i);
            continue;
        };
        if offset < stop {
            let lo = start + i;
            return Some(if inner {
                inner_link(text, lo, stop)
            } else {
                (lo, stop)
            });
        }
        i = stop - start;
    }
    None
}

/// The end of a backtick run that closes at the same length, if `i` opens one.
/// Used to step over a code span while looking for a link.
fn code_span_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] != b'`' {
        return None;
    }
    let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
    let open = i + run;
    find_subslice(&bytes[open..], &vec![b'`'; run]).map(|found| open + found + run)
}

/// What is between a link's brackets. An aliased wiki-link keeps its `target|`,
/// so `diL` on `[[a|b]]` leaves `[[]]` rather than a broken half.
fn inner_link(text: &str, lo: usize, hi: usize) -> (usize, usize) {
    let src = &text[lo..hi];
    if src.starts_with("![[") {
        (lo + 3, hi - 2)
    } else if src.starts_with("[[") {
        (lo + 2, hi - 2)
    } else {
        // `[text](url)` or `![alt](url)` — the text a reader sees, not the url.
        let open = lo + usize::from(src.starts_with('!')) + 1;
        let close = open + src[open - lo..].find(']').unwrap_or(0);
        (open, close)
    }
}

/// One heading the parser styles: where its line starts, the byte range of its
/// text, and how deep. The text is what the author wrote — `*emphasis*` and the
/// closing hashes and all — because concealment is a view decision and anything
/// that reads a heading reads a claim (headings-plan.md ruling 47).
#[derive(Debug, PartialEq, Eq)]
pub struct HeadingSpan {
    /// Byte offset of the line the heading sits on.
    pub line_start: usize,
    /// Byte range of the heading's text. Empty for a bare `#`.
    pub text_start: usize,
    pub text_end: usize,
    /// The marker count, 1 to 6.
    pub depth: u8,
}

/// The ATX headings, in order, from the parser's own spans. **The one answer to
/// "what sections does this note have"**: the vault index reads this scan, and the
/// text object reads the same spans, so a query groups by the sections the editor
/// styles rather than by a second opinion of what a section is
/// (headings-plan.md ruling 46).
///
/// Fence-aware for free: the scanner never styles a `#` inside a fenced block as a
/// heading, so a comment in code cannot end a section. A heading inside a blockquote
/// is not styled either — quoting somebody's heading is quoting, not sectioning.
pub fn heading_spans(text: &str) -> Vec<HeadingSpan> {
    /// Which half of a heading line a span is: the hashes, or what they name.
    enum Part {
        Marker,
        Text(usize, usize),
    }

    let mut out: Vec<HeadingSpan> = Vec::new();
    for span in style_spans(text) {
        let (depth, part) = match span.style {
            Style::Heading { level } => (level, Part::Text(span.start, span.end)),
            // The marker run is the depth, and a heading with nothing after the
            // markers has only the markers styled.
            Style::HeadingMarker => (
                text[span.start..span.end].chars().count() as u8,
                Part::Marker,
            ),
            _ => continue,
        };
        let (line_start, line_end) = line_at(text, span.start).unwrap_or((span.start, span.end));
        // One heading is two spans on one line, adjacent in span order: the hashes
        // say how deep, the text says what it is called.
        match out.last_mut() {
            Some(head) if head.line_start == line_start => match part {
                Part::Text(from, to) => {
                    head.text_start = from;
                    head.text_end = to;
                    // The styled text knows its own level, and it outranks a run of
                    // hashes: a closing `##` is not a second heading.
                    head.depth = depth;
                }
                Part::Marker if head.text_start == head.text_end => head.depth = depth,
                Part::Marker => {}
            },
            // A heading whose text is empty until a text span says otherwise: a
            // bare `#` is a section with no name (EDGE-3).
            _ => out.push(HeadingSpan {
                line_start,
                text_start: line_end,
                text_end: line_end,
                depth,
            }),
        }
    }
    out
}

/// A heading in the coordinates the rest of the program wants: the **0-based** line
/// it sits on, its depth, and the byte range of its text.
#[derive(Debug, PartialEq, Eq)]
pub struct HeadingLine {
    pub line: usize,
    pub text_start: usize,
    pub text_end: usize,
    pub depth: u8,
}

/// [`heading_spans`] counted into lines. The line table is built once, here, so no
/// consumer keeps its own copy of "which byte starts which line" — three of them
/// did, each with its own idea of what a heading is, and that is how one note ended
/// up with several different sets of sections (headings-plan.md ruling 46).
pub fn heading_lines(text: &str) -> Vec<HeadingLine> {
    let spans = heading_spans(text);
    if spans.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(spans.len());
    // The spans arrive in byte order, so one forward walk over the newlines counts
    // every line number without a table of them.
    let mut line = 0usize;
    let mut at = 0usize;
    for span in spans {
        while let Some(break_at) = text[at..span.line_start].find('\n') {
            at += break_at + 1;
            line += 1;
        }
        out.push(HeadingLine {
            line,
            text_start: span.text_start,
            text_end: span.text_end,
            depth: span.depth,
        });
    }
    out
}

/// The heading lines, as `(line start, level)` in order.
fn headings(text: &str) -> Vec<(usize, u8)> {
    heading_spans(text)
        .into_iter()
        .map(|h| (h.line_start, h.depth))
        .collect()
}

/// The section the cursor is in: the nearest heading at or above its line, and
/// everything that heading opens.
fn section_range(text: &str, offset: usize, inner: bool) -> Option<(usize, usize)> {
    let heads = headings(text);
    let (line_start, _) = line_at(text, offset)?;
    let (head, level) = *heads.iter().rev().find(|(start, _)| *start <= line_start)?;
    // The section ends where the next heading of the same or a higher level
    // begins, or at the end of the note. A deeper heading belongs to this section.
    let end = heads
        .iter()
        .find(|(start, lower)| *start > head && *lower <= level)
        .map_or(text.len(), |(start, _)| *start);
    if inner {
        // The body: the heading's own line left out, which is what `ci#` retypes
        // without losing the heading itself.
        Some((after_line(text, head).min(end), end))
    } else {
        Some((head, end))
    }
}

/// The block under the cursor, innermost first: a caret inside a nested list item names
/// that item and not its parent. This is the same scan `:block-id` writes into and
/// `#^id` resolves through — the object, the anchor and the embed are one atom, which is
/// the whole point of ruling 10. `inner` is the block as written — its indent and its
/// `-` or `>` or `|` markers included — because the block is the atom and half a bullet is
/// not: `di^` on a list item takes the bullet with it, which is what deleting an item
/// means. The outer range adds the block's line break, so `da^` takes the block and the
/// gap it left, the way `ap` takes a paragraph and its blank line.
fn block_range(text: &str, offset: usize, inner: bool) -> Option<(usize, usize)> {
    let map = block_map(text);
    let index = block_at(&map, offset)?;
    let block = map.blocks.get(index)?;
    if inner {
        Some((block.start, block.end))
    } else {
        Some((block.start, after_line(text, block.end)))
    }
}

/// The callout box under the cursor. The scanner emits one [`Style::Callout`]
/// span per line of the box, each covering the whole line, so a box is a run of
/// spans separated by nothing but line breaks.
fn callout_range(text: &str, offset: usize, inner: bool) -> Option<(usize, usize)> {
    let spans = style_spans(text);
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for span in spans
        .iter()
        .filter(|span| matches!(span.style, Style::Callout { .. }))
    {
        match groups.last_mut() {
            Some(group) if span.start <= group.1 + 1 => group.1 = span.end,
            _ => groups.push((span.start, span.end)),
        }
    }
    let (start, end) = *groups.iter().find(|(s, e)| *s <= offset && offset <= *e)?;
    if !inner {
        return Some((start, after_line(text, end)));
    }
    let body_end = after_line(text, end);
    let body_start = after_line(text, start).min(body_end);
    if body_start < body_end {
        // The body: everything after the header line, the `>` markers and the
        // body's own line breaks included, so the box is left whole and `ci*` is a
        // way to retype what is in it.
        return Some((body_start, body_end));
    }
    // A callout with no body lines — the usual one-liner — has no body to take,
    // and its title text *is* what it says. The scanner runs a
    // `Style::CalloutTitle` span from the text after the `[!kind]` marker to the
    // end of the header line, and the type name inside the brackets is a separate
    // span that stops short of that end: which is what keeps `di*` from eating the
    // marker and turning the box into an ordinary quote.
    let head_end = line_at(text, start).map_or(start, |(_, e)| e);
    let title = spans.iter().find(|span| {
        matches!(span.style, Style::CalloutTitle { .. })
            && span.end == head_end
            && span.start >= start
    });
    // A box with no title text either (`> [!note]` on its own) has an empty inner
    // object sitting where the text would go, so `ci*` puts the caret after the
    // marker and lets the user write the box.
    Some(title.map_or((head_end, head_end), |span| (span.start, span.end)))
}

/// The fenced block under the cursor, or the inline code span on its line when
/// the cursor is not inside a block.
fn code_range(text: &str, offset: usize, inner: bool) -> Option<(usize, usize)> {
    // Fence lines open and close a block in order; the body is the code between
    // the opening fence's line and the closing fence's line.
    let mut blocks: Vec<(usize, usize, usize)> = Vec::new();
    let mut open: Option<usize> = None;
    for span in style_spans(text)
        .iter()
        .filter(|span| matches!(span.style, Style::CodeFence))
    {
        match open.take() {
            None => open = Some(span.start),
            Some(start) => blocks.push((start, span.start, span.end)),
        }
    }
    // An unclosed fence runs to the end of the note: the user is mid-edit, and
    // `` d i ` `` should still find the code they were writing.
    if let Some(start) = open {
        blocks.push((start, text.len(), text.len()));
    }
    match blocks.iter().find(|(s, _, e)| *s <= offset && offset <= *e) {
        Some(&(start, close_start, close_end)) => Some(if inner {
            (after_line(text, start).min(close_start), close_start)
        } else {
            (start, after_line(text, close_end))
        }),
        // Not inside a block: an inline code span on the cursor's line.
        None => inline_code_range(text, offset, inner),
    }
}

/// The inline `` `code` `` span under `offset`, mirroring how `inline_spans`
/// walks a line: a run of N backticks opens and the next run of N closes it.
fn inline_code_range(text: &str, offset: usize, inner: bool) -> Option<(usize, usize)> {
    let (start, end) = line_at(text, offset)?;
    let line = &text[start..end];
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if offset < start + i {
            return None;
        }
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            let open = i + run;
            if let Some(found) = find_subslice(&bytes[open..], &vec![b'`'; run]) {
                let close = open + found;
                let stop = close + run;
                if offset < start + stop {
                    return Some(if inner {
                        (start + open, start + close)
                    } else {
                        (start + i, start + stop)
                    });
                }
                i = stop;
                continue;
            }
        }
        i += next_char_len(bytes, i);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text an object covers, for the assertions below.
    fn cut(text: &str, offset: usize, object: Object, inner: bool) -> String {
        let (lo, hi) = range(text, offset, object, inner).unwrap();
        text[lo..hi].to_string()
    }

    /// What the parser counts as a section, pinned at the source every other
    /// surface now reads (headings-plan.md ruling 46). The offsets are written out
    /// rather than derived because the whole point of the scan is that a byte range
    /// means something to whoever reads it next.
    #[test]
    fn heading_spans_name_the_sections_the_parser_styles() {
        let text = concat!(
            "# The Fall\n",        // 0
            "body\n",              // 11
            "## Deep *Fall* ##\n", // 16
            "```\n",               // 34
            "# fenced\n",          // 38
            "```\n",               // 47
            "#\tTabbed\n",         // 51
            "#\n",                 // 60
            "> ## Quoted\n",       // 62
        );
        let spans = heading_spans(text);
        let seen: Vec<(usize, &str, u8)> = spans
            .iter()
            .map(|h| (h.line_start, &text[h.text_start..h.text_end], h.depth))
            .collect();
        assert_eq!(
            seen,
            vec![
                (0, "The Fall", 1),
                // The closing hashes are inside the styled text, so they are part of
                // the section's name here and in the anchor matcher alike. Neither
                // surface strips them today; that is a parser question, not an index
                // one, and the two agreeing is what matters (EDGE-18).
                (16, "Deep *Fall* ##", 2),
                // A tab after the markers is whitespace to the parser and not to the
                // anchor matcher, which insists on a space: this section is real and
                // unreachable by `[[note#Tabbed]]` (EDGE-16).
                (51, "Tabbed", 1),
                // A bare `#` is a section with no name.
                (60, "", 1),
            ],
            "the fenced `#` is not a section, and a blockquoted one is not either"
        );
    }

    #[test]
    fn a_wiki_link_is_the_brackets_and_the_name() {
        let text = "see [[Two]] now\n";
        // Anywhere on the construct, including its brackets.
        for offset in 4..=10 {
            assert_eq!(
                cut(text, offset, Object::Link, false),
                "[[Two]]",
                "offset {offset}"
            );
        }
        assert_eq!(cut(text, 5, Object::Link, true), "Two");
        // Off the construct: nothing, which cancels the operator.
        assert!(range(text, 0, Object::Link, false).is_none());
        assert!(range(text, 12, Object::Link, false).is_none());
    }

    #[test]
    fn an_aliased_link_takes_its_target_with_its_alias() {
        let text = "[[Two|the second]]\n";
        assert_eq!(cut(text, 3, Object::Link, false), "[[Two|the second]]");
        assert_eq!(cut(text, 3, Object::Link, true), "Two|the second");
    }

    #[test]
    fn a_markdown_link_takes_its_text_and_not_its_url() {
        let text = "a [label](https://example.com) here\n";
        assert_eq!(
            cut(text, 4, Object::Link, false),
            "[label](https://example.com)"
        );
        assert_eq!(cut(text, 4, Object::Link, true), "label");
        // The url is inside the outer object but not inside the text.
        assert_eq!(
            cut(text, 20, Object::Link, false),
            "[label](https://example.com)"
        );
        assert_eq!(cut(text, 20, Object::Link, true), "label");
    }

    #[test]
    fn an_embed_takes_its_bang() {
        let text = "![[pic.png]]\n";
        assert_eq!(cut(text, 1, Object::Link, false), "![[pic.png]]");
        assert_eq!(cut(text, 1, Object::Link, true), "pic.png");
    }

    #[test]
    fn a_link_inside_code_is_not_a_link() {
        let text = "`` [[Two]] `` and [[Three]]\n";
        // The cursor on the bracketed name inside the code span finds the code,
        // not a link; the one after finds the real link.
        assert!(range(text, 4, Object::Link, false).is_none());
        assert_eq!(cut(text, 20, Object::Link, false), "[[Three]]");
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_its_level() {
        let text = "# Top\nintro\n## Sub\nbody\ndeep\n# Other\ntail\n";
        // From the heading itself, and from anywhere below it, `a#` is the whole
        // section: up to the next level-1 heading, deeper headings included.
        assert_eq!(
            cut(text, 0, Object::Section, false),
            "# Top\nintro\n## Sub\nbody\ndeep\n"
        );
        assert_eq!(
            cut(text, 8, Object::Section, false),
            "# Top\nintro\n## Sub\nbody\ndeep\n"
        );
        // The inner object leaves the heading's own line alone.
        assert_eq!(
            cut(text, 0, Object::Section, true),
            "intro\n## Sub\nbody\ndeep\n"
        );
    }

    #[test]
    fn a_deeper_section_stops_at_its_own_heading() {
        let text = "# Top\nintro\n## Sub\nbody\ndeep\n# Other\ntail\n";
        assert_eq!(
            cut(text, 12, Object::Section, false),
            "## Sub\nbody\ndeep\n"
        );
        assert_eq!(cut(text, 12, Object::Section, true), "body\ndeep\n");
        // The last section runs to the end of the note.
        assert_eq!(cut(text, 30, Object::Section, false), "# Other\ntail\n");
    }

    #[test]
    fn text_before_the_first_heading_is_no_section() {
        let text = "intro\n# Top\nbody\n";
        assert!(range(text, 0, Object::Section, false).is_none());
    }

    #[test]
    fn a_heading_in_code_does_not_open_a_section() {
        let text = "# Top\n```\n# Fake\n```\nbody\n";
        assert_eq!(
            cut(text, 12, Object::Section, false),
            "# Top\n```\n# Fake\n```\nbody\n"
        );
    }

    #[test]
    fn a_callout_takes_its_header_and_its_markers() {
        let text = "> [!warning] Watch out\n> first\n> second\nafter\n";
        assert_eq!(
            cut(text, 2, Object::Callout, false),
            "> [!warning] Watch out\n> first\n> second\n"
        );
        assert_eq!(cut(text, 2, Object::Callout, true), "> first\n> second\n");
        // From a body line, the same box.
        assert_eq!(
            cut(text, 25, Object::Callout, false),
            "> [!warning] Watch out\n> first\n> second\n"
        );
        assert!(range(text, 41, Object::Callout, false).is_none());
    }

    #[test]
    fn a_one_line_callout_holds_its_title() {
        let text = "> [!note] just this\nafter\n";
        // The whole box is the line, marker and all.
        assert_eq!(
            cut(text, 2, Object::Callout, false),
            "> [!note] just this\n"
        );
        // With no body lines, the title text *is* the content: `ci*` retypes what
        // the box says and leaves the `[!note]` marker alone, so the note is still
        // a note afterwards.
        assert_eq!(cut(text, 2, Object::Callout, true), "just this");
        // The type name inside the brackets is never the inner object, and a
        // cursor on it still selects the title.
        assert_eq!(cut(text, 5, Object::Callout, true), "just this");
    }

    #[test]
    fn a_callout_with_no_text_has_an_empty_inner_object() {
        let text = "> [!note]\nafter\n";
        assert_eq!(cut(text, 2, Object::Callout, false), "> [!note]\n");
        // Nothing to change, but the range sits where the text would go, so `ci*`
        // puts the caret after the marker and lets the user write the box.
        let (lo, hi) = range(text, 2, Object::Callout, true).unwrap();
        assert_eq!((lo, hi), (9, 9));
    }

    #[test]
    fn two_callouts_are_two_boxes() {
        let text = "> [!note] one\n\n> [!tip] two\n";
        assert_eq!(cut(text, 2, Object::Callout, false), "> [!note] one\n");
        assert_eq!(cut(text, 18, Object::Callout, false), "> [!tip] two\n");
    }

    #[test]
    fn a_code_block_takes_its_fences() {
        let text = "before\n```rust\nlet x = 1;\nlet y = 2;\n```\nafter\n";
        assert_eq!(
            cut(text, 12, Object::Code, false),
            "```rust\nlet x = 1;\nlet y = 2;\n```\n"
        );
        assert_eq!(
            cut(text, 12, Object::Code, true),
            "let x = 1;\nlet y = 2;\n"
        );
        // From a fence line, the same block.
        assert_eq!(
            cut(text, 7, Object::Code, false),
            "```rust\nlet x = 1;\nlet y = 2;\n```\n"
        );
    }

    #[test]
    fn an_unclosed_block_runs_to_the_end_of_the_note() {
        let text = "```rust\nlet x = 1;\n";
        assert_eq!(cut(text, 3, Object::Code, true), "let x = 1;\n");
    }

    #[test]
    fn an_inline_code_span_is_the_backticks_and_the_code() {
        let text = "use `cargo test` here\n";
        assert_eq!(cut(text, 5, Object::Code, false), "`cargo test`");
        assert_eq!(cut(text, 5, Object::Code, true), "cargo test");
        // A longer run of backticks opens and closes at the same length.
        let text = "a ``a`b`` c\n";
        assert_eq!(cut(text, 3, Object::Code, false), "``a`b``");
        assert_eq!(cut(text, 3, Object::Code, true), "a`b");
    }

    #[test]
    fn a_fenced_block_wins_over_a_span_on_the_same_line() {
        let text = "```\nlet `x` = 1;\n```\n";
        assert_eq!(cut(text, 8, Object::Code, true), "let `x` = 1;\n");
    }

    #[test]
    fn each_item_of_a_list_has_its_own_quotation() {
        // The shape that made the bug obvious: every item after the first opens its
        // quotation on a line whose predecessor ended in a quote mark.
        let text = "- \"first\"\n- \"second\"\n";
        assert_eq!(cut(text, 2, Object::Quote, true), "first");
        assert_eq!(cut(text, 12, Object::Quote, true), "second");
    }

    #[test]
    fn every_object_still_answers_below_another_block() {
        // The report was that objects stopped working on a line under a list item.
        // The quote scan's line-break rule was the culprit; the others are checked
        // here too, since they all read the same scan.
        let text =
            "# Top\n- one\n- two\nsee [[Two]] and say \"hi\" now\n\n> [!note] box\n\n```\ncode\n```\n";
        let at = |what: &str| text.find(what).expect("the note holds it");

        // On the line under the list: a link and a quotation.
        assert_eq!(cut(text, at("[[Two]]") + 3, Object::Link, true), "Two");
        assert_eq!(cut(text, at("\"hi\"") + 2, Object::Quote, true), "hi");
        // The section the list is in still runs to the end of the note.
        assert_eq!(cut(text, at("see [[Two]]"), Object::Section, false), text);
        // The callout under the paragraph, and the fence under the callout.
        assert_eq!(
            cut(text, at("> [!note]") + 2, Object::Callout, false),
            "> [!note] box\n"
        );
        assert_eq!(cut(text, at("code") + 1, Object::Code, true), "code\n");
    }

    #[test]
    fn the_scope_letters_are_the_vocabulary() {
        assert_eq!(Object::from_char('L'), Some(Object::Link));
        assert_eq!(Object::from_char('#'), Some(Object::Section));
        assert_eq!(Object::from_char('*'), Some(Object::Callout));
        assert_eq!(Object::from_char('`'), Some(Object::Code));
        assert_eq!(Object::from_char('q'), Some(Object::Quote));
        // `w`, `p` and `t` are answered elsewhere, and a lowercase `l` is the
        // right motion, not a link.
        assert_eq!(Object::from_char('w'), None);
        assert_eq!(Object::from_char('p'), None);
        assert_eq!(Object::from_char('t'), None);
        assert_eq!(Object::from_char('l'), None);
        // The block object is `^`, the mark a block id is written with. `b` and `B`
        // stay what vim uses them for (ruling 10's first letters, and the reason it
        // could not keep them).
        assert_eq!(Object::from_char('^'), Some(Object::Block));
        assert_eq!(Object::from_char('b'), Some(Object::Pair(Pair::Paren)));
        assert_eq!(Object::from_char('B'), Some(Object::Pair(Pair::Brace)));
    }

    #[test]
    fn the_block_object_is_the_paragraph_under_the_caret() {
        let text = "intro line\nwrapped line\n\nnext para\n";
        let at = text.find("wrapped").unwrap();
        assert_eq!(
            cut(text, at, Object::Block, true),
            "intro line\nwrapped line",
            "a soft-wrapped paragraph is one object, wherever the caret sits in it"
        );
        assert_eq!(
            cut(text, at, Object::Block, false),
            "intro line\nwrapped line\n",
            "`a^` takes the block's line break with it, the way `ap` takes the gap"
        );
    }

    #[test]
    fn the_block_object_names_the_innermost_list_item() {
        let text = "- parent\n    - child\n    - second child\n- next\n";
        assert_eq!(
            cut(
                text,
                text.find("second child").unwrap(),
                Object::Block,
                true
            ),
            "    - second child",
            "the item the caret is in, not its parent, its indent as written"
        );
        assert_eq!(
            cut(text, text.find("parent").unwrap(), Object::Block, true),
            "- parent\n    - child\n    - second child",
            "an item's object holds its children (ruling 5)"
        );
    }

    #[test]
    fn a_table_a_quote_and_a_fence_are_each_one_block() {
        let text = "| a | b |\n| - | - |\n| 1 | 2 |\n\n> quoted\n\n```\nfenced\n```\n";
        assert_eq!(
            cut(text, text.find("1 | 2").unwrap(), Object::Block, true),
            "| a | b |\n| - | - |\n| 1 | 2 |",
            "the whole table, because the table is the atom (ruling 5)"
        );
        assert_eq!(
            cut(text, text.find("quoted").unwrap(), Object::Block, true),
            "> quoted"
        );
        assert_eq!(
            cut(text, text.find("fenced").unwrap(), Object::Block, true),
            "```\nfenced\n```"
        );
    }

    #[test]
    fn a_heading_line_is_its_own_block_object() {
        // Ruling 4: an id on a heading names the heading line, and so does the object.
        let text = "# The Fall ^fall\nbody\n";
        assert_eq!(cut(text, 2, Object::Block, true), "# The Fall ^fall");
    }

    #[test]
    fn the_block_object_cancels_on_a_blank_line() {
        // No block, no object — the operator cancels the way a find that matches
        // nothing does, rather than taking the block above by surprise.
        let text = "one\n\nthree\n";
        assert!(range(text, text.find("\n\n").unwrap() + 1, Object::Block, true).is_none());
    }

    #[test]
    fn the_block_object_and_a_block_id_name_the_same_atom() {
        // The point of ruling 10: `i^` and `#^id` are the same thing seen from two ends,
        // so this asserts they read one scan rather than two guesses.
        let text = "the goblin waits ^goblin\n\nnext\n";
        let map = block_map(text);
        let anchor = map.anchors.first().expect("the id was read");
        let block = &map.blocks[anchor.block];
        let (lo, hi) = range(
            text,
            text.find("goblin waits").unwrap(),
            Object::Block,
            true,
        )
        .unwrap();
        assert_eq!(
            (lo, hi),
            (block.start, block.end),
            "the object's range is the block the anchor names"
        );
    }
}
