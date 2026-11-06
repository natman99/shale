//! The line-level diff behind agent edit review (roadmap P2.5a), and the
//! apply-or-stale patcher that "accept" is.
//!
//! Two jobs, one file, because they are the same fact seen from each side: turning
//! "the note was this, an agent wants it to be that" into hunks a human can read, and
//! then putting one of those hunks back onto whatever the note says *now*.
//!
//! A hunk carries its **unified-diff text** rather than a list of lines. That is what
//! the pending-edit file stores (roadmap P2.5a: readable, git-diffable, a human can
//! open it in any editor), and the patcher parses the same text back. The `@@` header
//! is written for the reader's benefit; the patcher uses the recorded positions and
//! treats the header as decoration.
//!
//! The conflict policy is **apply-or-stale**, ruled for v1: accept patches against
//! the current text, and if the context no longer matches — the user edited the same
//! lines, or two agents proposed over each other — the hunk is *stale*, and the only
//! moves left are reject it or let the agent re-propose. No 3-way merge, and no
//! guessing which side of a conflict the user meant.
//!
//! P4.2's time machine reuses this primitive rather than starting fresh.

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// How many unchanged lines a hunk carries around it. Three is `diff`'s default and
/// what a reader scanning a note expects to see.
pub const CONTEXT: usize = 3;

/// One contiguous change, with the unchanged lines around it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    /// The 1-based line of the note the diff was taken against where this hunk
    /// begins, counting its leading context.
    pub old_start: usize,
    /// How many lines of that note this hunk covers (context and removals).
    pub old_len: usize,
    /// …and the same two for the note the agent proposed.
    pub new_start: usize,
    pub new_len: usize,
    /// Whether the proposed note ended with a newline. A hunk that reaches the end of
    /// the file decides the file's last byte; one in the middle does not, and this is
    /// the only way to know which is which without the whole note.
    pub final_newline: bool,
    /// The unified-diff text: the `@@` header and its ` `, `-` and `+` lines, each
    /// without its own newline. This is what travels in the pending file.
    pub text: String,
}

impl Hunk {
    /// Lines this hunk takes away, for the strip's `−N +M`.
    pub fn removed(&self) -> usize {
        self.text
            .lines()
            .filter(|line| line.starts_with('-'))
            .count()
    }

    /// …and lines it adds.
    pub fn added(&self) -> usize {
        self.text
            .lines()
            .filter(|line| line.starts_with('+'))
            .count()
    }

    /// Move the hunk's recorded position by `lines`, rewriting its `@@` header so the
    /// file still reads true.
    ///
    /// Accepting one hunk changes how many lines come before every hunk after it, and
    /// a pending file that went on naming the old line numbers would describe a note
    /// that no longer exists. This is the *exact* shift the accepted hunk caused —
    /// not a search for a new home, which [`apply`] deliberately never does.
    pub fn shift(&mut self, lines: isize) {
        if lines == 0 {
            return;
        }
        self.old_start = (self.old_start as isize + lines).max(1) as usize;
        self.new_start = (self.new_start as isize + lines).max(1) as usize;
        self.text = rewrite_header(&self.text, self);
    }
}

/// What accepting a hunk can end as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The note with this hunk applied. The caller decides whether to write it.
    Applied(String),
    /// The context no longer matches, so the hunk does not describe this note any
    /// more. Nothing was changed; the strip shows why.
    Stale,
}

