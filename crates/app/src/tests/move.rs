//! Moving a note between folders, driven through the same keymap and input paths
//! the live app uses: `m` in the tree asks, the picker's rows answer, and the vault
//! is what changes. A move is a rename with a folder in it, so the tests check the
//! same things a rename has to keep — links, tabs, the index, the selection — plus
//! the one thing a rename never had to do: reach a folder somewhere else in the
//! vault.

use std::path::{Path, PathBuf};

/// Open a note the way a reader would: the window's own open path. A command that
/// acts on "the note in hand" needs one in hand.
fn open(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, name: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(name), cx))
        .unwrap();
}

use super::review::{command_line, say};
use super::*;
use gpui::WindowHandle;

/// What the open picker would choose right now, rendered the way a test reads a row.
fn choice(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |ws, _window, cx| ws.picker_choice(cx))
        .unwrap()
}

/// How many rows the picker is offering, or `None` while none is open.
fn matches(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<usize> {
    window
        .update(cx, |ws, _window, cx| ws.picker_matches(cx))
        .unwrap()
}

/// The tree's keyboard row, as a vault path.
fn selected(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |ws, _window, _cx| {
            ws.tree_selection().map(|p| shale_core::vault::rel_text(&p))
        })
        .unwrap()
}

/// A vault with the note in one folder and somewhere to put it.
fn two_folders(cx: &mut TestAppContext) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    workspace(
        cx,
        &[
            "notes/Deep.md",
            "notes/deeper/Deepest.md",
            "other/keep.md",
            "top.md",
        ],
    )
}

#[gpui::test]
fn m_in_the_tree_moves_the_note_to_another_folder(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The tree opens with its folders closed, so `l` opens `notes` and the second
    // `j` lands on the note: the first `j` is the folder inside it.
    cx.simulate_keystrokes("ctrl-w h l j j");
    assert_eq!(
        selected(&window, cx).as_deref(),
        Some("notes/Deep.md"),
        "the note the reader stopped on"
    );

    cx.simulate_keystrokes("m");
    assert_eq!(
        matches(&window, cx),
        Some(3),
        "the top level, `other/`, and `notes/deeper/` — not the folder the note is in"
    );
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("move notes/Deep.md -> top level"),
        "the top level leads, because leaving a folder is the move the picker exists for"
    );

    cx.simulate_input("other");
    assert_eq!(matches(&window, cx), Some(1), "one folder answers");
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("move notes/Deep.md -> other"),
        "and it is the one the reader typed"
    );

    cx.simulate_keystrokes("enter");
    assert!(
        dir.path().join("other/Deep.md").is_file(),
        "the file is in the folder"
    );
    assert!(
        !dir.path().join("notes/Deep.md").exists(),
        "and not where it was"
    );
    assert_eq!(
        selected(&window, cx).as_deref(),
        Some("other/Deep.md"),
        "the tree's selection rides along, so `m` again still means the same note"
    );
    assert!(
        say(&window, cx).contains("moved to other/Deep.md"),
        "the status line says where it went: {}",
        say(&window, cx)
    );
}

