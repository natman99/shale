//! Tabs (roadmap P3.1): what a tab is, what the keys and the command line do to the
//! bar, and what an exit owes a note that is not on screen.
//!
//! The behaviour these pin is the behaviour the design chose over the cheaper one:
//! **the editor belongs to the tab**, so a tab keeps its cursor, its scroll, its
//! folds, its view and its undo history, and a quit owes every note on the bar rather
//! than the one under the cursor.

use super::*;
use shale_core::session::{Jump, Session, Tab};

/// The note in tab `index`, and whether it is ahead of its file.
fn tabs(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<(String, bool)> {
    window
        .update(cx, |ws, _window, cx| {
            ws.tab_paths(cx)
                .into_iter()
                .map(|(path, dirty)| {
                    (
                        path.map(|path| path.display().to_string())
                            .unwrap_or_else(|| "—".to_string()),
                        dirty,
                    )
                })
                .collect()
        })
        .unwrap()
}

/// Which tab is on screen.
fn marker(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, _cx| ws.tab_index())
        .unwrap()
}

/// The buffer of the tab on screen.
fn buffer(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap()
}

/// The status line.
fn say(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// Open a note through the same door the tree and the picker use. Generic over the
/// test context because the helpers that build a window have a `TestAppContext` and
/// the ones that drive keys have a `VisualTestContext`.
fn show<C: gpui::AppContext>(cx: &mut C, window: &WindowHandle<Workspace>, note: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(note), cx))
        .unwrap();
}

/// Type a command line and press enter.
fn command_line(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// A vault with two notes, `a.md` open in the window's first tab.
fn two_notes(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(cx, &[("a.md", "first\n"), ("b.md", "second\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    (dir, window, cx)
}

/// `ctrl-t` puts the note in hand in a second tab, and the new tab is the one on
/// screen: two tabs on one note is the "read the top of this while I edit the bottom
/// of it" case, and it is why a tab's identity is an id rather than its path.
#[gpui::test]
fn ctrl_t_opens_the_note_in_hand_again(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    assert_eq!(tabs(&window, &mut cx), vec![("a.md".into(), false)]);
    cx.simulate_keystrokes("ctrl-t");
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), false), ("a.md".into(), false)],
        "the bar has the note twice"
    );
    assert_eq!(marker(&window, &mut cx), 1, "and the new one is on screen");
    assert_eq!(buffer(&window, &mut cx), "first\n", "loaded from disk");
}

/// `g t` and `g T` walk the tabs, wrapping, and the buffer that comes with them is the
/// tab's own.
#[gpui::test]
fn g_t_and_g_capital_walk_the_tabs(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-t");
    show(&mut cx, &window, "b.md");
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), false), ("b.md".into(), false)],
        "opening a note fills the tab in hand rather than adding another"
    );

    cx.simulate_keystrokes("g T");
    assert_eq!(marker(&window, &mut cx), 0);
    assert_eq!(buffer(&window, &mut cx), "first\n");
    // Wrapping, because a tab bar is a handful of rows and counting them is worse.
    cx.simulate_keystrokes("g T");
    assert_eq!(marker(&window, &mut cx), 1);
    cx.simulate_keystrokes("g t");
    assert_eq!(marker(&window, &mut cx), 0);
}

/// The reason the editor belongs to the tab rather than to the window: the cursor and
/// the undo history are the tab's, so switching tabs cannot undo another note's work
/// and cannot move a cursor the user left somewhere else.
#[gpui::test]
fn a_tab_keeps_its_own_cursor_and_undo(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    // Type in the first tab.
    cx.simulate_keystrokes("A");
    cx.simulate_input(" typed");
    assert_eq!(buffer(&window, &mut cx), "first typed\n");

    cx.simulate_keystrokes("ctrl-t");
    show(&mut cx, &window, "b.md");
    // The new tab is a different note with its own buffer, and its own undo stack:
    // `u` here has nothing to undo.
    cx.simulate_keystrokes("escape u");
    assert_eq!(
        buffer(&window, &mut cx),
        "second\n",
        "undo in the other tab did not touch this one"
    );

    // And the first tab still holds what was typed in it, unsaved.
    cx.simulate_keystrokes("g T");
    assert_eq!(buffer(&window, &mut cx), "first typed\n");
    assert!(
        tabs(&window, &mut cx)[0].1,
        "and the bar says it is ahead of its file"
    );
}

