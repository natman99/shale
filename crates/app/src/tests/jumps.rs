//! The jump chain: `ctrl-o` back to the note the user came from, `ctrl-i` forward
//! again. The chain itself is [`crate::workspace::jumps`]'s business; what is
//! checked here is the wiring through the window — that following a link records
//! the place being left, that coming back lands on the same byte of it, and that
//! the chain follows the vault when a note is deleted.

use super::*;

/// A note with a wiki-link on its second line. The link does not start at the
/// start of the line, so a chain that remembered only the line would not be
/// caught by these tests.
const A: &str = "line one\nsee [[b]] here\nline three\n";

/// The byte offset of the `[[b]]` span in [`A`].
const LINK: usize = 13;

/// Where the cursor is in the open note, as a buffer byte offset.
fn cursor(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| ws.editor().read(cx).cursor_offset())
        .unwrap()
}

/// What the window said about the last jump attempt.
fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// How far the chain reaches: `(back, forward)`.
fn lens(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    window
        .update(cx, |ws, _window, _cx| ws.jump_lens())
        .unwrap()
}

/// The open note, or `""` while none is.
fn active(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    state(window, cx).1
}

/// How many rows the picker is offering, or `None` while none is open.
fn picker_rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<usize> {
    window
        .update(cx, |ws, _window, cx| ws.picker_matches(cx))
        .unwrap()
}