/// The hunks that turn `old` into `new`, each carrying [`CONTEXT`] lines of context
/// and merged when they come within [`CONTEXT`] of each other, so a reader sees one
/// change rather than three.
pub fn hunks(old: &str, new: &str) -> Vec<Hunk> {
    let diff = TextDiff::from_lines(old, new);

    // Every line of both notes in one flat sequence, which is what makes the hunk
    // boundaries a question of distance rather than of bookkeeping.
    let flat: Vec<(ChangeTag, String)> = diff
        .iter_all_changes()
        .map(|change| (change.tag(), trim(change.value())))
        .collect();

    // Prefix counts: how many lines of each note come before each flat index. A
    // context or removal line belongs to the old note; a context or addition to the
    // new one.
    let mut old_before = Vec::with_capacity(flat.len() + 1);
    let mut new_before = Vec::with_capacity(flat.len() + 1);
    for (tag, _) in &flat {
        old_before.push(if *tag == ChangeTag::Insert { 0 } else { 1 });
        new_before.push(if *tag == ChangeTag::Delete { 0 } else { 1 });
    }
    running(&mut old_before);
    running(&mut new_before);

    let touched: Vec<usize> = flat
        .iter()
        .enumerate()
        .filter(|(_, (tag, _))| *tag != ChangeTag::Equal)
        .map(|(index, _)| index)
        .collect();

    let mut hunks = Vec::new();
    let mut group: Vec<usize> = Vec::new();
    for index in touched.into_iter().chain(std::iter::once(usize::MAX)) {
        if !group.is_empty() && index > group[group.len() - 1] + 2 * CONTEXT {
            hunks.push(render(&flat, &group, &old_before, &new_before, new));
            group.clear();
        }
        if index == usize::MAX {
            break;
        }
        group.push(index);
    }
    hunks
}

/// The lines of a note as the diff sees them: content without terminators, and
/// whether the text ended with a newline.
pub fn lines(text: &str) -> (Vec<&str>, bool) {
    if text.is_empty() {
        return (Vec::new(), false);
    }
    let final_newline = text.ends_with('\n');
    let body = if final_newline {
        &text[..text.len() - 1]
    } else {
        text
    };
    (body.split('\n').collect(), final_newline)
}

/// Put `hunk` onto `current`, or say it no longer fits.
///
/// The check is the hunk's own context and removal lines, in order, at the position
/// it was recorded at. That is deliberately strict: a hunk that would have to be
/// *searched for* is a hunk whose neighbourhood has already moved, and the ruled
/// policy is to call that stale rather than to guess.
pub fn apply(current: &str, hunk: &Hunk) -> Outcome {
    let (raw, final_newline) = lines(current);
    let Some(fit) = fit(current, hunk) else {
        return Outcome::Stale;
    };
    // Owned lines, because applying a hunk means putting strings the hunk brought
    // into the same sequence as the note's own.
    let mut lines: Vec<String> = raw.into_iter().map(str::to_string).collect();
    let ends_with_newline = if fit.reaches_end {
        hunk.final_newline
    } else {
        final_newline
    };
    lines.splice(fit.start..fit.start + fit.wanted.len(), fit.replacement);
    let mut text = lines.join("\n");
    if ends_with_newline {
        text.push('\n');
    }
    Outcome::Applied(text)
}

/// Where a hunk would land in `current`, as the byte range it rewrites and the text
/// that replaces it.
///
/// [`apply`] answers "what does the whole note say afterwards"; a caller holding a
/// live buffer wants the other question, because rewriting the whole note throws away
/// the cursor, the undo history and everything the user has not saved. This is the
/// same verdict, so the review strip can accept one hunk as one undo step and leave
/// the rest of the note alone. `None` is the same verdict as [`Outcome::Stale`].
pub fn region(current: &str, hunk: &Hunk) -> Option<(usize, usize, String)> {
    let fit = fit(current, hunk)?;
    let starts = line_starts(current);
    // A line past the end of the note starts where the note ends: that is the case of
    // a hunk that only adds, at or after the last line.
    let at = |line: usize| starts.get(line).copied().unwrap_or(current.len());
    let start = at(fit.start);
    let end = at(fit.start + fit.wanted.len());
    // The newline that ended the last replaced line is inside the range whenever a
    // line follows it; at the end of the note the hunk decides how the note ends,
    // exactly as in `apply`.
    let mut text = fit.replacement.join("\n");
    if !fit.reaches_end || hunk.final_newline {
        text.push('\n');
    }
    Some((start, end, text))
}

