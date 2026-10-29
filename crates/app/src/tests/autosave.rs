//! Autosave: the note reaches the disk without being asked.
//!
//! The countdown is gpui's simulated clock, so these tests advance it rather than
//! sleeping — which is what makes them prove the *debounce* and not just the
//! write. The three ways out of a note are all here: the pause running out, the
//! window losing focus, and the window being closed. The `quit` *action* is not:
//! it predates autosave, still writes a dirty note first, and gpui's test platform
//! has no meaningful `quit()` to observe.

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use gpui::BackgroundExecutor;

use super::*;

/// The default pause, spelled out here so a test that waits for it says what it
/// is waiting for rather than trusting a constant in another file.
const PAUSE: Duration = Duration::from_millis(2000);

/// Open a note in the window, so there is something to save.
fn open(window: &WindowHandle<Workspace>, cx: &mut TestAppContext, name: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(name), cx))
        .unwrap();
}

/// What a note holds on disk.
fn on_disk(dir: &tempfile::TempDir, name: &str) -> String {
    std::fs::read_to_string(dir.path().join(name)).unwrap()
}

/// Is the buffer ahead of the disk, as the window sees it.
fn dirty(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |ws, _window, cx| ws.buffer_is_dirty(cx))
        .unwrap()
}

/// The status bar's save indicator.
fn dot(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.dirty_indicator(cx).to_string())
        .unwrap()
}

/// Makes the window the active one, the way the live app's window is.
///
/// gpui's test platform opens a window without activating it, and
/// [`VisualTestContext::deactivate_window`] only sends the blur to a window it
/// believes is active — so a test that wants a blur has to ask for the focus
/// first.
fn activate(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) {
    window
        .update(cx, |_ws, window, _cx| window.activate_window())
        .unwrap();
    cx.run_until_parked();
}

/// The simulated clock, taken from the `TestAppContext` before the window context
/// borrows it.
///
/// A pause is two steps, because gpui's test executor makes a `timer` ready
/// without running the task that was waiting on it: move the clock, then let the
/// task run. Moving it from inside an app borrow re-enters the app and panics, so
/// the executor is held here rather than fetched through the window.
struct Clock(BackgroundExecutor);

impl Clock {
    /// Let `pause` of simulated time pass, and run what that makes ready.
    fn pause(&self, cx: &mut VisualTestContext, pause: Duration) {
        self.0.advance_clock(pause);
        cx.run_until_parked();
    }
}

/// Type, stop, and the note is on disk a pause later with nothing pressed.
#[gpui::test]
fn a_pause_after_typing_writes_the_note(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    open(&window, cx, "a.md");
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("i");
    cx.simulate_input("typed while I was away");
    assert!(dirty(&window, cx), "the edits are only in memory so far");
    assert_eq!(on_disk(&dir, "a.md"), "");

    // Just under the pause: a write between two keystrokes would be a write in
    // the middle of a thought, and a burst of typing would become a write per
    // keystroke.
    clock.pause(cx, PAUSE - Duration::from_millis(100));
    assert_eq!(
        on_disk(&dir, "a.md"),
        "",
        "the pause has not run out, so nothing is written"
    );
    assert!(dirty(&window, cx));

    clock.pause(cx, Duration::from_millis(200));
    assert_eq!(on_disk(&dir, "a.md"), "typed while I was away");
    assert!(
        !dirty(&window, cx),
        "and the buffer knows it is on disk, so the dot goes out"
    );
}

/// Typing again restarts the countdown, and only the last pause produces a write.
#[gpui::test]
fn typing_again_restarts_the_countdown_and_writes_once(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    open(&window, cx, "a.md");
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("i");
    cx.simulate_input("first thought");
    clock.pause(cx, Duration::from_millis(1000));
    cx.simulate_input(" and the second");
    // The first pause would have expired by now if the edit had not restarted it.
    clock.pause(cx, Duration::from_millis(1000));
    assert_eq!(on_disk(&dir, "a.md"), "", "the countdown restarted");

    clock.pause(cx, Duration::from_millis(1100));
    assert_eq!(
        on_disk(&dir, "a.md"),
        "first thought and the second",
        "one write, holding the whole note"
    );
}

