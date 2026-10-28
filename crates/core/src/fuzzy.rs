//! Fuzzy matching for the pickers: quick-open, the command palette and search.
//!
//! Deliberately small. A query matches a candidate when its characters appear in
//! order (a subsequence), and the score prefers the matches a reader would have
//! predicted: the start of the candidate, the start of a word inside it, and runs
//! of adjacent characters. That is enough to make `nb` find `Note Book` and
//! `shale.toml` find `notes/Shopping list.md` only when there is nothing better.
//! Matching folds case but not accents: `cafe` does not find `café`.

/// A match at the very start of the candidate.
const START_BONUS: i64 = 12;
/// A match that begins a word, i.e. follows one of [`WORD_STARTS`].
const WORD_BONUS: i64 = 9;
/// A query character that continues the run the previous one started.
const RUN_BONUS: i64 = 4;
/// Every query character that matched at all.
const CHAR_SCORE: i64 = 1;
/// A candidate the query matches exactly, which should beat any subsequence of a
/// longer candidate.
const EXACT_BONUS: i64 = 40;
/// Each character skipped between the first match and the last one.
const GAP_PENALTY: i64 = 1;
/// A long candidate is a worse answer than a short one that matched just as
/// well, so its extra length is charged at one point per this many characters.
const LENGTH_PENALTY_DIVISOR: i64 = 8;

/// The characters that begin a word in a path, a title or an action name.
const WORD_STARTS: &[char] = &[' ', '/', '-', '_', '.', '(', '[', ':'];

/// One fuzzy match: how good it is, and which parts of the candidate the query
/// consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    /// Higher is better.
    pub score: i64,
    /// The byte ranges of the candidate that matched, in order — what a picker
    /// highlights. Each range is one character long, so a multi-byte character
    /// gets a range that covers all of it.
    pub ranges: Vec<(usize, usize)>,
}

/// Match `query` against `candidate`, case-insensitively, as a subsequence.
///
/// Greedy but not naive: for each query character the scan takes the first
/// position that begins a word, and falls back to the first position that matches
/// at all. That is what lets `nb` reach the `B` of `Note Book` instead of stopping
/// at the `b` of `Noble`. Returns `None` when the characters do not appear in
/// order.
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<FuzzyMatch> {
    let query: Vec<char> = query.chars().map(lower).collect();
    if query.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            ranges: Vec::new(),
        });
    }
    let chars: Vec<char> = candidate.chars().map(lower).collect();
    // Byte offset of each character, so the ranges are usable on `candidate`.
    let mut offsets = Vec::with_capacity(chars.len());
    let mut byte = 0;
    for c in candidate.chars() {
        offsets.push(byte);
        byte += c.len_utf8();
    }

    let mut taken: Vec<usize> = Vec::with_capacity(query.len());
    let mut from = 0;
    for &q in &query {
        let mut first = None;
        let mut chosen = None;
        let mut i = from;
        while i < chars.len() {
            if chars[i] == q {
                if first.is_none() {
                    first = Some(i);
                }
                if i > 0 && WORD_STARTS.contains(&chars[i - 1]) {
                    chosen = Some(i);
                    break;
                }
            }
            i += 1;
        }
        let i = chosen.or(first)?;
        taken.push(i);
        from = i + 1;
    }

    let mut score = 0;
    for (n, &i) in taken.iter().enumerate() {
        score += CHAR_SCORE;
        if i == 0 {
            score += START_BONUS;
        } else if WORD_STARTS.contains(&chars[i - 1]) {
            score += WORD_BONUS;
        }
        if n > 0 && taken[n - 1] + 1 == i {
            score += RUN_BONUS;
        }
    }
    let span = taken.last().unwrap() - taken.first().unwrap() + 1;
    score -= (span - query.len()) as i64 * GAP_PENALTY;
    score -= (chars.len() - query.len()) as i64 / LENGTH_PENALTY_DIVISOR;
    if chars.len() == query.len() {
        score += EXACT_BONUS;
    }

    let ranges = taken
        .iter()
        .map(|&i| {
            let start = offsets[i];
            (
                start,
                start + candidate[start..].chars().next().unwrap().len_utf8(),
            )
        })
        .collect();
    Some(FuzzyMatch { score, ranges })
}

