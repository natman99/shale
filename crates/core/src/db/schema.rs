//! The shape of the vault index, and the one path that brings an existing DB to it.
//!
//! The DB is a **cache**: the notes are the source of truth, so a schema this
//! build does not recognise is rebuilt rather than migrated, and deleting
//! `.shale/shale.db` is always a valid answer to a broken index. That is what makes
//! the version check a guard instead of a migration framework: an older index is
//! dropped and filled again from the notes, and only a **newer** one — whose rows
//! this build would misunderstand — is refused out loud.
//!
//! Timestamps are **unix epoch seconds** in an `INTEGER`. SQLite renders them with
//! `datetime(col, 'unixepoch')`, they sort and subtract correctly, and it keeps
//! `shale-core` free of a date library for a column nothing in the editor reads.

use anyhow::{Context as _, Result};
use rusqlite::Connection;

/// The shape this build of Shale expects, stored in the DB's own
/// `user_version` so an older or newer file says which it is.
///
/// Bumped by changing [`SCHEMA_V2`] without touching [`SCHEMA_V1`]: an index at an
/// older version is dropped and rebuilt by [`apply`], never migrated.
pub const SCHEMA_VERSION: i64 = 2;

/// v1: the tables roadmap P1.2 lists.
///
/// `events` gains an `id` rowid the roadmap's sketch does not mention: the
/// scrubber and anything that reads the log back needs a stable order within the
/// second its rows were stamped, and an append-only table is exactly the case a
/// rowid is good at.
///
/// Kept after being superseded so the test that an older index is rebuilt has a
/// real v1 to build rather than a sketch of one; nothing at runtime needs it.
#[cfg(test)]
pub(crate) const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
    path             TEXT    PRIMARY KEY,
    created_at       INTEGER NOT NULL,
    modified_at      INTEGER NOT NULL,
    word_count       INTEGER NOT NULL DEFAULT 0,
    created_by       TEXT    NOT NULL DEFAULT '',
    last_modified_by TEXT    NOT NULL DEFAULT '',
    open_count       INTEGER NOT NULL DEFAULT 0,
    edit_count       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tags (
    note TEXT NOT NULL,
    tag  TEXT NOT NULL,
    PRIMARY KEY (note, tag)
);
CREATE INDEX IF NOT EXISTS tags_tag ON tags (tag);

CREATE TABLE IF NOT EXISTS links (
    source       TEXT    NOT NULL,
    target       TEXT    NOT NULL,
    first_seen_at INTEGER NOT NULL,
    last_seen_at  INTEGER NOT NULL,
    PRIMARY KEY (source, target)
);
CREATE INDEX IF NOT EXISTS links_target ON links (target);

CREATE TABLE IF NOT EXISTS events (
    id         INTEGER PRIMARY KEY,
    note       TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    at         INTEGER NOT NULL,
    provenance TEXT    NOT NULL,
    detail     TEXT    NOT NULL DEFAULT '',
    line_start INTEGER,
    line_end   INTEGER
);
CREATE INDEX IF NOT EXISTS events_note_at ON events (note, at);

