//! The inline part of the markdown scan: emphasis, code spans, links,
//! wiki-links and tags inside a line that `super::block` has already classified.
//! Every span is an absolute byte offset on a character boundary.

use super::{Style, StyleSpan};

/// Inline spans inside `content`, whose text starts at absolute `base`.
pub(super) fn inline_spans(base: usize, content: &str, out: &mut Vec<StyleSpan>) {
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
///
/// Also what the `L` text object measures a link's extent with, so it is visible
/// to the rest of the module.
pub(super) fn md_link(
    base: usize,
    content: &str,
    i: usize,
    out: &mut Vec<StyleSpan>,
) -> Option<usize> {
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
/// just past the construct. As with [`md_link`], the `L` text object reuses this
/// to measure a link rather than restyle it.
pub(super) fn wiki_link(
    base: usize,
    content: &str,
    i: usize,
    out: &mut Vec<StyleSpan>,
) -> Option<usize> {
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

/// The length in bytes of the character at `i`, so a walk can step over it.
pub(super) fn next_char_len(bytes: &[u8], i: usize) -> usize {
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

/// The first index of `needle` in `haystack`, as `str::find` for byte slices.
pub(super) fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
