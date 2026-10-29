//! The headless UI tests: a real `MarkdownEditor` in a gpui test window, driven
//! through the same keymap and input paths the live app uses.
//! `simulate_keystrokes` exercises the keys we bind to actions and
//! `simulate_input` the characters that reach the buffer through the input
//! handler, which is how an input-path regression gets caught without a live
//! window.

use super::*;
use crate::editor::MarkdownEditor;
use crate::workspace::{keybindings, Workspace};
use gpui::{Focusable, TestAppContext, VisualTestContext, WindowHandle};

mod autosave;
mod blocks;
mod commands;
mod keys;
mod motions;
mod paint;
mod picker;
mod popup;
mod search;
mod spell;
mod tree;

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
    let mut config = Config::default();
    config.spellcheck.enabled = false;
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
    let vault = Rc::new(Vault::open(dir.path()).unwrap());
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
