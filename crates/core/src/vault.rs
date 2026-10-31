use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::watch::Stamp;

/// Directories that should never be treated as vault content.
const IGNORED_DIRS: &[&str] = &[".git", ".obsidian", ".trash", ".shale"];

/// Characters Obsidian refuses in a file or folder name. Keeping them out of the
/// names Shale creates is what makes a vault stay openable in Obsidian.
const FORBIDDEN_NAME_CHARS: &[char] = &['*', '"', '\\', '/', '<', '>', ':', '|', '?'];

/// A markdown note inside a vault, addressed by a vault-relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultEntry {
    /// Vault-relative path, e.g. `notes/idea.md`.
    pub path: PathBuf,
}

impl VaultEntry {
    /// Display name without extension, e.g. `idea`.
    pub fn stem(&self) -> &str {
        self.path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
    }
}

/// An Obsidian-compatible vault rooted at a directory on disk.
#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open the vault rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        if !root.is_dir() {
            bail!("vault root is not a directory: {}", root.display());
        }
        let root = root
            .canonicalize()
            .with_context(|| format!("canonicalizing vault root {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a vault-relative path to an absolute path, rejecting any
    /// path that escapes the vault root.
    pub fn resolve(&self, rel: impl AsRef<Path>) -> Result<PathBuf> {
        let rel = rel.as_ref();
        if rel.is_absolute() {
            bail!("absolute paths are not allowed: {}", rel.display());
        }
        let mut joined = self.root.clone();
        for comp in rel.components() {
            match comp {
                Component::Normal(c) => joined.push(c),
                Component::ParentDir | Component::CurDir => {
                    bail!("path traversal is not allowed: {}", rel.display());
                }
                _ => bail!("invalid path component in: {}", rel.display()),
            }
        }
        Ok(joined)
    }

    /// Convert an absolute path inside the vault to a vault-relative path.
    pub fn relativize(&self, abs: impl AsRef<Path>) -> Result<PathBuf> {
        let abs = abs.as_ref();
        abs.strip_prefix(&self.root)
            .map(Path::to_path_buf)
            .with_context(|| {
                format!(
                    "path {} is outside vault root {}",
                    abs.display(),
                    self.root.display()
                )
            })
    }

    /// All markdown (`.md`) files in the vault, sorted by relative path.
    /// Hidden directories (dot-prefixed) and known non-content directories
    /// are skipped.
    pub fn list_markdown(&self) -> Result<Vec<VaultEntry>> {
        self.walk(|name| name.ends_with(".md"))
    }

    /// Every file in the vault that is not a markdown note — images and other
    /// attachments — sorted by relative path, with the same directories skipped
    /// as for notes.
    pub fn list_attachments(&self) -> Result<Vec<VaultEntry>> {
        self.walk(|name| !name.ends_with(".md"))
    }

    /// Every directory in the vault, sorted by relative path, with the same
    /// directories skipped as for notes. The file tree needs these because a
    /// folder that holds no notes yet is still a folder the user can see and
    /// open.
    pub fn list_dirs(&self) -> Result<Vec<VaultEntry>> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let read = match fs::read_dir(&dir) {
                Ok(read) => read,
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                Err(err) => {
                    return Err(err).with_context(|| format!("reading dir {}", dir.display()));
                }
            };
            for entry in read {
                let entry =
                    entry.with_context(|| format!("reading dir entry in {}", dir.display()))?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                let path = entry.path();
                if entry.file_type()?.is_dir() {
                    out.push(VaultEntry {
                        path: self.relativize(&path)?,
                    });
                    stack.push(path);
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Every file and directory in the vault, keyed by vault-relative path and
    /// stamped with what made it that path. This is the cheap half of file
    /// watching: one walk, no note contents read, so it can run on a timer; and
    /// the same pass the planned vault database will use to decide which notes
    /// have to be re-indexed.
    pub fn signature(&self) -> Result<BTreeMap<PathBuf, Stamp>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let read = match fs::read_dir(&dir) {
                Ok(read) => read,
                // The vault root itself may vanish while we are walking it.
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                Err(err) => {
                    return Err(err).with_context(|| format!("reading dir {}", dir.display()));
                }
            };
            for entry in read {
                let entry =
                    entry.with_context(|| format!("reading dir entry in {}", dir.display()))?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                let file_type = entry.file_type()?;
                let path = entry.path();
                let rel = self.relativize(&path)?;
                if file_type.is_dir() {
                    out.insert(rel, Stamp::Dir);
                    stack.push(path);
                } else if file_type.is_file() {
                    out.insert(rel, Stamp::for_file(&entry)?);
                }
            }
        }
        Ok(out)
    }

    /// The directory walk both listings share: every file whose name `accept`
    /// takes, as a vault-relative path.
    fn walk(&self, accept: impl Fn(&str) -> bool) -> Result<Vec<VaultEntry>> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let read = match fs::read_dir(&dir) {
                Ok(read) => read,
                // Race with deletion: skip directories that vanished.
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                Err(err) => {
                    return Err(err).with_context(|| format!("reading dir {}", dir.display()));
                }
            };
            for entry in read {
                let entry =
                    entry.with_context(|| format!("reading dir entry in {}", dir.display()))?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let file_type = entry.file_type()?;
                if name.starts_with('.') || IGNORED_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                let path = entry.path();
                if file_type.is_dir() {
                    stack.push(path);
                } else if file_type.is_file() && accept(&name) {
                    out.push(VaultEntry {
                        path: self.relativize(path)?,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    pub fn exists(&self, rel: impl AsRef<Path>) -> bool {
        self.resolve(rel).map(|p| p.is_file()).unwrap_or(false)
    }

    pub fn read(&self, rel: impl AsRef<Path>) -> Result<String> {
        let path = self.resolve(rel)?;
        fs::read_to_string(&path).with_context(|| format!("reading note {}", path.display()))
    }

    /// Write `content` to `rel`, creating parent directories as needed.
    /// Replace a note's contents **atomically** ([`crate::atomic`]): a crash, a
    /// signal, or an ENOSPC part-way through leaves the old note standing rather than
    /// an empty or half-written one. The notes are the source of truth, which is why
    /// they do not get to be written any other way.
    pub fn write(&self, rel: impl AsRef<Path>, content: &str) -> Result<()> {
        let path = self.resolve(rel)?;
        crate::atomic::write_file(&path, content)
            .with_context(|| format!("writing note {}", path.display()))
    }

    // ----- creating, renaming and deleting (the file tree's edits) ------

    /// Create an empty note at `rel`, adding `.md` when the name carries no
    /// extension, and return the vault-relative path that was created.
    ///
    /// Refuses to overwrite anything that already exists, so a mistyped name
    /// cannot quietly replace a note.
    pub fn create_note(&self, rel: impl AsRef<Path>) -> Result<PathBuf> {
        let rel = checked_rel(rel.as_ref())?;
        let rel = if rel.extension().is_some() {
            rel
        } else {
            rel.with_extension("md")
        };
        guard_unprotected(&rel)?;
        let path = self.resolve(&rel)?;
        if path.exists() {
            bail!("{} already exists", rel.display());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
        // Written the atomic way like every other note, even though an empty file
        // cannot be half-written: the day this grows a template body to put in the
        // new note, a plain write would be a note that can be lost at birth.
        crate::atomic::write_file(&path, "")
            .with_context(|| format!("creating note {}", path.display()))?;
        Ok(rel)
    }

    /// Create a directory (and any missing parents) inside the vault.
    pub fn create_dir(&self, rel: impl AsRef<Path>) -> Result<()> {
        let rel = checked_rel(rel.as_ref())?;
        guard_unprotected(&rel)?;
        let path = self.resolve(&rel)?;
        fs::create_dir_all(path).with_context(|| format!("creating directory {}", rel.display()))
    }

    /// Move `from` to `to` and return the new vault-relative path.
    ///
    /// The destination is validated as a name and must not exist, so a rename
    /// can never destroy another note; its parent directories are created. The
    /// source is only checked for traversal, because a vault copied in from
    /// elsewhere can hold names Shale itself would never create.
    pub fn rename(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<PathBuf> {
        let from = from.as_ref();
        let src = self.resolve(from)?;
        if !src.exists() {
            bail!("there is nothing at {} to rename", from.display());
        }
        let rel = checked_rel(to.as_ref())?;
        guard_unprotected(&rel)?;
        let dst = self.resolve(&rel)?;
        if dst.exists() {
            bail!("{} already exists", rel.display());
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
        fs::rename(&src, &dst)
            .with_context(|| format!("renaming {} to {}", src.display(), dst.display()))?;
        Ok(rel)
    }

    /// Delete one note. A directory is refused: use [`Self::delete_dir`].
    pub fn delete(&self, rel: impl AsRef<Path>) -> Result<()> {
        let rel = rel.as_ref();
        guard_unprotected(rel)?;
        let path = self.resolve(rel)?;
        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("looking up {}", path.display()))?;
        if meta.is_dir() {
            bail!("{} is a folder, not a note", rel.display());
        }
        fs::remove_file(&path).with_context(|| format!("deleting {}", path.display()))
    }

    /// Delete a folder, refusing one that still holds anything. That guard is what
    /// keeps a stray keypress from taking a whole branch of the vault with it.
    pub fn delete_dir(&self, rel: impl AsRef<Path>) -> Result<()> {
        let rel = rel.as_ref();
        guard_unprotected(rel)?;
        let path = self.resolve(rel)?;
        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("looking up {}", path.display()))?;
        if !meta.is_dir() {
            bail!("{} is not a folder", rel.display());
        }
        if fs::read_dir(&path)?.next().is_some() {
            bail!("{} is not empty", rel.display());
        }
        fs::remove_dir(&path).with_context(|| format!("deleting folder {}", path.display()))
    }

    /// Find the vault entry a wiki-link target refers to, using Obsidian's
    /// shortest-unique-name resolution rules:
    /// 1. exact relative-path match (with or without `.md`),
    /// 2. unique case-insensitive match on the file stem.
    ///
    /// `target` is a raw wiki-link target (no alias, no heading anchor).
    pub fn resolve_link(&self, target: &str, entries: &[VaultEntry]) -> Result<Option<VaultEntry>> {
        let target = target.trim();
        if target.is_empty() {
            return Ok(None);
        }

        // 1. Exact path match.
        let as_path = PathBuf::from(target);
        for entry in entries {
            if entry.path == as_path {
                return Ok(Some(entry.clone()));
            }
            if entry.path == as_path.with_extension("md") {
                return Ok(Some(entry.clone()));
            }
        }

        // 2. Case-insensitive stem match; ambiguous matches resolve to the
        // shortest path, matching Obsidian's behavior.
        let lower = target.to_lowercase();
        let mut best: Option<&VaultEntry> = None;
        for entry in entries {
            if entry.stem().to_lowercase() == lower {
                match best {
                    Some(b) if b.path.components().count() <= entry.path.components().count() => {}
                    _ => best = Some(entry),
                }
            }
        }
        Ok(best.cloned())
    }

    /// Find the attachment an embed target refers to (`![[diagram.png]]`), with
    /// the same shortest-unique-name rules as [`Self::resolve_link`]:
    /// 1. exact relative-path match,
    /// 2. case-insensitive relative-path match,
    /// 3. case-insensitive match on the file name (`diagram.png`) or the stem
    ///    (`diagram`), the shortest path winning a tie.
    ///
    /// Unlike a note link, an attachment keeps its extension, so both a bare
    /// stem and a full file name are worth trying: Obsidian accepts either.
    pub fn resolve_attachment(&self, target: &str, entries: &[VaultEntry]) -> Option<VaultEntry> {
        let target = target.trim();
        if target.is_empty() {
            return None;
        }
        let as_path = PathBuf::from(target);
        let lower_path = target.to_lowercase().replace('\\', "/");
        for entry in entries {
            if entry.path == as_path {
                return Some(entry.clone());
            }
            if entry.path.to_string_lossy().to_lowercase() == lower_path {
                return Some(entry.clone());
            }
        }
        let lower = target.to_lowercase();
        let mut best: Option<&VaultEntry> = None;
        for entry in entries {
            let name = entry.path.file_name().and_then(|s| s.to_str())?;
            if name.to_lowercase() != lower && entry.stem().to_lowercase() != lower {
                continue;
            }
            match best {
                Some(b) if b.path.components().count() <= entry.path.components().count() => {}
                _ => best = Some(entry),
            }
        }
        best.cloned()
    }

    /// Notes that link to `rel` (backlinks).
    pub fn backlinks(
        &self,
        rel: impl AsRef<Path>,
        entries: &[VaultEntry],
    ) -> Result<Vec<VaultEntry>> {
        let rel = rel.as_ref();
        let Some(self_entry) = entries.iter().find(|e| e.path == rel) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for entry in entries {
            if entry.path == rel {
                continue;
            }
            let text = self.read(&entry.path).unwrap_or_default();
            let links = crate::markdown::extract_wikilinks(&text);
            for link in links {
                let resolved = self.resolve_link(&link.target, entries)?;
                if resolved.as_ref() == Some(self_entry) {
                    out.push(entry.clone());
                    break;
                }
            }
        }
        Ok(out)
    }
}

/// Validate a vault-relative path that Shale is about to **create**: every
/// component must be a name a vault can hold, and the path may not traverse.
/// Returns the same path with any `.` components dropped.
fn checked_rel(rel: &Path) -> Result<PathBuf> {
    if rel.as_os_str().is_empty() {
        bail!("a name cannot be empty");
    }
    let mut out = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(c) => {
                let name = c
                    .to_str()
                    .with_context(|| format!("{} is not valid utf-8", rel.display()))?;
                check_name(name, rel)?;
                out.push(c);
            }
            Component::ParentDir | Component::CurDir => {
                bail!("path traversal is not allowed: {}", rel.display());
            }
            _ => bail!("invalid path component in: {}", rel.display()),
        }
    }
    Ok(out)
}

/// Reject a file or folder name that cannot live in a vault: Obsidian's
/// forbidden characters, control characters, the names that mean "this folder"
/// or "its parent", and a trailing `.` or space, which Obsidian also refuses.
fn check_name(name: &str, rel: &Path) -> Result<()> {
    if name.is_empty() {
        bail!("a name cannot be empty");
    }
    if name == "." || name == ".." {
        bail!("`{name}` is not a name a note can have");
    }
    for c in name.chars() {
        if FORBIDDEN_NAME_CHARS.contains(&c) {
            bail!("`{c}` is not allowed in a note name ({})", rel.display());
        }
        if c.is_control() {
            bail!(
                "control characters are not allowed in a note name ({})",
                rel.display()
            );
        }
    }
    if name.ends_with('.') || name.ends_with(' ') {
        bail!(
            "a note name cannot end with `{}` ({})",
            if name.ends_with('.') { '.' } else { ' ' },
            rel.display()
        );
    }
    Ok(())
}

/// Refuse to touch anything under a dot directory, which is where the vault's
/// own machinery (`.obsidian`, `.shale`, `.git`) lives.
fn guard_unprotected(rel: &Path) -> Result<()> {
    for comp in rel.components() {
        if let Component::Normal(c) = comp {
            if c.to_string_lossy().starts_with('.') {
                bail!("{} is not vault content", rel.display());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(tempfile::TempDir, Vault)> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("notes"))?;
        fs::create_dir_all(root.join(".obsidian"))?;
        fs::create_dir_all(root.join("assets"))?;
        fs::write(root.join("Welcome.md"), "Link to [[Another Note]].")?;
        fs::write(root.join("notes/Another Note.md"), "Back to [[welcome]].")?;
        fs::write(root.join(".obsidian/config.json"), "{}")?;
        fs::write(root.join("notes/ignored.txt"), "not markdown")?;
        fs::write(root.join("assets/Diagram.png"), "not really a png")?;
        Ok((dir, Vault::open(root)?))
    }

    #[test]
    fn lists_attachments_as_well_as_notes() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let paths: Vec<_> = vault
            .list_attachments()?
            .iter()
            .map(|e| e.path.clone())
            .collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("assets/Diagram.png"),
                PathBuf::from("notes/ignored.txt")
            ],
            "everything that is not a note, and no dot directories"
        );
        Ok(())
    }

    #[test]
    fn resolve_attachment_by_path_name_or_stem() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let entries = vault.list_attachments()?;

        assert_eq!(
            vault
                .resolve_attachment("assets/Diagram.png", &entries)
                .unwrap()
                .path,
            PathBuf::from("assets/Diagram.png")
        );
        // Case-insensitive path, and a bare name or stem anywhere in the vault.
        assert_eq!(
            vault
                .resolve_attachment("assets/diagram.PNG", &entries)
                .unwrap()
                .path,
            PathBuf::from("assets/Diagram.png")
        );
        assert_eq!(
            vault.resolve_attachment("Diagram", &entries).unwrap().path,
            PathBuf::from("assets/Diagram.png")
        );
        assert!(vault.resolve_attachment("missing.png", &entries).is_none());
        assert!(vault.resolve_attachment("", &entries).is_none());
        Ok(())
    }

    #[test]
    fn lists_markdown_sorted_and_skips_hidden() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let entries = vault.list_markdown()?;
        let paths: Vec<_> = entries.iter().map(|e| e.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("Welcome.md"),
                PathBuf::from("notes/Another Note.md")
            ]
        );
        Ok(())
    }

    #[test]
    fn rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        assert!(vault.resolve("../escape.md").is_err());
        assert!(vault.resolve("/absolute.md").is_err());
        assert!(vault.resolve("a/../../b.md").is_err());
        assert!(vault.resolve("ok/sub.md").is_ok());
    }

    #[test]
    fn write_creates_parents_and_read_roundtrips() -> Result<()> {
        let (_dir, vault) = fixture()?;
        vault.write("deep/nested/new.md", "hello")?;
        assert_eq!(vault.read("deep/nested/new.md")?, "hello");
        Ok(())
    }

    #[test]
    fn lists_folders_including_empty_ones() -> Result<()> {
        let (_dir, vault) = fixture()?;
        vault.create_dir("inbox/empty")?;
        let paths: Vec<_> = vault.list_dirs()?.iter().map(|e| e.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("assets"),
                PathBuf::from("inbox"),
                PathBuf::from("inbox/empty"),
                PathBuf::from("notes")
            ],
            "every folder, however empty, and no dot directories"
        );
        Ok(())
    }

    #[test]
    fn resolve_link_exact_and_by_stem() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let entries = vault.list_markdown()?;

        let by_stem = vault.resolve_link("Another Note", &entries)?.unwrap();
        assert_eq!(by_stem.path, PathBuf::from("notes/Another Note.md"));

        // Case-insensitive stem match.
        let by_case = vault.resolve_link("welcome", &entries)?.unwrap();
        assert_eq!(by_case.path, PathBuf::from("Welcome.md"));

        // Exact relative path with extension.
        let exact = vault
            .resolve_link("notes/Another Note.md", &entries)?
            .unwrap();
        assert_eq!(exact.path, PathBuf::from("notes/Another Note.md"));

        assert!(vault.resolve_link("missing", &entries)?.is_none());
        Ok(())
    }

    #[test]
    fn backlinks_found() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let entries = vault.list_markdown()?;
        let links = vault.backlinks("Welcome.md", &entries)?;
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, PathBuf::from("notes/Another Note.md"));
        Ok(())
    }

    #[test]
    fn open_missing_dir_fails() {
        assert!(Vault::open("/definitely/not/a/vault/here").is_err());
    }

    #[test]
    fn create_note_adds_the_extension_and_its_parents() -> Result<()> {
        let (_dir, vault) = fixture()?;
        assert_eq!(
            vault.create_note("notes/Idea")?,
            PathBuf::from("notes/Idea.md"),
            "a bare name becomes a note, and the folder is created on the way"
        );
        assert_eq!(vault.read("notes/Idea.md")?, "");
        Ok(())
    }

    #[test]
    fn create_rename_and_delete_round_trip() -> Result<()> {
        let (_dir, vault) = fixture()?;
        vault.create_note("inbox/Thought.md")?;
        assert_eq!(
            vault.rename("inbox/Thought.md", "notes/Thought.md")?,
            PathBuf::from("notes/Thought.md")
        );
        assert!(!vault.exists("inbox/Thought.md"));
        assert_eq!(vault.read("notes/Thought.md")?, "");
        vault.delete("notes/Thought.md")?;
        assert!(!vault.exists("notes/Thought.md"));
        Ok(())
    }

    #[test]
    fn an_edit_never_overwrites_or_replaces() -> Result<()> {
        let (_dir, vault) = fixture()?;
        // Creating over a note that exists would lose it.
        assert!(vault.create_note("Welcome.md").is_err());
        // So would renaming onto one.
        assert!(vault.rename("Welcome.md", "notes/Another Note.md").is_err());
        assert!(vault.exists("Welcome.md"));
        assert!(vault.exists("notes/Another Note.md"));
        // And a missing source is an error, not a silent move.
        assert!(vault.rename("nope.md", "elsewhere.md").is_err());
        Ok(())
    }

    #[test]
    fn names_a_vault_cannot_hold_are_refused() -> Result<()> {
        let (_dir, vault) = fixture()?;
        for name in [
            "quote\".md",
            "star*.md",
            "angle<.md",
            "colon:.md",
            "pipe|.md",
            "question?.md",
            "back\\\\slash.md",
            "trailing.",
            "trailing ",
            "",
            ".",
            "..",
        ] {
            assert!(
                vault.create_note(name).is_err(),
                "`{name}` is not a name Shale should create"
            );
        }
        assert_eq!(
            vault.create_note("new/folder/name.md")?,
            PathBuf::from("new/folder/name.md"),
            "`/` separates folders, so it is only forbidden inside a single name"
        );
        // A name Obsidian forbids, so the vault stays openable there.
        assert!(vault.create_dir("notes/a:b").is_err());
        // And nothing under a dot directory is vault content.
        assert!(vault.create_note(".shale/secret.md").is_err());
        assert!(vault.delete(".obsidian/config.json").is_err());
        assert!(vault.exists(".obsidian/config.json"));
        Ok(())
    }

    #[test]
    fn delete_refuses_a_folder_and_a_folder_with_notes_in_it() -> Result<()> {
        let (_dir, vault) = fixture()?;
        assert!(
            vault.delete("notes").is_err(),
            "a folder is not a note, and deleting one recursively is not `delete`"
        );
        assert!(vault.delete_dir("notes").is_err(), "it still holds notes");
        assert!(vault.exists("notes/Another Note.md"));
        vault.create_dir("empty")?;
        vault.delete_dir("empty")?;
        assert!(!vault.resolve("empty")?.exists());
        assert!(
            vault.delete_dir("Welcome.md").is_err(),
            "that is not a folder"
        );
        Ok(())
    }
}
