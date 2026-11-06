//! Opening a note, and writing one back.
//!
//! The seat and the disk: which tab holds which note, what a jump costs, and what a
//! save owes the index, the graph and the session file. Split out of `mod.rs`
//! (roadmap P3.4 Task 6 pushed it over the project's 1000-line rule); everything
//! here is still a descendant of `workspace`, so it reaches `Workspace`'s private
//! state directly and calls its neighbours through the same `pub(super)` doors.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{Context, Window};
use shale_core::vault::rel_text;

use crate::acts::Save;

use super::tabs::TabContent;
use super::Workspace;

impl Workspace {
    pub(crate) fn open_note(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        // What makes a note "recent" is having it in hand, whichever door it
        // came through -- picker, tree, `gd`, a tab click, the agent -- and it
        // counts even when the note was already open somewhere else.
        self._services.update(cx, |s, _| s.remember_note(path));
        // A note that is already open in another tab is *found*, not opened again:
        // following three links back to a note the tab bar already holds should not
        // put it on the bar twice. The current tab is the exception — reopening the
        // note that is already on screen re-reads it, which is what a reload from
        // disk asks for.
        // Any pane, not just this one (roadmap P3.2): a note another pane already
        // shows is never opened twice. It comes *here* (ruled 2026-11-05): the tab
        // moves whole into the pane with the keyboard, rather than dragging the
        // keyboard to wherever the note happened to be open. The graph tab is the
        // exception — it is one view per window, and `open_graph` focuses it where it
        // is, because a restore must not relocate it.
        if let Some((pix, tix)) = self.tab_for_path_anywhere(path) {
            if pix != self.active_pane {
                self.record_jump(path, cx);
                self.steal_tab(pix, tix, cx);
                return;
            }
            if tix != self.pane().active {
                self.record_jump(path, cx);
                self.show_tab(tix, cx);
            }
            return;
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
                } = &mut self.pane_mut().tabs[index].content
                {
                    *slot = Some(path.clone());
                    editor.update(cx, |editor, cx| editor.set_text(text.clone(), cx));
                }
                self.checker_opened(path, &text, cx);
                self.say(rel_text(path), cx);
                // The tab's note is only *now* the note being read: `note_tab` above
                // may have activated a tab that still held the previous one, and a
                // local graph following from inside that activation would re-centre on
                // the note the user is leaving. Mark again, once the path is set.
                self.mark_note_mru(cx);
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
    pub(super) fn follow_link(&mut self, target: &str, create: bool, cx: &mut Context<Self>) {
        match self.vault.resolve_link(target, &self.entries) {
            Ok(Some(entry)) => {
                let path = entry.path.clone();
                tracing::debug!(target, path = %path.display(), "following wiki-link");
                self.open_note(&path, cx);
            }
            // `gD`: the note the link asks for, made and then followed.
            Ok(None) if create => self.make_link_note(target, cx),
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
    /// The note a `gD` link asks for: created, then opened.
    ///
    /// It lands **beside the note the user is standing in** — a link written inside
    /// `areas/bread-v2/bread-v2.md` most often wants a note of that area, and Shale
    /// resolves links by shortest unique name anywhere in the vault, so the folder is
    /// a choice about tidiness rather than about whether the link works. A target that
    /// spells a path out (`[[areas/x/y]]`) is used verbatim, folders and all.
    fn make_link_note(&mut self, target: &str, cx: &mut Context<Self>) {
        let rel = if target.contains('/') {
            PathBuf::from(target)
        } else {
            let dir = self
                .note()
                .map(|note| note.parent().map(Path::to_path_buf).unwrap_or_default())
                .unwrap_or_default();
            dir.join(target)
        };
        match self.vault.create_note(rel) {
            Ok(created) => {
                // The same bookkeeping `:new` does: the tree has to know the file
                // exists, the index has to know it was created, and the selection
                // follows it so the tree is not left pointing at nothing.
                self.refresh_entries(cx);
                self.index_created(&created);
                self.tree_selection = Some(created.clone());
                // The message goes after the open, as `:new` does it: opening a note
                // says the note's name, and the creation is the thing worth saying.
                self.open_note(&created, cx);
                self.say(format!("created {}", rel_text(&created)), cx);
            }
            Err(err) => {
                tracing::warn!(target, "could not create the note a link asked for: {err:#}");
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
        let tab = self.pane().active;
        self.write_tab(tab, cx);
    }

    /// `:w <name>`: give the note in hand a name and write it -- only for a tab
    /// that has no name yet.
    ///
    /// A tab that already has a file refuses out loud. The user ruled this on
    /// 2026-11-05: `:w other-name` on an open note must not become a way to clone
    /// or move it by accident. Moving a note is `:rename`, which rewrites the
    /// links that point at the old name, and that difference is what the refusal
    /// points at.
    pub(crate) fn write_note_as(&mut self, name: &str, cx: &mut Context<Self>) {
        let index = self.pane().active;
        if let Some(existing) = self.pane().tabs[index].path() {
            let existing = existing.to_path_buf();
            self.say(
                format!(
                    "{} already has a name: `:w` writes it, `:rename` moves it",
                    rel_text(&existing)
                ),
                cx,
            );
            return;
        }
        if !self.name_blank_tab(index, Some(name), cx) {
            return;
        }
        self.write_tab(index, cx);
    }

    /// Give a tab that has no file a name, without writing it: the write belongs
    /// to the caller, so the bookkeeping a save does happens once. `false` means
    /// nothing was named, with the reason said in the status bar.
    ///
    /// `explicit` is a name the user typed at the `:` line. It may place the note
    /// in a folder, and a collision with an existing file is *refused* rather than
    /// worked around: a typed name is a specific request, and quietly answering it
    /// with `idea-2.md` is not what was asked. A name taken from the first line
    /// gets a number instead, because two notes that open with the same line are
    /// the common case and the user was not choosing a name at all.
    fn name_blank_tab(
        &mut self,
        index: usize,
        explicit: Option<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.pane().tabs[index].editor().cloned() else {
            // A graph tab, which has no text to write and no name to take.
            self.say("no file open", cx);
            return false;
        };
        let text = editor.read_with(cx, |editor, _cx| editor.text());

        let Some(name) = (match explicit {
            Some(typed) => shale_core::note_name::sanitize(typed, true),
            None => shale_core::note_name::from_line(&text),
        }) else {
            self.say(
                match explicit {
                    Some(_) => "that is not a name",
                    None => "nothing to name it after: `:w <name>` to name it",
                },
                cx,
            );
            return false;
        };

        let mut path = std::path::PathBuf::from(&name);
        if self.vault.exists(&path) {
            if explicit.is_some() {
                self.say(
                    format!("{} already exists: `:w` will not write over it", rel_text(&path)),
                    cx,
                );
                return false;
            }
            let stem = name.trim_end_matches(".md").trim_end_matches(".MD");
            // Bounded: a vault with 50 notes called `Idea` is a vault with a
            // problem, and the answer is to name the next one, not to search
            // forever.
            let mut taken = None;
            for n in 2..=50 {
                let candidate = format!("{stem}-{n}.md");
                if !self.vault.exists(&candidate) {
                    taken = Some(std::path::PathBuf::from(candidate));
                    break;
                }
            }
            match taken {
                Some(free) => path = free,
                None => {
                    self.say(format!("{} is taken 50 times over", name), cx);
                    return false;
                }
            }
        }

        // The text is already in the editor; the tab is what gains a name.
        if let TabContent::Note { path: slot, .. } = &mut self.pane_mut().tabs[index].content {
            *slot = Some(path.clone());
        }
        // The note is only now the note being read: the checker's scope and the
        // recents both key off the path, and a note the user just wrote into
        // existence is the most recent thing they have in hand.
        self.checker_opened(&path, &text, cx);
        self._services.update(cx, |s, _| s.remember_note(&path));
        self.mark_note_mru(cx);
        true
    }

    /// The write itself for one tab: no window, no repaint. The exits that end a
    /// session — `quit` and a termination signal — come through here, because neither
    /// has a frame to report through afterwards, and closing a tab writes its note
    /// whether or not that tab is the one on screen.
    pub(super) fn write_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        // A tab with no file is not "nothing open" -- it is a note the user has
        // been writing and not named yet. Its first line names it
        // (`shale_core::note_name`), which is what makes `:w` in a blank tab a
        // way to *start* a note rather than a dead end, and what stops a quit
        // from quietly dropping text that was never anywhere else.
        if self.pane().tabs[index].path().is_none() && !self.name_blank_tab(index, None, cx) {
            return;
        }
        let Some(path) = self.pane().tabs[index]
            .path()
            .map(|path| path.to_path_buf())
        else {
            self.say("no file open", cx);
            return;
        };
        // The handle is taken out of `self.pane().tabs` first: the write below reports through
        // `say`, which needs `self` mutably.
        let Some(editor) = self.pane().tabs[index].editor().cloned() else {
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
        // The delay is read before the tab is borrowed: `pane_mut` borrows the
        // whole window, and `self.config` has to be reachable to build it.
        let delay = Duration::from_millis(self.config.editor.autosave_after_ms);
        let tab = &mut self.pane_mut().tabs[index];
        tab.autosave_seq += 1;
        let seq = tab.autosave_seq;
        let id = tab.id;
        tab._autosave = Some(cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(delay).await;
            // The weak handle is what stops a timer outliving its window, the id is
            // what finds the tab after another one closed and moved the indices, and
            // the sequence number is what stops it writing a note that has been
            // edited again since it was parked.
            let _ = ws.update(cx, |ws, cx| {
                let found = ws.pane().tabs.iter().position(|tab| tab.id == id);
                if let Some(index) = found {
                    if ws.pane().tabs[index].autosave_seq == seq {
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
        let Some(editor) = self.pane().tabs[index].editor().cloned() else {
            return;
        };
        if !editor.read_with(cx, |editor, _cx| editor.is_dirty()) {
            return;
        }
        // A blank tab is named by its first line here exactly as `:w` names it, so
        // the note does not depend on which write reaches it first. The difference
        // is the silence: a beat that reported "nothing to name it after" every
        // couple of seconds would be a nag, not a status line.
        if self.pane().tabs[index].path().is_none() {
            let text = editor.read_with(cx, |editor, _cx| editor.text());
            if shale_core::note_name::from_line(&text).is_none() {
                return;
            }
            if !self.name_blank_tab(index, None, cx) {
                return;
            }
        }
        let Some(path) = self.pane().tabs[index]
            .path()
            .map(|path| path.to_path_buf())
        else {
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
        for index in 0..self.pane().tabs.len() {
            self.autosave(index, cx);
        }
    }
}