/// The best-scoring items first, at most `limit` of them, as `(match, index)`
/// pairs into `items`. Items that do not match are dropped, and equal scores keep
/// the order they arrived in, so a caller that passes notes in path order gets a
/// predictable list.
pub fn ranked<T, K>(query: &str, items: &[T], limit: usize, key: K) -> Vec<(FuzzyMatch, usize)>
where
    K: Fn(&T) -> &str,
{
    let mut scored: Vec<(FuzzyMatch, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| fuzzy_match(query, key(item)).map(|m| (m, idx)))
        .collect();
    // A stable sort keeps equal scores in the order the caller passed them.
    scored.sort_by_key(|(matched, _)| std::cmp::Reverse(matched.score));
    scored.truncate(limit);
    scored
}

/// One way down for a character: enough for the ASCII and Latin-1 case folding
/// the pickers need, without pretending to full Unicode (accents are left as
/// they are, and a character whose lowercase is two characters keeps its first).
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_that_is_not_a_subsequence_does_not_match() {
        assert!(fuzzy_match("xyz", "Welcome").is_none());
        // The characters are all there, but not in order.
        assert!(fuzzy_match("meW", "Welcome").is_none());
    }

    #[test]
    fn an_empty_query_matches_everything_and_highlights_nothing() {
        let m = fuzzy_match("", "Welcome").unwrap();
        assert_eq!(m.score, 0);
        assert!(m.ranges.is_empty());
    }

    #[test]
    fn matching_ignores_case() {
        let m = fuzzy_match("wel", "Welcome.md").unwrap();
        assert_eq!(
            m.ranges,
            vec![(0, 1), (1, 2), (2, 3)],
            "the ranges point at the lowercase letters in the candidate"
        );
    }

    #[test]
    fn a_word_start_beats_a_letter_in_the_middle_of_a_word() {
        let m = fuzzy_match("nb", "Note Book").unwrap();
        assert_eq!(
            m.ranges,
            vec![(0, 1), (5, 6)],
            "`nb` reaches the initials rather than the `b` inside `Note`"
        );
    }

    #[test]
    fn a_folder_or_word_boundary_counts_as_the_start_of_a_word() {
        let m = fuzzy_match("sl", "notes/Shopping list.md").unwrap();
        assert_eq!(m.ranges[0], (6, 7), "`s` is the start of `Shopping`");
        assert_eq!(m.ranges[1], (15, 16), "`l` is the start of `list`");
    }

    #[test]
    fn adjacent_letters_beat_the_same_letters_spread_out() {
        let together = fuzzy_match("no", "Note").unwrap();
        let apart = fuzzy_match("no", "Nonsense of note").unwrap();
        assert!(
            together.score > apart.score,
            "{together:?} should beat {apart:?}"
        );
    }

    #[test]
    fn an_exact_match_beats_a_subsequence_of_something_longer() {
        let exact = fuzzy_match("Welcome", "Welcome").unwrap();
        let longer = fuzzy_match("Welcome", "Welcome to the archive").unwrap();
        assert!(exact.score > longer.score, "{exact:?} vs {longer:?}");
    }

    #[test]
    fn a_shorter_candidate_beats_a_longer_one_matched_the_same_way() {
        let short = fuzzy_match("idea", "idea").unwrap();
        let long = fuzzy_match("idea", "idea-collection-of-things").unwrap();
        assert!(short.score > long.score, "{short:?} vs {long:?}");
    }

    #[test]
    fn ranges_are_byte_ranges_so_a_multibyte_letter_is_whole() {
        let m = fuzzy_match("é", "café note").unwrap();
        // `é` is two bytes, and the range that covers it has to cover both.
        assert_eq!(m.ranges, vec![(3, 5)]);
        let m = fuzzy_match("no", "café note").unwrap();
        assert_eq!(
            m.ranges,
            vec![(6, 7), (7, 8)],
            "and the offsets after it stay in bytes, not characters"
        );
    }

    #[test]
    fn ranked_orders_by_score_and_cuts_to_the_limit() {
        let notes = ["Welcome.md", "notes/Idea.md", "notes/Ideas for later.md"];
        let top = ranked("idea", &notes, 2, |path| path);
        assert_eq!(top.len(), 2, "the limit is respected");
        assert_eq!(notes[top[0].1], "notes/Idea.md");
        assert_eq!(notes[top[1].1], "notes/Ideas for later.md");
        assert!(
            ranked("zzzz", &notes, 10, |path| path).is_empty(),
            "a query nothing matches ranks nothing"
        );
    }
}
