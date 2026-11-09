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

use crate::frontmatter::{self, Kind, Prop};
use crate::graph::extract_tags;
use crate::markdown::extract_wikilinks;
use crate::vault::{rel_text, Vault, VaultEntry};
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
    /// Vault-relative path, as the `notes.path` column holds it: `/`-joined on every
    /// platform, by [`rel_text`].
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
    /// The frontmatter block's properties, as authored and then declared.
    props: Vec<Prop>,
    /// Typed edges the frontmatter asserts: `(kind, target)` per link-shaped value,
    /// where `kind` is the key that wrote it (`ally`, `region`) and `target` is the
    /// note it resolves to, or what the author wrote when nothing resolves.
    edges: Vec<(String, String)>,
    /// Why the block could not be read, if it could not. The note still gets its
    /// other rows — a bad block costs its own note its props, not the pass — and the
    /// pass reports it, because a silently empty `props` is a misleading answer.
    frontmatter_error: Option<String>,
}

/// The vault's key declarations: `<vault>/.shale/props.toml`, `cr = "number"`.
///
/// A declaration is the override half of the inferred-first / override-second rule
/// (ruling §1.2), and it is read once per pass rather than once per note because it
/// is one small file. A missing file is not an error and not a warning: a vault
/// without declarations gets inference alone.
pub struct Decls(BTreeMap<String, Kind>);

impl Decls {
    /// Read the declarations, ignoring a file that is absent or unreadable.
    ///
    /// Unreadable is ignored rather than reported because the file is optional and a
    /// vault whose *declarations* are broken should still get an index; the census
    /// that `:props` shows is where a key the author meant to declare shows up with
    /// the kind inference gave them.
    pub fn read(vault: &Vault) -> Decls {
        let path = vault.root().join(super::STATE_DIR).join("props.toml");
        let Ok(text) = fs::read_to_string(&path) else {
            return Decls(BTreeMap::new());
        };
        let Ok(map) = toml::from_str::<BTreeMap<String, String>>(&text) else {
            return Decls(BTreeMap::new());
        };
        Decls(
            map.into_iter()
                .filter_map(|(key, kind)| Kind::parse(&kind).map(|kind| (key, kind)))
                .collect(),
        )
    }

