//! The headless UI tests: a real `MarkdownEditor` in a gpui test window, driven
//! through the same keymap and input paths the live app uses.
//! `simulate_keystrokes` exercises the keys we bind to actions and
//! `simulate_input` the characters that reach the buffer through the input
//! handler, which is how an input-path regression gets caught without a live
//! window.

/// Draw frames *and* deliver the next-frame callbacks they schedule, which is what a
/// real window does and `draw().clear()` on its own does not. The keyboard and the hover
/// move to the tab on screen one frame after the tab does (`Workspace::focus_stale`), so
/// a test that presses a key straight after a tab change without this is pressing it
/// against a view that has already left the tree — the keystroke goes nowhere.
/// Type a `:` command the way a user does: escape out of whatever mode the editor is
/// in, open the command line, spell it, press enter.
pub(crate) fn type_command(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// Checker notes as the window would receive them from the server.
pub(crate) fn notes_from(notes: &[(&str, usize, usize)]) -> Vec<crate::lsp::Diagnostic> {
    notes
        .iter()
        .map(|(message, start, end)| crate::lsp::Diagnostic {
            start: *start,
            end: *end,
            message: message.to_string(),
            code: "SpellCheck".into(),
        })
        .collect()
}

/// Open a note by name, for fixtures whose vault holds more than one note.
pub(crate) fn show_at(cx: &mut TestAppContext, window: &WindowHandle<Workspace>, name: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(name), cx))
        .unwrap();
}

pub(crate) fn repaint(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        // Not through a window handle: the callback updates the workspace, and an
        // update of the workspace from inside one is a double borrow.
        cx.update(|window, cx| window.simulate_next_frame(cx));
    }
}

use super::*;
use crate::editor::MarkdownEditor;
// The slimmed `main.rs` no longer imports what the tests build windows from;
// they say so themselves now.
use crate::workspace::{keybindings, Workspace};
use gpui::{px, BackgroundExecutor, Focusable, TestAppContext, VisualTestContext, WindowHandle};
use shale_core::{Config, Vault};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

mod autosave;
mod blocks;
mod commands;
mod embeds;
mod exit;
mod focus;
mod graph;
mod index;
mod jumps;
mod keys;
mod live;
mod motions;
mod overlay;
mod paint;
mod panes;
mod picker;
mod popup;
mod queries;
mod quotes;
mod review;
mod search;
mod session;
mod sidebar;
mod spell;
mod start;
mod status;
mod tabs;
mod tree;
mod vaults;
mod views;
mod writing;

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
    //
    // The live MCP channel is off for a third reason: it binds a port, leaves a
    // thread per window, and writes `.shale/mcp.lock` into the vault, which is
    // exactly what a test counting what a command created is not expecting.
    // `tests::live` turns it back on, and it is on in the app.
    let mut config = test_config();
    config.index.enabled = false;
    workspace_with(cx, files, config)
}

/// The config a test window gets, and the reason it is not `Config::default()`.
///
/// The checker is off (no `harper-ls` per window), and so is the live MCP channel
/// (no port, no thread, no `.shale/mcp.lock` in the vault a test is counting). The
/// **theme is named** for a fourth reason: a vault that names no palette asks the
/// desktop, and that ask is a D-Bus round trip on a thread the test scheduler
/// refuses — `Your test is not deterministic`. Naming it also keeps the paint tests
/// on the palette they have always pinned; a test that means to look at colours
/// names its own through `workspace_with`.
pub(crate) fn test_config() -> Config {
    let mut config = Config::default();
    config.spellcheck.enabled = false;
    config.mcp.live = false;
    config.appearance.theme = "mocha".to_string();
    config
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
            ws.focus_active_tab(window, cx);
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
    let config = test_config();
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
