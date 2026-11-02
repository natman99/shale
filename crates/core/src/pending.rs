//! The queue an agent's edits land in when its profile asks for review
//! (roadmap P2.5a).
//!
//! **The files are the source of truth.** The vault index may mirror this queue so
//! the app can ask the DB about it, but a corrupt or missing DB never loses a
//! proposed edit: everything the review strip shows is read back out of these files.
//!
//! One file per MCP write call, in `<vault>/.shale/pending-edits/`, named
//! `<epoch>-<profile>-<tool>-<random>.toml`. One file per call because a `kill -9`
//! between two edits must not be able to leave a half-written multi-edit file, and
//! because a queue the user can `ls` and read in any editor is easier to trust than
//! one inside a database. The random tail is there because two writes in the same
//! second by the same profile would otherwise collide and one would overwrite the
//! other.
//!
//! The body is TOML with one `[[hunk]]` table per hunk, each carrying the
//! unified-diff text from [`crate::diff`]. So the file is readable, git-diffable, and
//! — importantly — it says what it wants to change without containing the whole note,
//! which is what lets it stay valid when the user edits the note in the meantime.
//!
//! The MCP server writes these (see `crates/mcp/src/pending_tools.rs`), which is what
//! buys two behaviours for free: edits queue while Shale is closed, and they survive
//! `kill -TERM` because they were never in the app's memory.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::config::Config;
use crate::diff::{self, Hunk};

/// The folder under `.shale` that holds the queue.
pub const PENDING_DIR: &str = "pending-edits";

/// One proposed edit: one MCP write call, as it sits on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingEdit {
    /// The file's stem. The strip, the MCP tools and the retractor all name an edit
    /// by this, so it is the one identifier in the feature.
    #[serde(default)]
    pub id: String,
    /// The note this proposes to change, vault-relative — the same form [`crate::Vault`]
    /// hands out.
    pub note: PathBuf,
    /// The profile that proposed it, which is who is allowed to retract it.
    pub profile: String,
    /// The tool call that made it, for the strip's line and for an agent reading its
    /// own queue back.
    pub tool: String,
    /// When it was proposed, in unix epoch seconds — the same stamp the index and the
    /// session use.
    pub created_at: i64,
    /// The hunks, in the order they appear in the note. TOML spells this `[[hunk]]`.
    #[serde(rename = "hunk", default)]
    pub hunks: Vec<Hunk>,
}

/// What reading the queue back found, including what it could not read.
#[derive(Debug, Default)]
pub struct Queue {
    /// The edits that parsed, oldest first.
    pub edits: Vec<PendingEdit>,
    /// Files that did not parse, so a caller can say so rather than pretend the
    /// queue is empty.
    pub unreadable: Vec<PathBuf>,
}

impl PendingEdit {
    /// A fresh id for a write call: the time it was proposed, who proposed it, what
    /// they called, and a tail so two calls in the same second stay two files. The
    /// tail is the same random hex the MCP lockfile uses.
    pub fn new_id(profile: &str, tool: &str, now: i64) -> String {
        let tail = crate::live::token()[..8].to_string();
        format!("{now}-{}-{}-{tail}", sanitize(profile), sanitize(tool))
    }

    /// The file this edit lives in.
    pub fn file_name(&self) -> String {
        format!("{}.toml", self.id)
    }
}

/// Where the queue lives.
pub fn dir(vault_root: &Path) -> PathBuf {
    Config::shale_dir(vault_root).join(PENDING_DIR)
}

/// The path an id names, refusing anything that is not a plain id.
///
/// An id reaches here from an MCP client, so it is treated like a path a user typed
/// into the file tree: no separators, no `..`, no escaping the queue.
pub fn path_for(vault_root: &Path, id: &str) -> Result<PathBuf> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains('.') {
        // No separators, and no dots at all: an id is a file stem, and the dots are
        // what `..` and an appended `.toml` would be made of.
        anyhow::bail!("not a pending-edit id: {id:?}");
    }
    Ok(dir(vault_root).join(format!("{id}.toml")))
}

