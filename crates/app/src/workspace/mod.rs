//! The window: the sidebar tree, the note, the status bar, and the vault edits
//! that connect them.
//!
//! Split into `keys.rs` (the keymap), `tree.rs` (the sidebar's keyboard state and
//! the rows it draws), `edits.rs` (the prompt and the vault edits behind its
//! answers) and `render.rs` (the frame). All four are descendants of `workspace`,
//! so they reach `Workspace`'s private state directly; anything one of them calls
//! on another is `pub(super)`.

mod edits;
mod keys;
mod render;
mod tree;

pub(crate) use keys::keybindings;

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{prelude::*, Context, Entity, SharedString, Subscription, Window};
use shale_core::theme::Palette;
use shale_core::vault::VaultEntry;
use shale_core::{Config, Vault};

use crate::acts::*;
use crate::editor::{EditorEvent, MarkdownEditor};
use crate::prompt;

/// The whole window's state: what is open, what the sidebar shows, and the editor
/// that holds the note.
pub(crate) struct Workspace {
    focus_handle: gpui::FocusHandle,
    vault: Rc<Vault>,
    #[allow(dead_code)]
    config: Rc<Config>,
    /// The palette the whole window is drawn with, shared with the editor so a
    /// theme change repaints everything at once.
    palette: Rc<Palette>,
    entries: Vec<VaultEntry>,
    /// The vault's folders, which is how an empty one still gets a tree row.
    dirs: Vec<VaultEntry>,
    expanded: HashSet<PathBuf>,
    active: Option<PathBuf>,
    /// The sidebar's own focus, so the tree can be driven from the keyboard.
    tree_focus: gpui::FocusHandle,
    /// The tree row the keyboard has selected.
    tree_selection: Option<PathBuf>,
    /// The rows the last frame drew, top to bottom, which is what `j`/`k` step
    /// through: a collapsed folder's children are skipped for free.
    tree_rows: Vec<PathBuf>,
    /// The open prompt, and what it is asking for.
    prompt: Option<(prompt::PromptKind, Entity<prompt::Prompt>)>,
    /// What had the keyboard when the prompt opened, so closing it puts the
    /// keyboard back where the user was working: a rename started from the tree
    /// leaves them in the tree.
    prompt_return_focus: Option<gpui::FocusHandle>,
    /// The subscription to the open prompt, replaced when another one opens.
    _prompt_events: Option<Subscription>,
    status: SharedString,
    editor: Entity<MarkdownEditor>,
    _editor_events: Subscription,
    _keystroke_observer: Subscription,
}

impl Workspace {
    pub(crate) fn new(vault: Rc<Vault>, config: Rc<Config>, cx: &mut Context<Self>) -> Self {
        let entries = vault.list_markdown().unwrap_or_default();
        let dirs = vault.list_dirs().unwrap_or_default();
        // Every folder starts open: the tree is the vault, not a browser.
        let expanded = dirs.iter().map(|e| e.path.clone()).collect();
        let editor = cx.new(|cx| MarkdownEditor::new(cx));
        let known = note_keys(&entries);
        editor.update(cx, |editor, cx| editor.set_known_links(known, cx));
        // The editor resolves `![[image.png]]` against the vault's attachments,
        // so it needs the vault itself and everything in it that is not a note.
        let attachments = vault.list_attachments().unwrap_or_default();
        editor.update(cx, |editor, cx| {
            editor.set_vault(vault.clone(), attachments, cx);
        });
        editor.update(cx, |editor, cx| {
            editor.set_font_family(&config.appearance.font_family, cx);
        });
        // The palette comes from `appearance.theme`: a built-in name or a
        // `<vault>/.shale/themes/<name>.toml` file. A theme that cannot be found
        // or parsed must not stop the editor from starting, so it is reported and
        // the default palette is used.
        let palette = match shale_core::theme::palette_for(vault.root(), &config.appearance.theme) {
            Ok(palette) => palette,
            Err(err) => {
                tracing::warn!("could not load the configured theme: {err:#}");
                Palette::DEFAULT
            }
        };
        let palette = Rc::new(palette);
        editor.update(cx, |editor, cx| {
            editor.set_palette(Rc::clone(&palette), cx);
            editor.set_font_size(config.appearance.font_size, cx);
        });
        let editor_events = cx.subscribe(&editor, |ws, _editor, event, cx| match event {
            EditorEvent::FollowLink { target } => ws.follow_link(target, cx),
        });
        // Keybinding bugs on Linux are almost always "what did the platform
        // actually deliver, and which action did it resolve to", so log both —
        // at trace level, since this fires for every keystroke and floods the
        // console at debug. Enable with RUST_LOG=shale=trace.
        // The subscription has to be kept alive or the observer is dropped.
        let keystroke_observer = cx.observe_keystrokes(|_ws, event, _window, _cx| {
            tracing::trace!(
                key = event.keystroke.key.as_str(),
                modifiers = ?event.keystroke.modifiers,
                action = event.action.as_ref().map(|action| action.name()),
                contexts = ?event.context_stack,
                "keystroke"
            );
        });
        Self {
            focus_handle: cx.focus_handle(),
            vault,
            config,
            palette,
            entries,
            dirs,
            expanded,
            active: None,
            tree_focus: cx.focus_handle(),
            tree_selection: None,
            tree_rows: Vec::new(),
            prompt: None,
            prompt_return_focus: None,
            _prompt_events: None,
            status: "".into(),
            editor,
            _editor_events: editor_events,
            _keystroke_observer: keystroke_observer,
        }
    }

