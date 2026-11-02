//! The path patterns an agent profile is written with (roadmap P2.4).
//!
//! Profiles say which parts of a vault an agent may touch, in the vocabulary a human
//! would write by hand: a folder, a note, `inbox/**`, `*.md`. So the matcher is small
//! and its rules are stated here rather than left to a crate with its own dialect.
//!
//! The vocabulary, over **vault-relative** paths:
//!
//! - A pattern with **no wildcards** is a *prefix*: it covers that path and anything
//!   under it. `journal` covers `journal` and `journal/2026/february.md`, which is
//!   what someone means when they write `deny = ["journal"]`. It does not cover
//!   `journaling.md` — the boundary is a path separator, not a character.
//! - A pattern ending in `/` is the same thing, spelled the other way: `inbox/`.
//! - `*` covers any run of characters **within one path segment**, so `*.md` covers
//!   `idea.md` but not `notes/idea.md`.
//! - `**` covers any number of whole segments, so `notes/**` covers `notes/a.md` and
//!   `notes/deep/a.md`, and `**` alone covers the vault. A trailing `**` needs at
//!   least one segment, which is why `notes/**` does not cover a note named `notes`.
//! - `?` covers exactly one character within a segment.
//!
//! Matching is case-sensitive, because the vault's paths are: on Linux `Notes/` and
//! `notes/` are different folders, and a permission check that quietly decided they
//! were the same would be a way to read the wrong one.

use std::path::Path;

/// Does `pattern` cover `path`?
///
/// `path` is vault-relative, the same form [`crate::Vault`] hands out and the same
/// form a profile's patterns are written in, so no conversion happens between what
/// the user typed and what is being tested.
pub fn covers(pattern: &str, path: &Path) -> bool {
    let pattern = pattern.trim().trim_end_matches('/');
    if pattern.is_empty() {
        // An empty pattern names nothing, so it covers nothing: a stray `""` in a
        // profile must not turn into "the whole vault".
        return false;
    }
    let text = path.display().to_string().replace('\\', "/");
    let text = text.trim_end_matches('/');

    if !pattern.contains(['*']) && !pattern.contains('?') {
        return text == pattern || text.starts_with(&format!("{pattern}/"));
    }
    matches_segments(
        &pattern.split('/').collect::<Vec<_>>(),
        &text
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>(),
    )
}

/// The segment-level walk: `**` may eat any number of path segments, anything else
/// takes exactly one.
fn matches_segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.first().copied() {
        None => path.is_empty(),
        Some("**") => {
            let rest = &pattern[1..];
            (0..=path.len()).any(|eaten| {
                // A trailing `**` is "everything inside", which is nothing if it
                // covers no segment at all — so `notes/**` stops at `notes/`.
                !(rest.is_empty() && eaten == 0) && matches_segments(rest, &path[eaten..])
            })
        }
        Some(segment) => {
            !path.is_empty()
                && segment_matches(segment, path[0])
                && matches_segments(&pattern[1..], &path[1..])
        }
    }
}

/// One segment against one pattern segment: `*` is any run of characters, `?` is one.
fn segment_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    matches_from(&pattern, 0, &text, 0)
}

fn matches_from(pattern: &[char], mut pi: usize, text: &[char], mut ti: usize) -> bool {
    loop {
        match pattern.get(pi) {
            None => return text.len() == ti,
            Some('*') => {
                // The star takes as much as it needs; whatever is left has to match
                // what is left. Trying the shortest first keeps the common
                // `*.md` case cheap.
                return (ti..=text.len())
                    .any(|taken| matches_from(pattern, pi + 1, text, ti + taken));
            }
            Some('?') => {
                if ti >= text.len() {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
            Some(want) => {
                if text.get(ti) != Some(want) {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers(pattern: &str, path: &str) -> bool {
        super::covers(pattern, Path::new(path))
    }

    /// A pattern with no wildcards is a prefix with a separator boundary: the folder
    /// and everything in it, and never a folder whose name merely starts the same.
    #[test]
    fn a_plain_pattern_is_a_path_prefix() {
        assert!(covers("journal", "journal"));
        assert!(covers("journal", "journal/2026/february.md"));
        assert!(!covers("journal", "journaling.md"));
        assert!(!covers("journal", "journals/february.md"));
        assert!(!covers("journal", "notes/journal.md"));
    }

    #[test]
    fn a_trailing_slash_spells_the_same_folder() {
        assert!(covers("inbox/", "inbox"));
        assert!(covers("inbox/", "inbox/idea.md"));
        assert!(covers("inbox/", "inbox/deep/idea.md"));
        assert!(!covers("inbox/", "drafts/idea.md"));
    }

    #[test]
    fn star_stays_inside_one_segment() {
        assert!(covers("*.md", "idea.md"));
        assert!(!covers("*.md", "notes/idea.md"));
        assert!(covers("notes/*.md", "notes/idea.md"));
        assert!(!covers("notes/*.md", "notes/deep/idea.md"));
        assert!(covers("a?.md", "ab.md"));
        assert!(!covers("a?.md", "a.md"));
        assert!(!covers("?a.md", "/a.md"));
    }

    #[test]
    fn double_star_walks_whole_segments() {
        assert!(covers("**", "idea.md"));
        assert!(covers("**", "notes/deep/idea.md"));
        assert!(covers("notes/**", "notes/a.md"));
        assert!(covers("notes/**", "notes/deep/a.md"));
        assert!(
            !covers("notes/**", "notes"),
            "a trailing `**` needs a segment"
        );
        assert!(!covers("notes/**", "inbox/a.md"));
        assert!(covers("**/*.md", "a.md"));
        assert!(covers("**/*.md", "notes/deep/a.md"));
        assert!(
            covers("a/**/b.md", "a/b.md"),
            "`**` may eat nothing in the middle"
        );
        assert!(covers("a/**/b.md", "a/x/y/b.md"));
    }

    /// The failure that matters for a permission check: nothing accidental ever
    /// matches everything.
    #[test]
    fn an_empty_pattern_covers_nothing() {
        assert!(!covers("", "a.md"));
        assert!(!covers("   ", "a.md"));
    }

    #[test]
    fn patterns_are_trimmed_and_paths_are_compared_as_written() {
        assert!(covers("  inbox/  ", "inbox/a.md"));
        assert!(
            !covers("Notes", "notes/a.md"),
            "case-sensitive, like the vault"
        );
    }
}
