//! The daily backup of the vault index (roadmap P1.6).
//!
//! The index is a cache, so a backup is insurance against a slow corruption — a
//! bad page creeping in and being *read* into the graph — rather than against
//! losing anything irreplaceable: the notes rebuild a deleted index in a second.
//! One copy per day in `.shale/backups/`, the newest [`KEEP`] kept.
//!
//! The copy is made by SQLite's online backup API, as a job on the writer
//! thread. A plain file copy would race the checkpoint — the main DB file is
//! rewritten piecemeal when the WAL is folded in — and a torn copy is worse than
//! no copy, because pruning would count it as one of seven. The backup API reads
//! a live database page by page and hands back a whole, consistent single file;
//! running it as a *writer* job is what makes "live" safe here: the writer is
//! the only writer, so nothing can change the source mid-copy. That is also the
//! one place the single-writer rule tolerates file I/O — it is the exception
//! because it is the only way to copy a database another thread owns.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result};
use rusqlite::backup::Backup;
use rusqlite::Connection;

use super::{now_epoch, Db};
use crate::atomic;

/// How many daily copies to keep. Seven: a week of mornings — enough to notice a
/// corruption days after it crept in, few enough that the copies (a few
/// megabytes each, of a cache) are never a second vault.
const KEEP: usize = 7;

/// The name of a day's backup: `shale-2026-10-31.db`. The date is ISO, so
/// sorting the names sorts them chronologically — which is what [`prune`] relies
/// on.
const PREFIX: &str = "shale-";
const SUFFIX: &str = ".db";

/// Pages per `sqlite3_backup_step`; the whole copy is a handful of megabytes at
/// most, so the step size only bounds how long the writer is held per slice.
const PAGES_PER_STEP: i32 = 100;

/// The directory the backups live in, beside the DB: `<vault>/.shale/backups/`.
fn backups_dir(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or(Path::new(".")).join("backups")
}

/// The day stamp a backup is named by, `YYYY-MM-DD`, from epoch seconds.
///
/// UTC on purpose: the rotation should not move when the machine's timezone
/// does, and the day the file names is the day the copy was taken. The
/// conversion is Howard Hinnant's `civil_from_days` — the days-from-civil
/// algorithm run backwards — because a date library is not wanted for one
/// format, and every value here is non-negative (pre-epoch clocks are not a
/// case worth branching for).
fn civil_date(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400) + 719_468; // days since 0000-03-01
    let era = days.div_euclid(146_097); // 400-year eras
    let doe = days.rem_euclid(146_097); // day of era
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of era-relative year
    let mp = (5 * doy + 2) / 153; // month counting from March
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Keep the newest [`KEEP`] daily copies and delete the rest. The names are ISO
/// dates, so the oldest are simply the first in sorted order.
fn prune(dir: &Path) -> Result<()> {
    let mut days: Vec<String> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(PREFIX) && name.ends_with(SUFFIX))
        .collect();
    days.sort();
    if days.len() <= KEEP {
        return Ok(());
    }
    for name in &days[..days.len() - KEEP] {
        fs::remove_file(dir.join(name))
            .with_context(|| format!("removing the old backup {name}"))?;
    }
    Ok(())
}

