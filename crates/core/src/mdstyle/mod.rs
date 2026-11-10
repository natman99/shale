//! Markdown style spans: what each range of the source should look like.
//!
//! Deliberately presentation-agnostic. The scanner reports *what* a range is
//! (a heading, emphasis, a table pipe, inline code, ...) and the UI decides the
//! colors and fonts, so themes can restyle without touching the parser and a
//! tree-sitter highlighter can later produce the same shape of output.
//!
//! Spans are absolute byte offsets into the whole text, always on character
//! boundaries, and may overlap: block-level spans (heading, quote, code block)
//! are emitted before the inline spans inside them, so a consumer that applies
//! them in order ends up with the more specific inline style winning.
//!
//! The scanner is line-based and deliberately pragmatic: it follows the subset
//! of markdown Obsidian renders in live preview, not the full CommonMark
//! grammar. Fenced code blocks and inline code spans suppress inline styling, as
//! Obsidian does.
//!
//! The scan is split in two: [`block`] classifies each line (a heading, a quote,
//! a callout, a fence, a table row) and [`inline`] styles the text inside those
//! constructs. This module holds the vocabulary both share — `Style`,
//! `StyleSpan`, `CalloutKind` — and the driver that walks the lines.

mod block;
mod inline;

// Typographic quotes: what each straight mark stands for, which decides both the
// glyph a rendering view draws and what the `q` text object selects.
pub mod quotes;

// The markdown text objects the vim engine acts on (`diL`, `ci*`, `` da` ``),
// which answer "what is the construct under this offset" from the same scan.
pub mod object;

use block::{callout_header_range, scan_line};

// The marker shapes a row can open with, shared with [`crate::continuation`],
// which continues on Enter exactly what this scanner recognises.
pub(crate) use block::{list_marker_end, task_checkbox};

/// What a range of markdown source is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Heading text, after the `#` markers.
    Heading { level: u8 },
    /// The `#` markers themselves.
    HeadingMarker,
    /// A blockquote's text.
    Quote,
    /// The `>` markers of a blockquote.
    QuoteMarker,
    /// A list bullet or number.
    ListMarker,
    /// A task checkbox, `[ ]` or `[x]`.
    TaskMark { done: bool },
    /// `**strong**` content.
    Strong,
    /// `*emphasis*` content.
    Emphasis,
    /// `~~strikethrough~~` content.
    Strike,
    /// `==highlight==` content.
    Highlight,
    /// The content of a code span, between its backticks.
    Code,
    /// The content of a fenced code block (one span per line).
    CodeBlock,
    /// The opening/closing fence line of a code block.
    CodeFence,
    /// The text of a `[text](url)` or `![[link]]`.
    LinkText,
    /// The `(url)` part of a `[text](url)`.
    LinkUrl,
    /// The target of an aliased wiki-link, `[[target|alias]]`: the part a reader
    /// never sees, so live preview hides it and shows only the alias.
    LinkTarget,
    /// The text of a `[[wiki-link]]` (its target, or its alias).
    WikiLink,
    /// A `![[wiki-embed]]`.
    WikiEmbed,
    /// A `#tag`.
    Tag,
    /// One line of an Obsidian callout (`> [!note] Title`), covering the line's
    /// content. The kind decides the accent colour the UI draws the box with.
    Callout { kind: CalloutKind },
    /// A callout's title line: its type name and any custom title.
    CalloutTitle { kind: CalloutKind },
    /// A `|` separating table cells.
    TablePipe,
    /// A table's `---` delimiter row.
    TableRule,
    /// A thematic break (`---`, `***`, `___`).
    Rule,
    /// A properties block's `---` fence, styled apart from [`Style::Rule`] on purpose.
    /// Live preview draws a thematic break as a rule across the editor and hides the
    /// dashes, which is exactly how a properties block ends up looking like a divider
    /// the author hoped meant something. The range comes from the parser the index
    /// runs, so the colour cannot promise properties the note does not have.
    FrontmatterFence,
    /// The whole properties block — both fences and every line between them — as one
    /// region span. It colours nothing: the line element paints a background band and
    /// a hairline boundary across the rows it covers, which is what makes the block
    /// read as one unit at a glance the way a fenced code block's background does.
    /// It is emitted by the same scan as the fences and the keys, so the band cannot
    /// cover rows the parser does not call a block, and a thematic break — which has
    /// no block — never gets one.
    FrontmatterBlock,
    /// The `key:` of a line inside a properties block. The value stays ordinary text:
    /// colouring it as though the editor understood it would be a claim about its type
    /// that the block alone does not support.
    FrontmatterKey,
    /// A properties block the parser refused, or a `---` on the first line that never
    /// closes. The note has **no** properties, and the block is drawn as the error it
    /// is instead of looking like it worked.
    FrontmatterBroken,
    /// Syntax punctuation to draw dimly: brackets, delimiters, backticks.
    Punctuation,
}

