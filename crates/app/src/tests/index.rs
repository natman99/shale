//! The vault index inside the window: what `:wordcount` reports, and how the index
//! keeps up with notes that move under it.
//!
//! These are the first tests that read the index rather than the notes, which makes
//! them the proof that the spine works end to end: the window opens a DB, the
//! startup pass fills it, the watcher and the save paths keep it current, and a
//! command reads a number out of it. The rows themselves — tags, links, events —
//! are `shale_core::db`'s own tests to make.
//!
//! Every other test helper turns the index off, so these open it deliberately with
//! [`indexed_vault`].

use super::*;

/// Type a command line and press enter.
fn type_command(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// The status line, where the count is reported.
fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// Six words, and three more in a folder, so a scoped count differs from the whole.
const WELCOME: &str = "# Welcome\n\nA note about the plan.\n";
const IDEA: &str = "three words here\n";

/// The startup pass reads the vault on a background thread, so a window that has an
/// index has to be waited for before it is asked what the index says.
#[gpui::test]
fn the_command_line_counts_the_vault_from_the_index(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME), ("notes/Idea.md", IDEA)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("wordcount", cx);
    assert_eq!(
        status(&window, cx),
        "9 words in 2 notes",
        "the count comes from the index the startup pass filled"
    );
}

/// `:wordcount notes` answers for one folder. The folder is a prefix of a note's
/// path, not a substring of it, so `notes/` and `notesx/` are different answers.
#[gpui::test]
fn the_count_scopes_to_one_folder(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Welcome.md", WELCOME),
            ("notes/Idea.md", IDEA),
            ("notesx/Elsewhere.md", IDEA),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("wordcount notes", cx);
    assert_eq!(status(&window, cx), "3 words in 1 note under notes/");
    type_command("wc", cx);
    assert_eq!(
        status(&window, cx),
        "12 words in 3 notes",
        "`wc` is the same command under the shorter name, with no folder"
    );
}

/// A note another program wrote is indexed by the same disk check that puts it in
/// the tree — the watcher is the index's event source once the startup pass ends.
#[gpui::test]
fn a_note_written_elsewhere_is_indexed_with_the_tree(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    std::fs::write(dir.path().join("extra.md"), "two more words\n").unwrap();
    window
        .update(cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();

    type_command("wordcount", cx);
    assert_eq!(
        status(&window, cx),
        "9 words in 2 notes",
        "the new note is in the count without Shale reading the vault again"
    );
}

/// Shale's own save is invisible to the watcher — its own writes are re-stamped out
/// of the way — so the save path has to report the note itself.
#[gpui::test]
fn a_note_shale_saves_is_indexed_at_once(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("Welcome.md"), cx)
        })
        .unwrap();
    // `A` at the end of the first line, so the words land where the count expects
    // them rather than in the middle of the heading's marker.
    cx.simulate_keystrokes("A");
    cx.simulate_input(" extra words here");
    type_command("w", cx);

    type_command("wordcount", cx);
    assert_eq!(
        status(&window, cx),
        "9 words in 1 note",
        "the save reported the note; nothing had to wait for a disk check"
    );
}

/// A note Shale creates is in the index from the moment it exists, and a note it
/// deletes is out of it — otherwise the count quietly grows stale in both
/// directions.
#[gpui::test]
fn creating_a_note_puts_it_in_the_count(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("Fresh");
    cx.simulate_keystrokes("enter i");
    cx.simulate_input("four words right in");
    type_command("w", cx);

    type_command("wordcount", cx);
    assert_eq!(status(&window, cx), "10 words in 2 notes");
}

/// A rename moves the note's rows rather than adding a second copy of them, so the
/// count does not start counting the same note twice.
#[gpui::test]
fn a_rename_moves_the_count_rather_than_adding_to_it(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(cx, &[("Target.md", IDEA), ("a.md", "see [[Target]] now\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space f");
    cx.simulate_input("target");
    cx.simulate_keystrokes("enter ctrl-shift-r");
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace backspace");
    cx.simulate_input("Renamed");
    cx.simulate_keystrokes("enter");

    assert!(dir.path().join("Renamed.md").is_file());
    type_command("wordcount", cx);
    assert_eq!(
        status(&window, cx),
        "6 words in 2 notes",
        "the note that moved is still counted once"
    );
}

/// A vault with `index.enabled = false` has nothing to count, and says so rather
/// than inventing a number — the same posture as a checker that will not start.
#[gpui::test]
fn a_vault_without_an_index_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("wordcount", cx);
    assert!(
        status(&window, cx).contains("no vault index"),
        "the window says which feature is missing: {}",
        status(&window, cx)
    );
}
