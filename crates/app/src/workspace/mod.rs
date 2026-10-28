//! The window: the sidebar tree, the note, the status bar, and the vault edits
//! that connect them.
//!
//! Split into `keys.rs` (the keymap), `tree.rs` (the sidebar's keyboard state and
//! the rows it draws), `edits.rs` (the prompt and the vault edits behind its
//! answers) and `render.rs` (the frame). All four are descendants of `workspace`,
//! so they reach `Workspace`'s private state directly; anything one of them calls
//! on another is `pub(super)`.

pub(crate) mod commands;
mod edits;
mod keys;
mod open;
mod render;
mod tree;

pub(crate) use keys::keybindings;
pub(crate) use open::PickerTarget;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui::{prelude::*, Context, Entity, SharedString, Subscription, Window};
use shale_core::theme::Palette;
use shale_core::vault::VaultEntry;
use shale_core::watch::{Stamp, VaultChanges};
use shale_core::{Config, Vault};

use crate::acts::*;
use crate::editor::{EditorEvent, MarkdownEditor};
use crate::lsp::{absolute, Diagnostic, Spell};
use crate::picker;
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
    /// The spell and grammar checker, when one is configured and was started.
    /// It owns a server process; the window only talks to it.
    spell: Option<Entity<Spell>>,
    /// The checker's notes on the open note, kept here so the status bar can
    /// count them without reading the checker every frame.
    spell_notes: usize,
    /// The subscription that carries the checker's answers to the editor.
    _spell_events: Option<Subscription>,
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
    /// What every path in the vault looked like the last time the disk was
    /// checked, which is what the next check explains the difference to.
    signature: BTreeMap<PathBuf, Stamp>,
    /// The subscription to the open prompt, replaced when another one opens.
    _prompt_events: Option<Subscription>,
    /// The open picker, if one is open: quick-open, the command palette or a
    /// vault search. Every row carries what it stands for, so there is no side
    /// table to keep in step with a list that changes on each keystroke.
    picker: Option<Entity<picker::Picker<PickerTarget>>>,
    /// What had the keyboard when the picker opened, so closing it puts the
    /// keyboard back where the user was working.
    picker_return_focus: Option<gpui::FocusHandle>,
    /// The subscription to the open picker, replaced when another one opens.
    _picker_events: Option<Subscription>,
    status: SharedString,
    editor: Entity<MarkdownEditor>,
    _editor_events: Subscription,
    _keystroke_observer: Subscription,
}