    pub(crate) fn open_note(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        match self.vault.read(path) {
            Ok(text) => {
                tracing::debug!(path = %path.display(), bytes = text.len(), "opened note");
                self.active = Some(path.clone());
                self.editor
                    .update(cx, |editor, cx| editor.set_text(text, cx));
                self.status = path.display().to_string().into();
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), "failed to read note: {err:#}");
                self.status = format!("{err:#}").into();
            }
        }
        cx.notify();
    }

    /// Open the note a wiki-link target refers to, if it exists.
    fn follow_link(&mut self, target: &str, cx: &mut Context<Self>) {
        match self.vault.resolve_link(target, &self.entries) {
            Ok(Some(entry)) => {
                let path = entry.path.clone();
                tracing::debug!(target, path = %path.display(), "following wiki-link");
                self.open_note(&path, cx);
            }
            Ok(None) => {
                tracing::debug!(target, "unresolved wiki-link");
                self.status = format!("no note named {target}").into();
                cx.notify();
            }
            Err(err) => {
                tracing::warn!(target, "failed to resolve link: {err:#}");
                self.status = format!("{err:#}").into();
                cx.notify();
            }
        }
    }

    pub(super) fn toggle_dir(&mut self, path: PathBuf) {
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
    }
    pub(super) fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.active.clone() else {
            self.status = "no file open".into();
            cx.notify();
            return;
        };
        let text = self.editor.read_with(cx, |editor, _cx| editor.text());
        match self.vault.write(&path, &text) {
            Ok(()) => {
                self.editor.update(cx, |editor, _cx| editor.mark_clean());
                self.status = format!("saved {}", path.display()).into();
            }
            Err(err) => {
                self.status = format!("{err:#}").into();
            }
        }
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn tree_rows(&self) -> Vec<PathBuf> {
        self.tree_rows.clone()
    }

    #[cfg(test)]
    pub(crate) fn tree_selection(&self) -> Option<PathBuf> {
        self.tree_selection.clone()
    }

    #[cfg(test)]
    pub(crate) fn active_path(&self) -> Option<PathBuf> {
        self.active.clone()
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> String {
        self.status.to_string()
    }

    /// The mode the status bar shows, which is how a test checks that the prompt
    /// really did take the keyboard.
    #[cfg(test)]
    pub(crate) fn mode_label(&self, cx: &gpui::App) -> &'static str {
        mode_label(self.editor.read(cx).mode())
    }

    /// What the open prompt holds, or `None` while no prompt is open.
    #[cfg(test)]
    pub(crate) fn prompt_text(&self, cx: &gpui::App) -> Option<String> {
        self.prompt
            .as_ref()
            .map(|(_, prompt)| prompt.read(cx).text().to_string())
    }
}

/// Lowercased link targets that resolve to an existing note: file stems and
/// vault-relative paths (with and without the `.md` extension).
fn note_keys(entries: &[VaultEntry]) -> HashSet<String> {
    let mut keys = HashSet::new();
    for entry in entries {
        keys.insert(entry.stem().to_lowercase());
        if let Some(s) = entry.path.to_str() {
            let lower = s.to_lowercase();
            keys.insert(lower.clone());
            if let Some(stripped) = lower.strip_suffix(".md") {
                keys.insert(stripped.to_string());
            }
        }
    }
    keys
}

/// The mode as the status bar shows it.
pub(super) fn mode_label(mode: shale_core::vim::Mode) -> &'static str {
    match mode {
        shale_core::vim::Mode::Normal => "NORMAL",
        shale_core::vim::Mode::Insert => "INSERT",
        shale_core::vim::Mode::Visual => "VISUAL",
        shale_core::vim::Mode::VisualLine => "V-LINE",
    }
}