/// Opening a note the bar already holds goes to that tab instead of putting the note
/// on the bar twice — Obsidian's rule, and the one that stops following three links
/// from making three tabs.
#[gpui::test]
fn opening_an_open_note_finds_its_tab(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-t");
    show(&mut cx, &window, "b.md");
    show(&mut cx, &window, "a.md");
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), false), ("b.md".into(), false)],
        "no third tab, and the second still holds b.md"
    );
    assert_eq!(marker(&window, &mut cx), 0, "and the frame moved");
}

/// `:tabclose` writes the note before the tab goes. A tab the user closed is not a
/// wish to lose the note, and a write that fails keeps the tab open.
#[gpui::test]
fn closing_a_tab_writes_its_note(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("A");
    cx.simulate_input(" saved");
    command_line("tabclose", &mut cx);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "first saved\n",
        "the note reached the disk"
    );
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("—".into(), false)],
        "and the tab went"
    );
}

/// Closing the only tab leaves an empty tab rather than an empty window: the window
/// keeps its at-least-one-tab invariant, which is what lets every part of it ask for
/// "the note" and get an answer, and the user keeps a place to type.
#[gpui::test]
fn closing_the_last_tab_leaves_an_empty_one(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    command_line("tabclose", &mut cx);
    assert_eq!(tabs(&window, &mut cx), vec![("—".into(), false)]);
    assert_eq!(buffer(&window, &mut cx), "");
    assert!(say(&window, &mut cx).contains("no note open"));
    // And the note is still the window's to open again.
    cx.simulate_keystrokes("i typed");
    assert_eq!(buffer(&window, &mut cx), "typed");
}

/// `:tabonly` closes the rest, and each of *their* notes is written on the way out by
/// the same rule `:tabclose` follows. The tab that stays is the one the user is looking
/// at: `:tabonly` is not a save, so its dot stays lit.
#[gpui::test]
fn tab_only_closes_the_rest(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-t");
    show(&mut cx, &window, "b.md");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" saved");
    // A second note made dirty in the tab that is about to be closed away.
    cx.simulate_keystrokes("escape g T");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" x");
    cx.simulate_keystrokes("escape g t");
    command_line("tabonly", &mut cx);
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("b.md".into(), true)],
        "one tab, still ahead of its file"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "first x\n",
        "the tab that went was written on the way out"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.md")).unwrap(),
        "second\n",
        "the tab that stayed was not"
    );
}

/// A quit owes every tab, not the one under the cursor: with autosave off, a note
/// dirty in the background is enough to refuse `:q`, because the user chose to save by
/// hand and the choice belongs to that note too.
#[gpui::test]
fn quit_refuses_a_dirty_background_tab(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    // The choice the test is about: the user saves by hand, so a quit may not do it
    // for them.
    window
        .update(&mut cx, |ws, _window, _cx| ws.set_autosave(false))
        .unwrap();
    cx.simulate_keystrokes("A");
    cx.simulate_input(" unsaved");
    cx.simulate_keystrokes("ctrl-t");
    command_line("q", &mut cx);
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), true), ("a.md".into(), false)],
        "the window is still up, with the bar as it was"
    );
    assert!(
        window
            .update(&mut cx, |ws, _window, cx| ws.any_tab_is_dirty(cx))
            .unwrap(),
        "and the note is still unsaved, not written on the way out"
    );
    assert!(
        say(&window, &mut cx).contains(":q!"),
        "and the bar names the way out: {}",
        say(&window, &mut cx)
    );
}

