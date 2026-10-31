//! Keeping the index in step with the notes (roadmap P1.3, P1.4).
//!
//! Two paths feed the same rows. At startup, [`Db::reconcile`] compares what the
//! index already claims about each note against what the disk says now and fixes the
//! difference; that is the pass that notices edits another program made while Shale
//! was closed, and it ends when the difference is closed. After that the index is
//! event-driven: the vault watcher hands [`Db::sync`] the paths that changed, and
//! Shale's own save, create, rename and delete paths report what they just did.
//!
//! ## What counts as an event
//!
//! `events` is the log the timeline replays, so it holds the moments someone
//! scrubbing the vault would recognise: a note created, deleted, moved, or written
//! by something other than the user's own typing. A plain user save writes no row.
//! History of what the user typed is git's job (PROJECT.md, "What the DB is *not*
//! for"), and autosave fires every two seconds of quiet, so recording it would bury
//! the log in non-events.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use rusqlite::{params, Connection};

use crate::graph::extract_tags;
use crate::markdown::extract_wikilinks;
use crate::vault::{Vault, VaultEntry};
use crate::watch::{Stamp, VaultChanges};

use super::{now_epoch, Db, Provenance};

/// The `links.target_kind` vocabulary: what a wiki-link's target turned out to be.
pub mod target_kind {
    /// A note in the vault. The `target` column holds its vault-relative path.
    pub const NOTE: &str = "note";
    /// An image or other file that is not a note. Also a path.
    pub const ATTACHMENT: &str = "attachment";
    /// A target that resolves to nothing. The `target` column holds what the author
    /// wrote, which is exactly what an unresolved-link list needs.
    pub const GHOST: &str = "ghost";
}

/// How many words a note has.
///
/// A word is a whitespace-delimited run holding at least one letter or digit, which
/// keeps the `#` of a heading, the `-` of a bullet and the `**` around bold text out
/// of the count while leaving `[[A note]]` as two words. Deliberately simple: the
/// number is shown to a person and compared against itself over time, not fed to
/// anything that needs a linguistic definition.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|token| token.chars().any(|c| c.is_alphanumeric()))
        .count()
}

/// What an `events` row is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// A note appeared.
    Create,
    /// A note's rows were dropped because its file is gone.
    Delete,
    /// A note moved. One row rather than a delete plus a create, so the scrubber can
    /// show the move as the single act it was.
    Rename,
    /// Something other than the user's own typing wrote the note.
    Modify,
}

impl EventKind {
    /// The word stored in the `kind` column and read back by the scrubber.
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Create => "create",
            EventKind::Delete => "delete",
            EventKind::Rename => "rename",
            EventKind::Modify => "modify",
        }
    }
}

/// What one indexing pass did.
#[derive(Debug, Default, Clone)]
pub struct SyncReport {
    /// Notes whose rows were written.
    pub indexed: usize,
    /// Notes whose rows were dropped.
    pub removed: usize,
    /// Notes that could not be read or indexed, with why. A pass carries on past
    /// these: one unreadable note must not cost the whole vault its index.
    pub failed: Vec<(PathBuf, String)>,
}

impl SyncReport {
    /// Whether the pass changed nothing, which is the normal case: the watcher calls
    /// [`Db::sync`] on every tick, and almost every tick has nothing to report.
    pub fn is_empty(&self) -> bool {
        self.indexed == 0 && self.removed == 0 && self.failed.is_empty()
    }
}

