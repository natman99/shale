//! The file tree and the vault edits behind it, driven through the same keymap
//! and input paths the live app uses: `ctrl-n` opens the prompt, the name is
//! typed through the input handler, and Enter is what touches the disk.

use std::path::{Path, PathBuf};

use super::*;

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

/// A fresh vault's folders start **closed**. The tree is still the vault, but a
/// thousand-note one with every folder open is a thousand rows laid out on every frame,
/// and a sidebar nobody can scan is not a tree. What the user opens is session state.
#[gpui::test]
fn a_fresh_vault_starts_with_its_folders_closed(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(
        cx,
        &["Welcome.md", "notes/First.md", "notes/deep/Second.md"],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let (rows, _) = state(&window, cx);
    assert_eq!(
        rows,
        vec![PathBuf::from("notes"), PathBuf::from("Welcome.md")],
        "the folder is a row; what is inside it is not"
    );
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

/// `ctrl-backspace` in the prompt takes the word before the caret, so a name that
/// went wrong is fixed a word at a time rather than character by character.
#[gpui::test]
fn ctrl_backspace_takes_a_word_out_of_a_name(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("Meeting Notes Draft");
    cx.simulate_keystrokes("ctrl-backspace");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.prompt_text(cx))
            .unwrap()
            .as_deref(),
        Some("Meeting Notes "),
        "the word and the space in front of the caret are gone"
    );

    cx.simulate_keystrokes("ctrl-backspace");
    cx.simulate_input("Plan");
    cx.simulate_keystrokes("enter");
    assert!(
        dir.path().join("Meeting Plan.md").is_file(),
        "the name that was left is what reached the disk"
    );
    assert!(
        !dir.path().join("Meeting Notes Draft.md").exists(),
        "and the word that was deleted did not"
    );
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

    // `ctrl-w h` gives the sidebar the keyboard; the bare `l` after it is the tree's
    // own expand, which the folder needs because a fresh vault starts with them closed.
    cx.simulate_keystrokes("ctrl-w h l");
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

    // The folder starts closed, so the bare `l` opens it and the `h` after it is the
    // close the test is about.
    cx.simulate_keystrokes("ctrl-w h l h");
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

/// `enter` on a note row opens the note *and* moves the keyboard to it: the tree
/// is where you go to find a note, so the next thing typed is meant for the note.
#[gpui::test]
fn enter_on_a_note_row_moves_to_the_editor(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h enter i");
    cx.simulate_input("typed");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "typed",
        "the note has the keyboard, so `i` is vim's insert and not a tree binding"
    );
    let (_, active) = state(&window, cx);
    assert_eq!(active, "Welcome.md", "and it is the note that row names");
}

/// A folder row is the other case: `enter` opens or closes it and the sidebar
/// keeps the keyboard, because the next key is another tree motion.
#[gpui::test]
fn enter_on_a_folder_row_stays_in_the_tree(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["notes/First.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The first row is the folder; `r` is bound only in the tree's context, so the
    // prompt it opens is proof the sidebar still had the keyboard.
    cx.simulate_keystrokes("ctrl-w h enter r");
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.prompt_text(cx))
            .unwrap()
            .is_some(),
        "the folder row left the keyboard where it was"
    );
}