/// The kind of an Obsidian callout, from the name in `[!name]`.
///
/// Obsidian ships a fixed set of callout types, each with its own colour, and
/// treats anything else as a plain "note" box; the aliases it accepts are
/// folded onto the same variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalloutKind {
    Note,
    Abstract,
    Info,
    Todo,
    Tip,
    Success,
    Question,
    Warning,
    Failure,
    Danger,
    Bug,
    Example,
    Quote,
    /// A type Obsidian does not know: drawn like a note, but the name is the
    /// reader's only clue.
    Other,
}

impl CalloutKind {
    /// The kind a callout header names, case-insensitively, following Obsidian's
    /// alias table (`faq` is `question`, `caution` is `danger`, and so on).
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "note" => Self::Note,
            "abstract" | "summary" | "tldr" => Self::Abstract,
            "info" => Self::Info,
            "todo" => Self::Todo,
            "tip" | "hint" | "important" => Self::Tip,
            "success" | "check" | "done" | "positive" => Self::Success,
            "question" | "help" | "faq" => Self::Question,
            "warning" | "caution" | "attention" => Self::Warning,
            "failure" | "fail" | "missing" => Self::Failure,
            "danger" | "error" => Self::Danger,
            "bug" => Self::Bug,
            "example" => Self::Example,
            "quote" | "cite" => Self::Quote,
            _ => Self::Other,
        }
    }
}

/// A range of source that shares a [`Style`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleSpan {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

/// Style spans for the whole text, ordered by start offset.
pub fn style_spans(text: &str) -> Vec<StyleSpan> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let content_len = line.trim_end_matches(['\n', '\r']).len();
        lines.push((offset, &line[..content_len]));
        offset += line.len();
    }

    // Which lines belong to a table: GFM tables need a delimiter row, so a row
    // is a table row when it is (or touches) one.
    let table_rows = table_row_flags(&lines);
    let front = frontmatter(text, &lines);

    let mut out: Vec<StyleSpan> = Vec::new();
    // The block's region goes in first: it starts at or before everything else the
    // block owns, and it colours nothing, so the row walk can mark every row of the
    // block — including a blank one inside it, which carries no other span.
    if let Some(front) = &front {
        out.push(StyleSpan {
            start: front.whole.0,
            end: front.whole.1,
            style: Style::FrontmatterBlock,
        });
    }
    // A block's keys go in before the line walk, so an inline style inside a value
    // still wins over them the way every other block/inline pair does.
    if let Some(front) = &front {
        for &(start, end) in &front.keys {
            out.push(StyleSpan {
                start,
                end,
                style: Style::FrontmatterKey,
            });
        }
    }
    let mut fence: Option<(u8, usize)> = None;
    // The kind of the callout whose body the previous line belonged to, if any:
    // a callout runs until the first line that is not part of a blockquote.
    let mut callout: Option<CalloutKind> = None;
    for (index, (line_start, content)) in lines.iter().enumerate() {
        let line_end = *line_start + content.len();
        // A block the parser refused is the note's error, top to bottom: no inline
        // styling inside it, because a link colour over a block that indexes nothing
        // is the note lying about itself in two colours at once.
        if let Some((start, end)) = front.as_ref().and_then(|f| f.broken) {
            if *line_start >= start && line_end <= end {
                out.push(StyleSpan {
                    start: *line_start,
                    end: line_end,
                    style: Style::FrontmatterBroken,
                });
                continue;
            }
        }
        // A fence line belongs to the block, not to the thematic-break branch below,
        // which would draw it as a rule across the editor and hide the dashes.
        if front
            .as_ref()
            .is_some_and(|f| f.fences.contains(line_start))
        {
            out.push(StyleSpan {
                start: *line_start,
                end: line_end,
                style: Style::FrontmatterFence,
            });
            continue;
        }
        // Inside the block, a link is painted only where it *is* one: the whole value,
        // which is the only shape the indexer turns into an edge.
        let in_block = front
            .as_ref()
            .is_some_and(|f| *line_start >= f.inner.0 && *line_start < f.inner.1);
        let allowed = if in_block {
            link_regions(*line_start, content)
        } else {
            Vec::new()
        };
        let mark = out.len();
        scan_line(
            *line_start,
            content,
            &mut fence,
            &mut callout,
            table_rows[index],
            &mut out,
        );
        if in_block {
            // Two rules, one reason: the block is the note's data, so it shows what the
            // file says and claims nothing the index does not hold.
            //
            // * Nothing here is hidden. Live preview hides a link's brackets, an aliased
            //   link's target and a markdown link's URL — which in prose is the point, and
            //   in a properties block is a value that reads as `ally: the Zhents` when the
            //   file says `ally: "[[Zhentarim|the Zhents]]"`. The brackets are exactly the
            //   difference between a link and a string, which is the question the block is
            //   drawn to answer.
            // * A link is painted only where it *is* one: the whole value, the only shape
            //   the indexer turns into an edge (see [`link_regions`]).
            let kept: Vec<StyleSpan> = out
                .drain(mark..)
                .filter(|s| {
                    !crate::mdlayout::is_marker(s.style)
                        && (!is_link_style(s.style)
                            || allowed
                                .iter()
                                .any(|(from, to)| *from <= s.start && s.end <= *to))
                })
                .collect();
            out.extend(kept);
        }
    }
    out.sort_by_key(|s| (s.start, s.end));
    out
}

