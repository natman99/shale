//! The file tree and the vault edits behind it, driven through the same keymap
//! and input paths the live app uses: `ctrl-n` opens the prompt, the name is
//! typed through the input handler, and Enter is what touches the disk.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{TestAppContext, VisualTestContext, WindowHandle};
use shale_core::{Config, Vault};

use super::*;
use crate::workspace::{keybindings, Workspace};

/// A workspace over a throwaway vault holding `files`, with the note focused the
/// way `main` focuses it. The `TempDir` is returned so the caller can check what
/// reached the disk.
fn workspace(
    cx: &mut TestAppContext,
    files: &[&str],
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let dir = tempfile::tempdir().unwrap();
    for name in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, "").unwrap();
    }
    let vault = Rc::new(Vault::open(dir.path()).unwrap());
    cx.update(|cx| cx.bind_keys(keybindings()));
    let window =
        cx.add_window(|_window, cx| Workspace::new(vault.clone(), Rc::new(Config::default()), cx));
    window
        .update(cx, |ws, window, cx| {
            ws.focus_editor(&FocusEditor, window, cx);
        })
        .unwrap();
    (dir, window)
}

/// The workspace's state, read out of the window.
fn state(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> (Vec<PathBuf>, String) {
    window
        .update(cx, |ws, _window, _cx| {
            (
                ws.tree_rows(),
                ws.active_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            )
        })
        .unwrap()
}

#[gpui::test]
fn ctrl_n_asks_for_a_name_and_creates_the_note(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("Idea");
    cx.simulate_keystrokes("enter");

    assert!(
        dir.path().join("Idea.md").is_file(),
        "the name became a note, with `.md` added"
    );
    let (rows, active) = state(&window, cx);
    assert_eq!(
        rows,
        vec![PathBuf::from("Idea.md"), PathBuf::from("Welcome.md")],
        "the tree re-read the vault"
    );
    assert_eq!(active, "Idea.md", "and the new note is the one on screen");
}

#[gpui::test]
fn escape_backs_out_of_the_prompt_without_touching_the_vault(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("Half typed");
    cx.simulate_keystrokes("escape");

    assert!(
        !dir.path().join("Half typed.md").exists(),
        "nothing was written"
    );
    let (rows, _) = state(&window, cx);
    assert_eq!(rows, vec![PathBuf::from("Welcome.md")]);
}

/// While the prompt is open its keys win: a letter that vim would use is a
/// letter here, and the editor stays in normal mode.
#[gpui::test]
fn the_prompt_takes_the_keyboard_from_vim(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("i");
    cx.simulate_keystrokes("enter");

    assert!(
        dir.path().join("i.md").is_file(),
        "`i` was typed, not bound"
    );
    let mode = window
        .update(cx, |ws, _window, cx| ws.mode_label(cx))
        .unwrap();
    assert_eq!(mode, "NORMAL", "the editor never left normal mode");
}

/// Backspace is what fixes a mistyped name, and it removes a whole character
/// rather than a byte of one.
#[gpui::test]
fn backspace_edits_the_name(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("Nope");
    cx.simulate_keystrokes("backspace backspace backspace backspace");
    cx.simulate_input("Kee");
    cx.simulate_keystrokes("enter");

    assert!(dir.path().join("Kee.md").is_file());
    assert!(!dir.path().join("Nope.md").exists());
}

#[gpui::test]
fn the_tree_walks_with_j_and_k(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/First.md", "notes/Second.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-b");
    let selected = window
        .update(cx, |ws, _window, _cx| ws.tree_selection())
        .unwrap();
    assert_eq!(
        selected,
        Some(PathBuf::from("notes")),
        "the folders come first, and the selection starts at the top"
    );

    cx.simulate_keystrokes("j j");
    let selected = window
        .update(cx, |ws, _window, _cx| ws.tree_selection())
        .unwrap();
    assert_eq!(selected, Some(PathBuf::from("notes/Second.md")));

    cx.simulate_keystrokes("k");
    let selected = window
        .update(cx, |ws, _window, _cx| ws.tree_selection())
        .unwrap();
    assert_eq!(selected, Some(PathBuf::from("notes/First.md")));

    // `enter` on a note opens it.
    cx.simulate_keystrokes("enter");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "notes/First.md");
}

