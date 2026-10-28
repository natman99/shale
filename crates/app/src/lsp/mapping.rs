//! Translating an LSP range into the buffer's byte offsets.
//!
//! The protocol counts positions in **UTF-16 code units** per line, while Shale's
//! [`shale_core::Buffer`] counts **bytes**. Neither is wrong and they are not the
//! same thing: an accented letter is one unit and two bytes, an emoji outside the
//! basic plane is two units and four. Every diagnostic that lands on a line with
//! either in front of it is misplaced if the two are treated as interchangeable.

use serde_json::Value;

/// One note from the checker, in the buffer's coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    /// Half-open byte range of the text the server is complaining about.
    pub start: usize,
    /// Byte just past [`Self::start`]'s text.
    pub end: usize,
    /// What the server thinks is wrong, in its words rather than ours.
    pub message: String,
    /// The server's own code for the kind of problem, such as `SpellCheck`.
    pub code: String,
}

/// The byte offset of an LSP `line`/`character` position in `text`, or `None` if
/// there is no such line. A character past the end of a line clamps to the end of
/// that line, which is what keeps a range that runs off the edge honest instead of
/// wrapping it into the next one.
pub(crate) fn offset_of(text: &str, line: usize, character: usize) -> Option<usize> {
    let line_start = byte_start_of_line(text, line)?;
    let line_text = text[line_start..].split('\n').next()?;
    let mut units = 0usize;
    let mut byte = line_start;
    for ch in line_text.chars() {
        if units >= character {
            return Some(byte);
        }
        units += ch.len_utf16();
        byte += ch.len_utf8();
    }
    Some(byte)
}

/// Where line `line` (0-based) starts in bytes.
fn byte_start_of_line(text: &str, line: usize) -> Option<usize> {
    if line == 0 {
        return Some(0);
    }
    let mut seen = 0usize;
    for (index, ch) in text.char_indices() {
        if ch == '\n' {
            seen += 1;
            if seen == line {
                return Some(index + 1);
            }
        }
    }
    None
}

/// The buffer range an LSP range covers, as a half-open byte range.
///
/// A range that ends before it starts — which a server can produce by clamping —
/// is dropped rather than drawn backwards.
pub(crate) fn range(
    text: &str,
    start: (usize, usize),
    end: (usize, usize),
) -> Option<(usize, usize)> {
    let start = offset_of(text, start.0, start.1)?;
    let end = offset_of(text, end.0, end.1)?;
    (start < end).then_some((start, end))
}

/// The diagnostics of one `textDocument/publishDiagnostics` notification, placed
/// in `text`. A diagnostic whose range cannot be placed is skipped: a squiggle in
/// the wrong place is worse than no squiggle.
pub(crate) fn diagnostics(params: &Value, text: &str) -> Vec<Diagnostic> {
    let Some(list) = params.get("diagnostics").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(list.len());
    for entry in list {
        let Some(start) = position(entry, "start") else {
            continue;
        };
        let Some(end) = position(entry, "end") else {
            continue;
        };
        let Some((start, end)) = range(text, start, end) else {
            continue;
        };
        out.push(Diagnostic {
            start,
            end,
            message: entry
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            // `code` is a string for Harper and a number for other servers, and
            // either is worth keeping: it is what tells a spelling note apart
            // from a grammar one.
            code: match entry.get("code") {
                Some(Value::String(code)) => code.clone(),
                Some(Value::Number(code)) => code.to_string(),
                _ => String::new(),
            },
        });
    }
    out
}

/// One corner of an LSP range, as (line, character).
fn position(entry: &Value, corner: &str) -> Option<(usize, usize)> {
    let point = entry.get("range")?.get(corner)?;
    Some((
        point.get("line")?.as_u64()? as usize,
        point.get("character")?.as_u64()? as usize,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ascii_range_lands_where_it_reads() {
        let text = "This are a mistake\nsecond line\n";
        // `mistake` on the first line, at units 11..18.
        let (start, end) = range(text, (0, 11), (0, 18)).unwrap();
        assert_eq!(&text[start..end], "mistake");
    }

    /// The character count is UTF-16, so everything in front of the range has to
    /// be counted in code units, not bytes and not characters.
    #[test]
    fn positions_count_utf16_units_not_bytes_or_characters() {
        let text = "café 🎩 naïve\n";
        // Units: c a f é(1) space 🎩(2) space n … so `naïve` starts at unit 8.
        let (start, end) = range(text, (0, 8), (0, 13)).unwrap();
        assert_eq!(&text[start..end], "naïve");

        // The accented e is one unit but two bytes, so unit 4 — the space after
        // it — sits at byte 5, not byte 4.
        let (start, _) = range(text, (0, 4), (0, 5)).unwrap();
        assert_eq!(start, "café".len(), "the space after the accented e");
    }

    #[test]
    fn a_range_on_a_later_line_is_counted_from_that_line() {
        let text = "first\nsecond naïve line\nthird\n";
        let (start, end) = range(text, (1, 7), (1, 12)).unwrap();
        assert_eq!(&text[start..end], "naïve");
    }

    /// A position past the end of a line clamps to the line's end rather than
    /// running into the next line's text.
    #[test]
    fn a_position_past_the_line_end_clamps() {
        let text = "short\nnext line\n";
        let (start, end) = range(text, (0, 2), (0, 99)).unwrap();
        assert_eq!(
            &text[start..end],
            "ort",
            "clamped at the newline, not past it"
        );
    }

    #[test]
    fn a_line_that_does_not_exist_has_no_offset() {
        assert_eq!(offset_of("one line\n", 5, 0), None);
    }

    /// A backwards range is a server's clamping artefact, not something to draw.
    #[test]
    fn a_backwards_range_is_dropped() {
        assert_eq!(range("abcdef", (0, 4), (0, 1)), None);
        assert_eq!(range("abcdef", (0, 2), (0, 2)), None);
    }

    #[test]
    fn published_diagnostics_are_placed_or_skipped() {
        let text = "thier spelling is wrong\n";
        let params = serde_json::json!({
            "uri": "file:///a.md",
            "diagnostics": [
                {
                    "message": "Did you mean to spell `thier` this way?",
                    "severity": 4,
                    "code": "SpellCheck",
                    "range": { "start": { "line": 0, "character": 0 },
                               "end": { "line": 0, "character": 5 } }
                },
                {
                    "message": "off the end of the note",
                    "range": { "start": { "line": 9, "character": 0 },
                               "end": { "line": 9, "character": 4 } }
                }
            ]
        });
        let found = diagnostics(&params, text);
        assert_eq!(found.len(), 1, "the unplaceable one is skipped");
        assert_eq!(found[0].code, "SpellCheck");
        assert_eq!(&text[found[0].start..found[0].end], "thier");
        assert!(found[0].message.contains("thier"));
    }

    #[test]
    fn a_payload_without_diagnostics_is_not_an_error() {
        assert!(diagnostics(&serde_json::json!({ "uri": "file:///a.md" }), "text").is_empty());
    }
}
