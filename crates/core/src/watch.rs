//! Noticing that the vault changed while Shale was not looking.
//!
//! The cheap half of file watching: [`Vault::signature`] walks the vault once and
//! stamps every path it finds without reading a byte of any note, and
//! [`VaultChanges::between`] turns two signatures into what actually happened.
//! That makes the check cheap enough to run on a timer, and it is the same
//! "what changed on disk" pass the planned vault database will need to decide
//! which notes to re-index.

use std::collections::BTreeMap;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};

/// What one path looked like when a signature was taken: enough to notice a
/// change without reading the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stamp {
    /// A file. Its size and modification time are where a change shows up.
    File {
        /// Size in bytes.
        len: u64,
        /// Last modification time. A filesystem that cannot report one is stamped
        /// with the epoch, which leaves the size as the only signal.
        mtime: SystemTime,
    },
    /// A directory. Only its existence is tracked: adding a child changes the
    /// folder's own mtime, but the child is in the signature too, so treating the
    /// folder as modified would report every edit twice.
    Dir,
}

impl Stamp {
    /// The stamp for a file the walk has already found.
    pub(crate) fn for_file(entry: &DirEntry) -> Result<Self> {
        let meta = entry
            .metadata()
            .with_context(|| format!("stating {}", entry.path().display()))?;
        Ok(Stamp::File {
            len: meta.len(),
            mtime: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        })
    }
}

/// What changed between two signatures of the same vault. Every list is in
/// vault-relative path order, because a signature is a [`BTreeMap`].
#[derive(Debug, Default, Clone)]
pub struct VaultChanges {
    /// Paths that exist now and did not before.
    pub added: Vec<PathBuf>,
    /// Paths that are gone.
    pub removed: Vec<PathBuf>,
    /// Paths that exist in both but are not the same file any more: a note whose
    /// contents or size changed.
    pub modified: Vec<PathBuf>,
}

impl VaultChanges {
    /// The difference between the vault as it was (`before`) and as it is now
    /// (`after`).
    pub fn between(before: &BTreeMap<PathBuf, Stamp>, after: &BTreeMap<PathBuf, Stamp>) -> Self {
        let mut changes = Self::default();
        for (path, stamp) in after {
            match before.get(path) {
                None => changes.added.push(path.clone()),
                Some(old) if old != stamp && matches!(stamp, Stamp::File { .. }) => {
                    changes.modified.push(path.clone());
                }
                Some(_) => {}
            }
        }
        for path in before.keys() {
            if !after.contains_key(path) {
                changes.removed.push(path.clone());
            }
        }
        changes
    }

    /// Whether nothing happened, which is the overwhelmingly common answer.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }

    /// Whether `path` is one of the paths that vanished.
    pub fn was_removed(&self, path: &Path) -> bool {
        self.removed.iter().any(|p| p == path)
    }

    /// Whether `path` is one of the paths whose contents changed.
    pub fn was_modified(&self, path: &Path) -> bool {
        self.modified.iter().any(|p| p == path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::vault::Vault;

    fn fixture() -> Result<(tempfile::TempDir, Vault)> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("notes"))?;
        fs::create_dir_all(root.join("assets"))?;
        fs::create_dir_all(root.join(".obsidian"))?;
        fs::write(root.join("Welcome.md"), "one")?;
        fs::write(root.join("notes/Another Note.md"), "two")?;
        fs::write(root.join("assets/Diagram.png"), "not really a png")?;
        fs::write(root.join(".obsidian/config.json"), "{}")?;
        Ok((dir, Vault::open(root)?))
    }

    #[test]
    fn a_signature_covers_notes_attachments_and_folders() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let sig = vault.signature()?;
        assert!(matches!(
            sig.get(Path::new("Welcome.md")),
            Some(Stamp::File { len: 3, .. })
        ));
        assert!(matches!(
            sig.get(Path::new("assets/Diagram.png")),
            Some(Stamp::File { .. })
        ));
        assert_eq!(sig.get(Path::new("notes")), Some(&Stamp::Dir));
        // Dot directories are not vault content, so they are not watched either.
        assert!(!sig.contains_key(Path::new(".obsidian")));
        Ok(())
    }

    #[test]
    fn nothing_changed_is_reported_as_nothing() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let before = vault.signature()?;
        let after = vault.signature()?;
        assert!(VaultChanges::between(&before, &after).is_empty());
        Ok(())
    }

    #[test]
    fn an_edit_is_a_modification_and_a_new_note_is_an_addition() -> Result<()> {
        let (dir, vault) = fixture()?;
        let before = vault.signature()?;
        fs::write(dir.path().join("Welcome.md"), "one two three")?;
        fs::write(dir.path().join("notes/New.md"), "fresh")?;
        let changes = VaultChanges::between(&before, &vault.signature()?);
        assert_eq!(changes.added, vec![PathBuf::from("notes/New.md")]);
        assert_eq!(changes.modified, vec![PathBuf::from("Welcome.md")]);
        assert!(changes.removed.is_empty());
        assert!(changes.was_modified(Path::new("Welcome.md")));
        Ok(())
    }

    #[test]
    fn a_delete_is_a_removal() -> Result<()> {
        let (dir, vault) = fixture()?;
        let before = vault.signature()?;
        fs::remove_file(dir.path().join("notes/Another Note.md"))?;
        let changes = VaultChanges::between(&before, &vault.signature()?);
        assert_eq!(
            changes.removed,
            vec![PathBuf::from("notes/Another Note.md")]
        );
        assert!(changes.added.is_empty() && changes.modified.is_empty());
        assert!(changes.was_removed(Path::new("notes/Another Note.md")));
        Ok(())
    }

    #[test]
    fn a_rename_is_a_removal_and_an_addition() -> Result<()> {
        let (dir, vault) = fixture()?;
        let before = vault.signature()?;
        fs::rename(
            dir.path().join("Welcome.md"),
            dir.path().join("Welcome Moved.md"),
        )?;
        let changes = VaultChanges::between(&before, &vault.signature()?);
        assert_eq!(changes.added, vec![PathBuf::from("Welcome Moved.md")]);
        assert_eq!(changes.removed, vec![PathBuf::from("Welcome.md")]);
        assert!(changes.modified.is_empty());
        Ok(())
    }

    #[test]
    fn a_folder_whose_children_changed_is_not_itself_modified() -> Result<()> {
        // Adding a note rewrites the folder's own mtime, but a folder's stamp is
        // its existence, so the note is the only thing reported.
        let (dir, vault) = fixture()?;
        let before = vault.signature()?;
        fs::write(dir.path().join("notes/Deep.md"), "x")?;
        let changes = VaultChanges::between(&before, &vault.signature()?);
        assert_eq!(changes.added, vec![PathBuf::from("notes/Deep.md")]);
        assert!(changes.modified.is_empty());
        Ok(())
    }
}