#[gpui::test]
fn rename_moves_the_note_on_disk(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Select the row and open it, so the rename has an open note to follow. The
    // note takes the keyboard, so the rename comes from the global binding.
    cx.simulate_keystrokes("ctrl-w h enter ctrl-shift-r");
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

/// A rename is only finished when the links that pointed at the old name point at
/// the new one, which means reading the rest of the vault and rewriting whatever
/// resolves to the note that moved.
#[gpui::test]
fn a_rename_rewrites_the_links_that_point_at_the_note(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(
        cx,
        &[
            ("Target.md", "the note that gets a new name\n"),
            ("a.md", "see [[Target]] and [[Target|read this]]\n"),
            ("b.md", "a link to [[a]] instead\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The rename acts on the open note, and the prompt's field starts with the
    // current stem, so the old name has to be erased before the new one is typed.
    cx.simulate_keystrokes("space f");
    cx.simulate_input("target");
    cx.simulate_keystrokes("enter ctrl-shift-r");
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace backspace");
    cx.simulate_input("Renamed");
    cx.simulate_keystrokes("enter");

    assert!(dir.path().join("Renamed.md").is_file());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "see [[Renamed]] and [[Renamed|read this]]\n",
        "both forms point at the new name, and the alias survives"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.md")).unwrap(),
        "a link to [[a]] instead\n",
        "a link to a different note is not touched"
    );
    let status = window.update(cx, |ws, _window, _cx| ws.status()).unwrap();
    assert!(
        status.contains("2 links"),
        "the status says what else the rename did: {status}"
    );
}

/// The one note Shale must not write blind is the open one: with unsaved edits,
/// the disk copy is not what the user is looking at, so its links wait for a save
/// and the status says so.
#[gpui::test]
fn a_rename_leaves_a_dirty_open_note_alone(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(
        cx,
        &[
            ("Target.md", "I link to [[Target]] myself\n"),
            ("a.md", "see [[Target]]\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space f");
    cx.simulate_input("target");
    cx.simulate_keystrokes("enter i x escape ctrl-shift-r");
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace backspace");
    cx.simulate_input("Renamed");
    cx.simulate_keystrokes("enter");

    assert_eq!(
        std::fs::read_to_string(dir.path().join("Renamed.md")).unwrap(),
        "I link to [[Target]] myself\n",
        "the note with unsaved edits is not written behind the user's back"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "see [[Renamed]]\n",
        "every other note is still updated"
    );
    let status = window.update(cx, |ws, _window, _cx| ws.status()).unwrap();
    assert!(
        status.contains("unsaved edits"),
        "and the unfinished part is named: {status}"
    );
}

/// A delete is the one destructive thing in the tree, so it asks first: anything
/// but `yes` leaves the note alone.
#[gpui::test]
fn delete_waits_for_a_yes(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h shift-d");
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

/// What the vault watcher does on its next tick, without waiting for the timer.
fn check_vault(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) {
    window
        .update(cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();
    cx.run_until_parked();
}

/// A note made by another program gets a row on its own.
#[gpui::test]
fn a_note_created_outside_shale_gets_a_row(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let (rows, _) = state(&window, cx);
    assert_eq!(rows, vec![PathBuf::from("Welcome.md")]);

    std::fs::write(dir.path().join("Outside.md"), "written elsewhere").unwrap();
    check_vault(&window, cx);

    let (rows, active) = state(&window, cx);
    assert_eq!(
        rows,
        vec![PathBuf::from("Outside.md"), PathBuf::from("Welcome.md")],
        "the tree followed the disk"
    );
    assert_eq!(active, "", "and nothing was opened that nobody asked for");
}

/// A note nobody is editing is re-read, which is what makes an external edit
/// visible instead of being silently overwritten by the next save.
#[gpui::test]
fn a_clean_note_changed_outside_shale_is_reloaded(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("Welcome.md"), cx)
        })
        .unwrap();
    std::fs::write(dir.path().join("Welcome.md"), "changed elsewhere").unwrap();
    check_vault(&window, cx);

    let text = window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap();
    assert_eq!(text, "changed elsewhere");
    let status = window.update(cx, |ws, _window, _cx| ws.status()).unwrap();
    assert!(
        status.contains("reloaded"),
        "the status bar says so: {status}"
    );
}

/// Unsaved edits outrank the disk: the buffer keeps what the user typed, and the
/// status bar tells them the file moved under them.
#[gpui::test]
fn an_outside_edit_does_not_overwrite_unsaved_edits(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("Welcome.md"), cx)
        })
        .unwrap();
    cx.simulate_keystrokes("A");
    cx.simulate_input(" mine");
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_is_dirty(cx))
            .unwrap(),
        "the buffer holds an edit that is not on disk"
    );

    std::fs::write(dir.path().join("Welcome.md"), "changed elsewhere").unwrap();
    check_vault(&window, cx);

    let text = window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap();
    assert!(text.ends_with(" mine"), "the edit survived: {text:?}");
    let status = window.update(cx, |ws, _window, _cx| ws.status()).unwrap();
    assert!(
        status.contains("changed on disk"),
        "and it is visible: {status}"
    );
}

/// A note deleted elsewhere leaves the tree. Deleted while it is open and
/// unsaved, the text stays on screen so `ctrl-s` can write it back.
#[gpui::test]
fn a_note_deleted_outside_shale_leaves_the_tree(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md", "Gone.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    std::fs::remove_file(dir.path().join("Gone.md")).unwrap();
    check_vault(&window, cx);
    let (rows, _) = state(&window, cx);
    assert_eq!(rows, vec![PathBuf::from("Welcome.md")]);

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("Welcome.md"), cx)
        })
        .unwrap();
    cx.simulate_keystrokes("A");
    cx.simulate_input(" mine");
    std::fs::remove_file(dir.path().join("Welcome.md")).unwrap();
    check_vault(&window, cx);

    let text = window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap();
    assert!(text.ends_with(" mine"), "the text is not thrown away");
    let status = window.update(cx, |ws, _window, _cx| ws.status()).unwrap();
    assert!(
        status.contains("deleted from disk"),
        "and it is visible: {status}"
    );
}