/// A collapsed folder takes its children out of the rows, so `j` steps over
/// them.
#[gpui::test]
fn h_closes_a_folder_and_j_steps_over_what_it_hid(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/First.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-b h");
    let (rows, _) = state(&window, cx);
    assert_eq!(
        rows,
        vec![PathBuf::from("notes"), PathBuf::from("Welcome.md")],
        "the folder's note is no longer a row"
    );

    cx.simulate_keystrokes("j");
    let selected = window
        .update(cx, |ws, _window, _cx| ws.tree_selection())
        .unwrap();
    assert_eq!(selected, Some(PathBuf::from("Welcome.md")));
}

#[gpui::test]
fn rename_moves_the_note_on_disk(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Select the row and open it, so the rename has an open note to follow.
    cx.simulate_keystrokes("ctrl-b enter r");
    // The field starts with the current stem, so only the change is typed.
    cx.simulate_input("ed");
    cx.simulate_keystrokes("enter");

    assert!(
        dir.path().join("Welcomeed.md").is_file(),
        "the stem was edited in place"
    );
    assert!(!dir.path().join("Welcome.md").exists());
    let (rows, active) = state(&window, cx);
    assert_eq!(active, "Welcomeed.md", "the open note follows its file");
    assert_eq!(rows, vec![PathBuf::from("Welcomeed.md")]);
}

/// A delete is the one destructive thing in the tree, so it asks first: anything
/// but `yes` leaves the note alone.
#[gpui::test]
fn delete_waits_for_a_yes(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-b shift-d");
    cx.simulate_input("no");
    cx.simulate_keystrokes("enter");
    assert!(
        dir.path().join("Welcome.md").is_file(),
        "`no` means the note stays"
    );

    cx.simulate_keystrokes("shift-d");
    cx.simulate_input("yes");
    cx.simulate_keystrokes("enter");
    assert!(!dir.path().join("Welcome.md").exists());
    let (rows, active) = state(&window, cx);
    assert!(rows.is_empty(), "and the tree noticed");
    assert_eq!(active, "", "with nothing open");
}

#[gpui::test]
fn a_new_folder_gets_a_row(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-n");
    cx.simulate_input("inbox");
    cx.simulate_keystrokes("enter");

    assert!(dir.path().join("inbox").is_dir());
    let (rows, _) = state(&window, cx);
    assert_eq!(
        rows,
        vec![PathBuf::from("inbox"), PathBuf::from("Welcome.md")],
        "an empty folder is still a folder the user can see"
    );

    // The new folder is selected, so the next note lands inside it.
    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("Thought");
    cx.simulate_keystrokes("enter");
    assert!(dir.path().join("inbox/Thought.md").is_file());
    let (_, active) = state(&window, cx);
    assert_eq!(active, Path::new("inbox/Thought.md").display().to_string());
}

/// A name the vault cannot hold is refused with a message instead of a crash, and
/// the prompt stays usable.
#[gpui::test]
fn an_impossible_name_is_refused(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("bad:name");
    cx.simulate_keystrokes("enter");

    assert_eq!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy() != "Welcome.md")
            .count(),
        0,
        "nothing was created"
    );
    let status = window.update(cx, |ws, _window, _cx| ws.status()).unwrap();
    assert!(
        status.contains("not allowed"),
        "the status bar says why: {status}"
    );
    let prompt = window
        .update(cx, |ws, _window, cx| ws.prompt_text(cx))
        .unwrap();
    assert_eq!(
        prompt.as_deref(),
        Some("bad:name"),
        "the prompt stays open so the name can be fixed rather than retyped"
    );
}