/// The other half of that promise: with autosave on, a quit writes every tab, so a
/// note edited in the background is not lost to the exit.
#[gpui::test]
fn quit_writes_every_tab_when_autosave_is_on(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", "first\n"), ("b.md", "second\n")]);
    window
        .update(cx, |ws, _window, _cx| ws.set_autosave(true))
        .unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" saved");
    cx.simulate_keystrokes("ctrl-t");
    command_line("q", &mut cx);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "first saved\n",
        "the background tab was written on the way out"
    );
}

/// The autosave debounce belongs to the tab: typing in one tab must not cancel another
/// tab's pending write, or the note the user left would sit unwritten until something
/// else happened to touch it.
#[gpui::test]
fn a_tab_switch_does_not_cancel_another_tabs_write(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", "first\n"), ("b.md", "second\n")]);
    window
        .update(cx, |ws, _window, _cx| ws.set_autosave(true))
        .unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    let clock = Clock(cx.executor());
    cx.simulate_keystrokes("A");
    cx.simulate_input(" saved");
    // Switch away before the countdown fires, then let it fire.
    cx.simulate_keystrokes("ctrl-t");
    clock.pause(&mut cx, Duration::from_millis(2100));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "first saved\n",
        "the tab that was left behind still wrote itself"
    );
    assert!(
        !tabs(&window, &mut cx)[0].1,
        "and its dot went out on the bar"
    );
}

/// Losing the window is the other half of "the user stopped typing", and it owes every
/// tab: the note the user left behind on the bar is not a note they stopped caring
/// about. `autosave_every_tab` is what the blur hook calls.
#[gpui::test]
fn a_blur_owes_every_tab(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", "first\n"), ("b.md", "second\n")]);
    window
        .update(cx, |ws, _window, _cx| ws.set_autosave(true))
        .unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" one");
    cx.simulate_keystrokes("ctrl-t");
    show(&mut cx, &window, "b.md");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" two");
    window
        .update(&mut cx, |ws, _window, cx| ws.autosave_every_tab(cx))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "first one\n",
        "the tab that was left behind"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.md")).unwrap(),
        "second two\n",
        "and the one that had the keyboard"
    );
}

/// The session remembers the tabs, in order, with the view each was in — a tab keeps
/// its own view, which is what the per-tab editor makes free.
#[gpui::test]
fn a_session_reopens_every_tab(cx: &mut TestAppContext) {
    let session = Session {
        open_note: Some(PathBuf::from("b.md")),
        cursor: None,
        scroll_y: None,
        expanded: vec![],
        view: Some("raw".into()),
        focus: None,
        focus_scope: None,
        typewriter: None,
        jumps_back: vec![],
        jumps_forward: vec![],
        tabs: vec![
            Tab {
                path: Some(PathBuf::from("a.md")),
                view: "live".into(),
                graph: false,
                camera: None,
            },
            Tab {
                path: Some(PathBuf::from("b.md")),
                view: "preview".into(),
                graph: false,
                camera: None,
            },
        ],
        active_tab: Some(1),
        graph_filter: None,
    };
    let (_dir, window) =
        session::vault_with_session(cx, &[("a.md", "first\n"), ("b.md", "second\n")], &session);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), false), ("b.md".into(), false)],
        "both tabs came back, in order"
    );
    assert_eq!(marker(&window, &mut cx), 1, "and the one with the keyboard");
    assert_eq!(buffer(&window, &mut cx), "second\n");
    // Preview takes edits, which is how the view is checked without a test hook.
    cx.simulate_keystrokes("i typed");
    assert_eq!(
        buffer(&window, &mut cx),
        "second\n",
        "the restored tab came back in preview"
    );
    cx.simulate_keystrokes("g T");
    cx.simulate_keystrokes("i typed");
    assert_eq!(
        buffer(&window, &mut cx),
        "typedfirst\n",
        "and the other one did not"
    );
}

