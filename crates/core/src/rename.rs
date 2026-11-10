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

/// One note whose text changes because of a rename — of a file, or of a property
/// key written across the vault. The shape is the same because what the caller does
/// with it is: write the new text, and count what changed in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
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
) -> Result<Vec<Rewrite>> {
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
            out.push(Rewrite {
                path: entry.path.clone(),
                text,
                changed,
            });
        }
    }
    Ok(out)
}

/// Every note that writes the property key `old`, with that key spelled `new`
/// (roadmap P4.3a M5: the rename that is the migration path for a vocabulary
/// cleanup — `Type` becomes `type` because someone asked for it, not because a
/// tool decided the vault should be lowercase).
///
/// Three things bound what gets touched, and each is a lie it is refusing to tell:
///
/// - **Only the note's top block.** Prose that reads `cr: 5` — a statblock in a
///   table, a sentence in a review — is not frontmatter, and rewriting it would
///   change what the note *says*.
/// - **Only top-level property lines.** A nested map's inner keys are indexed as
///   `nested` and never rewritten (ruling §1.5), so an indented `cr:` under
///   `data:` stays where its author put it.
/// - **Only the key span.** The value, its quotes and a trailing comment are the
///   author's and stay byte for byte; the span comes from the parser's own
///   `key_span`, the same one the editor colours, so the rename and the colour
///   cannot disagree about where the key ends.
///
/// Keys are case-significant, so renaming `Type` does not touch `type`. A block
/// that will not parse is skipped: it holds no properties, so rewriting its text
/// would be a guess about what the author meant.
pub fn rewrites_for_key_rename(
    vault: &Vault,
    entries: &[VaultEntry],
    old: &str,
    new: &str,
) -> Result<Vec<Rewrite>> {
    let mut out = Vec::new();
    for entry in entries {
        let text = vault.read(&entry.path)?;
        if let Some((text, changed)) = rename_key(&text, old, new) {
            out.push(Rewrite {
                path: entry.path.clone(),
                text,
                changed,
            });
        }
    }
    Ok(out)
}

/// One note's text with every top-level `old:` in its block spelled `new:`, and how
/// many lines changed. `None` when the note would not change at all.
fn rename_key(text: &str, old: &str, new: &str) -> Option<(String, usize)> {
    let block = crate::frontmatter::block(text)?;
    if block.malformed.is_some() {
        return None;
    }
    // Split on the newline rather than keeping it: `key_cut` reads a colon at the
    // end of a *line*, and a line still carrying its `\n` has no colon at its end.
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let mut changed = 0;
    for (ix, line) in lines.iter_mut().enumerate() {
        // The fences are not property lines, and neither is anything outside them.
        if ix <= block.lines.0 || ix > block.lines.1 {
            continue;
        }
        if let Some(edited) = rename_key_line(line, old, new) {
            *line = edited;
            changed += 1;
        }
    }
    if changed == 0 {
        return None;
    }
    Some((lines.join("\n"), changed))
}

/// One line with its key spelled `new`, or `None` when the line is not a top-level
/// property line naming `old`.
fn rename_key_line(line: &str, old: &str, new: &str) -> Option<String> {
    let (start, end) = crate::frontmatter::key_span(line)?;
    // Indented means nested: the line belongs to a map Shale indexes but never
    // rewrites.
    if !line[..start].is_empty() {
        return None;
    }
    let colon = end - 1;
    if crate::frontmatter::unquote(line[start..colon].trim()) != old {
        return None;
    }
    let mut out = String::with_capacity(line.len() + new.len());
    out.push_str(&line[..start]);
    out.push_str(new);
    out.push_str(&line[colon..]);
    Some(out)
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
    fn a_key_rename_touches_the_block_and_nothing_else() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        vault.write(
            "goblin.md",
            concat!(
                "---\n",
                "cr: 5 # challenge rating\n",
                "Type: goblin\n",
                "data:\n",
                "  cr: 9\n",
                "\"Max HP\": 7\n",
                "---\n",
                "\n",
                "In prose it reads cr: 5, and a table says\n",
                "\n",
                "| cr | 5 |\n",
            ),
        )?;
        vault.write("plain.md", "No block at all, and cr: 2 in the text.\n")?;
        let entries = vault.list_markdown()?;

        let rewrites = rewrites_for_key_rename(&vault, &entries, "cr", "challenge")?;
        assert_eq!(rewrites.len(), 1, "only the note with a block changes");
        assert_eq!(rewrites[0].changed, 1, "one line, not the nested one");
        assert_eq!(
            rewrites[0].text,
            concat!(
                "---\n",
                "challenge: 5 # challenge rating\n",
                "Type: goblin\n",
                "data:\n",
                "  cr: 9\n",
                "\"Max HP\": 7\n",
                "---\n",
                "\n",
                "In prose it reads cr: 5, and a table says\n",
                "\n",
                "| cr | 5 |\n",
            ),
            "the key span and nothing else: the comment, the case of Type, the \
             nested cr, the quoted key and the prose all stay byte for byte"
        );
        Ok(())
    }

    #[test]
    fn a_key_rename_reaches_every_note_that_writes_it() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        vault.write("a.md", "---\ncr: 5\n---\n")?;
        vault.write("notes/b.md", "---\ncr: 3\ncr: 4\n---\n")?;
        vault.write("c.md", "---\ncr: low\n---\n")?;
        let entries = vault.list_markdown()?;

        let rewrites = rewrites_for_key_rename(&vault, &entries, "cr", "CR")?;
        let by_path: std::collections::BTreeMap<_, _> = rewrites
            .iter()
            .map(|r| (r.path.clone(), (r.text.clone(), r.changed)))
            .collect();
        assert_eq!(by_path.len(), 3, "every note that writes the key");
        assert_eq!(by_path.get(Path::new("notes/b.md")).unwrap().1, 2);
        assert_eq!(
            by_path.get(Path::new("c.md")).unwrap().0,
            "---\nCR: low\n---\n",
            "a value the index calls text is still a value; the rename does not \
             decide what it means"
        );
        Ok(())
    }

    #[test]
    fn a_key_rename_is_case_significant_and_skips_a_block_it_cannot_read() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        vault.write("upper.md", "---\nType: goblin\n---\n")?;
        vault.write("lower.md", "---\ntype: goblin\n---\n")?;
        // Two ways a block holds no properties: the fence is never closed, and the
        // block closes but will not parse. Neither is something a rename should
        // edit — the index never claimed a key in them, so the note would change
        // without the census having listed it.
        vault.write("broken.md", "---\ncr: 5\n\nsome prose\n")?;
        vault.write("unparseable.md", "---\ncr: a: b\n---\n")?;
        let entries = vault.list_markdown()?;

        let rewrites = rewrites_for_key_rename(&vault, &entries, "Type", "type")?;
        assert_eq!(rewrites.len(), 1, "renaming `Type` is not renaming `type`");
        assert_eq!(rewrites[0].path, Path::new("upper.md"));

        let rewrites = rewrites_for_key_rename(&vault, &entries, "cr", "challenge")?;
        assert!(rewrites.is_empty(), "a malformed block is left alone");
        Ok(())
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
