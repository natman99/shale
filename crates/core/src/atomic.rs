//! Writing a file so a crash cannot leave it half-written.
//!
//! Every file Shale owns goes through here: the notes (which are the source of
//! truth, so losing one to a bad write is the worst failure the editor has), and
//! Shale's own state under `.shale`. Roadmap P1.5's rule — "all writes atomic
//! (temp + rename)" — is this module; the DB gets the same guarantee from SQLite's
//! transactions instead.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context as _, Result};

/// The suffix on the temp file every atomic write goes through.
///
/// Dot-prefixed on purpose: [`crate::Vault`]'s walk skips anything whose name starts
/// with a dot, so a temp file left behind by a crash is invisible to the file tree
/// and to the index rather than turning into a note called `.Welcome.md.shale-tmp`.
const TMP_SUFFIX: &str = ".shale-tmp";

/// The temp path a write to `path` stages through: the same directory, so the
/// rename at the end cannot cross a filesystem and cannot be anything but a rename.
///
/// Also what the DB backup (roadmap P1.6) stages through: the convention, not just
/// the text writer, is the thing worth sharing.
pub(crate) fn staging_path(path: &Path) -> std::path::PathBuf {
    let mut staged = std::ffi::OsString::from(".");
    if let Some(name) = path.file_name() {
        staged.push(name);
    } else {
        staged.push("shale");
    }
    staged.push(TMP_SUFFIX);
    path.with_file_name(staged)
}

/// Write `text` to `path` atomically.
///
/// Three steps, and the order is the whole point:
///
/// 1. write a temp file in the same directory;
/// 2. `sync_all` it, so the bytes are on the disk before anything points at them;
/// 3. `rename` it over the target, which POSIX guarantees is atomic — every other
///    process sees either the whole old file or the whole new one, never a
///    truncated middle.
///
/// A plain `fs::write` truncates the target first, so a crash, a signal, or an
/// ENOSPC part-way through leaves the note empty or half-written. The sync is what
/// makes the write survive a power cut; the rename is what makes it survive the
/// process being killed. (The *directory* entry is not synced: that is the one
/// durability step whose cost is disproportionate, and after a power cut the worst
/// case is that the rename itself is not yet recorded, not that a note is corrupt.)
///
/// Two things the rename would get wrong if left to itself, both fixed here:
/// a `chmod 444` note is a user saying *do not touch*, and `rename` consults the
/// directory rather than the file, so the refusal has to be explicit; and the
/// replacement file would otherwise start at the default permissions, turning a
/// private `0600` note world-readable by being edited. The old mode is copied onto
/// the staging file.
///
/// A temp file left behind by a failed step is removed; one left behind by a hard
/// kill is invisible to the vault either way.
pub(crate) fn write_file(path: &Path, text: &str) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    let existing = path.metadata().ok();
    if let Some(meta) = &existing {
        // `readonly` is a heuristic — the real answer is the write itself — but the
        // rename path cannot learn the answer any other way, and this is the same
        // answer a plain `fs::write` would have effectively given.
        if meta.permissions().readonly() {
            anyhow::bail!("{} is read-only", path.display());
        }
    }

    let staged = staging_path(path);

    let outcome = (|| -> Result<()> {
        let mut file =
            File::create(&staged).with_context(|| format!("creating {}", staged.display()))?;
        #[cfg(unix)]
        if let Some(meta) = &existing {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(
                &staged,
                fs::Permissions::from_mode(meta.permissions().mode()),
            )
            .with_context(|| format!("keeping the permissions of {}", path.display()))?;
        }
        file.write_all(text.as_bytes())
            .with_context(|| format!("writing {}", staged.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing {}", staged.display()))?;
        drop(file);
        fs::rename(&staged, path)
            .with_context(|| format!("renaming {} to {}", staged.display(), path.display()))?;
        Ok(())
    })();
    if outcome.is_err() {
        // Best effort: the note still holds what it held, and the half-written
        // staging file should not outlive the attempt that failed.
        let _ = fs::remove_file(&staged);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_write_replaces_the_note_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        write_file(&path, "first thought").unwrap();
        write_file(&path, "second thought").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second thought");

        // The staging file is renamed away, not left beside the note; and the
        // failure cleanup did not leave one either.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            leftovers,
            vec![std::ffi::OsString::from("a.md")],
            "one write, one file"
        );
    }

    #[test]
    fn a_crash_leftover_is_invisible_to_the_vault() -> Result<()> {
        // The other half of the story: a hard kill between the write and the rename
        // *can* leave a staging file on disk, and the dot prefix is what makes it
        // harmless rather than a note called `.a.md.shale-tmp`.
        let dir = tempfile::tempdir().unwrap();
        let vault = crate::Vault::open(dir.path())?;
        vault.write("a.md", "hello")?;
        fs::write(dir.path().join(".a.md.shale-tmp"), "half a thought")?;

        let names: Vec<_> = vault
            .list_markdown()?
            .iter()
            .map(|entry| entry.path.clone())
            .collect();
        assert_eq!(names, [PathBuf::from("a.md")]);
        assert!(
            !vault
                .signature()?
                .contains_key(Path::new(".a.md.shale-tmp")),
            "the index never sees the leftover, either"
        );
        Ok(())
    }

    /// The rename consults the directory, not the file, so without the explicit
    /// check a `chmod 444` note — a user saying *do not touch* — would be replaced
    /// anyway. The app's refuse-to-close path depends on this refusal.
    #[cfg(unix)]
    #[test]
    fn a_read_only_note_refuses_the_write_and_keeps_its_text() -> Result<()> {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.md");
        write_file(&path, "untouchable").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;

        let err = write_file(&path, "over the lock").unwrap_err();
        assert!(err.to_string().contains("read-only"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "untouchable");
        Ok(())
    }

    /// The replacement file is a new inode, so its mode comes from the default
    /// unless the old one is copied onto it: editing a private note must not make
    /// it world-readable.
    #[cfg(unix)]
    #[test]
    fn a_write_keeps_the_notes_permissions() -> Result<()> {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private.md");
        write_file(&path, "journal").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;

        write_file(&path, "journal, continued").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "the edit survived, the lock did not");
        Ok(())
    }
}
