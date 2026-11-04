//! The session file: what the vault looked like last time (roadmap P1.8).
//!
//! The shape is the config's, applied to window state: an incremental JSON file
//! in `.shale/`, every field optional, written atomically. What goes in it is
//! What goes in it is
//! what the *user chose inside a window* and no other place remembers — the
//! open note and where they stood in it, the tree's open folders, the view,
//! focus mode and its scope, typewriter scroll, the jump chain, and the graph
//! tab's camera and filter. What stays out
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
use shale_core::graph::filter::GraphFilter;
use shale_core::session::{GraphFilterSeat, Jump as CoreJump, Tab as CoreTab};
use shale_core::Session;

use super::Workspace;
use crate::dim;
use crate::editor::View;

/// The window's graph filter as a session seat, or `None` while it is the default.
///
/// Absent is the same answer as everything-on, so writing the seat only when it
/// differs keeps the file incremental — and it is what lets a session written before
/// the filter panel exist restore a graph with everything on. `folder` is left out of
/// both directions: it is M3's scope, nothing sets it yet, and a seat that cannot be
/// produced should not be one that can be read.
fn filter_seat(filter: &GraphFilter) -> Option<GraphFilterSeat> {
    let seat = GraphFilterSeat {
        orphans: filter.orphans,
        existing_only: filter.existing_only,
        tags: filter.tags,
        // A field the user opened and left empty is not a filter, and a session file
        // that says `"search": ""` reads like one that does.
        search: filter
            .search
            .clone()
            .filter(|query| !query.trim().is_empty()),
    };
    (seat != GraphFilterSeat::default()).then_some(seat)
}

