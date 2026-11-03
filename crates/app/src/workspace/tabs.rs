//! Tabs: one note per tab, and the editor that holds it (roadmap P3.1).
//!
//! **The editor belongs to the tab.** The alternative — one editor whose text is
//! swapped when the tab changes — is less code and worse behaviour: the cursor, the
//! scroll, the folds, the view the note is in and the undo history all live in the
//! editor, so swapping text would undo *another note's* edits when the user pressed
//! `u`. A tab gets its own [`MarkdownEditor`] and keeps everything the user made in
//! it, which is also what makes a per-tab view (this tab raw, that one preview) fall
//! out for free rather than needing a second table of per-note state.
//!
//! **A tab is a note, and opening a note that is already open goes to its tab.**
//! That is Obsidian's rule, and it is what stops following three links from making
//! three tabs. New tabs come from being asked for: `ctrl-t` / `:tabnew` on the note
//! in hand, or a middle click on a tab's `×`… which closes it. Two tabs may hold the
//! same note — that is the "read the top of this note while I edit the bottom of it"
//! case, and it is why the tab's identity is an id rather than its path.
//!
//! **There is always at least one tab.** Closing the last one leaves an empty tab
//! (no path, no text) rather than an empty window, which is what a window on an empty
//! vault starts with too. That invariant is what lets [`Workspace::editor`] be total
//! instead of optional, and it is why closing a tab is "remove it, or clear it if it
//! was the only one".
//!
//! Not here yet, and deliberately: per-tab jump chains and per-tab checker answers
//! (both are window-level today, and P3.2's panes will want the same decision), and
//! a tab bar that scrolls when there are more tabs than fit — they share the width
//! and ellipsis instead.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{prelude::*, Context, Entity, Focusable, Subscription, Task, Window};

use crate::acts::{TabClose, TabNew, TabNext, TabOnly, TabPrev};
use crate::editor::{EditorEvent, MarkdownEditor, View};

use super::Workspace;

/// One tab: a note, and the editor holding it.
pub(crate) struct Tab {
    /// Stable identity. A tab's *index* moves when another tab closes, and the
    /// editor's subscription outlives that, so the subscription carries the id and
    /// the window looks the tab up by it.
    pub(crate) id: usize,
    /// The note this tab holds. `None` is the empty tab: what closing the last tab
    /// leaves behind, and what a window over an empty vault starts with.
    pub(crate) path: Option<PathBuf>,
    /// The editor. Owned by the tab, so the cursor, scroll, folds, view and undo
    /// history are the ones the user left in this tab.
    pub(crate) editor: Entity<MarkdownEditor>,
    /// The subscription that carries this editor's events to the window. Alive for as
    /// long as the tab is (see [`Workspace::editor_event`]).
    pub(crate) _events: Subscription,
    /// Bumped by every edit in this tab. The debounce below writes only if the number
    /// it was started with is still the current one, so a burst of typing becomes one
    /// write at the end of the pause. Per tab, because a note typed in one tab must
    /// not cancel another tab's pending write.
    pub(crate) autosave_seq: u64,
    /// This tab's autosave countdown. Replacing it drops the old one.
    pub(crate) _autosave: Option<Task<()>>,
}

impl Workspace {
    /// The editor of the tab on screen. Every part of the window that talks to the
    /// note goes through this rather than holding an editor of its own, which is what
    /// keeps "the note" meaning "the note in this tab" everywhere at once.
    pub(crate) fn editor(&self) -> &Entity<MarkdownEditor> {
        &self.tabs[self.tab].editor
    }

    /// The note the tab on screen holds, or `None` for an empty tab.
    pub(crate) fn note(&self) -> Option<PathBuf> {
        self.tabs[self.tab].path.clone()
    }

