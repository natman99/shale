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
use rusqlite::types::Value;
use rusqlite::{params, Connection};

use crate::frontmatter::{self, KeyCensus, Kind, Prop};
use crate::graph::extract_tags;
use crate::markdown::extract_wikilinks;
use crate::mdstyle::object::heading_lines;
use crate::query::Query;
use crate::vault::{rel_text, Vault, VaultEntry};
use crate::watch::{Stamp, VaultChanges};

use super::{now_epoch, Db, Provenance};

/// The `links.target_kind` vocabulary: what a wiki-link's target turned out to be.
pub mod target_kind {
    /// A note in the vault. The `target` column holds its vault-relative path.
    pub const NOTE: &str = "note";
    /// An image or other file that is not a note. Also a path.
    pub const ATTACHMENT: &str = "attachment";
    /// A saved query (`.shq`): the row a `![[query]]` embed or a `[[query]]` link
    /// asserts. Also a path. Filed last, after notes and attachments, so a name that
    /// already means a note keeps meaning that (roadmap P4.3 M4b, ruling 26).
    pub const QUERY: &str = "query";
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
/// One section of one note, as a vault-wide section lookup answers it.
///
/// The note is named because the answer is about the vault; `line` is the **0-based**
/// line index, the same basis the `headings` table stores and a reveal reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Vault-relative path to the note that holds the section.
    pub note: String,
    /// 0-based line index of the heading's line.
    pub line: usize,
    /// The heading's text as the file wrote it, inline markdown included.
    pub text: String,
    /// The marker count, 1 to 6.
    pub depth: u8,
}

/// One section of a note, as schema v4 records it. Derived from the parser's
/// heading scan, never from anything the author wrote: there is no syntax for
/// "this is a section" beyond the heading itself, so the row cannot be lied to.
struct Heading {
    /// 0-based line index — the basis `events.line_start` uses and
    /// [`crate::transclude::heading_row`] returns, so a row drives a reveal with
    /// no translation (headings-plan.md ruling 48).
    line: i64,
    /// The heading's text as written: markers stripped, inline markdown intact
    /// (`The *Fall* of Zhent`). Ruling 47: the index stores claims, and
    /// concealment is a view decision.
    text: String,
    /// The marker count, 1 to 6.
    depth: u8,
}

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
    /// The note's sections, in line order, from the parser's own heading scan.
    headings: Vec<Heading>,
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

    /// Whether the vault's declaration file names this key at all. A rename needs the
    /// yes/no without the kind: the file is hand-kept and a rename of a note does not
    /// move it, so the reader has to be told which name the file is now pointing at.
    pub fn declares(&self, key: &str) -> bool {
        self.0.contains_key(key)
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
    queries: &[VaultEntry],
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
                // Last before the ghost: a saved query is a file the author owns, and
                // `![[query]]` asserts a link to it. The transclusion resolver and the
                // window's follow path order targets the same way, so the row, the
                // colour and the click cannot name three different files.
                None => match vault.resolve_link(target, queries).ok().flatten() {
                    Some(entry) => (rel_text(&entry.path), target_kind::QUERY),
                    None => (target.to_string(), target_kind::GHOST),
                },
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

    // The sections, from the same scan the fold and the `da#` text object read,
    // so a query groups by the sections the editor folds (ruling 46).
    let headings = heading_rows(&text);

    Ok(Record {
        path: rel_text(rel),
        size: meta.len() as i64,
        modified_at,
        words: word_count(&text) as i64,
        tags,
        links,
        edges,
        props,
        headings,
        frontmatter_error,
    })
}

