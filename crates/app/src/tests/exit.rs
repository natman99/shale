//! Leaving the session under a termination signal (roadmap P1.5).
//!
//! The signal handlers themselves belong to `main` — a test process that installed
//! a SIGTERM handler of its own would be testing the kernel — but the *watch* is
//! the window's, and a test can hand it a flag of its own and raise it. What is
//! observable is the promise the exit makes: the dirty note reaches the disk and
//! the buffer stops being dirty. The `cx.quit()` at the end of the sequence is a
//! no-op on the test platform (see `tests::autosave`), so the window stays usable
//! afterwards, which is exactly what lets the assertions run.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::*;

/// The poll interval in `workspace::exit`, spelled out here so the wait says what
/// it is waiting for rather than trusting a constant in another file.
const POLL: Duration = Duration::from_millis(200);

/// Open a note, type into it, and leave the window dirty.
fn dirty_note(
    dir: &tempfile::TempDir,
    window: &WindowHandle<Workspace>,
    cx: &mut TestAppContext,
) -> Clock {
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx)
        })
        .unwrap();
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window((*window).into(), cx);
    cx.simulate_keystrokes("i");
    cx.simulate_input("a thought worth keeping");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "",
        "autosave has not had its pause, so the edit is only in memory"
    );
    clock
}

/// A raised flag is answered with the note on the disk: the whole point of
/// catching the signal instead of dying from it.
#[gpui::test]
fn a_termination_signal_writes_the_dirty_note(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    let clock = dirty_note(&dir, &window, cx);

    let flag = Arc::new(AtomicBool::new(false));
    window
        .update(cx, |ws, _window, cx| {
            ws.watch_for_termination(Arc::clone(&flag), cx)
        })
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The flag goes up *after* the watch starts, the way a `kill` arrives in the
    // middle of a session rather than before it.
    flag.store(true, Ordering::SeqCst);
    clock.pause(cx, POLL + Duration::from_millis(50));

    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "a thought worth keeping",
        "the exit wrote what the user could see"
    );
    let clean = window
        .update(cx, |ws, _window, cx| !ws.buffer_is_dirty(cx))
        .unwrap();
    assert!(clean, "and the buffer knows it is on disk");
}

/// A signal with no note open has nothing to write, and the exit goes ahead
/// anyway — the watch must not wedge the window on an empty save path.
#[gpui::test]
fn a_termination_with_no_note_open_still_exits(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["a.md"]);
    let clock = Clock(cx.executor());

    let flag = Arc::new(AtomicBool::new(false));
    window
        .update(cx, |ws, _window, cx| {
            ws.watch_for_termination(Arc::clone(&flag), cx)
        })
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    flag.store(true, Ordering::SeqCst);
    clock.pause(cx, POLL + Duration::from_millis(50));

    // The window still answers: the watch ran, broke out of its loop, and left
    // nothing half-done behind.
    let open = window
        .update(cx, |ws, _window, _cx| ws.active_path().is_none())
        .unwrap();
    assert!(open, "no note was open before or after");
}