/// Shale's own save must not come back as somebody else's edit, so the signature
/// is re-taken after it writes and the next pass has nothing to report.
#[gpui::test]
fn saving_our_own_note_does_not_look_like_an_outside_change(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("Welcome.md"), cx)
        })
        .unwrap();
    cx.simulate_keystrokes("i");
    cx.simulate_input("typed");
    cx.simulate_keystrokes("escape ctrl-s");
    check_vault(&window, cx);

    let text = window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap();
    assert_eq!(text, "typed", "the buffer still holds what was typed");
    let status = window.update(cx, |ws, _window, _cx| ws.status()).unwrap();
    assert!(
        status.contains("saved"),
        "the last word is still the save: {status}"
    );
}

/// The tree's three states are the window's state grammar (`design-language.md` §4):
/// the open note's row and the pointer's row take their backgrounds, and the row the
/// keyboard is on takes the focus rule. A third background would make the cursor and
/// the open note look alike, which is the pair every vault edit depends on telling
/// apart.
#[gpui::test]
fn the_tree_marks_its_keyboard_row_with_the_focus_rule(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/First.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    // The sidebar has the keyboard, and its selection starts at the top row.
    cx.simulate_keystrokes("ctrl-w h");
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let want = window
        .update(cx, |ws, _window, _cx| {
            gpui::Hsla::from(gpui::rgb(ws.palette().ui_active_border))
        })
        .unwrap();
    let ruled = window
        .update(cx, move |_ws, window, _cx| {
            window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    // Every row carries a rule so none of them shift when the
                    // keyboard moves; only the selected one is drawn in the
                    // focus colour, and only the selected one has a width.
                    quad.border_widths.top.as_f32() > 0.0
                        && (quad.border_color.h - want.h).abs() < 0.001
                        && (quad.border_color.s - want.s).abs() < 0.001
                        && (quad.border_color.l - want.l).abs() < 0.001
                })
                .collect::<Vec<_>>()
        })
        .unwrap();
    // `painted_quads` accumulates over every frame that was drawn, so one row
    // comes back once per frame: what is being counted is rows, not quads.
    let mut places = ruled
        .iter()
        .map(|quad| (quad.bounds.origin.x.as_f32(), quad.bounds.origin.y.as_f32()))
        .collect::<Vec<_>>();
    places.sort_by(|a, b| a.partial_cmp(b).unwrap());
    places.dedup();
    assert_eq!(
        places.len(),
        1,
        "one row carries the focus rule: {places:?}"
    );
}

/// Where the note is painted: the widest quad wearing the note's own background
/// colour, as `(x, width)`.
///
/// The note column is the widest thing in that colour — the editor inside it is
/// narrower by the column's padding, and the chrome around it wears other colours —
/// so this is the note's box on screen. Both halves matter: the sidebar's width in
/// device pixels depends on the window's scale factor, so a test that hard-coded 260
/// would be measuring the display rather than the layout.
fn note_box(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> (f32, f32) {
    window
        .update(cx, |ws, window, _cx| {
            let want = gpui::Hsla::from(gpui::rgb(ws.palette().bg));
            window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    quad.background.as_solid().is_some_and(|bg| {
                        (bg.h - want.h).abs() < 0.001
                            && (bg.s - want.s).abs() < 0.001
                            && (bg.l - want.l).abs() < 0.001
                    })
                })
                .map(|quad| {
                    (
                        quad.bounds.origin.x.as_f32(),
                        quad.bounds.size.width.as_f32(),
                    )
                })
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        })
        .unwrap()
        .unwrap_or((0.0, 0.0))
}

/// `space e` puts the file tree away and brings it back, the way nvim-tree and
/// NERDTree answer `<leader>e`. The bar says which half was reached, because a
/// sidebar that vanishes with no other change on screen is easy to mistake for a
/// key that did nothing.
#[gpui::test]
fn space_e_puts_the_file_tree_away_and_brings_it_back(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/First.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    assert!(
        window
            .update(cx, |ws, _window, _cx| ws.tree_visible())
            .unwrap(),
        "a window starts with its tree showing"
    );

    cx.simulate_keystrokes("space e");
    let (hidden, status) = window
        .update(cx, |ws, _window, _cx| (!ws.tree_visible(), ws.status()))
        .unwrap();
    assert!(hidden, "the chord put the tree away");
    assert!(status.contains("tree hidden"), "the bar says so: {status}");

    cx.simulate_keystrokes("space e");
    let (shown, status) = window
        .update(cx, |ws, _window, _cx| (ws.tree_visible(), ws.status()))
        .unwrap();
    assert!(shown, "the same two keys bring it back");
    assert!(status.contains("tree shown"), "and say which: {status}");
}

