//! Markdown styling spans: what each range of the source should look like.
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

    let mut out: Vec<StyleSpan> = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    // The kind of the callout whose body the previous line belonged to, if any:
    // a callout runs until the first line that is not part of a blockquote.
    let mut callout: Option<CalloutKind> = None;
    for (index, (line_start, content)) in lines.iter().enumerate() {
        scan_line(
            *line_start,
            content,
            &mut fence,
            &mut callout,
            table_rows[index],
            &mut out,
        );
    }
    out.sort_by_key(|s| (s.start, s.end));
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

/// A thematic break: three or more of the same `-`, `*` or `_`, spaces allowed.
fn is_thematic_break(content: &str) -> bool {
    let trimmed = content.trim();
    let Some(first) = trimmed.chars().next() else {
        return false;
    };
    if !matches!(first, '-' | '*' | '_') {
        return false;
    }
    let mut count = 0;
    for c in trimmed.chars() {
        if c == first {
            count += 1;
        } else if !(c == ' ' || c == '\t') {
            return false;
        }
    }
    count >= 3
}

fn scan_line(
    base: usize,
    content: &str,
    fence: &mut Option<(u8, usize)>,
    callout: &mut Option<CalloutKind>,
    in_table: bool,
    out: &mut Vec<StyleSpan>,
) {
    let line_end = base + content.len();
    let trimmed = content.trim_start();

    // Anything that is not a blockquote line ends the callout that was open
    // (including a blank line, a fence, or ordinary text).
    if !trimmed.starts_with('>') {
        *callout = None;
    }

    // Inside a fenced block: everything is code until the closing fence.
    if let Some((fc, flen)) = *fence {
        if !trimmed.is_empty() && closes_fence(trimmed.as_bytes(), fc, flen) {
            *fence = None;
            out.push(StyleSpan {
                start: base,
                end: line_end,
                style: Style::CodeFence,
            });
        } else {
            out.push(StyleSpan {
                start: base,
                end: line_end,
                style: Style::CodeBlock,
            });
        }
        return;
    }

    if let Some((fc, flen)) = opens_fence(trimmed.as_bytes()) {
        *fence = Some((fc, flen));
        out.push(StyleSpan {
            start: base,
            end: line_end,
            style: Style::CodeFence,
        });
        return;
    }

    if content.trim().is_empty() {
        return;
    }

    // ATX heading: `#` markers, then the heading text.
    let hashes = content.chars().take_while(|&c| c == '#').count();
    if hashes > 0
        && hashes <= 6
        && content[hashes..]
            .chars()
            .next()
            .is_none_or(|c| c == ' ' || c == '\t')
    {
        let marker_end = base + hashes;
        out.push(StyleSpan {
            start: base,
            end: marker_end,
            style: Style::HeadingMarker,
        });
        let text_start = content[hashes..]
            .char_indices()
            .find(|(_, c)| *c != ' ' && *c != '\t')
            .map(|(i, _)| base + hashes + i)
            .unwrap_or(line_end);
        if text_start < line_end {
            out.push(StyleSpan {
                start: text_start,
                end: line_end,
                style: Style::Heading {
                    level: hashes as u8,
                },
            });
            inline_spans(text_start, &content[text_start - base..], out);
        }
        return;
    }

    if is_thematic_break(content) {
        out.push(StyleSpan {
            start: base,
            end: line_end,
            style: Style::Rule,
        });
        return;
    }

    // A table's delimiter row is styled as a whole.
    if in_table && is_table_delimiter_row(content) {
        out.push(StyleSpan {
            start: base,
            end: line_end,
            style: Style::TableRule,
        });
        return;
    }

    // Blockquote: one or more `>` markers, then quoted text (which may itself
    // hold inline styles or a nested quote).
    if trimmed.starts_with('>') {
        let mut i = base;
        while i < line_end {
            let rest = &text_from(content, i - base);
            let Some(offset) = rest.chars().next() else {
                break;
            };
            if offset != '>' {
                break;
            }
            let next = i + offset.len_utf8();
            out.push(StyleSpan {
                start: i,
                end: next,
                style: Style::QuoteMarker,
            });
            i = next;
            // Skip the single optional space after the marker.
            if text_from(content, i - base).starts_with(' ') {
                i += 1;
            }
        }
        let content_start = i;
        // A callout is a blockquote whose first line opens with `[!type]`; the
        // blockquote lines that follow it are the same box, so they inherit its
        // kind. Only the line that opens the callout has a header.
        let header = if callout.is_none() {
            callout_header(text_from(content, content_start - base))
        } else {
            None
        };
        let kind = match (*callout, header) {
            (Some(kind), _) => kind,
            (None, Some((kind, ..))) => kind,
            (None, None) => {
                // An ordinary blockquote.
                if content_start < line_end {
                    out.push(StyleSpan {
                        start: content_start,
                        end: line_end,
                        style: Style::Quote,
                    });
                    inline_spans(content_start, text_from(content, content_start - base), out);
                }
                return;
            }
        };
        *callout = Some(kind);
        out.push(StyleSpan {
            start: base,
            end: line_end,
            style: Style::Callout { kind },
        });
        let title_start = match header {
            Some((_, name_end, header_end, title)) => {
                // `[!` and `]` are syntax the reader should not see; the type
                // name and the custom title are the callout's heading.
                out.push(StyleSpan {
                    start: content_start,
                    end: content_start + 2,
                    style: Style::Punctuation,
                });
                out.push(StyleSpan {
                    start: content_start + 2,
                    end: content_start + name_end,
                    style: Style::CalloutTitle { kind },
                });
                if header_end > name_end {
                    out.push(StyleSpan {
                        start: content_start + name_end,
                        end: content_start + header_end,
                        style: Style::Punctuation,
                    });
                }
                Some(content_start + title)
            }
            // A body line of the callout: ordinary text on the box's
            // background, with no title styling.
            None => {
                if content_start < line_end {
                    inline_spans(content_start, text_from(content, content_start - base), out);
                }
                None
            }
        };
        if let Some(title_start) = title_start.filter(|&start| start < line_end) {
            out.push(StyleSpan {
                start: title_start,
                end: line_end,
                style: Style::CalloutTitle { kind },
            });
            inline_spans(title_start, text_from(content, title_start - base), out);
        }
        return;
    }

    // List item, with an optional task checkbox.
    if let Some(marker_end) = list_marker_end(content) {
        out.push(StyleSpan {
            start: base,
            end: base + marker_end,
            style: Style::ListMarker,
        });
        let rest = &content[marker_end..];
        if let Some(checkbox) = task_checkbox(rest) {
            out.push(StyleSpan {
                start: base + marker_end,
                end: base + marker_end + checkbox.len(),
                style: Style::TaskMark {
                    done: checkbox.contains('x') || checkbox.contains('X'),
                },
            });
            let text_start = base + marker_end + checkbox.len();
            if text_start < line_end {
                inline_spans(text_start, &content[marker_end + checkbox.len()..], out);
            }
            table_pipes(base, content, in_table, out);
            return;
        }
        inline_spans(base + marker_end, rest, out);
        table_pipes(base, content, in_table, out);
        return;
    }

    if in_table {
        table_pipes(base, content, true, out);
    }

    inline_spans(base, content, out);
}

