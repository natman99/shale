//! Why a note answers an operator, in the note's own words.
//!
//! The index answers *which* notes. This answers *why*, and it answers it out of
//! the note's text rather than out of the index's tables — a ruling (2026-11-09),
//! not a shortcut. `tags(note, tag)` cannot say whether the note wrote
//! `#character` in the body or `tags: character` in frontmatter, and a row that
//! shows the second when the note holds the first is a row that says something the
//! note does not. The tables hold claims; a display string is not a claim. The
//! same ruling left the door open for a schema v4 that stores the line and byte
//! offset of every indexed fact (roadmap P4.14), which would turn this scan into a
//! lookup — worth doing when something needs to *jump* to the position, not before.
//!
//! Two rules keep this honest:
//!
//! 1. **The scanner that built the index finds the evidence.** Body tags come from
//!    [`crate::mdstyle::style_spans`], the same scanner [`crate::graph::index::extract_tags`]
//!    uses, so a `#` inside a code fence or a heading is not evidence here either.
//!    A plain substring search would happily point at the word "character" in prose
//!    while the tag that matched sits somewhere the reader cannot see — a row right
//!    about the note and wrong about the reason.
//! 2. **When the index and the note disagree, there is no evidence.** The file may
//!    have changed since it was indexed. The surface then shows what it showed
//!    before rather than a line that is not in the note.

use crate::frontmatter;
use crate::mdstyle::{style_spans, Style};
use crate::query::{Cmp, Op};
use crate::search::find_all;

/// One line of the note that carries the match, and which bytes of it the query
/// named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// The line as it stands in the note, without its line ending.
    pub line: String,
    /// Byte ranges within [`line`](Self::line): the part the query actually named.
    /// Never a range the note does not contain — a highlight on text that is not
    /// there is worse than no highlight.
    pub ranges: Vec<(usize, usize)>,
}

/// The line of `text` that makes `op` true, when the note still says it.
///
/// `None` for the operators whose answer is a fact about the *file* rather than a
/// line in the note — `path:` and `modified:` — and for any operator the text no
/// longer supports.
pub fn evidence(text: &str, op: &Op) -> Option<Evidence> {
    match op {
        Op::Tag(name) => tag(text, name),
        Op::Link(name) => link(text, name),
        Op::Prop { key, cmp, value } => prop(text, key, *cmp, value.as_deref()),
        Op::Path(_) | Op::Modified(_) => None,
    }
}

/// The line `at` falls on, and the byte its text starts at.
fn line_at(text: &str, at: usize) -> (String, usize) {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    (text[start..end].trim_end_matches('\r').to_string(), start)
}

/// `tag:foo` — the body's `#foo`, or the frontmatter `tags:` that feeds the same
/// table (P4.3a M2). Whichever the note wrote *first*, because a row that quotes
/// the bottom of a note while the tag is on its first line reads as a different
/// note.
fn tag(text: &str, name: &str) -> Option<Evidence> {
    let mut best: Option<(usize, Evidence)> = None;

    for span in style_spans(text) {
        if span.style != Style::Tag {
            continue;
        }
        let written = &text[span.start..span.end];
        if !written.trim_start_matches('#').eq_ignore_ascii_case(name) {
            continue;
        }
        let (line, line_start) = line_at(text, span.start);
        let ev = Evidence {
            line,
            // The `#` comes with it: that is what makes the line a tag rather than
            // a word, and dropping it would quote something the note does not say.
            ranges: vec![(span.start - line_start, span.end - line_start)],
        };
        best = Some((span.start, ev));
        break;
    }

    if let Some(block) = frontmatter::block(text) {
        for prop in &block.props {
            if !frontmatter::is_tag_key(&prop.key) {
                continue;
            }
            let written = prop.value.trim().trim_start_matches('#').trim();
            if !written.eq_ignore_ascii_case(name) {
                continue;
            }
            if let Some(region) = prop_region(text, &block, &prop.key) {
                let ev = region.pick(Some(name));
                if best.as_ref().is_none_or(|(offset, _)| region.at < *offset) {
                    best = Some((region.at, ev));
                }
                break;
            }
        }
    }

    best.map(|(_, ev)| ev)
}