impl Workspace {
    pub(crate) fn new(vault: Rc<Vault>, config: Rc<Config>, cx: &mut Context<Self>) -> Self {
        let entries = vault.list_markdown().unwrap_or_default();
        let dirs = vault.list_dirs().unwrap_or_default();
        let signature = vault.signature().unwrap_or_default();
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
            // The buffer moved, so the checker's copy of the note is out of date.
            // `Spell::changed` debounces, so a typed word is one request.
            EditorEvent::Changed => ws.send_note_to_checker(cx),
        });
        // Spell and grammar checking runs in a language server (`harper-ls` by
        // default). A server that is not installed, or will not start, is a
        // missing feature rather than a broken editor: it is reported and the
        // window carries on without one.
        let (spell, spell_notes, spell_events) = if config.spellcheck.enabled {
            match Spell::start(&config.spellcheck, cx) {
                Ok(spell) => {
                    // The checker notifies when it has new answers; those become
                    // the editor's wavy underlines.
                    let events =
                        cx.observe(&spell, |ws, spell, cx| ws.show_checker_notes(spell, cx));
                    (Some(spell), 0, Some(events))
                }
                Err(err) => {
                    tracing::warn!("no spellcheck: {err:#}");
                    (None, 0, None)
                }
            }
        } else {
            (None, 0, None)
        };
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
            spell,
            spell_notes,
            _spell_events: spell_events,
            entries,
            dirs,
            expanded,
            active: None,
            tree_focus: cx.focus_handle(),
            tree_selection: None,
            tree_rows: Vec::new(),
            prompt: None,
            prompt_return_focus: None,
            picker: None,
            picker_return_focus: None,
            _picker_events: None,
            signature,
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
                    .update(cx, |editor, cx| editor.set_text(text.clone(), cx));
                self.checker_opened(path, &text, cx);
                self.status = path.display().to_string().into();
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), "failed to read note: {err:#}");
                self.status = format!("{err:#}").into();
            }
        }
        cx.notify();
    }

    /// Open the note and put the cursor on `line` (1-based, the way a search hit
    /// reports it), scrolling it into view: the row the user was reading in the
    /// picker becomes the row the editor shows them.
    pub(crate) fn open_note_at(&mut self, path: &PathBuf, line: usize, cx: &mut Context<Self>) {
        self.open_note(path, cx);
        // Only if the note really opened — a read error leaves `active` alone.
        if self.active.as_deref() == Some(path.as_path()) {
            self.editor.update(cx, |editor, cx| {
                editor.reveal_line(line.saturating_sub(1), cx)
            });
        }
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

    /// The editor entity. Only the tests ask for it, to look at what the window
    /// handed the checker.
    #[cfg(test)]
    pub(crate) fn editor(&self) -> &Entity<MarkdownEditor> {
        &self.editor
    }

    /// The checker's notes on the open note, as the editor draws them.
    #[cfg(test)]
    pub(crate) fn spell_note_count(&self) -> usize {
        self.spell_notes
    }

    /// Whether a checker process is running. Only the tests ask: the window's
    /// behaviour must not depend on the answer, and this is how that is checked.
    #[cfg(test)]
    pub(crate) fn has_checker(&self) -> bool {
        self.spell.is_some()
    }

    pub(super) fn toggle_dir(&mut self, path: PathBuf) {
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
    }

    // ----- spell and grammar checking ------------------------------------

    /// Tell the checker which note the editor now holds. It keeps one document
    /// open at a time, so this replaces whatever it was last looking at. The path
    /// is vault-relative here and absolute by the time it becomes a URI, which is
    /// what the server matches its answers back to.
    fn checker_opened(&mut self, path: &Path, text: &str, cx: &mut Context<Self>) {
        let Some(spell) = self.spell.clone() else {
            return;
        };
        let path = absolute(self.vault.root(), path);
        spell.update(cx, |spell, cx| spell.open(&path, text, cx));
    }

    /// The editor no longer holds a note, so the checker has nothing to say.
    fn checker_closed(&mut self, cx: &mut Context<Self>) {
        let Some(spell) = self.spell.clone() else {
            return;
        };
        spell.update(cx, |spell, cx| spell.close(cx));
    }

    /// The buffer changed, so the checker's copy of the note is out of date. It
    /// debounces, which is what keeps typing a word at one request per word
    /// rather than one per character.
    fn send_note_to_checker(&mut self, cx: &mut Context<Self>) {
        let Some(spell) = self.spell.clone() else {
            return;
        };
        let text = self.editor.read_with(cx, |editor, _cx| editor.text());
        spell.update(cx, |spell, cx| spell.changed(&text, cx));
    }

    /// The checker answered. Its notes become the editor's wavy underlines and
    /// the number the status bar shows, so the user can tell "no notes" from
    /// "not finished yet".
    fn show_checker_notes(&mut self, spell: Entity<Spell>, cx: &mut Context<Self>) {
        let notes: Vec<Diagnostic> = spell.read(cx).diagnostics().to_vec();
        self.spell_notes = notes.len();
        let marks: Vec<(usize, usize)> = notes.iter().map(|d| (d.start, d.end)).collect();
        self.editor
            .update(cx, |editor, cx| editor.set_marks(marks, cx));
        cx.notify();
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
                // Shale just changed the disk itself; re-stamping it is what stops
                // the watcher reporting our own save as an outside edit.
                self.resign();
                self.status = format!("saved {}", path.display()).into();
            }
            Err(err) => {
                self.status = format!("{err:#}").into();
            }
        }
        cx.notify();
    }

    /// `ctrl-w h`: move the keyboard to the panel on the left.
    ///
    /// Today the window has two panels, so this is the sidebar; a tiling split
    /// view will make it step sideways between panes instead. The command is
    /// ignored while a prompt or a picker is up, because those own the keyboard
    /// and moving focus out from under them would leave them open and unreachable.
    pub(super) fn window_left(
        &mut self,
        _: &WindowLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prompt.is_some() || self.picker.is_some() {
            return;
        }
        self.focus_tree(&FocusTree, window, cx);
    }

    /// `ctrl-w l`: move the keyboard to the note.
    pub(super) fn window_right(
        &mut self,
        _: &WindowRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.prompt.is_some() || self.picker.is_some() {
            return;
        }
        self.focus_editor(&FocusEditor, window, cx);
    }

    /// Fold a change made outside Shale into the window.
    ///
    /// The tree always follows the disk. The open note is re-read only when Shale
    /// holds no unsaved edits, because reloading would throw away what the user
    /// typed; a note deleted out from under a dirty buffer stays on screen so
    /// `ctrl-s` can write it back.
    pub(crate) fn apply_disk_changes(
        &mut self,
        after: BTreeMap<PathBuf, Stamp>,
        cx: &mut Context<Self>,
    ) {
        let changes = VaultChanges::between(&self.signature, &after);
        if changes.is_empty() {
            return;
        }
        tracing::debug!(
            added = changes.added.len(),
            removed = changes.removed.len(),
            modified = changes.modified.len(),
            "the vault changed on disk"
        );
        self.signature = after;
        self.refresh_entries(cx);
        if let Some(path) = self.active.clone() {
            let dirty = self.editor.read_with(cx, |editor, _cx| editor.is_dirty());
            if changes.was_removed(&path) {
                if dirty {
                    self.status = format!(
                        "{} deleted from disk; your edits are still here",
                        path.display()
                    )
                    .into();
                } else {
                    self.active = None;
                    self.editor
                        .update(cx, |editor, cx| editor.set_text(String::new(), cx));
                    self.checker_closed(cx);
                    self.status = format!("{} deleted from disk", path.display()).into();
                }
            } else if changes.was_modified(&path) {
                if dirty {
                    self.status = format!(
                        "{} changed on disk; ctrl-s will overwrite it",
                        path.display()
                    )
                    .into();
                } else {
                    self.open_note(&path, cx);
                    self.status = format!("{} reloaded from disk", path.display()).into();
                }
            }
        }
        cx.notify();
    }

    /// Re-take the vault's signature. Shale's own edits change the disk, and the
    /// watcher compares against this, so without it a save of ours would come
    /// back as somebody else's edit.
    pub(super) fn resign(&mut self) {
        self.signature = self.vault.signature().unwrap_or_default();
    }

    /// What the watcher does on its timer — re-walk the vault and fold any
    /// difference into the window — without waiting for the timer.
    #[cfg(test)]
    pub(crate) fn check_vault_now(&mut self, cx: &mut Context<Self>) {
        let after = self.vault.signature().unwrap_or_default();
        self.apply_disk_changes(after, cx);
    }

    /// The open note's text as the editor holds it, and whether that text is
    /// ahead of the disk: how a test checks what a disk change did to a note.
    #[cfg(test)]
    pub(crate) fn buffer_text<C: gpui::AppContext>(&self, cx: &C) -> String {
        self.editor.read_with(cx, |editor, _cx| editor.text())
    }

    #[cfg(test)]
    pub(crate) fn buffer_is_dirty<C: gpui::AppContext>(&self, cx: &C) -> bool {
        self.editor.read_with(cx, |editor, _cx| editor.is_dirty())
    }

    /// The cursor's buffer position (row, column), 0-based: how a test checks
    /// that choosing a search hit lands on the line that row was showing.
    #[cfg(test)]
    pub(crate) fn cursor_position<C: gpui::AppContext>(&self, cx: &C) -> (usize, usize) {
        self.editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
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

    /// The note the open picker would choose, or `None` while no picker is open:
    /// how a test checks what a typed query narrowed the list to. A command row
    /// reads as its label, and a search hit as `path:line`.
    #[cfg(test)]
    pub(crate) fn picker_choice(&self, cx: &gpui::App) -> Option<String> {
        self.picker
            .as_ref()
            .and_then(|picker| picker.read(cx).chosen())
            .map(|target| match target {
                PickerTarget::Note(path) => path.display().to_string(),
                PickerTarget::Hit { path, line } => format!("{}:{}", path.display(), line),
                PickerTarget::Command(command) => command.label().to_string(),
            })
    }

    /// Where the note's first display line sits, in window coordinates: how a
    /// test checks the editor's margins, and that a picker floats over the text
    /// instead of moving it. `(top edge, x of the first character)`.
    #[cfg(test)]
    pub(crate) fn note_origin<C: gpui::AppContext>(
        &self,
        cx: &C,
    ) -> Option<(gpui::Pixels, gpui::Pixels)> {
        self.editor.read_with(cx, |editor, _cx| {
            let top = editor.hit_line_edges(0)?.0;
            // `hit_line_x` is the painted line's left edge in window
            // coordinates; `x_for_offset` is relative to the line, so it would
            // always report 0 and prove nothing about the margins.
            let left = editor.hit_line_x(0)?.0;
            Some((top, left))
        })
    }

    /// How many rows the open picker is offering.
    #[cfg(test)]
    pub(crate) fn picker_matches(&self, cx: &gpui::App) -> Option<usize> {
        self.picker.as_ref().map(|picker| picker.read(cx).matches())
    }
}

