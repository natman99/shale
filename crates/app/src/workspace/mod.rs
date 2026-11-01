//! The window: the sidebar tree, the note, the status bar, and the vault edits
//! that connect them.
//!
//! Split into `keys.rs` (the keymap), `tree.rs` (the sidebar's keyboard state and
//! the rows it draws), `edits.rs` (the prompt and the vault edits behind its
//! answers) and `render.rs` (the frame). All four are descendants of `workspace`,
//! so they reach `Workspace`'s private state directly; anything one of them calls
//! on another is `pub(super)`.

mod bar;
mod edits;
mod exit;
mod index;
mod jumps;
mod keys;
mod open;
mod popups;
mod render;
mod search;
mod session;
#[cfg(test)]
mod testhooks;
mod themes;
mod tree;
mod views;

pub(crate) use bar::STATUS_TTL;
pub(crate) use keys::keybindings;
pub(crate) use open::PickerTarget;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{prelude::*, Context, Entity, SharedString, Subscription, Task, Window};
use shale_core::db::{Db, Provenance};
use shale_core::theme::{self, Palette};
use shale_core::vault::VaultEntry;
use shale_core::watch::{Stamp, VaultChanges};
use shale_core::{Config, Vault};

use crate::acts::*;
use crate::editor::{EditorEvent, MarkdownEditor};
use crate::lsp::{absolute, Diagnostic, Fix, Spell};
use crate::picker;
use crate::prompt;

