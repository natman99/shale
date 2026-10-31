//! The vault index: a SQLite cache derived from the notes.
//!
//! The notes are the source of truth and always will be (PROJECT.md, Database).
//! Everything here can be deleted and rebuilt, which is why nothing in the editor
//! is allowed to *require* the index: a missing, locked or corrupt DB is a missing
//! feature, reported and carried on without, exactly like a `harper-ls` that will
//! not start.
//!
//! ## Who is allowed to write
//!
//! One thread — the one [`Db::open`] starts — owns the write connection. Everyone
//! else (the UI thread, the MCP handlers, the startup reconciliation thread) sends
//! a closure over a channel and the writer runs it. That is the binding rule in
//! PROJECT.md, and the reason it is a rule: SQLite allows one writer at a time, and
//! the usual fixes for that (retry loops, one connection per thread sharing a
//! mutex, a connection per call) each trade a clear failure for a mysterious one.
//!
//! Readers are separate connections. WAL mode is what lets them read while the
//! writer holds the write lock, so a query from the UI never waits on an index
//! update and vice versa.
//!
//! A job runs inside one transaction, so a multi-statement update is atomic
//! without the caller saying so. Transactions are meant to be short: the writer is
//! the only one, and holding it across file I/O or parsing would stall every
//! reader that needs a fresh snapshot.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use rusqlite::Connection;

mod backup;
mod index;
mod schema;
#[cfg(test)]
mod tests;

pub use index::{target_kind, word_count, EventKind, SyncReport};
pub use schema::SCHEMA_VERSION;

/// The directory Shale keeps its per-vault state in: the config, the themes, the
/// session file and this index.
pub const STATE_DIR: &str = ".shale";

/// The index's file inside [`STATE_DIR`].
pub const DB_FILE: &str = "shale.db";

/// How long a connection waits for the write lock before giving up. Long enough
/// that a queued burst of index writes is never seen as a failure by a reader that
/// opened a write connection of its own, short enough that a genuinely stuck
/// writer is an error rather than a hang.
const BUSY_TIMEOUT_MS: u64 = 4000;

/// One unit of write work, run by the writer thread against its own connection.
type Job = Box<dyn FnOnce(&Connection) -> Result<()> + Send + 'static>;

/// A job on its way to the writer thread, with somewhere to put the outcome.
struct Request {
    job: Job,
    /// `Some` when the caller is waiting for the result, `None` for work that is
    /// fire-and-forget (the reconciliation pass, an event log row).
    reply: Option<Sender<Result<()>>>,
}

/// Who wrote a note, which every write path stamps and the events table keeps.
///
/// The vocabulary is deliberately closed and stringly on the way in: it is a column
/// a human reads in the scrubber and a query filters on, not a type the editor
/// branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// The user, through Shale.
    User,
    /// The standalone `shale-mcp` server, or the app's own MCP handlers.
    Mcp,
    /// A rename, which touches every note that linked to the old name.
    Rename,
    /// A template or capture, which creates rather than edits.
    Template,
    /// Another program, noticed by the vault watcher.
    External,
}

impl Provenance {
    /// The word stored in the DB and shown in the scrubber.
    pub fn as_str(self) -> &'static str {
        match self {
            Provenance::User => "user",
            Provenance::Mcp => "mcp",
            Provenance::Rename => "rename",
            Provenance::Template => "template",
            Provenance::External => "external",
        }
    }
}