/// How often the vault is re-walked looking for changes made outside Shale. Long
/// enough that the walk never lands on a keystroke's path, short enough that a
/// note created in another program shows up in the tree almost at once.
const WATCH_INTERVAL: Duration = Duration::from_millis(750);

/// Start watching the vault: every [`WATCH_INTERVAL`] the disk is re-walked on a
/// background thread and any difference is folded into the window by
/// [`Workspace::apply_disk_changes`].
///
/// Polling rather than an inotify dependency keeps `shale-core` free of external
/// crates, and the signature pass is the same one the planned vault database will
/// use to decide which notes to re-index. Only a weak handle is held, so closing
/// the window ends the loop.
pub(crate) fn start_watcher(workspace: &Entity<Workspace>, cx: &mut gpui::App) {
    // A copy, not the `Rc`: the walk runs on a background thread, and `Rc` cannot
    // cross one.
    let vault = (*workspace.read(cx).vault).clone();
    let watched = workspace.downgrade();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(WATCH_INTERVAL).await;
            let vault = vault.clone();
            let Ok(signature) = cx
                .background_executor()
                .spawn(async move { vault.signature() })
                .await
            else {
                continue;
            };
            if watched
                .update(cx, |ws, cx| ws.apply_disk_changes(signature, cx))
                .is_err()
            {
                // The window is gone, so there is nothing left to keep current.
                break;
            }
        }
    })
    .detach();
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
