//! Bringing a file in from outside the vault: the platform hands the window a list of
//! paths, the tree decides which folder they land in, and only a markdown note or an
//! image is allowed through. The drop is driven through gpui's real file-drop events —
//! the same translation the X11 and Windows backends feed the window — so the tests
//! exercise the same path the live app does.

use std::path::{Path, PathBuf};

use super::review::say;
use super::*;
use gpui::{ExternalPaths, FileDropEvent, InputEvent, WindowHandle};

/// A vault with a note in each of two folders.
fn vault(cx: &mut TestAppContext) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    workspace(cx, &["notes/Deep.md", "other/keep.md"])
}

/// A file outside the vault, the kind a file manager hands over.
fn outside(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// Paint a frame, so an element that only exists after an update has bounds to find.
fn draw(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// The middle of a named element, in window px.
fn centre(cx: &mut VisualTestContext, selector: &'static str) -> gpui::Point<gpui::Pixels> {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no element named {selector:?} on screen"));
    gpui::point(
        bounds.origin.x + bounds.size.width / 2.,
        bounds.origin.y + bounds.size.height / 2.,
    )
}

/// The platform's own gesture: the files enter the window, the pointer lets go over
/// `selector`, and the drag ends. gpui turns these into a hover and a drop, which is
/// what the tree's listeners answer.
fn drop_on(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, paths: &[PathBuf]) {
    let paths = ExternalPaths(paths.to_vec().into());
    cx.update(|window, app| {
        window.dispatch_event(
            FileDropEvent::Entered {
                position: at,
                paths,
            }
            .to_platform_input(),
            app,
        );
        window.dispatch_event(
            FileDropEvent::Submit { position: at }.to_platform_input(),
            app,
        );
        window.dispatch_event(FileDropEvent::Exited.to_platform_input(), app);
        window.dispatch_event(FileDropEvent::Ended.to_platform_input(), app);
    });
}

fn drop_on_row(cx: &mut VisualTestContext, selector: &'static str, paths: &[PathBuf]) {
    let at = centre(cx, selector);
    drop_on(cx, at, paths);
}

/// A markdown note dropped on a folder row comes into the vault under that folder,
/// keeping its name.
#[gpui::test]
fn dropping_a_note_on_a_folder_row_imports_it(cx: &mut TestAppContext) {
    let (dir, window) = vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let from = tempfile::tempdir().unwrap();
    let src = outside(
        from.path(),
        "Goblin.md",
        "the night market, in three paragraphs\n".as_bytes(),
    );

    drop_on_row(cx, "tree-row other", &[src]);

    let landed = dir.path().join("other/Goblin.md");
    assert!(
        landed.is_file(),
        "the note came into the folder the pointer let go over: {:?}",
        on_disk(dir.path())
    );
    assert_eq!(
        std::fs::read_to_string(&landed).unwrap(),
        "the night market, in three paragraphs\n",
        "and it came in as it was, byte for byte"
    );
    assert!(
        from.path().join("Goblin.md").is_file(),
        "the file outside the vault is left where it was: an import copies"
    );
    assert_eq!(
        say(&window, cx),
        "imported other/Goblin.md",
        "the status line names what came in"
    );
}

/// An image comes in too — the other half of the filter.
#[gpui::test]
fn dropping_an_image_imports_it(cx: &mut TestAppContext) {
    let (dir, window) = vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let from = tempfile::tempdir().unwrap();
    let src = outside(
        from.path(),
        "map.png",
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a],
    );

    drop_on_row(cx, "tree-row notes", &[src]);

    let landed = dir.path().join("notes/map.png");
    assert!(
        landed.is_file(),
        "the picture landed in the folder it was dropped on: {:?}",
        on_disk(dir.path())
    );
    assert_eq!(
        std::fs::read(&landed).unwrap(),
        vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a],
        "and no text munging touched the bytes"
    );
    assert!(
        say(&window, cx).contains("imported notes/map.png"),
        "the status line says so: {}",
        say(&window, cx)
    );
}

/// Anything else is counted and left outside. A vault full of whatever the file
/// manager happened to be holding is not what a drag asked for.
#[gpui::test]
fn a_file_that_is_neither_a_note_nor_an_image_is_left_out(cx: &mut TestAppContext) {
    let (dir, window) = vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let from = tempfile::tempdir().unwrap();
    let txt = outside(from.path(), "shopping.txt", b"milk, bread\n");
    let zip = outside(from.path(), "archive.zip", b"PK\x03\x04");

    drop_on_row(cx, "tree-row other", &[txt, zip]);

    assert_eq!(
        on_disk(dir.path()),
        vec!["notes/Deep.md", "other/keep.md"],
        "nothing came in"
    );
    assert_eq!(
        say(&window, cx),
        "skipped 2 files: only notes and images come in",
        "and the refusal is said out loud"
    );
}

/// A name that is taken is refused, not overwritten: the common case is dropping the
/// same screenshot in twice, and quietly replacing the one the author has been
/// embedding is the worst thing a drop could do.
#[gpui::test]
fn an_import_onto_a_name_that_is_taken_is_refused(cx: &mut TestAppContext) {
    let (dir, window) = vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let from = tempfile::tempdir().unwrap();
    let src = outside(from.path(), "keep.md", b"an impostor\n");

    drop_on_row(cx, "tree-row other", &[src]);

    assert_eq!(
        std::fs::read_to_string(dir.path().join("other/keep.md")).unwrap(),
        "",
        "the note in the vault still holds what it held"
    );
    assert!(
        say(&window, cx).contains("other/keep.md already exists"),
        "and the refusal names the file: {}",
        say(&window, cx)
    );
}