/// What one note contributes to the index.
///
/// Worked out before the write transaction opens, on purpose: the writer thread is
/// the vault's only one, so reading and parsing happen outside it, and a note that
/// cannot be read costs its own row rather than taking the rest of the pass down
/// with it.
struct Record {
    /// Vault-relative path, as the `notes.path` column holds it.
    path: String,
    /// Bytes on disk, so a change that lands inside the second the note was last
    /// indexed is still seen as a change.
    size: i64,
    /// Modification time in epoch seconds.
    modified_at: i64,
    /// Words, by [`word_count`].
    words: i64,
    /// Tag names without the `#`, deduped, in the order they appear.
    tags: Vec<String>,
    /// Link targets, deduped, each paired with its [`target_kind`].
    links: Vec<(String, &'static str)>,
}

/// Read one note and work out its rows.
///
/// `notes` and `attachments` are the vault's listings, which link resolution needs;
/// a pass takes them once and shares them, because each is a directory walk.
fn record(
    vault: &Vault,
    rel: &Path,
    notes: &[VaultEntry],
    attachments: &[VaultEntry],
) -> Result<Record> {
    let abs = vault.resolve(rel)?;
    let text = fs::read_to_string(&abs).with_context(|| format!("indexing {}", abs.display()))?;
    let meta = fs::metadata(&abs).with_context(|| format!("indexing {}", abs.display()))?;
    let modified_at = meta
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut links: Vec<(String, &'static str)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for link in extract_wikilinks(&text) {
        let target = link.target.trim();
        if target.is_empty() {
            continue;
        }
        // The same resolution order the graph and the backlinks use: a note, then an
        // attachment, then a ghost keyed by what the author wrote.
        let (stored, kind) = match vault.resolve_link(target, notes) {
            Ok(Some(entry)) => (entry.path.to_string_lossy().into_owned(), target_kind::NOTE),
            // A target that names a real note but fails to resolve (a path that
            // escapes the vault, say) is treated as unresolved rather than dropped,
            // so the author still sees it in an unresolved-link list.
            _ => match vault.resolve_attachment(target, attachments) {
                Some(entry) => (
                    entry.path.to_string_lossy().into_owned(),
                    target_kind::ATTACHMENT,
                ),
                None => (target.to_string(), target_kind::GHOST),
            },
        };
        // One row per (source, target, kind): a note that links to the same note
        // three times has one link. The separator keeps `a|b` and `a\u{0}b` apart.
        if seen.insert(format!("{stored}\u{0}{kind}")) {
            links.push((stored, kind));
        }
    }

    Ok(Record {
        path: rel.to_string_lossy().into_owned(),
        size: meta.len() as i64,
        modified_at,
        words: word_count(&text) as i64,
        tags: extract_tags(&text),
        links,
    })
}

/// Write one note's rows, replacing whatever was there for it.
///
/// `created_at` is stamped only when the note is new to the index. The filesystem's
/// birth time is not something [`Stamp`] reads, so the column means "when the index
/// first saw this note", and re-indexing must not move it. `edit_count` and
/// `open_count` are left to the phases that decide what an edit and an open are.
fn write_record(conn: &Connection, note: &Record, provenance: Provenance) -> Result<()> {
    conn.execute(
        "INSERT INTO notes (path, created_at, modified_at, size, word_count, last_modified_by)
         VALUES (?1, ?2, ?2, ?3, ?4, ?5)
         ON CONFLICT(path) DO UPDATE SET
             modified_at = excluded.modified_at,
             size = excluded.size,
             word_count = excluded.word_count,
             last_modified_by = excluded.last_modified_by",
        params![
            note.path,
            note.modified_at,
            note.size,
            note.words,
            provenance.as_str()
        ],
    )
    .with_context(|| format!("indexing {}", note.path))?;

    conn.execute("DELETE FROM tags WHERE note = ?1", params![note.path])
        .with_context(|| format!("indexing {}", note.path))?;
    for tag in &note.tags {
        conn.execute(
            "INSERT OR IGNORE INTO tags (note, tag) VALUES (?1, ?2)",
            params![note.path, tag],
        )
        .with_context(|| format!("indexing {}", note.path))?;
    }

    // Links are replaced rather than upserted, but `first_seen_at` survives the
    // replacement: it is what link-age colouring reads, and a note being re-indexed
    // says nothing about how old its links are. Reading the old rows back is what
    // lets a link that stays keep its age while one that goes really does go.
    let mut previous: BTreeMap<(String, String), i64> = BTreeMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT target, target_kind, first_seen_at FROM links WHERE source = ?1")
            .with_context(|| format!("indexing {}", note.path))?;
        let rows = stmt
            .query_map(params![note.path], |row| {
                Ok((
                    (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                    row.get::<_, i64>(2)?,
                ))
            })
            .with_context(|| format!("indexing {}", note.path))?;
        for row in rows {
            let ((target, kind), first) = row.with_context(|| format!("indexing {}", note.path))?;
            previous.insert((target, kind), first);
        }
    }
    conn.execute("DELETE FROM links WHERE source = ?1", params![note.path])
        .with_context(|| format!("indexing {}", note.path))?;
    for (target, kind) in &note.links {
        let first_seen = previous
            .remove(&(target.to_string(), kind.to_string()))
            .unwrap_or(note.modified_at);
        conn.execute(
            "INSERT INTO links (source, target, target_kind, first_seen_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![note.path, target, kind, first_seen, note.modified_at],
        )
        .with_context(|| format!("indexing {}", note.path))?;
    }
    Ok(())
}

/// Drop everything the index claims about one note.
fn drop_note(conn: &Connection, path: &str) -> Result<()> {
    conn.execute("DELETE FROM notes WHERE path = ?1", params![path])?;
    conn.execute("DELETE FROM tags WHERE note = ?1", params![path])?;
    conn.execute("DELETE FROM links WHERE source = ?1", params![path])?;
    // A note that is gone is not a link target either. The notes that pointed at it
    // still contain the text, so their links come back as ghosts the next time each
    // of those notes is indexed.
    conn.execute("DELETE FROM links WHERE target = ?1", params![path])?;
    Ok(())
}

/// Append one row to the log the scrubber replays.
fn event(
    conn: &Connection,
    note: &str,
    kind: EventKind,
    provenance: Provenance,
    detail: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO events (note, kind, at, provenance, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            note,
            kind.as_str(),
            now_epoch(),
            provenance.as_str(),
            detail
        ],
    )
    .with_context(|| format!("recording a {} event for {note}", kind.as_str()))?;
    Ok(())
}