impl Workspace {
    /// Everything the next window should know, as of this frame.
    ///
    /// `pub(super)` rather than private so the test-only surface can report what a
    /// window would write; writing it stays this module's business.
    pub(super) fn capture_session(&self, cx: &mut Context<Self>) -> Session {
        // The seat: where the user was in the note on screen. The graph tab has no
        // buffer, so every one of these stays absent rather than remembering the
        // place in a note that was not on screen.
        let seat = self.editor().map(|editor| {
            editor.read_with(cx, |editor, _cx| {
                (
                    editor.cursor_offset(),
                    editor.scroll_y(),
                    editor.view().word().to_string(),
                    editor.focus_mode(),
                    editor.focus_scope().word().to_string(),
                    editor.typewriter(),
                )
            })
        });
        let (back, forward) = self.jumps.snapshot();
        let mut expanded: Vec<_> = self.expanded.iter().cloned().collect();
        expanded.sort();
        // An empty tab is not a tab worth restoring: it holds no note, and a session
        // file full of nulls is noise. One empty tab means "the window had nothing
        // open", which `open_note: None` already says. The graph tab is worth
        // restoring even with no path, because the graph is the thing that was open.
        let tabs = self
            .tab_state(cx)
            .into_iter()
            .filter(|tab| tab.path.is_some() || tab.graph)
            .map(|tab| CoreTab {
                path: tab.path,
                view: tab.view,
                graph: tab.graph,
                camera: tab.camera,
            })
            .collect();
        Session {
            open_note: self.note(),
            cursor: seat.as_ref().map(|seat| seat.0),
            scroll_y: seat.as_ref().map(|seat| seat.1),
            expanded,
            view: seat.as_ref().map(|seat| seat.2.clone()),
            focus: seat.as_ref().map(|seat| seat.3),
            focus_scope: seat.as_ref().map(|seat| seat.4.clone()),
            typewriter: seat.as_ref().map(|seat| seat.5),
            tree_visible: Some(self.tree_visible),
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
            active_tab: Some(self.pane().active),
            graph_filter: filter_seat(&self.graph_filter),
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

        // The tree, before any frame is drawn: a window that opens with its sidebar
        // away should never have had one painted first to move it out of the way.
        if let Some(visible) = session.tree_visible {
            self.tree_visible = visible;
        }

        // The graph filter, before any tab is opened: the graph tab's first frame is
        // built from the window's filter, so a seat that arrives afterwards would be
        // restored into a picture that had already been laid out without it.
        if let Some(seat) = session.graph_filter.as_ref() {
            self.graph_filter = GraphFilter {
                orphans: seat.orphans,
                existing_only: seat.existing_only,
                tags: seat.tags,
                search: seat.search.clone(),
                folder: None,
            };
        }

        // The tabs, left to right (roadmap P3.1). A session file written before tabs
        // have none, and the `open_note` branch below is that case — which is also
        // what a one-tab session writes today, so the two agree.
        let had_tabs = !session.tabs.is_empty();
        if had_tabs {
            for tab in session.tabs.iter() {
                if tab.graph {
                    // The graph tab comes through the same door `:graph` uses, so its
                    // index is built exactly as an interactive open would build it —
                    // and the one-per-window rule means a file that somehow names two
                    // graph tabs restores one.
                    self.open_graph(cx);
                    // Where the user had left the view. Set before anything is painted,
                    // so the first frame uses their camera rather than a fresh fit that
                    // the next frame would have to undo.
                    if let (Some(seat), Some(view)) = (tab.camera, self.graph_view().cloned()) {
                        view.update(cx, |view, _cx| {
                            view.set_camera(Some(shale_core::graph::geom::Camera::new(
                                seat.pan, seat.zoom,
                            )))
                        });
                    }
                    continue;
                }
                let Some(path) = tab.path.as_ref() else {
                    continue;
                };
                // Every tab comes through the same door `ctrl-t` uses, so a restored
                // tab is wired up exactly as a new one would be. The empty tab the
                // window started with is dropped at the end of this block.
                self.add_tab(cx);
                if self.vault.exists(path) {
                    self.load_note(path, cx);
                    // Each tab keeps the view it was in: one note left in raw mode to
                    // read another in preview is the point of the per-tab editor.
                    if let Some(editor) = self.editor().cloned() {
                        editor.update(cx, |editor, cx| {
                            editor.set_view(View::from_word(&tab.view), cx)
                        });
                    }
                }
            }
            // A note deleted since last time is not a session, it is a memory, so its
            // tab goes — and with it the empty tab the window started with. The window
            // keeps the at-least-one-tab invariant either way.
            self.pane_mut()
                .tabs
                .retain(|tab| tab.path().is_some() || tab.is_graph());
            if self.pane().tabs.is_empty() {
                self.add_tab(cx);
            }
            self.pane_mut().active = session
                .active_tab
                .unwrap_or(0)
                .min(self.pane().tabs.len() - 1);
            self.mark_note_mru();
            // The cursor and scroll belong to the tab that had the keyboard: the
            // session remembers one seat, the one the user was looking at. With the
            // graph on screen there is no seat to put back.
            if let Some(offset) = session.cursor {
                if let Some(editor) = self.editor().cloned() {
                    editor.update(cx, |editor, cx| {
                        editor.restore_place(offset, session.scroll_y, cx)
                    });
                }
            }
        } else if let Some(note) = session.open_note.as_ref() {
            // A note deleted since last time is not a session, it is a memory;
            // the window opens with nothing rather than an error on frame one.
            if self.vault.exists(note) {
                self.load_note(note, cx);
                if self.note().as_deref() == Some(note.as_path()) {
                    if let Some(offset) = session.cursor {
                        if let Some(editor) = self.editor().cloned() {
                            editor.update(cx, |editor, cx| {
                                editor.restore_place(offset, session.scroll_y, cx)
                            });
                        }
                    }
                }
            }
        }

        // Focus mode and typewriter scroll are the frame's decisions rather than the
        // note's, so they land on the tab that had the keyboard. With tabs open the
        // other tabs start as the editor's defaults have them; per-tab focus is a
        // P3.2 question, since it is the same question a pane asks. The graph tab has
        // neither, so there is nothing to land them on.
        if let Some(editor) = self.editor().cloned() {
            editor.update(cx, |editor, cx| {
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
        }

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

        // Folders come back closed unless the session says otherwise, so the note the
        // window is standing on has to have its path opened or it would be a row that
        // does not exist.
        if let Some(path) = self.note() {
            self.expand_ancestors(&path);
        }
    }
}
