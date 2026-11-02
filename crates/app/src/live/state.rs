//! What the window publishes for the live channel, and how each endpoint answers
//! from it (roadmap P2.3).
//!
//! The HTTP thread never reaches into the window: gpui's state belongs to the main
//! thread. So the window *publishes* a copy of what an agent is allowed to ask about,
//! and the thread answers from the copy. That is why this type is a plain data
//! snapshot rather than a view of the editor, and why it holds no note text — an
//! agent that wants the text has the vault tools, which read the disk, and the disk
//! is where a note's meaning lives.
//!
//! The staleness this buys is one refresh beat (see `workspace::live`), which is
//! shorter than anything an agent can act on.

use serde_json::{json, Value};
use shale_core::live::{Status, LIVE_TOOLS, PATH_JUMPS, PATH_NOTE, PATH_SELECTION, PATH_STATUS};

/// One window's live state, as the HTTP thread is allowed to see it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Snapshot {
    /// The open note, vault-relative. `None` when the window holds no note.
    pub note: Option<String>,
    /// Whether what is on screen is ahead of what is on disk.
    pub dirty: bool,
    /// The view: `live`, `raw` or `preview`.
    pub view: String,
    /// The vim mode: `normal`, `insert` or `visual`.
    pub mode: String,
    /// The cursor's buffer row, 0-based, as the buffer counts it.
    pub row: usize,
    /// …and its column in characters.
    pub col: usize,
    /// The cursor as a byte offset into the note — the form the buffer and the
    /// editor's own tools take, so an agent can hand it straight back.
    pub byte: usize,
    /// The selection as byte offsets `(low, high)`, however it was made: visual mode
    /// or the mouse. `None` when nothing is selected.
    pub selection: Option<(usize, usize)>,
    /// The jump chain, nearest first, as vault-relative paths.
    pub back: Vec<String>,
    /// …and the half `ctrl-i` walks forward through.
    pub forward: Vec<String>,
    /// What has the keyboard: `none`, `picker`, `prompt` or `popup`. A live
    /// mutation will have to queue behind anything but `none`; the field is here now
    /// so that rule has something to read when the mutations arrive.
    pub overlay: String,
    /// When this was published, in unix epoch seconds — the same stamp the index and
    /// the lockfile use.
    pub updated_at: i64,
}

impl Snapshot {
    /// The JSON one of the live endpoints answers with, or `None` for a path there is
    /// no answer for — which the server turns into a 404 that names what *does*
    /// exist.
    ///
    /// `/status` is built through [`Status`] rather than written out here, so the
    /// shape the app answers with and the shape `shale-mcp` parses cannot drift.
    pub(crate) fn answer(&self, path: &str, version: &str, port: u16) -> Option<Value> {
        match path {
            PATH_STATUS => {
                let status = Status {
                    app_running: true,
                    live_tools: LIVE_TOOLS.iter().map(|name| (*name).to_string()).collect(),
                    version: version.to_string(),
                    port,
                    overlay: self.overlay.clone(),
                };
                serde_json::to_value(status).ok()
            }
            PATH_NOTE => Some(json!({
                "path": self.note,
                "dirty": self.dirty,
                "view": self.view,
                "mode": self.mode,
                "cursor": { "row": self.row, "col": self.col, "byte": self.byte },
                "updated_at": self.updated_at,
            })),
            PATH_SELECTION => Some(json!({
                "path": self.note,
                "mode": self.mode,
                "selection": self
                    .selection
                    .map(|(low, high)| json!({ "start": low, "end": high })),
                "updated_at": self.updated_at,
            })),
            PATH_JUMPS => Some(json!({
                "back": self.back,
                "forward": self.forward,
                "current": self.note,
                "updated_at": self.updated_at,
            })),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            note: Some("notes/Idea.md".into()),
            dirty: true,
            view: "live".into(),
            mode: "visual".into(),
            row: 2,
            col: 4,
            byte: 31,
            selection: Some((28, 40)),
            back: vec!["notes/Before.md".into()],
            forward: Vec::new(),
            overlay: "none".into(),
            updated_at: 1_700_000_000,
        }
    }

    #[test]
    fn the_status_answer_is_the_shape_the_client_parses() {
        let body = snapshot().answer(PATH_STATUS, "0.1.0", 4321).unwrap();
        let status: Status = serde_json::from_value(body).unwrap();
        assert!(status.app_running);
        assert_eq!(status.port, 4321);
        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.live_tools, LIVE_TOOLS);
        assert_eq!(status.overlay, "none");
    }

    #[test]
    fn the_note_answer_carries_the_cursor_in_every_form() {
        let body = snapshot().answer(PATH_NOTE, "0.1.0", 4321).unwrap();
        assert_eq!(body["path"], "notes/Idea.md");
        assert!(body["dirty"].as_bool().unwrap());
        assert_eq!(body["cursor"]["byte"], 31);
        assert_eq!(body["cursor"]["row"], 2);
    }

    #[test]
    fn a_window_with_no_note_and_no_selection_says_so_rather_than_failing() {
        let empty = Snapshot::default();
        let body = empty.answer(PATH_NOTE, "0.1.0", 1).unwrap();
        assert!(
            body["path"].is_null(),
            "no note open is an answer, not an error"
        );
        let body = empty.answer(PATH_SELECTION, "0.1.0", 1).unwrap();
        assert!(body["selection"].is_null());
    }

    #[test]
    fn the_jump_answer_walks_both_directions() {
        let body = snapshot().answer(PATH_JUMPS, "0.1.0", 1).unwrap();
        assert_eq!(body["back"][0], "notes/Before.md");
        assert_eq!(body["forward"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn a_path_nobody_answers_for_is_none() {
        assert!(snapshot().answer("/nonsense", "0.1.0", 1).is_none());
    }
}