/// Where a note's properties block is, as the parser sees it.
///
/// The ranges come from [`crate::frontmatter::block`] — the same parse the indexer
/// runs — because the whole point of styling the block is to answer "is this being
/// picked up?", and an answer from a second, slightly different scanner is a second
/// opinion, not an answer.
struct Frontmatter {
    /// The byte offset each fence line starts at. Two of them, unless the block is
    /// broken, in which case the fences are covered by `broken`.
    fences: Vec<usize>,
    /// The whole block, first fence to the end of the last one (or the range the
    /// parser refused). The band the editor draws is asked of this, so the band and
    /// the parser cannot disagree about where the block is.
    whole: (usize, usize),
    /// Each `key:` inside the block, as the author wrote it.
    keys: Vec<(usize, usize)>,
    /// The whole block when the parser refused it, or the lone `---` when the note
    /// opens a block and never closes it.
    broken: Option<(usize, usize)>,
    /// The lines *between* the fences, as byte offsets: from the first line after the
    /// opening fence up to the closing one. A line inside it holds a property value,
    /// and a value gets a stricter link rule than the rest of the note (see
    /// [`link_regions`]).
    inner: (usize, usize),
}

/// The parts of a property line that the indexer would read as *one link*.
///
/// Everywhere else in a note, `[[X]]` is painted as a link because it is one: the body
/// scanner turns it into an edge. Inside a properties block that is no longer true —
/// the indexer asks one question about the whole value, `frontmatter::wikilink`, and a
/// value only becomes an edge when it *is* a single link. So `ally: "[[Zhentarim]]"`
/// paints as a link, and `group: see [[Zhentarim]] for details`, or
/// `group: [[Federation]] [[Empire]]`, paints as the text it is. A link colour there
/// would be the editor claiming an edge the index does not hold, which is the same
/// mistake as colouring a divider as a block.
///
/// The *meaning* comes from the parser — `wikilink`, `flow_list`, `unquote`,
/// `strip_comment` are the same functions `block` calls — so the only thing computed
/// here is where the value sits on the line.
fn link_regions(line_start: usize, content: &str) -> Vec<(usize, usize)> {
    let trimmed = content.trim_start();
    let indent = line_start + (content.len() - trimmed.len());
    // A list item's value is what follows its dash; a key's value is what follows its
    // colon. Anything else inside the block — an indented continuation, a nested map —
    // is structure the index files as inert, so nothing on it is a link.
    let value_at = if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| {
        trimmed
            .strip_prefix("* ")
            .or_else(|| trimmed.strip_prefix("+ "))
    }) {
        (indent + 2, rest)
    } else if let Some((_, key_end)) = crate::frontmatter::key_span(content) {
        let after = &content[key_end..];
        (
            line_start + key_end + (after.len() - after.trim_start().len()),
            after.trim_start(),
        )
    } else {
        return Vec::new();
    };
    let authored = crate::frontmatter::strip_comment(value_at.1).trim_end();
    if authored.is_empty() {
        return Vec::new();
    }
    let value = crate::frontmatter::unquote(authored);
    if crate::frontmatter::wikilink(&value).is_some() {
        return vec![(value_at.0, value_at.0 + authored.len())];
    }
    // A flow list carries one value per item, so each item decides for itself. The
    // parser hands back the items in order and each is a substring of what was written,
    // so walking forward through the line finds where each one sits.
    if let Some(items) = crate::frontmatter::flow_list(authored) {
        let mut regions = Vec::new();
        let mut cursor = 0usize;
        for item in items {
            if crate::frontmatter::wikilink(&item).is_none() {
                continue;
            }
            let Some(offset) = authored[cursor..].find(&item) else {
                continue;
            };
            let start = value_at.0 + cursor + offset;
            regions.push((start, start + item.len()));
            cursor += offset + item.len();
        }
        return regions;
    }
    Vec::new()
}

/// A style that tells the reader "this is a link".
fn is_link_style(style: Style) -> bool {
    matches!(
        style,
        Style::WikiLink | Style::WikiEmbed | Style::LinkTarget | Style::LinkText | Style::LinkUrl
    )
}

