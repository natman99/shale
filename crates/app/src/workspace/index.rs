//! The vault index inside the window (roadmap P1.3, P1.4).
//!
//! The index is a **cache**: the notes are the source of truth, so nothing here is
//! allowed to fail the window. Every call is best-effort and reports itself in the
//! log, and a window with no index (turned off in the config, or a DB that would
//! not open) is a window with a few features missing rather than a broken one — the
//! same posture the spellchecker has.
//!
//! Two paths keep it current. [`Workspace::start_index`] runs the one startup pass
//! that catches what other programs did while Shale was closed; after that the
//! watcher's ticks and Shale's own save, create, rename and delete paths report
//! what changed, which is what keeps the steady state free: nothing is re-read
//! unless its stamp moved.

use std::path::{Path, PathBuf};

use gpui::{Context, Window};
use shale_core::db::Provenance;
use shale_core::watch::VaultChanges;

use super::Workspace;
use crate::acts::WordCount;

impl Workspace {
    /// The startup reconciliation: read the vault once on a background thread,
    /// index the notes the index does not already match, and stop.
    ///
    /// It ends on purpose. Roadmap P1.3 asks for a pass rather than a loop because
    /// the vault watcher is already running, and its ticks report everything after
    /// this point.
    pub(super) fn start_index(&mut self, cx: &mut Context<Self>) {
        let Some(db) = self.db.clone() else {
            return;
        };
        // A copy of the vault rather than the `Rc`: the walk and the reading happen
        // on a background thread, and `Rc` cannot cross one.
        let vault = (*self.vault).clone();
        self._index = Some(cx.spawn(async move |_ws, cx| {
            let report = cx
                .background_executor()
                .spawn(async move { db.reconcile(&vault) })
                .await;
            match report {
                Ok(report) => {
                    if !report.is_empty() {
                        tracing::debug!(
                            indexed = report.indexed,
                            removed = report.removed,
                            "the vault index was brought up to date"
                        );
                    }
                    for (path, why) in &report.failed {
                        // One note that cannot be read costs its own rows, not the
                        // vault's index, so it is named and the pass stands.
                        tracing::warn!("could not index {}: {why}", path.display());
                    }
                }
                Err(err) => {
                    // The index keeps working — a failed pass is not a failed
                    // handle — so the next watcher tick tries these notes again.
                    tracing::warn!("could not bring the vault index up to date: {err:#}");
                }
            }
        }));
    }

    /// Fold one set of disk changes into the index.
    pub(super) fn index_changes(&self, changes: &VaultChanges, provenance: Provenance) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.sync(&self.vault, changes, provenance) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Re-index the note Shale just wrote.
    pub(super) fn index_saved(&self, path: &Path) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.note_saved(&self.vault, path, Provenance::User) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Index the notes a rename rewrote to point at the new name. Shale's own
    /// writes are stamped out of the watcher's view, so nothing else would notice
    /// them.
    pub(super) fn index_rewrites(&self, paths: &[PathBuf]) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        if let Err(err) = db.notes_saved(&self.vault, paths, Provenance::Rename) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Index a note Shale just created.
    pub(super) fn index_created(&self, path: &Path) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.note_created(&self.vault, path, Provenance::User) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Drop a note Shale just deleted.
    pub(super) fn index_deleted(&self, path: &Path) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.note_deleted(path, Provenance::User) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Move a note's rows, and record the move as one event.
    pub(super) fn index_renamed(&self, from: &Path, to: &Path) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.note_renamed(&self.vault, from, to) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// `:wordcount [folder]`: the first feature that reads the index rather than
    /// the notes, which is what makes it the proof that the spine works.
    ///
    /// The count is the index's, so it says which notes it counted. A note whose
    /// indexing failed is simply not in the number, and a vault with no index says
    /// so instead of inventing one.
    pub(super) fn wordcount(&mut self, folder: &str, cx: &mut Context<Self>) {
        let Some(db) = self.db.as_ref() else {
            self.say(
                "there is no vault index to count (index.enabled = false)",
                cx,
            );
            cx.notify();
            return;
        };
        let folder = folder.trim();
        match db.word_count_total(folder) {
            Ok((words, notes)) => {
                // One note is one note, and the bar is read out loud in the user's
                // head, so "1 notes" would sit wrong there.
                let word_word = if words == 1 { "word" } else { "words" };
                let note_word = if notes == 1 { "note" } else { "notes" };
                let scope = if folder.is_empty() {
                    format!("{words} {word_word} in {notes} {note_word}")
                } else {
                    format!("{words} {word_word} in {notes} {note_word} under {folder}/")
                };
                self.say(scope, cx);
            }
            Err(err) => {
                tracing::warn!("could not count the vault: {err:#}");
                self.say(format!("could not count the vault: {err:#}"), cx);
            }
        }
        cx.notify();
    }

    /// The palette row's `:wordcount`: the whole vault, because a palette row has
    /// nowhere to type a folder.
    pub(super) fn show_wordcount(
        &mut self,
        _: &WordCount,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.wordcount("", cx);
    }
}