/// The whole window's state: what is open, what the sidebar shows, and the editor
/// that holds the note.
pub(crate) struct Workspace {
    focus_handle: gpui::FocusHandle,
    vault: Rc<Vault>,
    /// What the user asked for in `<vault>/.shale/config.toml`. The window reads
    /// it when it starts the checker, which `:harper on` does at any time, and
    /// replaces it when it writes one of its own settings back into that file.
    config: Rc<Config>,
    /// The palette the whole window is drawn with, shared with the editor so a
    /// theme change repaints everything at once.
    palette: Rc<Palette>,
    /// The name that palette was chosen by (`appearance.theme`, or what the theme
    /// picker or `:theme` picked since). Only the status line asks.
    theme_name: String,
    /// The palette to go back to if the theme picker closes without a choice,
    /// because everything it previewed on the way was not an answer. `None` when
    /// no preview is in flight.
    theme_backup: Option<Rc<Palette>>,
    /// The spell and grammar checker, when one is configured and was started.
    /// It owns a server process; the window only talks to it.
    spell: Option<Entity<Spell>>,
    /// The checker's notes on the open note. They live here rather than only in
    /// the checker so that the status bar can count them, and `K` or a resting
    /// pointer can open a popup for the word under the cursor without asking the
    /// server anything — which is also what makes that path testable.
    checker_notes: Vec<Diagnostic>,
    /// The popup the window is showing, if any. There is one slot and one only:
    /// opening another replaces it, and every way out goes through `close_popup`.
    popup: Option<Entity<crate::popup::Popup>>,
    /// What had the keyboard before the popup took it, so closing hands it back.
    popup_return_focus: Option<gpui::FocusHandle>,
    /// The display line the popup sits below. Its place is worked out again each
    /// frame, so the panel follows the note as it scrolls and goes away when the
    /// line does.
    popup_anchor: Option<usize>,
    /// Whether the pointer opened the popup. Only then does moving off the word
    /// close it; a popup asked for with `K` stays until the user says otherwise.
    popup_from_pointer: bool,
    /// Which marked range the popup is about, so a mouse-move over the same word
    /// does not reopen it and a checker answer can tell whether its subject is
    /// still marked.
    popup_note: Option<(usize, usize)>,
    /// What the checker offered about that range, in the order it offered it: the
    /// popup shows these titles, and a chosen row number indexes back into them.
    popup_fixes: Vec<Fix>,
    /// A focus to hand the keyboard to at the start of the next frame, for the one
    /// path that has to close a popup without a window to do it with (the file
    /// watcher). Setting this is what keeps "closing always returns focus" true.
    pending_focus: Option<gpui::FocusHandle>,
    /// The subscription to the open popup, replaced when another one opens.
    _popup_events: Option<Subscription>,
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
    /// The `/` search: the query, what it matched in the open note, and which
    /// match the cursor is on. It lives here rather than in the field so that
    /// `n`/`N` still work once the field has closed.
    search: search::SearchState,
    /// The jump chain: the notes the user came from and where in them they were,
    /// which `ctrl-o`/`ctrl-i` walk. See [`jumps`].
    jumps: jumps::Jumps,
    /// What had the keyboard when the prompt opened, so closing it puts the
    /// keyboard back where the user was working: a rename started from the tree
    /// leaves them in the tree.
    prompt_return_focus: Option<gpui::FocusHandle>,
    /// What every path in the vault looked like the last time the disk was
    /// checked, which is what the next check explains the difference to.
    signature: BTreeMap<PathBuf, Stamp>,
    /// The vault index, when one is configured and opened: the cache of notes,
    /// tags, links and word counts that `:wordcount` reads now and the graph,
    /// vault search and agent read later.
    ///
    /// `None` is a missing feature rather than a broken window — the same posture
    /// as a checker that will not start — because the notes are the source of truth
    /// and the index can always be rebuilt from them. It is shared by `Arc` so the
    /// startup pass can hold one handle on a background thread while the window
    /// keeps its own.
    db: Option<Arc<Db>>,
    /// The startup reconciliation pass. Kept so it is cancelled with the window;
    /// it ends on its own otherwise (see [`Workspace::start_index`]).
    _index: Option<Task<()>>,
    /// The daily index backup loop. Kept so it is cancelled with the window; it
    /// never ends on its own (see [`Workspace::start_backups`]).
    _backup: Option<Task<()>>,
    /// The task that watches for a termination signal. Only started by `main`, and
    /// only for as long as the window is (see [`exit`]).
    _termination: Option<Task<()>>,
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
    /// Bumped by every message the status bar is given. The countdown that clears
    /// a message writes only if the number it was started with is still the
    /// current one, so an older timer can never wipe out a newer message.
    status_seq: u64,
    /// The status-bar countdown currently parked. Replacing it drops the old one,
    /// exactly as [`Workspace::_autosave`] does for the save debounce.
    _status_timer: Option<Task<()>>,
    editor: Entity<MarkdownEditor>,
    _editor_events: Subscription,
    /// Bumped by every edit to the note. A debounce timer writes only if the
    /// number it was started with is still the current one, so a burst of typing
    /// becomes one write at the end of the pause rather than one per keystroke.
    autosave_seq: u64,
    /// The autosave countdown currently parked. Replacing it drops the old timer;
    /// [`Workspace::autosave_seq`] is the belt to that brace, because a timer that
    /// already woke up is stopped by the number, not by the drop.
    _autosave: Option<Task<()>>,
    /// The window-activation subscription: losing focus writes the note at once
    /// instead of waiting out the countdown. `attach_window` adds it, because that
    /// is where there is a window to observe.
    _activation_events: Option<Subscription>,
    /// The pointer-over-the-note subscription. It needs a window to hand the
    /// keyboard to, so `attach_window` adds it rather than `new`.
    _hover_events: Option<Subscription>,
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
        // the default palette is used. The name is kept, so the status line can
        // say what is on screen after a live switch.
        let configured = config.appearance.theme.trim().to_string();
        let (palette, theme_name) = match theme::palette_for(vault.root(), &configured) {
            Ok(palette) => (
                palette,
                if configured.is_empty() {
                    Palette::DEFAULT_NAME.to_string()
                } else {
                    configured
                },
            ),
            Err(err) => {
                tracing::warn!("could not load the configured theme: {err:#}");
                (Palette::DEFAULT, Palette::DEFAULT_NAME.to_string())
            }
        };
        let palette = Rc::new(palette);
        editor.update(cx, |editor, cx| {
            editor.set_palette(Rc::clone(&palette), cx);
            editor.set_font_size(config.appearance.font_size, cx);
        });
        // Focus mode and typewriter scroll deliberately do *not* come from the
        // config: they are state the user chose in this window, and they belong
        // with the session file (roadmap P1.8) rather than the settings they edit
        // by hand. The editor's own defaults — both off, scope `block` — stand.
        let editor_events = cx.subscribe(&editor, |ws, _editor, event, cx| match event {
            EditorEvent::FollowLink { target } => ws.follow_link(target, cx),
            // The buffer moved, so the checker's copy of the note is out of date
            // (`Spell::changed` debounces, so a typed word is one request) and the
            // autosave countdown restarts.
            EditorEvent::Changed => {
                ws.send_note_to_checker(cx);
                ws.queue_autosave(cx);
                // The matches are byte offsets into the note, so an edit moves
                // them: re-run the query rather than let them drift.
                ws.refresh_search(cx);
            }
            // Hover is answered by the subscription `attach_window` adds, which is
            // the one that has a window to hand the keyboard to.
            EditorEvent::Hovered { .. } => {}
        });
        // Spell and grammar checking runs in a language server (`harper-ls` by
        // default). A server that is not installed, or will not start, is a
        // missing feature rather than a broken editor: it is reported and the
        // window carries on without one.
        let (spell, spell_events) = if config.spellcheck.enabled {
            match Spell::start(&config.spellcheck, cx) {
                Ok(spell) => {
                    // The checker notifies when it has new answers; those become
                    // the editor's wavy underlines.
                    let events =
                        cx.observe(&spell, |ws, spell, cx| ws.show_checker_notes(spell, cx));
                    (Some(spell), Some(events))
                }
                Err(err) => {
                    tracing::warn!("no spellcheck: {err:#}");
                    (None, None)
                }
            }
        } else {
            (None, None)
        };
        // The vault index is a cache the notes can always rebuild, so an index that
        // is switched off in the config or will not open is a missing feature rather
        // than a broken window: reported, and the window carries on without one.
        let db = if config.index.enabled {
            match Db::open(vault.root()) {
                Ok(db) => Some(db),
                Err(err) => {
                    tracing::warn!("no vault index: {err:#}");
                    None
                }
            }
        } else {
            None
        };
        let db = db.map(Arc::new);
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
        let mut workspace = Self {
            focus_handle: cx.focus_handle(),
            vault,
            config,
            palette,
            theme_name: theme_name.clone(),
            theme_backup: None,
            spell,
            checker_notes: Vec::new(),
            popup: None,
            popup_return_focus: None,
            popup_anchor: None,
            popup_from_pointer: false,
            popup_note: None,
            popup_fixes: Vec::new(),
            pending_focus: None,
            _popup_events: None,
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
            search: search::SearchState::default(),
            jumps: jumps::Jumps::default(),
            picker: None,
            picker_return_focus: None,
            _picker_events: None,
            signature,
            db,
            _index: None,
            _backup: None,
            _termination: None,
            _prompt_events: None,
            status: "".into(),
            status_seq: 0,
            _status_timer: None,
            editor,
            _editor_events: editor_events,
            autosave_seq: 0,
            _autosave: None,
            _activation_events: None,
            _hover_events: None,
            _keystroke_observer: keystroke_observer,
        };
        // Roadmap P1.8: the window comes up where the last one left it, before
        // anything else has a chance to look at what "where" was.
        workspace.restore_session(cx);
        // The one startup pass that catches what other programs did to the vault
        // while Shale was closed. After it, the watcher and the save paths keep the
        // index current between them.
        workspace.start_index(cx);
        workspace
    }

    /// Wire up the part of the window that needs a window to act on: the pointer's
    /// position over the note, because opening a popup is a focus change.
    ///
    /// Called once per window, by the frame that builds it. Until it is, the window
    /// simply never hovers — which is why the rest of the window can be built, and
    /// tested, without one.
    ///
    /// This is also where the two window-shaped save hooks live: losing focus, and
    /// the window manager asking to close. Both need a window, so neither can live
    /// in `new`.
    pub(crate) fn attach_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._hover_events.is_some() {
            return;
        }
        self._hover_events = Some(cx.subscribe_in(
            &self.editor,
            window,
            |ws, _editor, event, window, cx| match event {
                EditorEvent::Hovered { offset, line, at } => {
                    ws.pointer_hovered(*offset, *line, *at, window, cx)
                }
                // Everything else the editor reports is handled by the subscription
                // `new` makes, which needs no window.
                EditorEvent::FollowLink { .. } | EditorEvent::Changed => {}
            },
        ));
        self._activation_events = Some(cx.observe_window_activation(window, |ws, window, cx| {
            // Losing focus is the other half of "the user stopped typing": they
            // have gone somewhere else with the note still ahead of the disk, so
            // it is written now rather than after the countdown. Activating again
            // has nothing to catch up on.
            if !window.is_window_active() {
                ws.autosave(cx);
            }
        }));

        // The window manager's close button is the one exit that does not come
        // through `quit`, and gpui gives the window a hook for it: the callback
        // returns whether the window may close. On Wayland it is
        // `xdg_toplevel::Event::Close` that calls it.
        //
        // The write here is the one `quit` does rather than the autosave, because
        // `editor.autosave = false` means "do not write while I am working", not
        // "lose the note when the window goes away". A note that cannot be written
        // keeps the window open, exactly as `:q` refuses to quit: the status bar
        // says why, and there is still a window to act on that from.
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |ws, cx| {
                if ws.editor.read_with(cx, |editor, _cx| editor.is_dirty()) {
                    ws.save(&Save, window, cx);
                    if ws.editor.read_with(cx, |editor, _cx| editor.is_dirty()) {
                        // The write failed: the window stays, with the reason in
                        // the status bar.
                        return false;
                    }
                }
                // Roadmap P1.8: the session goes with the note — this is the last
                // moment the window knows what it looked like.
                ws.save_session(cx);
                true
            })
            .unwrap_or(true)
        });
    }

    /// Put a message in the status bar and start the countdown that clears it.
    ///
    /// The bar's other segments are *state*: the mode, the save dot, the checker's
    /// count and the search's readout are each true for exactly as long as they
    /// are on screen, so they clear themselves by changing. A message is different
    /// — it is a reply to something the user just did, and it goes stale. `:typewriter
    /// off` is worth reading once, not for the rest of the session, and a bar that
    /// keeps reporting it is a bar full of old information.
    ///
    /// So every message is parked with a timer, in the same shape as the autosave
    /// debounce: a new message *replaces* the parked task, which drops the old
    /// countdown and restarts the wait — that is what the tests catch. The sequence
    /// number is the belt to that brace, for a countdown that already woke up and
    /// has not applied yet: it writes only if the number it started with is still
    /// current, so it cannot clear a message that arrived after it.
    pub(crate) fn say(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status = message.into();
        self.status_seq += 1;
        let seq = self.status_seq;
        self._status_timer = Some(cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(STATUS_TTL).await;
            // The weak handle is what stops a countdown outliving its window.
            let _ = ws.update(cx, |ws, cx| {
                if ws.status_seq == seq {
                    ws.status = "".into();
                    cx.notify();
                }
            });
        }));
    }

    pub(crate) fn open_note(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        // Going to a different note is a jump, so the place being left goes onto
        // the chain `ctrl-o` walks back.
        self.record_jump(path, cx);
        self.load_note(path, cx);
        cx.notify();
    }

    /// Read `path` and make it the note on screen: everything [`Self::open_note`]
    /// does except decide what happens to the jump chain. Whether it worked, which
    /// is what the jump path needs to know before it moves the cursor.
    fn load_note(&mut self, path: &PathBuf, cx: &mut Context<Self>) -> bool {
        // A popup belongs to the note it was opened over, so a new note ends it.
        self.drop_popup(cx);
        // The same for a search: its matches are offsets into the note that is
        // closing.
        self.clear_search(cx);
        match self.vault.read(path) {
            Ok(text) => {
                tracing::debug!(path = %path.display(), bytes = text.len(), "opened note");
                self.active = Some(path.clone());
                self.editor
                    .update(cx, |editor, cx| editor.set_text(text.clone(), cx));
                self.checker_opened(path, &text, cx);
                self.say(path.display().to_string(), cx);
                true
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), "failed to read note: {err:#}");
                self.say(format!("{err:#}"), cx);
                false
            }
        }
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
                self.say(format!("no note named {target}"), cx);
                cx.notify();
            }
            Err(err) => {
                tracing::warn!(target, "failed to resolve link: {err:#}");
                self.say(format!("{err:#}"), cx);
                cx.notify();
            }
        }
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
    /// "not finished yet"; they also refresh a popup that is already up, whose
    /// rows are the checker's own offers.
    fn show_checker_notes(&mut self, spell: Entity<Spell>, cx: &mut Context<Self>) {
        let notes: Vec<Diagnostic> = spell.read(cx).diagnostics().to_vec();
        self.show_notes(notes, cx);
    }

    /// Put a fresh set of notes on the open note: the underlines, the count, and
    /// the rows of a popup that is up.
    fn show_notes(&mut self, notes: Vec<Diagnostic>, cx: &mut Context<Self>) {
        self.checker_notes = notes;
        let marks: Vec<(usize, usize)> = self
            .checker_notes
            .iter()
            .map(|note| (note.start, note.end))
            .collect();
        self.editor
            .update(cx, |editor, cx| editor.set_marks(marks, cx));
        self.refresh_popup_rows(cx);
        cx.notify();
    }
    /// Start or stop the spell and grammar checker, whatever the config asked for
    /// at startup (`:harper on|off`).
    ///
    /// Turning it off drops the checker entity, whose `Drop` ends the server
    /// process, and with it the underlines and any panel about one of them.
    /// Turning it on starts the server the config names and hands it the note that
    /// is open; a server that is not installed is reported the way a missing one is
    /// at startup — a missing feature rather than a broken editor. The note waits
    /// for the server's handshake, so it is checked whether the handshake wins the
    /// race or the note does.
    pub(super) fn set_checker(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.spell.is_some() == on {
            self.say(
                if on {
                    "the spellchecker is already on"
                } else {
                    "the spellchecker is already off"
                },
                cx,
            );
            cx.notify();
            return;
        }
        if on {
            match Spell::start(&self.config.spellcheck, cx) {
                Ok(spell) => {
                    let events =
                        cx.observe(&spell, |ws, spell, cx| ws.show_checker_notes(spell, cx));
                    self._spell_events = Some(events);
                    self.spell = Some(spell);
                    self.say("spellcheck on", cx);
                    // This server has never seen the note on screen. Handing it
                    // over in the same breath as starting the server is safe only
                    // because the checker stages the open until its handshake is
                    // answered: a `didOpen` the server reads while it is still
                    // starting is one it throws away, and the note is then never
                    // checked (`:harper on` used to leave exactly that).
                    if let Some(path) = self.active.clone() {
                        let text = self.editor.read_with(cx, |editor, _cx| editor.text());
                        self.checker_opened(&path, &text, cx);
                    }
                }
                Err(err) => {
                    tracing::warn!("no spellcheck: {err:#}");
                    self.say(format!("{err:#}"), cx);
                }
            }
        } else {
            self._spell_events = None;
            self.spell = None;
            // A panel about a word the checker no longer has anything to say about
            // has nothing left to show.
            self.drop_popup(cx);
            self.show_notes(Vec::new(), cx);
            self.say("spellcheck off", cx);
        }
        cx.notify();
    }

    /// Quit, writing the open note first when it holds unsaved edits.
    ///
    /// Autosave usually has already written it, but a quit can come in the middle
    /// of the countdown, and work the user can see on screen must not be lost to a
    /// keystroke timing. `ctrl-q`, `:q` and `:wq` all come through here because the
    /// danger is the same; `:wq` asks for the write whether or not the note is
    /// dirty. Closing the window from outside Shale — the window manager's button —
    /// does not reach this action, so `attach_window` makes the same promise there.
    pub(super) fn quit(&mut self, _: &Quit, _window: &mut Window, cx: &mut Context<Self>) {
        let dirty = self.editor.read_with(cx, |editor, _cx| editor.is_dirty());
        if dirty {
            self.write_note(cx);
            // The save said why it failed in the status bar. Quitting now would
            // throw the note away, so the user gets to see that first.
            let still = self.editor.read_with(cx, |editor, _cx| editor.is_dirty());
            if still {
                cx.notify();
                return;
            }
        }
        // Roadmap P1.5: an exit leaves the index drained and checkpointed, so the
        // next launch opens one DB file rather than a WAL full of unapplied writes.
        self.flush_index();
        // ...and roadmap P1.8: it leaves the session too, so the next window opens
        // on this note, at this place, in this view.
        self.save_session(cx);
        cx.quit();
    }

    pub(super) fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.write_note(cx);
        cx.notify();
    }

    /// The write itself: no window, no repaint. The exits that end a session — `quit`
    /// and a termination signal — come through here, because neither has a frame to
    /// report through afterwards.
    fn write_note(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active.clone() else {
            self.say("no file open", cx);
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
                // ...and the index has to be told, for the same reason.
                self.index_saved(&path);
                self.say(format!("saved {}", path.display()), cx);
            }
            Err(err) => {
                // Logged as well as said: an exit path can fail a save with nobody
                // left to read the status bar.
                tracing::error!(path = %path.display(), "save failed: {err:#}");
                self.say(format!("{err:#}"), cx);
            }
        }
    }

    /// Restart the autosave countdown for an edit that just happened.
    ///
    /// Every keystroke replaces the parked timer, so what fires is one write after
    /// the typing stopped — `editor.autosave_after_ms` of quiet, which is two
    /// seconds by default. Dropping the old [`Task`] stops its timer; the sequence
    /// number is what stops a timer that had already woken from writing a note the
    /// user has since edited again.
    ///
    /// The `editor.autosave` check here is only so a setting that is off does not
    /// park a timer for nothing; [`Workspace::autosave`] is where the setting
    /// actually governs.
    fn queue_autosave(&mut self, cx: &mut Context<Self>) {
        if !self.config.editor.autosave {
            return;
        }
        self.autosave_seq += 1;
        let seq = self.autosave_seq;
        let delay = Duration::from_millis(self.config.editor.autosave_after_ms);
        self._autosave = Some(cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(delay).await;
            // The weak handle is what stops a timer outliving its window, and the
            // sequence number is what stops it writing a note that has been edited
            // again since it was parked.
            let _ = ws.update(cx, |ws, cx| {
                if ws.autosave_seq == seq {
                    ws.autosave(cx);
                }
            });
        }));
    }

    /// Write the open note when it holds edits — quietly. No status-line report:
    /// this runs on a timer and on every blur, so a message each time would bury
    /// everything the bar is for. The dot going out is the signal that it worked.
    ///
    /// Nothing is skipped for an open prompt. A rename re-points the open note at
    /// the new path when it applies, so writing the old path first just means the
    /// renamed file has the edits in it — which is the outcome the user wanted and
    /// did not have to stop typing for.
    ///
    /// `editor.autosave` is checked here rather than only where the countdown is
    /// parked, so the setting governs both ways the countdown-driven path is
    /// entered: the timer and the blur. The two exits that end the session — `quit`
    /// and the window manager's close — write through `save` instead, because
    /// turning autosave off is not a wish to lose the note.
    fn autosave(&mut self, cx: &mut Context<Self>) {
        if !self.config.editor.autosave {
            return;
        }
        if !self.editor.read_with(cx, |editor, _cx| editor.is_dirty()) {
            return;
        }
        let Some(path) = self.active.clone() else {
            return;
        };
        let text = self.editor.read_with(cx, |editor, _cx| editor.text());
        match self.vault.write(&path, &text) {
            Ok(()) => {
                self.editor.update(cx, |editor, _cx| editor.mark_clean());
                // Our own write must not come back as somebody else's edit.
                self.resign();
                self.index_saved(&path);
                // The session rides the same beat (roadmap P1.8): the pause that
                // writes the note is also the moment the window knows what it
                // looks like, and a crash between here and the next pause loses
                // at most one pause's worth of state.
                self.save_session(cx);
                tracing::debug!(path = %path.display(), "autosaved");
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), "autosave failed: {err:#}");
                self.say(format!("autosave failed: {err:#}"), cx);
                cx.notify();
            }
        }
    }

    /// Whether something other than the editor or the tree holds the keyboard.
    ///
    /// A prompt, a picker or a popup is a modal layer: it has its own bindings, and
    /// a context-free binding would otherwise fire through it. Every handler for a
    /// context-free key asks this before it moves anything underneath the layer.
    pub(super) fn overlay_owns_keys(&self) -> bool {
        self.prompt.is_some() || self.picker.is_some() || self.popup.is_some()
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
        if self.overlay_owns_keys() {
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
        if self.overlay_owns_keys() {
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
        // The index follows the disk as surely as the tree does, and gets the same
        // change list. Everything in it is best-effort: a failed index is a log
        // line, never a reason the window stops showing the vault.
        self.index_changes(&changes, Provenance::External);
        // A note that is gone cannot be jumped to, so it leaves the chain — the
        // same rule the explicit delete follows.
        for removed in &changes.removed {
            self.jumps.forget(removed);
        }
        if let Some(path) = self.active.clone() {
            let dirty = self.editor.read_with(cx, |editor, _cx| editor.is_dirty());
            if changes.was_removed(&path) {
                if dirty {
                    self.say(
                        format!(
                            "{} deleted from disk; your edits are still here",
                            path.display()
                        ),
                        cx,
                    );
                } else {
                    self.active = None;
                    self.editor
                        .update(cx, |editor, cx| editor.set_text(String::new(), cx));
                    self.checker_closed(cx);
                    self.say(format!("{} deleted from disk", path.display()), cx);
                }
            } else if changes.was_modified(&path) {
                if dirty {
                    self.say(
                        format!(
                            "{} changed on disk; ctrl-s will overwrite it",
                            path.display()
                        ),
                        cx,
                    );
                } else {
                    self.open_note(&path, cx);
                    self.say(format!("{} reloaded from disk", path.display()), cx);
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
