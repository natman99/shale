//! Session state: what the vault looked like last time (roadmap P1.8).
//!
//! The file is `<vault>/.shale/session.json`, and the rule that puts it in a
//! file rather than the DB: the DB is a cache, derived from the notes and safe
//! to delete; the session is state the *user chose* — which note they were
//! reading, how far they had scrolled — and it has to survive a cache rebuild.
//! It is written with the same atomic discipline as every other file Shale
//! owns ([`crate::atomic`]).
//!
//! Every field is optional and defaults to "nothing to restore", so the file
//! stays **incremental** like the config: a Shale that knows more fields writes
//! more, an older one reading a newer file ignores what it does not know
//! (serde's default), and a half-written key set still restores the half.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::db::STATE_DIR;

/// The session file inside [`STATE_DIR`].
pub fn session_path(vault_root: &Path) -> PathBuf {
    vault_root.join(STATE_DIR).join("session.json")
}

/// One stop on the jump chain: a note and where the cursor was left in it, or the
/// graph tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Jump {
    /// Vault-relative path of the note. `None` is the graph tab, which is a place in
    /// the window rather than a file (roadmap P3.4). Optional, and defaulted, so a
    /// session file written before a graph could be a stop on the chain reads exactly
    /// as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// The cursor there, as a byte offset into the note. `0` for the graph tab, which
    /// has no cursor to put back — its camera comes from the tab's own seat.
    pub cursor: usize,
}

/// One open tab, as the session remembers it (roadmap P3.1).
///
/// The view is per tab because a tab keeps its own: leaving one note in raw mode to
/// read another in preview is the whole point of having both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    /// Vault-relative path of the note the tab holds. `None` for a tab that holds no
    /// note — the graph tab (roadmap P3.4). Optional rather than an empty path so a
    /// session file written before a tab could hold anything else still restores
    /// exactly as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// Which of the three views that tab was in: `raw`, `live` or `preview`. Empty for
    /// a tab that has no note to be in a view.
    pub view: String,
    /// Whether this tab is the vault's graph. At most one tab in a window is, and its
    /// [`Self::path`] is `None`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub graph: bool,
    /// Where the graph tab was looking (roadmap P3.4). `None` means "fit the whole
    /// graph", which is also what a session written before a graph tab could be panned
    /// restores to. Like the scroll position of a note, this is state the user chose in
    /// a window, so it belongs here rather than in the config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<GraphSeat>,
}

/// The graph tab's camera, as the session remembers it.
///
/// Three numbers, and no attempt at precision: `f32` is what the camera itself is
/// made of, and a restored view that is a fraction of a pixel off is not a fact a
/// person can notice.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GraphSeat {
    /// Screen px the world origin lands at.
    pub pan: [f32; 2],
    /// World units to screen px.
    pub zoom: f32,
}

/// serde: a flag that is `false` is the default, so it does not belong in the file.
fn is_false(value: &bool) -> bool {
    !*value
}

/// What the graph tab was showing, as the session remembers it.
///
/// The three toggles and the search line, and nothing else: `folder` is M3's
/// constellation scope and has no way to be set yet, so it has no business in the
/// file. Like the camera, this is a decision made inside a window rather than a
/// setting to edit by hand, which is why it is here and not in the config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphFilterSeat {
    /// Whether notes that link nothing were shown.
    pub orphans: bool,
    /// Whether links to files that do not exist were hidden.
    pub existing_only: bool,
    /// Whether `#tag` nodes were shown.
    pub tags: bool,
    /// What was typed in the search line, if anything.
    pub search: Option<String>,
}

impl Default for GraphFilterSeat {
    /// The same defaults as the filter itself: everything on.
    fn default() -> Self {
        Self {
            orphans: true,
            existing_only: false,
            tags: true,
            search: None,
        }
    }
}