impl Db {
    /// Take today's backup if it has not been taken yet, and rotate.
    ///
    /// `Ok(None)` means today's copy already exists and nothing was written —
    /// the answer the hourly poll gives all day except at the day boundary.
    ///
    /// The whole copy is one job on the writer thread (module docs): the source
    /// cannot change mid-copy, and the destination is staged under the same
    /// dot-prefixed temp name every other atomic write in Shale uses, so a kill
    /// between the copy and the rename leaves an invisible file rather than a
    /// half-written backup that pruning would count as one of seven.
    pub fn backup(&self) -> Result<Option<PathBuf>> {
        let dir = backups_dir(&self.path);
        let day = civil_date(now_epoch());
        let dest = dir.join(format!("{PREFIX}{day}{SUFFIX}"));
        if dest.exists() {
            return Ok(None);
        }
        let staged = atomic::staging_path(&dest);

        // The job runs on the writer thread, so the paths it names move into it;
        // the caller keeps its own copy to hand back.
        let job_dir = dir.clone();
        let job_staged = staged.clone();
        let job_dest = dest.clone();
        self.write(move |conn| {
            fs::create_dir_all(&job_dir)
                .with_context(|| format!("creating {}", job_dir.display()))?;
            let _ = fs::remove_file(&job_staged);
            let outcome = (|| -> Result<()> {
                let mut out = Connection::open(&job_staged)
                    .with_context(|| format!("creating {}", job_staged.display()))?;
                let copy =
                    Backup::new(conn, &mut out).context("starting the vault index backup")?;
                copy.run_to_completion(PAGES_PER_STEP, Duration::ZERO, None)
                    .with_context(|| {
                        format!("copying the vault index to {}", job_staged.display())
                    })?;
                drop(copy);
                drop(out);
                fs::rename(&job_staged, &job_dest)
                    .with_context(|| format!("renaming the backup to {}", job_dest.display()))?;
                Ok(())
            })();
            if outcome.is_err() {
                let _ = fs::remove_file(&job_staged);
            }
            outcome?;
            prune(&job_dir)
        })?;
        Ok(Some(dest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vault in a temp directory, and its index. The `TempDir` is returned so
    /// the caller keeps it alive for as long as the DB is open.
    fn opened() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        (dir, db)
    }

    /// The day stamps the rotation names files by. Known pairs, including the
    /// two edges the algorithm gets wrong if the March-based year is off: the
    /// January side of a year boundary, and a leap day.
    #[test]
    fn the_day_stamp_is_the_utc_civil_date() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(1_709_251_200), "2024-03-01"); // the March epoch shift
        assert_eq!(civil_date(1_704_067_199), "2023-12-31"); // one second before 2024
        assert_eq!(civil_date(951_782_400), "2000-02-29"); // the leap day
        assert_eq!(civil_date(1_793_404_800), "2026-10-31");
    }

    #[test]
    fn a_backup_is_a_whole_readable_copy_of_the_index() {
        let (dir, db) = opened();
        db.write(|conn| {
            conn.execute(
                "INSERT INTO notes (path, created_at, modified_at) VALUES ('a.md', 10, 20)",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        let path = db
            .backup()
            .unwrap()
            .expect("no copy exists yet, so one is made");
        assert_eq!(
            path.parent().unwrap(),
            dir.path().join(".shale").join("backups"),
            "the copies live beside the DB, not in the vault's root"
        );

        // The copy is a database in its own right: opened by a stranger, it has
        // the rows and the schema the source had.
        let conn = Connection::open(&path).unwrap();
        let path_in_copy: String = conn
            .query_row("SELECT path FROM notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(path_in_copy, "a.md");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, super::super::SCHEMA_VERSION);
    }

    #[test]
    fn the_second_backup_of_the_same_day_is_skipped() {
        let (dir, db) = opened();
        let first = db.backup().unwrap().expect("the first copy is made");
        let second = db.backup().unwrap();
        assert!(
            second.is_none(),
            "today's copy exists, so nothing is written"
        );
        let copies = fs::read_dir(dir.path().join(".shale").join("backups"))
            .unwrap()
            .count();
        assert_eq!(copies, 1, "and no second file appeared: {first:?}");
    }

    /// The rotation keeps seven. The fakes are named by date, which is exactly
    /// what the sort depends on; the two oldest are what go.
    #[test]
    fn the_rotation_keeps_the_newest_seven() {
        let (_dir, db) = opened();
        let backups = backups_dir(db.path());
        fs::create_dir_all(&backups).unwrap();
        for day in ["2026-10-20", "2026-10-21", "2026-10-22", "2026-10-23"] {
            fs::write(backups.join(format!("{PREFIX}{day}{SUFFIX}")), "fake").unwrap();
        }
        // Four fakes plus today's real copy is five — under the cap. Add three
        // more fakes to push it over.
        for day in ["2026-10-24", "2026-10-25", "2026-10-26"] {
            fs::write(backups.join(format!("{PREFIX}{day}{SUFFIX}")), "fake").unwrap();
        }

        let today = db.backup().unwrap().expect("today's copy is the eighth");
        let mut kept: Vec<String> = fs::read_dir(&backups)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        kept.sort();
        assert_eq!(kept.len(), KEEP, "eight candidates, seven kept: {kept:?}");
        assert!(
            kept.contains(&today.file_name().unwrap().to_string_lossy().into_owned()),
            "today's copy is never the one pruned"
        );
        assert!(
            !kept.iter().any(|name| name.ends_with("2026-10-20.db")),
            "the oldest is what goes: {kept:?}"
        );
    }
}