/// The note's sections as index rows, from the parser's own scan — the same walk the
/// fold and the text object read, which is the whole point of ruling 46.
///
/// The text is trimmed at both ends, so trailing spaces are not part of a section's
/// name; the markers are already gone, because the scan never includes them.
fn heading_rows(text: &str) -> Vec<Heading> {
    heading_lines(text)
        .into_iter()
        .map(|head| Heading {
            line: head.line as i64,
            text: text[head.text_start..head.text_end].trim().to_string(),
            depth: head.depth,
        })
        .collect()
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
    // Sections are replaced wholesale, exactly like props: the note is the unit,
    // so a heading that moved or went away takes its row with it (ruling 50).
    conn.execute("DELETE FROM headings WHERE note = ?1", params![note.path])
        .with_context(|| format!("indexing {}", note.path))?;
    for heading in &note.headings {
        conn.execute(
            "INSERT INTO headings (note, line, text, depth) VALUES (?1, ?2, ?3, ?4)",
            params![
                note.path,
                heading.line,
                heading.text,
                i64::from(u32::from(heading.depth))
            ],
        )
        .with_context(|| format!("indexing {}", note.path))?;
    }

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
    conn.execute("DELETE FROM headings WHERE note = ?1", params![path])?;
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
    /// Every alias the vault declares, as `(alias, vault-relative path of the note
    /// that declares it)`, in note order and then list order.
    ///
    /// There is no `aliases` table on purpose: an alias is a property like any other,
    /// and a second table holding the same values would be a second answer to keep in
    /// step. This is the one reader, used by the link pass below and by anything that
    /// wants an alias-aware [`crate::vault::LinkResolver`].
    pub fn aliases(&self) -> Result<Vec<(String, String)>> {
        let conn = self.read()?;
        alias_pairs(&conn)
    }

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
            // A note going away can un-shadow an alias: `[[Zork]]` was `Zork.md`'s
            // while it existed, and belongs to whichever note calls it an alias now.
            file_alias_links(conn)?;
            Ok(())
        })
    }

    /// Move a note's rows from one path to another and record the move as one event.
    ///
    /// The note at `to` is read again rather than the row being renamed, so the new
    /// path is indexed the same way a created note would be even if the rename also
    /// edited the title or the links inside.
    pub fn note_renamed(&self, vault: &Vault, from: &Path, to: &Path) -> Result<()> {
        let old = rel_text(from);
        if !is_markdown(to) {
            // A saved query has no `notes` row to move, and giving it one would put its
            // stem in the list of names a link may resolve to — a claim the markdown
            // scan never made, and one that would colour a dead name as a live one.
            // The links that point *at* the query are re-filed when the notes holding
            // them are rewritten and re-indexed, which is exactly what a rename does.
            let new = rel_text(to);
            return self.write(move |conn| {
                drop_note(conn, &old)?;
                event(conn, &new, EventKind::Rename, Provenance::Rename, &old)?;
                file_alias_links(conn)?;
                Ok(())
            });
        }
        let notes = vault.list_markdown()?;
        let attachments = vault.list_attachments()?;
        let queries = vault.list_queries()?;
        let decls = Decls::read(vault);
        let note = record(vault, to, &notes, &attachments, &queries, &decls)?;
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
            // The rename moved a name, which is exactly the event that can hand a
            // ghost to an alias — or take one back.
            file_alias_links(conn)?;
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

    /// Every key the vault writes, with how far it reaches: the census behind the
    /// vault-wide properties pane (P4.3a M5). Ordered by how many notes write the key
    /// and then by the key, which is the order a person scanning a vocabulary wants —
    /// the keys that matter most are at the top, and a tie is alphabetical rather than
    /// whatever the group-by happened to visit.
    ///
    /// One statement over the whole table rather than one read per key: the pane
    /// asks for every key at once, and `kinds_for_keys` — narrowed to keys a note
    /// already named — is the wrong shape for that question.
    pub fn key_census(&self) -> Result<Vec<KeyCensus>> {
        let conn = self.read()?;
        let mut stmt = conn.prepare_cached(
            "SELECT key, COUNT(DISTINCT note), COUNT(*), GROUP_CONCAT(DISTINCT kind) \
             FROM props GROUP BY key \
             ORDER BY COUNT(DISTINCT note) DESC, key ASC",
        )?;
        let out = stmt
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let notes: i64 = row.get(1)?;
                let values: i64 = row.get(2)?;
                let kinds: String = row.get(3).unwrap_or_default();
                let mut kinds: Vec<Kind> = kinds.split(',').filter_map(Kind::parse).collect();
                // `GROUP_CONCAT` makes no promise about order, so the answer's order
                // is decided here rather than inherited from the scan — and it is
                // sorted by the spelling the reader is shown, because this list is
                // displayed as it is and `Kind`'s own order is the enum's, not the
                // alphabet's.
                kinds.sort_by_key(|kind| kind.as_str());
                Ok(KeyCensus {
                    key,
                    notes: notes as usize,
                    values: values as usize,
                    kinds,
                })
            })?
            .flatten()
            .collect();
        Ok(out)
    }

    /// The properties the index holds for one note, in the order they were written.
    ///
    /// Read back from the DB rather than re-parsed from the buffer, which is the point
    /// of the table: `:props`, a `.shq` body and the `prop:` operator all ask the same
    /// question, and only one of them has the note open.
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

    /// Every section in the vault whose text contains `want`, in note-then-line
    /// order; an empty `want` is the whole outline, up to `limit` rows.
    ///
    /// This is what the `headings_text` index is for: the question is asked across
    /// notes nobody has open, which is the only reason a section needs to be in a
    /// table at all (`headings-plan.md` question G). Matching is SQLite's `LIKE`, so
    /// it is case-insensitive for ASCII — the same slack the anchor matcher gives an
    /// author typing from memory — and the wildcards in what the reader typed are
    /// quoted, so `100%` finds a section called `100%` and not everything.
    pub fn sections_matching(&self, want: &str, limit: usize) -> Result<Vec<Section>> {
        let like = format!("%{}%", escape_like(want.trim()));
        let conn = self.read()?;
        let mut stmt = conn
            .prepare(
                "SELECT note, line, text, depth FROM headings
                 WHERE text LIKE ?1 ESCAPE '\\'
                 ORDER BY note, line
                 LIMIT ?2",
            )
            .context("reading the vault index's sections")?;
        let rows = stmt
            .query_map(params![like, limit as i64], |row| {
                Ok(Section {
                    note: row.get(0)?,
                    line: row.get::<_, i64>(1)? as usize,
                    text: row.get(2)?,
                    depth: row.get::<_, i64>(3)?.clamp(0, 6) as u8,
                })
            })
            .context("reading the vault index's sections")?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.context("reading the vault index's sections")?);
        }
        Ok(out)
    }

    /// Which notes a query selects, as vault-relative paths, sorted.
    ///
    /// This is the reader the `tags`, `links` and `props` tables were built for:
    /// vault search's operators (P4.14) and, later, the same grammar in a `.shq`.
    /// One statement per query, so the operators **and** together and the plan stays
    /// on the indexes the schema already has (`tags_tag`, `links_target`,
    /// `props_key (key, kind)`).
    ///
    /// Two parts are deliberately not SQL:
    ///
    /// - `path:` is answered in Rust — [`crate::glob::covers`] when the pattern is a
    ///   glob, substring when it is not — because SQLite's `LIKE` is a different
    ///   language and the ignore rules already speak `covers`.
    /// - **a comparison never casts.** An ordering reads only rows the index recorded
    ///   as numbers, so a note whose `cr` says `low` is not a note with a low
    ///   challenge rating; `CAST('low' AS REAL)` is `0` and SQLite will not warn.
    ///   Equality follows the value instead: a number compares as a number, anything
    ///   else compares the stored text, which is what makes `prop:due=2026-11-08`
    ///   find a date row.
    pub fn notes_matching(&self, q: &Query) -> Result<Vec<PathBuf>> {
        let (conditions, args, paths) = where_clause(q);
        let sql = format!("SELECT path FROM notes n WHERE 1=1{conditions}");
        let conn = self.read()?;
        let mut stmt = conn.prepare(&sql)?;
        let mut out: Vec<PathBuf> = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |row| {
                row.get::<_, String>(0)
            })?
            .map(|row| row.map(PathBuf::from))
            .collect::<Result<_, _>>()?;
        out.sort();
        drop(stmt);

        // `path:` last, because it is the cheapest filter to apply in Rust and the
        // one SQLite would get wrong.
        if !paths.is_empty() {
            out.retain(|p| paths.iter().all(|pat| path_matches(pat, p)));
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
        let queries = vault.list_queries()?;
        // And one read of the vault's key declarations, shared the same way.
        let decls = Decls::read(vault);

        let mut written: Vec<Record> = Vec::new();
        let mut created: Vec<String> = Vec::new();
        let mut touched: Vec<String> = Vec::new();

        for path in &changes.added {
            if !is_markdown(path) {
                continue;
            }
            match record(vault, path, &notes, &attachments, &queries, &decls) {
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
            match record(vault, path, &notes, &attachments, &queries, &decls) {
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
            // A note this pass wrote may declare an alias that explains a ghost an
            // older note left behind — and neither note's text changed, so nothing
            // else would ever revisit that row. Same transaction: the index is never
            // observable with the alias filed and the link still a ghost.
            file_alias_links(conn)?;
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

/// The `aliases:` values the index holds, as `(alias, note)` pairs, in note order and
/// then list order.
fn alias_pairs(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT note, value FROM props WHERE key = 'aliases' AND value <> '' \
             ORDER BY note, ordinal",
        )
        .context("reading the vault index's aliases")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, String>(0)?))
        })
        .context("reading the vault index's aliases")?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("reading the vault index's aliases")?);
    }
    Ok(out)
}

/// The names the three file rules already answer, lowercased — a vault-relative path,
/// that path without its `.md`, and the file stem — each mapped to the note it answers,
/// shortest path winning a tie exactly as `LinkResolver::by_stem` does.
///
/// This does double duty. An alias whose name is in this set is not an alias for
/// resolution purposes, which is what keeps the fourth rule additive. And a ghost whose
/// text is in this set has a note behind it *now*, whether or not the note existed when
/// the link was indexed — which is the same staleness, one step earlier in the list.
fn answered_paths(conn: &Connection) -> Result<BTreeMap<String, String>> {
    let mut stmt = conn
        .prepare("SELECT path FROM notes ORDER BY path")
        .context("reading the vault index's note names")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .context("reading the vault index's note names")?;
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for row in rows {
        let path = row.context("reading the vault index's note names")?;
        let depth = Path::new(&path).components().count();
        let lower = path.to_lowercase();
        let mut keys = vec![lower.clone()];
        if let Some(stripped) = lower.strip_suffix(".md") {
            keys.push(stripped.to_string());
        }
        if let Some(stem) = Path::new(&path).file_stem().and_then(|s| s.to_str()) {
            keys.push(stem.to_lowercase());
        }
        for key in keys {
            match out.get(&key) {
                Some(current) if Path::new(current).components().count() <= depth => {}
                _ => {
                    out.insert(key, path.clone());
                }
            }
        }
    }
    Ok(out)
}

