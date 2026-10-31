//! The headless UI tests: a real `MarkdownEditor` in a gpui test window, driven
//! through the same keymap and input paths the live app uses.
//! `simulate_keystrokes` exercises the keys we bind to actions and
//! `simulate_input` the characters that reach the buffer through the input
//! handler, which is how an input-path regression gets caught without a live
//! window.

use super::*;
use crate::editor::MarkdownEditor;
use crate::workspace::{keybindings, Workspace};
use gpui::{BackgroundExecutor, Focusable, TestAppContext, VisualTestContext, WindowHandle};
use std::path::Path;
use std::time::Duration;

mod autosave;
mod blocks;
mod commands;
mod exit;
mod focus;
mod index;
mod jumps;
mod keys;
mod motions;
mod paint;
mod picker;
mod popup;
mod quotes;
mod search;
mod session;
mod spell;
mod status;
mod tree;
mod views;

/// The simulated clock, taken from the `TestAppContext` before a window context
/// borrows it.
///
/// A pause is two steps, because gpui's test executor makes a `timer` ready
/// without running the task that was waiting on it: move the clock, then let the
/// task run. Moving it from inside an app borrow re-enters the app and panics, so
/// the executor is held here rather than fetched through the window.
pub(crate) struct Clock(BackgroundExecutor);

impl Clock {
    /// Let `pause` of simulated time pass, and run what that makes ready.
    pub(crate) fn pause(&self, cx: &mut VisualTestContext, pause: Duration) {
        self.0.advance_clock(pause);
        cx.run_until_parked();
    }
}

/// A workspace over a throwaway vault holding `files`, with the note focused the
/// way `main` focuses it. The `TempDir` is returned so the caller can change what
/// is on disk and check what reached it.
pub(crate) fn workspace(
    cx: &mut TestAppContext,
    files: &[&str],
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    // The default config asks for a spell and grammar server, which is a real
    // process per window. The tests here are about the window, not the checker
    // (`tests::spell` and `crate::lsp` cover that), and a `harper-ls` per test is
    // hundreds of megabytes of other people's dictionaries.
    //
    // The vault index is off for the same reason: a DB file and a writer thread per
    // window is not what a keymap or a paint test is about. `tests::index` turns it
    // back on, and it is on in the app.
    let mut config = Config::default();
    config.spellcheck.enabled = false;
    config.index.enabled = false;
    workspace_with(cx, files, config)
}

/// The same, with the config the caller chose: how a test turns autosave off, or
/// gives the window a theme. Whatever is passed, the caller owns the checker
/// setting — a real `harper-ls` per window is hundreds of megabytes of
/// dictionaries, so a test that does not mean to test the checker turns it off.
pub(crate) fn workspace_with(
    cx: &mut TestAppContext,
    files: &[&str],
    config: Config,
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let dir = tempfile::tempdir().unwrap();
    for name in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, "").unwrap();
    }
    let window = open_window(cx, dir.path(), config);
    (dir, window)
}

/// The window both helpers end up with: the vault over `dir`, the keymap bound, and
/// the note focused the way `main` focuses it.
fn open_window(cx: &mut TestAppContext, dir: &Path, config: Config) -> WindowHandle<Workspace> {
    let vault = Rc::new(Vault::open(dir).unwrap());
    cx.update(|cx| cx.bind_keys(keybindings()));
    let window = cx.add_window(|_window, cx| Workspace::new(vault.clone(), Rc::new(config), cx));
    window
        .update(cx, |ws, window, cx| {
            // The same call `main` makes, so the tests see the window the live app
            // has, hover included.
            ws.attach_window(window, cx);
            ws.focus_editor(&FocusEditor, window, cx);
        })
        .unwrap();
    window
}

/// A window that **does** keep a vault index, over a vault holding `notes` with
/// real text in them.
///
/// The notes are written before the window exists, so the startup reconciliation
/// pass has them to find rather than racing with them; the wait afterwards is that
/// pass finishing, since it reads the vault on a background thread.
///
/// The `TempDir` is returned so the caller can change what is on disk and check
/// what the index made of it.
pub(crate) fn indexed_vault(
    cx: &mut TestAppContext,
    notes: &[(&str, &str)],
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let mut config = Config::default();
    config.spellcheck.enabled = false;
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in notes {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
    }
    let window = open_window(cx, dir.path(), config);
    cx.run_until_parked();
    (dir, window)
}

/// A vault whose notes hold real text. `workspace` makes empty files, which is
/// enough for the tree and the keymap but not for anything that reads a note; the
/// new files are folded in the way the vault watcher folds a change made outside
/// Shale.
pub(crate) fn vault_with(
    cx: &mut TestAppContext,
    notes: &[(&str, &str)],
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let (dir, window) = workspace(cx, &[]);
    for (name, text) in notes {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
    }
    window
        .update(cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();
    (dir, window)
}

/// The workspace's state, read out of the window: the tree rows and the note that
/// is open.
pub(crate) fn state(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
) -> (Vec<PathBuf>, String) {
    window
        .update(cx, |ws, _window, _cx| {
            (
                ws.tree_rows(),
                ws.active_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            )
        })
        .unwrap()
}