CREATE TABLE IF NOT EXISTS marks (
    name       TEXT    PRIMARY KEY,
    path       TEXT    NOT NULL,
    position   INTEGER NOT NULL,
    label      TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS edges (
    src  TEXT NOT NULL,
    kind TEXT NOT NULL,
    dst  TEXT NOT NULL,
    PRIMARY KEY (src, kind, dst)
);
CREATE INDEX IF NOT EXISTS edges_dst ON edges (dst, kind);
"#;

/// v2: v1 plus the two columns the indexer needs and the roadmap's sketch implies
/// but does not name.
///
/// `notes.size` is the byte size of the note when it was indexed. The roadmap
/// records created/modified "from `watch::Stamp`", and a stamp is a size *and* an
/// mtime: without the size, an edit that lands in the same second the note was
/// last indexed is invisible to the startup reconciliation, because the two
/// `modified_at` values match.
///
/// `links.target_kind` is the roadmap's own `target_kind` (note, attachment or
/// ghost). `links` is the body's wiki-links (what `first_seen_at` powers link-age
/// colouring with); `edges` is the typed layer, where a frontmatter link property
/// and a body wikilink are different `kind`s. Display inverses live in the
/// edge-type registry, never here — one directed row per fact.
const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
    path             TEXT    PRIMARY KEY,
    created_at       INTEGER NOT NULL,
    modified_at      INTEGER NOT NULL,
    size             INTEGER NOT NULL DEFAULT 0,
    word_count       INTEGER NOT NULL DEFAULT 0,
    created_by       TEXT    NOT NULL DEFAULT '',
    last_modified_by TEXT    NOT NULL DEFAULT '',
    open_count       INTEGER NOT NULL DEFAULT 0,
    edit_count       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tags (
    note TEXT NOT NULL,
    tag  TEXT NOT NULL,
    PRIMARY KEY (note, tag)
);
CREATE INDEX IF NOT EXISTS tags_tag ON tags (tag);

CREATE TABLE IF NOT EXISTS links (
    source       TEXT    NOT NULL,
    target       TEXT    NOT NULL,
    target_kind TEXT NOT NULL DEFAULT 'note',
    first_seen_at INTEGER NOT NULL,
    last_seen_at  INTEGER NOT NULL,
    PRIMARY KEY (source, target, target_kind)
);
CREATE INDEX IF NOT EXISTS links_target ON links (target);

CREATE TABLE IF NOT EXISTS events (
    id         INTEGER PRIMARY KEY,
    note       TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    at         INTEGER NOT NULL,
    provenance TEXT    NOT NULL,
    detail     TEXT    NOT NULL DEFAULT '',
    line_start INTEGER,
    line_end   INTEGER
);
CREATE INDEX IF NOT EXISTS events_note_at ON events (note, at);

CREATE TABLE IF NOT EXISTS marks (
    name       TEXT    PRIMARY KEY,
    path       TEXT    NOT NULL,
    position   INTEGER NOT NULL,
    label      TEXT    NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS edges (
    src  TEXT NOT NULL,
    kind TEXT NOT NULL,
    dst  TEXT NOT NULL,
    PRIMARY KEY (src, kind, dst)
);
CREATE INDEX IF NOT EXISTS edges_dst ON edges (dst, kind);
"#;

/// The tables each superseded version created, so an index at an older version can
/// be dropped rather than migrated. Every name here is this crate's own constant.
const SUPERSEDED_TABLES: [&str; 6] = ["notes", "tags", "links", "events", "marks", "edges"];

/// Bring `conn` to [`SCHEMA_VERSION`].
///
/// Every statement is `IF NOT EXISTS`, so this is safe to run against a database
/// that already holds the tables — which is the normal case, since `Db::open` runs
/// it on every launch.
///
/// An index at an **older** version is dropped and rebuilt. That is not laziness: a
/// migration framework exists to protect data nothing else has, and this index is a
/// cache the notes can always rebuild, so the honest answer to a shape this build
/// does not recognise is to start the cache again. A **newer** version is refused,
/// because rebuilding there would throw away rows the newer Shale expects.
pub(crate) fn apply(conn: &Connection) -> Result<()> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("could not read the vault index's schema version")?;
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version > SCHEMA_VERSION {
        // Written by a newer Shale. Rebuilding would throw away the index that
        // newer Shale expects, so refuse and let the caller say so.
        anyhow::bail!(
            "the vault index is schema {version}, which is newer than this Shale's {SCHEMA_VERSION}"
        );
    }
    if version > 0 {
        for table in SUPERSEDED_TABLES {
            // The names are this crate's constants, never anything read from a file
            // or typed by a user, so there is nothing to parameterise.
            conn.execute_batch(&format!("DROP TABLE IF EXISTS {table};"))
                .with_context(|| format!("could not drop the superseded {table} table"))?;
        }
    }
    conn.execute_batch(SCHEMA_V2)
        .context("could not create the vault index's tables")?;
    // `PRAGMA` takes no bound parameters, and the value is this crate's own
    // constant rather than anything read from the file.
    conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))
        .context("could not stamp the vault index's schema version")?;
    Ok(())
}