/// What the next window should look like.
///
/// Deliberately *not* saved: anything the user edits by hand (that is the
/// config's job), and anything derived (the index, the checker's notes). The pane
/// layout will extend this file when P3.2 lands — same file, more fields, same
/// defaults.
/// Where a window sat and how big it was, in logical pixels.
///
/// The app *offers* this and the platform decides how much to honor: X11 and
/// Windows place the window as asked; Wayland takes the size as a hint and
/// picks the position itself. A stored rect that lands on no display at all
/// (an unplugged monitor) is discarded by the reader, not chased.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowGeom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Whether the window was maximized. The rect is still stored alongside:
    /// it is the size to restore *to* when the user un-maximizes.
    pub maximized: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// The note that was open, vault-relative.
    pub open_note: Option<PathBuf>,
    /// The cursor in that note, as a byte offset into it. An offset rather than
    /// a row and column because it is what the buffer takes directly, and a
    /// stale one is simply clamped at restore.
    pub cursor: Option<usize>,
    /// The editor's vertical scroll offset, in gpui's own units and sign —
    /// saved as it came from the scroll handle and handed straight back.
    pub scroll_y: Option<f32>,
    /// The folders expanded in the file tree.
    pub expanded: Vec<PathBuf>,
    /// The view the note was in: `raw`, `live` or `preview`.
    pub view: Option<String>,
    /// Whether focus mode was on, and its scope (`block` or `section`). These
    /// three — focus, scope, typewriter — left the config for exactly this
    /// item: they are decisions made inside a window, not settings edited by
    /// hand.
    pub focus: Option<bool>,
    /// The focus scope word.
    pub focus_scope: Option<String>,
    /// Whether typewriter scroll was on.
    pub typewriter: Option<bool>,
    /// Whether the file tree was showing. `None` is showing, so a session file that
    /// predates the toggle restores a window with its tree. Like focus mode, this is a
    /// decision made inside a window rather than a setting to edit by hand.
    pub tree_visible: Option<bool>,
    /// The jump chain: the notes behind the user (oldest first) and the ones
    /// ahead, the way `ctrl-o`/`ctrl-i` walk them.
    pub jumps_back: Vec<Jump>,
    /// The forward half of the chain.
    pub jumps_forward: Vec<Jump>,
    /// The open tabs, left to right on the tab bar (roadmap P3.1). Empty means the
    /// file predates tabs, and [`Session::open_note`] is the whole of it — which is
    /// what keeps an old session file restoring exactly one note.
    pub tabs: Vec<Tab>,
    /// Which of [`Session::tabs`] had the keyboard. Out of range is clamped at
    /// restore, because a tab closed since the file was written is not an error.
    pub active_tab: Option<usize>,
    /// The window's frame: size, position, maximized. `None` is "no opinion"
    /// and the platform places the window. Saved per window slot, since two
    /// windows on one vault sit in different places.
    pub window: Option<WindowGeom>,
    /// What the graph tab was showing (roadmap P3.4 Task 8). `None` is the default
    /// filter rather than "no graph", so a session file that predates the filter panel
    /// restores a graph with everything on.
    pub graph_filter: Option<GraphFilterSeat>,
}

/// The session file: one [`Session`] slot per window that last showed the
/// vault. A window writes its own slot and only its own; the file is the
/// union, so two windows closing in either order both land.
///
/// The file used to be a single bare [`Session`] object. A file that is not
/// in slots form is read as slot 0 — one old-format file restores exactly
/// the one window it remembered — and the next write migrates it.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sessions {
    pub slots: Vec<Session>,
}

impl Sessions {
    /// Read the whole file. No file is the normal case: empty slots.
    pub fn load(vault_root: &Path) -> Result<Sessions> {
        let path = session_path(vault_root);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Sessions::default());
            }
            Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
        };
        let value: serde_json::Value =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        if value.get("slots").is_some() {
            serde_json::from_value(value).with_context(|| format!("parsing {}", path.display()))
        } else {
            // Pre-slots format: one window's Session, now slot 0.
            let one: Session = serde_json::from_value(value)
                .with_context(|| format!("parsing {}", path.display()))?;
            Ok(Sessions { slots: vec![one] })
        }
    }

    /// Write the whole file, atomically (module docs).
    pub fn save(&self, vault_root: &Path) -> Result<()> {
        let path = session_path(vault_root);
        let text = serde_json::to_string_pretty(self).context("rendering the session file")?;
        atomic::write_file(&path, &text)
    }
}

impl Session {
    /// Read the session for `vault_root`. A vault with no session file is the
    /// normal case, not an error: it answers [`Session::default`] — nothing to
    /// restore. A file that exists but does not parse is an error, so the
    /// caller can say so in the log rather than silently forgetting a session.
    ///
    /// This is slot 0 — the first window's memory — which is also what a
    /// single-window vault means.
    pub fn load(vault_root: &Path) -> Result<Session> {
        Ok(Sessions::load(vault_root)?
            .slots
            .into_iter()
            .next()
            .unwrap_or_default())
    }

