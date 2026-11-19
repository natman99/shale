//! The vocabulary lint (vocabulary-plan.md M2). The sweep is worth nothing if a
//! drifted word can walk back in one label at a time, so the words themselves are a
//! test: one word per thing, a length the palette column can hold, and a renamed row
//! still findable by the word a reader would type.

use super::*;
use crate::command::Command;

/// A generated doc, read the way the docs-parity tests read it: the checked-in text,
/// CRLF-normalised. Those tests already pin the file to the table that generates it, so
/// scanning the file is scanning the table — and the failure names a line a reader sees.
fn doc(name: &str) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/");
    std::fs::read_to_string(format!("{path}{name}"))
        .unwrap_or_default()
        .replace("\r\n", "\n")
}

/// Words that used to name the same thing as another word. `cursor` for the caret,
/// `proposal` for a hunk, `Write …` for a save: each pair was in the app at once, and
/// the failure mode is one of them reappearing in a new row rather than the set
/// drifting all at once.
const BANNED: &[&str] = &[
    "cursor",
    "proposal",
    "Write the note",
    "Write every",
    "writing its note",
    "saving its notes",
];

/// `strip` is banned as a name for the right-hand panel, which is the sidebar. The
/// graph's filter strip is a different widget with a different name, so the exception
/// is spelled out here rather than the rule being loosened to fit it.
const PANEL_AS_STRIP: &str = "strip";

/// The words that are allowed to appear only as part of a larger, different name.
const EXCUSED: &[(&str, &str)] = &[("strip", "filter strip")];

/// Every word the app shows a reader, in the two generated surfaces: the command list
/// and the keymap. The parity tests already pin those files to the tables that generate
/// them, so scanning the files also holds the generators' own prose to the rule, and a
/// failure names a line a reader actually sees.
/// The words in one line of text, with the excused spellings removed first.
fn hits(text: &str) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = BANNED
        .iter()
        .copied()
        .filter(|w| text.contains(w))
        .collect();
    let scrubbed = EXCUSED
        .iter()
        .fold(text.to_string(), |acc, (_, excuse)| acc.replace(excuse, ""));
    if scrubbed.contains(PANEL_AS_STRIP) {
        found.push(PANEL_AS_STRIP);
    }
    found
}

#[test]
fn the_reader_sees_one_word_per_thing() {
    let mut offenders: Vec<String> = Vec::new();
    // The tables first, so a drifted word is red before anyone regenerates a doc.
    for command in Command::every() {
        for (what, text) in [("label", command.label()), ("keys", command.keys())] {
            let found = hits(text);
            if !found.is_empty() {
                offenders.push(format!("{} {what}: {found:?} — {text}", command.id()));
            }
        }
    }
    // The generated surfaces, where the generators' prose and the keymap's
    // what-it-does column live.
    for surface in ["commands.md", "keymap.md"] {
        let text = doc(surface);
        for (n, line) in text.lines().enumerate() {
            let found = hits(line);
            if !found.is_empty() {
                offenders.push(format!("docs/{surface}:{}: {found:?} — {line}", n + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "a word the vocabulary has already settled reappears:\n{}",
        offenders.join("\n")
    );
}

/// The palette is a narrow column and the longest row sets the width every row is
/// measured against. The set currently fits in 40 characters; a row that needs more
/// has to argue for it by making the set longer, not by being written long.
#[test]
fn a_palette_row_fits_the_column() {
    let too_long: Vec<String> = Command::all()
        .filter(|command| command.label().len() > 40)
        .map(|command| {
            format!(
                "{}: {} ({} chars)",
                command.id(),
                command.label(),
                command.label().len()
            )
        })
        .collect();
    assert!(
        too_long.is_empty(),
        "palette rows over the column budget:\n{}",
        too_long.join("\n")
    );
}

/// A rename that drops the noun leaves a row nobody can find: the palette matches a
/// subsequence of the label, so the word a reader types has to survive the rename.
/// This is the assertion that caught `Preview the note…` becoming unreachable when it
/// became `Show the note read-only` — the palette test that typed `preview` did not
/// fail, it quietly started running a different command.
#[gpui::test]
fn a_renamed_row_is_still_found_by_the_word_people_type(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "one line\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    for (query, want) in [
        ("save", "Save the note"),
        ("dirty", "Save every dirty note"),
        ("saving", "Close this tab, saving first"),
        ("pin", "Pin or unpin this tab"),
        ("source", "Show the note as source"),
        ("live", "Show the note as live preview"),
        ("read", "Show the note as read-only preview"),
        ("graph", "Graph: the whole vault"),
        ("3d", "Graph: 3D"),
        ("flat", "Graph: flat"),
        ("lens", "Choose a lens for the graph"),
        ("caret", "Centre the view on the caret"),
        ("dim", "Dim everything but this block"),
        ("sidebar", "Show or hide the sidebar"),
        ("hunk", "Review the pending hunks"),
        ("clipboard", "Copy to clipboard"),
        ("frontmatter", "Add frontmatter properties to the note"),
        ("props", "Show this note's properties"),
        ("checker", "Show the checker's note for this word"),
    ] {
        cx.simulate_keystrokes("ctrl-shift-p");
        cx.simulate_input(query);
        let offered = window
            .update(cx, |ws, _window, cx| ws.picker_rows(cx))
            .unwrap();
        assert!(
            offered.iter().any(|row| row == want),
            "`{query}` does not offer {want:?}: {offered:?}"
        );
        cx.simulate_keystrokes("escape");
    }
}