/// The dot is the autosave's whole report: lit while the note is ahead of the
/// disk, out once it is written.
#[gpui::test]
fn the_dirty_dot_lights_and_goes_out(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["a.md"]);
    open(&window, cx, "a.md");
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    assert_eq!(dot(&window, cx), "", "a note that is on disk says nothing");

    cx.simulate_keystrokes("i");
    cx.simulate_input("x");
    assert_eq!(dot(&window, cx), "●", "and an edit says so at once");

    clock.pause(cx, PAUSE + Duration::from_millis(100));
    assert_eq!(dot(&window, cx), "", "the write puts the bar back");
}

/// Losing focus writes the note at once, without waiting out the pause: the user
/// has gone somewhere else with the edits still only in memory.
#[gpui::test]
fn losing_focus_writes_the_note(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    open(&window, cx, "a.md");
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("i");
    cx.simulate_input("away before the pause");
    clock.pause(cx, Duration::from_millis(200));
    assert_eq!(on_disk(&dir, "a.md"), "", "the pause has not run out");

    activate(&window, cx);
    cx.deactivate_window();
    assert_eq!(
        on_disk(&dir, "a.md"),
        "away before the pause",
        "the blur is what wrote it"
    );
    assert!(!dirty(&window, cx));
}

/// With autosave off, Shale writes when told to and not otherwise — which is what
/// makes `ctrl-s` and `:w` worth keeping.
#[gpui::test]
fn autosave_off_leaves_the_note_to_its_owner(cx: &mut TestAppContext) {
    let mut config = Config::default();
    config.spellcheck.enabled = false;
    config.editor.autosave = false;
    let (dir, window) = workspace_with(cx, &["a.md"], config);
    open(&window, cx, "a.md");
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("i");
    cx.simulate_input("not yours to write");
    clock.pause(cx, PAUSE * 3);
    assert_eq!(on_disk(&dir, "a.md"), "", "off means off");
    assert!(dirty(&window, cx));

    // Losing focus does not write either: the setting is off, not slower.
    activate(&window, cx);
    cx.deactivate_window();
    assert_eq!(on_disk(&dir, "a.md"), "");

    // `ctrl-s` still does the job, through the keymap rather than an action call.
    cx.simulate_keystrokes("escape ctrl-s");
    assert_eq!(on_disk(&dir, "a.md"), "not yours to write");
    assert!(!dirty(&window, cx));
}

/// The window manager's close button never reaches the `quit` action, so the hook
/// `attach_window` registers is what writes a note that is ahead of the disk. This
/// is the case the debounce and the blur cannot cover.
#[gpui::test]
fn closing_the_window_writes_the_note(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    open(&window, cx, "a.md");
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("i");
    cx.simulate_input("closed mid-countdown");
    clock.pause(cx, Duration::from_millis(200));
    assert_eq!(on_disk(&dir, "a.md"), "", "the pause has not run out");

    assert!(
        cx.simulate_close(),
        "the note was written, so the window is allowed to go"
    );
    assert_eq!(on_disk(&dir, "a.md"), "closed mid-countdown");
}

/// A note that cannot be written keeps its window open, the way `:q` refuses to
/// quit: the alternative is a silent loss, and a window is where the user can
/// still see what went wrong.
#[gpui::test]
fn a_note_that_cannot_be_written_keeps_its_window(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    open(&window, cx, "a.md");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("i");
    cx.simulate_input("nowhere to write this");

    std::fs::set_permissions(
        dir.path().join("a.md"),
        std::fs::Permissions::from_mode(0o444),
    )
    .unwrap();

    assert!(
        !cx.simulate_close(),
        "the close is refused while the note is unwritten"
    );
    assert!(dirty(&window, cx), "and the dot stays lit");
}

/// The write is gated on there being something to write: opening a note is not an
/// edit, so a window that has only been looked at never rewrites the vault.
#[gpui::test]
fn a_note_that_was_not_touched_is_not_written(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    std::fs::write(dir.path().join("a.md"), "left alone\n").unwrap();
    open(&window, cx, "a.md");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The same entry point the blur hook uses, with nothing asking for a write.
    window
        .update(cx, |ws, _window, cx| ws.autosave_now(cx))
        .unwrap();
    assert_eq!(on_disk(&dir, "a.md"), "left alone\n");
    assert_eq!(dot(&window, cx), "");

    // And the close hook finds nothing to do either, so the window goes.
    assert!(cx.simulate_close());
}
