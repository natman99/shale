//! Opening a note, and writing one back.
//!
//! The seat and the disk: which tab holds which note, what a jump costs, and what a
//! save owes the index, the graph and the session file. Split out of `mod.rs`
//! (roadmap P3.4 Task 6 pushed it over the project's 1000-line rule); everything
//! here is still a descendant of `workspace`, so it reaches `Workspace`'s private
//! state directly and calls its neighbours through the same `pub(super)` doors.

use std::path::PathBuf;
use std::time::Duration;

use gpui::{Context, Window};
use shale_core::vault::rel_text;

use crate::acts::Save;

use super::tabs::TabContent;
use super::Workspace;

impl Workspace {
    pub(crate) fn open_note(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        // A note that is already open in another tab is *found*, not opened again:
        // following three links back to a note the tab bar already holds should not
        // put it on the bar twice. The current tab is the exception — reopening the
        // note that is already on screen re-reads it, which is what a reload from
        // disk asks for.
        if let Some(index) = self.tab_for_path(path) {
            if index != self.tab {
                self.record_jump(path, cx);
                self.show_tab(index, cx);
                return;
            }
        }
        // Going to a different note is a jump, so the place being left goes onto
        // the chain `ctrl-o` walks back.
        self.record_jump(path, cx);
        self.load_note(path, cx);
        cx.notify();
    }

