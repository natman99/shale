//! Translating an LSP range into the buffer's byte offsets.
//!
//! The protocol counts positions in **UTF-16 code units** per line, while Shale's
//! [`shale_core::Buffer`] counts **bytes**. Neither is wrong and they are not the
//! same thing: an accented letter is one unit and two bytes, an emoji outside the
//! basic plane is two units and four. Every diagnostic that lands on a line with
//! either in front of it is misplaced if the two are treated as interchangeable.

use serde_json::{json, Value};

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

/// The buffer byte offset `byte` as an LSP (line, character) position — the
/// inverse of [`offset_of`], and with the same UTF-16 rule on the way out. A byte
/// that lands inside a character (which the buffer never hands over, but a bug
/// elsewhere might) is snapped back to the start of that character rather than
/// panicking on the slice.
pub(crate) fn position_of(text: &str, byte: usize) -> (usize, usize) {
    let mut byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    let line = text[..byte].matches('\n').count();
    let line_start = byte_start_of_line(text, line).unwrap_or(0);
    let character = text[line_start..byte]
        .chars()
        .map(|ch| ch.len_utf16())
        .sum();
    (line, character)
}

/// The LSP range covering the buffer bytes `start..end`.
pub(crate) fn lsp_range(text: &str, start: usize, end: usize) -> Value {
    let (line, character) = position_of(text, start);
    let (end_line, end_character) = position_of(text, end);
    json!({
        "start": { "line": line, "character": character },
        "end": { "line": end_line, "character": end_character }
    })
}

/// One thing the checker offers to do about a diagnostic.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fix {
    /// The row's text, in the server's words: `Replace with: “their”`,
    /// `Add "Thier" to the user dictionary.`.
    pub title: String,
    /// (start, end, replacement) as **buffer byte offsets**. Empty for a fix that
    /// is only a command — adding a word to a dictionary changes no text.
    pub edits: Vec<(usize, usize, String)>,
    /// The server command to run, with its arguments **verbatim**. Harper builds
    /// those arguments itself (a word, the document URI, a whole lint as JSON),
    /// and re-deriving them here would be a second place to get them wrong.
    /// A code action's own command runs *after* its edits, which is what the
    /// protocol asks of a client.
    pub command: Option<(String, Vec<Value>)>,
}

