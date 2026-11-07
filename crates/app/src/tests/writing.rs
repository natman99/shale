//! Writing a note that has no name yet: `:w` in a blank tab, `:w <name>`, and the
//! rule that a note which already has a file is never renamed or cloned by the
//! command line. The naming itself (`shale_core::note_name`) is tested in core;
//! these tests are about what the window does with it.

use super::commands::status;
use super::*;

/// A vault holding `a.md`, and a window on a blank tab -- nothing open, nothing
/// named, ready to be typed into.
fn blank(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(cx, &[("a.md", "Idea\n")]);
    let cx = VisualTestContext::from_window(window.into(), cx);
    (dir, window, cx)
}

/// Type into the tab on screen, the way a hand does it.
fn type_text(text: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("i");
    cx.simulate_input(text);
    cx.simulate_keystrokes("escape");
}

/// What is on disk under `name`, if anything.
fn disk(dir: &tempfile::TempDir, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.path().join(name)).ok()
}

/// The notes on the bar of pane `pix`, by name.
fn names(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, pix: usize) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| {
            ws.pane_tab_paths(pix, cx)
                .into_iter()
                .map(|p| p.map(|p| p.display().to_string()).unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .unwrap()
}

/// The whole point: a blank tab is a note you are writing, and `:w` gives it a
/// name out of the first line it holds.
#[gpui::test]
fn w_in_a_blank_tab_names_the_note_after_its_first_line(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = blank(cx);
    type_text("Shopping list\nmilk\nbread\n", &mut cx);

    type_command("w", &mut cx);

    assert_eq!(
        disk(&dir, "Shopping list.md").as_deref(),
        Some("Shopping list\nmilk\nbread\n"),
        "the file is there, with the text in it"
    );
    assert_eq!(
        names(&window, &mut cx, 0),
        vec!["Shopping list.md".to_string()],
        "and the tab knows it"
    );
    assert!(
        status(&window, &mut cx).contains("saved Shopping list.md"),
        "the bar says where it went: {}",
        status(&window, &mut cx)
    );
}

/// The name is the words, not the markdown or the punctuation: a heading names a
/// note, and the characters the filesystem refuses fall out.
#[gpui::test]
fn the_name_comes_from_the_line_cleaned_not_verbatim(cx: &mut TestAppContext) {
    let (dir, _window, mut cx) = blank(cx);
    type_text("# My Note: v2\nbody\n", &mut cx);

    type_command("w", &mut cx);

    assert_eq!(
        disk(&dir, "My Note v2.md").as_deref(),
        Some("# My Note: v2\nbody\n"),
        "the heading is kept in the text and cleaned out of the name"
    );
}

/// `:w <name>` names the note in hand, and the name may place it in a folder --
/// typing a path at the command line is asking for one.
#[gpui::test]
fn w_with_a_name_names_the_note_in_hand(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = blank(cx);
    type_text("anything\n", &mut cx);

    type_command("w areas/idea", &mut cx);

    assert_eq!(disk(&dir, "areas/idea.md").as_deref(), Some("anything\n"));
    assert_eq!(
        names(&window, &mut cx, 0),
        vec![Path::new("areas").join("idea.md").display().to_string()]
    );
}

/// The ruling: a note that already has a file is not renamed, copied, or moved by
/// `:w <name>`. The command refuses, and the note is left exactly as it was --
/// because the difference between this and `:rename` is that `:rename` rewrites
/// the links that point at the old name.
#[gpui::test]
fn w_with_a_name_refuses_on_a_note_that_already_has_one(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = blank(cx);
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx)
        })
        .unwrap();
    type_text("edited\n", &mut cx);

    type_command("w other", &mut cx);

    assert_eq!(disk(&dir, "other.md"), None, "no clone was made");
    assert_eq!(
        disk(&dir, "a.md").as_deref(),
        Some("Idea\n"),
        "the note on disk is untouched -- the command did nothing"
    );
    assert_eq!(
        names(&window, &mut cx, 0),
        vec!["a.md".to_string()],
        "and the tab still holds the note it had"
    );
    assert!(
        status(&window, &mut cx).contains("already has a name"),
        "the bar says why: {}",
        status(&window, &mut cx)
    );
}

/// An explicit name that collides is refused rather than quietly suffixed: a typed
/// name is a specific request, and `idea-2.md` is not what was asked for.
#[gpui::test]
fn w_with_a_name_will_not_write_over_an_existing_note(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = blank(cx);
    type_text("my version\n", &mut cx);

    type_command("w a.md", &mut cx);

    assert_eq!(
        disk(&dir, "a.md").as_deref(),
        Some("Idea\n"),
        "the existing note is exactly as it was"
    );
    assert!(
        status(&window, &mut cx).contains("already exists"),
        "the bar says why: {}",
        status(&window, &mut cx)
    );
}

/// A name taken from the first line is different: two notes that open with the
/// same line are the common case, and the user was not choosing a name at all --
/// so the second one gets a number instead of a refusal.
#[gpui::test]
fn a_name_taken_from_the_line_gets_a_number_when_it_is_taken(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("Idea.md", "the first one\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    type_text("Idea\nthe second one\n", &mut cx);

    type_command("w", &mut cx);

    assert_eq!(
        disk(&dir, "Idea-2.md").as_deref(),
        Some("Idea\nthe second one\n"),
        "the vault already had `Idea.md`"
    );
    assert_eq!(
        disk(&dir, "Idea.md").as_deref(),
        Some("the first one\n"),
        "and the note that already had that name is untouched"
    );
}

/// A blank tab with nothing in it has no name to take, and no empty file is
/// created to fill the void.
#[gpui::test]
fn an_empty_blank_tab_is_not_written(cx: &mut TestAppContext) {
    let (dir, _window, mut cx) = blank(cx);
    type_command("w", &mut cx);

    assert!(
        disk(&dir, "untitled.md").is_none(),
        "no file out of nothing"
    );
    let mut entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    entries.sort();
    assert_eq!(
        entries.iter().filter(|name| name.ends_with(".md")).count(),
        1,
        "the vault still holds only the note it started with: {entries:?}"
    );
}

/// A name typed at the `:` line cannot leave the vault. The `..` segments fall out
/// and the note lands where the vault starts.
#[gpui::test]
fn a_typed_name_cannot_escape_the_vault(cx: &mut TestAppContext) {
    let (dir, _window, mut cx) = blank(cx);
    type_text("payload\n", &mut cx);

    type_command("w ../../evil", &mut cx);

    assert_eq!(disk(&dir, "evil.md").as_deref(), Some("payload\n"));
    assert!(
        !dir.path().parent().unwrap().join("evil.md").exists(),
        "nothing was written outside the vault"
    );
}

/// The writes an exit owes go through the same path, so an unnamed note names
/// itself on the way out instead of its text going nowhere. `autosave_every_tab`
/// is the test's stand-in for the countdown a quit drives (`workspace::testhooks`).
#[gpui::test]
fn an_exit_names_an_unnamed_note_rather_than_losing_it(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = blank(cx);
    type_text("Half a thought\n", &mut cx);

    window
        .update(&mut cx, |ws, _window, cx| ws.autosave_every_tab(cx))
        .unwrap();

    assert_eq!(
        disk(&dir, "Half a thought.md").as_deref(),
        Some("Half a thought\n")
    );
}