/// `link:Foo` — the line holding the `[[…]]`. Only the bracketed spelling is
/// link-shaped (the M2 ruling), so the brackets are what this looks for, and a
/// statblock that says `name: Goblin` is not evidence of a link to `Goblin.md`.
fn link(text: &str, name: &str) -> Option<Evidence> {
    let wanted = stem(name);
    for span in style_spans(text) {
        // The scanner already splits a link into its parts: `[[lair]]` is one span
        // over the target, and `[[lair|the dark]]` is a `LinkTarget` span over
        // `lair|` plus a `WikiLink` span over the alias a reader sees. A `link:`
        // answers with the target, so the alias is not evidence — and the target
        // span carries the pipe, which is punctuation rather than part of the name.
        if !matches!(
            span.style,
            Style::WikiLink | Style::WikiEmbed | Style::LinkTarget
        ) {
            continue;
        }
        let written = &text[span.start..span.end];
        let target = stem(written.trim().trim_end_matches('|'));
        if target != wanted {
            continue;
        }
        let (line, line_start) = line_at(text, span.start);
        // The target span runs to the pipe in `[[lair|the dark]]`; the pipe is not
        // part of the name, so the highlight stops short of it.
        let mut end = span.end;
        if text[span.start..end].ends_with('|') {
            end -= 1;
        }
        return Some(Evidence {
            line,
            ranges: vec![(span.start - line_start, end - line_start)],
        });
    }
    None
}

/// The name a link points at, without a `.md` the author may or may not have typed.
fn stem(name: &str) -> String {
    name.trim()
        .strip_suffix(".md")
        .unwrap_or(name.trim())
        .to_string()
}

/// `prop:key…` — the frontmatter line that writes the key, with the value brought
/// forward when the query named one.
///
/// `Prop` carries no line number, so the key is what ties an indexed row back to
/// the line that wrote it. A value that is not on the key's own line is looked for
/// on the lines under it, which is where a list's items live:
///
/// ```yaml
/// traits:
///   - sneaky
/// ```
fn prop(text: &str, key: &str, cmp: Cmp, value: Option<&str>) -> Option<Evidence> {
    let block = frontmatter::block(text)?;
    let region = prop_region(text, &block, key)?;
    Some(region.pick(if cmp == Cmp::Has { None } else { value }))
}

/// Where a key is written in the block: its own line, and the indented lines under
/// it, which is where a list's values live.
struct Region<'a> {
    /// The byte the key's line starts at — what makes "which came first" answerable.
    at: usize,
    key_line: &'a str,
    under: Vec<&'a str>,
    /// The key as the query wrote it, for the fallback highlight.
    key_name: String,
    /// The value the note authored under this key, when it authored one. A bound is
    /// not the note's text: `prop:cr<=5` on a note reading `cr: 3` has no `5` in it,
    /// and the honest thing to forward is the `3` that satisfied the bound.
    authored: Option<String>,
}

impl<'a> Region<'a> {
    /// The line to quote, and the part of it to bring forward. The line that
    /// actually holds the value wins; when no line does, the key's own line is
    /// quoted with the key forward, because a highlight on text the note does not
    /// contain is worse than no highlight.
    fn pick(&self, want: Option<&str>) -> Evidence {
        let lines = || std::iter::once(self.key_line).chain(self.under.iter().copied());
        if let Some(want) = want {
            for line in lines() {
                if let Some(found) = find_all(line, want).into_iter().next() {
                    return Evidence {
                        line: line.to_string(),
                        ranges: vec![found],
                    };
                }
            }
        }
        if let Some(authored) = self.authored.as_deref() {
            if !authored.trim().is_empty() {
                for line in lines() {
                    if let Some(found) = find_all(line, authored).into_iter().next() {
                        return Evidence {
                            line: line.to_string(),
                            ranges: vec![found],
                        };
                    }
                }
            }
        }
        let key_at = self.key_line.find(&self.key_name).unwrap_or(0);
        Evidence {
            line: self.key_line.to_string(),
            ranges: vec![(key_at, key_at + self.key_name.len())],
        }
    }
}

