//! What Enter does on a row.
//!
//! Obsidian carries a row's marker onto the row that Enter starts: a bullet, a
//! numbered item, a task or a `>` quote keeps going, and pressing Enter on a row
//! that holds *nothing but* its marker leaves the construct instead of starting a
//! second empty item. This module answers that question for a byte offset; the UI
//! applies it in insert mode, so the marker text lands in the same undo group as
//! the typing around it and `.` replays it.
//!
//! The marker shapes are the ones the style scanner already recognises
//! ([`crate::mdstyle`]), so live preview hides exactly the markers this continues.

use crate::mdstyle::{list_marker_end, task_checkbox};

/// What pressing Enter on the row that holds an offset should do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnterAction {
    /// Insert this text at the cursor: a newline, then the marker the new row
    /// should open with.
    Continue { text: String },
    /// The row holds nothing but its marker, so delete `start..end` (buffer byte
    /// offsets: the marker and its indent) and leave the row blank.
    Clear { start: usize, end: usize },
    /// An ordinary newline: the row opens no marker worth continuing.
    Newline,
}

/// Decide what Enter does on the row containing `offset`.
///
/// `in_code` says whether that row is inside a fenced block, where `-`, `>` and
/// `1.` are literal text and must not be continued.
pub fn enter_action(text: &str, offset: usize, in_code: bool) -> EnterAction {
    if in_code {
        return EnterAction::Newline;
    }
    let offset = offset.min(text.len());
    let row_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let row_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let row = &text[row_start..row_end];
    let Some(marker) = row_marker(row) else {
        return EnterAction::Newline;
    };
    if row[marker.end..].trim().is_empty() {
        return EnterAction::Clear {
            start: row_start,
            end: row_start + marker.end,
        };
    }
    EnterAction::Continue {
        text: format!("\n{}", marker.next),
    }
}

/// A row's opening marker.
struct RowMarker {
    /// Byte offset in the row where its marker ends, including the whitespace
    /// after it and a task's checkbox. Everything from here on is the row's
    /// content, so an empty remainder means the row is only its marker.
    end: usize,
    /// The text the next row should open with, indent included.
    next: String,
}

/// The marker `row` opens with, if it has one: blockquote markers, then a bullet,
/// a numbered item or a task.
fn row_marker(row: &str) -> Option<RowMarker> {
    // The indent is kept, so a nested item continues at the same depth.
    let indent_len = first_non_indent(row).unwrap_or(row.len());
    let indent = &row[..indent_len];
    let mut pos = indent_len;

    // Blockquote markers, each with its optional single space — the same shape
    // the style scanner reads. They nest, and a list marker may follow them.
    let mut quotes = String::new();
    while row[pos..].starts_with('>') {
        pos += 1;
        if row[pos..].starts_with(' ') {
            pos += 1;
        }
        quotes.push_str("> ");
    }

    let rest = &row[pos..];
    let Some(marker_end) = list_marker_end(rest) else {
        if quotes.is_empty() {
            return None;
        }
        // A quote with no list inside it: the next row keeps the quote markers.
        return Some(RowMarker {
            end: skip_indent(row, pos),
            next: format!("{indent}{quotes}"),
        });
    };
    let marker = &rest[..marker_end];
    let bullet = marker.chars().next()?;
    let checkbox = task_checkbox(&rest[marker_end..]);
    let end = skip_indent(row, pos + marker_end + checkbox.map_or(0, |c| c.len()));

    // What the next row opens with. A task always starts unchecked, whatever the
    // row being continued held; a numbered item counts up (Obsidian's behaviour),
    // keeping `01.`-style padding.
    let item = if checkbox.is_some() {
        format!("{bullet} [ ] ")
    } else if bullet.is_ascii_digit() {
        let digits: String = marker.chars().take_while(|c| c.is_ascii_digit()).collect();
        let close = marker[digits.len()..].chars().next()?;
        let next = digits.parse::<u64>().ok()?.checked_add(1)?;
        format!("{next:0width$}{close} ", width = digits.len())
    } else {
        format!("{bullet} ")
    };
    Some(RowMarker {
        end,
        next: format!("{indent}{quotes}{item}"),
    })
}

/// The byte offset of the first character that is not a space or a tab.
fn first_non_indent(row: &str) -> Option<usize> {
    row.char_indices()
        .find(|(_, c)| *c != ' ' && *c != '\t')
        .map(|(i, _)| i)
}

