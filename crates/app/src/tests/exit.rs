//! Leaving the session under a termination signal (roadmap P1.5).
//!
//! The signal handlers themselves belong to `main` — a test process that installed
//! a SIGTERM handler of its own would be testing the kernel — but the *wait* is a
//! plain gpui task, and a test can build a [`Termination`] of its own, spawn the
//! watcher, and wake it: the same send the self-pipe reader makes when a signal
//! lands. What is observable is the promise the exit makes: the dirty note reaches
//! the disk and the buffer stops being dirty. The `cx.quit()` at the end of the
//! sequence is a no-op on the test platform (see `tests::autosave`), so the window
//! stays usable afterwards, which is exactly what lets the assertions run.
//!
//! The windows here come through the vault registry (`open_vault_window`), because
//! that is the walk the watcher takes: a `kill` answers for every window the
//! process holds, and only the registry knows them all.

use super::*;
use crate::workspace::exit::{self, Termination};
use crate::workspace::vaults::{self, OpenOptions};

/// A throwaway vault named `name` with one empty note, the checker, index and
/// live MCP off (the same fixture reasons as `tests::vaults`), and the registry
/// and recent-store globals in place. Returns the temp dir that owns the tree,
/// the vault path, and the wake-up pair a test spawns the watcher with.
fn registry_vault(
    cx: &mut TestAppContext,
    name: &str,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    Termination,
    exit::TerminationWatcher,
) {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join(name);
    write_vault(&dir);
    let (termination, watcher) = Termination::new();
    cx.update(|cx| {
        cx.bind_keys(keybindings());
        cx.set_global(vaults::VaultWindows::new());
        cx.set_global(vaults::RecentStore(base.path().join("recent.json")));
    });
    (base, dir, termination, watcher)
}

/// The vault on disk: one empty note, and the checker, index and live MCP off
/// so the run is about the exit, not about a `harper-ls` or a DB file.
fn write_vault(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join(".shale")).unwrap();
    std::fs::write(dir.join("a.md"), "").unwrap();
    std::fs::write(
        dir.join(".shale").join("config.toml"),
        "[spellcheck]\nenabled = false\n[index]\nenabled = false\n[mcp]\nlive = false\n",
    )
    .unwrap();
}

/// Open `a.md` in `window`, type into it, and leave the window dirty.
fn dirty_note(vault: &std::path::Path, window: &WindowHandle<Workspace>, cx: &mut TestAppContext) {
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx)
        })
        .unwrap();
    let cx = &mut VisualTestContext::from_window((*window).into(), cx);
    cx.simulate_keystrokes("i");
    cx.simulate_input("a thought worth keeping");
    assert_eq!(
        std::fs::read_to_string(vault.join("a.md")).unwrap(),
        "",
        "autosave has not had its pause, so the edit is only in memory"
    );
}

/// A raised flag is answered with the note on the disk: the whole point of
/// catching the signal instead of dying from it.
#[gpui::test]
fn a_termination_signal_writes_the_dirty_note(cx: &mut TestAppContext) {
    let (_base, vault, termination, watcher) = registry_vault(cx, "alpha");
    let window = cx
        .update(|cx| vaults::open_vault_window(&vault, OpenOptions::default(), cx))
        .unwrap();
    dirty_note(&vault, &window, cx);

    cx.update(|cx| exit::spawn_watcher(watcher, cx));

    // The wake lands *after* the watcher is up, the way a `kill` arrives in the
    // middle of a session rather than before it. No clock advance: the wake is
    // a message, not a timer, and the watcher answers it in the same breath.
    termination.wake();
    cx.run_until_parked();

    assert_eq!(
        std::fs::read_to_string(vault.join("a.md")).unwrap(),
        "a thought worth keeping",
        "the exit wrote what the user could see"
    );
    // And the window is gone: `leave` closed it, which is the other half of
    // the promise a signal gets. A handle that no longer resolves *is* the
    // teardown having happened.
    let gone = window
        .update(cx, |ws, _window, _cx| ws.active_path().is_none())
        .is_err();
    assert!(gone, "and the window closed");
}

/// A signal with no note open has nothing to write, and the exit goes ahead
/// anyway — the watch must not wedge the window on an empty save path.
#[gpui::test]
fn a_termination_with_no_note_open_still_exits(cx: &mut TestAppContext) {
    let (_base, vault, termination, watcher) = registry_vault(cx, "alpha");
    let window = cx
        .update(|cx| vaults::open_vault_window(&vault, OpenOptions::default(), cx))
        .unwrap();
    cx.update(|cx| exit::spawn_watcher(watcher, cx));
    termination.wake();
    cx.run_until_parked();

    // The exit went ahead: the watch ran, saved nothing because there was
    // nothing to save, and closed the window anyway.
    let gone = window
        .update(cx, |ws, _window, _cx| ws.active_path().is_none())
        .is_err();
    assert!(gone, "the empty window closed like any other");
}

/// A `kill` that lands before the watcher is spawned is not lost: the flag is
/// the fact, and the watcher reads it before it ever sleeps. This is the
/// startup race — a signal during the first second of a run, when the channel
/// send had no task waiting on the other end.
#[gpui::test]
fn a_termination_raised_before_the_watcher_starts_is_still_answered(cx: &mut TestAppContext) {
    let (_base, vault, termination, watcher) = registry_vault(cx, "alpha");
    let window = cx
        .update(|cx| vaults::open_vault_window(&vault, OpenOptions::default(), cx))
        .unwrap();
    dirty_note(&vault, &window, cx);

    termination.wake();
    cx.update(|cx| exit::spawn_watcher(watcher, cx));
    cx.run_until_parked();

    assert_eq!(
        std::fs::read_to_string(vault.join("a.md")).unwrap(),
        "a thought worth keeping",
        "the late watcher still read the flag and wrote the note"
    );
}

/// The watcher walks the whole registry, not the window that started the run:
/// two vaults, two windows, both dirty, and a single wake writes **both**
/// notes — asserted by which file holds which words, not by a count.
#[gpui::test]
fn one_termination_writes_every_window(cx: &mut TestAppContext) {
    let (_base, a, termination, watcher) = registry_vault(cx, "alpha");
    let b = _base.path().join("beta");
    write_vault(&b);
    let wa = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    let wb = cx
        .update(|cx| vaults::open_vault_window(&b, OpenOptions::default(), cx))
        .unwrap();
    dirty_note(&a, &wa, cx);
    wb.update(cx, |ws, _window, cx| {
        ws.open_note(&PathBuf::from("a.md"), cx)
    })
    .unwrap();
    {
        let cx = &mut VisualTestContext::from_window(*wb, cx);
        cx.simulate_keystrokes("i");
        cx.simulate_input("and a second thought");
    }

    cx.update(|cx| exit::spawn_watcher(watcher, cx));
    termination.wake();
    cx.run_until_parked();

    assert_eq!(
        std::fs::read_to_string(a.join("a.md")).unwrap(),
        "a thought worth keeping",
        "the first window's note reached the disk"
    );
    assert_eq!(
        std::fs::read_to_string(b.join("a.md")).unwrap(),
        "and a second thought",
        "and so did the second window's, unasked"
    );
}
