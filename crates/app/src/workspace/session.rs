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
use shale_core::session::Jump as CoreJump;
use shale_core::Session;

use super::Workspace;
use crate::dim;
use crate::editor::View;

impl Workspace {
    /// Everything the next window should know, as of this frame.
    fn capture_session(&self, cx: &mut Context<Self>) -> Session {
        let editor = self.editor.read_with(cx, |editor, _cx| {
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
        Session {
            open_note: self.active.clone(),
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

        if let Some(note) = session.open_note.as_ref() {
            // A note deleted since last time is not a session, it is a memory;
            // the window opens with nothing rather than an error on frame one.
            if self.vault.exists(note) {
                self.load_note(note, cx);
                if self.active.as_deref() == Some(note.as_path()) {
                    if let Some(offset) = session.cursor {
                        self.editor.update(cx, |editor, cx| {
                            editor.restore_place(offset, session.scroll_y, cx)
                        });
                    }
                }
            }
        }

        self.editor.update(cx, |editor, cx| {
            if let Some(view) = session.view.as_deref() {
                editor.set_view(View::from_word(view), cx);
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
