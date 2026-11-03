//! The window's half of the live channel (roadmap P2.3): publishing what the user is
//! looking at, and owning the server that answers for it.
//!
//! The HTTP thread cannot read gpui state — that belongs to the main thread — so the
//! window copies it into a [`Snapshot`] and the thread answers from the copy. The copy
//! happens on a short beat rather than on every change: an agent asks a question once
//! in a while, and a copy per keystroke would be work paid for by nobody. A quarter of
//! a second is shorter than anything an agent can act on, and the same shape as the
//! termination-signal poll this crate already uses for "a thread that must be able to
//! be told to stop".
//!
//! The server is a *feature*, not a guarantee: a port that cannot be bound leaves the
//! window exactly as it was and says so in the log, in the same posture as a spell
//! server that will not start.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::Context;
use shale_core::vim::Mode;

use crate::live::{LiveServer, Snapshot};

use super::Workspace;

/// How often the window republishes its live state.
const LIVE_REFRESH: Duration = Duration::from_millis(250);

/// The vim mode in the lowercase words the live API uses, rather than the status bar's
/// uppercase ones: this string is an agent's input as much as a human's reading.
fn mode_word(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "normal",
        Mode::Insert => "insert",
        Mode::Visual => "visual",
        Mode::VisualLine => "visual-line",
    }
}

/// What has the keyboard, which is what a future live mutation would have to queue
/// behind. Only one of these is ever open at a time.
fn overlay_word(picker: bool, prompt: bool, popup: bool) -> &'static str {
    if picker {
        "picker"
    } else if prompt {
        "prompt"
    } else if popup {
        "popup"
    } else {
        "none"
    }
}

/// A stop on the jump chain, named the way the live API names places: the note's
/// vault-relative path, or `"graph"` for the graph tab — the same word `live_note`
/// reports for `view`, so an agent can tell that a `ctrl-o` will move the window to a
/// view of the whole vault rather than to a file.
fn jump_word(path: Option<std::path::PathBuf>) -> String {
    match path {
        Some(path) => path.display().to_string(),
        None => "graph".to_string(),
    }
}

impl Workspace {
    /// Start the live channel: publish once, bind the port, and keep publishing.
    ///
    /// Called from `Workspace::new` when `[mcp] live` is on. Nothing else in the
    /// window depends on it, so a failure here is a log line and a missing feature.
    pub(crate) fn start_live(&mut self, cx: &mut Context<Self>) {
        if !self.config.mcp.live {
            tracing::debug!("the live MCP channel is off: [mcp] live = false");
            return;
        }
        let shared = Arc::new(Mutex::new(Snapshot::default()));
        self.live_snapshot = Some(Arc::clone(&shared));
        self.publish_live(cx);

        match LiveServer::start(self.vault.root(), shared) {
            Ok(server) => {
                // The port is in the lockfile for `shale-mcp` to read; the log line
                // is for the human wondering whether the channel came up at all.
                tracing::debug!(port = server.port(), "the live MCP channel is up");
                self.live = Some(server);
                self._live = Some(cx.spawn(async move |ws, cx| {
                    loop {
                        cx.background_executor().timer(LIVE_REFRESH).await;
                        // The weak handle: when the window is gone there is nothing
                        // left to publish, and the beat ends with it.
                        if ws.update(cx, |ws, cx| ws.publish_live(cx)).is_err() {
                            break;
                        }
                    }
                }));
            }
            Err(err) => {
                self.live_snapshot = None;
                tracing::warn!("the live MCP channel is off: {err:#}");
            }
        }
    }

    /// Copy what the window is showing into the snapshot the server answers from.
    ///
    /// Deliberately not the note's text: an agent that wants it has the vault tools,
    /// which read the disk, and the disk is where a note's meaning lives. What only
    /// the window knows is where the user is *in* it.
    ///
    /// `pub(crate)` because the beat is not the only way to want a publish: the tests
    /// ask for one directly rather than waiting on simulated time.
    pub(crate) fn publish_live(&mut self, cx: &mut Context<Self>) {
        let Some(shared) = &self.live_snapshot else {
            return;
        };
        let (back, forward) = self.jumps.snapshot();
        // The graph tab has no buffer, so the snapshot says what is on screen and
        // leaves the cursor fields at their nothing rather than repeating the last
        // note's. An agent reading `view: "graph"` knows why they are empty.
        let (note, dirty, view, mode, row, col, byte, selection) = match self.editor() {
            Some(handle) => {
                let editor = handle.read(cx);
                let (row, col) = editor.cursor_position();
                (
                    self.note().as_ref().map(|path| path.display().to_string()),
                    editor.is_dirty(),
                    editor.view().word().to_string(),
                    mode_word(editor.mode()).to_string(),
                    row,
                    col,
                    editor.cursor_offset(),
                    editor.selection(),
                )
            }
            None => (
                None,
                false,
                "graph".to_string(),
                String::new(),
                0,
                0,
                0,
                None,
            ),
        };
        let snapshot = Snapshot {
            note,
            dirty,
            view,
            mode,
            row,
            col,
            byte,
            selection,
            back: back.into_iter().map(|jump| jump_word(jump.path)).collect(),
            forward: forward
                .into_iter()
                .map(|jump| jump_word(jump.path))
                .collect(),
            overlay: overlay_word(
                self.picker.is_some(),
                self.prompt.is_some(),
                self.popup.is_some(),
            )
            .to_string(),
            updated_at: shale_core::now_epoch(),
        };
        // The lock is held only long enough to swap the copy, so a request that
        // arrives mid-publish is answered from the previous beat rather than waiting.
        if let Ok(mut slot) = shared.lock() {
            *slot = snapshot;
        }
    }

    /// Take the live channel down: stop the beat, stop answering, and take the
    /// lockfile away. Every graceful exit runs this, so the next `shale-mcp` to look
    /// finds nothing rather than a stale hint.
    pub(crate) fn stop_live(&mut self) {
        self._live = None;
        self.live_snapshot = None;
        if let Some(server) = self.live.take() {
            if let Err(err) = server.stop() {
                tracing::warn!("could not remove the MCP lock: {err:#}");
            }
        }
    }
}