/// A `LIKE` pattern matching everything under `prefix`, with the wildcards escaped so
/// a folder named `100% done` matches itself and nothing else.
fn like_prefix(prefix: &str) -> String {
    let mut out = String::new();
    for c in prefix.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

impl Db {
    /// Bring the index up to date with the whole vault, then stop.
    ///
    /// This is the startup pass roadmap P1.3 asks for. The index is turned back into
    /// a signature — the same shape [`Vault::signature`] returns — and diffed
    /// against the disk, so the notes that need re-reading are exactly the ones that
    /// changed while Shale was not running. It writes no `events` rows: a first pass
    /// over a vault is not a thousand creations.
    pub fn reconcile(&self, vault: &Vault) -> Result<SyncReport> {
        let stored = self.stored_signature()?;
        let current: BTreeMap<PathBuf, Stamp> = vault
            .signature()?
            .into_iter()
            .map(|(path, stamp)| (path, whole_seconds(stamp)))
            .collect();
        let changes = VaultChanges::between(&stored, &current);
        if changes.is_empty() {
            return Ok(SyncReport::default());
        }
        self.sync_with(vault, &changes, Provenance::External, false)
    }

    /// Fold one set of disk changes into the index, recording the notable ones as
    /// events.
    pub fn sync(
        &self,
        vault: &Vault,
        changes: &VaultChanges,
        provenance: Provenance,
    ) -> Result<SyncReport> {
        self.sync_with(vault, changes, provenance, true)
    }

    /// Re-index one note Shale just wrote. No event: the user's own save is not
    /// timeline news.
    pub fn note_saved(&self, vault: &Vault, rel: &Path, provenance: Provenance) -> Result<()> {
        let changes = VaultChanges {
            added: Vec::new(),
            removed: Vec::new(),
            modified: vec![rel.to_path_buf()],
        };
        self.sync_with(vault, &changes, provenance, true)?;
        Ok(())
    }

    /// Re-index several notes Shale just rewrote — the notes a rename edited to
    /// point at the new name. Shale's own writes are re-stamped out of the watcher's
    /// view, so nothing else would notice them.
    pub fn notes_saved(
        &self,
        vault: &Vault,
        paths: &[PathBuf],
        provenance: Provenance,
    ) -> Result<SyncReport> {
        let changes = VaultChanges {
            added: Vec::new(),
            removed: Vec::new(),
            modified: paths.to_vec(),
        };
        self.sync_with(vault, &changes, provenance, true)
    }

    /// Index a note that just appeared, and record that it did.
    pub fn note_created(&self, vault: &Vault, rel: &Path, provenance: Provenance) -> Result<()> {
        let changes = VaultChanges {
            added: vec![rel.to_path_buf()],
            removed: Vec::new(),
            modified: Vec::new(),
        };
        self.sync_with(vault, &changes, provenance, true)?;
        Ok(())
    }

    /// Drop a note that just went away, and record that it did.
    pub fn note_deleted(&self, rel: &Path, provenance: Provenance) -> Result<()> {
        let path = rel.to_string_lossy().into_owned();
        self.write(move |conn| {
            drop_note(conn, &path)?;
            event(conn, &path, EventKind::Delete, provenance, "")?;
            Ok(())
        })
    }

    /// Move a note's rows from one path to another and record the move as one event.
    ///
    /// The note at `to` is read again rather than the row being renamed, so the new
    /// path is indexed the same way a created note would be even if the rename also
    /// edited the title or the links inside.
    pub fn note_renamed(&self, vault: &Vault, from: &Path, to: &Path) -> Result<()> {
        let notes = vault.list_markdown()?;
        let attachments = vault.list_attachments()?;
        let note = record(vault, to, &notes, &attachments)?;
        let old = from.to_string_lossy().into_owned();
        self.write(move |conn| {
            drop_note(conn, &old)?;
            write_record(conn, &note, Provenance::Rename)?;
            event(
                conn,
                &note.path,
                EventKind::Rename,
                Provenance::Rename,
                &old,
            )?;
            Ok(())
        })
    }

    /// Words and notes under `folder`, which is vault-relative and empty for the
    /// whole vault.
    ///
    /// The number is only as complete as the index: a note that has never been
    /// indexed, or whose indexing failed, is not in it, so a caller presenting this
    /// should present it as the index's count rather than a divine one.
    pub fn word_count_total(&self, folder: &str) -> Result<(i64, i64)> {
        let mut folder = folder.replace('\\', "/");
        if folder.ends_with('/') {
            folder.pop();
        }
        if !folder.is_empty() {
            folder.push('/');
        }
        let conn = self.read()?;
        let (words, notes): (i64, i64) = conn
            .query_row(
                "SELECT COALESCE(SUM(word_count), 0), COUNT(*) FROM notes \
                 WHERE path LIKE ?1 ESCAPE '\\'",
                params![like_prefix(&folder)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .context("counting words in the vault index")?;
        Ok((words, notes))
    }

    /// The shared body of every incremental pass.
    ///
    /// `events` is false only for [`Db::reconcile`], where "this note is new to the
    /// index" is not the same statement as "this note was created".
    fn sync_with(
        &self,
        vault: &Vault,
        changes: &VaultChanges,
        provenance: Provenance,
        events: bool,
    ) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        if changes.is_empty() {
            return Ok(report);
        }
        // One walk each, shared by every note in the pass.
        let notes = vault.list_markdown()?;
        let attachments = vault.list_attachments()?;

        let mut written: Vec<Record> = Vec::new();
        let mut created: Vec<String> = Vec::new();
        let mut touched: Vec<String> = Vec::new();

        for path in &changes.added {
            if !is_markdown(path) {
                continue;
            }
            match record(vault, path, &notes, &attachments) {
                Ok(note) => {
                    if events {
                        created.push(note.path.clone());
                    }
                    written.push(note);
                }
                Err(err) => report.failed.push((path.clone(), format!("{err:#}"))),
            }
        }
        for path in &changes.modified {
            if !is_markdown(path) {
                continue;
            }
            match record(vault, path, &notes, &attachments) {
                Ok(note) => {
                    // Only a writer that is not the user typing is worth the log:
                    // see this module's header.
                    if events && provenance != Provenance::User {
                        touched.push(note.path.clone());
                    }
                    written.push(note);
                }
                Err(err) => report.failed.push((path.clone(), format!("{err:#}"))),
            }
        }
        // A path the watcher reports as both changed and gone is a rename it saw as
        // two ticks; the removal wins, so nothing is indexed for a note that is no
        // longer there.
        let gone: Vec<String> = changes
            .removed
            .iter()
            .filter(|path| is_markdown(path))
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        report.removed = gone.len();
        written.retain(|note| !gone.contains(&note.path));
        created.retain(|path| !gone.contains(path));
        touched.retain(|path| !gone.contains(path));

        if written.is_empty() && gone.is_empty() {
            return Ok(report);
        }
        report.indexed = written.len();

        self.write(move |conn| {
            for note in &written {
                write_record(conn, note, provenance)?;
            }
            for path in &gone {
                drop_note(conn, path)?;
            }
            // The log is written by the same transaction as the rows it describes,
            // so a scrubber never sees an event whose rows are missing.
            if events {
                for path in &created {
                    event(conn, path, EventKind::Create, provenance, "")?;
                }
                for path in &touched {
                    event(conn, path, EventKind::Modify, provenance, "")?;
                }
                for path in &gone {
                    event(conn, path, EventKind::Delete, provenance, "")?;
                }
            }
            Ok(())
        })?;
        Ok(report)
    }

    /// The index as a signature, so [`VaultChanges::between`] can diff the disk
    /// against what the index already believes.
    fn stored_signature(&self) -> Result<BTreeMap<PathBuf, Stamp>> {
        let conn = self.read()?;
        let mut stmt = conn
            .prepare("SELECT path, modified_at, size FROM notes")
            .context("reading the vault index's notes")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .context("reading the vault index's notes")?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (path, at, size) = row.context("reading the vault index's notes")?;
            out.insert(
                PathBuf::from(path),
                Stamp::File {
                    len: size.max(0) as u64,
                    mtime: UNIX_EPOCH + Duration::from_secs(at.max(0) as u64),
                },
            );
        }
        Ok(out)
    }
}

/// Whether a path is a note, which is the same test [`Vault::list_markdown`] uses.
///
/// The signature the watcher compares includes every file and directory, so a pass
/// is routinely handed folders and images that have no business in `notes`.
fn is_markdown(path: &Path) -> bool {
    path.extension().map(|e| e == "md").unwrap_or(false)
}

/// A stamp whose mtime is whole seconds, which is all the index can remember.
///
/// [`Db::reconcile`] diffs the index against the disk by equality, and a filesystem
/// hands out sub-second mtimes, so without this every note in the vault would look
/// modified at every startup. What that gives up — an edit that lands inside the
/// second the note was last indexed — is what the row's `size` is there to catch.
fn whole_seconds(stamp: Stamp) -> Stamp {
    match stamp {
        Stamp::Dir => Stamp::Dir,
        Stamp::File { len, mtime } => Stamp::File {
            len,
            mtime: UNIX_EPOCH
                + Duration::from_secs(
                    mtime
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{SCHEMA_V1, SCHEMA_VERSION};
    use std::fs::{File, FileTimes};

    /// A vault whose notes are written by hand, and its index. The `TempDir` is
    /// returned so the caller keeps it alive as long as the DB is open.
    fn vault_with(files: &[(&str, &str)]) -> (tempfile::TempDir, Vault, Db) {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        for (path, text) in files {
            vault.write(path, text).unwrap();
        }
        let db = Db::open(dir.path()).unwrap();
        (dir, vault, db)
    }

    /// One column from every row of a query, sorted, so an assertion is about the
    /// rows the index holds rather than the order SQLite happened to return them.
    fn column(db: &Db, sql: &str) -> Vec<String> {
        let conn = db.read().unwrap();
        let mut stmt = conn.prepare(sql).unwrap();
        let mut out: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        out.sort();
        out
    }

    /// The log as `kind:provenance` words, oldest first.
    fn log(db: &Db) -> Vec<String> {
        let conn = db.read().unwrap();
        let mut stmt = conn
            .prepare("SELECT kind || ':' || provenance FROM events ORDER BY id")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect()
    }

    #[test]
    fn the_first_pass_reads_every_note_once() {
        let (_dir, vault, db) = vault_with(&[
            ("Welcome.md", "# Welcome\n\nA [[Note Two]] and #alpha.\n"),
            ("notes/Note Two.md", "Two words only\n"),
            ("assets/pic.png", "not a note\n"),
        ]);
        let report = db.reconcile(&vault).unwrap();
        assert_eq!(report.indexed, 2, "the folder and the image were skipped");
        assert!(report.failed.is_empty());
        assert_eq!(
            column(&db, "SELECT path FROM notes"),
            vec!["Welcome.md", "notes/Note Two.md"]
        );
        assert_eq!(
            column(
                &db,
                "SELECT CAST(word_count AS TEXT) FROM notes WHERE path = 'notes/Note Two.md'"
            ),
            vec!["3"],
            "the heading marker is not a word but the heading text is"
        );
        assert_eq!(column(&db, "SELECT tag FROM tags"), vec!["alpha"]);
        assert_eq!(
            column(&db, "SELECT target || ':' || target_kind FROM links"),
            vec!["notes/Note Two.md:note"],
            "the link is stored as the note it resolves to"
        );
        assert!(
            db.reconcile(&vault).unwrap().is_empty(),
            "and the second pass has nothing to do"
        );
    }

    /// The reason `notes.size` exists: `modified_at` is only as fine as a second, so
    /// an edit that lands inside the second the note was last indexed is invisible
    /// unless the row also remembers how big the note was.
    #[test]
    fn an_edit_inside_the_same_second_is_still_an_edit() {
        let (dir, vault, db) = vault_with(&[("one.md", "one\n")]);
        assert_eq!(db.reconcile(&vault).unwrap().indexed, 1);

        vault.write("one.md", "one two three four five\n").unwrap();
        // Back to the stamp the index already holds, so the mtime alone reports no
        // change at all.
        let stored = db.stored_signature().unwrap();
        let stamp = match stored.get(Path::new("one.md")).unwrap() {
            Stamp::File { mtime, .. } => *mtime,
            Stamp::Dir => panic!("a note is not a folder"),
        };
        let file = File::options()
            .write(true)
            .open(dir.path().join("one.md"))
            .unwrap();
        file.set_times(FileTimes::new().set_modified(stamp))
            .unwrap();

        let report = db.reconcile(&vault).unwrap();
        assert_eq!(report.indexed, 1, "the size is what noticed");
        assert_eq!(
            column(
                &db,
                "SELECT CAST(word_count AS TEXT) FROM notes WHERE path = 'one.md'"
            ),
            vec!["5"]
        );
    }

    /// A link's age is what link-age colouring reads, so re-indexing a note must
    /// restamp the link's `last_seen_at` and leave its `first_seen_at` alone — while
    /// a link the author deleted really does go.
    #[test]
    fn a_link_keeps_its_age_until_it_is_removed() {
        let (_dir, vault, db) = vault_with(&[("a.md", "see [[b]]\n"), ("b.md", "b\n")]);
        db.reconcile(&vault).unwrap();
        let first = column(&db, "SELECT CAST(first_seen_at AS TEXT) FROM links");

        vault.write("a.md", "still see [[b]] here\n").unwrap();
        db.notes_saved(&vault, &[PathBuf::from("a.md")], Provenance::User)
            .unwrap();
        assert_eq!(
            column(&db, "SELECT CAST(first_seen_at AS TEXT) FROM links"),
            first,
            "the link is the same link it was"
        );

        vault.write("a.md", "no links now\n").unwrap();
        db.notes_saved(&vault, &[PathBuf::from("a.md")], Provenance::User)
            .unwrap();
        assert_eq!(
            column(&db, "SELECT target FROM links"),
            Vec::<String>::new()
        );
    }

    /// What the timeline replays: a creation, a move and a deletion are news; the
    /// user's own save is not, because autosave fires every two seconds of quiet.
    #[test]
    fn the_log_holds_the_moments_someone_would_recognise() {
        let (_dir, vault, db) = vault_with(&[("idea.md", "a thought\n")]);
        db.note_created(&vault, Path::new("idea.md"), Provenance::Template)
            .unwrap();
        assert_eq!(log(&db), vec!["create:template"]);

        vault.write("idea.md", "a thought, edited\n").unwrap();
        db.note_saved(&vault, Path::new("idea.md"), Provenance::User)
            .unwrap();
        assert_eq!(
            log(&db),
            vec!["create:template"],
            "the user's own save is git's history, not the index's"
        );

        db.note_saved(&vault, Path::new("idea.md"), Provenance::Mcp)
            .unwrap();
        assert_eq!(
            log(&db),
            vec!["create:template", "modify:mcp"],
            "but anything else that writes a note is"
        );

        db.note_deleted(Path::new("idea.md"), Provenance::User)
            .unwrap();
        assert_eq!(
            log(&db),
            vec!["create:template", "modify:mcp", "delete:user"]
        );
        assert_eq!(column(&db, "SELECT path FROM notes"), Vec::<String>::new());
    }

    /// A rename is one event and one set of rows, not a delete and a create that
    /// leave the scrubber guessing.
    #[test]
    fn a_rename_moves_the_rows_and_logs_one_event() {
        let (_dir, vault, db) = vault_with(&[("old.md", "a thought\n")]);
        db.note_created(&vault, Path::new("old.md"), Provenance::User)
            .unwrap();
        vault.rename("old.md", "new.md").unwrap();
        db.note_renamed(&vault, Path::new("old.md"), Path::new("new.md"))
            .unwrap();

        assert_eq!(column(&db, "SELECT path FROM notes"), vec!["new.md"]);
        let conn = db.read().unwrap();
        let detail: String = conn
            .query_row(
                "SELECT note || ' <- ' || detail FROM events WHERE kind = 'rename'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(detail, "new.md <- old.md");
        assert_eq!(
            log(&db),
            vec!["create:user", "rename:rename"],
            "and the rename is not reported as a deletion"
        );
    }

    /// A target that names nothing is kept as what the author wrote, which is what
    /// an unresolved-link list needs; an image reached by wiki-link is an attachment.
    #[test]
    fn ghosts_and_attachments_are_links_too() {
        let (_dir, vault, db) = vault_with(&[
            ("a.md", "![[assets/pic.png]] [[nowhere]] [[deep]]\n"),
            ("assets/pic.png", "bytes\n"),
            ("notes/deep.md", "deep\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(&db, "SELECT target || ':' || target_kind FROM links"),
            vec![
                "assets/pic.png:attachment",
                "notes/deep.md:note",
                "nowhere:ghost"
            ],
            "a bare stem resolves to the note it names, an image to an attachment, \
             and a target that resolves nowhere is kept as the author wrote it"
        );
    }

    #[test]
    fn word_counts_scope_to_a_folder() {
        let (_dir, vault, db) = vault_with(&[
            ("a.md", "one two three\n"),
            ("notes/b.md", "four five\n"),
            // A folder whose name starts with the one being asked about, which a
            // plain prefix match would sweep in.
            ("notesx/c.md", "six\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(db.word_count_total("").unwrap(), (6, 3));
        assert_eq!(db.word_count_total("notes").unwrap(), (2, 1));
        assert_eq!(
            db.word_count_total("notes/").unwrap(),
            (2, 1),
            "a trailing slash is the same folder"
        );
        assert_eq!(db.word_count_total("nowhere").unwrap(), (0, 0));
    }

    /// The startup pass turns the rows back into a signature, which is what lets it
    /// reuse the watcher's diff instead of inventing a second one.
    #[test]
    fn the_index_can_be_read_back_as_a_signature() {
        let (dir, vault, db) = vault_with(&[("one.md", "one\n")]);
        db.reconcile(&vault).unwrap();
        let disk = vault.signature().unwrap();
        let stored = db.stored_signature().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            stored.get(Path::new("one.md")).cloned(),
            disk.get(Path::new("one.md"))
                .map(|stamp| whole_seconds(stamp.clone())),
            "the row says the same about the note as the disk does, to the second \
             the index can remember"
        );
        assert!(dir.path().exists());
    }

    /// An index left by an older Shale is a cache, not a record: the notes have it
    /// all, so the answer to a shape this build does not recognise is to start the
    /// cache again rather than to grow a migration framework around it.
    #[test]
    fn an_older_index_is_rebuilt_rather_than_migrated() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".shale")).unwrap();
        {
            let conn = Connection::open(dir.path().join(".shale/shale.db")).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch("PRAGMA user_version = 1;").unwrap();
            conn.execute(
                "INSERT INTO notes (path, created_at, modified_at) VALUES ('gone.md', 5, 5)",
                [],
            )
            .unwrap();
        }

        let db = Db::open(dir.path()).unwrap();
        assert_eq!(db.schema_version(), SCHEMA_VERSION);
        assert_eq!(
            column(&db, "SELECT path FROM notes"),
            Vec::<String>::new(),
            "the v1 rows are gone with the v1 shape"
        );
        // And the columns v2 added are really there.
        db.write(|conn| {
            conn.execute(
                "INSERT INTO notes (path, created_at, modified_at, size) VALUES ('x.md', 1, 1, 7)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            column(&db, "SELECT path FROM notes WHERE size = 7"),
            vec!["x.md"]
        );
    }
}