/// Where one hunk lands in a note the user is looking at (roadmap P2.5b).
///
/// [`apply`] answers "what does the note say afterwards" and [`region`] answers "what
/// bytes does it rewrite"; the inline overlay asks a third question — *which lines are
/// being taken away, and where do the new ones sit*. It cannot answer that from the
/// byte range, because a hunk carries three lines of context on each side and the
/// context is not part of the change: painting the context as though it were would
/// strike through lines the proposal leaves alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The note's 0-based lines this hunk would take away. These are **real lines of
    /// the buffer**, which is what lets the overlay strike the note's own text rather
    /// than draw a copy of it: a removal has nothing phantom about it.
    pub removed: Vec<usize>,
    /// The lines the hunk brings, each group with the 0-based line of the note it sits
    /// **before**. `0` means "above the first line", and a number past the last line
    /// means "at the end". These are the phantom rows: they are not in the buffer, and
    /// nothing that moves through the note — `j`/`k`, selection, search, folds — sees
    /// them.
    ///
    /// A hunk can bring more than one group because a hunk is one *contiguous* change
    /// with its context, and two changes within three lines of each other are merged
    /// into one hunk ([`CONTEXT`]).
    pub added: Vec<(usize, Vec<String>)>,
}

/// Put `hunk` against `current` line by line, or say it no longer fits.
///
/// The verdict is the same one [`apply`] and [`region`] reach — the hunk's own context
/// and removal lines, in order, at the position it was recorded at — because the
/// overlay must not paint a proposal somewhere the accept would not land it. A hunk
/// that is stale has no placement: the strip keeps showing it, badged stale, and the
/// note stays as it is. That is the ruled shape of "strip authoritative, inline
/// opportunistic".
pub fn place(current: &str, hunk: &Hunk) -> Option<Placement> {
    let fit = fit(current, hunk)?;
    let mut line = fit.start;
    let mut removed = Vec::new();
    let mut added: Vec<(usize, Vec<String>)> = Vec::new();
    let mut waiting: Vec<String> = Vec::new();
    for (tag, text) in hunk_body(&hunk.text) {
        match tag {
            // Context is where the note and the proposal agree, so it ends any run of
            // additions: they belong just above this line.
            ' ' => {
                if !waiting.is_empty() {
                    added.push((line, std::mem::take(&mut waiting)));
                }
                line += 1;
            }
            '-' => {
                if !waiting.is_empty() {
                    added.push((line, std::mem::take(&mut waiting)));
                }
                removed.push(line);
                line += 1;
            }
            _ => waiting.push(text),
        }
    }
    if !waiting.is_empty() {
        added.push((line, waiting));
    }
    Some(Placement { removed, added })
}

/// What a hunk expects to find where it says it belongs, and what it brings there.
///
/// [`apply`] and [`region`] are one verdict asked two ways, and both need this: the
/// hunk's context and removal lines, in order, at the position it was recorded at.
/// `None` is the stale verdict — a hunk that would have to be *searched for* is one
/// whose neighbourhood has already moved.
fn fit(current: &str, hunk: &Hunk) -> Option<Fit> {
    let (raw, _) = lines(current);
    let body = hunk_body(&hunk.text);
    if body.is_empty() {
        return None;
    }

    let start = hunk.old_start.saturating_sub(1);
    let wanted: Vec<String> = body
        .iter()
        .filter(|(tag, _)| *tag != '+')
        .map(|(_, line)| line.clone())
        .collect();
    if start > raw.len() || start + wanted.len() > raw.len() {
        return None;
    }
    if !raw[start..start + wanted.len()]
        .iter()
        .copied()
        .eq(wanted.iter().map(String::as_str))
    {
        return None;
    }
    let replacement: Vec<String> = body
        .iter()
        .filter(|(tag, _)| *tag != '-')
        .map(|(_, line)| line.clone())
        .collect();
    let reaches_end = start + wanted.len() >= raw.len();

    Some(Fit {
        start,
        wanted,
        replacement,
        reaches_end,
    })
}

/// One hunk's fit against a note: where it belongs, what it expects, what it brings.
struct Fit {
    /// The 0-based line of the note where the hunk's leading context begins.
    start: usize,
    /// The lines the hunk expects to find there: its context and removal lines.
    wanted: Vec<String>,
    /// The lines the hunk brings: its context and addition lines.
    replacement: Vec<String>,
    /// Whether the hunk reaches the end of the note, which is what decides how the
    /// note ends.
    reaches_end: bool,
}

/// The byte offset each line of a note starts at, in the same view [`lines`] gives:
/// a trailing newline does not start a line of its own.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (offset, c) in text.char_indices() {
        if c == '\n' {
            starts.push(offset + 1);
        }
    }
    if text.ends_with('\n') {
        starts.pop();
    }
    starts
}

