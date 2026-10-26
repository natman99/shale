use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Directories that should never be treated as vault content.
const IGNORED_DIRS: &[&str] = &[".git", ".obsidian", ".trash", ".shale"];

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
                } else if file_type.is_file() && name.ends_with(".md") {
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
    pub fn write(&self, rel: impl AsRef<Path>, content: &str) -> Result<()> {
        let path = self.resolve(rel)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }
        fs::write(&path, content).with_context(|| format!("writing note {}", path.display()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<(tempfile::TempDir, Vault)> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("notes"))?;
        fs::create_dir_all(root.join(".obsidian"))?;
        fs::write(root.join("Welcome.md"), "Link to [[Another Note]].")?;
        fs::write(root.join("notes/Another Note.md"), "Back to [[welcome]].")?;
        fs::write(root.join(".obsidian/config.json"), "{}")?;
        fs::write(root.join("notes/ignored.txt"), "not markdown")?;
        Ok((dir, Vault::open(root)?))
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
}