    /// Write the session for `vault_root` as slot 0, atomically (module docs).
    /// Any other slots the file holds are left alone.
    pub fn save(&self, vault_root: &Path) -> Result<()> {
        let mut all = Sessions::load(vault_root).unwrap_or_default();
        if all.slots.is_empty() {
            all.slots.push(self.clone());
        } else {
            all.slots[0] = self.clone();
        }
        all.save(vault_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_round_trips_through_the_state_directory() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        let session = Session {
            window: None,
            open_note: Some(PathBuf::from("notes/idea.md")),
            cursor: Some(42),
            scroll_y: Some(-640.0),
            expanded: vec![PathBuf::from("notes")],
            view: Some("preview".into()),
            focus: Some(true),
            focus_scope: Some("section".into()),
            typewriter: Some(true),
            tree_visible: Some(false),
            jumps_back: vec![Jump {
                path: Some(PathBuf::from("Welcome.md")),
                cursor: 7,
            }],
            jumps_forward: vec![],
            tabs: vec![
                Tab {
                    path: Some(PathBuf::from("Welcome.md")),
                    view: "live".into(),
                    graph: false,
                    camera: None,
                },
                Tab {
                    path: Some(PathBuf::from("notes/idea.md")),
                    view: "raw".into(),
                    graph: false,
                    camera: None,
                },
            ],
            active_tab: Some(1),
            graph_filter: Some(GraphFilterSeat {
                orphans: false,
                existing_only: true,
                tags: true,
                search: Some("idea".into()),
            }),
        };
        session.save(dir.path())?;
        assert_eq!(
            session_path(dir.path()),
            dir.path().join(".shale").join("session.json"),
            "beside the config and the DB, not in the vault's root"
        );
        assert_eq!(Session::load(dir.path())?, session);
        Ok(())
    }

    #[test]
    fn a_vault_with_no_session_restores_nothing() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Session::load(dir.path())?, Session::default());
        Ok(())
    }

    /// The incremental rule: a file that names only some keys still restores
    /// the keys it names, and a Shale that knows fewer fields ignores the rest.
    #[test]
    fn a_partial_session_is_a_partial_restore() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".shale"))?;
        std::fs::write(
            session_path(dir.path()),
            r#"{ "open_note": "a.md", "cursor": 3, "brand_new_key": 7 }"#,
        )?;
        let session = Session::load(dir.path())?;
        assert_eq!(session.open_note.as_deref(), Some(Path::new("a.md")));
        assert_eq!(session.cursor, Some(3));
        assert_eq!(session.view, None);
        // A file written before the tree toggle restores a window with its tree
        // showing, which is what `None` means on this field.
        assert_eq!(session.tree_visible, None);
        Ok(())
    }

    /// A file that exists but is not JSON is an error, not a silent fresh
    /// start: the caller logs that the session was forgotten.
    #[test]
    fn a_corrupt_session_says_so() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".shale"))?;
        std::fs::write(session_path(dir.path()), "{ not json")?;
        let err = Session::load(dir.path()).unwrap_err();
        assert!(err.to_string().contains("parsing"), "{err}");
        Ok(())
    }

    /// The slots are separate memories: writing one leaves the others standing,
    /// which is what lets two windows on one vault each remember its own room.
    #[test]
    fn slots_do_not_overwrite_each_other() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        let one = Session {
            open_note: Some(PathBuf::from("one.md")),
            window: Some(WindowGeom {
                x: 100.0,
                y: 50.0,
                w: 1200.0,
                h: 800.0,
                maximized: false,
            }),
            ..Session::default()
        };
        let two = Session {
            open_note: Some(PathBuf::from("two.md")),
            ..Session::default()
        };
        let mut sessions = Sessions {
            slots: vec![one.clone(), two.clone()],
        };
        sessions.save(dir.path())?;
        let back = Sessions::load(dir.path())?;
        assert_eq!(back.slots, vec![one, two]);
        Ok(())
    }

    /// The file a single-window Shale left behind — a bare `Session`, no
    /// `slots` key — reads as slot 0, so the first window of the multi-window
    /// era wakes up exactly where the last window of the old era stood.
    #[test]
    fn a_pre_slots_session_becomes_slot_zero() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".shale"))?;
        std::fs::write(
            session_path(dir.path()),
            r#"{ "open_note": "old.md", "cursor": 9 }"#,
        )?;
        let sessions = Sessions::load(dir.path())?;
        assert_eq!(sessions.slots.len(), 1);
        assert_eq!(sessions.slots[0].open_note.as_deref(), Some(Path::new("old.md")));
        assert_eq!(sessions.slots[0].cursor, Some(9));
        Ok(())
    }
}