/// The instant a row is stamped with: whole seconds, shared by every writer so two
/// events in the same second are ordered by rowid rather than by a clock that was
/// never asked to be finer.
pub fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Open handle to the vault's index.
#[derive(Debug)]
pub struct Db {
    /// Where the DB file is, so a reader can open another connection to it.
    path: PathBuf,
    /// The queue's sending end. `None` once the barrier has run: closing it is
    /// what ends the writer thread.
    tx: Option<Sender<Request>>,
    /// The writer thread's failure, once there is one. Every later write reports
    /// it instead of queueing work nobody will do.
    failure: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl Db {
    /// Open (creating if needed) `<vault>/.shale/shale.db` and start the writer.
    ///
    /// The schema is applied on the calling thread, before the writer starts, so a
    /// schema this build cannot handle is an error from `open` rather than
    /// something the queue discovers later.
    pub fn open(vault_root: &Path) -> Result<Db> {
        let dir = vault_root.join(STATE_DIR);
        fs::create_dir_all(&dir).with_context(|| {
            format!(
                "could not create the vault's state directory {}",
                dir.display()
            )
        })?;
        let path = dir.join(DB_FILE);
        {
            let conn = connect(&path)?;
            schema::apply(&conn)?;
        }

        let (tx, rx) = mpsc::channel::<Request>();
        let failure = Arc::new(Mutex::new(None));
        let writer_failure = Arc::clone(&failure);
        let writer_path = path.clone();
        let thread = std::thread::Builder::new()
            .name("shale-db-writer".to_string())
            .spawn(move || writer_loop(&writer_path, rx, &writer_failure))
            .context("could not start the vault index's writer thread")?;
        Ok(Db {
            path,
            tx: Some(tx),
            failure,
            thread: Some(thread),
        })
    }

    /// Where the DB file is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The schema version stamped in the file. Asked by anything that has to decide
    /// whether an existing index is the one it expects — and by the tests that
    /// check a reopen did not rebuild one.
    pub fn schema_version(&self) -> i64 {
        // Best effort on purpose: a DB that cannot answer this is a DB whose next
        // write will report the problem anyway.
        self.read()
            .and_then(|conn| {
                conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                    .context("could not read the vault index's schema version")
            })
            .unwrap_or(0)
    }

    /// Send one write job and wait for it to run.
    ///
    /// The wait is the point: a caller that asks for a result is a caller whose
    /// next step depends on the row being there. Bulk work that nothing waits on
    /// should use [`Db::queue`] instead.
    pub fn write(
        &self,
        job: impl FnOnce(&Connection) -> Result<()> + Send + 'static,
    ) -> Result<()> {
        let (reply, reply_rx) = mpsc::channel();
        self.send(Request {
            job: Box::new(job),
            reply: Some(reply),
        })?;
        reply_rx
            .recv()
            .context("the vault index's writer thread went away before answering")?
    }

    /// Send one write job without waiting for it.
    ///
    /// A failure is remembered on the handle and reported by the next write or by
    /// [`Db::shutdown`], which is what lets the reconciliation pass keep feeding
    /// rows without a thread per row checking on the last one.
    pub fn queue(
        &self,
        job: impl FnOnce(&Connection) -> Result<()> + Send + 'static,
    ) -> Result<()> {
        self.send(Request {
            job: Box::new(job),
            reply: None,
        })
    }

    /// A connection for reading.
    ///
    /// Marked `query_only`, so a reader cannot become the second writer by
    /// accident. WAL means this connection reads the snapshot that was current when
    /// its read transaction starts, which is why readers are cheap and never wait
    /// on the writer.
    pub fn read(&self) -> Result<Connection> {
        let conn = connect(&self.path)?;
        conn.execute_batch("PRAGMA query_only = ON;")
            .context("could not open the vault index for reading")?;
        Ok(conn)
    }

    /// Drain what is queued and checkpoint the WAL, so the index is one file again.
    ///
    /// The queue is FIFO, so a job that only checkpoints runs after every job that
    /// was queued before it: that is the drain, without the handle having to know
    /// what is in the queue. Roadmap P1.5 asks for it at shutdown, and it is worth
    /// asking for at any clean exit, because an index with no `-wal` beside it is
    /// what P1.6's backup copies and what `PRAGMA integrity_check` inspects.
    ///
    /// An index whose writer already failed reports that rather than pretending it
    /// is current.
    pub fn flush(&self) -> Result<()> {
        self.write(|conn| {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .context("could not checkpoint the vault index")?;
            Ok(())
        })
    }

    /// The barrier roadmap P1.5 asks for: stop accepting work, let what is already
    /// queued finish, and report anything the writer failed on.
    pub fn shutdown(mut self) -> Result<()> {
        // Dropping the sender is what ends the loop: `recv` returns `Err` once the
        // queue is drained, so nothing queued is lost.
        self.tx = None;
        let thread = self.thread.take();
        if let Some(thread) = thread {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("the vault index's writer thread panicked"))?;
        }
        if let Some(failure) = self.failure.lock().unwrap().take() {
            anyhow::bail!("the vault index failed: {failure}");
        }
        Ok(())
    }

    /// The one place a request leaves a thread.
    fn send(&self, request: Request) -> Result<()> {
        if let Some(failure) = self.failure.lock().unwrap().as_ref() {
            anyhow::bail!("the vault index's writer thread failed: {failure}");
        }
        let tx = self.tx.as_ref().context("the vault index is closed")?;
        // `SendError`'s own message would name the payload type, which is this
        // module's private business; what a caller needs is that the writer is gone.
        tx.send(request)
            .map_err(|_| anyhow::anyhow!("the vault index's writer thread is gone"))
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        // A caller that never asked for the barrier still must not leak the writer
        // thread, and closing its connection is what checkpoints the WAL.
        self.tx = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Open a connection and set the pragmas every connection needs.
///
/// `journal_mode` is persistent — it is stored in the DB file — but setting it
/// again is free and means a connection never assumes the file is still in the
/// mode the last one left it. `synchronous = NORMAL` is the usual WAL trade: a
/// power loss can lose the last few index rows, and the index can be rebuilt.
fn connect(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("could not open the vault index {}", path.display()))?;
    conn.execute_batch(&format!(
        "PRAGMA journal_mode = WAL; PRAGMA busy_timeout = {BUSY_TIMEOUT_MS}; PRAGMA synchronous = NORMAL;"
    ))
    .with_context(|| format!("could not set the vault index's pragmas on {}", path.display()))?;
    Ok(conn)
}

/// The writer thread: one connection, one job at a time, until the queue closes.
fn writer_loop(path: &Path, rx: Receiver<Request>, failure: &Mutex<Option<String>>) {
    let mut conn = match connect(path) {
        Ok(conn) => conn,
        Err(err) => {
            record(failure, &format!("{err:#}"));
            return;
        }
    };
    while let Ok(request) = rx.recv() {
        let Request { job, reply } = request;
        let outcome = run(&mut conn, job);
        if let Err(err) = &outcome {
            record(failure, &format!("{err:#}"));
        }
        if let Some(reply) = reply {
            // The caller may have gone away; that is not a write failure.
            let _ = reply.send(outcome);
        }
    }
}

/// Run one job inside one transaction.
fn run(conn: &mut Connection, job: Job) -> Result<()> {
    conn.execute_batch("BEGIN")
        .context("could not start a vault index transaction")?;
    match job(&*conn) {
        Ok(()) => {
            conn.execute_batch("COMMIT")
                .context("could not commit a vault index transaction")?;
            Ok(())
        }
        Err(err) => {
            // The rollback is best effort: if it fails too, the job's own error is
            // the one worth reporting.
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// Remember the first failure, which is the one that explains the rest.
fn record(failure: &Mutex<Option<String>>, message: &str) {
    let mut slot = failure.lock().unwrap();
    if slot.is_none() {
        *slot = Some(message.to_string());
    }
}
