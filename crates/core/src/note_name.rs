//! Naming a note that does not have a name yet.
//!
//! A blank tab can be written into before it has a file, and the first thing the
//! user typed is the best guess at what it is called. This turns a line of text
//! — or a name typed at the `:` line — into a vault-relative markdown file name,
//! and refuses to invent anything unsafe: no path that leaves the vault, no
//! hidden file, no name that is only punctuation.
//!
//! The rule it encodes is the one the app writes by: a note's name is a phrase,
//! not an identifier. Spaces survive; the characters the filesystem forbids
//! become spaces, because `My Note v2.md` is what the user meant by
//! `My Note: v2`, and `my_note_v2.md` is what a machine would have guessed.

/// The longest name taken from a line. A note named after a 300-character
/// heading is a file name nobody can read in a tab, and a tab bar full of the
/// same 60 characters is worse.
pub const MAX_NAME: usize = 60;

/// The characters a name may not carry, and what they become: a space, so the
/// words around them stay words. `/` is handled separately, because whether it
/// is legal at all is the caller's question, not the character's.
const FORBIDDEN: &[char] = &['\\', ':', '*', '?', '"', '<', '>', '|'];

/// The markdown a heading is made of, so `# Ideas` names a note `Ideas` and not
/// `# Ideas`. Matched at the start of the line only: a `*` in the middle of a
/// sentence is emphasis, and a `-` in the middle of a word is a hyphen.
// Longest first: `###` must be tried before `#`, or the short marker matches
// the line, refuses to strip it (no space follows it), and stops the loop.
const BLOCK_MARKERS: &[&str] = &["######", "#####", "####", "###", "##", "#", ">", "-", "*", "+"];

/// Make `raw` into a vault-relative markdown name, or `None` when nothing
/// usable is left of it.
///
/// `folders` says whether `/` is kept. A name typed at the `:` line may place
/// the note (`areas/idea`), because typing a path is asking for one; a name
/// taken from a heading never should, because a heading that happens to hold a
/// slash is not a request to reorganise the vault.
pub fn sanitize(raw: &str, folders: bool) -> Option<String> {
    let mut text = raw.trim().to_string();
    text = strip_block_marker(&text);

    // Split into path segments when `/` is legal, so each one can be checked on
    // its own; when it is not, the whole name is one segment and `/` is just
    // another character the filesystem would refuse.
    // When `/` is not legal it is just another character the filesystem would
    // refuse, so it becomes a space before the name is taken apart.
    let text = if folders { text } else { text.replace('/', " ") };
    let segments: Vec<Option<String>> = if folders {
        text.split('/').map(clean_segment).collect()
    } else {
        vec![clean_segment(&text)]
    };

    // `.` and `..` are not names, and a name that starts with a dot is a hidden
    // file. Everything else that survives the cleaning is kept.
    let kept: Vec<String> = segments
        .into_iter()
        .flatten()
        .filter(|part| part != "." && part != "..")
        .collect();
    if kept.is_empty() {
        return None;
    }

    let joined = kept.join("/");
    let joined = truncate(&joined);
    if joined.is_empty() {
        return None;
    }

    // The extension is part of the name being a note, not of the name the user
    // typed: `idea` and `idea.md` are the same answer.
    if joined.to_ascii_lowercase().ends_with(".md") {
        Some(joined)
    } else {
        Some(format!("{joined}.md"))
    }
}

/// The first usable name in `text`: its first line that holds anything.
pub fn from_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| sanitize(line, false))
}