/// A two-note vault, with `a.md` open at its first byte.
fn two_notes(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(cx, &[("a.md", A), ("b.md", "the linked note\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    open_a(&mut cx, &window);
    (dir, window, cx)
}

/// Opening `a.md` the way quick-open does, with the cursor on the link.
fn open_a(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) {
    cx.simulate_keystrokes("space f enter");
    assert_eq!(active(window, cx), "a.md", "the top row is `a.md`");
    cx.simulate_keystrokes("j l l l l");
    assert_eq!(
        cursor(window, cx),
        LINK,
        "the cursor is on the `[[b]]` span"
    );
}

/// Following a wiki-link is a jump, and `ctrl-o` returns to the exact byte of the
/// link the user was reading. `ctrl-i` walks back out along the same path.
#[gpui::test]
fn a_link_then_ctrl_o_returns_to_the_same_byte(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x");
    assert_eq!(active(&window, cx), "b.md", "`g x` followed the link");
    assert_eq!(
        lens(&window, cx),
        (1, 0),
        "and left one place to come back to"
    );

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(active(&window, cx), "a.md", "`ctrl-o` came back");
    assert_eq!(cursor(&window, cx), LINK, "on the byte of the link");
    assert_eq!(
        lens(&window, cx),
        (0, 1),
        "and the note just left is now ahead"
    );

    cx.simulate_keystrokes("ctrl-i");
    assert_eq!(active(&window, cx), "b.md", "`ctrl-i` went forward again");
    assert_eq!(cursor(&window, cx), 0, "where it was left");
    assert_eq!(lens(&window, cx), (1, 0), "and the chain flipped back");
}

/// Choosing somewhere new makes the jumps the user stepped back over stale, so
/// `ctrl-i` has nothing to offer afterwards — the same shape a browser's forward
/// history has after a new page is visited.
#[gpui::test]
fn a_new_destination_drops_the_forward_half(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x ctrl-o");
    assert_eq!(active(&window, cx), "a.md");
    assert_eq!(lens(&window, cx), (0, 1), "one jump ahead of us");

    cx.simulate_keystrokes("space f");
    cx.simulate_input("b");
    cx.simulate_keystrokes("enter");
    assert_eq!(active(&window, cx), "b.md", "a new destination");
    assert_eq!(lens(&window, cx), (1, 0), "which dropped the forward half");

    cx.simulate_keystrokes("ctrl-i");
    assert_eq!(
        status(&window, cx),
        "no newer jump",
        "so `ctrl-i` says there is nowhere newer to go"
    );
    assert_eq!(active(&window, cx), "b.md", "and nothing moved");
}

/// `ctrl-o` before the user has gone anywhere is a question with no answer: the
/// status bar says so, and the note and cursor stay put.
#[gpui::test]
fn a_jump_back_with_nothing_behind_it_says_so(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;
    let before = cursor(&window, cx);

    cx.simulate_keystrokes("ctrl-o ctrl-i");
    assert_eq!(
        status(&window, cx),
        "no newer jump",
        "the last of the two is the one that reported"
    );
    assert_eq!(active(&window, cx), "a.md", "neither moved the note");
    assert_eq!(cursor(&window, cx), before, "or the cursor");
    assert_eq!(lens(&window, cx), (0, 0), "and nothing was recorded");
}

/// Re-opening the note that is already open is not a jump. The disk-reload path
/// opens the same path too, so without this the chain would fill with the note
/// the user never left.
#[gpui::test]
fn re_opening_the_open_note_records_nothing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("space f enter");
    assert_eq!(active(&window, cx), "a.md", "the same note opened again");
    assert_eq!(lens(&window, cx), (0, 0), "and the chain did not grow");
}

/// A deleted note must not stay in the chain, and the chain must still work with
/// nothing on screen: `ctrl-o` is the press the user reaches for the moment the
/// note under them disappears.
#[gpui::test]
fn a_deleted_note_leaves_the_chain(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    // a -> b -> a, so the chain behind the open note holds both notes. The
    // `space` that arms the chord is vim's right motion, so it moves the cursor
    // one byte along in `b.md` first — which is exactly the place the chain has
    // to remember.
    cx.simulate_keystrokes("space f");
    cx.simulate_input("b");
    cx.simulate_keystrokes("enter");
    assert_eq!(active(&window, cx), "b.md");
    cx.simulate_keystrokes("space f");
    let left = cursor(&window, cx);
    assert_eq!(left, 1, "the chord's `space` moved the cursor right");
    cx.simulate_input("a");
    cx.simulate_keystrokes("enter");
    assert_eq!(active(&window, cx), "a.md");
    assert_eq!(lens(&window, cx), (2, 0), "both notes are behind us");

    cx.simulate_keystrokes("ctrl-shift-d");
    cx.simulate_input("yes");
    cx.simulate_keystrokes("enter");
    assert_eq!(active(&window, cx), "", "the open note is gone");
    assert_eq!(
        lens(&window, cx),
        (1, 0),
        "and its entries went with it, leaving the other note"
    );

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        active(&window, cx),
        "b.md",
        "so `ctrl-o` lands on the note that still exists"
    );
    assert_eq!(cursor(&window, cx), left, "where it was left");
    assert_eq!(
        lens(&window, cx),
        (0, 0),
        "and nothing was pushed ahead, because there was no note to leave"
    );
}

/// A rename re-points the chain instead of leaving it aimed at a path that no
/// longer resolves.
#[gpui::test]
fn a_rename_moves_the_chain_with_the_note(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x");
    assert_eq!(active(&window, cx), "b.md");

    // The note behind us is `a.md`; renaming it must not strand the jump. The
    // tree opens on the open note, so `k` walks the selection up to the other one.
    cx.simulate_keystrokes("ctrl-w h k r");
    // The field starts with the current stem, so only the change is typed.
    cx.simulate_input("lpha");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        status(&window, cx),
        "renamed to alpha.md",
        "the rename went through"
    );

    cx.simulate_keystrokes("ctrl-w l ctrl-o");
    assert_eq!(
        active(&window, cx),
        "alpha.md",
        "`ctrl-o` follows the note to its new name"
    );
    assert_eq!(cursor(&window, cx), LINK, "and to the byte it was left at");
}

/// The jump keys are bound with no key context, so they arrive while a picker is
/// open. The handler has to stand down, or the note under the picker would change
/// while the user is still choosing.
#[gpui::test]
fn the_jump_keys_stand_down_while_an_overlay_is_open(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x");
    assert_eq!(active(&window, cx), "b.md");

    cx.simulate_keystrokes("space f");
    assert_eq!(picker_rows(&window, cx), Some(2), "the picker is up");

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        picker_rows(&window, cx),
        Some(2),
        "`ctrl-o` did not close it"
    );
    assert_eq!(
        active(&window, cx),
        "b.md",
        "and did not change the note underneath it"
    );
}