    /// Which tab holds `path`, if one does. This is what makes opening a note that is
    /// already open a tab change rather than a second copy of the note.
    pub(super) fn tab_for_path(&self, path: &Path) -> Option<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.path.as_deref() == Some(path))
    }

    /// Which tab has this id, for the subscriptions that captured one.
    fn tab_for_id(&self, id: usize) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// Add a tab after the one on screen and make it the active one. The editor is
    /// empty and pathless when it arrives: what it holds is decided by the caller,
    /// which is also the point at which the note's text is read.
    pub(super) fn add_tab(&mut self, cx: &mut Context<Self>) -> usize {
        let id = self.tab_seq;
        self.tab_seq += 1;
        // The view the user is working in, which is what a tab opened from a note in
        // raw mode is opened in. There is no tab to ask yet only at startup, where
        // the editor's own default is the right answer.
        let view = self
            .tabs
            .get(self.tab)
            .map(|tab| tab.editor.read(cx).view())
            .unwrap_or(View::Live);
        let editor = cx.new(|cx| MarkdownEditor::new(cx));
        // Everything the window knows that an editor cannot discover for itself —
        // the vault, the attachments, the link names, the font, the palette — is
        // applied here, so a tab opened at any point in the session starts with the
        // settings the session is already using.
        editor.update(cx, |editor, cx| {
            editor.set_vault(Rc::clone(&self.vault), self.attachments.clone(), cx);
            editor.set_known_links(self.known.clone(), cx);
            editor.set_palette(Rc::clone(&self.palette), cx);
            editor.set_font_size(self.config.appearance.font_size, cx);
            editor.set_font_family(&self.config.appearance.font_family, cx);
            editor.set_view(view, cx);
        });
        let events = cx.subscribe(&editor, move |ws, _editor, event, cx| {
            ws.editor_event(id, event, cx)
        });
        let index = if self.tabs.is_empty() {
            0
        } else {
            self.tab + 1
        };
        self.tabs.insert(
            index,
            Tab {
                id,
                path: None,
                editor,
                _events: events,
                autosave_seq: 0,
                _autosave: None,
            },
        );
        self.tab = index;
        // The new tab is the drawn one, so both the pointer's subscription and the
        // keyboard have to come here; neither path has a window, so the next frame is
        // asked for both.
        self.hover_stale = true;
        self.focus_stale = true;
        index
    }

    /// What the window does with an editor's news, which it can only decide by asking
    /// *which* tab that editor belongs to: an edit in the tab on screen re-checks the
    /// note and restarts its autosave, and an edit in a tab nobody is looking at only
    /// has to reach the disk.
    pub(super) fn editor_event(&mut self, id: usize, event: &EditorEvent, cx: &mut Context<Self>) {
        let Some(index) = self.tab_for_id(id) else {
            // The tab closed between the event and this call. Nothing to do: the
            // editor is on its way out with the tab.
            return;
        };
        let active = index == self.tab;
        match event {
            EditorEvent::FollowLink { target } => {
                // Only the tab on screen can be clicked or `gx`-ed, but a link
                // followed in a tab that is not on screen would otherwise edit the
                // wrong note, so it is brought on screen first.
                if !active {
                    self.show_tab(index, cx);
                }
                self.follow_link(target, cx);
            }
            EditorEvent::Changed => {
                self.queue_autosave(index, cx);
                if active {
                    // The checker's copy of the note, and the search's byte offsets,
                    // both belong to the note on screen.
                    self.send_note_to_checker(cx);
                    self.refresh_search(cx);
                }
            }
            // Hover is answered by the subscription `attach_window` keeps on the tab
            // that is on screen, which is the one that has a window to act on.
            EditorEvent::Hovered { .. } => {}
        }
        cx.notify();
    }

    /// Make tab `index` the one on screen. The note itself is already loaded — that is
    /// what a per-tab editor means — so what a tab change owes is the same cleanup
    /// opening a note does: a popup and a search belong to the note they were made on.
    pub(super) fn show_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.tab || index >= self.tabs.len() {
            return;
        }
        self.drop_popup(cx);
        self.clear_search(cx);
        self.tab = index;
        // The pointer can only hover over the note that is drawn, so the hover
        // subscription has to move with the frame. It needs a window, and this path
        // does not always have one, so the next frame is asked to do it.
        self.hover_stale = true;
        // For the same reason, the keyboard: the node that holds it is the drawn tab's
        // editor, and the entity it is sitting on is no longer in the tree.
        self.focus_stale = true;
        // The checker follows the note on screen, so its answers are the ones the
        // user can act on. `checker_opened` sends the new note and clears the old
        // note's diagnostics.
        if let Some(path) = self.note() {
            let text = self.editor().read_with(cx, |editor, _cx| editor.text());
            self.checker_opened(&path, &text, cx);
            self.say(path.display().to_string(), cx);
        } else {
            self.checker_closed(cx);
        }
        cx.notify();
    }

    /// Hand the keyboard to the tab on screen.
    ///
    /// A tab owns its editor, so the node the keystrokes reach is the drawn tab's
    /// editor. After a tab change the focus tracker still points at the entity that is
    /// no longer drawn, and every keystroke after it is lost — which is why every path
    /// that moves the frame marks this owed, and the next frame pays it.
    pub(super) fn focus_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_stale = false;
        let focus = self.editor().read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Move the tab on screen by `by`, wrapping at the ends: a tab bar is usually a
    /// handful of tabs, and wrapping beats counting them. `gt`/`gT` and
    /// `:tabnext`/`:tabprev` all come here.
    pub(super) fn select_tab(&mut self, by: isize, cx: &mut Context<Self>) {
        if self.tabs.len() < 2 {
            self.say("only one tab", cx);
            cx.notify();
            return;
        }
        let next = (self.tab as isize + by).rem_euclid(self.tabs.len() as isize) as usize;
        self.show_tab(next, cx);
    }

    /// Close a tab: its note reaches the disk first if it is ahead of it, and the tab
    /// goes. Closing the only tab clears it instead, which is the empty tab a window
    /// starts with — the window does not close because a tab did.
    ///
    /// **A write that fails keeps the tab open.** The user can see the reason in the
    /// status bar, and a tab that closes on a failed write is work that vanished
    /// because of it.
    pub(super) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        // A tab holding a note writes it before it goes; an empty tab has nothing to
        // write, and nothing to fail at writing.
        if self.tabs[index].path.is_some()
            && self.tabs[index]
                .editor
                .read_with(cx, |editor, _cx| editor.is_dirty())
        {
            self.write_tab(index, cx);
            if self.tabs[index]
                .editor
                .read_with(cx, |editor, _cx| editor.is_dirty())
            {
                cx.notify();
                return false;
            }
        }
        if self.tabs.len() == 1 {
            // The last tab: cleared rather than removed, so the window keeps its
            // invariant and the user keeps a place to type.
            self.clear_tab(0, cx);
            self.checker_closed(cx);
            self.say("no note open", cx);
        } else {
            self.tabs.remove(index);
            // The tab to the left, which is where the eye already is.
            self.tab = index.min(self.tabs.len() - 1);
            self.hover_stale = true;
            self.focus_stale = true;
            self.drop_popup(cx);
            self.clear_search(cx);
            if let Some(path) = self.note() {
                let text = self.editor().read_with(cx, |editor, _cx| editor.text());
                self.checker_opened(&path, &text, cx);
            }
        }
        cx.notify();
        true
    }

    /// Empty a tab without closing it: what `:tabonly` leaves, and what closing the
    /// last tab leaves.
    pub(super) fn clear_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.tabs[index].path = None;
        self.tabs[index]._autosave = None;
        self.tabs[index]
            .editor
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
    }

    /// `ctrl-t` / `:tabnew`: a new tab holding the note in hand. Two tabs on one note
    /// is the point — the same note read twice, each with its own cursor, scroll,
    /// folds and undo history.
    pub(super) fn tab_new(&mut self, _: &TabNew, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.note();
        let index = self.add_tab(cx);
        if let Some(path) = path.as_ref() {
            match self.vault.read(path) {
                Ok(text) => {
                    self.tabs[index].path = Some(path.clone());
                    self.tabs[index]
                        .editor
                        .update(cx, |editor, cx| editor.set_text(text, cx));
                    // The view came from the tab it was opened from, and `set_view`
                    // is what makes a preview tab read-only, so nothing else is owed
                    // here.
                }
                Err(err) => {
                    // The tab exists and is empty rather than the keystroke doing
                    // nothing: the status bar says why, and the user is one `:e` away
                    // from a note.
                    tracing::warn!(path = %path.display(), "failed to read note: {err:#}");
                    self.say(format!("{err:#}"), cx);
                }
            }
        }
        self.say(format!("{} tabs", self.tabs.len()), cx);
        // The keyboard comes here now rather than waiting for the next frame: this is
        // a keystroke the editor made, and the node it belongs to is the new tab's.
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// `g t` / `:tabnext`. The keyboard is handed to the tab that arrived on screen:
    /// the keystroke that got here came from the tab that is leaving it.
    pub(super) fn tab_next(&mut self, _: &TabNext, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab(1, cx);
        self.focus_active_tab(window, cx);
    }

    /// `g T` / `:tabprev`: the other direction, and the same debt to the keyboard.
    pub(super) fn tab_prev(&mut self, _: &TabPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab(-1, cx);
        self.focus_active_tab(window, cx);
    }

    /// `:tabclose`: close the tab on screen. Its note is written first, which is the
    /// same promise a quit makes with autosave on — a tab the user closes is not a
    /// wish to lose the note.
    pub(super) fn tab_close(&mut self, _: &TabClose, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab(self.tab, cx);
        self.focus_active_tab(window, cx);
    }

    /// `:tabonly`: close every tab but this one. The others' notes are written by the
    /// same rule `:tabclose` follows, and a tab whose write fails stays open.
    pub(super) fn tab_only(&mut self, _: &TabOnly, window: &mut Window, cx: &mut Context<Self>) {
        let keep = self.tab;
        let others: Vec<usize> = (0..self.tabs.len()).filter(|n| *n != keep).collect();
        let mut closed = 0;
        for index in others.into_iter().rev() {
            if self.close_tab(index, cx) {
                closed += 1;
            }
        }
        if closed == 0 {
            self.say("nothing else to close", cx);
        }
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// `:tabmove <n>`: the tab on screen goes to position `n`, which is how a tab bar
    /// gets ordered without dragging anything. The command line is the only door with
    /// a number in it, so it calls this directly rather than through an action — the
    /// same posture `:theme <name>` keeps.
    pub(super) fn move_tab(&mut self, to: usize, cx: &mut Context<Self>) {
        let from = self.tab;
        let to = to.min(self.tabs.len() - 1);
        if from == to {
            self.say(format!("tab {} of {}", from + 1, self.tabs.len()), cx);
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.tab = to;
        self.say(format!("tab {} of {}", to + 1, self.tabs.len()), cx);
        cx.notify();
    }

    /// The tab bar's own click: point at a tab and it is the note on screen, and the
    /// keyboard goes with it.
    pub(super) fn click_tab(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tab_for_id(id) else {
            return;
        };
        self.show_tab(index, cx);
        let handle = self.editor().read(cx).focus_handle.clone();
        window.focus(&handle, cx);
    }

    /// The tab bar's middle click, and the `×` on a tab: close it. The keyboard has
    /// to be moved off a closed tab's editor for the same reason a tab change moves it
    /// on.
    pub(super) fn click_tab_close(
        &mut self,
        id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tab_for_id(id) else {
            return;
        };
        self.close_tab(index, cx);
        self.focus_active_tab(window, cx);
    }

    /// Every tab's note to the disk, for the exits that owe more than the note on
    /// screen: `:q`, `:wq`, a termination signal, and the window manager's close.
    /// Whether every tab came out clean, which is what lets a quit decide whether it
    /// may happen.
    pub(super) fn write_all_tabs(&mut self, cx: &mut Context<Self>) -> bool {
        let mut clean = true;
        for index in 0..self.tabs.len() {
            if !self.tabs[index]
                .editor
                .read_with(cx, |editor, _cx| editor.is_dirty())
            {
                continue;
            }
            self.write_tab(index, cx);
            if self.tabs[index]
                .editor
                .read_with(cx, |editor, _cx| editor.is_dirty())
            {
                clean = false;
            }
        }
        clean
    }

    /// Whether any tab is ahead of its note on disk, which is what `:q` asks with
    /// autosave off, and what the tab bar's dots show.
    pub(super) fn any_dirty(&self, cx: &gpui::App) -> bool {
        self.tabs
            .iter()
            .any(|tab| tab.editor.read_with(cx, |editor, _cx| editor.is_dirty()))
    }

    /// The tabs as the tab bar and the session file want them: path, whether the note
    /// is ahead of the disk, and the view the tab is in.
    pub(crate) fn tab_state(&self, cx: &gpui::App) -> Vec<TabState> {
        self.tabs
            .iter()
            .map(|tab| TabState {
                id: tab.id,
                path: tab.path.clone(),
                dirty: tab.editor.read_with(cx, |editor, _cx| editor.is_dirty()),
                view: tab.editor.read(cx).view().word().to_string(),
            })
            .collect()
    }
}

/// What a tab looks like from outside it: enough for the tab bar to draw a row and
/// for the session file to remember one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TabState {
    /// The tab's stable id, which is what a click is answered with.
    pub(crate) id: usize,
    /// The note it holds, or `None` for an empty tab.
    pub(crate) path: Option<PathBuf>,
    /// Whether its note is ahead of the disk: the dot on the tab.
    pub(crate) dirty: bool,
    /// Which of the three views the tab is in, since a tab keeps its own.
    pub(crate) view: String,
}
