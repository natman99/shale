//! The blank tab's start block (`workspace::start`): the front door of a pane
//! that is not standing on a note. What is in the vault, what was open last,
//! and the keys out -- and the block is gone the moment the tab starts being a
//! note, so the writing surface is never behind a menu.

use shale_core::session::Sessions;

use super::*;

/// A vault with two notes and a window that is not standing on either.
fn blank(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(cx, &[("a.md", "first\n"), ("b.md", "second\n")]);
    let cx = VisualTestContext::from_window(window.into(), cx);
    (dir, window, cx)
}

/// Whether pane `pix` is showing the block.
fn block(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, pix: usize) -> bool {
    window
        .update(cx, |ws, _window, cx| ws.show_start_block(pix, cx))
        .unwrap()
}

/// The rows the block would offer, in the order it would offer them.
fn offered(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<PathBuf> {
    window
        .update(cx, |ws, _window, cx| ws.start_recents(cx))
        .unwrap()
}

/// Open a note through the door the picker, the tree and `gd` all use.
fn open(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>, note: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(note), cx))
        .unwrap();
}

fn notes_in(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
    pix: usize,
) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| {
            ws.pane_tab_paths(pix, cx)
                .into_iter()
                .map(|p| p.map(|p| p.display().to_string()).unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .unwrap()
}

/// The whole point of the feature: a pane with nothing open is a page with
/// doors, not a void. And a pane that *is* standing on a note has no business
/// offering a start screen.
#[gpui::test]
fn a_blank_tab_offers_a_start_block_and_a_note_tab_does_not(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = blank(cx);
    assert!(
        block(&window, &mut cx, 0),
        "a blank tab is a front door, not a void"
    );
    open(&mut cx, &window, "a.md");
    assert!(
        !block(&window, &mut cx, 0),
        "a note tab has no start screen in front of it"
    );
}

/// The block never stands between a thought and the page: insert mode takes it
/// away before a character is written, and it does not come back while the tab
/// holds text. It does come back when the tab is blank again -- the block
/// follows the tab's state, it is not a one-way latch.
#[gpui::test]
fn the_block_steps_aside_the_moment_the_tab_starts_being_a_note(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = blank(cx);
    assert!(block(&window, &mut cx, 0));

    cx.simulate_keystrokes("i");
    assert!(
        !block(&window, &mut cx, 0),
        "insert mode is the tab becoming a note"
    );

    cx.simulate_keystrokes("h escape");
    assert!(
        !block(&window, &mut cx, 0),
        "a tab with text in it is a note, blank or not"
    );

    cx.simulate_keystrokes("u");
    assert!(
        block(&window, &mut cx, 0),
        "undo back to nothing and the door is there again"
    );
}

/// Recents are the notes that were in hand, newest first, and re-opening one
/// moves it to the front instead of listing it twice.
#[gpui::test]
fn the_notes_in_hand_are_the_ones_offered(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = blank(cx);
    assert!(
        offered(&window, &mut cx).is_empty(),
        "a vault with no history yet offers no rows"
    );

    open(&mut cx, &window, "a.md");
    open(&mut cx, &window, "b.md");
    assert_eq!(
        offered(&window, &mut cx),
        vec![PathBuf::from("b.md"), PathBuf::from("a.md")]
    );

    open(&mut cx, &window, "a.md");
    assert_eq!(
        offered(&window, &mut cx),
        vec![PathBuf::from("a.md"), PathBuf::from("b.md")],
        "the list is what was last read, not the order they were created"
    );
}

/// Every row is a promise that clicking lands somewhere. A note deleted from
/// disk drops out of the menu rather than offering to open a missing file.
#[gpui::test]
fn a_recent_that_is_no_longer_on_disk_is_not_offered(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = blank(cx);
    open(&mut cx, &window, "a.md");
    open(&mut cx, &window, "b.md");
    std::fs::remove_file(dir.path().join("a.md")).unwrap();
    window
        .update(&mut cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();

    assert_eq!(offered(&window, &mut cx), vec![PathBuf::from("b.md")]);
}

/// The list belongs to the vault, not the window: a second window over the
/// same vault starts with the first one's history, because "what was I just
/// reading" is a fact about the vault.
#[gpui::test]
fn the_recent_list_is_the_vaults_and_not_the_windows(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", "first\n"), ("b.md", "second\n")]);
    for note in ["a.md", "b.md"] {
        window
            .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(note), cx))
            .unwrap();
    }
    window
        .update(cx, |ws, _window, cx| ws.save_session(cx))
        .unwrap();

    // The file, as the next window will find it.
    let on_disk = Sessions::load(dir.path()).unwrap();
    assert_eq!(
        on_disk.recent,
        vec![PathBuf::from("b.md"), PathBuf::from("a.md")],
        "the list rides in the session file"
    );

    let mut config = test_config();
    config.spellcheck.enabled = false;
    config.index.enabled = false;
    let two = open_window(cx, dir.path(), config);
    let recents = two
        .update(cx, |ws, _window, cx| ws.start_recents(cx))
        .unwrap();
    assert_eq!(
        recents,
        vec![PathBuf::from("b.md"), PathBuf::from("a.md")],
        "a second window on the vault does not start cold"
    );
}

/// A row names its own pane: clicking a recent in a pane without the keyboard
/// opens the note *there*, the same rule the tab bar's rows follow.
#[gpui::test]
fn a_recent_clicked_in_a_pane_opens_in_that_pane(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = blank(cx);
    open(&mut cx, &window, "a.md");
    cx.simulate_keystrokes("ctrl-w v");
    assert_eq!(panes_of(&window, &mut cx), 2, "a split to click in");
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, _cx| ws.active_pane_ix())
            .unwrap(),
        1,
        "the split took the keyboard, so pane 0 is the one without it"
    );

    // The click is in pane 0: the note opens there, not in the pane that has
    // the keys.
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_recent_in(0, &PathBuf::from("b.md"), cx);
        })
        .unwrap();

    assert_eq!(
        notes_in(&window, &mut cx, 0),
        vec!["b.md".to_string()],
        "the note opened where it was clicked"
    );
    assert_eq!(
        notes_in(&window, &mut cx, 1),
        vec!["".to_string()],
        "the other pane is untouched by a click it did not receive"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, _cx| ws.active_pane_ix())
            .unwrap(),
        0,
        "and the keyboard went with the click"
    );
}

fn panes_of(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, _cx| ws.pane_count())
        .unwrap()
}
