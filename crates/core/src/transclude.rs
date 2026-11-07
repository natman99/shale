//! Transclusion: what a `![[somewhere]]` line contributes to the note that holds it.
//!
//! P4.0a is the read-only half. The host's buffer is never touched — an embed is
//! content the *view* adds, which is why the app paints it as a phantom block
//! (`crates/app/src/editor/overlay.rs` is the same trick for a queued proposal).
//! Everything here is pure: parse the reference, resolve it through the vault
//! index, cut out the section an anchor names, and expand the embeds nested
//! inside the result, bounded by [`MAX_DEPTH`].
//!
//! Two rules do the safety work, both from the design's binding rulings:
//!
//! - **The depth cap.** 1 editable + 2 read-only = 3 levels. P4.0a has only
//!   read-only embeds, so the whole budget is here. Past it an embed is a badge
//!   with the note's name — clickable, no content — which is what kills an
//!   embed loop by construction rather than by detection.
//! - **The chain.** A note already on the path from the host to this embed never
//!   expands again. The cap alone would let `a → b → a` burn all three levels on
//!   two notes; the chain says "you are already here" at the second `a`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::vault::{Vault, VaultEntry};

/// How many levels of embed expand before they become badges.
pub const MAX_DEPTH: usize = 3;

/// A `![[...]]` reference on a line that is nothing but that reference.
///
/// Whole-line only, for the same reason [`crate::markdown::line_image_embed`]
/// is: the view hands the entire display row over to what the embed resolves
/// to, so a row mixing an embed with prose would put the paint and the wrap
/// model out of step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedRef {
    /// The link target, without its `#anchor` or `|alias`.
    pub target: String,
    /// The heading the embed asks for. `None` is the whole note; Obsidian's
    /// bare `![[note#]]` means the same thing and parses to `None`.
    pub anchor: Option<String>,
    /// The `|alias`, kept for the badge and the embed's own label. Not used to
    /// resolve: a link resolves by its target.
    pub alias: Option<String>,
}

/// The embed a line is, if the line is nothing but one.
pub fn line_embed(line: &str) -> Option<EmbedRef> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("![[")?.strip_suffix("]]")?;
    // A nested bracket is a mistake, not a link.
    if rest.contains('[') || rest.contains(']') {
        return None;
    }
    let (link, alias) = match rest.split_once('|') {
        Some((link, alias)) => (link, Some(alias.trim().to_string())),
        None => (rest, None),
    };
    let (target, anchor) = match link.split_once('#') {
        Some((target, anchor)) => (
            target.trim().to_string(),
            // A bare `#` asks for the whole note.
            (!anchor.trim().is_empty()).then(|| anchor.trim().to_string()),
        ),
        None => (link.trim().to_string(), None),
    };
    if target.is_empty() {
        return None;
    }
    Some(EmbedRef {
        target,
        anchor,
        alias,
    })
}

/// Where a painted row came from: which note, and which row of it.
///
/// This is what turns an embed's rows from a picture into a place. A row that
/// knows its note and row can be clicked, can hold a caret, and can be checked
/// for still existing — which is what a section badge and an entered embed both
/// need (P4.0b). A row that is only a string can be looked at.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    pub path: PathBuf,
    /// Zero-based line index within that note.
    pub row: usize,
}

/// One row an embed contributes, and how far inside it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedLine {
    /// 1 is the host's own embed; a nested embed's rows are deeper. Drives the
    /// indent and the rule the app draws down the side of the block.
    pub depth: usize,
    /// The note this row is a line *of*, when it is one. `None` for a row the
    /// resolver invented — a badge naming a note that stopped expanding — which
    /// is content to read and nothing to edit.
    pub origin: Option<Origin>,
    pub row: EmbedRow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedRow {
    /// Source text, styled by the app the way the note's own lines are.
    Text(String),
    /// An embed that stopped expanding: its name, and nothing else.
    Badge(String),
}

/// What one embed contributes to the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// Nothing to paint. The line stays exactly as the note wrote it — which is
    /// right for an attachment embed (the image renderer owns that row) and for
    /// a target that resolves to no note. A *broken* embed deserves a badge of
    /// its own; that is P6.9's broken-embed pass, not this one.
    Nothing,
    /// The embed stopped expanding: name only.
    Badge(String),
    Lines(Vec<EmbedLine>),
}

/// Where the resolver reads a note's text.
///
/// The default is the file. The app answers from an open buffer when it has one:
/// an embed must show the note as Shale has it in hand, not as the disk has it,
/// or a window displays two versions of one note at once (P4.0b ruling 12).
pub trait Sources {
    /// The note's current text, or `None` if there is no such note.
    fn text(&self, path: &Path) -> Option<String>;

