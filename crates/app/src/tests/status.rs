//! How long a message stays in the status bar.
//!
//! The bar's other segments are *state* — the mode, the save dot, the checker's
//! count, the search's readout — and each is true for exactly as long as it is on
//! screen, so it clears itself by changing. A message is different: it is a reply
//! to something the user just did, and it goes stale. These tests ask that it
//! stops being shown when it stops being news, and that the state segments outlive
//! it.

use super::*;
use crate::workspace::STATUS_TTL;

/// A window with one note open, so there is a command to run and a bar to read.
fn window_with_note(cx: &mut TestAppContext) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let (dir, window) = workspace(cx, &["a.md"]);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx);
        })
        .unwrap();
    (dir, window)
}

/// A window with autosave off, for the test that waits out the status countdown.
/// The countdown is 8 s and the save debounce is 2 s, so a test that advances one
/// advances the other too — and a note that got written on its own no longer has
/// an unsaved dot to check on.
fn window_that_keeps_its_edits(
    cx: &mut TestAppContext,
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let mut config = Config::default();
    config.spellcheck.enabled = false;
    config.editor.autosave = false;
    let (dir, window) = workspace_with(cx, &["a.md"], config);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx);
        })
        .unwrap();
    (dir, window)
}

/// Type a command line and press enter.
fn type_command(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// The status line, where a command reports what it did.
fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// A message is worth reading once, not for the rest of the session. This is the
/// complaint the countdown answers: `:typewriter off` reported itself and then
/// went on reporting itself while the user did other things.
#[gpui::test]
fn a_status_message_goes_away_on_its_own(cx: &mut TestAppContext) {
    let (_dir, window) = window_with_note(cx);
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("typewriter on", cx);
    assert!(
        status(&window, cx).contains("typewriter scroll on"),
        "the command answers itself: {:?}",
        status(&window, cx)
    );

    // Half the wait, and it is still there: the user has to be able to read it.
    clock.pause(cx, STATUS_TTL / 2);
    assert!(
        status(&window, cx).contains("typewriter scroll on"),
        "still readable halfway through: {:?}",
        status(&window, cx)
    );

    clock.pause(cx, STATUS_TTL);
    assert_eq!(
        status(&window, cx),
        "",
        "the bar stops reporting what the user already read"
    );
}

/// Two commands in a row: the second message gets its own full wait, and the
/// first one's countdown does not clear it. Replacing the parked task is what
/// makes that true — leave the old countdown running and the second message
/// disappears at the first message's deadline.
#[gpui::test]
fn a_newer_message_outlives_the_countdown_it_arrived_under(cx: &mut TestAppContext) {
    let (_dir, window) = window_with_note(cx);
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("typewriter on", cx);
    clock.pause(cx, STATUS_TTL / 4);
    type_command("w", cx);
    assert!(
        status(&window, cx).contains("saved"),
        "the second command replaced the first: {:?}",
        status(&window, cx)
    );

    // Now at three quarters of the way through the second message's wait, which
    // is past the first one's deadline entirely. Without the sequence number this
    // is where the older countdown wipes out a message it knows nothing about.
    clock.pause(cx, STATUS_TTL * 3 / 4);
    assert!(
        status(&window, cx).contains("saved"),
        "the older countdown did not clear it: {:?}",
        status(&window, cx)
    );

    clock.pause(cx, STATUS_TTL);
    assert_eq!(status(&window, cx), "", "and its own countdown does");
}

/// Clearing the message clears the message. The mode and the save dot are state,
/// and they stay until what they report changes.
#[gpui::test]
fn the_state_segments_outlive_the_message(cx: &mut TestAppContext) {
    let (_dir, window) = window_that_keeps_its_edits(cx);
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("typewriter on", cx);
    cx.simulate_keystrokes("i x");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.mode_label(cx))
            .unwrap(),
        "INSERT",
        "typing says so in the bar"
    );
    assert!(
        !window
            .update(cx, |ws, _window, cx| ws.dirty_indicator(cx))
            .unwrap()
            .is_empty(),
        "and so does the unsaved dot"
    );

    clock.pause(cx, STATUS_TTL);
    assert_eq!(status(&window, cx), "", "the message is gone");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.mode_label(cx))
            .unwrap(),
        "INSERT",
        "the mode is not"
    );
    assert!(
        !window
            .update(cx, |ws, _window, cx| ws.dirty_indicator(cx))
            .unwrap()
            .is_empty(),
        "nor is the dot"
    );
}
