//! The live MCP channel as a window runs it (roadmap P2.3): the lock it leaves,
//! what the port answers about the note on screen, and what a graceful exit takes
//! back with it.
//!
//! The channel is off in the shared test config — a bound port, a thread and a file
//! in the vault are side effects no keymap test wants — so these are the tests that
//! turn it on. `crate::live` covers the protocol and `shale_core::live` the contract;
//! what is covered here is the wiring between a window and them.

use super::*;
use shale_core::live::{get, probe, Lock, PATH_NOTE, PATH_STATUS};

/// A window with `[mcp] live` on leaves a lock naming a port, and that port answers
/// with the note the window is looking at.
#[gpui::test]
fn the_window_answers_what_it_is_showing(cx: &mut TestAppContext) {
    let mut config = Config::default();
    config.spellcheck.enabled = false;
    config.index.enabled = false;
    config.mcp.live = true;
    let (dir, window) = workspace_with(cx, &["One.md", "Two.md"], config);

    let lock = Lock::read(dir.path())
        .unwrap()
        .expect("a live window leaves a lock");
    assert_eq!(
        lock.pid,
        std::process::id(),
        "the lock names this process, which is what a probe is checked against"
    );
    assert_eq!(lock.version, env!("CARGO_PKG_VERSION"));

    let status = get(lock.port, &lock.token, PATH_STATUS).unwrap();
    assert_eq!(status["app_running"], true);
    assert_eq!(status["port"], lock.port);
    assert_eq!(status["overlay"], "none", "the note has the keyboard");

    // In the app the window's first frame publishes it; a publish asked for directly is
    // how a test says "now" without drawing a frame to get there.
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("One.md"), cx);
            ws.publish_live(cx);
        })
        .unwrap();

    let note = get(lock.port, &lock.token, PATH_NOTE).unwrap();
    assert_eq!(note["path"], "One.md");
    assert_eq!(note["view"], "live");
    assert_eq!(note["mode"], "normal");
    assert_eq!(note["cursor"]["row"], 0);

    // A graceful exit takes the port and the lock with it, which is what lets the
    // next `shale-mcp` tell "the window closed" from "a stale hint left behind".
    window
        .update(cx, |ws, _window, cx| ws.stop_live(cx))
        .unwrap();
    assert!(
        Lock::read(dir.path()).unwrap().is_none(),
        "the lock goes with the window"
    );
    assert!(
        probe(lock.port, &lock.token).is_none(),
        "and so does the answer"
    );
}

/// The answer is kept current by drawing: what the window drew is what the port
/// reports, on the frame that drew it, with no beat in between. And a window that
/// draws nothing publishes nothing — the idle-park contract reaches the live channel
/// too, where the old 250 ms beat cost four wakeups a second to learn that the user
/// had not moved.
#[gpui::test]
fn the_answer_follows_what_the_window_draws(cx: &mut TestAppContext) {
    let mut config = Config::default();
    config.spellcheck.enabled = false;
    config.index.enabled = false;
    config.mcp.live = true;
    let (dir, window) = workspace_with(cx, &["One.md"], config);
    let lock = Lock::read(dir.path())
        .unwrap()
        .expect("a live window leaves a lock");

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("One.md"), cx)
        })
        .unwrap();
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let before = window.update(cx, |ws, _window, _cx| ws.live_publishes()).unwrap();
    cx.simulate_keystrokes("i");
    cx.simulate_input("hello");
    // The frame that draws the edit is the publish. No timer, no beat.
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let note = get(lock.port, &lock.token, PATH_NOTE).unwrap();
    assert_eq!(note["mode"], "insert", "the mode travels too");
    assert_eq!(note["cursor"]["col"], 5);
    assert_eq!(note["dirty"], true, "the edit has not been written yet");
    assert!(
        window
            .update(cx, |ws, _window, _cx| ws.live_publishes())
            .unwrap()
            > before,
        "the frame that drew the edit published it"
    );

    // At rest the answer stops being re-made: nothing is republishing what nobody
    // changed. `updated_at` is epoch seconds, too coarse to show this, so the count
    // of publishes is what the test reads.
    let settled = window
        .update(cx, |ws, _window, _cx| ws.live_publishes())
        .unwrap();
    clock.pause(cx, Duration::from_millis(600));
    assert_eq!(
        window
            .update(cx, |ws, _window, _cx| ws.live_publishes())
            .unwrap(),
        settled,
        "the window sat still and published anyway"
    );

    window
        .update(cx, |ws, _window, cx| ws.stop_live(cx))
        .unwrap();
}

/// With the channel off, the window opens no port and leaves nothing in the vault —
/// including no `.shale/` for a test counting what a command created to trip over.
#[gpui::test]
fn a_window_with_the_channel_off_leaves_nothing_behind(cx: &mut TestAppContext) {
    let (dir, _window) = workspace(cx, &["One.md"]);

    assert!(Lock::read(dir.path()).unwrap().is_none());
    assert!(
        !dir.path().join(".shale").exists(),
        "no lock, and no directory to hold it"
    );
}
