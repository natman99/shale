//! The index's own contract: where the file goes, what shape it has, and the
//! single-writer rule that everything downstream depends on.

use std::sync::mpsc;
use std::thread;

use super::*;

/// A vault in a temp directory, and its index. The `TempDir` is returned so the
/// caller keeps it alive for as long as the DB is open.
fn opened() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    (dir, db)
}

/// The tables roadmap P1.2 lists, in the order the check reads them.
const TABLES: [&str; 6] = ["notes", "tags", "links", "events", "marks", "edges"];

#[test]
fn opening_a_vault_puts_the_index_in_its_state_directory() {
    let (dir, db) = opened();
    assert_eq!(
        db.path(),
        dir.path().join(".shale").join("shale.db"),
        "the DB lives with the config, not in the vault's root"
    );
    assert!(db.path().exists(), "and it is really there");
}

/// A reader writing a query by hand (the inline-SQL feature) needs the names to be
/// the ones the roadmap promises.
#[test]
fn every_table_the_roadmap_promises_is_there() {
    let (_dir, db) = opened();
    let conn = db.read().unwrap();
    for table in TABLES {
        let found: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "{table}");
    }
}

#[test]
fn the_index_opens_in_wal_mode() {
    // WAL is what lets the standalone `shale-mcp` and the app's readers work while
    // the writer thread holds the write lock, so it is a requirement rather than a
    // tuning choice.
    let (_dir, db) = opened();
    let conn = db.read().unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

/// Reopening must not lose the index or rebuild it, and must not stamp a version
/// twice.
#[test]
fn reopening_finds_the_same_index_at_the_same_version() {
    let (dir, db) = opened();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO notes (path, created_at, modified_at) VALUES (?1, 10, 20)",
            ["a.md"],
        )?;
        Ok(())
    })
    .unwrap();
    let version = db.schema_version();
    assert_eq!(version, SCHEMA_VERSION);
    drop(db);

    let again = Db::open(dir.path()).unwrap();
    assert_eq!(again.schema_version(), SCHEMA_VERSION);
    let conn = again.read().unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1, "the rows survived the reopen");
}

/// The whole point of the writer thread: two callers, one connection.
#[test]
fn every_write_runs_on_the_one_writer_thread() {
    let (_dir, db) = opened();
    let mut ids = Vec::new();
    for _ in 0..3 {
        // The caller's own thread id, taken inside the job, is the writer's.
        let (reply, reply_rx) = mpsc::channel();
        db.queue(move |_conn| {
            let _ = reply.send(thread::current().id());
            Ok(())
        })
        .unwrap();
        ids.push(reply_rx.recv().unwrap());
    }
    let outside = thread::current().id();
    assert!(
        ids.windows(2).all(|pair| pair[0] == pair[1]),
        "three writes, one thread: {ids:?}"
    );
    assert!(!ids.contains(&outside), "and it is not the caller's thread");
}

/// A write that returns is a write a reader can already see — and a reader is not
/// allowed to become a second writer.
#[test]
fn a_reader_sees_committed_work_and_cannot_write() {
    let (_dir, db) = opened();
    db.write(|conn| {
        conn.execute(
            "INSERT INTO tags (note, tag) VALUES (?1, ?2)",
            ["a.md", "reading"],
        )?;
        Ok(())
    })
    .unwrap();

    let conn = db.read().unwrap();
    let tag: String = conn
        .query_row("SELECT tag FROM tags WHERE note = 'a.md'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(tag, "reading");

    let err = conn
        .execute("INSERT INTO tags (note, tag) VALUES ('b.md', 'nope')", [])
        .unwrap_err();
    assert!(
        err.to_string().contains("readonly database"),
        "the read connection refuses to write: {err}"
    );
}

/// A job that fails partway leaves nothing behind, because the writer wraps it in
/// one transaction.
#[test]
fn a_failed_job_leaves_no_rows_and_says_so() {
    let (_dir, db) = opened();
    let outcome = db.write(|conn| {
        conn.execute(
            "INSERT INTO notes (path, created_at, modified_at) VALUES ('gone.md', 1, 2)",
            [],
        )?;
        anyhow::bail!("the job decided to fail");
    });
    assert!(outcome.is_err());

    let conn = db.read().unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0, "the transaction rolled back with the job");

    // And the failure is not forgotten: the next write refuses rather than
    // queueing work into a broken connection.
    let next = db.write(|_| Ok(()));
    assert!(next.is_err(), "a writer that has failed stops taking work");
}

/// Fire-and-forget work still has to surface its failure, and the barrier is where
/// it surfaces.
#[test]
fn the_barrier_reports_what_the_writer_failed_on() {
    let (_dir, db) = opened();
    db.queue(|_| anyhow::bail!("a queued job failed")).unwrap();
    // The queue does not wait, so the barrier is the answer.
    let err = db.shutdown().unwrap_err();
    assert!(err.to_string().contains("a queued job failed"), "{err}");
}

/// A DB written by a newer Shale is refused out loud. Deleting the file is always
/// a valid repair, but silently rebuilding would throw away the index that newer
/// Shale is expecting to find.
#[test]
fn a_newer_schema_is_refused_rather_than_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = Db::open(dir.path()).unwrap();
        db.write(|conn| {
            conn.execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION + 7))
                .context("could not stamp a newer version")
        })
        .unwrap();
    }
    let err = Db::open(dir.path()).unwrap_err();
    assert!(err.to_string().contains("newer than this Shale"), "{err}");
}

/// The barrier is not optional bookkeeping: dropping the handle must not leave a
/// thread running against a temp directory that is about to disappear.
#[test]
fn dropping_the_handle_ends_the_writer_thread() {
    let (dir, db) = opened();
    let path = dir.path().to_path_buf();
    drop(db);
    // The file is still there and still a valid index — the writer closed its
    // connection rather than being killed with one open.
    let conn = Connection::open(path.join(".shale").join("shale.db")).unwrap();
    let count: i64 = conn
        .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