    /// A number that moves whenever any note this resolver can reach changes.
    ///
    /// A cache keyed on it needs no freshness policy: it is thrown out exactly
    /// when a note is edited, and not re-resolved when nothing happened. The
    /// default is a constant, which is right for a source that changes by
    /// outside means and gets invalidated by a watcher instead.
    fn stamp(&self) -> u64 {
        0
    }
}

impl Sources for Vault {
    fn text(&self, path: &Path) -> Option<String> {
        self.read(path).ok()
    }
}

/// The body a note embed contributes, read from the files.
///
/// `host` is the note holding the embed, so a self-embed is caught by the chain
/// instead of spending the depth budget on itself. `entries` is the vault's
/// markdown list the caller already holds — resolution is a table lookup, and
/// building the table per embed would cost more than reading the note.
pub fn resolve(vault: &Vault, entries: &[VaultEntry], host: &Path, embed: &EmbedRef) -> Body {
    resolve_from(vault, entries, host, embed, vault)
}

/// As [`resolve`], reading each note through `sources`.
pub fn resolve_from(
    vault: &Vault,
    entries: &[VaultEntry],
    host: &Path,
    embed: &EmbedRef,
    sources: &dyn Sources,
) -> Body {
    let mut chain = vec![host.to_path_buf()];
    expand(vault, entries, embed, 1, &mut chain, sources)
}

fn expand(
    vault: &Vault,
    entries: &[VaultEntry],
    embed: &EmbedRef,
    depth: usize,
    chain: &mut Vec<PathBuf>,
    sources: &dyn Sources,
) -> Body {
    let Some(entry) = vault.resolve_link(&embed.target, entries).ok().flatten() else {
        // Not a note. An attachment is the image renderer's row; anything else
        // is a broken link, and the raw line is the honest rendering of one.
        return Body::Nothing;
    };
    let path = entry.path.clone();
    if chain.contains(&path) {
        return Body::Badge(label(embed, entry.stem()));
    }
    let Some(text) = sources.text(&path) else {
        return Body::Nothing;
    };
    // The section the anchor names, as *rows of the note* rather than as strings,
    // so every painted row keeps the address it was cut from.
    let wanted: Vec<(usize, &str)> = match embed.anchor.as_deref() {
        None => text.lines().enumerate().collect(),
        Some(anchor) => match section_rows(&text, anchor) {
            Some(rows) => rows,
            // The note is there and the section is not. Say so rather than
            // painting an empty block that reads as a successful embed, and say it
            // in the same words a followed link uses when its heading is missing,
            // so the two surfaces cannot disagree about what went wrong (ruled
            // 2026-11-06, and P4.6a's wording).
            None => {
                return Body::Badge(format!("no section \"{anchor}\" in {}", entry.stem()));
            }
        },
    };
    // This note is now on the path to everything inside it. Without the push,
    // `a -> b -> a` only stops at the depth cap, spending the whole budget on
    // two notes instead of saying "you are already here" at the second `a`.
    chain.push(path.clone());
    let rows = rows_of(vault, entries, &path, &wanted, depth, chain, sources);
    chain.pop();
    if rows.is_empty() {
        return Body::Nothing;
    }
    Body::Lines(rows)
}

#[allow(clippy::too_many_arguments)]
fn rows_of(
    vault: &Vault,
    entries: &[VaultEntry],
    note: &Path,
    wanted: &[(usize, &str)],
    depth: usize,
    chain: &mut Vec<PathBuf>,
    sources: &dyn Sources,
) -> Vec<EmbedLine> {
    let mut rows = Vec::new();
    for (row, line) in wanted.iter().copied() {
        let here = || {
            Some(Origin {
                path: note.to_path_buf(),
                row,
            })
        };
        match line_embed(line) {
            // A nested embed, with the budget left to expand it. Its rows carry
            // the *nested* note's origin, which is the honest one: clicking one
            // of them lands in the note it is a line of.
            Some(inner) if depth < MAX_DEPTH => {
                match expand(vault, entries, &inner, depth + 1, chain, sources) {
                    Body::Lines(inner_rows) => rows.extend(inner_rows),
                    Body::Badge(name) => rows.push(EmbedLine {
                        depth: depth + 1,
                        origin: None,
                        row: EmbedRow::Badge(name),
                    }),
                    Body::Nothing => rows.push(EmbedLine {
                        depth,
                        origin: here(),
                        row: EmbedRow::Text(line.to_string()),
                    }),
                }
            }
            // Out of budget: the name, not the content.
            Some(inner) => rows.push(EmbedLine {
                depth,
                origin: None,
                row: EmbedRow::Badge(label(&inner, inner.target.as_str())),
            }),
            None => rows.push(EmbedLine {
                depth,
                origin: here(),
                row: EmbedRow::Text(line.to_string()),
            }),
        }
    }
    rows
}

