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

use crate::markdown::Anchor;
use crate::mdstyle::object::{heading_lines, HeadingLine};
use crate::vault::{LinkResolver, Vault, VaultEntry};

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
    /// What the embed asks for inside the note: a section, or one block (P4.9).
    /// `None` is the whole note; Obsidian's bare `![[note#]]` means the same thing and
    /// parses to `None`.
    pub anchor: Option<Anchor>,
    /// The `|alias`, kept for the badge and the embed's own label. Not used to
    /// resolve: a link resolves by its target.
    pub alias: Option<String>,
}

/// The embed a line is, if the line is nothing but one.
pub fn line_embed(line: &str) -> Option<EmbedRef> {
    let trimmed = line.trim();
    // An embed line may carry its own block id (`![[a]] ^x`), and the id is the note's
    // claim on the line, not part of the link. Cutting it first is what keeps adding an
    // id from silently un-embedding a working transclusion: the id is what a `#^x` link
    // elsewhere names, and the embed is still an embed.
    let trimmed = crate::mdstyle::without_block_anchor(trimmed);
    let rest = trimmed.strip_prefix("![[")?.strip_suffix("]]")?;
    // A nested bracket is a mistake, not a link.
    if rest.contains('[') || rest.contains(']') {
        return None;
    }
    let (link, alias) = match rest.split_once('|') {
        Some((link, alias)) => (link, Some(alias.trim().to_string())),
        None => (rest, None),
    };
    // The same splitter the link scanner uses, so `gd` on `[[note#^abc]]` and an embed of
    // `![[note#^abc]]` cannot mean two different things.
    let (target, anchor) = crate::markdown::split_anchor(link);
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
    /// An embed that stopped expanding: its name, and the door a click opens.
    Badge(Badge),
}

/// An embed that stopped expanding: what it says, and the door a click opens.
///
/// The label alone used to be the whole payload, which made a badge a caption: it named
/// a note the reader was standing next to and could not reach from where they were. The
/// link the badge stands for rides along so the app can answer a click on it the way it
/// answers `gd` on the `![[…]]` row itself (roadmap P4.0a, leftover #1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    /// Why there is no content, in the words a followed link uses for the same problem,
    /// so the two surfaces cannot disagree about what went wrong.
    pub label: String,
    /// The embed as the note wrote it — target, anchor, alias. A door is written in the
    /// same terms the link resolver takes, so a badge still opens the right note after a
    /// rename, and lands on the section the embed asked for.
    pub embed: EmbedRef,
}

/// A saved query an embed points at (roadmap P4.3 M4).
///
/// The resolver cannot answer it. A query's content is what its program *computes*, and
/// the index to compute it against belongs to the window — so the resolver says what the
/// embed points at (the file, the name the reader knows it by, and the program as the
/// file wrote it) and the app runs it and draws the answer under the `![[…]]` row. One
/// resolution, two halves: nobody gets to decide on their own what `![[Creature
/// ratings]]` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryBody {
    /// The `.shq` itself, vault-relative.
    pub path: PathBuf,
    /// The embed as the author wrote it, so the block's title row can be the door a
    /// click opens — the same `EmbedRef` a stopped embed's badge carries.
    pub embed: EmbedRef,
    /// The declaration's `name`, else the file's stem: the name the reader knows the
    /// query by, which is what the block's title row says.
    pub name: String,
    /// The file parsed as a query note, or the reason it is not one. A failure is not a
    /// ghost and not a silence: the block says it (EDGE-11).
    pub note: Result<crate::query_note::QueryNote, String>,
}

/// What one embed contributes to the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// Nothing to paint. The line stays exactly as the note wrote it — which is
    /// right for an attachment embed (the image renderer owns that row) and for
    /// a target that resolves to no note. A *broken* embed deserves a badge of
    /// its own; that is P6.9's broken-embed pass, not this one.
    Nothing,
    /// The embed stopped expanding: its name, and the door a click opens.
    Badge(Badge),
    Lines(Vec<EmbedLine>),
    /// The embed names a saved query. The app runs it; the rows it draws are the
    /// answer, not text the resolver could hand over.
    Query(QueryBody),
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

/// What an embed target is looked up against: the vault's notes and its saved queries,
/// each with its own [`LinkResolver`], plus the aliases the notes declare.
///
/// Built once per resolution rather than per embed, because the tables are O(entries) and
/// one embed body can nest several levels deep. The aliases belong here for the same
/// reason they belong to the jump path: an `aliases:` property is a name the note answers
/// to, and an embed or a hover preview that refused that name while `gd` accepted it would
/// be two surfaces disagreeing about one link (bug report, 2026-11-12).
pub struct Targets {
    /// The notes, with the aliases they declare.
    notes: LinkResolver,
    /// The saved queries. Aliases are never consulted here: an alias's owner is a note,
    /// and a name declared by a note is not a name a query can be found by.
    queries: LinkResolver,
}