/// `from` advanced past any spaces and tabs.
fn skip_indent(row: &str, from: usize) -> usize {
    first_non_indent(&row[from..]).map_or(row.len(), |i| from + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text Enter would insert, or `None` for an ordinary newline.
    fn continued(text: &str, offset: usize) -> Option<String> {
        match enter_action(text, offset, false) {
            EnterAction::Continue { text } => Some(text),
            _ => None,
        }
    }

    #[test]
    fn a_bullet_continues_with_the_same_bullet() {
        assert_eq!(continued("- one", 5).as_deref(), Some("\n- "));
        assert_eq!(continued("* one", 5).as_deref(), Some("\n* "));
        assert_eq!(continued("+ one", 5).as_deref(), Some("\n+ "));
        // The bullet, not the content, decides: Enter mid-row still continues it.
        assert_eq!(continued("- one", 2).as_deref(), Some("\n- "));
    }

    #[test]
    fn the_indent_is_kept() {
        assert_eq!(continued("  - one", 7).as_deref(), Some("\n  - "));
        assert_eq!(continued("\t- one", 7).as_deref(), Some("\n\t- "));
    }

    #[test]
    fn a_numbered_item_counts_up() {
        assert_eq!(continued("1. one", 6).as_deref(), Some("\n2. "));
        assert_eq!(continued("9. one", 6).as_deref(), Some("\n10. "));
        assert_eq!(continued("08. one", 7).as_deref(), Some("\n09. "));
        assert_eq!(continued("3) one", 6).as_deref(), Some("\n4) "));
    }

    #[test]
    fn a_task_always_continues_empty() {
        assert_eq!(continued("- [ ] todo", 10).as_deref(), Some("\n- [ ] "));
        assert_eq!(continued("- [x] done", 10).as_deref(), Some("\n- [ ] "));
        assert_eq!(continued("* [x] done", 10).as_deref(), Some("\n* [ ] "));
    }

    #[test]
    fn a_quote_continues_and_can_hold_a_list() {
        assert_eq!(continued("> quoted", 8).as_deref(), Some("\n> "));
        assert_eq!(continued("> > deep", 8).as_deref(), Some("\n> > "));
        assert_eq!(continued("> - item", 8).as_deref(), Some("\n> - "));
        assert_eq!(
            continued("> [!note] A callout", 17).as_deref(),
            Some("\n> "),
            "a callout header is a quote line, so the box keeps going"
        );
    }

    #[test]
    fn a_marker_only_row_is_cleared_instead_of_doubled() {
        let text = "- one\n- ";
        assert_eq!(
            enter_action(text, text.len(), false),
            EnterAction::Clear {
                start: 6,
                end: text.len()
            },
            "the second Enter removes the empty item"
        );
        let text = "- [ ]";
        assert_eq!(
            enter_action(text, text.len(), false),
            EnterAction::Clear { start: 0, end: 5 },
            "a task's checkbox is part of its marker"
        );
        let text = "> ";
        assert_eq!(
            enter_action(text, 2, false),
            EnterAction::Clear { start: 0, end: 2 }
        );
        let text = "  1. ";
        assert_eq!(
            enter_action(text, 5, false),
            EnterAction::Clear { start: 0, end: 5 },
            "the indent goes too, leaving a blank row"
        );
    }

    #[test]
    fn a_row_with_content_is_not_cleared() {
        let text = "- one";
        assert_eq!(
            enter_action(text, 5, false),
            EnterAction::Continue {
                text: "\n- ".to_string()
            }
        );
    }

    #[test]
    fn ordinary_rows_get_an_ordinary_newline() {
        for text in [
            "plain prose",
            "# Heading",
            "---",
            "| a | b |",
            "1not a list",
            "-not a list",
            "",
        ] {
            assert_eq!(
                enter_action(text, text.len(), false),
                EnterAction::Newline,
                "`{text}`"
            );
        }
    }

    #[test]
    fn markers_inside_a_fenced_block_are_literal() {
        let text = "```\n- one\n```";
        assert_eq!(
            enter_action(text, 8, true),
            EnterAction::Newline,
            "the UI says the row is code, so nothing is continued"
        );
    }

    #[test]
    fn the_row_is_found_around_the_cursor() {
        let text = "top\n- one\nbottom";
        let cursor = text.find("one").unwrap() + 1;
        assert_eq!(
            enter_action(text, cursor, false),
            EnterAction::Continue {
                text: "\n- ".to_string()
            }
        );
    }
}