/// The slice of `content` from a byte offset that is already known to be a
/// character boundary.
fn text_from(content: &str, offset: usize) -> &str {
    &content[offset..]
}

/// The `[!type]` header that opens a callout, matched against the text after a
/// blockquote's `>` markers.
///
/// Returns the callout's [`CalloutKind`], the end of its type name, the end of
/// the whole header (the `]` plus Obsidian's optional `-`/`+` fold marker) and
/// where the custom title begins — all byte offsets from the start of `rest`,
/// each on a character boundary. A header must be `[!` followed by a bare word,
/// so `[! a note]` or `[!note!]` stay ordinary quote text.
fn callout_header(rest: &str) -> Option<(CalloutKind, usize, usize, usize)> {
    let body = rest.strip_prefix("[!")?;
    let close = body.find(']')?;
    let name = &body[..close];
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'/')
    {
        return None;
    }
    // `body` starts two bytes into `rest`, at the `[!`.
    let name_end = 2 + close;
    let mut header_end = name_end + 1;
    if rest[header_end..].starts_with(['-', '+']) {
        header_end += 1;
    }
    let title_start = if rest[header_end..].starts_with(' ') {
        header_end + 1
    } else {
        header_end
    };
    Some((
        CalloutKind::from_name(name),
        name_end,
        header_end,
        title_start,
    ))
}