/// The command of a code-action item, in either shape: a `CodeAction` carries it
/// as an object with the arguments one level deeper, and a bare `Command` *is* the
/// command, with `command` holding its name.
fn command_of(item: &Value) -> Option<(String, Vec<Value>)> {
    let args = |holder: &Value| {
        holder
            .get("arguments")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    match item.get("command")? {
        Value::String(name) => Some((name.clone(), args(item))),
        obj @ Value::Object(_) => Some((obj.get("command")?.as_str()?.to_string(), args(obj))),
        _ => None,
    }
}

/// The fixes of one `textDocument/codeAction` response, placed in `text`.
///
/// The response is a mixed bag: `CodeAction`s (an `edit` plus an optional
/// `command`) and bare `Command`s (a `command` and nothing else), and the two
/// spell their arguments at different depths. Edits that belong to a different
/// document than `uri` are dropped — this client has one document open, and
/// silently rewriting another note behind the user is not on the table.
pub(crate) fn fixes(response: &Value, text: &str, uri: &str) -> Vec<Fix> {
    let Some(items) = response.get("result").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let command = command_of(item);
        let title = item
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();
        let mut edits = Vec::new();
        if let Some(changes) = item
            .get("edit")
            .and_then(|edit| edit.get("changes"))
            .and_then(Value::as_object)
        {
            for (change_uri, list) in changes {
                if change_uri != uri {
                    tracing::debug!(
                        uri = change_uri.as_str(),
                        "a fix wants to edit another document, so its edits are left out"
                    );
                    continue;
                }
                let Some(list) = list.as_array() else {
                    continue;
                };
                for edit in list {
                    let Some(start) = position(edit, "start") else {
                        continue;
                    };
                    let Some(end) = position(edit, "end") else {
                        continue;
                    };
                    let Some((start, end)) = range(text, start, end) else {
                        continue;
                    };
                    edits.push((
                        start,
                        end,
                        edit.get("newText")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    ));
                }
            }
        }
        if title.is_empty() || (edits.is_empty() && command.is_none()) {
            // Nothing to show and nothing to do: not a fix we can offer.
            continue;
        }
        out.push(Fix {
            title,
            edits,
            command,
        });
    }
    out
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

    /// The byte-for-LSP-position translation, which is what a `codeAction` request
    /// needs. It has to agree with [`offset_of`] in both directions, including
    /// across an emoji, which is two units and four bytes.
    #[test]
    fn positions_round_trip_through_bytes() {
        let text = "café 🎩 naïve\nsecond\n";
        for byte in [0usize, 1, 5, 6, 10, 11, 17, 18, 25] {
            let (line, character) = position_of(text, byte);
            let back = offset_of(text, line, character).unwrap();
            assert_eq!(back, byte, "byte {byte} should come back to itself");
        }
        // The second line counts from its own start, in units.
        assert_eq!(position_of(text, text.find("second").unwrap()), (1, 0));
        assert_eq!(position_of("aé🎩b", 1), (0, 1));
        assert_eq!(position_of("aé🎩b", 3), (0, 2), "the emoji is one unit");
        assert_eq!(
            position_of("aé🎩b", 5),
            (0, 2),
            "a byte inside a character is snapped back to its start, not a panic"
        );
    }

    /// A `CodeAction` from Harper: the edit is the fix, and the command that comes
    /// with it (recording a statistic) travels with its arguments untouched.
    #[test]
    fn a_quickfix_carries_its_edit_and_its_command() {
        let text = "Thier spelling\n";
        let response = serde_json::json!({
            "result": [{
                "title": "Replace with: “Their”",
                "kind": "quickfix",
                "edit": { "changes": { "file:///a.md": [{
                    "range": { "start": { "line": 0, "character": 0 },
                               "end": { "line": 0, "character": 5 } },
                    "newText": "Their" }]} },
                "command": { "title": "Record lint statistic",
                             "command": "HarperRecordLint",
                             "arguments": ["{\"Lint\":{}}"] }
            }]
        });
        let found = fixes(&response, text, "file:///a.md");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Replace with: “Their”");
        assert_eq!(
            found[0].edits,
            vec![(0, 5, "Their".to_string())],
            "the edit lands on the misspelled word"
        );
        let (name, args) = found[0].command.clone().unwrap();
        assert_eq!(name, "HarperRecordLint");
        assert_eq!(args, vec![serde_json::json!("{\"Lint\":{}}")]);
    }

    /// The bare `Command` entries — ignore this lint, add the word to a
    /// dictionary — offer no text edit, and their second argument is a whole JSON
    /// value rather than a string.
    #[test]
    fn a_bare_command_is_a_fix_with_no_edit() {
        let text = "Thier spelling\n";
        let response = serde_json::json!({
            "result": [
                { "title": "Ignore Harper error.", "command": "HarperIgnoreLint",
                  "arguments": ["file:///a.md", { "lint_kind": "Spelling", "span": { "start": 0, "end": 5 } }] },
                { "title": "Add \"Thier\" to the user dictionary.", "command": "HarperAddToUserDict",
                  "arguments": ["Thier", "file:///a.md"] }
            ]
        });
        let found = fixes(&response, text, "file:///a.md");
        assert_eq!(found.len(), 2);
        assert!(found[0].edits.is_empty(), "ignoring changes no text");
        let (name, args) = found[0].command.clone().unwrap();
        assert_eq!(name, "HarperIgnoreLint");
        assert_eq!(args[0], serde_json::json!("file:///a.md"));
        assert_eq!(
            args[1],
            serde_json::json!({ "lint_kind": "Spelling", "span": { "start": 0, "end": 5 } }),
            "the lint value is passed through, not rebuilt"
        );
        assert_eq!(found[1].command.as_ref().unwrap().0, "HarperAddToUserDict");
    }

    /// A fix that edits a document other than the open one is offered without its
    /// edits: this client has one document, and writing into another note behind
    /// the user is not something a spelling popup should do.
    #[test]
    fn an_edit_to_another_document_is_left_out() {
        let text = "Thier spelling\n";
        let response = serde_json::json!({
            "result": [{
                "title": "Fix it everywhere",
                "edit": { "changes": { "file:///other.md": [{
                    "range": { "start": { "line": 0, "character": 0 },
                               "end": { "line": 0, "character": 5 } },
                    "newText": "Their" }] } }
            }, {
                "title": "Add \"Thier\" to the workspace dictionary.",
                "command": "HarperAddToWSDict", "arguments": ["Thier", "file:///a.md"]
            }]
        });
        let found = fixes(&response, text, "file:///a.md");
        assert_eq!(
            found.len(),
            1,
            "the edit-only fix with nothing left to do is dropped"
        );
        assert_eq!(found[0].title, "Add \"Thier\" to the workspace dictionary.");
    }
}
