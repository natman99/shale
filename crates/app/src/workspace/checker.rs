//! The window's half of the spell and grammar checker (roadmap P0.6).
//!
//! The window never checks anything itself: it hands the server the note on screen,
//! keeps the note current as the buffer changes, and turns the answers into wavy
//! underlines, the count the status bar shows, and the rows of the one popup that
//! can be up. The protocol lives in [`crate::lsp`]; this module is only what the
//! window does with it.
//!
//! A descendant of `workspace`, so it reaches `Workspace`'s private state
//! (`spell`, `checker_notes`, `_spell_events`) directly; the methods the other
//! descendants call are `pub(super)`.

use std::path::Path;

use crate::lsp::{absolute, Diagnostic, Spell};
use gpui::{Context, Entity};

use super::Workspace;

impl Workspace {
    /// Tell the checker which note the editor now holds. It keeps one document
    /// open at a time, so this replaces whatever it was last looking at. The path
    /// is vault-relative here and absolute by the time it becomes a URI, which is
    /// what the server matches its answers back to.
    pub(super) fn checker_opened(&mut self, path: &Path, text: &str, cx: &mut Context<Self>) {
        let Some(spell) = self.spell.clone() else {
            return;
        };
        let path = absolute(self.vault.root(), path);
        spell.update(cx, |spell, cx| spell.open(&path, text, cx));
    }

    /// The editor no longer holds a note, so the checker has nothing to say.
    pub(super) fn checker_closed(&mut self, cx: &mut Context<Self>) {
        let Some(spell) = self.spell.clone() else {
            return;
        };
        spell.update(cx, |spell, cx| spell.close(cx));
    }

    /// The buffer changed, so the checker's copy of the note is out of date. It
    /// debounces, which is what keeps typing a word at one request per word
    /// rather than one per character.
    pub(super) fn send_note_to_checker(&mut self, cx: &mut Context<Self>) {
        let Some(spell) = self.spell.clone() else {
            return;
        };
        let Some(editor) = self.editor().cloned() else {
            // The graph tab is on screen: there is no buffer for the checker to have
            // a stale copy of.
            return;
        };
        let text = editor.read_with(cx, |editor, _cx| editor.text());
        spell.update(cx, |spell, cx| spell.changed(&text, cx));
    }

    /// The checker answered. Its notes become the editor's wavy underlines and
    /// the number the status bar shows, so the user can tell "no notes" from
    /// "not finished yet"; they also refresh a popup that is already up, whose
    /// rows are the checker's own offers.
    pub(super) fn show_checker_notes(&mut self, spell: Entity<Spell>, cx: &mut Context<Self>) {
        let notes: Vec<Diagnostic> = spell.read(cx).diagnostics().to_vec();
        self.show_notes(notes, cx);
    }

    /// Put a fresh set of notes on the open note: the underlines, the count, and
    /// the rows of a popup that is up.
    pub(super) fn show_notes(&mut self, notes: Vec<Diagnostic>, cx: &mut Context<Self>) {
        self.checker_notes = notes;
        let marks: Vec<(usize, usize)> = self
            .checker_notes
            .iter()
            .map(|note| (note.start, note.end))
            .collect();
        if let Some(editor) = self.editor().cloned() {
            editor.update(cx, |editor, cx| editor.set_marks(marks, cx));
        }
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
                    if let Some(path) = self.note().clone() {
                        if let Some(editor) = self.editor().cloned() {
                            let text = editor.read_with(cx, |editor, _cx| editor.text());
                            self.checker_opened(&path, &text, cx);
                        }
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
}
