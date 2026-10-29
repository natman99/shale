//! The block-level part of the markdown scan: what one line *is* (a heading, a
//! quote, a callout, a list item, a fence, a table row) and the spans that
//! follows from it. Inline styling of the text inside those constructs is done
//! by `super::inline`.

use super::{inline::inline_spans, is_table_delimiter_row, CalloutKind, Style, StyleSpan};

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

/// The spans for one line: first the block-level facts (a fence, a heading, a
/// rule, a table row, a quote or callout, a list item), then the inline spans
/// inside whatever text is left.
///
/// `fence` and `callout` carry the block state from the previous line, since
/// both run across lines.
pub(super) fn scan_line(
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

/// The `[!type]` header that opens a callout, as a byte range of `content`: from
/// the `[` through the `]` and Obsidian's optional `-`/`+` fold marker.
///
/// `content` is a whole line, so the walk starts at its first byte — which is
/// also where [`scan_line`] starts it, so the two agree that an indented `>` is
/// ordinary quote text rather than a callout.
pub(super) fn callout_header_range(content: &str) -> Option<(usize, usize)> {
    let start = quote_content_start(content);
    // `callout_header` counts from the `[!`, so its ends are relative to `start`.
    let (_, _, header_end, _) = callout_header(&content[start..])?;
    Some((start, start + header_end))
}

/// Where a blockquote line's content begins: past its `>` markers and the single
/// optional space after each. The same walk [`scan_line`] does while it draws
/// those markers; byte indexing is enough because `>` and a space are one byte
/// each.
fn quote_content_start(content: &str) -> usize {
    let bytes = content.as_bytes();
    let mut i = 0;
    while bytes.get(i) == Some(&b'>') {
        i += 1;
        if bytes.get(i) == Some(&b' ') {
            i += 1;
        }
    }
    i
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
/// The end of a list marker in `content` (its indent, the `-`/`*`/`+` or the
/// number and its closer, and the whitespace after it), as a byte offset in
/// `content`. `pub(crate)` because [`crate::continuation`] continues the marker
/// the scanner recognises, so the two must agree on what a marker is.
pub(crate) fn list_marker_end(content: &str) -> Option<usize> {
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

/// `[ ]` or `[x]` at the start of a list item's content. `pub(crate)` for the
/// same reason as [`list_marker_end`].
pub(crate) fn task_checkbox(rest: &str) -> Option<&str> {
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