fn frontmatter(text: &str, lines: &[(usize, &str)]) -> Option<Frontmatter> {
    if let Some(block) = crate::frontmatter::block(text) {
        if block.malformed.is_some() {
            return Some(Frontmatter {
                fences: Vec::new(),
                whole: block.bytes,
                keys: Vec::new(),
                broken: Some(block.bytes),
                inner: block.bytes,
            });
        }
        let line_start = |index: usize| lines.get(index).map(|(start, _)| *start);
        let (Some(open), Some(close)) = (line_start(block.lines.0), line_start(block.lines.1))
        else {
            return None;
        };
        let mut keys = Vec::new();
        let mut inner = (close, close);
        for index in (block.lines.0 + 1)..block.lines.1 {
            if let Some((start, content)) = lines.get(index) {
                inner = (inner.0.min(*start), inner.1.max(*start + content.len()));
                if let Some((key_start, key_end)) = crate::frontmatter::key_span(content) {
                    keys.push((start + key_start, start + key_end));
                }
            }
        }
        let close_end = lines
            .get(block.lines.1)
            .map(|(start, content)| *start + content.len())
            .unwrap_or(close);
        return Some(Frontmatter {
            fences: vec![open, close],
            whole: (open, close_end),
            keys,
            broken: None,
            inner,
        });
    }
    // An opening fence that never closes. `block` answers `None` both for "this is not
    // frontmatter" and for "this is frontmatter with no closing fence", and the second
    // one is the note silently losing every property it wrote — the case the colour
    // exists to catch, so the scanner has to tell them apart itself.
    //
    // The line after the fence decides. A `key:` after it is an author who meant a
    // properties block and forgot to close it; a blank line, prose or nothing is a
    // thematic break written at the top of a note, which is what it has always looked
    // like.
    let (start, content) = *lines.first()?;
    let bare = content.strip_prefix('\u{feff}').unwrap_or(content);
    if bare != crate::frontmatter::FENCE {
        return None;
    }
    let closes = lines[1..]
        .iter()
        .any(|(_, content)| *content == crate::frontmatter::FENCE || *content == "...");
    if closes {
        return None;
    }
    let meant_a_block = lines
        .get(1)
        .is_some_and(|(_, content)| crate::frontmatter::key_span(content).is_some());
    if !meant_a_block {
        return None;
    }
    Some(Frontmatter {
        fences: Vec::new(),
        whole: (start, start + content.len()),
        keys: Vec::new(),
        broken: Some((start, start + content.len())),
        inner: (start, start + content.len()),
    })
}

/// The `[!type]` headers that open a callout, as byte ranges covering the
/// brackets, the type name and Obsidian's optional `-`/`+` fold marker.
///
/// This is markup, not prose. Live preview hides the brackets, and a checker that
/// has never heard of callouts reads the type name as the start of a sentence:
/// harper-ls answers `SentenceCapitalization` for `info` and `warning`, so the
/// one word the user has to get exactly right for the box to be a box is the one
/// that comes back underlined. The spell and grammar client drops the notes that
/// fall inside one of these ranges, and this is what it drops them against.
///
/// The walk keeps the same state as [`style_spans`], because only the line that
/// *opens* a callout has a header: a body line that happens to start with
/// `[!note]` is ordinary quote text, and stays where a checker can see it.
pub fn callout_headers(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let mut open = false;
    for line in text.split_inclusive('\n') {
        let content_len = line.trim_end_matches(['\n', '\r']).len();
        let content = &line[..content_len];
        // A line that is not part of a blockquote ends the callout, exactly as
        // `scan_line` decides it does.
        if !content.trim_start().starts_with('>') {
            open = false;
        } else if !open {
            if let Some((start, end)) = callout_header_range(content) {
                out.push((offset + start, offset + end));
                open = true;
            }
        }
        offset += line.len();
    }
    out
}

/// Which lines belong to a table: the delimiter row itself, and the rows above
/// and below it that contain a pipe. Shared with [`crate::table`] so styling
/// and cell editing can never disagree about what is a table.
pub fn table_row_flags(lines: &[(usize, &str)]) -> Vec<bool> {
    let mut flags = vec![false; lines.len()];
    for (index, (_, content)) in lines.iter().enumerate() {
        if !is_table_delimiter_row(content) {
            continue;
        }
        flags[index] = true;
        let mut above = index;
        while above > 0 && has_pipe(lines[above - 1].1) {
            above -= 1;
            flags[above] = true;
        }
        let mut below = index;
        while below + 1 < lines.len() && has_pipe(lines[below + 1].1) {
            below += 1;
            flags[below] = true;
        }
    }
    flags
}

fn has_pipe(content: &str) -> bool {
    content.contains('|')
}