/// The ` `, `-` and `+` lines of a hunk's text, in order, with the `@@` header left
/// out.
fn hunk_body(text: &str) -> Vec<(char, String)> {
    text.lines()
        .filter_map(|line| {
            let (tag, rest) = line.split_at(1);
            match tag {
                " " | "-" | "+" => Some((tag.chars().next().unwrap(), rest.to_string())),
                _ => None,
            }
        })
        .collect()
}

/// Turn one group of flat indices into a hunk, widened by [`CONTEXT`] on each side.
fn render(
    flat: &[(ChangeTag, String)],
    group: &[usize],
    old_before: &[usize],
    new_before: &[usize],
    proposed: &str,
) -> Hunk {
    let first = group[0];
    let last = group[group.len() - 1];
    let start = first.saturating_sub(CONTEXT);
    let end = (last + CONTEXT + 1).min(flat.len());

    let old_start = old_before[start] + 1;
    let new_start = new_before[start] + 1;
    let old_len = flat[start..end]
        .iter()
        .filter(|(tag, _)| *tag != ChangeTag::Insert)
        .count();
    let new_len = flat[start..end]
        .iter()
        .filter(|(tag, _)| *tag != ChangeTag::Delete)
        .count();

    let mut text = header(old_start, old_len, new_start, new_len);
    for (tag, line) in &flat[start..end] {
        let marker = match tag {
            ChangeTag::Equal => ' ',
            ChangeTag::Delete => '-',
            ChangeTag::Insert => '+',
        };
        text.push('\n');
        text.push(marker);
        text.push_str(line);
    }

    Hunk {
        old_start,
        old_len,
        new_start,
        new_len,
        final_newline: proposed.ends_with('\n'),
        text,
    }
}

/// The `@@` line of a hunk, which says where it sits in each note. Written for the
/// human reading the pending file: the patcher works from the recorded positions.
fn header(old_start: usize, old_len: usize, new_start: usize, new_len: usize) -> String {
    format!("@@ -{old_start},{old_len} +{new_start},{new_len} @@")
}

/// The `@@` line of `hunk` replaced in place, so a shifted hunk's text and its
/// recorded positions cannot drift apart.
fn rewrite_header(text: &str, hunk: &Hunk) -> String {
    let body = match text.split_once('\n') {
        Some((_, body)) => body,
        None => "",
    };
    if body.is_empty() {
        header(hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len)
    } else {
        format!(
            "{}\n{}",
            header(hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len),
            body
        )
    }
}

/// Turn each prefix entry into the count of entries up to (not including) it.
fn running(prefix: &mut Vec<usize>) {
    let mut total = 0usize;
    for slot in prefix.iter_mut() {
        let add = *slot;
        *slot = total;
        total += add;
    }
    prefix.push(total);
}