/// `---` / `1.` / `+` list markers, including their trailing whitespace.
fn list_marker_end(content: &str) -> Option<usize> {
    let indent = content
        .char_indices()
        .find(|(_, c)| *c != ' ' && *c != '\t')
        .map(|(i, _)| i)
        .unwrap_or(content.len());
    let rest = &content[indent..];
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    let marker_len = if matches!(first, '-' | '*' | '+') {
        first.len_utf8()
    } else if first.is_ascii_digit() {
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        let after = &rest[digits..];
        if after.starts_with('.') || after.starts_with(')') {
            digits + 1
        } else {
            return None;
        }
    } else {
        return None;
    };
    let after = &rest[marker_len..];
    if !after.starts_with(' ') && !after.starts_with('\t') {
        return None;
    }
    let spaces = after
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    Some(indent + marker_len + spaces)
}

/// `[ ]` or `[x]` at the start of a list item's content.
fn task_checkbox(rest: &str) -> Option<&str> {
    if !rest.starts_with('[') {
        return None;
    }
    let marker = &rest[..rest
        .char_indices()
        .nth(3)
        .map(|(i, _)| i)
        .unwrap_or(rest.len())];
    if marker == "[ ]" || marker == "[x]" || marker == "[X]" {
        Some(marker)
    } else {
        None
    }
}

fn table_pipes(base: usize, content: &str, in_table: bool, out: &mut Vec<StyleSpan>) {
    if !in_table {
        return;
    }
    for (i, c) in content.char_indices() {
        if c == '|' {
            out.push(StyleSpan {
                start: base + i,
                end: base + i + 1,
                style: Style::TablePipe,
            });
        }
    }
}