/// A table delimiter row: `| --- | :--: |`, at least one cell. Must contain a
/// pipe, so a plain `---` thematic break is not mistaken for one.
pub fn is_table_delimiter_row(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() || !trimmed.contains('-') || !trimmed.contains('|') {
        return false;
    }
    let body = trimmed.trim_start_matches('|').trim_end_matches('|');
    if body.trim().is_empty() {
        return false;
    }
    body.split('|').all(|cell| {
        let cell = cell.trim();
        let cell = cell.strip_prefix(':').unwrap_or(cell);
        let cell = cell.strip_suffix(':').unwrap_or(cell);
        !cell.is_empty() && cell.chars().all(|c| c == '-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spans of `style` as (start, end) pairs, for readable assertions.
    fn spans(text: &str, style: Style) -> Vec<(usize, usize)> {
        style_spans(text)
            .into_iter()
            .filter(|s| s.style == style)
            .map(|s| (s.start, s.end))
            .collect()
    }

    /// The source each span of `style` covers.
    fn texts(text: &str, style: Style) -> Vec<&str> {
        style_spans(text)
            .into_iter()
            .filter(|s| s.style == style)
            .map(|s| &text[s.start..s.end])
            .collect()
    }

    /// The band the editor draws is asked of ONE region span, so it cannot cover a row
    /// the parser does not call part of the block, and a note whose only `---` is a
    /// thematic break has no region to draw at all.
    #[test]
    fn the_band_region_is_exactly_what_the_parser_calls_the_block() {
        let text = "---\ncr: 3\n\ntags: [a, b]\n---\n# Goblin\n\nA rule:\n\n---\n";
        let block = crate::frontmatter::block(text).unwrap();
        assert_eq!(
            spans(text, Style::FrontmatterBlock),
            vec![(block.bytes.0, block.bytes.1)],
            "the region is the parser's block range, not a second opinion on it"
        );
        // The blank line inside the block carries no key and no fence. The band is
        // asked of the region rather than of the spans, so it is still covered.
        let blank = text.find("\n\n").unwrap() + 1;
        let (from, to) = spans(text, Style::FrontmatterBlock)[0];
        assert!(
            from <= blank && blank < to,
            "the blank row inside a block sits inside its band"
        );
        // And the rule further down the note is not part of it.
        assert_eq!(spans(text, Style::FrontmatterBlock).len(), 1);
        assert!(spans("# Title\n\n---\n\nbody\n", Style::FrontmatterBlock).is_empty());
    }

    /// A block that never closes is still the block's rows, and the region is where
    /// the editor says so: the fence the author opened, which is what the broken
    /// colour already marks.
    #[test]
    fn an_unclosed_block_still_has_a_region() {
        let text = "---\ncr: 3\nno closing fence\n";
        assert_eq!(
            texts(text, Style::FrontmatterBlock),
            texts(text, Style::FrontmatterBroken),
            "the band and the error colour answer for the same rows"
        );
    }

    /// A properties block is not a divider, and the difference is the whole point:
    /// live preview draws a thematic break as a rule across the editor and hides its
    /// dashes, so a block and a rule used to look identical.
    #[test]
    fn a_properties_block_is_styled_as_a_block() {
        let text = "---\ncr: 3\ntags: [a, b]\n---\n# Goblin\n\nA rule:\n\n---\n";
        assert_eq!(texts(text, Style::FrontmatterFence), vec!["---", "---"]);
        assert_eq!(texts(text, Style::FrontmatterKey), vec!["cr:", "tags:"]);
        // The rule further down the note is still a rule; the block's fences are not.
        assert_eq!(texts(text, Style::Rule), vec!["---"]);
    }

    /// A value inside the block keeps whatever inline styling it earns, because a
    /// `[[…]]` in a property is link-shaped — that is what writes the `edges` table —
    /// and a block colour that swallowed it would hide the one value the graph cares
    /// about.
    #[test]
    fn a_property_value_keeps_its_inline_styling() {
        let text = "---\nally: [[Goblin]]\n---\n";
        assert_eq!(texts(text, Style::WikiLink), vec!["Goblin"]);
        assert_eq!(texts(text, Style::FrontmatterKey), vec!["ally:"]);
    }

    /// Inside a block the link colour is a claim about the index: it goes where the
    /// indexer files an edge, and nowhere the indexer does not.
    #[test]
    fn a_property_is_painted_as_a_link_only_where_it_is_one() {
        // The whole value is the link: painted.
        assert_eq!(
            texts("---\nally: \"[[Zhentarim]]\"\n---\n", Style::WikiLink),
            vec!["Zhentarim"]
        );
        // A link *inside* a value that is not a link: no edge, so no colour.
        assert!(spans(
            "---\nnotes: see [[Zhentarim]] for details\n---\n",
            Style::WikiLink
        )
        .is_empty());
        // Two links in one value is not one link either — the parser calls it `text`.
        assert!(spans(
            "---\ngroup: [[Federation]] [[Empire]]\n---\n",
            Style::WikiLink
        )
        .is_empty());
        // A link to a section of this note names no note.
        assert!(spans("---\nsee: [[#Government]]\n---\n", Style::WikiLink).is_empty());
        // A markdown link in a property makes no edge either: the indexer only asks
        // `frontmatter::wikilink` about the value.
        let web = "---\nweb: [the web](https://example.com)\n---\n";
        assert!(spans(web, Style::LinkText).is_empty());
        assert!(spans(web, Style::LinkUrl).is_empty());
        // And the body is untouched by any of this.
        assert_eq!(
            texts("see [[Zhentarim]] for details", Style::WikiLink),
            vec!["Zhentarim"]
        );
    }

    /// Every value the index files as a link is painted as one, including the shapes
    /// that carry more than one value on a line.
    #[test]
    fn every_property_the_index_files_as_a_link_is_painted_as_one() {
        assert_eq!(
            texts(
                "---\ngroup:\n  - \"[[Federation]]\"\n  - [[Empire]]\n  - plain\n---\n",
                Style::WikiLink
            ),
            vec!["Federation", "Empire"]
        );
        assert_eq!(
            texts(
                "---\ngroup: [\"[[Federation]]\", \"[[Empire]]\", plain]\n---\n",
                Style::WikiLink
            ),
            vec!["Federation", "Empire"]
        );
        // An aliased link keeps its link colour on the alias, but unlike prose its
        // target and brackets are not marker spans: nothing in the block is hidden.
        let aliased = "---\nally: \"[[Zhentarim|the Zhents]]\"\n---\n";
        assert_eq!(texts(aliased, Style::WikiLink), vec!["the Zhents"]);
        assert!(spans(aliased, Style::LinkTarget).is_empty());
        assert!(spans(aliased, Style::Punctuation).is_empty());
        let web = "---\nweb: \"[the web](https://example.com)\"\n---\n";
        assert!(spans(web, Style::LinkUrl).is_empty());
        // An anchored link is still a link to the note, anchor and all.
        assert!(!spans(
            "---\nregion: \"[[Waterdeep#Government]]\"\n---\n",
            Style::WikiLink
        )
        .is_empty());
    }

    /// The drift guard: the painter's idea of "this value is a link" and the parser's
    /// are the same idea. If `block` starts filing a value as a link the colour does not
    /// cover — or the other way round — this fails rather than the vault quietly lying.
    #[test]
    fn the_link_colour_and_the_index_agree_on_every_shape() {
        for shape in [
            "ally: \"[[Zhentarim]]\"",
            "ally: [[Zhentarim]]",
            "ally: [[Zhentarim|the Zhents]]",
            "ally: [[Zhentarim#A heading]]",
            "ally: see [[Zhentarim]] for details",
            "ally: [[Federation]] [[Empire]]",
            "ally: [\"[[Federation]]\", \"[[Empire]]\"]",
            "ally: [\"[[Federation]]\", plain]",
            "ally: ![[map.png]]",
            "ally: [[#a heading]]",
            "ally: Zhentarim",
            "ally:",
            "web: [the web](https://example.com)",
        ] {
            let text = format!("---\n{shape}\n---\nbody\n");
            let block = crate::frontmatter::block(&text).unwrap_or_else(|| panic!("{shape}"));
            let filed = block
                .props
                .iter()
                .filter(|p| p.kind == crate::frontmatter::Kind::Link)
                .count();
            let painted = link_regions(4, shape.trim()).len();
            assert_eq!(painted, filed, "{shape}: the colour and the index disagree");
        }
    }

    /// A block the parser refuses indexes nothing, so it is drawn as the error it is
    /// rather than in the colour of a block that works.
    #[test]
    fn a_block_that_will_not_parse_is_the_error_it_is() {
        // An unquoted value still holding `: ` is genuinely ambiguous YAML (EDGE-11).
        let text = "---\nname: goblin: yes\n---\nbody\n";
        assert_eq!(
            texts(text, Style::FrontmatterBroken),
            vec!["---", "name: goblin: yes", "---"]
        );
        assert!(spans(text, Style::FrontmatterFence).is_empty());
        assert!(spans(text, Style::FrontmatterKey).is_empty());
        assert!(spans(text, Style::Rule).is_empty());
    }

    /// The nastiest silent failure in the file: an opening fence with no closing one.
    /// The parser sees no block at all, so every property in the note is dropped, and
    /// the one line that says so has been drawing as a decorative rule.
    #[test]
    fn an_unclosed_block_says_so() {
        let text = "---\ncr: 3\n# Goblin\n";
        assert_eq!(texts(text, Style::FrontmatterBroken), vec!["---"]);
        assert!(spans(text, Style::Rule).is_empty());
    }

    /// The other side of that call: a `---` at the top of a note followed by prose is
    /// a thematic break someone wrote on purpose, and it keeps its rule.
    #[test]
    fn a_rule_at_the_top_of_a_note_is_still_a_rule() {
        for text in ["---\n\nprose\n", "---\nprose\n", "---\n"] {
            assert_eq!(texts(text, Style::Rule), vec!["---"], "{text:?}");
            assert!(spans(text, Style::FrontmatterBroken).is_empty(), "{text:?}");
        }
    }

    /// `...` closes a block as well as `---` (EDGE-2), and the fence the author wrote
    /// is the fence the note shows.
    #[test]
    fn a_dots_fence_is_a_fence() {
        let text = "---\ncr: 3\n...\nbody\n";
        assert_eq!(texts(text, Style::FrontmatterFence), vec!["---", "..."]);
        assert_eq!(texts(text, Style::FrontmatterKey), vec!["cr:"]);
    }

    #[test]
    fn headings() {
        let text = "# Title\ntext\n### Sub";
        assert_eq!(spans(text, Style::Heading { level: 1 }), vec![(2, 7)]);
        assert_eq!(texts(text, Style::HeadingMarker), vec!["#", "###"]);
        assert_eq!(spans(text, Style::Heading { level: 3 }), vec![(17, 20)]);
        assert_eq!(texts(text, Style::Heading { level: 3 }), vec!["Sub"]);
    }

    #[test]
    fn emphasis_and_code() {
        let text = "a **b** c *d* `e` ~~f~~ ==g==";
        assert_eq!(texts(text, Style::Strong), vec!["b"]);
        assert_eq!(texts(text, Style::Emphasis), vec!["d"]);
        assert_eq!(texts(text, Style::Code), vec!["e"]);
        assert_eq!(texts(text, Style::Strike), vec!["f"]);
        assert_eq!(texts(text, Style::Highlight), vec!["g"]);
        assert_eq!(
            texts(text, Style::Punctuation),
            vec!["**", "**", "*", "*", "`", "`", "~~", "~~", "==", "=="],
            "every inline marker, including the code span's backticks"
        );
    }

    #[test]
    fn an_escaped_marker_is_punctuation() {
        let text = "a \\* literal \\b";
        assert_eq!(texts(text, Style::Punctuation), vec!["\\"]);
        assert!(spans(text, Style::Emphasis).is_empty());
    }

    #[test]
    fn code_suppresses_inline_markers() {
        let text = "`a **b** c`";
        assert_eq!(texts(text, Style::Code), vec!["a **b** c"]);
        assert!(spans(text, Style::Strong).is_empty());

        let fenced = "```\nlet x = **y**;\n```";
        assert_eq!(texts(fenced, Style::CodeBlock), vec!["let x = **y**;"]);
        assert_eq!(texts(fenced, Style::CodeFence), vec!["```", "```"]);
        assert!(spans(fenced, Style::Strong).is_empty());
    }

    #[test]
    fn snake_case_is_not_emphasis() {
        let text = "snake_case_name and _real_";
        assert_eq!(texts(text, Style::Emphasis), vec!["real"]);
    }

    #[test]
    fn quotes_lists_and_tasks() {
        let text = "> quoted **bold**\n- item\n1. other\n- [x] done\n- [ ] todo";
        assert_eq!(texts(text, Style::QuoteMarker), vec![">"]);
        assert_eq!(texts(text, Style::Quote), vec!["quoted **bold**"]);
        assert_eq!(texts(text, Style::Strong), vec!["bold"]);
        assert_eq!(
            texts(text, Style::ListMarker),
            vec!["- ", "1. ", "- ", "- "]
        );
        assert_eq!(
            texts(text, Style::TaskMark { done: true }),
            vec!["[x]"],
            "a checked box"
        );
        assert_eq!(
            texts(text, Style::TaskMark { done: false }),
            vec!["[ ]"],
            "and the unchecked one"
        );
    }

    #[test]
    fn callouts_are_a_styled_blockquote() {
        let text = "> [!warning] Watch out\n> mind the **gap**\n\nplain";
        assert_eq!(
            texts(
                text,
                Style::Callout {
                    kind: CalloutKind::Warning
                }
            ),
            vec!["> [!warning] Watch out", "> mind the **gap**"],
            "the box covers both lines and stops at the blank one"
        );
        assert_eq!(
            texts(
                text,
                Style::CalloutTitle {
                    kind: CalloutKind::Warning
                }
            ),
            vec!["warning", "Watch out"],
            "the type name and the custom title are the box's heading"
        );
        assert_eq!(
            texts(text, Style::Punctuation),
            vec!["[!", "]", "**", "**"],
            "the header's brackets are hidden syntax, the type name is not"
        );
        assert!(
            texts(text, Style::Quote).is_empty(),
            "a callout is not drawn as quote text"
        );
        assert_eq!(texts(text, Style::Strong), vec!["gap"]);
    }

    #[test]
    fn callout_kinds_and_fold_markers() {
        let text = "> [!todo]-\n> body";
        assert_eq!(
            texts(text, Style::Punctuation),
            vec!["[!", "]-"],
            "the fold marker is syntax too"
        );
        assert_eq!(
            spans(
                text,
                Style::CalloutTitle {
                    kind: CalloutKind::Todo
                }
            ),
            vec![(4, 8)],
            "a callout with no title has only its type name, which starts after `[!`"
        );
        // Obsidian's aliases name the same box.
        assert_eq!(CalloutKind::from_name("CAUTION"), CalloutKind::Warning);
        assert_eq!(CalloutKind::from_name("faq"), CalloutKind::Question);
        assert_eq!(CalloutKind::from_name("thing"), CalloutKind::Other);
    }

    /// The `[!type]` headers a checker has to be kept away from: the marker whole,
    /// Obsidian's fold marker included. The blank line matters — consecutive
    /// blockquote lines are one box, so the second header opens a second one.
    #[test]
    fn callout_header_ranges() {
        let text = "> [!info] Heads up\n> body\n\n> [!warning]- Folded\n";
        let found: Vec<&str> = callout_headers(text)
            .iter()
            .map(|(start, end)| &text[*start..*end])
            .collect();
        assert_eq!(found, ["[!info]", "[!warning]-"]);
    }

    /// Only the line that *opens* a callout has a header, and a bracket that is
    /// not a bare word is not a header at all — the same rules [`style_spans`]
    /// uses, so the ranges and the painted box can never disagree about which
    /// lines are a box.
    #[test]
    fn only_a_real_header_opens_a_callout() {
        let text = "> [!note]\n> [!note] again\n> [! not a header]\n> [!note!]\n";
        let found: Vec<&str> = callout_headers(text)
            .iter()
            .map(|(start, end)| &text[*start..*end])
            .collect();
        assert_eq!(found, ["[!note]"], "the rest of it is quote text");
    }

    /// A header inside a callout that is already open is body text — which is also
    /// how it is painted, so a checker is right to have its say about it.
    #[test]
    fn a_header_inside_an_open_callout_is_body_text() {
        let text = "> [!info] Heads up\n> [!todo] Nested\n";
        let found: Vec<&str> = callout_headers(text)
            .iter()
            .map(|(start, end)| &text[*start..*end])
            .collect();
        assert_eq!(found, ["[!info]"]);
        // The same line on its own is a header, nested quote markers and all.
        assert_eq!(callout_headers(">> [!todo] Nested\n").len(), 1);
    }

    #[test]
    fn a_quote_that_is_not_a_callout_stays_a_quote() {
        let text = "> [! not a callout]\n> [note] plain";
        assert!(spans(
            text,
            Style::Callout {
                kind: CalloutKind::Note
            }
        )
        .is_empty());
        assert_eq!(
            texts(text, Style::Quote),
            vec!["[! not a callout]", "[note] plain"]
        );
    }

    #[test]
    fn rules_and_tables() {
        let text = "a | b\n--- | ---\nc | d\n\n***";
        assert_eq!(texts(text, Style::TableRule), vec!["--- | ---"]);
        assert_eq!(texts(text, Style::TablePipe), vec!["|", "|"]);
        assert_eq!(texts(text, Style::Rule), vec!["***"]);
    }

    #[test]
    fn a_lone_dash_line_is_a_rule_but_a_table_needs_a_delimiter() {
        let text = "a | b\n- item";
        assert!(spans(text, Style::TablePipe).is_empty());
        assert!(spans(text, Style::TableRule).is_empty());
    }

    #[test]
    fn links_and_tags() {
        let text = "[text](url) ![[img.png]] [[Note]] #tag here";
        assert_eq!(texts(text, Style::LinkText), vec!["text"]);
        assert_eq!(texts(text, Style::LinkUrl), vec!["(url)"]);
        assert_eq!(texts(text, Style::WikiEmbed), vec!["img.png"]);
        assert_eq!(texts(text, Style::WikiLink), vec!["Note"]);
        assert_eq!(texts(text, Style::Tag), vec!["#tag"]);
        assert_eq!(
            texts(text, Style::Punctuation),
            vec!["[", "]", "![[", "]]", "[[", "]]"]
        );
    }

    #[test]
    fn an_aliased_wiki_link_keeps_only_the_alias_visible() {
        let text = "[[The Target|The Alias]]";
        assert_eq!(texts(text, Style::LinkTarget), vec!["The Target|"]);
        assert_eq!(texts(text, Style::WikiLink), vec!["The Alias"]);
        assert_eq!(texts(text, Style::Punctuation), vec!["[[", "]]"]);
    }

    #[test]
    fn spans_stay_on_char_boundaries_and_in_order() {
        let text = "# Ünïcode **bold** 🎉\n- [x] ✅ done\n";
        for span in style_spans(text) {
            assert!(text.is_char_boundary(span.start), "start {:?}", span);
            assert!(text.is_char_boundary(span.end), "end {:?}", span);
            assert!(span.start < span.end, "empty span {:?}", span);
        }
        let spans = style_spans(text);
        assert!(spans.windows(2).all(|w| w[0].start <= w[1].start));
    }

    #[test]
    fn multibyte_offsets_are_bytes() {
        let text = "é **b**";
        assert_eq!(texts(text, Style::Strong), vec!["b"]);
        assert_eq!(
            style_spans(text)
                .into_iter()
                .find(|s| s.style == Style::Strong)
                .map(|s| &text[s.start..s.end]),
            Some("b")
        );
    }
}