/// A tab whose note was deleted while Shale was closed is not a tab any more: it is
/// dropped, and the window still has the one it started with.
#[gpui::test]
fn a_session_tab_whose_note_is_gone_is_dropped(cx: &mut TestAppContext) {
    let session = Session {
        open_note: Some(PathBuf::from("gone.md")),
        cursor: None,
        scroll_y: None,
        expanded: vec![],
        view: Some("live".into()),
        focus: None,
        focus_scope: None,
        typewriter: None,
        jumps_back: vec![Jump {
            path: Some(PathBuf::from("a.md")),
            cursor: 0,
        }],
        jumps_forward: vec![],
        tabs: vec![
            Tab {
                path: Some(PathBuf::from("gone.md")),
                view: "live".into(),
                graph: false,
                camera: None,
            },
            Tab {
                path: Some(PathBuf::from("a.md")),
                view: "live".into(),
                graph: false,
                camera: None,
            },
        ],
        active_tab: Some(0),
        graph_filter: None,
    };
    let (_dir, window) = session::vault_with_session(cx, &[("a.md", "first\n")], &session);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert_eq!(tabs(&window, &mut cx), vec![("a.md".into(), false)]);
    assert_eq!(marker(&window, &mut cx), 0);
}

/// `:tabmove` is the one tab command with a number, and the number is where the tab
/// goes: the tab bar gets ordered without dragging anything.
#[gpui::test]
fn tab_move_reorders_the_bar(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-t");
    show(&mut cx, &window, "b.md");
    command_line("tabmove 1", &mut cx);
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("b.md".into(), false), ("a.md".into(), false)],
        "the tab in hand moved to the front"
    );
    assert_eq!(
        marker(&window, &mut cx),
        0,
        "and it is still the one on screen"
    );
}

/// One tab is not a thing to walk, and saying so is better than a key that appears to
/// do nothing.
#[gpui::test]
fn walking_one_tab_says_so(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("g t");
    assert!(say(&window, &mut cx).contains("only one tab"));
    assert_eq!(marker(&window, &mut cx), 0);
}

/// The title the window manager was given. It is written while the frame is drawn,
/// so asking for it means drawing a frame first. Unlike the other helpers here it
/// needs no window handle: the title lives on the test window, not on the workspace.
fn title(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.window_title()
}

/// The window is titled for its vault and the note on screen, and follows the tab
/// that is on screen. This is what `alt-tab` and a window list have to say about the
/// window, so two vaults open at once have to be tellible apart without one.
#[gpui::test]
fn the_window_is_titled_for_its_vault_and_note(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", "first\n"), ("notes/Deep Note.md", "deep\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let vault = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    show(&mut cx, &window, "a.md");
    assert_eq!(
        title(&mut cx).as_deref(),
        Some(format!("{vault} — a").as_str()),
        "the vault, then the note"
    );

    // The note's own name: not its folder, not its extension, because the title is
    // not a path and the tab bar names the same note the same way.
    show(&mut cx, &window, "notes/Deep Note.md");
    assert_eq!(
        title(&mut cx).as_deref(),
        Some(format!("{vault} — Deep Note").as_str())
    );

    // The graph tab has no note, and the title says *which window this is* rather
    // than which view of it happens to be up, so it goes back to the vault alone.
    cx.simulate_keystrokes("space g");
    assert_eq!(title(&mut cx).as_deref(), Some(vault.as_str()));

    // Walking back to the note brings its name back: the title tracks the tab on
    // screen, not the last note that was opened. `escape` is the graph's way out,
    // and it lands on the note tab the frame was last on.
    cx.simulate_keystrokes("escape");
    assert_eq!(
        title(&mut cx).as_deref(),
        Some(format!("{vault} — Deep Note").as_str())
    );
}

/// A window with nothing to name is named for its vault alone: no "no note", and no
/// program name, which tells anyone with two vaults open nothing.
#[gpui::test]
fn an_empty_window_is_titled_for_its_vault(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &[]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let vault = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(title(&mut cx).as_deref(), Some(vault.as_str()));
}