/// Inline spans inside `content`, whose text starts at absolute `base`.
fn inline_spans(base: usize, content: &str, out: &mut Vec<StyleSpan>) {
    let bytes = content.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                // An escape covers the next character. The backslash itself is
                // syntax, so it is marked as punctuation: live preview hides it
                // and `\*` reads as a literal `*`.
                let next = if i + 1 < bytes.len() {
                    next_char_len(bytes, i + 1)
                } else {
                    0
                };
                if next > 0 && (bytes[i + 1] as char).is_ascii_punctuation() {
                    push_span(base, i, i + 1, Style::Punctuation, out);
                }
                i += 1 + next;
            }
            b'`' => {
                let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
                let open = i + run;
                if let Some(found) = find_subslice(&bytes[open..], &vec![b'`'; run]) {
                    // `found` is relative to `open`: the closing run starts at
                    // `close` and the whole span ends at `end`.
                    let close = open + found;
                    let end = close + run;
                    // The backticks are punctuation (hidden in live preview, and
                    // outside the code span's background); only the content
                    // between them is the code.
                    push_span(base, i, open, Style::Punctuation, out);
                    push_span(base, open, close, Style::Code, out);
                    push_span(base, close, end, Style::Punctuation, out);
                    i = end;
                } else {
                    i += 1;
                }
            }
            b'!' if bytes.get(i + 1) == Some(&b'[') => {
                // `![[embed]]` before `![alt](url)`.
                let end = if bytes.get(i + 2) == Some(&b'[') {
                    wiki_link(base, content, i, out)
                } else {
                    md_link(base, content, i, out)
                };
                i = end.map(|end| end - base).unwrap_or(i + 1);
            }
            b'[' if bytes.get(i + 1) == Some(&b'[') => {
                if let Some(end) = wiki_link(base, content, i, out) {
                    i = end - base;
                } else {
                    i += 1;
                }
            }
            b'[' => {
                if let Some(end) = md_link(base, content, i, out) {
                    i = end - base;
                } else {
                    i += 1;
                }
            }
            b'#' if is_tag_start(bytes, i) => {
                let end = tag_end(bytes, i);
                out.push(StyleSpan {
                    start: base + i,
                    end: base + end,
                    style: Style::Tag,
                });
                i = end;
            }
            b'=' if bytes.get(i + 1) == Some(&b'=') => {
                if let Some(end) = paired(bytes, i, "==") {
                    push_wrapped(base, i, end, "==", Style::Highlight, out);
                    i = end + 2;
                } else {
                    i += 2;
                }
            }
            b'~' if bytes.get(i + 1) == Some(&b'~') => {
                if let Some(end) = paired(bytes, i, "~~") {
                    push_wrapped(base, i, end, "~~", Style::Strike, out);
                    i = end + 2;
                } else {
                    i += 2;
                }
            }
            b'*' | b'_' => {
                let two = bytes.get(i + 1) == Some(&bytes[i]);
                let marker = if two {
                    &content[i..i + 2]
                } else {
                    &content[i..i + 1]
                };
                // `_` only opens emphasis at a word boundary, so snake_case and
                // `a_b_c` stay plain.
                let underscore_inside_word =
                    bytes[i] == b'_' && i > 0 && (bytes[i - 1] as char).is_alphanumeric();
                if underscore_inside_word {
                    i += 1;
                    continue;
                }
                if let Some(end) = paired(bytes, i, marker) {
                    let style = if two { Style::Strong } else { Style::Emphasis };
                    push_wrapped(base, i, end, marker, style, out);
                    i = end + marker.len();
                } else {
                    i += marker.len();
                }
            }
            _ => i += next_char_len(bytes, i),
        }
    }
}

/// The end offset (byte index of the closing marker) of a `marker` pair that
/// opens at `i`, if there is one with non-blank content.
fn paired(bytes: &[u8], i: usize, marker: &str) -> Option<usize> {
    let m = marker.as_bytes();
    let open = i + m.len();
    let close = find_subslice(&bytes[open..], m)? + open;
    if close == open {
        return None;
    }
    let first = bytes[open] as char;
    let last = bytes[close - 1] as char;
    if first.is_whitespace() || last.is_whitespace() {
        return None;
    }
    Some(close)
}

fn push_wrapped(
    base: usize,
    i: usize,
    close: usize,
    marker: &str,
    style: Style,
    out: &mut Vec<StyleSpan>,
) {
    let mlen = marker.len();
    out.push(StyleSpan {
        start: base + i,
        end: base + i + mlen,
        style: Style::Punctuation,
    });
    out.push(StyleSpan {
        start: base + i + mlen,
        end: base + close,
        style,
    });
    out.push(StyleSpan {
        start: base + close,
        end: base + close + mlen,
        style: Style::Punctuation,
    });
}

/// `[text](url)` and `![alt](url)`, opening at the `!` or the `[`. Emits the
/// brackets dimmed, the text as a link and the `(url)` as a url, and returns
/// the absolute offset just past the construct.
fn md_link(base: usize, content: &str, i: usize, out: &mut Vec<StyleSpan>) -> Option<usize> {
    let bytes = content.as_bytes();
    let bracket = if bytes[i] == b'!' { i + 1 } else { i };
    let close = find_subslice(&bytes[bracket + 1..], b"]")? + bracket + 1;
    if bytes.get(close + 1) != Some(&b'(') {
        return None;
    }
    let url_open = close + 1;
    let url_close = find_subslice(&bytes[url_open + 1..], b")")? + url_open + 1;
    push_span(base, i, bracket + 1, Style::Punctuation, out);
    push_span(base, bracket + 1, close, Style::LinkText, out);
    push_span(base, close, close + 1, Style::Punctuation, out);
    push_span(base, url_open, url_close + 1, Style::LinkUrl, out);
    Some(base + url_close + 1)
}