/// File the links an alias explains.
///
/// A `links` row holds its target as it resolved **when the note was indexed**. An
/// alias declared afterwards — later in the same pass, in a later one, or by a note
/// that did not exist when the link was written — changes nothing on disk, so nothing
/// would re-index the linking note, and the row goes on reading as a ghost: unresolved
/// in the editor, missing from the graph, and absent from the aliased note's backlinks.
/// The fix is a DB edit rather than a re-read: the ghosts are already rows, and the
/// aliases are already in `props`.
///
/// The rule is `LinkResolver`'s, and the shadow check is the half that matters: an
/// alias is consulted only when the three file rules find nothing, so a ghost that a
/// note name answers stays that note's business. `[[Zork]]` written before `Zork.md`
/// existed is re-filed to `Zork.md`, not to whichever note lists `Zork` as an alias —
/// and that re-filing is a second thing this pass fixes for free, since a note created
/// after a link was indexed left the link a ghost until someone edited it.
///
/// A vault that declares no aliases pays nothing here: the scan is gated on there being
/// an alias to consult, so the notes-without-frontmatter majority of a vault never sees
/// the extra query on every save.
///
/// Matching happens in Rust rather than in SQL because the resolver lowercases with
/// Unicode rules while SQLite's `NOCASE` folds ASCII only — an alias differing from its
/// ghost by a non-ASCII letter would be filed by the editor and not by the index, and
/// the whole point of this pass is that the two never disagree.
///
/// One direction this does **not** undo: an alias *removed* leaves the rows it filed
/// pointing at the note. The row holds the resolved path, not what the author typed, so
/// there is nothing to restore it to; the link becomes honest again when the linking
/// note is next edited, or the index is rebuilt. Storing the authored target next to the
/// resolved one is the schema change that would make removal exact, and it is the same
/// door the per-match offsets would open.
fn file_alias_links(conn: &Connection) -> Result<()> {
    let pairs = alias_pairs(conn)?;
    if pairs.is_empty() {
        return Ok(());
    }
    // The alias table, with the resolver's tie-break: shortest path wins, and a tie
    // goes to whoever came first in note order.
    let mut by_alias: BTreeMap<String, String> = BTreeMap::new();
    for (alias, owner) in pairs {
        let key = alias.trim().to_lowercase();
        if key.is_empty() {
            continue;
        }
        match by_alias.get(&key) {
            Some(current)
                if Path::new(current).components().count()
                    <= Path::new(&owner).components().count() => {}
            _ => {
                by_alias.insert(key, owner);
            }
        }
    }
    let answered = answered_paths(conn)?;
    by_alias.retain(|alias, _| !answered.contains_key(alias));
    if by_alias.is_empty() && answered.is_empty() {
        return Ok(());
    }

    let ghosts: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT source, target FROM links WHERE target_kind = 'ghost'")
            .context("reading the vault index's unresolved links")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .context("reading the vault index's unresolved links")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (source, target) in ghosts {
        let key = target.trim().to_lowercase();
        // The file rules first, then the alias — the resolver's order, so a ghost that
        // a note name answers belongs to that note even if some other note has been
        // calling it an alias all along.
        let Some(owner) = answered.get(&key).or_else(|| by_alias.get(&key)).cloned() else {
            continue;
        };
        // `OR IGNORE`, then the delete: the source may already link to the owner under
        // its real name, and two rows for one link would be a lie about the vault. When
        // the update changes nothing, this ghost *is* that duplicate, so it goes away.
        // `target_kind` moves with it: the row is no longer an unresolved name, it is
        // a link to a note, and every reader that filters on the kind has to see that.
        let changed = conn.execute(
            "UPDATE OR IGNORE links SET target = ?1, target_kind = 'note' WHERE source = ?2 AND target = ?3 AND target_kind = 'ghost'",
            params![owner, source, target],
        )?;
        if changed == 0 {
            conn.execute(
                "DELETE FROM links WHERE source = ?1 AND target = ?2 AND target_kind = 'ghost'",
                params![source, target],
            )?;
        }
    }

    // The same for the edges a property wrote: `ally: "[[Hobgoblin]]"` is a link like
    // any other, resolved by the same rules at the same moment.
    let edges: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT src, kind, dst FROM edges")
            .context("reading the vault index's property edges")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .context("reading the vault index's property edges")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (src, kind, dst) in edges {
        let key = dst.trim().to_lowercase();
        let Some(owner) = by_alias.get(&key).cloned() else {
            continue;
        };
        if owner == dst {
            continue;
        }
        let changed = conn.execute(
            "UPDATE OR IGNORE edges SET dst = ?1 WHERE src = ?2 AND kind = ?3 AND dst = ?4",
            params![owner, src, kind, dst],
        )?;
        if changed == 0 {
            conn.execute(
                "DELETE FROM edges WHERE src = ?1 AND kind = ?2 AND dst = ?3",
                params![src, kind, dst],
            )?;
        }
    }
    Ok(())
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

/// The conditions the operators mean, the values they carry, and the `path:` patterns.
///
/// Two doors read this. The live search binds the values to a statement; `:save-query`
/// writes them into a `.shq`, because a saved file has no way to be handed an argument.
/// One builder, or `prop:cr<=5` would mean one thing in the search box and another in the
/// file it saves to.
///
/// The patterns come back uncompiled because `path:` is answered by Shale's own matcher —
/// see [`Db::notes_matching`] — and [`compile_sql`] refuses that operator rather than
/// asking SQLite a different question in the saved file.
fn where_clause(q: &Query) -> (String, Vec<Value>, Vec<String>) {
    use crate::query::{After, Cmp, Op};
    let mut sql = String::new();
    let mut args: Vec<Value> = Vec::new();
    // Path operators are refined after the query; see the method comment.
    let mut paths: Vec<String> = Vec::new();

    for op in &q.ops {
        match op {
            Op::Tag(tag) => add(
                &mut sql,
                &mut args,
                " AND EXISTS (SELECT 1 FROM tags t WHERE t.note = n.path \
                     AND t.tag = ? COLLATE NOCASE)",
                &[tag],
            ),
            Op::Link(name) => {
                // A link is stored as the resolved path, or as the raw `[[name]]`
                // when the target does not exist yet. Both are reachable by the
                // name the user would type: exactly, at the vault root, nested,
                // and as a ghost.
                let file = if name.ends_with(".md") {
                    name.to_string()
                } else {
                    format!("{name}.md")
                };
                let like = format!("%/{}", escape_like(&file));
                add(
                    &mut sql,
                    &mut args,
                    " AND EXISTS (SELECT 1 FROM links l WHERE l.source = n.path AND ( \
                 l.target = ? COLLATE NOCASE \
              OR l.target = ? COLLATE NOCASE \
              OR l.target LIKE ? ESCAPE '\\' \
              OR (l.target_kind = 'ghost' AND l.target = ? COLLATE NOCASE)))",
                    &[name, &file, &like, name],
                );
            }
            Op::Prop { key, cmp, value } => match cmp {
                Cmp::Has => add(
                    &mut sql,
                    &mut args,
                    " AND EXISTS (SELECT 1 FROM props p WHERE p.note = n.path AND p.key = ?)",
                    &[key],
                ),
                Cmp::Ne => {
                    // "cr is not 5" is a statement about notes that *have* a cr.
                    // A note with no cr at all is not a note with a different one.
                    let frag = prop_equality(value.as_deref().unwrap_or(""));
                    add(
                            &mut sql,
                            &mut args,
                            &format!(
                                " AND EXISTS (SELECT 1 FROM props p WHERE p.note = n.path AND p.key = ?) \
                 AND NOT EXISTS (SELECT 1 FROM props p WHERE p.note = n.path AND p.key = ?{frag})"
                            ),
                            &[key, key, value.as_deref().unwrap_or("")],
                        );
                }
                Cmp::Eq => {
                    let frag = prop_equality(value.as_deref().unwrap_or(""));
                    add(
                            &mut sql,
                            &mut args,
                            &format!(
                                " AND EXISTS (SELECT 1 FROM props p WHERE p.note = n.path AND p.key = ?{frag})"
                            ),
                            &[key, value.as_deref().unwrap_or("")],
                        );
                }
                other => {
                    let frag = prop_ordering(*other);
                    add(
                            &mut sql,
                            &mut args,
                            &format!(
                                " AND EXISTS (SELECT 1 FROM props p WHERE p.note = n.path AND p.key = ?{frag})"
                            ),
                            &[key, value.as_deref().unwrap_or("")],
                        );
                }
            },
            Op::Modified(after) => {
                // A date counts from midnight UTC of the day typed; a duration
                // counts back from this moment.
                let from = match after {
                    After::Since(secs) => now_epoch().saturating_sub(*secs),
                    After::At(secs) => *secs,
                };
                add_values(
                    &mut sql,
                    &mut args,
                    " AND n.modified_at >= ?",
                    &[Value::Integer(from)],
                );
            }
            Op::Path(pattern) => paths.push(pattern.clone()),
        }
    }

    (sql, args, paths)
}

/// A search as the SQL a hand-written `.shq` would hold (P4.3 M6).
///
/// This is where "one grammar, one meaning" pays for itself: the operators typed in the
/// search box compile into the statement the file keeps, so saving a search and writing the
/// file out by hand are the same act — and the saved query answers what the search did.
///
/// Three searches cannot be saved, and each is refused by name rather than compiled into a
/// statement that answers a different question later:
///
/// - **plain text**, which searches the *lines* of notes. The index holds tags, links,
///   properties and dates, not lines.
/// - **`path:`**, which Shale answers with [`crate::glob::covers`] — the matcher the ignore
///   rules already speak — while SQLite's `LIKE` is a different language.
/// - **`modified:` with a duration** (`7d`), which counts back from the moment it runs.
///   Written into a file it would freeze that moment and mean "since the day it was saved".
///   A date (`modified:2026-11-01`) is a fixed point and saves fine.
pub fn compile_sql(q: &Query) -> Result<String, String> {
    use crate::query::{After, Op};
    if !q.unknown.is_empty() {
        return Err(format!("not an operator: {}", q.unknown.join(", ")));
    }
    if q.ops.is_empty() {
        return Err(
            "plain text searches the lines of notes, and a saved query asks the index".to_string(),
        );
    }
    if q.ops.iter().any(|op| matches!(op, Op::Path(_))) {
        return Err(
            "path: is answered by Shale's own matcher, not by SQL; a saved query would ask              SQLite a different question"
                .to_string(),
        );
    }
    if q.ops
        .iter()
        .any(|op| matches!(op, Op::Modified(After::Since(_))))
    {
        return Err(
            "modified: with a duration counts from the moment it runs; written into a file it              would mean \"since the day it was saved\" — a date (modified:2026-11-01) saves fine"
                .to_string(),
        );
    }
    let (conditions, args, _) = where_clause(q);
    Ok(format!(
        "SELECT path AS note FROM notes n WHERE 1=1{} ORDER BY path",
        inline(&conditions, &args)
    ))
}

/// The statement with its values written in, in the order the fragments asked for them.
///
/// Only the `?N` marks [`add_values`] wrote are replaced, and the values are appended
/// rather than rescanned, so a tag spelled `what?` stays a tag instead of becoming a hole.
fn inline(sql: &str, args: &[Value]) -> String {
    let mut out = String::new();
    let mut values = args.iter();
    let mut rest = sql;
    while let Some(at) = rest.find('?') {
        let mut end = at + 1;
        while rest[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
        {
            end += 1;
        }
        out.push_str(&rest[..at]);
        match values.next() {
            Some(value) => out.push_str(&literal(value)),
            None => out.push_str(&rest[at..end]),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    debug_assert!(
        values.next().is_none(),
        "every value had a mark waiting for it"
    );
    out
}

/// A value as SQL text: a saved file has no way to be handed an argument.
///
/// Doubling the quotes is SQL's only escaping rule, and the only thing that can go wrong
/// with a value that holds one.
fn literal(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Real(r) => format!("{r}"),
        Value::Text(t) => format!("'{}'", t.replace('\'', "''")),
        Value::Blob(bytes) => format!(
            "x'{}'",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ),
    }
}

/// Append a fragment to the statement, numbering each `?` in it against `args`.
///
/// The fragment is written with plain `?` so it reads like SQL; the numbering is
/// what lets one statement carry every operator of a query.
fn add(sql: &mut String, args: &mut Vec<Value>, frag: &str, values: &[&str]) {
    add_values(
        sql,
        args,
        frag,
        &values
            .iter()
            .map(|v| Value::Text(v.to_string()))
            .collect::<Vec<_>>(),
    )
}

/// [`add`] with values that are already the right SQLite type.
///
/// A timestamp has to arrive as an integer: binding it as text would have SQLite
/// compare an INTEGER column against a string, which is a type comparison rather
/// than the one being asked for.
fn add_values(sql: &mut String, args: &mut Vec<Value>, frag: &str, values: &[Value]) {
    let parts: Vec<&str> = frag.split('?').collect();
    assert_eq!(
        parts.len() - 1,
        values.len(),
        "the fragment and its values must agree"
    );
    let base = args.len();
    for (i, part) in parts.iter().enumerate() {
        sql.push_str(part);
        if i + 1 < parts.len() {
            sql.push_str(&format!("?{}", base + i + 1));
        }
    }
    args.extend(values.iter().cloned());
}

/// The SQL that compares a property row for equality against `value`.
///
/// A numeric value compares numerically and reads only rows the index recorded as
/// numbers; anything else compares the stored text, which is what lets
/// `prop:due=2026-11-08` find a date row without pretending dates are numbers.
fn prop_equality(value: &str) -> String {
    if value.parse::<f64>().is_ok() {
        " AND p.kind = 'number' AND CAST(p.value AS REAL) = ?".to_string()
    } else {
        " AND p.value = ? COLLATE NOCASE".to_string()
    }
}

/// The SQL for an ordering comparison. Always numeric — see
/// [`Db::notes_matching`]. The parser has already refused an ordering whose value
/// is not a number, so a non-numeric value never reaches here.
fn prop_ordering(cmp: crate::query::Cmp) -> &'static str {
    use crate::query::Cmp;
    match cmp {
        Cmp::Lt => " AND p.kind = 'number' AND CAST(p.value AS REAL) < ?",
        Cmp::Le => " AND p.kind = 'number' AND CAST(p.value AS REAL) <= ?",
        Cmp::Gt => " AND p.kind = 'number' AND CAST(p.value AS REAL) > ?",
        Cmp::Ge => " AND p.kind = 'number' AND CAST(p.value AS REAL) >= ?",
        _ => " AND p.kind = 'number' AND CAST(p.value AS REAL) = ?",
    }
}

/// Whether a `path:` pattern selects a note.
///
/// A pattern with glob metacharacters is matched by [`crate::glob::covers`], the
/// same matcher the ignore rules use. Without them it is a substring, because
/// `path:notes/` is how someone says "under notes/" and a glob would have said
/// `notes/*` to mean the same thing.
fn path_matches(pattern: &str, path: &Path) -> bool {
    if pattern.chars().any(|c| matches!(c, '*' | '?' | '[' | '{')) {
        crate::glob::covers(pattern, path)
    } else {
        rel_text(path).contains(pattern)
    }
}

/// Quote the LIKE wildcards in a name used as a link target, so `link:100%` looks
/// for a note called `100%` rather than one whose name ends in anything.
fn escape_like(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_VERSION};
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

    /// One column from every row, **in the order the query asked for it**. [`column`]
    /// sorts, which is right for a set of rows and wrong for a section list, whose
    /// whole point is that it came back in reading order.
    fn column_ordered(db: &Db, sql: &str) -> Vec<String> {
        let conn = db.read().unwrap();
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect()
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

    /// A `![[query]]` embed asserts a link to the saved query, filed as its own kind: an
    /// unresolved-link list can still tell a dead name from a live one, and the backlink
    /// list gets a path to point at (roadmap P4.3 M4b).
    #[test]
    fn a_query_embed_is_indexed_as_a_link_to_the_query() {
        let (_dir, vault, db) = vault_with(&[
            ("Lair.md", "![[tough]]\nand [[tough]] too\n"),
            ("queries/tough.shq", "+++\n+++\nSELECT 1\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT source || ' ' || target || ':' || target_kind FROM links"
            ),
            // One row, not two: the embed and the plain link name one file, and the
            // dedup key is (source, target, kind).
            vec!["Lair.md queries/tough.shq:query"],
            "the saved query is a link target, not a ghost"
        );
    }

    /// Renaming a saved query records the move without giving the query a row in
    /// `notes`. A `.shq` is not a note, and a notes row for it would put its stem in the
    /// list of names a link may resolve to — a claim the markdown scan never made, and
    /// one that would colour a dead name as a live one (roadmap P4.3 M4b).
    #[test]
    fn renaming_a_saved_query_leaves_it_out_of_the_notes_table() {
        let (_dir, vault, db) = vault_with(&[
            ("Lair.md", "![[tough]]\n"),
            ("queries/tough.shq", "+++\n+++\nSELECT 1\n"),
        ]);
        db.reconcile(&vault).unwrap();
        vault
            .rename(
                std::path::Path::new("queries/tough.shq"),
                std::path::Path::new("queries/brutal.shq"),
            )
            .unwrap();
        db.note_renamed(
            &vault,
            std::path::Path::new("queries/tough.shq"),
            std::path::Path::new("queries/brutal.shq"),
        )
        .unwrap();
        assert_eq!(
            column(&db, "SELECT path FROM notes ORDER BY path"),
            vec!["Lair.md"],
            "the query moved, and stayed out of the notes"
        );
    }

    /// The index orders targets the way the resolver and the follow path do: a name that
    /// already means a note keeps meaning that, even when a `.shq` spells it too.
    #[test]
    fn a_note_beats_a_query_in_the_index_as_well() {
        let (_dir, vault, db) = vault_with(&[
            ("Lair.md", "![[tough]]\n"),
            ("tough.md", "a note\n"),
            ("queries/tough.shq", "+++\n+++\nSELECT 1\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT source || ' ' || target || ':' || target_kind FROM links"
            ),
            vec!["Lair.md tough.md:note"],
            "the note wins, in the index as in the embed"
        );
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

    /// The rebuild this release will actually give its users: an index at v3 (no
    /// `headings`) becomes v4 by being dropped, not by an `ALTER TABLE`.
    #[test]
    fn an_index_at_v3_gains_the_headings_table_by_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".shale")).unwrap();
        {
            let conn = Connection::open(dir.path().join(".shale/shale.db")).unwrap();
            conn.execute_batch(SCHEMA_V3).unwrap();
            conn.execute_batch("PRAGMA user_version = 3;").unwrap();
        }
        let db = Db::open(dir.path()).unwrap();
        assert_eq!(db.schema_version(), SCHEMA_VERSION);
        assert_eq!(
            column(&db, "SELECT CAST(COUNT(*) AS TEXT) FROM headings"),
            vec!["0"],
            "the table exists, and it is empty until the notes are read again"
        );
    }

    /// P4.3b M1: the sections a note has reach `headings`, in line order, from the
    /// parser's own scan — the same spans the fold and the `da#` text object read, so
    /// a query groups by the sections the editor folds (ruling 46). The fenced `#` is
    /// not a section, the blockquoted one is, a bare `#` is a section with no name,
    /// and a note may name two sections the same way (EDGE-1/2/3/6).
    #[test]
    fn a_notes_sections_are_indexed_in_line_order() {
        let (_dir, vault, db) = vault_with(&[(
            "Lair.md",
            concat!(
                "intro\n",
                "# The Fall\n",
                "body\n",
                "## Deep *Fall*\n",
                "```\n",
                "# not a heading\n",
                "```\n",
                "> ## Quoted\n",
                "#\n",
                "## Deep *Fall*\n",
            ),
        )]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column_ordered(
                &db,
                "SELECT line || '|' || depth || '|' || text FROM headings ORDER BY line"
            ),
            vec!["1|1|The Fall", "3|2|Deep *Fall*", "8|1|", "9|2|Deep *Fall*",],
            "`line` is 0-based, `text` is what the author wrote (asterisks and all), the \
             fence held the comment out of the section list, and `> ## Quoted` on line 7 is \
             quoted text rather than a section of this note — the parser styles no heading \
             inside a blockquote, which is also what the fold and the anchor matcher do"
        );
    }

    /// P4.3b M3: the table answers the vault-wide question, which is the only reason a
    /// section needs to be in a table at all (question G). Note-then-line order, because
    /// the answer is an outline of the vault rather than a bag of headings.
    #[test]
    fn a_section_can_be_found_across_the_vault() {
        let (_dir, vault, db) = vault_with(&[
            ("Third.md", "## Deep *Fall*\n"),
            ("Lair.md", "# The Fall\n## The Deep\n"),
            ("Other.md", "# the fall\n"),
        ]);
        db.reconcile(&vault).unwrap();
        let found = db.sections_matching("fall", 20).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|s| (s.note.as_str(), s.line, s.text.as_str(), s.depth))
                .collect::<Vec<_>>(),
            vec![
                ("Lair.md", 0, "The Fall", 1),
                ("Other.md", 0, "the fall", 1),
                ("Third.md", 0, "Deep *Fall*", 2),
            ],
            "matched case-insensitively, the way an author types an anchor"
        );

        assert_eq!(
            db.sections_matching("", 20)
                .unwrap()
                .into_iter()
                .map(|s| s.note)
                .collect::<Vec<_>>(),
            vec!["Lair.md", "Lair.md", "Other.md", "Third.md"],
            "an empty pattern is the whole outline, not nothing"
        );
    }

    /// The wildcards in what the reader typed are quoted, so a section called `100%`
    /// is found by `100%` and everything else is not swept in — the same trap the
    /// `link:` operator fell into (P4.14).
    #[test]
    fn a_section_pattern_is_a_substring_and_not_a_glob() {
        let (_dir, vault, db) = vault_with(&[("Weird.md", "## 100%\n## 1000\n## a_b\n## axb\n")]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            db.sections_matching("100%", 20)
                .unwrap()
                .into_iter()
                .map(|s| s.text)
                .collect::<Vec<_>>(),
            vec!["100%"]
        );
        assert_eq!(
            db.sections_matching("a_b", 20)
                .unwrap()
                .into_iter()
                .map(|s| s.text)
                .collect::<Vec<_>>(),
            vec!["a_b"]
        );
    }

    /// The roadmap's stated purpose — a query that groups by section — asked through
    /// the runner a saved query uses. What v4 can group by is the section itself:
    /// props and links have no line column yet, so "which section is this property
    /// in" waits for one, and the plan says so rather than pretending otherwise.
    #[test]
    fn a_saved_query_can_group_the_vault_by_section() {
        let (_dir, vault, db) = vault_with(&[
            ("notes/A.md", "# A\n## Scene\nfirst\n## Scene\nsecond\n"),
            ("notes/B.md", "# B\n## Scene\n"),
            ("notes/C.md", "# C\nno scenes here\n"),
        ]);
        db.reconcile(&vault).unwrap();
        let rows = db
            .run(
                "SELECT note || ' ' || CAST(COUNT(*) AS TEXT) FROM headings
                 WHERE lower(text) = 'scene'
                 GROUP BY note
                 ORDER BY note",
                &[],
            )
            .unwrap();
        let got: Vec<String> = rows.rows.iter().map(|row| row[0].display()).collect();
        assert_eq!(
            got,
            vec!["notes/A.md 2", "notes/B.md 1"],
            "two sections with the same name in one note are two rows (ruling 50), \
             and a note with none is simply absent"
        );
    }

    /// The rows belong to the note: an edit that drops a heading drops its row, and a
    /// deleted note takes its sections with it (ruling 50, EDGE-11/12). Nothing else
    /// points at a heading, so nothing else has to follow.
    #[test]
    fn sections_follow_the_note_being_edited_and_deleted() {
        let (_dir, vault, db) = vault_with(&[("a.md", "# One\n## Two\n")]);
        db.reconcile(&vault).unwrap();
        assert_eq!(column(&db, "SELECT text FROM headings"), vec!["One", "Two"]);

        vault.write("a.md", "# One\nprose only\n").unwrap();
        db.note_saved(&vault, Path::new("a.md"), Provenance::User)
            .unwrap();
        assert_eq!(
            column(&db, "SELECT text FROM headings"),
            vec!["One"],
            "the section that stopped existing stopped being a row"
        );

        db.note_deleted(Path::new("a.md"), Provenance::User)
            .unwrap();
        assert_eq!(
            column(&db, "SELECT CAST(COUNT(*) AS TEXT) FROM headings"),
            vec!["0"],
            "and the note going away took its sections with it"
        );
    }

    /// The table answers the question it was priced for, through the same read-only
    /// runner a saved query runs on: which notes have a section by this name, and what
    /// sections does that note have, in order. The app has no door on these rows yet —
    /// that is M3 — but a table nothing asks about is a shell (question G).
    #[test]
    fn a_query_can_ask_what_sections_a_note_has() {
        let (_dir, vault, db) = vault_with(&[
            ("Lair.md", "# The Fall\n## The Deep\n### Deeper\n"),
            ("Other.md", "# The Fall\n"),
        ]);
        db.reconcile(&vault).unwrap();

        let named = db
            .run(
                "SELECT note || ':' || text FROM headings WHERE text = 'The Fall' ORDER BY note",
                &[],
            )
            .unwrap();
        let got: Vec<String> = named.rows.iter().map(|row| row[0].display()).collect();
        assert_eq!(
            got,
            vec!["Lair.md:The Fall", "Other.md:The Fall"],
            "the `headings_text` index is what a vault-wide anchor lookup reads"
        );

        let outline = db
            .run(
                "SELECT text FROM headings WHERE note = 'Lair.md' ORDER BY line",
                &[],
            )
            .unwrap();
        let got: Vec<String> = outline.rows.iter().map(|row| row[0].display()).collect();
        assert_eq!(
            got,
            vec!["The Fall", "The Deep", "Deeper"],
            "in reading order, which is the order an outline wants"
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

    /// The census the vault-wide pane reads: one row per key, how far each reaches,
    /// and every kind its values come in. `notes` and `values` are different numbers
    /// on purpose — one note writing `cr:` twice is one note and two values — and a
    /// key whose values disagree keeps **both** kinds rather than picking one, which
    /// is the whole reason the census exists.
    #[test]
    fn the_census_says_how_far_each_key_reaches_and_what_kinds_it_wears() {
        let (_dir, vault, db) = vault_with(&[
            ("a.md", "---\ncr: 3\ncr: 4\nname: Goblin\nMax HP: 5\n---\n"),
            ("b.md", "---\ncr: low\nname: Hobgoblin\n---\n"),
        ]);
        db.reconcile(&vault).unwrap();
        let rows: Vec<(String, usize, usize, Vec<&str>)> = db
            .key_census()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c.key.clone(),
                    c.notes,
                    c.values,
                    c.kinds.iter().map(|k| k.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("cr".to_string(), 2, 3, vec!["number", "text"]),
                ("name".to_string(), 2, 2, vec!["text"]),
                ("Max HP".to_string(), 1, 1, vec!["number"]),
            ],
            "the keys that reach furthest come first, ties are alphabetical, and a \
             key's own case and spaces survive the group-by"
        );
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

    /// The same link on both sides of the fence means the same thing: an anchored prop
    /// link resolves to the note, exactly as `[[Waterdeep#Government]]` in the body does.
    #[test]
    fn a_prop_link_to_a_heading_is_an_edge_to_the_note_not_a_ghost() {
        let (_dir, vault, db) = vault_with(&[
            (
                "notes/lares.md",
                "---\nregion: \"[[Waterdeep#Government]]\"\n---\n\nBody\n",
            ),
            ("notes/waterdeep.md", "The city.\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT kind || '=' || dst FROM edges WHERE src = 'notes/lares.md'"
            ),
            vec!["region=notes/waterdeep.md".to_string()],
            "the anchor names a section of Waterdeep, not a note called `Waterdeep#Government`"
        );
        assert_eq!(
            column(&db, "SELECT value || ':' || kind FROM props"),
            vec!["[[Waterdeep#Government]]:link".to_string()],
            "the row still holds what the author typed"
        );
    }

    /// The alias rule end to end: a link written before the name behind it existed
    /// stops being a ghost the moment an `aliases:` list explains it — no file re-read,
    /// no text scan, one DB edit in the same transaction that filed the alias.
    #[test]
    fn an_alias_filed_later_re_files_the_link_that_named_it() {
        let (_dir, vault, db) = vault_with(&[
            ("notes/hero.md", "Met [[Hobgoblin]] in the dark.\n"),
            ("notes/plain.md", "Nothing here.\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT source || ' ' || target || ':' || target_kind FROM links"
            ),
            vec!["notes/hero.md Hobgoblin:ghost".to_string()],
            "before the alias exists, a ghost is the honest answer"
        );

        // The alias arrives in a note that was already indexed. Nothing about `hero.md`
        // changed, so nothing else would ever revisit its row.
        vault
            .write(
                "notes/goblin.md",
                "---\naliases:\n  - Hobgoblin\n---\n# Goblin\n",
            )
            .unwrap();
        db.note_created(&vault, Path::new("notes/goblin.md"), Provenance::External)
            .unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT source || ' ' || target || ':' || target_kind FROM links"
            ),
            vec!["notes/hero.md notes/goblin.md:note".to_string()],
            "the ghost is filed as an edge instead of rotting until someone edits hero.md"
        );
    }

    /// The block the "add properties" command writes opens with `aliases:` left empty,
    /// so an empty value has to be a key the note writes and **not** an alias: an alias
    /// is a name, and the empty string is nobody's name. The row stays in `props` — the
    /// note does write the key, and `:props` and `prop:aliases` should say so — and only
    /// the alias reader skips it.
    #[test]
    fn an_empty_aliases_field_is_a_key_and_not_an_alias() {
        let (_dir, vault, db) = vault_with(&[
            ("notes/goblin.md", "---\naliases:\n---\n# Goblin\n"),
            ("notes/other.md", "---\naliases: anon\n---\n# Other\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT note || '=' || value FROM props WHERE key = 'aliases' ORDER BY note"
            ),
            vec![
                "notes/goblin.md=".to_string(),
                "notes/other.md=anon".to_string()
            ],
            "the unfilled field is still a key the note writes"
        );
        assert_eq!(
            db.aliases().unwrap(),
            vec![("anon".to_string(), "notes/other.md".to_string())],
            "and the empty one is an alias for nothing"
        );
    }

    /// The fourth rule stays fourth, and a property's edge is filed by the same sweep as
    /// a body link.
    #[test]
    fn an_alias_never_steals_a_name_and_files_property_edges_too() {
        let (_dir, vault, db) = vault_with(&[(
            "notes/hero.md",
            "---\nally: \"[[Hobgoblin]]\"\n---\nSaw [[Zork]].\n",
        )]);
        db.reconcile(&vault).unwrap();
        // Both names are ghosts: the body link and the property link, which the body
        // scanner also sees because the block is part of the text.
        assert_eq!(
            column(&db, "SELECT target || ':' || target_kind FROM links"),
            vec!["Hobgoblin:ghost".to_string(), "Zork:ghost".to_string()]
        );
        assert_eq!(
            column(&db, "SELECT kind || '=' || dst FROM edges"),
            vec!["ally=Hobgoblin".to_string()]
        );

        // Two notes arrive at once: one that *is* called Zork, one that calls both names
        // an alias. The file rule wins the first and the alias takes the second.
        vault.write("notes/zork.md", "The tower.\n").unwrap();
        vault
            .write(
                "notes/b.md",
                "---\naliases:\n  - Zork\n  - Hobgoblin\n---\n",
            )
            .unwrap();
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT source || ' ' || target || ':' || target_kind FROM links WHERE source = 'notes/hero.md'"
            ),
            vec![
                "notes/hero.md notes/b.md:note".to_string(),
                "notes/hero.md notes/zork.md:note".to_string(),
            ],
            "a note actually called Zork owns `[[Zork]]`; b.md's alias takes only the name nothing else answers"
        );
        assert_eq!(
            column(&db, "SELECT kind || '=' || dst FROM edges"),
            vec!["ally=notes/b.md".to_string()],
            "the property's edge is filed by the same sweep"
        );
    }

    /// Two notes claiming one alias are settled the way a stem tie is: shortest path,
    /// so the answer is stable across passes rather than whatever SQLite read last.
    #[test]
    fn two_notes_claiming_one_alias_tie_break_like_a_stem() {
        let (_dir, vault, db) = vault_with(&[
            ("notes/hero.md", "Met [[Hobgoblin]].\n"),
            (
                "deep/deeper/goblin.md",
                "---\naliases:\n  - Hobgoblin\n---\n",
            ),
            ("goblin.md", "---\naliases:\n  - Hobgoblin\n---\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            column(
                &db,
                "SELECT target FROM links WHERE source = 'notes/hero.md'"
            ),
            vec!["goblin.md".to_string()]
        );
    }

    /// The one reader both the sweep and an alias-aware resolver go through.
    #[test]
    fn the_index_reads_its_aliases_in_note_and_list_order() {
        let (_dir, vault, db) = vault_with(&[(
            "notes/goblin.md",
            "---\naliases:\n  - Hobgoblin\n  - Yapper\n---\n",
        )]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            db.aliases().unwrap(),
            vec![
                ("Hobgoblin".to_string(), "notes/goblin.md".to_string()),
                ("Yapper".to_string(), "notes/goblin.md".to_string()),
            ]
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

    // --- the query operators (P4.14) -------------------------------------------

    /// The vault the operators are priced against: notes that differ in exactly the
    /// ways a query can ask about — a `cr` that is a number in one note and prose in
    /// another, a tag written two ways, a link to a note that does not exist.
    fn query_vault() -> (tempfile::TempDir, Db) {
        let (dir, vault, db) = vault_with(&[
            (
                "notes/goblin.md",
                "---\ncr: 5\ntags: [monster, goblinoid]\n---\n# Goblin\n\nDigs in a [[lair]].\n",
            ),
            (
                "notes/snare.md",
                "---\ncr: low\ntags: monster\n---\n# Snare\n\nA [[lair]] snare.\n",
            ),
            (
                "notes/lair.md",
                "---\ncr: 8\ntags: [location]\n---\n# Lair\n\nWhere goblins dig.\n",
            ),
            ("factions/zhentarim.md", "# Zhentarim\n\nA faction.\n"),
            (
                "inbox/plot.md",
                "# Plot\n\nA [[Zhentarim]] scheme and a [[Ghost Here]] hole.\n",
            ),
        ]);
        db.reconcile(&vault).unwrap();
        (dir, db)
    }

    /// Run a query and name the notes it selects. Which notes, never how many: two
    /// sets of the same size make a count pass for the wrong answer.
    fn matched(db: &Db, input: &str) -> Vec<String> {
        let q = crate::query::parse(input);
        assert!(q.unknown.is_empty(), "{input} was refused: {q:?}");
        db.notes_matching(&q)
            .unwrap()
            .iter()
            .map(rel_text)
            .collect()
    }

    /// The oracle for M6, and the only one worth writing: the file a search saves has to
    /// answer what the search box answered. Every operator pair here goes through the
    /// compiler and then through the runner, on the same index, and the two answers are
    /// compared as sets of notes.
    #[test]
    fn a_saved_search_asks_the_index_the_same_question_the_search_box_asks() {
        let (_dir, db) = query_vault();
        for input in [
            "tag:monster",
            "link:Zhentarim",
            r#"link:"Ghost Here""#,
            "prop:cr<=5",
            "prop:cr=8",
            r#"prop:"cr"<=5 tag:goblinoid"#,
            "modified:2000-01-01",
        ] {
            let parsed = crate::query::parse(input);
            assert!(parsed.unknown.is_empty(), "{input} was refused: {parsed:?}");
            let sql = compile_sql(&parsed).unwrap_or_else(|err| panic!("{input}: {err}"));
            let rows = db.run(&sql, &[]).unwrap();
            let mut got: Vec<String> = rows.rows.iter().map(|row| row[0].display()).collect();
            got.sort();
            let mut asked = matched(&db, input);
            asked.sort();
            assert_eq!(
                got, asked,
                "{input} compiled into a different question: {sql}"
            );
        }
    }

    /// A value written into a file has to survive being read back out of one. The quote
    /// that SQL has to escape is the one a vault can actually contain.
    #[test]
    fn a_saved_search_keeps_a_value_that_holds_a_quote() {
        let (_dir, vault, db) = vault_with(&[(
            "notes/bob.md",
            "---\ntags: [Bob's gang]\n---\n# Bob\n\nA quote in a name.\n",
        )]);
        db.reconcile(&vault).unwrap();
        let parsed = crate::query::parse(r#"tag:"Bob's gang""#);
        let sql = compile_sql(&parsed).expect("a quoted tag");
        assert!(
            sql.contains("'Bob''s gang'"),
            "the quote is doubled, as SQL reads it: {sql}"
        );
        assert_eq!(matched(&db, r#"tag:"Bob's gang""#), vec!["notes/bob.md"]);
        let rows = db.run(&sql, &[]).unwrap();
        assert_eq!(
            rows.rows
                .iter()
                .map(|row| row[0].display())
                .collect::<Vec<_>>(),
            vec!["notes/bob.md"],
            "and the file answers the same question"
        );
    }

    /// Three searches the index cannot be asked later, each refused by name rather than
    /// compiled into a statement that means something else by then.
    #[test]
    fn a_search_that_cannot_be_saved_says_which_part_of_it_cannot() {
        for (input, why) in [
            ("goblin", "lines"),
            ("path:notes/", "path:"),
            ("modified:7d", "duration"),
        ] {
            let err =
                compile_sql(&crate::query::parse(input)).expect_err("{input} should not save");
            assert!(
                err.contains(why),
                "{input} refused for the wrong reason: {err}"
            );
        }
    }

    /// The link fragment escapes the LIKE wildcards in a name, which only means anything
    /// if the statement says which character escapes them. A note called `100%` is a note,
    /// not a pattern — and this is the statement a saved query keeps, so the mistake would
    /// outlive the search box.
    #[test]
    fn a_link_name_holding_a_wildcard_is_a_name_not_a_pattern() {
        let (_dir, vault, db) = vault_with(&[
            ("notes/bet.md", "# Bet\n\nA [[100%]] chance.\n"),
            ("notes/100%.md", "# 100%\n\nAll of it.\n"),
        ]);
        db.reconcile(&vault).unwrap();
        assert_eq!(
            matched(&db, r#"link:"100%""#),
            vec!["notes/bet.md"],
            "the note that links it, found by a name that happens to hold a wildcard"
        );
    }

    #[test]
    fn tag_link_and_path_each_select_their_notes() {
        let (_dir, db) = query_vault();
        assert_eq!(
            matched(&db, "tag:monster"),
            vec!["notes/goblin.md", "notes/snare.md"],
            "a frontmatter list and a bare scalar are the same tag"
        );
        assert_eq!(
            matched(&db, "link:Zhentarim"),
            vec!["inbox/plot.md"],
            "the name a person would type, not the path they would not"
        );
        assert_eq!(
            matched(&db, r#"link:"Ghost Here""#),
            vec!["inbox/plot.md"],
            "a link to a note that does not exist is still a link, and a name with a
             space is quoted like any other value"
        );
        assert_eq!(
            matched(&db, "path:notes/"),
            vec!["notes/goblin.md", "notes/lair.md", "notes/snare.md"]
        );
        // The glob dialect is the one the ignore rules use, not SQL's `LIKE`: `*`
        // stays inside one segment, so `*.md` is the vault's top level and `**/*.md`
        // is everywhere. Pinning this is the point — a `LIKE '%.md'` would have
        // answered both the same and quietly disagreed with a profile's patterns.
        assert!(
            matched(&db, "path:*.md").is_empty(),
            "no note sits at the vault root"
        );
        assert_eq!(
            matched(&db, "path:notes/*.md"),
            vec!["notes/goblin.md", "notes/lair.md", "notes/snare.md"]
        );
        assert_eq!(
            matched(&db, "path:**/*.md"),
            vec![
                "factions/zhentarim.md",
                "inbox/plot.md",
                "notes/goblin.md",
                "notes/lair.md",
                "notes/snare.md"
            ]
        );
    }

    /// The rule the whole `kind` column exists for. Without the kind filter SQLite
    /// computes `CAST('low' AS REAL)` as `0`, so the note that says `cr: low` ranks
    /// above the one with a real challenge rating — and nothing complains.
    #[test]
    fn a_numeric_comparison_needs_a_number() {
        let (_dir, db) = query_vault();
        assert_eq!(matched(&db, "prop:cr<=5"), vec!["notes/goblin.md"]);
        assert_eq!(matched(&db, "prop:cr>5"), vec!["notes/lair.md"]);
        assert_eq!(
            matched(&db, "prop:cr>=5"),
            vec!["notes/goblin.md", "notes/lair.md"]
        );
        // The prose row is not invisible; it answers a question about text.
        assert_eq!(matched(&db, "prop:cr=low"), vec!["notes/snare.md"]);
        assert_eq!(
            matched(&db, "prop:cr"),
            vec!["notes/goblin.md", "notes/lair.md", "notes/snare.md"],
            "with no comparison the question is only whether the note writes the key"
        );
    }

    /// `cr!=5` is a statement about notes that have a cr. A note with no cr is not a
    /// note with a different one.
    #[test]
    fn not_equal_still_requires_the_key() {
        let (_dir, db) = query_vault();
        assert_eq!(
            matched(&db, "prop:cr!=5"),
            vec!["notes/lair.md", "notes/snare.md"]
        );
    }

    /// Operators **and** together — which is what makes `tag:monster prop:cr<=5` a
    /// question a GM actually asks at the table.
    #[test]
    fn the_operators_and_together() {
        let (_dir, db) = query_vault();
        assert_eq!(
            matched(&db, "tag:monster prop:cr<=5"),
            vec!["notes/goblin.md"]
        );
        assert_eq!(
            matched(&db, "tag:monster link:lair"),
            vec!["notes/goblin.md", "notes/snare.md"]
        );
        assert!(
            matched(&db, "tag:monster link:Zhentarim").is_empty(),
            "no note is both, and saying so is the answer"
        );
    }

    /// `modified:` counts back from now for a duration, and from midnight UTC for a
    /// date. The mtime is set by hand so the test does not depend on when it runs.
    #[test]
    fn modified_counts_back_from_now() {
        let (dir, db) = query_vault();
        let old = UNIX_EPOCH + Duration::from_secs((now_epoch() - 30 * 86_400) as u64);
        let file = std::fs::File::options()
            .write(true)
            .open(dir.path().join("notes/snare.md"))
            .unwrap();
        file.set_times(FileTimes::new().set_modified(old)).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        db.reconcile(&vault).unwrap();

        let recent = matched(&db, "modified:7d");
        assert!(
            !recent.contains(&"notes/snare.md".to_string()),
            "a note untouched for a month is not recent"
        );
        assert!(recent.contains(&"notes/goblin.md".to_string()));
        assert_eq!(
            matched(&db, "modified:60d"),
            vec![
                "factions/zhentarim.md",
                "inbox/plot.md",
                "notes/goblin.md",
                "notes/lair.md",
                "notes/snare.md"
            ]
        );
    }
}