/// Write one proposed edit, atomically, so a kill between the write and the rename
/// cannot leave half a file.
pub fn write(vault_root: &Path, edit: &PendingEdit) -> Result<PathBuf> {
    let folder = dir(vault_root);
    std::fs::create_dir_all(&folder).context("creating the pending-edit queue")?;
    let path = folder.join(edit.file_name());
    let text = toml::to_string_pretty(edit).context("rendering the pending edit")?;
    atomic::write_file(&path, &text)
        .with_context(|| format!("writing the pending edit {}", path.display()))?;
    Ok(path)
}

/// Read the whole queue back. A file that does not parse is reported rather than
/// dropped silently: an edit the user cannot see is worse than an ugly message.
pub fn list(vault_root: &Path) -> Queue {
    let mut queue = Queue::default();
    let Ok(entries) = std::fs::read_dir(dir(vault_root)) else {
        // No queue directory at all: nobody has proposed anything yet.
        return queue;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        match read(&path) {
            Ok(edit) => queue.edits.push(edit),
            Err(_) => queue.unreadable.push(path),
        }
    }
    queue.edits.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    queue
}

/// One queue file.
pub fn read(path: &Path) -> Result<PendingEdit> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut edit: PendingEdit =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if edit.id.is_empty() {
        // The id is the file's stem, so a file that does not carry one still has a
        // name the strip can use.
        edit.id = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    Ok(edit)
}

/// Take an edit out of the queue: accepted, rejected, or retracted by the agent that
/// proposed it.
pub fn remove(vault_root: &Path, id: &str) -> Result<()> {
    let path = path_for(vault_root, id)?;
    std::fs::remove_file(&path)
        .with_context(|| format!("removing the pending edit {}", path.display()))
}

/// Which of an edit's hunks no longer fit the note as it stands.
///
/// This is the apply-or-stale policy being *asked* rather than *applied*: the strip
/// badges a hunk stale when its context no longer matches, and accepting it then does
/// nothing but say so. A note that is not on disk counts as empty, which is what a
/// proposed new note expects.
pub fn stale_hunks(edit: &PendingEdit, current: &str) -> Vec<usize> {
    edit.hunks
        .iter()
        .enumerate()
        .filter(|(_, hunk)| matches!(diff::apply(current, hunk), diff::Outcome::Stale))
        .map(|(index, _)| index)
        .collect()
}

/// The edit with one of its hunks taken out: every hunk below it is shifted by the
/// lines that hunk would have changed, so the positions the file records still name
/// the same places in the note.
///
/// The strip works one hunk at a time — accept this one, reject that one — and the
/// file *is* the queue, so the file has to follow. Leaving the hunk in it would offer
/// the same change twice; leaving the hunks below it where they were would make them
/// stale for no reason, because the shift is exact rather than a guess about where the
/// agent meant. `None` when no hunks are left, which is when the queue file goes away.
///
/// `hunk` out of range is `None` as well: there is nothing left to take out, and the
/// caller's next move is the same (take the file out of the queue).
pub fn without_hunk(edit: &PendingEdit, hunk: usize) -> Option<PendingEdit> {
    let mut edit = edit.clone();
    if hunk >= edit.hunks.len() {
        return None;
    }
    let taken = edit.hunks.remove(hunk);
    let delta = taken.new_len as isize - taken.old_len as isize;
    for later in &mut edit.hunks {
        // Only the hunks that come after the accepted one move. Within one edit they
        // are ordered and disjoint, but the position is checked rather than assumed,
        // because a hand-edited queue file is a thing that happens.
        if later.old_start >= taken.old_start + taken.old_len {
            later.shift(delta);
        }
    }
    (!edit.hunks.is_empty()).then_some(edit)
}

