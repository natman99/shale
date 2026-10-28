//! Fuzzy matching for the pickers: quick-open, the command palette and search.
//!
//! Deliberately small. A query matches a candidate when its characters appear in
//! order (a subsequence), and the score prefers the matches a reader would have
//! predicted: the start of the candidate, the start of a word inside it, and runs
//! of adjacent characters. That is enough to make `nb` find `Note Book` and
//! `shale.toml` find `notes/Shopping list.md` only when there is nothing better.
//! The best placement of the query is found by dynamic programming, so the score
//! is the best available rather than what a left-to-right scan happens to pick.
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
/// The placement that scores best wins: the start of the candidate, a word start
/// and adjacent runs are worth more, and the characters skipped between matches
/// cost. Returns `None` when the characters do not appear in order.
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

    let (mut score, taken) = best_alignment(&query, &chars)?;
    // A long candidate is a worse answer than a short one that matched just as
    // well. This term does not depend on where the characters went, so it sits
    // outside the alignment.
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

/// The highest-scoring way to place `query` inside `chars` as a subsequence, as
/// that score and the candidate index each query character landed on.
///
/// Dynamic programming rather than a scan, for two reasons. A greedy scan can
/// fail outright: `new note` against `New note ctrl-n` takes the `n` that starts
/// *note* because a word start is worth more, then finds no `w` after it and
/// reports no match at all. And a greedy scan is not even the best-scoring
/// choice: the adjacent run `New` is worth more than jumping to a word start.
///
/// `row[ci]` is the best score for the query prefix so far that ends with that
/// prefix's character on candidate index `ci`. Extending an alignment by one
/// character either continues a run (worth [`RUN_BONUS`]) or skips characters
/// (one [`GAP_PENALTY`] each), so the best predecessor is the best of those two,
/// and the second is kept as a running maximum instead of rescanning.
fn best_alignment(query: &[char], chars: &[char]) -> Option<(i64, Vec<usize>)> {
    /// Low enough that no real alignment can lose to it, high enough that adding
    /// a bonus to it cannot look like a score.
    const NEG: i64 = i64::MIN / 4;

    let mut row: Vec<i64> = vec![NEG; chars.len()];
    // For each query character, the index the previous one landed on for the best
    // alignment that ends at each candidate index: how the alignment is walked
    // back once the last character is placed.
    let mut parents: Vec<Vec<usize>> = Vec::with_capacity(query.len());

    for (qi, &q) in query.iter().enumerate() {
        let mut next: Vec<i64> = vec![NEG; chars.len()];
        let mut parent: Vec<usize> = vec![usize::MAX; chars.len()];
        // The best `row[prev] + prev * GAP_PENALTY` over the candidate indices at
        // least two before the one being scored. Adding `-(ci + 1) * GAP_PENALTY`
        // to it turns that into the gap penalty between `prev` and `ci`.
        let mut running = NEG;
        let mut running_at = usize::MAX;
        for (ci, &c) in chars.iter().enumerate() {
            if c == q {
                let base = CHAR_SCORE
                    + if ci == 0 {
                        START_BONUS
                    } else if WORD_STARTS.contains(&chars[ci - 1]) {
                        WORD_BONUS
                    } else {
                        0
                    };
                if qi == 0 {
                    next[ci] = base;
                } else {
                    if ci > 0 && row[ci - 1] > NEG {
                        next[ci] = row[ci - 1] + base + RUN_BONUS;
                        parent[ci] = ci - 1;
                    }
                    if running > NEG {
                        let score = running + base - (ci + 1) as i64 * GAP_PENALTY;
                        if score > next[ci] {
                            next[ci] = score;
                            parent[ci] = running_at;
                        }
                    }
                }
            }
            // Fold this index in only after scoring it, so it is available to the
            // indices after the next one and never to its immediate neighbour,
            // which is the run case above.
            if row[ci] > NEG {
                let value = row[ci] + ci as i64 * GAP_PENALTY;
                if value > running {
                    running = value;
                    running_at = ci;
                }
            }
        }
        row = next;
        parents.push(parent);
    }

    let mut at = usize::MAX;
    let mut score = NEG;
    for (ci, &placed) in row.iter().enumerate() {
        if placed > score {
            score = placed;
            at = ci;
        }
    }
    if at == usize::MAX {
        return None;
    }
    let mut taken = vec![0usize; query.len()];
    for qi in (0..query.len()).rev() {
        taken[qi] = at;
        at = parents[qi][at];
    }
    Some((score, taken))
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
    fn a_query_with_a_space_still_finds_the_words_it_spells() {
        // The left-to-right scan this replaced took the word-start `n` of `note`
        // because a word start is worth more, then had no `w` left to find and
        // reported that the row did not match at all.
        let m = fuzzy_match("new note", "New note ctrl-n, n in the tree").unwrap();
        assert_eq!(
            m.ranges,
            (0..8).map(|i| (i, i + 1)).collect::<Vec<_>>(),
            "the candidate's first eight characters spell the query"
        );
    }

    #[test]
    fn the_best_scoring_placement_wins_rather_than_the_first_available_one() {
        let m = fuzzy_match("ne", "New note").unwrap();
        assert_eq!(
            m.ranges,
            vec![(0, 1), (1, 2)],
            "the adjacent `Ne` is worth more than jumping to the `e` of `note`"
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