/// `[[target|alias]]` and `![[target]]`, opening at the `!` or the first `[`.
/// The brackets are punctuation, the target of an aliased link is [`Style::LinkTarget`]
/// and the text a reader sees is [`Style::WikiLink`] / [`Style::WikiEmbed`], so
/// live preview can hide the syntax around the name. Returns the absolute offset
/// just past the construct.
fn wiki_link(base: usize, content: &str, i: usize, out: &mut Vec<StyleSpan>) -> Option<usize> {
    let bytes = content.as_bytes();
    let bang = bytes[i] == b'!';
    let open = if bang { i + 1 } else { i };
    if bytes.get(open + 1) != Some(&b'[') {
        return None;
    }
    let close = find_subslice(&bytes[open + 2..], b"]]")? + open + 2;
    // The brackets (and the `!` of an embed) are punctuation, so live preview
    // can hide them and show just the link text. With an alias the target and
    // the `|` are hidden too: `[[target|alias]]` reads as `alias`.
    push_span(base, i, open + 2, Style::Punctuation, out);
    let inner = open + 2;
    let shown = if bang {
        Style::WikiEmbed
    } else {
        Style::WikiLink
    };
    match find_subslice(&bytes[inner..close], b"|") {
        Some(bar) => {
            let bar = inner + bar;
            push_span(base, inner, bar + 1, Style::LinkTarget, out);
            push_span(base, bar + 1, close, shown, out);
        }
        None => push_span(base, inner, close, shown, out),
    }
    push_span(base, close, close + 2, Style::Punctuation, out);
    Some(base + close + 2)
}

/// Pushes a span given in line-relative offsets.
fn push_span(base: usize, start: usize, end: usize, style: Style, out: &mut Vec<StyleSpan>) {
    if end > start {
        out.push(StyleSpan {
            start: base + start,
            end: base + end,
            style,
        });
    }
}

fn is_tag_start(bytes: &[u8], i: usize) -> bool {
    if i > 0 {
        let prev = bytes[i - 1] as char;
        if prev.is_alphanumeric() || prev == '_' || prev == '#' || prev == '/' {
            return false;
        }
    }
    let Some(next) = bytes.get(i + 1) else {
        return false;
    };
    let next = *next as char;
    next.is_alphanumeric() || next == '_' || next == '/'
}

fn tag_end(bytes: &[u8], i: usize) -> usize {
    let mut end = i + 1;
    while end < bytes.len() {
        let c = bytes[end] as char;
        if c.is_alphanumeric() || matches!(c, '_' | '-' | '/') {
            end += next_char_len(bytes, end);
        } else {
            break;
        }
    }
    end
}

fn next_char_len(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    if b < 0xc0 {
        // ASCII, or a continuation byte, which cannot start a character: count
        // it as one so a scan never runs off the end of the string.
        1
    } else if b < 0xe0 {
        2
    } else if b < 0xf0 {
        3
    } else {
        4
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn opens_fence(trimmed: &[u8]) -> Option<(u8, usize)> {
    let fc = *trimmed.first()?;
    if fc != b'`' && fc != b'~' {
        return None;
    }
    let run = trimmed.iter().take_while(|&&b| b == fc).count();
    if run < 3 {
        return None;
    }
    Some((fc, run))
}

fn closes_fence(trimmed: &[u8], fc: u8, flen: usize) -> bool {
    if trimmed.first() != Some(&fc) {
        return false;
    }
    let run = trimmed.iter().take_while(|&&b| b == fc).count();
    run >= flen && trimmed[run..].iter().all(u8::is_ascii_whitespace)
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