/// Profile and tool names become part of a file name, so they are reduced to the
/// characters that can safely be in one.
fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{apply, hunks, Outcome};

    const NOTE: &str = "one\ntwo\nthree\nfour\nfive\n";

    /// An edit carrying a new-note diff; the tests that care about a particular diff
    /// fill `hunks` in themselves.
    fn edit(note: &str, proposed: &str, profile: &str) -> PendingEdit {
        PendingEdit {
            id: PendingEdit::new_id(profile, "write_note", 1_700_000_000),
            note: PathBuf::from(note),
            profile: profile.to_string(),
            tool: "write_note".to_string(),
            created_at: 1_700_000_000,
            hunks: hunks("", proposed),
        }
    }

    #[test]
    fn a_queued_edit_round_trips_through_the_file() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let proposed = "one\ntwo\nTHREE\nfour\nfive\n";
        let mut edit = edit("idea.md", proposed, "writing-help");
        edit.hunks = hunks(NOTE, proposed);
        let path = write(dir.path(), &edit)?;

        assert_eq!(read(&path)?, edit, "the file is the edit");
        let text = std::fs::read_to_string(&path)?;
        assert!(text.contains("[[hunk]]"), "spelled the way the plan ruled");
        assert!(
            text.contains("-three") && text.contains("+THREE"),
            "the diff text is readable in the file:\n{text}"
        );
        Ok(())
    }

    /// Two write calls in the same second by the same profile are two edits, not one
    /// file overwriting another.
    #[test]
    fn two_calls_in_the_same_second_are_two_files() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let first = edit("a.md", "x\n", "writing-help");
        let mut second = edit("b.md", "y\n", "writing-help");
        second.id = PendingEdit::new_id("writing-help", "write_note", 1_700_000_000);
        assert_ne!(first.id, second.id);
        write(dir.path(), &first)?;
        write(dir.path(), &second)?;
        assert_eq!(list(dir.path()).edits.len(), 2);
        Ok(())
    }

    #[test]
    fn the_queue_reads_back_oldest_first() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut old = edit("old.md", "x\n", "writing-help");
        old.id = "1000-a".into();
        old.created_at = 1000;
        let mut new = edit("new.md", "x\n", "writing-help");
        new.id = "2000-b".into();
        new.created_at = 2000;
        write(dir.path(), &new)?;
        write(dir.path(), &old)?;
        let notes: Vec<PathBuf> = list(dir.path())
            .edits
            .iter()
            .map(|e| e.note.clone())
            .collect();
        assert_eq!(
            notes,
            vec![PathBuf::from("old.md"), PathBuf::from("new.md")]
        );
        Ok(())
    }

    /// A file the app cannot read is a message, not a silently empty queue.
    #[test]
    fn a_file_that_is_not_an_edit_is_reported() -> Result<()> {
        let vault = tempfile::tempdir()?;
        write(vault.path(), &edit("a.md", "x\n", "p"))?;
        // The queue lives under `.shale`, which is where the app looks.
        std::fs::write(dir(vault.path()).join("broken.toml"), "not an edit =")?;
        let queue = list(vault.path());
        assert_eq!(queue.edits.len(), 1);
        assert_eq!(queue.unreadable.len(), 1);
        Ok(())
    }

    #[test]
    fn an_empty_vault_has_an_empty_queue() {
        let dir = tempfile::tempdir().unwrap();
        let queue = list(dir.path());
        assert!(queue.edits.is_empty());
        assert!(queue.unreadable.is_empty());
    }

    #[test]
    fn removing_an_edit_takes_its_file() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let edit = edit("a.md", "x\n", "writing-help");
        let path = write(dir.path(), &edit)?;
        assert!(path.is_file());
        remove(dir.path(), &edit.id)?;
        assert!(!path.exists());
        assert!(list(dir.path()).edits.is_empty());
        Ok(())
    }

    /// The id comes from an MCP client, so it gets the same confinement a vault path
    /// gets: it cannot name something outside the queue.
    #[test]
    fn an_id_cannot_escape_the_queue() {
        let dir = tempfile::tempdir().unwrap();
        for hostile in [
            "../config",
            "..",
            "a/b",
            "a\\b",
            "",
            ".shale/config",
            "x.toml",
        ] {
            assert!(
                path_for(dir.path(), hostile).is_err(),
                "{hostile:?} must not name a queue file"
            );
        }
    }

    /// The strip's stale badge: the hunk whose context the user has since changed.
    #[test]
    fn a_hunk_the_user_walked_over_is_stale() -> Result<()> {
        let proposed = "one\ntwo\nTHREE\nfour\nfive\n";
        let mut edit = edit("idea.md", proposed, "p");
        edit.hunks = hunks(NOTE, proposed);
        assert!(
            stale_hunks(&edit, NOTE).is_empty(),
            "fresh, so nothing is stale"
        );

        let user_edited = "one\ntwo\nthree, mine\nfour\nfive\n";
        assert_eq!(stale_hunks(&edit, user_edited), vec![0]);

        // A note that went away entirely: every hunk is stale, and that is reported
        // rather than crashing.
        assert_eq!(stale_hunks(&edit, "").len(), edit.hunks.len());
        Ok(())
    }

    #[test]
    fn an_id_is_a_file_name() {
        let id = PendingEdit::new_id("writing-help", "write_note", 1_700_000_000);
        assert!(id.starts_with("1700000000-writing-help-write_note-"));
        assert!(!id.contains('/'));
        // A profile named with something awkward cannot break the file name.
        let id = PendingEdit::new_id("../evil name", "write/note", 1);
        assert!(!id.contains('/') && !id.contains(".shale"), "{id}");
    }

    /// The strip accepts one hunk at a time, so the file has to keep up: the accepted
    /// hunk gone, the ones below it moved by exactly the lines the accepted one
    /// changed, and the file gone when nothing is left.
    #[test]
    fn dropping_a_hunk_re_anchors_the_rest() -> Result<()> {
        // Far enough apart that the two changes are two hunks rather than one.
        let long: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        let proposed = long
            .replace("line 2\n", "TWO\nTWO-2\n")
            .replace("line 18\n", "SEVEN\n");
        let mut edit = edit("idea.md", &proposed, "p");
        edit.hunks = hunks(&long, &proposed);
        assert_eq!(edit.hunks.len(), 2);
        let second_at = edit.hunks[1].old_start;

        let after = without_hunk(&edit, 0).expect("one hunk is still pending");
        assert_eq!(after.hunks.len(), 1);
        assert_eq!(after.id, edit.id, "the same edit, not a new one");
        assert_eq!(
            after.hunks[0].old_start,
            second_at + 1,
            "the accepted hunk added a line, so the one below it moved down one"
        );
        // The re-anchored hunk still fits the half-edited note, which is the whole
        // point: it did not go stale just because something above it changed.
        let half = apply(&long, &edit.hunks[0]);
        let Outcome::Applied(half) = half else {
            panic!("the first hunk is fresh")
        };
        assert!(
            stale_hunks(&after, &half).is_empty(),
            "the second hunk is still applicable"
        );

        // Accepting the last hunk leaves no edit, which is when the file goes.
        assert!(without_hunk(&after, 0).is_none());
        assert!(without_hunk(&edit, 9).is_none(), "no such hunk");
        Ok(())
    }

    /// A hand-edited file can carry hunks in any order; only the ones below the
    /// accepted hunk move.
    #[test]
    fn only_the_hunks_below_move() {
        let mut edit = edit("idea.md", "", "p");
        edit.hunks = vec![
            Hunk {
                old_start: 20,
                old_len: 1,
                new_start: 20,
                new_len: 4,
                final_newline: true,
                text: "@@ -20,1 +20,4 @@\n-old\n+new\n+more\n+lines".to_string(),
            },
            Hunk {
                old_start: 5,
                old_len: 1,
                new_start: 5,
                new_len: 1,
                final_newline: true,
                text: "@@ -5,1 +5,1 @@\n-five\n+FIVE".to_string(),
            },
        ];
        let after = without_hunk(&edit, 0).unwrap();
        assert_eq!(after.hunks[0].old_start, 5, "the hunk above stays put");
    }
}