/// A file that already lives in the vault is not copied: inside the vault, a file
/// moves with `:move`, which takes the links with it.
#[gpui::test]
fn a_file_already_in_the_vault_is_not_copied(cx: &mut TestAppContext) {
    let (dir, window) = vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let inside = dir.path().join("notes/Deep.md");

    drop_on_row(cx, "tree-row other", &[inside]);

    assert!(
        !dir.path().join("other/Deep.md").exists(),
        "no second copy appeared: {:?}",
        on_disk(dir.path())
    );
    assert!(
        say(&window, cx).contains("already in the vault"),
        "and the answer says what to do instead: {}",
        say(&window, cx)
    );
}

/// The tree's own background is the top level, for an import exactly as for a move.
#[gpui::test]
fn dropping_on_the_tree_background_imports_to_the_top_level(cx: &mut TestAppContext) {
    let (dir, window) = vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let from = tempfile::tempdir().unwrap();
    let src = outside(from.path(), "Loose.md", b"a note with no folder\n");

    let panel = cx
        .debug_bounds("tree-scroll")
        .expect("the tree panel is on screen");
    drop_on(
        cx,
        gpui::point(
            panel.origin.x + panel.size.width / 2.,
            panel.origin.y + panel.size.height - gpui::px(20.),
        ),
        &[src],
    );

    assert!(
        dir.path().join("Loose.md").is_file(),
        "the note came in at the top level: {:?}",
        on_disk(dir.path())
    );
}

/// What came in is revealed: the folder is opened and the imported note is the tree's
/// row, so the author can see the file they asked for instead of wondering whether the
/// drop did anything.
#[gpui::test]
fn an_imported_note_is_revealed_in_the_tree(cx: &mut TestAppContext) {
    let (_dir, window) = vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let from = tempfile::tempdir().unwrap();
    let src = outside(from.path(), "Goblin.md", b"a new note\n");

    drop_on_row(cx, "tree-row other", &[src]);
    draw(cx);

    let (rows, selected) = window
        .update(cx, |ws, _window, _cx| {
            (
                ws.tree_rows()
                    .iter()
                    .map(shale_core::vault::rel_text)
                    .collect::<Vec<String>>(),
                ws.tree_selection()
                    .as_deref()
                    .map(shale_core::vault::rel_text),
            )
        })
        .unwrap();
    assert!(
        rows.iter().any(|r| r == "other/Goblin.md"),
        "the folder it landed in is open and the row is drawn: {rows:?}"
    );
    assert_eq!(
        selected.as_deref(),
        Some("other/Goblin.md"),
        "and the tree is standing on it"
    );
}

/// A drop of several files says one thing: the count, not a line per file.
#[gpui::test]
fn a_drop_of_several_files_says_the_count(cx: &mut TestAppContext) {
    let (dir, window) = vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let from = tempfile::tempdir().unwrap();
    let a = outside(from.path(), "One.md", b"one\n");
    let b = outside(from.path(), "Two.md", b"two\n");
    let c = outside(from.path(), "map.png", b"\x89PNG");
    let d = outside(from.path(), "notes.txt", b"not welcome\n");

    drop_on_row(cx, "tree-row other", &[a, b, c, d]);

    assert_eq!(
        say(&window, cx),
        "imported 3 files into other/; skipped 1 file: only notes and images come in",
        "the successes are counted, the refusal is explained"
    );
    for name in ["One.md", "Two.md", "map.png"] {
        assert!(
            dir.path().join("other").join(name).is_file(),
            "{name} came in"
        );
    }
    assert!(
        !dir.path().join("other/notes.txt").exists(),
        "and the text file did not"
    );
}

/// The index learns about the note that came in, so the vault reads as though the note
/// had always been there: the link it makes is a backlink on the note it points at.
/// Without this, the imported note is a file the tree can see and the vault cannot.
#[gpui::test]
fn an_imported_note_is_in_the_index(cx: &mut TestAppContext) {
    // The index is off in the plain test fixture — a DB and a writer thread per window
    // is not what a keymap test is about — so this one asks for the indexed vault.
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("notes/Deep.md", "# Deep\n\nthe deep note\n"),
            ("other/keep.md", ""),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let from = tempfile::tempdir().unwrap();
    let src = outside(from.path(), "Caller.md", "points at [[Deep]]\n".as_bytes());

    drop_on_row(cx, "tree-row other", &[src]);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("notes/Deep.md"), cx)
        })
        .unwrap();
    // The strip's backlinks view: open the panel, take the seat, walk to the view.
    cx.simulate_keystrokes("space b");
    draw(cx);
    cx.simulate_keystrokes("ctrl-w l ]");
    cx.run_until_parked();
    draw(cx);

    let rows = window
        .update(cx, |ws, _window, _cx| ws.sidebar_rows())
        .unwrap();
    assert!(
        rows.iter().any(|(_, _, text)| text.contains("Caller")),
        "the imported note's link answers as a backlink: {rows:?}"
    );
}

/// Every file the vault holds, as a sorted list of vault-relative paths with `/`
/// separators — the way a test says "nothing came in" without depending on how a walk
/// happens to order things, or which separator the platform happens to use.
fn on_disk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let name = e.file_name().to_string_lossy().to_string();
        if e.file_type().unwrap().is_dir() {
            // The vault's own machinery (`.shale`, `.git`) is not vault content, and a
            // test about what came in should not be moved by what the app wrote home.
            if !name.starts_with('.') {
                walk(root, &e.path(), out);
            }
        } else {
            out.push(
                e.path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}