/// What a stopped embed is called: the alias if the author gave one, else the
/// resolved note's name.
fn label(embed: &EmbedRef, stem: &str) -> String {
    embed
        .alias
        .clone()
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| stem.to_string())
}

/// The rows of the section a heading anchor names, as `(row, line)` pairs.
///
/// The row numbers are the point: they are what make an embedded line
/// addressable — "this painted row is row 7 of `gift.md`" — which is what a
/// click, a caret, and a "this section is gone" badge all need (P4.0b).
///
/// Matched on the heading's text, trimmed of `#`, case-insensitively — the same
/// slack the link resolver gives a target, for the same reason: the author types
/// from memory. A `#` inside a fenced code block is neither a heading nor a
/// boundary; the fence itself is part of the section it sits in.
pub fn section_rows<'a>(text: &'a str, anchor: &str) -> Option<Vec<(usize, &'a str)>> {
    let want = anchor.trim().to_lowercase();
    let mut inside_fence = false;
    let mut level: Option<usize> = None;
    let mut out: Vec<(usize, &'a str)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if fence(line) {
            inside_fence = !inside_fence;
        }
        let heading = if inside_fence {
            None
        } else {
            heading_level(line)
        };
        match (level, heading) {
            (None, Some(_)) if heading_text(line).eq_ignore_ascii_case(&want) => {
                level = Some(heading.unwrap());
                out.push((index, line));
            }
            (Some(lvl), Some(h)) if h <= lvl => break,
            (Some(_), _) | (None, None) => {
                if level.is_some() {
                    out.push((index, line));
                }
            }
            (None, Some(_)) => {}
        }
    }
    (!out.is_empty()).then_some(out)
}

/// As [`section_rows`], with the row numbers dropped.
pub fn section(text: &str, anchor: &str) -> Option<Vec<String>> {
    section_rows(text, anchor)
        .map(|rows| rows.into_iter().map(|(_, line)| line.to_string()).collect())
}

/// The line a heading anchor names, or `None` if no heading matches.
///
/// The same matching [`section`] uses — trimmed of `#`, case-insensitive, never a `#`
/// inside a fenced block — because `gd` to `[[note#The Fall]]` and an embed of
/// `![[note#The Fall]]` must agree about which heading they mean. A jump that finds a
/// heading the embed would not, or the other way round, gives the reader two answers to
/// the same written question.
pub fn heading_row(text: &str, anchor: &str) -> Option<usize> {
    let want = anchor.trim().to_lowercase();
    let mut inside_fence = false;
    for (index, line) in text.lines().enumerate() {
        if fence(line) {
            inside_fence = !inside_fence;
            continue;
        }
        if inside_fence {
            continue;
        }
        if heading_level(line).is_some() && heading_text(line).eq_ignore_ascii_case(&want) {
            return Some(index);
        }
    }
    None
}

/// `#` count of an ATX heading, or `None` if the line is not one. A `#` inside a
/// fenced code block is not a heading, so a fence toggles the scan off.
fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        Some(hashes)
    } else {
        None
    }
}

fn heading_text(line: &str) -> &str {
    line.trim_start()
        .trim_start_matches('#')
        .trim_start()
        .trim_end()
}

/// Whether a line opens or closes a fenced code block.
pub fn fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

