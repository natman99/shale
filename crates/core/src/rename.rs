//! Rewriting the wiki-links that point at a note when it is renamed.
//!
//! A rename is only finished when the links that pointed at the old name point at
//! the new one. Nothing in a vault records which notes those are, so the only way
//! to find them is to read every note and resolve what it links to — the same walk
//! vault search makes.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::markdown::extract_wikilinks;
use crate::vault::{Vault, VaultEntry};

/// One note whose text changes because of the rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRewrite {
    /// Which note to rewrite, vault-relative, and named as it was *before* the
    /// rename — the caller knows the new path and can map that one row.
    pub path: PathBuf,
    /// The note's new text.
    pub text: String,
    /// How many links in it changed.
    pub changed: usize,
}

/// Every note that links to `old`, with those links pointing at `new` instead.
///
/// `entries` has to be the vault as it was **before** the rename: resolving a
/// target is what decides whether a link points at the note, and once the file has
/// moved the old name resolves to nothing.
///
/// Only the name part of a target is rewritten (see
/// [`crate::markdown::WikiLink::stem_start`]), so a link written as a path keeps
/// its folder, a link to a heading keeps its anchor, an aliased link keeps its
/// alias, and an embed keeps its `!`. The renamed note is scanned too, because a
/// note can link to itself.
pub fn rewrites_for_rename(
    vault: &Vault,
    entries: &[VaultEntry],
    old: impl AsRef<Path>,
    new: impl AsRef<Path>,
) -> Result<Vec<LinkRewrite>> {
    let old = old.as_ref();
    let Some(new_stem) = new
        .as_ref()
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for entry in entries {
        let text = vault.read(&entry.path)?;
        let (text, changed) = rename_targets(&text, entries, vault, old, &new_stem);
        if changed > 0 {
            out.push(LinkRewrite {
                path: entry.path.clone(),
                text,
                changed,
            });
        }
    }
    Ok(out)
}

/// The links in one note that point at `old`, spliced to `new_stem`.
fn rename_targets(
    text: &str,
    entries: &[VaultEntry],
    vault: &Vault,
    old: &Path,
    new_stem: &str,
) -> (String, usize) {
    let edits: Vec<(usize, usize)> = extract_wikilinks(text)
        .iter()
        .filter_map(|link| {
            let resolved = vault.resolve_link(&link.target, entries).ok()??;
            (resolved.path == old).then_some((link.stem_start, link.stem_end))
        })
        .collect();
    if edits.is_empty() {
        return (text.to_string(), 0);
    }
    // Spliced back to front is not needed: the scanner reports links in source
    // order and their ranges never overlap.
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for &(start, end) in &edits {
        out.push_str(&text[from..start]);
        out.push_str(new_stem);
        from = end;
    }
    out.push_str(&text[from..]);
    (out, edits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vault with one note every test renames, and links to it in every form
    /// that matters.
    fn fixture() -> Result<(tempfile::TempDir, Vault, Vec<VaultEntry>)> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        vault.write("Target.md", "I link to [[Target]] myself.\n")?;
        vault.write(
            "a.md",
            "Plain [[Target]] and aliased [[Target|read this]].\n\
             A [[Other]] note and `[[Target]]` in code.\n",
        )?;
        vault.write(
            "notes/b.md",
            "Path [[notes/Target]] anchor [[Target#A heading]].\n",
        )?;
        vault.write("notes/Target.md", "A different note with the same name.\n")?;
        vault.write("Other.md", "Nothing links here.\n")?;
        let entries = vault.list_markdown()?;
        Ok((dir, vault, entries))
    }

    #[test]
    fn a_rename_rewrites_every_link_that_points_at_the_note() -> Result<()> {
        let (_d, vault, entries) = fixture()?;
        let rewrites = rewrites_for_rename(&vault, &entries, "a.md", "Renamed.md")?;
        // Nothing links to a.md, so there is nothing to rewrite.
        assert!(rewrites.is_empty());

        let rewrites = rewrites_for_rename(&vault, &entries, "Target.md", "Renamed.md")?;
        let by_path: std::collections::BTreeMap<_, _> = rewrites
            .iter()
            .map(|r| (r.path.clone(), r.text.clone()))
            .collect();
        assert_eq!(
            by_path.get(Path::new("a.md")).map(String::as_str),
            Some(
                "Plain [[Renamed]] and aliased [[Renamed|read this]].\n\
                 A [[Other]] note and `[[Target]]` in code.\n"
            ),
            "the alias survives and the code span does not"
        );
        assert_eq!(
            by_path.get(Path::new("notes/b.md")).map(String::as_str),
            Some("Path [[notes/Target]] anchor [[Renamed#A heading]].\n"),
            "the anchor survives; the path-form link resolves to the shortest \
             match, which is the root note, and keeps its folder text"
        );
        assert_eq!(
            by_path.get(Path::new("Target.md")).map(String::as_str),
            Some("I link to [[Renamed]] myself.\n"),
            "a note that links to itself is rewritten too"
        );
        assert_eq!(rewrites.iter().map(|r| r.changed).sum::<usize>(), 4);
        Ok(())
    }

    /// Two notes share a name in different folders. Obsidian resolves a bare
    /// `[[Target]]` to the shortest path, so only that one is renamed, and the
    /// link that spells the other one out is left alone.
    #[test]
    fn a_link_that_means_the_other_note_is_left_alone() -> Result<()> {
        let (_d, vault, entries) = fixture()?;
        let rewrites = rewrites_for_rename(&vault, &entries, "notes/Target.md", "Renamed.md")?;
        assert!(
            rewrites.iter().all(|r| !r.text.contains("[[Renamed]]")),
            "every bare [[Target]] in the vault means the root note"
        );
        Ok(())
    }

    #[test]
    fn a_link_written_in_another_case_finds_the_note() -> Result<()> {
        let (_d, vault, _entries) = fixture()?;
        vault.write("c.md", "Lowercase [[target]] here.\n")?;
        let entries = vault.list_markdown()?;
        let rewrites = rewrites_for_rename(&vault, &entries, "Target.md", "Renamed.md")?;
        let c = rewrites
            .iter()
            .find(|r| r.path == Path::new("c.md"))
            .expect("c.md links to the renamed note");
        assert_eq!(c.text, "Lowercase [[Renamed]] here.\n");
        Ok(())
    }

    /// A rename that cannot name a note (no file stem) rewrites nothing rather
    /// than splicing an empty name into every link.
    #[test]
    fn a_target_with_no_name_rewrites_nothing() -> Result<()> {
        let (_d, vault, entries) = fixture()?;
        let rewrites = rewrites_for_rename(&vault, &entries, "Target.md", "")?;
        assert!(rewrites.is_empty());
        Ok(())
    }
}
