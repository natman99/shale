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

mod blocks;
mod keys;
mod motions;
mod paint;
mod picker;
mod tree;

/// A workspace over a throwaway vault holding `files`, with the note focused the
/// way `main` focuses it. The `TempDir` is returned so the caller can change what
/// is on disk and check what reached it.
pub(crate) fn workspace(
    cx: &mut TestAppContext,
    files: &[&str],
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
    let window =
        cx.add_window(|_window, cx| Workspace::new(vault.clone(), Rc::new(Config::default()), cx));
    window
        .update(cx, |ws, window, cx| {
            ws.focus_editor(&FocusEditor, window, cx);
        })
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