    /// The declared kind of a key, if the vault declares one.
    fn get(&self, key: &str) -> Option<Kind> {
        self.0.get(key).copied()
    }
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
    decls: &Decls,
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
            Ok(Some(entry)) => (rel_text(&entry.path), target_kind::NOTE),
            // A target that names a real note but fails to resolve (a path that
            // escapes the vault, say) is treated as unresolved rather than dropped,
            // so the author still sees it in an unresolved-link list.
            _ => match vault.resolve_attachment(target, attachments) {
                Some(entry) => (rel_text(&entry.path), target_kind::ATTACHMENT),
                None => (target.to_string(), target_kind::GHOST),
            },
        };
        // One row per (source, target, kind): a note that links to the same note
        // three times has one link. The separator keeps `a|b` and `a\u{0}b` apart.
        if seen.insert(format!("{stored}\u{0}{kind}")) {
            links.push((stored, kind));
        }
    }

    // The frontmatter block, if the note opens with one. Parsing happens here, on the
    // caller's thread and outside the write transaction (EDGE-26), and the scan stops
    // at the closing fence, so a 5k-note vault pays for the block's head rather than
    // for the note (EDGE-27).
    let block = frontmatter::block(&text);
    let (props, frontmatter_error) = match block {
        None => (Vec::new(), None),
        Some(block) if block.malformed.is_some() => {
            (Vec::new(), Some(block.malformed.unwrap_or_default()))
        }
        Some(block) => {
            let mut props = block.props;
            for prop in props.iter_mut() {
                if let Some(kind) = decls.get(&prop.key) {
                    frontmatter::declare(prop, kind);
                }
            }
            (props, None)
        }
    };

    // The projections the props feed (ruling §1.3). Both are additive: the body's
    // `#tags` and `[[links]]` keep working exactly as they did, and frontmatter adds
    // to them rather than replacing them — which is a test, not a comment.
    let mut tags = extract_tags(&text);
    for prop in &props {
        if frontmatter::is_tag_key(&prop.key) {
            let tag = prop.value.trim().trim_start_matches('#').trim();
            if !tag.is_empty() && !tags.iter().any(|t| t == tag) {
                tags.push(tag.to_string());
            }
        }
    }

    // Only the `[[…]]` spelling is a link. The plan's looser test — "or resolves to a
    // note path" — was tried and rejected by its own consequence: a statblock that
    // says `name: Goblin` in a vault that has a `Goblin.md` would assert a link to
    // itself, and every `type: goblin` in the vault would light up as an edge. Naming a
    // note is not the same claim as linking to it, and Obsidian agrees: an unquoted
    // value is a string. So the author's brackets are the intent, and a bare value
    // stays text for a query to compare however it likes.
    let resolver = crate::vault::LinkResolver::new(notes);

    // Every link-shaped value is one directed edge, keyed by the property that wrote
    // it (EDGE-16). A target that resolves to nothing still gets its row: an ally
    // that is not a note is a plot hole worth seeing, not a row to drop.
    let mut edges: Vec<(String, String)> = Vec::new();
    for prop in &props {
        if prop.kind != Kind::Link {
            continue;
        }
        let target = frontmatter::wikilink(&prop.value).unwrap_or(prop.value.trim());
        if target.is_empty() {
            continue;
        }
        let stored = match resolver.resolve_link(target) {
            Some(entry) => rel_text(&entry.path),
            None => target.to_string(),
        };
        if !edges.iter().any(|(_, dst)| dst == &stored) {
            edges.push((prop.key.clone(), stored));
        }
    }

    Ok(Record {
        path: rel_text(rel),
        size: meta.len() as i64,
        modified_at,
        words: word_count(&text) as i64,
        tags,
        links,
        edges,
        props,
        frontmatter_error,
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

    // Typed edges, one row per link-shaped value, keyed by the property that wrote it.
    // Only the note's OWN rows go here: a row pointing at this note belongs to the note
    // that wrote it, and wiping it on this note's re-index loses a fact nobody asked to
    // lose (`links` deletes by source for exactly this reason; the `target`/`dst` delete
    // belongs to drop_note, where the pointing notes really are due to be re-read).
    conn.execute("DELETE FROM edges WHERE src = ?1", params![note.path])
        .with_context(|| format!("indexing {}", note.path))?;
    for (kind, dst) in &note.edges {
        conn.execute(
            "INSERT OR IGNORE INTO edges (src, kind, dst) VALUES (?1, ?2, ?3)",
            params![note.path, kind, dst],
        )
        .with_context(|| format!("indexing {}", note.path))?;
    }

    // Properties are replaced wholesale (EDGE-19): delete-then-insert for the note's
    // whole prop set, with no per-key diff. That is what lets a frontmatter block the
    // author deleted take its rows with it, and it is why `props_note` exists.
    conn.execute("DELETE FROM props WHERE note = ?1", params![note.path])
        .with_context(|| format!("indexing {}", note.path))?;
    for prop in &note.props {
        conn.execute(
            "INSERT INTO props (note, key, ordinal, value, kind) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                note.path,
                prop.key,
                // NULL means a scalar; an index means one item of a list.
                prop.ordinal.map(|o| o as i64),
                prop.value,
                prop.kind.as_str()
            ],
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
    conn.execute(
        "DELETE FROM edges WHERE src = ?1 OR dst = ?1",
        params![path],
    )?;
    conn.execute("DELETE FROM props WHERE note = ?1", params![path])?;
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
        let path = rel_text(rel);
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
        let decls = Decls::read(vault);
        let note = record(vault, to, &notes, &attachments, &decls)?;
        let old = rel_text(from);
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

    /// The properties the index holds for one note, in the order they were written.
    ///
    /// Read back from the DB rather than re-parsed from the buffer, which is the point
    /// of the table: `:props`, a `.shq` body and the `prop:` operator all ask the same
    /// question, and only one of them has the note open.
    /// The kinds the vault holds for each key the caller is looking at — the kind
    /// census (ruling §1.3), narrowed to those keys so the read stays on the
    /// `props_key (key, kind)` index. A key with more than one kind is a disagreement
    /// worth *showing*: the app badges it and the query author decides what to do about
    /// it. This is why `declare` refuses to cast a value that cannot carry a kind — a
    /// row that lies about its kind makes the census report a clean vault.
    pub fn kinds_for_keys(&self, keys: &[&str]) -> Result<BTreeMap<String, Vec<Kind>>> {
        let conn = self.read()?;
        let mut out = BTreeMap::new();
        for key in keys {
            let mut stmt = conn
                .prepare_cached("SELECT DISTINCT kind FROM props WHERE key = ?1 ORDER BY kind")?;
            let kinds: Vec<Kind> = stmt
                .query_map(params![key], |row| {
                    Ok(Kind::parse(&row.get::<_, String>(0)?))
                })?
                .flatten()
                .flatten()
                .collect();
            if !kinds.is_empty() {
                out.insert((*key).to_string(), kinds);
            }
        }
        Ok(out)
    }

    pub fn note_props(&self, rel: &Path) -> Result<Vec<Prop>> {
        let path = rel_text(rel);
        let conn = self.read()?;
        let mut stmt = conn
            .prepare(
                "SELECT key, value, ordinal, kind FROM props WHERE note = ?1 \
                 ORDER BY rowid",
            )
            .context("reading the vault index's properties")?;
        let rows = stmt
            .query_map(params![path], |row| {
                let kind: String = row.get(3)?;
                Ok(Prop {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    ordinal: row.get::<_, Option<i64>>(2)?.map(|o| o as usize),
                    kind: Kind::parse(&kind).unwrap_or(Kind::Text),
                })
            })
            .context("reading the vault index's properties")?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.context("reading the vault index's properties")?);
        }
        Ok(out)
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
        // And one read of the vault's key declarations, shared the same way.
        let decls = Decls::read(vault);

        let mut written: Vec<Record> = Vec::new();
        let mut created: Vec<String> = Vec::new();
        let mut touched: Vec<String> = Vec::new();

        for path in &changes.added {
            if !is_markdown(path) {
                continue;
            }
            match record(vault, path, &notes, &attachments, &decls) {
                Ok(note) => {
                    report_frontmatter(&mut report, path, &note);
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
            match record(vault, path, &notes, &attachments, &decls) {
                Ok(note) => {
                    report_frontmatter(&mut report, path, &note);
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
            .map(rel_text)
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

/// Say so when a note's frontmatter block would not parse.
///
/// The note keeps its other rows and the pass carries on — one bad block costs its
/// own note its props, not the vault its index — but an empty `props` that came from
/// a parse failure is a different answer from an empty `props` that came from a note
/// with no frontmatter, and the difference belongs in the report.
fn report_frontmatter(report: &mut SyncReport, path: &Path, note: &Record) {
    if let Some(why) = &note.frontmatter_error {
        report
            .failed
            .push((path.to_path_buf(), format!("frontmatter: {why}")));
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
    use crate::db::schema::{SCHEMA_V1, SCHEMA_V2, SCHEMA_VERSION};
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

    /// The rebuild that will actually happen for our users: an index at v2 (no
    /// `props`) becomes v3 by being dropped, not by an `ALTER TABLE`.
    #[test]
    fn an_index_at_the_previous_version_gains_the_props_table_by_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".shale")).unwrap();
        {
            let conn = Connection::open(dir.path().join(".shale/shale.db")).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute_batch("PRAGMA user_version = 2;").unwrap();
        }
        let db = Db::open(dir.path()).unwrap();
        assert_eq!(db.schema_version(), SCHEMA_VERSION);
        assert_eq!(
            column(&db, "SELECT CAST(COUNT(*) AS TEXT) FROM props"),
            vec!["0"],
            "the table exists, and it is empty until the notes are read again"
        );
    }

    /// P4.3a M1: the frontmatter block reaches `props`, one row per value, with the
    /// kind inferred from the value itself.
    #[test]
    fn a_note_properties_are_indexed_one_row_per_value() {
        let (_dir, vault, db) = vault_with(&[(
            "Goblin.md",
            "---\ncr: 1/4\ntype: goblin\nhp: 7\naliases:\n  - Goblin\n  - Hobgoblin\n---\n# Goblin\n",
        )]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT key || '=' || COALESCE(CAST(ordinal AS TEXT), '-') \
                 || ':' || value || ':' || kind FROM props"
            ),
            vec![
                "aliases=0:Goblin:text",
                "aliases=1:Hobgoblin:text",
                // As authored: `1/4` is text in YAML, and the index says so rather
                // than quietly deciding otherwise.
                "cr=-:1/4:text",
                "hp=-:7:number",
                "type=-:goblin:text",
            ]
        );
    }

    /// Ruling §1.2 and EDGE-17: a key the vault declares wins over inference, and a
    /// declared-numeric key normalizes the fraction spelling the domain uses.
    #[test]
    fn a_declared_key_overrides_the_inferred_kind() {
        let (dir, vault, db) = vault_with(&[("Goblin.md", "---\ncr: 1/4\n---\nbody\n")]);
        std::fs::write(
            dir.path().join(".shale").join("props.toml"),
            "cr = \"number\"\n",
        )
        .unwrap();
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(&db, "SELECT value || ':' || kind FROM props"),
            vec!["0.25:number"],
            "a `cr <= 5` query has to see CR 1/4 as a number"
        );
    }

    /// The declaration is a claim about the key; a prose value is a claim about the
    /// row; when they disagree the row keeps its truth and the census shows the
    /// conflict. Declaring `session` numeric is the case the GM round added (as text,
    /// `Session 10` sorts before `Session 2`) — and the same declaration must not turn
    /// `session: tonight` into a number, because SQLite reads `CAST('tonight' AS REAL)`
    /// as 0 and a chronological query would rank it first in silence.
    #[test]
    fn a_declaration_does_not_cast_a_value_that_cannot_carry_it() {
        let (dir, vault, db) = vault_with(&[
            ("Session-15.md", "---\nsession: 15\n---\nbody\n"),
            ("Loose.md", "---\nsession: tonight\n---\nbody\n"),
        ]);
        std::fs::write(
            dir.path().join(".shale").join("props.toml"),
            "session = \"number\"\n",
        )
        .unwrap();
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(&db, "SELECT note || ' ' || value || ':' || kind FROM props"),
            ["Loose.md tonight:text", "Session-15.md 15:number",]
                .map(String::from)
                .to_vec()
        );
        // The census (`GROUP BY key, kind`) is what makes the disagreement a thing the
        // user can see rather than a wrong answer, and it only works because both kinds
        // survived in the table.
        assert_eq!(
            column(&db, "SELECT key || '=' || kind || '=' || CAST(COUNT(*) AS TEXT) FROM props GROUP BY key, kind"),
            ["session=number=1", "session=text=1"].map(String::from).to_vec()
        );
    }

    /// EDGE-19: re-indexing replaces the note's whole prop set, so a frontmatter
    /// block the author deleted takes its rows with it.
    #[test]
    fn a_deleted_block_takes_its_rows_with_it() {
        let (_dir, vault, db) = vault_with(&[("one.md", "---\ncr: 1\n---\nbody\n")]);
        db.reconcile(&vault).unwrap();
        assert_eq!(column(&db, "SELECT value FROM props"), vec!["1"]);
        vault.write("one.md", "body only now\n").unwrap();
        db.note_saved(&vault, Path::new("one.md"), Provenance::User)
            .unwrap();
        assert!(
            column(&db, "SELECT value FROM props").is_empty(),
            "no per-key diff means no row left behind"
        );
    }

    /// EDGE-24: a deleted note takes its props with it.
    #[test]
    fn a_deleted_note_takes_its_props_with_it() {
        let (_dir, vault, db) = vault_with(&[("gone.md", "---\ncr: 1\n---\nbody\n")]);
        db.reconcile(&vault).unwrap();
        db.note_deleted(Path::new("gone.md"), Provenance::External)
            .unwrap();
        assert!(column(&db, "SELECT value FROM props").is_empty());
    }

    /// EDGE-20: a rename must not leave props pointing at the old path.
    #[test]
    fn a_renamed_notes_props_follow_it() {
        let (_dir, vault, db) = vault_with(&[("old.md", "---\ncr: 3\n---\nbody\n")]);
        db.reconcile(&vault).unwrap();
        vault.rename("old.md", "new.md").unwrap();
        db.note_renamed(&vault, Path::new("old.md"), Path::new("new.md"))
            .unwrap();
        assert_eq!(
            column(&db, "SELECT note || '=' || value FROM props"),
            vec!["new.md=3"]
        );
    }

    /// The error posture, inherited from the indexer: a block that will not parse
    /// costs that note its props, is reported, and costs nothing else.
    #[test]
    fn a_broken_block_costs_one_note_its_props_and_says_so() {
        let (_dir, vault, db) = vault_with(&[
            ("bad.md", "---\nsummary: a: b\n---\nbody text\n"),
            ("good.md", "---\ncr: 2\n---\n"),
        ]);
        let report = db.reconcile(&vault).unwrap();
        assert_eq!(report.indexed, 2, "both notes still got their rows");
        assert_eq!(report.failed.len(), 1);
        assert!(
            report.failed[0].1.starts_with("frontmatter:"),
            "the report says which part failed: {:?}",
            report.failed
        );
        assert_eq!(
            column(&db, "SELECT value FROM props"),
            vec!["2"],
            "only the bad note lost its props"
        );
        assert_eq!(
            column(&db, "SELECT path FROM notes WHERE path = 'bad.md'"),
            vec!["bad.md"],
            "and it is still a note in the index"
        );
    }

    /// EDGE-8: a duplicate key is findable in SQL with no column for it, because a
    /// repeated occurrence restarts its ordinals.
    #[test]
    fn a_duplicate_key_shows_itself_in_the_census() {
        let (_dir, vault, db) = vault_with(&[(
            "dup.md",
            "---\ntags: a\ntags: b\nsolo: x\nlist:\n  - p\n  - q\n---\n",
        )]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT key FROM props WHERE note = 'dup.md' GROUP BY key \
                 HAVING COUNT(*) > COUNT(DISTINCT COALESCE(ordinal, -1))"
            ),
            vec!["tags"],
            "two scalars for one key, and nothing else in the note looks like that"
        );
    }

    /// The regression the plan asks for by name: a note with body tags and no
    /// frontmatter indexes exactly as it did before props existed.
    #[test]
    fn a_note_without_frontmatter_indexes_as_it_always_did() {
        let (_dir, vault, db) =
            vault_with(&[("plain.md", "# Plain\n\nA [[Goblin]] and #alpha.\n")]);
        let report = db.reconcile(&vault).unwrap();
        assert!(report.failed.is_empty());
        assert_eq!(column(&db, "SELECT tag FROM tags"), vec!["alpha"]);
        assert_eq!(
            column(&db, "SELECT target || ':' || target_kind FROM links"),
            vec!["Goblin:ghost"],
            "no note of that name in this vault, so a ghost — as before props existed"
        );
        assert!(column(&db, "SELECT value FROM props").is_empty());
    }

    /// The read side `:props` uses: the note's own rows, in order, kinds decoded.
    #[test]
    fn the_index_answers_for_one_notes_props() {
        let (_dir, vault, db) =
            vault_with(&[("one.md", "---\nname: Goblin\ndead: 2024-01-31\n---\n")]);
        db.reconcile(&vault).unwrap();
        let rows = db.note_props(Path::new("one.md")).unwrap();
        assert_eq!(rows.len(), 2);
        // Authored order, not alphabetical: the rows were inserted in the order the
        // block was written, and the rowid keeps that.
        assert_eq!(rows[0].key, "name");
        assert_eq!(rows[0].kind, Kind::Text);
        assert_eq!(rows[1].key, "dead");
        assert_eq!(rows[1].kind, Kind::Date);
    }

    /// The M2 exit criterion in both directions: frontmatter tags add to the body's
    /// `#tags`, and a note with no frontmatter indexes exactly as it did before
    /// properties existed at all.
    #[test]
    fn a_frontmatter_tag_adds_to_the_body_tags_and_a_plain_note_is_unchanged() {
        let (_dir, vault, db) = vault_with(&[
            (
                "notes/both.md",
                "---\ntags:\n  - \"#party\"\n  - villain\n---\nBody #bodytag text\n",
            ),
            ("notes/plain.md", "Body #bodytag only\n"),
        ]);
        db.reconcile(&vault).unwrap();
        // The leading `#` on a frontmatter tag is optional and means the same thing
        // either way; the body tag and both frontmatter tags are rows.
        assert_eq!(
            column(
                &db,
                "SELECT note || ' ' || tag FROM tags WHERE note = 'notes/both.md'"
            ),
            [
                "notes/both.md bodytag",
                "notes/both.md party",
                "notes/both.md villain",
            ]
            .map(String::from)
            .to_vec()
        );
        // The regression half: nothing about a body-only note moved.
        assert_eq!(
            column(&db, "SELECT tag FROM tags WHERE note = 'notes/plain.md'"),
            vec!["bodytag".to_string()]
        );
    }

    /// EDGE-16: every link-shaped value is one directed row keyed by the property that
    /// wrote it, and a target that resolves to nothing keeps its row anyway.
    #[test]
    fn a_link_property_writes_typed_edges_and_a_ghost_keeps_its_row() {
        let (_dir, vault, db) = vault_with(&[
            (
                "notes/lares.md",
                "---\nregion: \"[[Waterdeep]]\"\nally:\n  - \"[[Zhentarim]]\"\n  - \"[[Ashover]]\"\n---\n\nBody\n",
            ),
            ("notes/waterdeep.md", "The city.\n"),
        ]);
        db.reconcile(&vault).unwrap();
        let rows = column(
            &db,
            "SELECT kind || '=' || dst FROM edges WHERE src = 'notes/lares.md'",
        );
        // A wikilink that names a note is stored resolved; the two allies are rows of
        // their own; and the ghost is stored as authored, because an ally that is not a
        // note is a plot hole worth seeing rather than a row to drop.
        assert_eq!(
            rows,
            [
                "ally=Ashover",
                "ally=Zhentarim",
                "region=notes/waterdeep.md",
            ]
            .map(String::from)
            .to_vec()
        );

        // The prop agrees with the edge table about what is a link.
        assert_eq!(
            column(
                &db,
                "SELECT key || '=' || kind FROM props WHERE note = 'notes/lares.md' ORDER BY key",
            ),
            // Two list items are two rows, which is what a repeated key means here.
            ["ally=link", "ally=link", "region=link"]
                .map(String::from)
                .to_vec()
        );
    }

    /// Re-indexing replaces a note's edges rather than adding to them, and deleting the
    /// note takes its rows with it.
    #[test]
    fn an_edge_set_is_replaced_on_reindex_and_dropped_with_the_note() {
        let (_dir, vault, db) = vault_with(&[("a.md", "---\nally: \"[[B]]\"\n---\n\nBody\n")]);
        let edges = |db: &Db| column(db, "SELECT src || ' ' || kind || '=' || dst FROM edges");
        db.reconcile(&vault).unwrap();
        assert_eq!(edges(&db), vec!["a.md ally=B".to_string()]);
        vault
            .write(
                "a.md",
                "---\nally: \"[[B]]\"\nally: \"[[C]]\"\n---\n\nBody\n",
            )
            .unwrap();
        db.note_saved(&vault, Path::new("a.md"), Provenance::User)
            .unwrap();
        assert_eq!(
            edges(&db),
            ["a.md ally=B", "a.md ally=C"].map(String::from).to_vec(),
            "re-indexing replaces the note's edges rather than adding to them"
        );
        vault.delete("a.md").unwrap();
        db.note_deleted(Path::new("a.md"), Provenance::External)
            .unwrap();
        assert!(edges(&db).is_empty());
    }
}
