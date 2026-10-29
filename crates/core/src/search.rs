use anyhow::Result;

use crate::vault::{Vault, VaultEntry};

/// One matching line in a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub path: std::path::PathBuf,
    /// 1-based line number.
    pub line_number: usize,
    pub line_text: String,
}

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Match whole words only.
    pub whole_word: bool,
    /// Case-sensitive matching (default is case-insensitive).
    pub case_sensitive: bool,
    /// Limit the number of hits returned.
    pub max_hits: Option<usize>,
}

/// Search all markdown notes in the vault for `query`, line by line.
pub fn search(
    vault: &Vault,
    entries: &[VaultEntry],
    query: &str,
    opts: &SearchOptions,
) -> Result<Vec<SearchHit>> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut hits = Vec::new();
    for entry in entries {
        let text = vault.read(&entry.path).unwrap_or_default();
        for (idx, line) in text.lines().enumerate() {
            if line_matches(line, query, opts) {
                hits.push(SearchHit {
                    path: entry.path.clone(),
                    line_number: idx + 1,
                    line_text: line.to_string(),
                });
                if let Some(max) = opts.max_hits {
                    if hits.len() >= max {
                        return Ok(hits);
                    }
                }
            }
        }
    }
    Ok(hits)
}

fn line_matches(line: &str, query: &str, opts: &SearchOptions) -> bool {
    if opts.case_sensitive {
        if opts.whole_word {
            contains_whole_word(line, query, true)
        } else {
            line.contains(query)
        }
    } else {
        let lower_line = line.to_lowercase();
        let lower_query = query.to_lowercase();
        if opts.whole_word {
            contains_whole_word(&lower_line, &lower_query, false)
        } else {
            lower_line.contains(&lower_query)
        }
    }
}

fn contains_whole_word(haystack: &str, needle: &str, case_sensitive: bool) -> bool {
    let (h, n) = if case_sensitive {
        (haystack.to_string(), needle.to_string())
    } else {
        (haystack.to_lowercase(), needle.to_lowercase())
    };
    let mut from = 0;
    while let Some(pos) = h[from..].find(&n) {
        let abs = from + pos;
        let before_ok = abs == 0
            || !h[..abs]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        let after = &h[abs + n.len()..];
        let after_ok = after.is_empty()
            || !after
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if before_ok && after_ok {
            return true;
        }
        from = abs + 1;
    }
    false
}

/// Every occurrence of `query` in `text`, as byte ranges — the in-note search
/// behind `/`.
///
/// Literal rather than fuzzy: `/` is vim's search, and a query that matched by
/// subsequence would move the cursor to somewhere the query is not. Case follows
/// vim's "smart case": an all-lowercase query matches either case, a query with a
/// capital in it matches exactly. Matches never overlap, which is what stepping
/// with `n` needs — `aa` in `aaaa` is two hits, not three.
///
/// The scan walks characters rather than searching a case-folded copy, because
/// folding can change a string's byte length (`İ` folds to two characters) and
/// these ranges have to be byte offsets into the note.
pub fn find_all(text: &str, query: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    if query.is_empty() {
        return found;
    }
    let fold = !query.chars().any(|c| c.is_uppercase());
    let mut at = 0usize;
    while at < text.len() {
        match match_at(text, at, query, fold) {
            Some(end) => {
                found.push((at, end));
                at = end;
            }
            None => {
                // One character forward, and always to a character boundary.
                at += text[at..].chars().next().map_or(1, |c| c.len_utf8());
            }
        }
    }
    found
}

/// The byte just past `at` when `query` starts there, or `None`.
fn match_at(text: &str, at: usize, query: &str, fold: bool) -> Option<usize> {
    let mut used = 0usize;
    for want in query.chars() {
        let have = text[at + used..].chars().next()?;
        // Folding compares the lowercased characters, which covers the ordinary
        // case; a fold that changes length (`ß`) is not chased, and the range
        // stays the note's own bytes either way.
        if have != want && !(fold && have.to_lowercase().eq(want.to_lowercase())) {
            return None;
        }
        used += have.len_utf8();
    }
    Some(at + used)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/` in the open note: every occurrence, as byte ranges.
    #[test]
    fn find_all_reports_every_occurrence() {
        assert_eq!(
            find_all("Hello World hello", "hello"),
            vec![(0, 5), (12, 17)]
        );
        assert_eq!(find_all("nothing here", "else"), Vec::new());
        assert_eq!(find_all("anything", ""), Vec::new());
    }

    /// vim's "smart case": an all-lowercase query matches either case, and a
    /// query with a capital in it matches exactly.
    #[test]
    fn find_all_folds_case_only_for_a_lowercase_query() {
        assert_eq!(find_all("Hello World", "world"), vec![(6, 11)]);
        assert_eq!(find_all("hello world", "World"), Vec::new());
        assert_eq!(find_all("hello world", "world"), vec![(6, 11)]);
    }

    /// Stepping with `n` needs matches that never overlap: `aa` in `aaaa` is two
    /// hits, not the three a naive scan would report.
    #[test]
    fn find_all_matches_never_overlap() {
        assert_eq!(find_all("aaaa", "aa"), vec![(0, 2), (2, 4)]);
    }

    /// The ranges are byte offsets into the note, so a multi-byte letter must not
    /// shift them, and folding must not invent a length change.
    #[test]
    fn find_all_counts_bytes_not_characters() {
        assert_eq!(find_all("naïve Naïve", "naïve"), vec![(0, 6), (7, 13)]);
        assert_eq!(find_all("Ähnlich", "ähnlich"), vec![(0, 8)]);
    }

    fn fixture() -> Result<(tempfile::TempDir, Vault, Vec<VaultEntry>)> {
        let dir = tempfile::tempdir()?;
        let vault = Vault::open(dir.path())?;
        vault.write("a.md", "Hello World\nhello again\nGoodbye")?;
        vault.write("b.md", "Well hello there\nHelloWorld is one word")?;
        let entries = vault.list_markdown()?;
        Ok((dir, vault, entries))
    }

    #[test]
    fn case_insensitive_by_default() -> Result<()> {
        let (_d, vault, entries) = fixture()?;
        let hits = search(&vault, &entries, "hello", &SearchOptions::default())?;
        assert_eq!(hits.len(), 4);
        assert_eq!(hits[0].line_number, 1);
        Ok(())
    }

    #[test]
    fn case_sensitive() -> Result<()> {
        let (_d, vault, entries) = fixture()?;
        let opts = SearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        let hits = search(&vault, &entries, "hello", &opts)?;
        assert_eq!(hits.len(), 2);
        Ok(())
    }

    #[test]
    fn whole_word() -> Result<()> {
        let (_d, vault, entries) = fixture()?;
        let opts = SearchOptions {
            whole_word: true,
            ..Default::default()
        };
        let hits = search(&vault, &entries, "hello", &opts)?;
        // Should not match "HelloWorld".
        assert_eq!(hits.len(), 3);
        Ok(())
    }

    #[test]
    fn max_hits_respected() -> Result<()> {
        let (_d, vault, entries) = fixture()?;
        let opts = SearchOptions {
            max_hits: Some(2),
            ..Default::default()
        };
        let hits = search(&vault, &entries, "hello", &opts)?;
        assert_eq!(hits.len(), 2);
        Ok(())
    }
}
