//! The session file: what the vault looked like last time (roadmap P1.8).
//!
//! The shape is the config's, applied to window state: an incremental JSON file
//! in `.shale/`, every field optional, written atomically. What goes in it is
//! what the *user chose inside a window* and no other place remembers — the
//! open note and where they stood in it, the tree's open folders, the view,
//! focus mode and its scope, typewriter scroll, the jump chain. What stays out
//! is what the config owns (settings edited by hand) and what the index owns
//! (anything derived from the notes).
//!
//! Two beats write it: the autosave's pause and every graceful exit (`:q`, the
//! window manager's close, a termination signal). Restore happens once, in
//! [`Workspace::new`], before the first frame — so the window comes up on the
//! note the user was reading, not on an empty editor they have to navigate back
//! to. A session file that cannot be parsed is a log line and a fresh start:
//! forgotten state is sad, a window that will not open is worse.

use gpui::Context;
use shale_core::session::{Jump as CoreJump, Tab as CoreTab};
use shale_core::Session;

use super::Workspace;
use crate::dim;
use crate::editor::View;

impl Workspace {
    /// Everything the next window should know, as of this frame.
    fn capture_session(&self, cx: &mut Context<Self>) -> Session {
        let editor = self.editor().read_with(cx, |editor, _cx| {
            (
                editor.cursor_offset(),
                editor.scroll_y(),
                editor.view().word().to_string(),
                editor.focus_mode(),
                editor.focus_scope().word().to_string(),
                editor.typewriter(),
            )
        });
        let (back, forward) = self.jumps.snapshot();
        let mut expanded: Vec<_> = self.expanded.iter().cloned().collect();
        expanded.sort();
        // An empty tab is not a tab worth restoring: it holds no note, and a session
        // file full of nulls is noise. One empty tab means "the window had nothing
        // open", which `open_note: None` already says.
        let tabs = self
            .tab_state(cx)
            .into_iter()
            .filter_map(|tab| {
                tab.path.map(|path| CoreTab {
                    path,
                    view: tab.view,
                })
            })
            .collect();
        Session {
            open_note: self.note(),
            cursor: Some(editor.0),
            scroll_y: Some(editor.1),
            expanded,
            view: Some(editor.2),
            focus: Some(editor.3),
            focus_scope: Some(editor.4),
            typewriter: Some(editor.5),
            jumps_back: back
                .into_iter()
                .map(|jump| CoreJump {
                    path: jump.path,
                    cursor: jump.cursor,
                })
                .collect(),
            jumps_forward: forward
                .into_iter()
                .map(|jump| CoreJump {
                    path: jump.path,
                    cursor: jump.cursor,
                })
                .collect(),
            tabs,
            active_tab: Some(self.tab),
        }
    }

    /// Write the session file. Best-effort like everything that is not the
    /// notes: a session that did not reach the disk is tomorrow's shrug, not a
    /// reason to fail the save or the exit that triggered it.
    pub(super) fn save_session(&self, cx: &mut Context<Self>) {
        if let Err(err) = self.capture_session(cx).save(self.vault.root()) {
            tracing::warn!("the session was not saved: {err:#}");
        }
    }

    /// Put the window back the way the last one left it, if a session file
    /// says how. Called once from [`Workspace::new`].
    ///
    /// The order is the note first (its buffer has to exist before a cursor, a
    /// scroll or the read-only half of preview can mean anything), then the
    /// frame's decisions, then the jump chain — and the note opens through
    /// `load_note`, not `open_note`, because restoring a seat is not a jump and
    /// must not push the restored chain around.
    pub(super) fn restore_session(&mut self, cx: &mut Context<Self>) {
        let session = match Session::load(self.vault.root()) {
            Ok(session) => session,
            Err(err) => {
                tracing::warn!("the session file was not understood, starting fresh: {err:#}");
                return;
            }
        };
        if session == Session::default() {
            return;
        }

        self.expanded = session.expanded.into_iter().collect();

        // The tabs, left to right (roadmap P3.1). A session file written before tabs
        // have none, and the `open_note` branch below is that case — which is also
        // what a one-tab session writes today, so the two agree.
        let had_tabs = !session.tabs.is_empty();
        if had_tabs {
            for (index, tab) in session.tabs.iter().enumerate() {
                // The window's first tab is already there; the rest come through the
                // same door `ctrl-t` uses, so a restored tab is wired up exactly as a
                // new one would be.
                if index > 0 {
                    self.add_tab(cx);
                }
                if self.vault.exists(&tab.path) {
                    self.load_note(&tab.path, cx);
                    // Each tab keeps the view it was in: one note left in raw mode to
                    // read another in preview is the point of the per-tab editor.
                    self.editor().update(cx, |editor, cx| {
                        editor.set_view(View::from_word(&tab.view), cx)
                    });
                }
            }
            // A note deleted since last time is not a session, it is a memory, so its
            // tab goes — and the window keeps the at-least-one-tab invariant.
            self.tabs.retain(|tab| tab.path.is_some());
            if self.tabs.is_empty() {
                self.add_tab(cx);
            }
            self.tab = session.active_tab.unwrap_or(0).min(self.tabs.len() - 1);
            // The cursor and scroll belong to the tab that had the keyboard: the
            // session remembers one seat, the one the user was looking at.
            if let Some(offset) = session.cursor {
                self.editor().update(cx, |editor, cx| {
                    editor.restore_place(offset, session.scroll_y, cx)
                });
            }
        } else if let Some(note) = session.open_note.as_ref() {
            // A note deleted since last time is not a session, it is a memory;
            // the window opens with nothing rather than an error on frame one.
            if self.vault.exists(note) {
                self.load_note(note, cx);
                if self.note().as_deref() == Some(note.as_path()) {
                    if let Some(offset) = session.cursor {
                        self.editor().update(cx, |editor, cx| {
                            editor.restore_place(offset, session.scroll_y, cx)
                        });
                    }
                }
            }
        }

        // Focus mode and typewriter scroll are the frame's decisions rather than the
        // note's, so they land on the tab that had the keyboard. With tabs open the
        // other tabs start as the editor's defaults have them; per-tab focus is a
        // P3.2 question, since it is the same question a pane asks.
        self.editor().update(cx, |editor, cx| {
            // The view is a tab's own, and the loop above already handed each tab the
            // view it was left in. `session.view` is the pre-tab field: it is only
            // read when there were no tabs to ask, where it describes the one tab.
            if !had_tabs {
                if let Some(view) = session.view.as_deref() {
                    editor.set_view(View::from_word(view), cx);
                }
            }
            if session.focus == Some(true) {
                editor.set_focus_mode(true, cx);
            }
            if let Some(scope) = session.focus_scope.as_deref() {
                editor.set_focus_scope(dim::Scope::from_word(scope), cx);
            }
            if session.typewriter == Some(true) {
                editor.set_typewriter(true, cx);
            }
        });

        let back = session
            .jumps_back
            .into_iter()
            .map(|jump| super::jumps::Jump {
                path: jump.path,
                cursor: jump.cursor,
            })
            .collect();
        let forward = session
            .jumps_forward
            .into_iter()
            .map(|jump| super::jumps::Jump {
                path: jump.path,
                cursor: jump.cursor,
            })
            .collect();
        self.jumps.restore(back, forward);
    }
}
