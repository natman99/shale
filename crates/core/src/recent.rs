//! The app-level memory of which vaults have been opened.
//!
//! A vault's own state lives inside it (`.shale/`); this is the other half:
//! the list of vaults the app knows, kept where per-application state usually
//! goes — `$XDG_STATE_HOME` (falling back to `~/.local/state`), in `shale/`.
//! The picker reads the list, and startup opens `last` when no vault was
//! named on the command line.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

/// How many vaults the list remembers. The picker is a short menu, not a
/// archive: a vault that has not been opened in months has earned its exit.
const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentVaults {
    /// The vault startup should open when the command line names none.
    #[serde(default)]
    pub last: Option<PathBuf>,
    /// Most recently used first.
    #[serde(default)]
    pub recent: Vec<PathBuf>,
}

impl RecentVaults {
    /// `shale/recent.json` under the XDG state directory.
    pub fn store_path() -> PathBuf {
        let base = match std::env::var_os("XDG_STATE_HOME") {
            Some(v) if !v.is_empty() => PathBuf::from(v),
            _ => {
                let home = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(PathBuf::from)
                    .unwrap_or_default();
                home.join(".local").join("state")
            }
        };
        base.join("shale").join("recent.json")
    }

    /// Read the list. A missing or unreadable file is not an error: an app
    /// with no memory still starts, it just has nothing to remember.
    pub fn load() -> Self {
        Self::load_from(&Self::store_path())
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::store_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serialize recent vaults")?;
        // Same write-then-rename discipline as every other file Shale owns:
        // a crash mid-write must not leave a half-written memory behind.
        let tmp = path.with_extension("json.tmp");
        fs::write(tmp.as_path(), text).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("rename over {}", path.display()))?;
        Ok(())
    }

    /// Record that `vault` was opened: it becomes the last vault and the head
    /// of the list, and any older entry for the same path is dropped. Paths
    /// are canonicalized so the same vault reached by two spellings is one
    /// entry.
    pub fn touch(&mut self, vault: &Path) {
        let vault = vault.canonicalize().unwrap_or_else(|_| vault.to_path_buf());
        self.recent.retain(|p| p != &vault);
        self.recent.insert(0, vault.clone());
        self.recent.truncate(MAX_RECENT);
        self.last = Some(vault);
    }

    /// The vault to open with no command line: `last`, if it is still a
    /// directory on disk. A vault on a disconnected drive does not block
    /// startup; the next entry is not consulted — the picker is.
    pub fn startup_vault(&self) -> Option<PathBuf> {
        self.last
            .as_ref()
            .filter(|p| p.is_dir())
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "shale-recent-{}-{}",
            name,
            std::process::id()
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn touch_puts_the_vault_first_and_last() {
        let a = dir("a");
        let b = dir("b");
        let mut r = RecentVaults::default();
        r.touch(&a);
        r.touch(&b);
        assert_eq!(r.recent.len(), 2);
        assert_eq!(r.recent[0], b.canonicalize().unwrap(), "most recent first");
        assert_eq!(r.last.as_deref(), Some(b.canonicalize().unwrap().as_path()));
    }

    #[cfg(unix)]
    #[test]
    fn the_same_vault_by_two_spellings_is_one_entry() {
        let base = dir("spellings");
        let direct = base.join("vault");
        fs::create_dir_all(&direct).unwrap();
        let link = base.join("alias");
        std::os::unix::fs::symlink(&direct, &link).unwrap();
        let mut r = RecentVaults::default();
        r.touch(&direct);
        r.touch(&link);
        assert_eq!(r.recent.len(), 1, "one vault, one entry");
    }

    #[test]
    fn the_list_is_capped() {
        let mut r = RecentVaults::default();
        for i in 0..(MAX_RECENT + 5) {
            let d = dir(&format!("cap{i}"));
            r.touch(&d);
        }
        assert_eq!(r.recent.len(), MAX_RECENT);
    }

    #[test]
    fn save_and_load_round_trip() {
        let base = dir("roundtrip");
        let a = dir("rt-a");
        let path = base.join("nested").join("recent.json");
        let mut r = RecentVaults::default();
        r.touch(&a);
        r.save_to(&path).unwrap();
        let back = RecentVaults::load_from(&path);
        assert_eq!(back, r);
    }

    #[test]
    fn a_missing_or_broken_file_is_an_empty_memory() {
        let base = dir("missing");
        assert_eq!(RecentVaults::load_from(&base.join("nope.json")), RecentVaults::default());
        let broken = base.join("broken.json");
        fs::write(&broken, "not = = toml").unwrap();
        assert_eq!(RecentVaults::load_from(&broken), RecentVaults::default());
    }

    #[test]
    fn startup_skips_a_vault_that_is_gone() {
        let gone = PathBuf::from("/definitely/not/a/vault/anywhere");
        let r = RecentVaults { last: Some(gone), recent: vec![] };
        assert_eq!(r.startup_vault(), None);
    }
}