impl Targets {
    /// The resolver pair for one vault's lists. `aliases` is `(alias, vault-relative
    /// path of the note that declares it)` — what the index's `props` table already
    /// holds, so there is one source for them.
    pub fn new(
        entries: &[VaultEntry],
        queries: &[VaultEntry],
        aliases: &[(String, String)],
    ) -> Self {
        Self {
            notes: LinkResolver::new(entries).with_aliases(aliases),
            queries: LinkResolver::new(queries),
        }
    }
}

/// The body a note embed contributes, read from the files.
///
/// `host` is the note holding the embed, so a self-embed is caught by the chain
/// instead of spending the depth budget on itself. `entries` is the vault's
/// markdown list the caller already holds — resolution is a table lookup, and
/// building the table per embed would cost more than reading the note. `queries` is
/// the same kind of list for `.shq` files: a target that names no note may still name
/// a saved query. `aliases` is what makes `![[anon]]` find the note that declares
/// `anon`, exactly as `[[anon]]` does when a link is followed.
pub fn resolve(
    vault: &Vault,
    entries: &[VaultEntry],
    queries: &[VaultEntry],
    aliases: &[(String, String)],
    host: &Path,
    embed: &EmbedRef,
) -> Body {
    resolve_from(entries, queries, aliases, host, embed, vault)
}

/// As [`resolve`], reading each note through `sources`.
#[allow(clippy::too_many_arguments)]
pub fn resolve_from(
    entries: &[VaultEntry],
    queries: &[VaultEntry],
    aliases: &[(String, String)],
    host: &Path,
    embed: &EmbedRef,
    sources: &dyn Sources,
) -> Body {
    let targets = Targets::new(entries, queries, aliases);
    let mut chain = vec![host.to_path_buf()];
    expand(&targets, embed, 1, &mut chain, sources)
}

#[allow(clippy::too_many_arguments)]
fn expand(
    targets: &Targets,
    embed: &EmbedRef,
    depth: usize,
    chain: &mut Vec<PathBuf>,
    sources: &dyn Sources,
) -> Body {
    let Some(entry) = targets.notes.resolve_link(&embed.target).cloned() else {
        // Not a note. It may be a saved query — and a note wins when both exist, the
        // same precedence an alias is given over a note name.
        if let Some(query) = targets.queries.resolve_link(&embed.target).cloned() {
            return query_body(&query, embed, sources);
        }
        // An attachment is the image renderer's row; anything else
        // is a broken link, and the raw line is the honest rendering of one.
        return Body::Nothing;
    };

    let path = entry.path.clone();
    if chain.contains(&path) {
        return Body::Badge(Badge {
            label: label(embed, entry.stem()),
            embed: embed.clone(),
        });
    }
    let Some(text) = sources.text(&path) else {
        return Body::Nothing;
    };
    // The section the anchor names, as *rows of the note* rather than as strings,
    // so every painted row keeps the address it was cut from.
    let wanted: Vec<(usize, &str)> = match embed.anchor.as_ref() {
        None => text.lines().enumerate().collect(),
        Some(Anchor::Heading(heading)) => match section_rows(&text, heading) {
            Some(rows) => rows,
            // The note is there and the section is not. Say so rather than
            // painting an empty block that reads as a successful embed, and say it
            // in the same words a followed link uses when its heading is missing,
            // so the two surfaces cannot disagree about what went wrong (ruled
            // 2026-11-06, and P4.6a's wording).
            None => {
                return Body::Badge(Badge {
                    label: format!("no section \"{heading}\" in {}", entry.stem()),
                    embed: embed.clone(),
                });
            }
        },
        // One block, cut by its id: the same rows the block map says the id names, so
        // what an embed shows and what `:blocks` lists and what `ib` selects are one
        // answer (ruling 46's rule, applied to blocks).
        Some(Anchor::Block(id)) => match block_rows(&text, id) {
            Some(rows) => rows,
            None => {
                return Body::Badge(Badge {
                    label: format!("no block \"^{id}\" in {}", entry.stem()),
                    embed: embed.clone(),
                });
            }
        },
    };
    // This note is now on the path to everything inside it. Without the push,
    // `a -> b -> a` only stops at the depth cap, spending the whole budget on
    // two notes instead of saying "you are already here" at the second `a`.
    chain.push(path.clone());
    let rows = rows_of(targets, &path, &wanted, depth, chain, sources);
    chain.pop();
    if rows.is_empty() {
        return Body::Nothing;
    }
    Body::Lines(rows)
}