/// The point of hiding the tree is the width, so that is what is measured: the note
/// has to take the sidebar's column rather than leave a gap where the tree was. The
/// frame is `flex`, so this happens by not drawing the sidebar at all — the test is
/// here to keep it that way when the frame is next reworked.
#[gpui::test]
fn the_note_takes_the_width_the_tree_had(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The first frame has no measured width yet, so paint more than once.
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let (x_with, width_with) = note_box(&window, cx);
    assert!(width_with > 0.0, "the note paints a background at all");
    assert!(
        x_with > 0.0,
        "with the tree showing, the note starts to the right of it: x {x_with}"
    );

    cx.simulate_keystrokes("space e");
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let (x_without, width_without) = note_box(&window, cx);

    // With the tree away the note starts at the content area's left edge —
    // which is now inside the window's 4 px gutter (8 px at the test
    // window's 2x scale), not under the compositor's border.
    assert_eq!(
        x_without, 8.0,
        "with the tree away the note starts at the content edge"
    );
    // The gap the sidebar left is exactly what the note gained, so nothing is
    // left behind where the tree used to be.
    assert_eq!(
        width_without - width_with,
        x_with - x_without,
        "the note gained the sidebar's column: {x_with} -> {x_without}, {width_with} -> {width_without}"
    );
}

/// A tree that goes away while it holds the keyboard must not leave the keyboard on
/// a row that is no longer drawn: every key from there would be aimed at nothing.
/// The note takes the keyboard instead, which is checked by typing into it.
#[gpui::test]
fn a_hidden_tree_does_not_keep_the_keyboard(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h");
    assert!(
        window
            .update(cx, |ws, window, _cx| ws.tree_focus().is_focused(window))
            .unwrap(),
        "the sidebar has the keyboard"
    );

    cx.simulate_keystrokes("space e");
    cx.simulate_keystrokes("i typed");
    let text = window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap();
    assert!(
        text.contains("typed"),
        "the keys after the tree went away reached the note: {text:?}"
    );

    // And `ctrl-w h` is the way back: a window move aimed at a sidebar that is not
    // drawn would otherwise be a dead key.
    cx.simulate_keystrokes("escape ctrl-w h");
    let (shown, focused) = window
        .update(cx, |ws, window, _cx| {
            (ws.tree_visible(), ws.tree_focus().is_focused(window))
        })
        .unwrap();
    assert!(
        shown && focused,
        "the move brings the tree back and focuses it"
    );
}

/// `:tree` is the same command from the command line, and takes `on` and `off` the
/// way `:focus` and `:typewriter` do.
#[gpui::test]
fn the_command_line_moves_the_tree_both_ways(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes(":");
    cx.simulate_input("tree off");
    cx.simulate_keystrokes("enter");
    let (hidden, status) = window
        .update(cx, |ws, _window, _cx| (!ws.tree_visible(), ws.status()))
        .unwrap();
    assert!(hidden, "`:tree off` put it away");
    assert!(status.contains("tree hidden"), "and said so: {status}");

    // Asking for a state that is already true changes nothing, and says that
    // rather than pretending to have done something.
    cx.simulate_keystrokes(":");
    cx.simulate_input("tree off");
    cx.simulate_keystrokes("enter");
    let status = window.update(cx, |ws, _window, _cx| ws.status()).unwrap();
    assert!(
        status.contains("already hidden"),
        "the second `:tree off` reports itself: {status}"
    );

    cx.simulate_keystrokes(":");
    cx.simulate_input("tree on");
    cx.simulate_keystrokes("enter");
    assert!(
        window
            .update(cx, |ws, _window, _cx| ws.tree_visible())
            .unwrap(),
        "`:tree on` brings it back"
    );

    // A bare `:tree` is the toggle, so it agrees with the chord.
    cx.simulate_keystrokes(":");
    cx.simulate_input("tree");
    cx.simulate_keystrokes("enter");
    assert!(
        !window
            .update(cx, |ws, _window, _cx| ws.tree_visible())
            .unwrap(),
        "a bare `:tree` turns it around"
    );
}