#[gpui::test]
fn the_move_picker_skips_the_folder_the_note_is_already_in(cx: &mut TestAppContext) {
    let (_dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h j m");
    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert!(
        !rows.iter().any(|r| r.ends_with("-> notes")),
        "offering the note's own folder invites a refusal for a row that looked like \
         an answer: {rows:?}"
    );
}

#[gpui::test]
fn move_names_a_folder_from_the_vault_root(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open(&window, cx, "notes/Deep.md");
    command_line("move other/", cx);
    assert!(
        dir.path().join("other/Deep.md").is_file(),
        "the note kept its name and changed folders: {:?}",
        on_disk(dir.path())
    );
    assert!(
        !dir.path().join("notes/Deep.md").exists(),
        "and the old path is gone"
    );
}

/// A folder named after a note that exists is a mistake worth naming. Without the
/// guard the filesystem answers `File exists (os error 17)`, which reads like a bug
/// rather than like what the reader typed.
#[gpui::test]
fn a_folder_named_after_a_note_is_refused(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open(&window, cx, "notes/Deep.md");
    command_line("move top.md", cx);
    assert!(
        say(&window, cx).contains("is a note, not a folder"),
        "{}",
        say(&window, cx)
    );
    assert!(
        dir.path().join("notes/Deep.md").is_file(),
        "nothing moved: {:?}",
        on_disk(dir.path())
    );
}

#[gpui::test]
fn move_creates_a_folder_that_is_not_there(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open(&window, cx, "notes/Deep.md");
    command_line("move new/deep/", cx);
    assert!(
        dir.path().join("new/deep/Deep.md").is_file(),
        "the folders on the way are made, the same way a rename makes them"
    );
}

#[gpui::test]
fn a_bare_move_asks_with_the_picker(cx: &mut TestAppContext) {
    let (_dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open(&window, cx, "notes/Deep.md");
    command_line("move", cx);
    assert!(
        matches(&window, cx).is_some(),
        "no folder typed is a question, not a move to nowhere"
    );
}

/// Typing a folder that is not there is asking for one, not failing to find one.
#[gpui::test]
fn typing_a_folder_that_is_not_there_offers_to_create_it(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h l j j m");
    cx.simulate_input("new/deep");
    assert_eq!(matches(&window, cx), Some(1), "one row answers");
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("move notes/Deep.md -> new/deep"),
        "and it is the folder to create"
    );
    cx.simulate_keystrokes("enter");
    assert!(dir.path().join("new/deep/Deep.md").is_file());
}

/// The vault's top level is a destination like any other, and `/` is how it is
/// spelled on the command line: every empty part of a typed path is dropped, so
/// `/` leaves no folder at all.
#[gpui::test]
fn move_to_the_vault_root(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open(&window, cx, "notes/Deep.md");
    command_line("move /", cx);
    assert!(
        dir.path().join("Deep.md").is_file(),
        "the note is at the top level: {:?}",
        on_disk(dir.path())
    );
    assert!(
        !dir.path().join("notes/Deep.md").exists(),
        "and not in its folder: {:?}",
        on_disk(dir.path())
    );
}

/// Paint a frame, so an element that only exists after an update has bounds to find.
fn draw(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// The middle of a named element, in window px — the way a test puts the pointer on a
/// tree row without guessing where the layout put it.
fn centre(cx: &mut VisualTestContext, selector: &'static str) -> gpui::Point<gpui::Pixels> {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no element named {selector:?} on screen"));
    gpui::point(
        bounds.origin.x + bounds.size.width / 2.,
        bounds.origin.y + bounds.size.height / 2.,
    )
}

/// Pick up what is under the pointer and carry it to `to`, without letting go: the way
/// a test looks at a drag that is still in the air.
fn carry(
    cx: &mut VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
) {
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::default());
    // A drag starts on movement, not on the press: a press that never moves is a click.
    cx.simulate_mouse_move(
        gpui::point(from.x + gpui::px(6.), from.y + gpui::px(6.)),
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
}

/// Let go of whatever is being carried.
fn let_go(cx: &mut VisualTestContext, to: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::default());
}

/// Pick up what is under the pointer, carry it to `to`, and let go.
fn drag(
    cx: &mut VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
) {
    carry(cx, from, to);
    let_go(cx, to);
}

/// The same, between two named elements.
fn drag_between(cx: &mut VisualTestContext, from: &'static str, to: &'static str) {
    let from = centre(cx, from);
    let to = centre(cx, to);
    drag(cx, from, to);
}

/// The mouse answer to `m`: pick a note up out of the tree and drop it on a folder.
#[gpui::test]
fn dragging_a_note_onto_a_folder_row_moves_it(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The tree opens with its folders closed, so the note is only on screen once
    // `notes` is open.
    cx.simulate_keystrokes("ctrl-w h l");
    let from = centre(cx, "tree-row notes/Deep.md");
    let to = centre(cx, "tree-row other");
    carry(cx, from, to);
    assert!(
        window
            .update(cx, |_, _window, cx| cx.has_active_drag())
            .unwrap(),
        "the note is in the air, following the pointer"
    );
    let_go(cx, to);
    assert!(
        dir.path().join("other/Deep.md").is_file(),
        "the note landed in the folder it was dropped on: {:?}",
        on_disk(dir.path())
    );
    assert!(
        !dir.path().join("notes/Deep.md").exists(),
        "and not where it was: {:?}",
        on_disk(dir.path())
    );
    assert!(
        say(&window, cx).contains("moved to other/Deep.md"),
        "the status line says what the drag did: {}",
        say(&window, cx)
    );
}

/// A note row is a destination too: the folder that note sits in. That is the only way
/// to say "put it next to this one" with the mouse.
#[gpui::test]
fn dragging_a_note_onto_a_note_row_moves_it_into_that_folder(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    window
        .update(cx, |ws, _window, _cx| {
            ws.toggle_dir(std::path::PathBuf::from("other"))
        })
        .unwrap();
    draw(cx);
    drag_between(cx, "tree-row top.md", "tree-row other/keep.md");
    assert!(
        dir.path().join("other/top.md").is_file(),
        "the note joined the folder its neighbour lives in: {:?}",
        on_disk(dir.path())
    );
}

/// The tree's own background, below the last row, is the vault's top level.
#[gpui::test]
fn dropping_a_note_on_the_tree_background_moves_it_to_the_top_level(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h l");
    let panel = cx
        .debug_bounds("tree-scroll")
        .expect("the tree panel is on screen");
    let from = centre(cx, "tree-row notes/Deep.md");
    drag(
        cx,
        from,
        gpui::point(
            panel.origin.x + panel.size.width / 2.,
            panel.origin.y + panel.size.height - gpui::px(20.),
        ),
    );
    assert!(
        dir.path().join("Deep.md").is_file(),
        "the note left its folder: {:?}",
        on_disk(dir.path())
    );
}

/// A folder row does not lift: a folder cannot be moved, and a handle that always
/// fails to land is worse than no handle.
#[gpui::test]
fn a_folder_row_does_not_lift(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let from = centre(cx, "tree-row notes");
    let to = centre(cx, "tree-row other");
    carry(cx, from, to);
    assert!(
        !window
            .update(cx, |_, _window, cx| cx.has_active_drag())
            .unwrap(),
        "a folder never gets picked up: there is nothing in the air to drop"
    );
    let_go(cx, to);
    assert!(
        dir.path().join("notes/Deep.md").is_file(),
        "the folder and everything in it stayed put: {:?}",
        on_disk(dir.path())
    );
    assert!(
        !say(&window, cx).contains("moved"),
        "and nothing was claimed: {}",
        say(&window, cx)
    );
}

/// A drop on the folder the note already lives in changes nothing, and the status line
/// does not claim that it did.
#[gpui::test]
fn a_note_dropped_where_it_already_lives_says_nothing(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h l");
    let before = say(&window, cx);
    drag_between(cx, "tree-row notes/Deep.md", "tree-row notes");
    assert!(
        dir.path().join("notes/Deep.md").is_file(),
        "the note is where it always was: {:?}",
        on_disk(dir.path())
    );
    assert_eq!(
        say(&window, cx),
        before,
        "a drop where the note already lives says nothing"
    );
}

/// The point of a move is that the vault still reads afterwards. A link that spells
/// out the old folder has to follow; a link that still resolves is left alone.
#[gpui::test]
fn a_move_rewrites_the_links_that_spelled_the_old_folder(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(
        cx,
        &[
            ("notes/Deep.md", ""),
            ("other/keep.md", ""),
            ("b.md", "one [[notes/Deep]]\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open(&window, cx, "notes/Deep.md");
    command_line("move other/", cx);
    let after = std::fs::read_to_string(dir.path().join("b.md")).unwrap();
    assert!(
        !after.contains("notes/Deep"),
        "the old spelling is gone: {after}"
    );
    assert!(
        after.contains("[["),
        "and the link is still a link: {after}"
    );
    assert!(
        say(&window, cx).contains("1 link in 1 note"),
        "the status line counts what it changed: {}",
        say(&window, cx)
    );
}

/// The tab holding the note is the same tab, with the same unsaved state, under a
/// new path — a move is not a close and a reopen.
#[gpui::test]
fn the_open_note_follows_its_file(cx: &mut TestAppContext) {
    let (_dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("notes/Deep.md"), cx)
        })
        .unwrap();
    command_line("move other/", cx);

    let (_, active) = state(&window, cx);
    assert_eq!(
        active, "other/Deep.md",
        "the tab is still the note it was, with its new name"
    );
}

/// The jump chain named the note by its old path, and a chain that points at a note
/// that no longer resolves is a dead end the reader has to step over by hand. The
/// moved note is the one *behind* in the chain here, with no tab holding it: the tab
/// loop cannot save this, only the chain can.
#[gpui::test]
fn a_move_moves_the_jump_chain_with_the_note(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("notes/Deep.md", "[[other/keep]] links out\n"),
            ("other/keep.md", "stays put\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open(&window, cx, "notes/Deep.md");
    // The caret is at the start of the line, which is where the link starts, so
    // `g x` follows it and leaves Deep.md behind in the chain.
    cx.simulate_keystrokes("g x");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "other/keep.md", "the link was followed");

    // The tree opens on the note on screen; `k` steps up to the folder above the
    // note that is going to move, `l` opens it, `j` lands on that note.
    cx.simulate_keystrokes("ctrl-w h k l j");
    assert_eq!(
        selected(&window, cx).as_deref(),
        Some("notes/Deep.md"),
        "the note the chain is holding"
    );
    cx.simulate_keystrokes("m");
    cx.simulate_input("other");
    cx.simulate_keystrokes("enter");
    assert!(
        say(&window, cx).contains("moved to other/Deep.md"),
        "{}",
        say(&window, cx)
    );

    cx.simulate_keystrokes("ctrl-w l ctrl-o");
    let (_, active) = state(&window, cx);
    assert_eq!(
        active, "other/Deep.md",
        "`ctrl-o` follows the chain to the note's new folder"
    );
}

/// A folder's own links are its children's problem: the notes inside it spell out
/// the old folder name, and nothing rewrites those yet. Refusing beats moving the
/// folder and leaving the vault full of dead links.
#[gpui::test]
fn moving_a_folder_is_refused_until_the_links_inside_can_follow(cx: &mut TestAppContext) {
    let (dir, window) = two_folders(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h");
    assert_eq!(
        selected(&window, cx).as_deref(),
        Some("notes"),
        "the tree opens on its first row, a folder"
    );
    cx.simulate_keystrokes("m");
    assert!(
        say(&window, cx).contains("not built yet"),
        "{}",
        say(&window, cx)
    );
    assert!(
        matches(&window, cx).is_none(),
        "and no picker opens for a move that cannot be done"
    );
    assert!(dir.path().join("notes/Deep.md").is_file(), "nothing moved");
}

/// A move that would overwrite a note that is already there is refused, and the
/// note that was asked to move stays where it was.
#[gpui::test]
fn a_move_onto_a_name_that_is_taken_is_refused(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["notes/Deep.md", "other/Deep.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open(&window, cx, "notes/Deep.md");
    command_line("move other/", cx);
    assert!(
        say(&window, cx).contains("already exists"),
        "{}",
        say(&window, cx)
    );
    assert!(
        dir.path().join("notes/Deep.md").is_file(),
        "the note that was asked to move is still where it was"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("other/Deep.md")).unwrap(),
        "",
        "the note that was already there is untouched"
    );
}

/// The vault as it sits on disk, sorted, folders marked with a `/`.
fn on_disk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    walk(dir, 0, &mut out);
    out
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            out.push(format!(
                "{}{}/",
                " ".repeat(depth),
                e.file_name().to_string_lossy()
            ));
            walk(&e.path(), depth + 2, out);
        } else {
            out.push(format!(
                "{}{}",
                " ".repeat(depth),
                e.file_name().to_string_lossy()
            ));
        }
    }
}
