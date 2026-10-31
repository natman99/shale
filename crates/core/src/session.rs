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

/// One stop on the jump chain: a note and where the cursor was left in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Jump {
    /// Vault-relative path of the note.
    pub path: PathBuf,
    /// The cursor there, as a byte offset into the note.
    pub cursor: usize,
}

/// What the next window should look like.
///
/// Deliberately *not* saved: anything the user edits by hand (that is the
/// config's job), and anything derived (the index, the checker's notes). The
/// tab and pane layout will extend this file when P3.1/P3.2 land — same file,
/// more fields, same defaults.
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
    /// The jump chain: the notes behind the user (oldest first) and the ones
    /// ahead, the way `ctrl-o`/`ctrl-i` walk them.
    pub jumps_back: Vec<Jump>,
    /// The forward half of the chain.
    pub jumps_forward: Vec<Jump>,
}

impl Session {
    /// Read the session for `vault_root`. A vault with no session file is the
    /// normal case, not an error: it answers [`Session::default`] — nothing to
    /// restore. A file that exists but does not parse is an error, so the
    /// caller can say so in the log rather than silently forgetting a session.
    pub fn load(vault_root: &Path) -> Result<Session> {
        let path = session_path(vault_root);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Session::default());
            }
            Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
        };
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    /// Write the session for `vault_root`, atomically (module docs).
    pub fn save(&self, vault_root: &Path) -> Result<()> {
        let path = session_path(vault_root);
        let text = serde_json::to_string_pretty(self).context("rendering the session file")?;
        atomic::write_file(&path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_round_trips_through_the_state_directory() -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        let session = Session {
            open_note: Some(PathBuf::from("notes/idea.md")),
            cursor: Some(42),
            scroll_y: Some(-640.0),
            expanded: vec![PathBuf::from("notes")],
            view: Some("preview".into()),
            focus: Some(true),
            focus_scope: Some("section".into()),
            typewriter: Some(true),
            jumps_back: vec![Jump {
                path: PathBuf::from("Welcome.md"),
                cursor: 7,
            }],
            jumps_forward: vec![],
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
}