/// The set of embed targets a note's text refers to, for the app's cache:
/// when one of these changes on disk, the note's embeds are stale.
pub fn targets(text: &str) -> HashSet<String> {
    text.lines()
        .filter_map(line_embed)
        .map(|e| e.target)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn an_embed_line_is_parsed_into_target_anchor_and_alias() {
        let e = line_embed("![[morgoth]]").unwrap();
        assert_eq!(e.target, "morgoth");
        assert_eq!(e.anchor, None);
        assert_eq!(e.alias, None);

        let e = line_embed("  ![[notes/morgoth#The Fall|the fall]]  ").unwrap();
        assert_eq!(e.target, "notes/morgoth");
        assert_eq!(e.anchor.as_deref(), Some("The Fall"));
        assert_eq!(e.alias.as_deref(), Some("the fall"));

        // A bare `#` is the whole note, not an empty heading.
        assert_eq!(line_embed("![[morgoth#]]").unwrap().anchor, None);
    }

    #[test]
    fn a_line_that_is_not_only_an_embed_is_not_an_embed() {
        // The view gives the whole row to the embed, so a mixed row is not one.
        assert!(line_embed("see ![[morgoth]] below").is_none());
        assert!(line_embed("![[morgoth]] and ![[silmaril]]").is_none());
        assert!(line_embed("![[mor[goth]]]").is_none());
        assert!(line_embed("![[]]").is_none());
        assert!(line_embed("[[morgoth]]").is_none());
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_its_own_level_or_shallower() {
        let text =
            "# Top\nintro\n## The Fall\nfirst\n### Deep\nstill the fall\n## The Return\nnot this\n";
        let s = section(text, "The Fall").unwrap();
        assert_eq!(
            s,
            vec!["## The Fall", "first", "### Deep", "still the fall"]
        );
        // Shallower ends it too.
        let s = section("# Top", "Top").unwrap();
        assert_eq!(s, vec!["# Top"]);
        // Matched case-insensitively, and an unknown anchor is no section at all.
        assert_eq!(section(text, "the fall").unwrap()[0], "## The Fall");
        assert!(section(text, "Nowhere").is_none());
    }

    #[test]
    fn a_heading_inside_a_code_block_is_not_a_section_boundary() {
        let text = "## Real\nbefore\n```\n# Fake\n```\nafter\n## Next\nstop\n";
        let s = section(text, "Real").unwrap();
        assert_eq!(
            s,
            vec!["## Real", "before", "```", "# Fake", "```", "after"]
        );
    }

    /// The address a painted row is expected to carry.
    fn at(path: &str, row: usize) -> Option<Origin> {
        Some(Origin {
            path: path.into(),
            row,
        })
    }

    fn vault_with(files: &[(&str, &str)]) -> (TempDir, Vault, Vec<VaultEntry>) {
        let dir = TempDir::new().unwrap();
        for (path, body) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(&full, body).unwrap();
        }
        let vault = Vault::open(dir.path()).unwrap();
        let entries = vault.list_markdown().unwrap();
        (dir, vault, entries)
    }

    #[test]
    fn an_embed_of_a_whole_note_is_its_lines_at_depth_one() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "before\n![[gift]]\nafter\n"),
            ("gift.md", "one\ntwo\n"),
        ]);
        let e = line_embed("![[gift]]").unwrap();
        let body = resolve(&v, &entries, Path::new("host.md"), &e);
        let Body::Lines(rows) = body else {
            panic!("expected lines, got {body:?}");
        };
        assert_eq!(
            rows,
            vec![
                EmbedLine {
                    depth: 1,
                    origin: at("gift.md", 0),
                    row: EmbedRow::Text("one".into())
                },
                EmbedLine {
                    depth: 1,
                    origin: at("gift.md", 1),
                    row: EmbedRow::Text("two".into())
                },
            ]
        );
    }

    #[test]
    fn an_anchored_embed_is_that_section_alone() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[gift#Middle]]"),
            ("gift.md", "# Top\nno\n## Middle\nyes\n## End\nno\n"),
        ]);
        let e = line_embed("![[gift#Middle]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].row, EmbedRow::Text("## Middle".into()));
        assert_eq!(
            rows[0].origin,
            at("gift.md", 2),
            "the section's rows keep their row numbers in the note, not in the block"
        );
    }

    #[test]
    fn embeds_nested_inside_an_embed_are_deeper_and_expand() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[a]]"),
            ("a.md", "a-text\n![[b]]"),
            ("b.md", "b-text"),
        ]);
        let e = line_embed("![[a]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![
                EmbedLine {
                    depth: 1,
                    origin: at("a.md", 0),
                    row: EmbedRow::Text("a-text".into())
                },
                // The nested row belongs to `b`, and says so: clicking it lands in
                // the note it is a line of, not in the note that embedded it.
                EmbedLine {
                    depth: 2,
                    origin: at("b.md", 0),
                    row: EmbedRow::Text("b-text".into())
                },
            ]
        );
    }

    #[test]
    fn three_levels_of_content_expand_and_the_fourth_is_a_name() {
        // MAX_DEPTH counts levels of *content*: host -> a -> b -> c is three, so
        // c's text is painted; an embed inside c would be a fourth and stops.
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[a]]"),
            ("a.md", "![[b]]"),
            ("b.md", "![[c]]"),
            ("c.md", "third level"),
        ]);
        let Body::Lines(rows) = resolve(
            &v,
            &entries,
            Path::new("host.md"),
            &line_embed("![[a]]").unwrap(),
        ) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![EmbedLine {
                depth: 3,
                origin: at("c.md", 0),
                row: EmbedRow::Text("third level".into())
            }]
        );

        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[a]]"),
            ("a.md", "![[b]]"),
            ("b.md", "![[c]]"),
            ("c.md", "![[d]]"),
            ("d.md", "never painted"),
        ]);
        let Body::Lines(rows) = resolve(
            &v,
            &entries,
            Path::new("host.md"),
            &line_embed("![[a]]").unwrap(),
        ) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![EmbedLine {
                depth: 3,
                origin: None,
                row: EmbedRow::Badge("d".into())
            }],
            "over budget: the name, not the content"
        );
    }

    #[test]
    fn a_note_already_on_the_chain_badges_instead_of_looping() {
        let (_d, v, entries) = vault_with(&[("host.md", "![[a]]"), ("a.md", "text\n![[host]]")]);
        let e = line_embed("![[a]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![
                EmbedLine {
                    depth: 1,
                    origin: at("a.md", 0),
                    row: EmbedRow::Text("text".into())
                },
                // The badge sits one level deeper than a's own text, which is
                // where the content it replaces would have been.
                EmbedLine {
                    depth: 2,
                    origin: None,
                    row: EmbedRow::Badge("host".into())
                },
            ]
        );
    }

    #[test]
    fn a_self_embed_badges_at_the_first_level() {
        let (_d, v, entries) = vault_with(&[("host.md", "![[host]]")]);
        let e = line_embed("![[host]]").unwrap();
        assert_eq!(
            resolve(&v, &entries, Path::new("host.md"), &e),
            Body::Badge("host".into())
        );
    }

    #[test]
    fn an_attachment_or_missing_target_contributes_nothing_and_leaves_the_line() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[diagram.png]]\n![[nowhere]]"),
            ("diagram.png", "x"),
        ]);
        assert_eq!(
            resolve(
                &v,
                &entries,
                Path::new("host.md"),
                &line_embed("![[diagram.png]]").unwrap()
            ),
            Body::Nothing,
            "the image renderer owns that row"
        );
        assert_eq!(
            resolve(
                &v,
                &entries,
                Path::new("host.md"),
                &line_embed("![[nowhere]]").unwrap()
            ),
            Body::Nothing,
            "a broken link is honest as its own text until P6.9 gives it a badge"
        );
    }

    #[test]
    fn an_anchor_that_names_no_heading_says_which_section_is_missing() {
        let (_d, v, entries) = vault_with(&[("host.md", "![[gift#Nope]]"), ("gift.md", "# Top\n")]);
        assert_eq!(
            resolve(
                &v,
                &entries,
                Path::new("host.md"),
                &line_embed("![[gift#Nope]]").unwrap()
            ),
            // Not `Nothing`, which is for a target that is no note at all: the note
            // is there, and the reader is owed the reason the block is empty.
            Body::Badge("no section \"Nope\" in gift".into())
        );
    }

    #[test]
    fn the_alias_names_a_badge_the_target_alone_could_not() {
        // The embed that runs out of budget is the one whose alias matters:
        // it is the only thing the reader sees of it.
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[a]]"),
            ("a.md", "![[b]]"),
            ("b.md", "![[c]]"),
            ("c.md", "![[d|the last]]"),
        ]);
        let Body::Lines(rows) = resolve(
            &v,
            &entries,
            Path::new("host.md"),
            &line_embed("![[a]]").unwrap(),
        ) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![EmbedLine {
                depth: 3,
                origin: None,
                row: EmbedRow::Badge("the last".into())
            }],
            "the author's name survives the depth cap"
        );
    }

    #[test]
    fn heading_row_finds_the_line_an_anchor_names() {
        let text = "# Top\nintro\n```\n# Fake\n```\n## The Fall\nbody\n";
        assert_eq!(heading_row(text, "The Fall"), Some(5));
        assert_eq!(
            heading_row(text, "the fall"),
            Some(5),
            "matched case-insensitively"
        );
        assert_eq!(
            heading_row(text, "  The Fall  "),
            Some(5),
            "padded like an author types"
        );
        assert_eq!(
            heading_row(text, "Fake"),
            None,
            "a `#` inside a fence is not a heading"
        );
        assert_eq!(heading_row(text, "Nowhere"), None);
    }

    #[test]
    fn targets_lists_what_a_note_embeds_so_the_cache_knows_when_to_drop() {
        let text = "text\n![[a]]\n![[b#H]]\nsee ![[c]] inline\n![[a]]\n";
        let t = targets(text);
        assert_eq!(
            t.len(),
            2,
            "a twice is one target; c is inline, not an embed"
        );
        assert!(t.contains("a") && t.contains("b"));
    }
}