/// One trailing newline, which is what `from_lines` keeps on every line.
fn trim(line: &str) -> String {
    line.strip_suffix('\n').unwrap_or(line).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A note with enough lines that context lines are visible rather than the whole
    /// file.
    const NOTE: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n";

    #[test]
    fn the_same_text_has_no_hunks() {
        assert!(hunks(NOTE, NOTE).is_empty());
        assert!(hunks("", "").is_empty());
    }

    #[test]
    fn one_changed_line_is_one_hunk_with_context() {
        let changed = NOTE.replace("five", "FIVE");
        let hunks = hunks(NOTE, &changed);
        assert_eq!(hunks.len(), 1);
        let hunk = &hunks[0];
        assert_eq!(hunk.removed(), 1);
        assert_eq!(hunk.added(), 1);
        // Three lines of context either side, which is four of `one`..`four` and
        // `six`..`eight`: the change is on line 5.
        assert_eq!(hunk.old_start, 2);
        assert_eq!(hunk.old_len, 7);
        assert!(hunk.text.starts_with("@@ -2,7 +2,7 @@"));
        assert!(hunk.text.contains("-five\n+FIVE"));
    }

    #[test]
    fn an_accept_round_trips_to_the_proposed_note() {
        let changed = NOTE.replace("two", "TWO\nTWO-B");
        let hunks = hunks(NOTE, &changed);
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            apply(NOTE, &hunks[0]),
            Outcome::Applied(changed.clone()),
            "accepting every hunk gives the proposed note"
        );
    }

    #[test]
    fn a_new_note_is_one_hunk_applied_to_nothing() {
        let proposed = "# Idea\n\nA thought.\n";
        let hunks = hunks("", proposed);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[0].old_len, 0);
        assert_eq!(hunks[0].added(), 3);
        assert_eq!(apply("", &hunks[0]), Outcome::Applied(proposed.into()));
    }

    #[test]
    fn changes_far_apart_stay_apart_and_near_ones_merge() {
        let far = NOTE.replace("one", "ONE").replace("eight", "EIGHT");
        assert_eq!(
            hunks(NOTE, &far).len(),
            2,
            "lines 1 and 8 cannot share context"
        );

        let near = NOTE.replace("two", "TWO").replace("four", "FOUR");
        assert_eq!(hunks(NOTE, &near).len(), 1, "lines 2 and 4 overlap");
    }

    /// The apply-or-stale rule, which is the whole conflict policy of v1.
    #[test]
    fn a_hunk_whose_context_moved_is_stale_not_guessed() {
        let proposed = NOTE.replace("five", "FIVE");
        let hunk = hunks(NOTE, &proposed).remove(0);

        // The user edited the very line the hunk wants to remove.
        let user_edited = NOTE.replace("five", "five, but mine");
        assert_eq!(apply(&user_edited, &hunk), Outcome::Stale);

        // And a hunk that points past the end of a note that got shorter.
        let shorter = "one\ntwo\n";
        assert_eq!(apply(shorter, &hunk), Outcome::Stale);
    }

    #[test]
    fn a_stale_hunk_changes_nothing() {
        let proposed = NOTE.replace("five", "FIVE");
        let hunk = hunks(NOTE, &proposed).remove(0);
        let untouched = NOTE.replace("five", "mine");
        if let Outcome::Applied(text) = apply(&untouched, &hunk) {
            panic!("it should have been stale, and it changed the note: {text}");
        }
    }

    /// The trailing newline is part of the note, so it must survive both ways.
    #[test]
    fn the_last_byte_of_the_note_survives_a_middle_hunk() {
        let no_final_newline = "one\ntwo\nthree";
        let proposed = "one\nTWO\nthree";
        let hunk = hunks(no_final_newline, proposed).remove(0);
        assert_eq!(
            apply(no_final_newline, &hunk),
            Outcome::Applied(proposed.into())
        );

        // A note that ends without a newline, edited to end with one: the hunk
        // reaches the end, so the proposal decides.
        let add_newline = "one\ntwo\nthree\n";
        let hunk = hunks(no_final_newline, add_newline).remove(0);
        assert_eq!(
            apply(no_final_newline, &hunk),
            Outcome::Applied(add_newline.into())
        );
    }

    #[test]
    fn a_deletion_at_the_end_of_a_note_is_applied() {
        let proposed = "one\ntwo\n";
        let hunk = hunks(NOTE, proposed).remove(0);
        assert_eq!(apply(NOTE, &hunk), Outcome::Applied(proposed.into()));
    }

    #[test]
    fn a_hunk_of_rubbish_is_stale() {
        let hunk = Hunk {
            old_start: 1,
            old_len: 0,
            new_start: 1,
            new_len: 0,
            final_newline: true,
            text: "@@ -1,0 +1,0 @@".to_string(),
        };
        assert_eq!(apply(NOTE, &hunk), Outcome::Stale);
    }

    /// A big note with one small change stays a small hunk: the strip and the file
    /// carry the change, not the note.
    #[test]
    fn a_big_note_still_produces_a_small_hunk() {
        let big: String = (1..=500).map(|n| format!("line {n}\n")).collect();
        let proposed = big.replace("line 250\n", "line 250 edited\n");
        let hunks = hunks(&big, &proposed);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_len, 7);
        assert_eq!(apply(&big, &hunks[0]), Outcome::Applied(proposed));
    }

    /// Splicing every region into the note is the same edit as `apply`, which is what
    /// lets the strip accept a hunk into a live buffer instead of replacing the note.
    #[test]
    fn a_region_spliced_in_is_what_apply_produces() {
        let cases: Vec<(&str, String)> = vec![
            (NOTE, NOTE.replace("five", "FIVE")),
            (NOTE, format!("{NOTE}nine\n")),
            (NOTE, "one\ntwo\n".to_string()),
            (NOTE, String::new()),
            ("", "a brand new note\n".to_string()),
            ("no trailing newline", "no trailing newline".to_string()),
            ("one\ntwo\nthree", "one\nTWO\nthree".to_string()),
        ];
        for (current, proposed) in cases {
            for hunk in hunks(current, &proposed) {
                let (start, end, text) = region(current, &hunk).expect("a fresh hunk has a region");
                let mut patched = String::from(current);
                patched.replace_range(start..end, &text);
                assert_eq!(
                    apply(current, &hunk),
                    Outcome::Applied(patched),
                    "region and apply disagree about {current:?} / {hunk:?}"
                );
            }
        }
    }

    /// The offsets are byte offsets into the note, which is what a buffer edits by.
    #[test]
    fn a_region_is_a_byte_range_and_a_replacement() {
        let hunk = hunks(NOTE, &NOTE.replace("three", "THREE")).remove(0);
        // The hunk carries three lines of context on each side, so it runs from the
        // note's first byte to the start of "seven".
        let (start, end, text) = region(NOTE, &hunk).unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 28, "…through the newline that ends six");
        assert_eq!(
            text, "one\ntwo\nTHREE\nfour\nfive\nsix\n",
            "the replacement is the hunk's context and additions"
        );
    }

    #[test]
    fn a_stale_hunk_has_no_region() {
        let hunk = hunks(NOTE, &NOTE.replace("five", "FIVE")).remove(0);
        let walked_over = NOTE.replace("five", "five, mine");
        assert_eq!(apply(&walked_over, &hunk), Outcome::Stale);
        assert_eq!(region(&walked_over, &hunk), None);
    }

    /// A hunk that only adds, at the end of a note, still carries its context, and
    /// the region it names is the context it rewrites itself against.
    #[test]
    fn an_addition_at_the_end_replaces_its_own_context() {
        let proposed = format!("{NOTE}nine\n");
        let hunk = hunks(NOTE, &proposed).remove(0);
        let (start, end, text) = region(NOTE, &hunk).unwrap();
        assert!(
            start < end,
            "the hunk's context is rewritten with itself in it"
        );
        assert_eq!(end, NOTE.len(), "…and it runs to the end of the note");
        assert!(text.ends_with("six\nseven\neight\nnine\n"), "{text}");
        let mut patched = NOTE.to_string();
        patched.replace_range(start..end, &text);
        assert_eq!(patched, proposed);
    }

    /// Accepting one hunk moves every hunk after it, and the header moves with the
    /// recorded position so the file keeps reading true.
    #[test]
    fn a_shifted_hunk_moves_its_header_with_its_position() {
        // Far enough apart that the two changes are two hunks rather than one.
        let long: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        let proposed = long
            .replace("line 2\n", "TWO\nTWO-2\n")
            .replace("line 18\n", "SEVEN\n");
        let mut hunks = hunks(&long, &proposed);
        assert_eq!(hunks.len(), 2, "one change near the top, one near the end");
        let (_, rest) = hunks.split_first_mut().unwrap();
        let second = &mut rest[0];
        let before = (second.old_start, second.new_start);
        // The first hunk replaces 1 line with 2, so everything after it moves down 1.
        second.shift(1);
        assert_eq!(second.old_start, before.0 + 1);
        assert_eq!(second.new_start, before.1 + 1);
        assert!(
            second.text.starts_with(&format!(
                "@@ -{},{} +{},{} @@",
                second.old_start, second.old_len, second.new_start, second.new_len
            )),
            "the header says what the fields say: {:?}",
            second.text.lines().next().unwrap()
        );
        assert_eq!(
            second.text.lines().count(),
            second.old_len + second.added() + 1,
            "shifting kept the body"
        );
    }

    /// The inline overlay's question (P2.5b): which of the note's own lines does this
    /// proposal take away, and where do its new lines sit.
    #[test]
    fn a_placement_names_the_lines_and_where_the_new_ones_sit() {
        let proposed = NOTE.replace("five", "FIVE");
        let hunk = hunks(NOTE, &proposed).remove(0);
        let at = place(NOTE, &hunk).expect("fresh, so it places");
        assert_eq!(at.removed, vec![4], "line 5 of the note, counted from 0");
        assert_eq!(
            at.added,
            vec![(5, vec!["FIVE".to_string()])],
            "the new line sits above line index 5, i.e. where the old one was"
        );
    }

    /// A hunk carries three lines of context on each side. The overlay must not paint
    /// those: they are the part the proposal agrees to leave alone, and striking them
    /// would tell the user the agent means to rewrite lines it does not touch.
    #[test]
    fn a_placement_leaves_the_context_out() {
        let proposed = NOTE.replace("five", "FIVE");
        let hunk = hunks(NOTE, &proposed).remove(0);
        assert!(hunk.text.contains(" three"), "the hunk does carry context");
        let at = place(NOTE, &hunk).unwrap();
        for line in [2, 3, 5, 6] {
            assert!(
                !at.removed.contains(&line),
                "line {line} is context, not a removal"
            );
        }
    }

    #[test]
    fn an_insertion_removes_nothing() {
        let proposed = NOTE.replace("four\n", "four\nFOUR-AGAIN\n");
        let hunk = hunks(NOTE, &proposed).remove(0);
        let at = place(NOTE, &hunk).unwrap();
        assert!(at.removed.is_empty(), "nothing is taken away");
        assert_eq!(at.added, vec![(4, vec!["FOUR-AGAIN".to_string()])]);
    }

    #[test]
    fn a_deletion_bringing_nothing_places_empty_additions() {
        let proposed = NOTE.replace("five\n", "");
        let hunk = hunks(NOTE, &proposed).remove(0);
        let at = place(NOTE, &hunk).unwrap();
        assert_eq!(at.removed, vec![4]);
        assert!(at.added.is_empty(), "the proposal brings no lines");
    }

    /// Two changes close enough to share a hunk are still two changes: the overlay
    /// paints both, and painting only the first would hide half a proposal.
    #[test]
    fn one_hunk_can_change_in_two_places() {
        let proposed = NOTE.replace("two", "TWO").replace("four", "FOUR");
        let hunk = hunks(NOTE, &proposed).remove(0);
        assert_eq!(hunk.added(), 2, "one hunk, two changes");
        let at = place(NOTE, &hunk).unwrap();
        assert_eq!(at.removed, vec![1, 3]);
        assert_eq!(
            at.added,
            vec![(2, vec!["TWO".to_string()]), (4, vec!["FOUR".to_string()])]
        );
    }

    /// A proposal against a note that does not exist yet places at the top of an empty
    /// buffer, which is what lets a new note's whole text show as phantom lines.
    #[test]
    fn a_new_note_places_at_the_top_of_nothing() {
        let proposed = "# Idea\n\nA thought.\n";
        let hunk = hunks("", proposed).remove(0);
        let at = place("", &hunk).expect("an empty note is what a new note expects");
        assert!(at.removed.is_empty());
        assert_eq!(
            at.added,
            vec![(0, vec!["# Idea".into(), String::new(), "A thought.".into()])]
        );
    }

    /// A stale hunk has no placement. The strip still lists it, badged; the note is
    /// left exactly as it is, because an overlay painting a change the accept would
    /// refuse is worse than no overlay.
    #[test]
    fn a_stale_hunk_places_nowhere() {
        let proposed = NOTE.replace("five", "FIVE");
        let hunk = hunks(NOTE, &proposed).remove(0);
        assert_eq!(place(&NOTE.replace("five", "five, mine"), &hunk), None);
        assert_eq!(place("one\ntwo\n", &hunk), None);
    }

    /// Additions at the end of the note are anchored past the last line, which is how
    /// the overlay knows to hang them below everything rather than above something.
    #[test]
    fn an_addition_at_the_end_is_anchored_past_the_last_line() {
        let proposed = format!("{NOTE}P.S.\n");
        let hunk = hunks(NOTE, &proposed).remove(0);
        let at = place(NOTE, &hunk).unwrap();
        assert!(at.removed.is_empty());
        assert_eq!(at.added, vec![(8, vec!["P.S.".to_string()])]);
    }
}