/// One path segment: forbidden characters become spaces, whitespace collapses,
/// and a leading or trailing dot is dropped so the name is never hidden.
fn clean_segment(part: &str) -> Option<String> {
    let replaced: String = part
        .chars()
        .map(|c| {
            if FORBIDDEN.contains(&c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();

    let mut out = String::new();
    for word in replaced.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }

    let out = out.trim_matches(|c| c == '.' || c == ' ').to_string();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Strip a leading markdown block marker (`# `, `- `, `> `, and a numbered
/// list's `1. `), so the name is the words and not the syntax.
fn strip_block_marker(text: &str) -> String {
    let mut rest = text.trim_start();
    loop {
        let before = rest;
        if let Some(head) = BLOCK_MARKERS.iter().find(|m| rest.starts_with(**m)) {
            let after = &rest[head.len()..];
            // `#` only counts as a marker when a space follows it: `#tag` is a
            // tag, and a note named `#tag` is a note about a tag.
            if after.starts_with(char::is_whitespace) {
                rest = after.trim_start();
            }
        } else if let Some((digits, tail)) = rest.split_once(['.', ')']) {
            // `1. Ideas` is a list item; `v2. Ideas` is not a list item.
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                rest = tail.trim_start();
            }
        }
        if rest == before {
            return rest.to_string();
        }
    }
}

/// Cut to [`MAX_NAME`] at a character boundary, then trim what the cut left
/// dangling at the end.
fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_NAME {
        return text.to_string();
    }
    let cut: String = text.chars().take(MAX_NAME).collect();
    cut.trim_end_matches([' ', '.', '/']).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heading_names_the_note_and_not_the_syntax() {
        assert_eq!(sanitize("# Ideas for the garden", false).as_deref(), Some("Ideas for the garden.md"));
        assert_eq!(sanitize("### - Buy milk", false).as_deref(), Some("Buy milk.md"));
        assert_eq!(sanitize("1. First thought", false).as_deref(), Some("First thought.md"));
    }

    #[test]
    fn a_tag_is_not_a_heading() {
        // Only `# ` is syntax. A line that is a tag keeps its `#` in the name:
        // stripping a character the user typed is not the same as stripping
        // markdown, and a note about a tag is allowed to look like one.
        assert_eq!(sanitize("#tag", false).as_deref(), Some("#tag.md"));
    }

    #[test]
    fn characters_the_filesystem_refuses_become_spaces() {
        assert_eq!(sanitize("My Note: v2 <draft>", false).as_deref(), Some("My Note v2 draft.md"));
        assert_eq!(sanitize("a/b/c", false).as_deref(), Some("a b c.md"));
    }

    #[test]
    fn a_name_typed_at_the_command_line_may_place_the_note() {
        assert_eq!(sanitize("areas/idea", true).as_deref(), Some("areas/idea.md"));
        // A heading never may: the slash is a character there, not a request.
        assert_eq!(sanitize("areas/idea", false).as_deref(), Some("areas idea.md"));
    }

    #[test]
    fn a_name_cannot_leave_the_vault() {
        assert_eq!(sanitize("../../etc/passwd", true).as_deref(), Some("etc/passwd.md"));
        assert_eq!(sanitize("..", true), None);
        assert_eq!(sanitize("./.", true), None);
    }

    #[test]
    fn a_name_is_never_hidden() {
        assert_eq!(sanitize(".hidden", false).as_deref(), Some("hidden.md"));
    }

    #[test]
    fn nothing_usable_is_no_name() {
        assert_eq!(sanitize("   ", false), None);
        assert_eq!(sanitize("***", false), None);
        assert_eq!(sanitize(":*?", false), None);
    }

    #[test]
    fn the_extension_is_not_doubled() {
        assert_eq!(sanitize("idea.md", false).as_deref(), Some("idea.md"));
        assert_eq!(sanitize("idea.MD", false).as_deref(), Some("idea.MD"));
    }

    #[test]
    fn a_long_line_is_cut_at_a_word_not_a_character() {
        let long = "word ".repeat(40);
        let name = sanitize(&long, false).expect("a name");
        assert!(name.chars().count() <= MAX_NAME + 3, "the name is bounded: {name}");
        assert!(!name.ends_with(' '), "the cut does not leave a dangling space");
    }

    #[test]
    fn the_first_line_with_something_in_it_is_the_name() {
        assert_eq!(from_line("\n\n# Shopping\nmilk\n").as_deref(), Some("Shopping.md"));
        assert_eq!(from_line(""), None);
    }
}