#[allow(clippy::too_many_arguments)]
fn rows_of(
    targets: &Targets,
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
                match expand(targets, &inner, depth + 1, chain, sources) {
                    Body::Lines(inner_rows) => rows.extend(inner_rows),
                    // A saved query reached through another note's body: its name and
                    // its door, not a second table inside this one. The answer belongs
                    // to the row that asked for it, and a row of a block is not that.
                    Body::Query(query) => rows.push(EmbedLine {
                        depth: depth + 1,
                        origin: None,
                        row: EmbedRow::Badge(Badge {
                            label: query.name.clone(),
                            embed: inner.clone(),
                        }),
                    }),
                    Body::Badge(badge) => rows.push(EmbedLine {
                        depth: depth + 1,
                        origin: None,
                        row: EmbedRow::Badge(badge),
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
                row: EmbedRow::Badge(Badge {
                    label: label(&inner, inner.target.as_str()),
                    embed: inner.clone(),
                }),
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

/// The body an embed gets when its target names a saved query (roadmap P4.3 M4).
///
/// The program is read through `sources` — the same door an embedded note's text comes
/// through, so a `.shq` open in a tab with unsaved edits is *in hand* and the block
/// shows the program the reader is looking at, not the disk's older one.
///
/// An anchor asks a program for a section. It has none — its `#` lines are SQL comments,
/// not headings — and the answer is the sentence the note branch already uses for the
/// same mistake, so the two surfaces cannot disagree about what went wrong.
fn query_body(query: &VaultEntry, embed: &EmbedRef, sources: &dyn Sources) -> Body {
    // A program has neither sections nor blocks: its `#` lines are SQL comments and its
    // text holds no `^id` the scan would read. The sentence names the kind the author
    // asked for, so the two mistakes do not read the same (EDGE-16).
    if let Some(anchor) = embed.anchor.as_ref() {
        let noun = match anchor {
            Anchor::Heading(_) => "section",
            Anchor::Block(_) => "block",
        };
        return Body::Badge(Badge {
            label: format!("no {noun} \"{}\" in {}", anchor.written(), query.stem()),
            embed: embed.clone(),
        });
    }
    let Some(text) = sources.text(&query.path) else {
        // The list says the file is there and the read says it is not: the vault moved
        // between the frame and now. The raw line is the honest rendering of that.
        return Body::Nothing;
    };
    let note = crate::query_note::QueryNote::parse(&text);
    let (name, note) = match note {
        Ok(note) => (note.display_name(query.stem()).to_string(), Ok(note)),
        Err(why) => (query.stem().to_string(), Err(why.to_string())),
    };
    Body::Query(QueryBody {
        path: query.path.clone(),
        embed: embed.clone(),
        name,
        note,
    })
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
    let head = find_heading(text, anchor)?;
    // The section runs to the next heading of the same depth or shallower. The
    // headings come from the parser's scan, so a `#` inside a fenced block neither
    // opens a section nor ends one — the fence is part of the section it sits in.
    let heads = heading_lines(text);
    let end = heads
        .iter()
        .find(|other| other.line > head.line && other.depth <= head.depth)
        .map_or(usize::MAX, |other| other.line);
    Some(
        text.lines()
            .enumerate()
            .skip(head.line)
            .take_while(|(line, _)| *line < end)
            .collect(),
    )
}

/// As [`section_rows`], with the row numbers dropped.
pub fn section(text: &str, anchor: &str) -> Option<Vec<String>> {
    section_rows(text, anchor)
        .map(|rows| rows.into_iter().map(|(_, line)| line.to_string()).collect())
}

/// The rows of the block a block anchor names, as `(row, line)` pairs.
///
/// The cousin of [`section_rows`], and the same shape on purpose: the row numbers are
/// what make an embedded line addressable, and a block's rows are its own lines — a
/// paragraph's soft-wrapped lines, a list item *and its sub-list*, a whole table, a
/// whole quote, a heading line alone (ruling 4).
///
/// Matched on the id exactly, case-sensitively: Obsidian's ids are, and two ids that
/// differ only in case are two ids. Two ids on one block both address it (EDGE-10) and
/// the first in reading order wins, the way a heading match does.
pub fn block_rows<'a>(text: &'a str, id: &str) -> Option<Vec<(usize, &'a str)>> {
    let (first, last) = find_block(text, id)?;
    Some(
        text.lines()
            .enumerate()
            .skip(first)
            .take_while(|(line, _)| *line <= last)
            .collect(),
    )
}

/// The line a block anchor names — the block's **first** line — or `None` if no block
/// carries the id. What a jump reveals, and the same number the `blocks` table stores,
/// so a row from the index and a row from the buffer drive the same reveal (EDGE-12/13).
pub fn block_row(text: &str, id: &str) -> Option<usize> {
    find_block(text, id).map(|(first, _)| first)
}

/// The block an id names, as its first and last line, from the one block scan.
fn find_block(text: &str, id: &str) -> Option<(usize, usize)> {
    if id.is_empty() {
        // `[[note#^]]` names nothing (EDGE-14) — the same rule an empty heading anchor
        // gets: refuse, rather than matching the first block that has no id.
        return None;
    }
    let map = crate::mdstyle::block_map(text);
    map.anchors
        .iter()
        .find(|anchor| anchor.id == id)
        .map(|anchor| {
            let block = &map.blocks[anchor.block];
            (block.first_line, block.last_line)
        })
}

/// The line a heading anchor names, or `None` if no heading matches.
///
/// The same matching [`section`] uses — trimmed of `#`, case-insensitive, never a `#`
/// inside a fenced block — because `gd` to `[[note#The Fall]]` and an embed of
/// `![[note#The Fall]]` must agree about which heading they mean. A jump that finds a
/// heading the embed would not, or the other way round, gives the reader two answers to
/// the same written question.
pub fn heading_row(text: &str, anchor: &str) -> Option<usize> {
    find_heading(text, anchor).map(|head| head.line)
}

/// The heading an anchor names, from the parser's own scan: the same walk the fold,
/// the vault index and the text object read, so the section an embed slices and the
/// row a jump reveals are the same section (headings-plan.md ruling 46).
///
/// Matching stays the anchor matcher's own rule — trimmed, case-insensitive, first
/// match wins — because the author types from memory. What moved here is only "is
/// this line a heading, and which part of it is its name".
fn find_heading(text: &str, anchor: &str) -> Option<HeadingLine> {
    let want = anchor.trim().to_lowercase();
    if want.is_empty() {
        // An empty anchor is a typo, not a request for the nameless section a bare
        // `#` makes. The index still lists that section: the census and the jump are
        // allowed to differ about a heading with no name (EDGE-15).
        return None;
    }
    heading_lines(text).into_iter().find(|head| {
        // A heading carrying a block id is still its name's section: the id is the
        // note's claim on the line, not part of the title (EDGE-27).
        crate::mdstyle::without_block_anchor(&text[head.text_start..head.text_end])
            .trim()
            .eq_ignore_ascii_case(&want)
    })
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
        assert_eq!(e.anchor, Some(Anchor::Heading("The Fall".into())));
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
        let body = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e);
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
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
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
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
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
            &[],
            &[],
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
            &[],
            &[],
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
                row: EmbedRow::Badge(Badge {
                    label: "d".into(),
                    embed: line_embed("![[d]]").unwrap(),
                })
            }],
            "over budget: the name, not the content"
        );
    }

    /// An embed line can carry its own block id: `![[a]] ^x` is still an embed, and the
    /// id is what a `#^x` link elsewhere names. Requiring the line to be *only* the
    /// embed would make adding an id silently un-embed a working transclusion.
    #[test]
    fn an_embed_line_can_carry_its_own_block_id() {
        let e = line_embed("![[gift#^atom]] ^here").unwrap();
        assert_eq!(e.target, "gift");
        assert_eq!(e.anchor, Some(Anchor::Block("atom".into())));
        // Without the space, the caret is not an id (the grammar asks for whitespace
        // before it), and the line is not an embed shape either. That is the author's
        // typo, painted as written.
        assert_eq!(line_embed("![[gift]]^here"), None);
    }

    /// The recursion cap is the standing one, and a block anchor buys no extra levels:
    /// host -> a -> b -> c is three levels of content, and the embed inside c's block
    /// stops at its name (P4.9's standing rule: there is a cap).
    #[test]
    fn block_embeds_stop_at_the_same_depth_as_everything_else() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[a#^one]]"),
            ("a.md", "![[b#^two]] ^one"),
            ("b.md", "![[c#^three]] ^two"),
            ("c.md", "![[d#^four]] ^three"),
            ("d.md", "never painted ^four"),
        ]);
        let e = line_embed("![[a#^one]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![EmbedLine {
                depth: MAX_DEPTH,
                origin: None,
                row: EmbedRow::Badge(Badge {
                    label: "d".into(),
                    embed: line_embed("![[d#^four]]").unwrap(),
                })
            }],
            "the fourth level is a name, not content"
        );
    }

    /// A block that embeds its own note is cut by the chain, not by the cap: the badge
    /// says "you are already here" at the second `a` instead of spending the whole budget
    /// on one note.
    #[test]
    fn a_block_that_embeds_its_own_note_badges_instead_of_looping() {
        let (_d, v, entries) = vault_with(&[("host.md", "![[a#^x]]"), ("a.md", "![[a#^x]] ^x\n")]);
        let e = line_embed("![[a#^x]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![EmbedLine {
                depth: 2,
                origin: None,
                row: EmbedRow::Badge(Badge {
                    label: "a".into(),
                    embed: line_embed("![[a#^x]]").unwrap(),
                })
            }],
            "the loop is cut at the second mention of the note"
        );
    }

    #[test]
    fn a_note_already_on_the_chain_badges_instead_of_looping() {
        let (_d, v, entries) = vault_with(&[("host.md", "![[a]]"), ("a.md", "text\n![[host]]")]);
        let e = line_embed("![[a]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
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
                    row: EmbedRow::Badge(Badge {
                        label: "host".into(),
                        embed: line_embed("![[host]]").unwrap(),
                    })
                },
            ]
        );
    }

    #[test]
    fn a_self_embed_badges_at_the_first_level() {
        let (_d, v, entries) = vault_with(&[("host.md", "![[host]]")]);
        let e = line_embed("![[host]]").unwrap();
        assert_eq!(
            resolve(&v, &entries, &[], &[], Path::new("host.md"), &e),
            Body::Badge(Badge {
                label: "host".into(),
                embed: e.clone(),
            })
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
                &[],
                &[],
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
                &[],
                &[],
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
                &[],
                &[],
                Path::new("host.md"),
                &line_embed("![[gift#Nope]]").unwrap()
            ),
            // Not `Nothing`, which is for a target that is no note at all: the note
            // is there, and the reader is owed the reason the block is empty.
            Body::Badge(Badge {
                label: "no section \"Nope\" in gift".into(),
                embed: line_embed("![[gift#Nope]]").unwrap(),
            })
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
            &[],
            &[],
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
                row: EmbedRow::Badge(Badge {
                    label: "the last".into(),
                    embed: line_embed("![[d|the last]]").unwrap(),
                })
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

        // The line scanner this matcher used to be insisted on a space after the
        // markers, so it called these prose and the parser called them headings.
        // Reading the parser's scan is the whole of ruling 46, and this is the
        // behaviour it changes: a section that looks like one can be jumped to.
        assert_eq!(
            heading_row("intro\n#\tTabbed\nbody\n", "Tabbed"),
            Some(1),
            "a tab after the markers is whitespace to the parser (EDGE-2)"
        );
        assert_eq!(
            heading_row("~~~\n# Fake\n~~~\n# Fake\n", "Fake"),
            Some(3),
            "a tilde fence hides a heading, and the closer has to match the opener"
        );
        assert_eq!(
            heading_row("#\nnamed\n", ""),
            None,
            "a bare `#` is a section with no name, and an empty anchor is a typo, not \
             a request for it (EDGE-15)"
        );
    }

    /// The section an embed slices and the row a jump reveals come from one scan, so
    /// a heading the old line scanner refused is both jumpable *and* sliceable.
    #[test]
    fn a_section_named_by_an_anchor_that_needs_a_tab_is_still_a_section() {
        let text = "intro\n#\tTabbed\nbody\nnext\n# Other\n";
        assert_eq!(
            section(text, "Tabbed"),
            Some(vec![
                "#\tTabbed".to_string(),
                "body".to_string(),
                "next".to_string()
            ]),
            "the section runs to the next heading of its own level or shallower"
        );
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

    /// A saved query is an embed target too: `![[tough]]` names `queries/tough.shq` when
    /// no note does. The resolver hands over the program, not an answer — the index to
    /// compute it against belongs to the window (roadmap P4.3 M4).
    #[test]
    fn an_embed_of_a_saved_query_carries_its_program() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[tough]]"),
            (
                "queries/tough.shq",
                "+++\nname = \"Creature ratings\"\n+++\nSELECT note FROM props\n",
            ),
        ]);
        let queries = v.list_queries().unwrap();
        let e = line_embed("![[tough]]").unwrap();
        let Body::Query(query) = resolve(&v, &entries, &queries, &[], Path::new("host.md"), &e)
        else {
            panic!("expected a query body");
        };
        assert_eq!(query.path, Path::new("queries/tough.shq"));
        assert_eq!(
            query.name, "Creature ratings",
            "the declaration's name, which is what the reader knows it by"
        );
        assert_eq!(
            query.note.as_ref().unwrap().sql.trim(),
            "SELECT note FROM props"
        );
    }

    /// A note wins when a note and a query answer to the same name: the embed is the
    /// note's body. The same precedence an alias is given over a note name — nothing
    /// that already means a note gets quietly retargeted at a query.
    #[test]
    fn a_note_beats_a_query_of_the_same_name() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[tough]]"),
            ("tough.md", "a note, not a query"),
            ("queries/tough.shq", "+++\n+++\nSELECT 1\n"),
        ]);
        let queries = v.list_queries().unwrap();
        let e = line_embed("![[tough]]").unwrap();
        let body = resolve(&v, &entries, &queries, &[], Path::new("host.md"), &e);
        let Body::Lines(rows) = body else {
            panic!("the note's body, not the query: {body:?}");
        };
        assert_eq!(rows[0].row, EmbedRow::Text("a note, not a query".into()));
    }

    #[test]
    fn an_embed_of_an_alias_reaches_the_note_that_declares_it() {
        // The fourth rule — the `aliases:` property — belongs to the embed path as much
        // as to the jump path. A hover preview or a `![[anon]]` that refused a name `gd`
        // accepts is two surfaces disagreeing about one link (bug report, 2026-11-12).
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[anon]]"),
            (
                "masked.md",
                "---\naliases:\n  - anon\n---\nthe masked one\n",
            ),
        ]);
        let aliases = vec![("anon".to_string(), "masked.md".to_string())];
        let e = line_embed("![[anon]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &aliases, Path::new("host.md"), &e)
        else {
            panic!("the alias resolves to the note that declares it");
        };
        // The body came from the note that *declares* the alias, and every row keeps
        // that note's address: clicking one of these rows lands in masked.md.
        assert!(rows.iter().all(|line| line
            .origin
            .as_ref()
            .is_some_and(|o| o.path == std::path::Path::new("masked.md"))));
        assert!(rows
            .iter()
            .any(|line| { matches!(&line.row, EmbedRow::Text(text) if text == "the masked one") }));
    }

    #[test]
    fn an_alias_never_steals_an_embed_from_a_note_of_that_name() {
        // The rule is additive, and an embed is where that matters most: a note called
        // `anon.md` keeps its own embed even when another note declares `anon`.
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[anon]]"),
            ("anon.md", "the real anon"),
            (
                "masked.md",
                "---\naliases:\n  - anon\n---\nthe masked one\n",
            ),
        ]);
        let aliases = vec![("anon".to_string(), "masked.md".to_string())];
        let e = line_embed("![[anon]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &aliases, Path::new("host.md"), &e)
        else {
            panic!("the file rule answers first");
        };
        assert_eq!(rows[0].row, EmbedRow::Text("the real anon".into()));
    }

    /// A program that is not a query note is not a ghost and not a silence: the body
    /// carries the reason, so the block can say it (EDGE-11).
    #[test]
    fn an_embedded_file_that_is_not_a_query_note_carries_the_reason() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[tough]]"),
            // A declaration that opens and never closes: the file is not a query note,
            // whatever its body says. (A body the *runner* would refuse is a different
            // gate, and parses fine.)
            ("queries/tough.shq", "+++\nname = \"tough\"\n"),
        ]);
        let queries = v.list_queries().unwrap();
        let e = line_embed("![[tough]]").unwrap();
        let Body::Query(query) = resolve(&v, &entries, &queries, &[], Path::new("host.md"), &e)
        else {
            panic!("expected a query body");
        };
        assert_eq!(
            query.name, "tough",
            "the stem, since there is no declaration to read"
        );
        let Err(why) = query.note else {
            panic!("the parse should have failed");
        };
        assert!(why.contains("+++"), "the reason names the fence: {why}");
    }

    /// The program is read through `Sources`, not from the disk: a `.shq` open in a tab
    /// with unsaved edits is answered by the program the reader is looking at. A
    /// resolver that read the file itself would quietly disagree with the editor.
    #[test]
    fn a_query_embed_reads_its_program_through_the_source_door() {
        struct Edited(&'static str);
        impl Sources for Edited {
            fn text(&self, _path: &Path) -> Option<String> {
                Some(self.0.to_string())
            }
        }
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[tough]]"),
            ("queries/tough.shq", "+++\n+++\nSELECT 1\n"),
        ]);
        let queries = v.list_queries().unwrap();
        let e = line_embed("![[tough]]").unwrap();
        let Body::Query(query) = resolve_from(
            &entries,
            &queries,
            &[],
            Path::new("host.md"),
            &e,
            &Edited("+++\n+++\nSELECT 2\n"),
        ) else {
            panic!("expected a query body");
        };
        assert_eq!(
            query.note.unwrap().sql.trim(),
            "SELECT 2",
            "the text in hand, not the disk's older one"
        );
    }

    /// A query note has no sections — its `#` lines are SQL comments — so an anchor is
    /// refused in the same words the note branch uses for a missing section.
    #[test]
    fn an_anchor_on_a_query_embed_is_refused() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[tough#hp]]"),
            ("queries/tough.shq", "+++\n+++\nSELECT 1\n"),
        ]);
        let queries = v.list_queries().unwrap();
        let e = line_embed("![[tough#hp]]").unwrap();
        let Body::Badge(badge) = resolve(&v, &entries, &queries, &[], Path::new("host.md"), &e)
        else {
            panic!("expected a stopped embed");
        };
        assert_eq!(badge.label, "no section \"hp\" in tough");
    }

    /// The anchor's **kind** comes from the link's spelling, never from what the note
    /// happens to contain (EDGE-23): the caret claims the position, so `#^foo` asks for
    /// a block id even in a note whose heading is literally `^foo`.
    #[test]
    fn the_anchor_kind_is_decided_by_the_spelling() {
        assert_eq!(
            line_embed("![[gift#^goblin]]").unwrap().anchor,
            Some(Anchor::Block("goblin".into()))
        );
        assert_eq!(
            line_embed("![[gift#The Fall]]").unwrap().anchor,
            Some(Anchor::Heading("The Fall".into()))
        );
        // `#^` with nothing after it is still a block question, with nothing to name
        // (EDGE-14); a bare `#` is still the whole note.
        assert_eq!(
            line_embed("![[gift#^]]").unwrap().anchor,
            Some(Anchor::Block(String::new()))
        );
        assert_eq!(line_embed("![[gift#]]").unwrap().anchor, None);
        // The chained form Obsidian accepts is refused in this branch's own words: the
        // anchor reads as a heading literally named that, which no note has (ruling K).
        assert_eq!(
            line_embed("![[gift#The Fall#^goblin]]").unwrap().anchor,
            Some(Anchor::Heading("The Fall#^goblin".into()))
        );
    }

    /// A block embed cuts the block: the id's own soft-wrapped lines, and not the
    /// paragraph next door. The id's line is part of the block — it is the note's claim,
    /// painted and never hidden (ruling 6).
    #[test]
    fn an_embed_of_a_block_is_that_blocks_lines_only() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[gift#^goblin]]"),
            (
                "gift.md",
                "The goblin waits\nby the door. ^goblin\n\nNot the troll.\n",
            ),
        ]);
        let e = line_embed("![[gift#^goblin]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![
                EmbedLine {
                    depth: 1,
                    origin: at("gift.md", 0),
                    row: EmbedRow::Text("The goblin waits".into())
                },
                EmbedLine {
                    depth: 1,
                    origin: at("gift.md", 1),
                    row: EmbedRow::Text("by the door. ^goblin".into())
                },
            ],
            "the block's rows keep their addresses in the note"
        );
    }

    /// EDGE-7 and ruling 5: a list item's block carries its sub-list, and the id on the
    /// parent's bullet names the whole thing.
    #[test]
    fn a_block_embed_of_a_list_item_carries_its_sub_list() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[gift#^kid]]"),
            (
                "gift.md",
                "- Gemmy ^kid\n    - Unhelpful assistant\n- Someone else\n",
            ),
        ]);
        let e = line_embed("![[gift#^kid]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows.iter()
                .map(|r| r.origin.as_ref().map(|o| o.row))
                .collect::<Vec<_>>(),
            vec![Some(0), Some(1)],
            "the parent's line and its child's, and not the next sibling"
        );
    }

    /// EDGE-8/9: a table's id stands on its own line under the table, and names every
    /// row of it — rows are not addressable on their own.
    #[test]
    fn a_block_embed_of_a_table_carries_the_whole_table() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[gift#^tbl]]"),
            ("gift.md", "| a | b |\n| - | - |\n| 1 | 2 |\n\n^tbl\n"),
        ]);
        let e = line_embed("![[gift#^tbl]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(rows.len(), 3, "three rows of the table, no more");
        assert_eq!(rows[0].row, EmbedRow::Text("| a | b |".into()));
    }

    /// Ruling 4: an id on a heading line names **the line**, not its section. The two
    /// anchor kinds have two meanings and never overlap.
    #[test]
    fn a_block_anchor_on_a_heading_names_the_line_and_not_the_section() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[gift#^fall]]"),
            ("gift.md", "## The Fall ^fall\nthe city went under\n"),
        ]);
        let e = line_embed("![[gift#^fall]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        assert_eq!(
            rows,
            vec![EmbedLine {
                depth: 1,
                origin: at("gift.md", 0),
                row: EmbedRow::Text("## The Fall ^fall".into())
            }]
        );
        // The same note through the heading anchor is the section: two spellings, two
        // answers, both honest.
        let h = line_embed("![[gift#The Fall]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &h) else {
            panic!("expected lines");
        };
        assert_eq!(rows.len(), 2);
    }

    /// The refusal names the kind the author asked for, so a block miss never reads like
    /// a heading miss. The followed-link surface uses the same sentence (M3's app half),
    /// which is why the wording lives here and not in a view.
    #[test]
    fn a_missing_block_anchor_says_so_in_its_own_words() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[gift#^gone]]\n![[gift#Gone]]"),
            ("gift.md", "text ^here\n"),
        ]);
        let block = line_embed("![[gift#^gone]]").unwrap();
        let Body::Badge(badge) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &block)
        else {
            panic!("expected a stopped embed");
        };
        assert_eq!(badge.label, "no block \"^gone\" in gift");
        assert_eq!(
            badge.embed.anchor,
            Some(Anchor::Block("gone".into())),
            "the door still carries what the note asked for"
        );

        let heading = line_embed("![[gift#Gone]]").unwrap();
        let Body::Badge(badge) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &heading)
        else {
            panic!("expected a stopped embed");
        };
        assert_eq!(badge.label, "no section \"Gone\" in gift");
    }

    /// EDGE-14: `#^` names nothing. It does not match the first block that happens to
    /// have no id — the rule an empty heading anchor already gets.
    #[test]
    fn an_empty_block_anchor_names_nothing() {
        let (_d, v, entries) =
            vault_with(&[("host.md", "![[gift#^]]"), ("gift.md", "one\n\ntwo\n")]);
        let e = line_embed("![[gift#^]]").unwrap();
        let Body::Badge(badge) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
            panic!("expected a stopped embed");
        };
        assert_eq!(badge.label, "no block \"^\" in gift");
    }

    /// Ruling 2: ids are case-sensitive, because two ids differing only in case are two
    /// ids. (A heading anchor stays case-insensitive: the author types a title from
    /// memory, but an id is copied.)
    #[test]
    fn a_block_id_is_case_sensitive() {
        let text = "The goblin waits. ^goblin\n";
        assert_eq!(block_row(text, "goblin"), Some(0));
        assert_eq!(block_row(text, "Goblin"), None);
    }

    /// Ruling 6: the standing cap bounds block embeds too. The caret does not buy a
    /// fourth level, and the row that stops still carries the block it was asked for.
    #[test]
    fn block_embeds_are_bounded_by_the_same_cap() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[a#^x]]"),
            ("a.md", "- a-one ^x\n    ![[b#^y]]\n"),
            ("b.md", "- b-one ^y\n    ![[c#^z]]\n"),
            ("c.md", "- c-one ^z\n    ![[d#^w]]\n"),
            ("d.md", "d-one ^w\n"),
        ]);
        let e = line_embed("![[a#^x]]").unwrap();
        let Body::Lines(rows) = resolve(&v, &entries, &[], &[], Path::new("host.md"), &e) else {
            panic!("expected lines");
        };
        let badges: Vec<&Badge> = rows
            .iter()
            .filter_map(|row| match &row.row {
                EmbedRow::Badge(badge) => Some(badge),
                _ => None,
            })
            .collect();
        assert_eq!(badges.len(), 1, "one embed stopped by the cap: {rows:?}");
        assert_eq!(badges[0].label, "d");
        assert_eq!(
            badges[0].embed.anchor,
            Some(Anchor::Block("w".into())),
            "a stopped block embed is still a door to that block"
        );
        assert!(
            rows.iter().all(|row| row.depth <= MAX_DEPTH),
            "no row sits deeper than the cap: {rows:?}"
        );
    }

    /// EDGE-16: a program has no blocks, and the refusal says *block* rather than
    /// *section*, because the author asked for a block.
    #[test]
    fn a_block_anchor_on_a_query_embed_is_refused_as_a_block() {
        let (_d, v, entries) = vault_with(&[
            ("host.md", "![[tough#^hp]]"),
            ("queries/tough.shq", "+++\n+++\nSELECT 1\n"),
        ]);
        let queries = v.list_queries().unwrap();
        let e = line_embed("![[tough#^hp]]").unwrap();
        let Body::Badge(badge) = resolve(&v, &entries, &queries, &[], Path::new("host.md"), &e)
        else {
            panic!("expected a stopped embed");
        };
        assert_eq!(badge.label, "no block \"^hp\" in tough");
    }

    /// The number a jump needs is the number the `blocks` table stores: the block's
    /// **first** line. They have to agree, or the index and the buffer answer a reveal
    /// differently (EDGE-12/13).
    #[test]
    fn block_row_names_the_blocks_first_line() {
        let text = "intro\n\nThe goblin waits\nby the door. ^goblin\n\n- one\n    - two ^kid\n";
        assert_eq!(block_row(text, "goblin"), Some(2));
        assert_eq!(block_row(text, "kid"), Some(6));
        assert_eq!(block_row(text, "gone"), None);
        assert_eq!(block_row(text, ""), None);
    }
}