fn prop_region<'a>(text: &'a str, block: &frontmatter::Block, key: &str) -> Option<Region<'a>> {
    let (start, end) = block.bytes;
    let mut at = start;
    let mut lines = text[start..end].split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        let written = line.trim_end_matches(['\n', '\r']);
        if authored_key(written) == Some(key) {
            let mut under: Vec<&str> = Vec::new();
            while let Some(next) = lines.peek() {
                let next = next.trim_end_matches(['\n', '\r']);
                if next.trim().is_empty() || authored_key(next).is_some() {
                    break;
                }
                under.push(next);
                lines.next();
            }
            return Some(Region {
                at,
                key_line: written,
                under,
                key_name: key.to_string(),
                authored: block
                    .props
                    .iter()
                    .find(|prop| prop.key == key)
                    .map(|prop| prop.value.clone()),
            });
        }
        at += line.len();
    }
    None
}

/// The key a frontmatter line authors, quotes stripped, or `None` when the line is
/// not a key (a list item, a blank, a fence).
fn authored_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('-') || trimmed.starts_with('#') {
        return None;
    }
    let (key, _) = trimmed.split_once(':')?;
    let key = key.trim().trim_matches('"').trim_matches('\'');
    (!key.is_empty()).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(query: &str) -> Vec<Op> {
        crate::query::parse(query).ops
    }

    fn one(query: &str, text: &str) -> Option<Evidence> {
        evidence(text, &ops(query)[0])
    }

    /// Quote the line, with the highlighted span marked by `[…]`.
    fn shown(ev: &Evidence) -> String {
        let mut out = String::new();
        let mut at = 0;
        for (start, end) in &ev.ranges {
            out.push_str(&ev.line[at..*start]);
            out.push('[');
            out.push_str(&ev.line[*start..*end]);
            out.push(']');
            at = *end;
        }
        out.push_str(&ev.line[at..]);
        out
    }

    #[test]
    fn a_body_tag_is_quoted_with_its_hash() {
        let text = "# Goblin\n\nA #monster of the underdark.\n";
        assert_eq!(
            shown(&one("tag:monster", text).unwrap()),
            "A [#monster] of the underdark."
        );
    }

    /// The rule the whole module rests on: the scanner that built the index finds
    /// the evidence. A `#` in a code fence never entered the `tags` table, so it
    /// cannot be the reason a note is in the list.
    #[test]
    fn a_tag_inside_a_code_fence_is_not_evidence() {
        let text = "Real #monster here.\n\n```\n#monster is not a tag\n```\n";
        let ev = one("tag:monster", text).unwrap();
        assert_eq!(ev.line, "Real #monster here.");
    }

    /// Frontmatter tags feed the same table as body tags, so they are evidence too
    /// — and they are the only evidence when the note has no `#` at all.
    #[test]
    fn a_frontmatter_tag_is_evidence_as_a_frontmatter_line() {
        let text = "---\ntags: character\n---\n\nProse about a character.\n";
        let ev = one("tag:character", text).unwrap();
        assert_eq!(ev.line, "tags: character");
        assert_eq!(shown(&ev), "tags: [character]");
    }

    /// Prose that merely contains the word is not why the note matched. The index
    /// would not have returned this note either, but the finder must not claim it
    /// if some *other* line is the reason.
    #[test]
    fn prose_holding_the_word_is_not_the_reason() {
        let text = "The character sheet is plain prose.\n\nA real tag: #character.\n";
        let ev = one("tag:character", text).unwrap();
        assert_eq!(ev.line, "A real tag: #character.");
    }

    #[test]
    fn a_wikilink_is_quoted_with_its_brackets_around_the_target() {
        let text = "Lives in [[lair]] with allies.\n";
        let ev = one("link:lair", text).unwrap();
        assert_eq!(ev.line, "Lives in [[lair]] with allies.");
        assert_eq!(
            ev.ranges,
            vec![(11, 15)],
            "the target is forwarded, not the punctuation around it"
        );
    }

    /// An aliased link points at the half a reader never sees.
    #[test]
    fn an_aliased_link_evidences_its_target() {
        let text = "Lives in [[lair|the dark]].\n";
        let ev = one("link:lair", text).unwrap();
        assert_eq!(ev.line, "Lives in [[lair|the dark]].");
        assert_eq!(
            ev.ranges,
            vec![(11, 15)],
            "the target, not the alias and not the pipe that follows it"
        );
    }

    /// Naming a note is not linking to it: the M2 ruling that only `[[…]]` is
    /// link-shaped. A `name: Goblin` in a vault holding `Goblin.md` is not a link
    /// to itself, and this must not start quoting it as one.
    #[test]
    fn a_property_that_names_a_note_is_not_link_evidence() {
        let text = "---\nname: Goblin\n---\n\nNo brackets anywhere.\n";
        assert!(one("link:Goblin", text).is_none());
    }

    #[test]
    fn a_prop_line_is_quoted_with_its_value_forward() {
        let text = "---\ncr: 5\nname: Goblin\n---\n";
        assert_eq!(shown(&one("prop:cr<=5", text).unwrap()), "cr: [5]");
        assert_eq!(
            shown(&one("prop:cr", text).unwrap()),
            "cr: [5]",
            "even a bare `prop:cr` forwards what the note actually says under the key"
        );
    }

    /// A list's values live on the lines under its key. The row should quote the
    /// item that answers, not the bare key.
    #[test]
    fn a_list_value_is_found_on_the_line_under_its_key() {
        let text = "---\ntraits:\n  - sneaky\n  - brave\n---\n";
        assert_eq!(
            shown(&one("prop:traits=sneaky", text).unwrap()),
            "  - [sneaky]",
            "the line as authored, indentation and all"
        );
    }

    /// A quoted key is authored with quotes and indexed without them.
    #[test]
    fn a_quoted_key_matches_its_line() {
        let text = "---\n\"Max HP\": 7\n---\n";
        assert_eq!(
            shown(&one("prop:\"Max HP\"<=9", text).unwrap()),
            "\"Max HP\": [7]",
            "the note's own value, not the bound the query typed"
        );
    }

    /// A bound is not the note's text. `prop:cr<=5` on a note reading `cr: 3` has
    /// no `5` anywhere in it, and the honest thing to forward is the `3` that
    /// satisfied the bound.
    #[test]
    fn an_ordering_forwards_the_value_the_note_wrote() {
        let text = "---\ncr: 3\n---\n";
        assert_eq!(shown(&one("prop:cr<=5", text).unwrap()), "cr: [3]");
    }

    /// The index can be ahead of the file: the note was indexed, then edited. The
    /// key's line is still the answer, and the highlight stays on text the note
    /// actually holds rather than on the value the query was looking for.
    #[test]
    fn a_highlight_never_lands_on_text_the_note_does_not_hold() {
        let text = "---\ncr: low\n---\n";
        let ev = one("prop:cr=high", text).unwrap();
        assert_eq!(shown(&ev), "cr: [low]");
    }

    /// The operators whose answer is a fact about the file, not a line in it.
    #[test]
    fn path_and_modified_have_no_text_to_quote() {
        let text = "---\ntags: character\n---\n";
        assert!(one("path:notes/*.md", text).is_none());
        assert!(one("modified:7d", text).is_none());
    }

    /// Document order: when a note tags the same thing twice, the row quotes the
    /// first one the author wrote.
    #[test]
    fn the_first_writing_wins() {
        let text = "---\ntags: monster\n---\n\nAlso #monster down here.\n";
        assert_eq!(one("tag:monster", text).unwrap().line, "tags: monster");
        let text = "Up here: #monster.\n\n---\ntags: monster\n---\n";
        assert_eq!(one("tag:monster", text).unwrap().line, "Up here: #monster.");
    }
}