    /// Read `path` into the tab on screen and make it that tab's note: everything
    /// [`Self::open_note`] does except decide what happens to the jump chain and
    /// whether another tab already holds the note. Whether it worked, which is what
    /// the jump path needs to know before it moves the cursor.
    pub(super) fn load_note(&mut self, path: &PathBuf, cx: &mut Context<Self>) -> bool {
        // A popup belongs to the note it was opened over, so a new note ends it.
        self.drop_popup(cx);
        // The same for a search: its matches are offsets into the note that is
        // closing.
        self.clear_search(cx);
        // The graph tab is never turned into a note tab, so a note opened while the
        // graph is on screen moves the frame first (see [`Workspace::note_tab`]).
        let index = self.note_tab(cx);
        match self.vault.read(path) {
            Ok(text) => {
                tracing::debug!(path = %path.display(), bytes = text.len(), "opened note");
                if let TabContent::Note {
                    path: slot, editor, ..
                } = &mut self.tabs[index].content
                {
                    *slot = Some(path.clone());
                    editor.update(cx, |editor, cx| editor.set_text(text.clone(), cx));
                }
                self.checker_opened(path, &text, cx);
                self.say(rel_text(path), cx);
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
        // Only if the note really opened — a read error leaves the tab alone.
        if self.note().as_deref() == Some(path.as_path()) {
            if let Some(editor) = self.editor().cloned() {
                editor.update(cx, |editor, cx| {
                    editor.reveal_line(line.saturating_sub(1), cx)
                });
            }
        }
    }

    /// Open the note a wiki-link target refers to, if it exists.
    pub(super) fn follow_link(&mut self, target: &str, cx: &mut Context<Self>) {
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
    pub(super) fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.write_note(cx);
        cx.notify();
    }

    /// The write of the note on screen. [`Workspace::write_all_tabs`] is the one the
    /// exits use, because with tabs open a quit owes every note on the bar, not just
    /// the one under the cursor.
    fn write_note(&mut self, cx: &mut Context<Self>) {
        let tab = self.tab;
        self.write_tab(tab, cx);
    }

    /// The write itself for one tab: no window, no repaint. The exits that end a
    /// session — `quit` and a termination signal — come through here, because neither
    /// has a frame to report through afterwards, and closing a tab writes its note
    /// whether or not that tab is the one on screen.
    pub(super) fn write_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(path) = self.tabs[index].path().map(|path| path.to_path_buf()) else {
            self.say("no file open", cx);
            return;
        };
        // The handle is taken out of `self.tabs` first: the write below reports through
        // `say`, which needs `self` mutably.
        let Some(editor) = self.tabs[index].editor().cloned() else {
            return;
        };
        let text = editor.read_with(cx, |editor, _cx| editor.text());
        match self.vault.write(&path, &text) {
            Ok(()) => {
                editor.update(cx, |editor, _cx| editor.mark_clean());
                // Shale just changed the disk itself; re-stamping it is what stops
                // the watcher reporting our own save as an outside edit.
                self.resign();
                // ...and the index has to be told, for the same reason.
                self.index_saved(&path);
                // ...and the graph, for the same reason: the watcher never sees our
                // own write.
                self.graph_note_saved(&path, &text);
                self.refresh_graph(cx);
                self.say(format!("saved {}", rel_text(&path)), cx);
            }
            Err(err) => {
                // Logged as well as said: an exit path can fail a save with nobody
                // left to read the status bar.
                tracing::error!(path = %path.display(), "save failed: {err:#}");
                self.say(format!("{err:#}"), cx);
            }
        }
    }

    /// Restart the autosave countdown for an edit in one tab.
    ///
    /// Every keystroke replaces that tab's parked timer, so what fires is one write
    /// after the typing stopped — `editor.autosave_after_ms` of quiet, which is two
    /// seconds by default. Dropping the old [`Task`] stops its timer; the sequence
    /// number is what stops a timer that had already woken from writing a note the
    /// user has since edited again.
    ///
    /// **Per tab**, which is the point of putting both halves in the tab: typing in
    /// one tab must not cancel another tab's pending write, and a tab that closes
    /// takes its countdown with it.
    ///
    /// The `editor.autosave` check here is only so a setting that is off does not
    /// park a timer for nothing; [`Workspace::autosave`] is where the setting
    /// actually governs.
    pub(super) fn queue_autosave(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.config.editor.autosave {
            return;
        }
        let tab = &mut self.tabs[index];
        tab.autosave_seq += 1;
        let seq = tab.autosave_seq;
        let id = tab.id;
        let delay = Duration::from_millis(self.config.editor.autosave_after_ms);
        tab._autosave = Some(cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(delay).await;
            // The weak handle is what stops a timer outliving its window, the id is
            // what finds the tab after another one closed and moved the indices, and
            // the sequence number is what stops it writing a note that has been
            // edited again since it was parked.
            let _ = ws.update(cx, |ws, cx| {
                let found = ws.tabs.iter().position(|tab| tab.id == id);
                if let Some(index) = found {
                    if ws.tabs[index].autosave_seq == seq {
                        ws.autosave(index, cx);
                    }
                }
            });
        }));
    }

    /// Write one tab's note when it holds edits — quietly. No status-line report:
    /// this runs on a timer and on every blur, so a message each time would bury
    /// everything the bar is for. The dot going out on that tab is the signal that it
    /// worked.
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
    pub(super) fn autosave(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.config.editor.autosave {
            return;
        }
        let Some(editor) = self.tabs[index].editor().cloned() else {
            return;
        };
        if !editor.read_with(cx, |editor, _cx| editor.is_dirty()) {
            return;
        }
        let Some(path) = self.tabs[index].path().map(|path| path.to_path_buf()) else {
            return;
        };
        let text = editor.read_with(cx, |editor, _cx| editor.text());
        match self.vault.write(&path, &text) {
            Ok(()) => {
                editor.update(cx, |editor, _cx| editor.mark_clean());
                // Our own write must not come back as somebody else's edit.
                self.resign();
                self.index_saved(&path);
                self.graph_note_saved(&path, &text);
                self.refresh_graph(cx);
                // The session rides the same beat (roadmap P1.8): the pause that
                // writes a note is also the moment the window knows what it looks
                // like, and a crash between here and the next pause loses at most one
                // pause's worth of state.
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

    /// Every tab's countdown-driven write, for the blur: the user left the window
    /// with notes ahead of the disk, and which tab they were looking at is beside the
    /// point.
    pub(super) fn autosave_all(&mut self, cx: &mut Context<Self>) {
        for index in 0..self.tabs.len() {
            self.autosave(index, cx);
        }
    }
}
