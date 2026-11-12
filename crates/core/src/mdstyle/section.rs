//! The structure acts: moving a section among its siblings, and adding or removing a
//! level from a heading and everything nested under it (roadmap P4.13 M2,
//! sidebar-plan.md rulings 9 and 10).
//!
//! Both acts are **pure**: they take the note's text and answer with the byte-range
//! replacements that would do the job, or with the reason the text will not do it. The
//! window applies the replacements as one grouped edit, so a single `u` undoes the
//! relevel of a five-heading subtree, and the whole rule set is table-testable without
//! a buffer, a fold state, or a window.
//!
//! The section boundaries come from [`heading_lines`], which is the parser's own scan:
//! a `#` inside a fenced block is not a heading, a heading inside a blockquote is
//! quoted text, and an indented `#` is not a heading at all — so none of them end a
//! section here either. There is one answer to "what sections does this note have" and
//! these acts do not keep a second one.
//!
//! Two rules the shape of the edit enforces rather than checks afterwards:
//!
//! - **A move never re-parents.** A section swaps with a sibling — a section under the
//!   same parent — and refuses when the neighbour on that side belongs to somebody
//!   else (EDGE-S2).
//! - **A relevel takes the subtree the text has *now*.** The nested headings are found
//!   before any level changes, because recomputing them afterwards would drag in
//!   headings that were never under the one being moved (EDGE-S8).

use super::object::{heading_lines, HeadingLine};

/// Which way a section moves among its siblings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Swap with the sibling above.
    Up,
    /// Swap with the sibling below.
    Down,
}

/// Why the text will not do what was asked. Every door that calls these acts reads
/// one of these out loud rather than swallowing the keystroke.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The line asked for is not a heading, so there is no section to act on.
    NotAHeading,
    /// The heading is already `#`: there is no level above it. Refused rather than
    /// clamped, because a keystroke that looks like it worked and did nothing is the
    /// worst answer (EDGE-S9).
    AlreadyTop,
    /// The heading, or one nested under it, is already `######`.
    AlreadyDeepest,
    /// The neighbour on that side sits under a different parent, so the move would
    /// re-parent the section.
    DifferentParent,
    /// There is no sibling section on that side.
    NoNeighbour,
}

/// One replacement: put `text` where `text[start..end]` is now.
pub type Edit = (usize, usize, String);

/// What an act would do, and where the heading it acted on ends up — so the caller can
/// keep its selection on the thing it just moved instead of losing it to a line number
/// that no longer names it.
#[derive(Debug, PartialEq, Eq)]
pub struct StructureEdit {
    pub edits: Vec<Edit>,
    /// The line the heading is on once the edits are applied.
    pub heading_line: usize,
}

/// A heading and the section it opens.
#[derive(Debug, PartialEq, Eq)]
pub struct Section {
    /// The 0-based line the heading sits on.
    pub line: usize,
    pub depth: u8,
    /// The byte the heading's line starts at.
    pub start: usize,
    /// The byte the next heading of the same or a shallower level starts at, or the
    /// end of the note. The section's own text *and* everything nested under it,
    /// including the blank lines that separate it from what follows — which is what
    /// lets a swap move the spacing along with the prose.
    pub end: usize,
}

/// Every section in the note, in order, each ending where a heading of the same or a
/// shallower level begins.
pub fn sections(text: &str) -> Vec<Section> {
    let heads = heading_lines(text);
    sections_of(text, &heads)
}

fn sections_of(text: &str, heads: &[HeadingLine]) -> Vec<Section> {
    heads
        .iter()
        .enumerate()
        .map(|(i, head)| {
            let end = heads[i + 1..]
                .iter()
                .find(|next| next.depth <= head.depth)
                .map_or(text.len(), |next| next.line_start);
            Section {
                line: head.line,
                depth: head.depth,
                start: head.line_start,
                end,
            }
        })
        .collect()
}

/// The index range of the headings nested under `at` — `at` included — as the text
/// stands **now**. Everything after the first heading that is not deeper than `at`
/// belongs to somebody else.
fn subtree(heads: &[HeadingLine], at: usize) -> std::ops::Range<usize> {
    let depth = heads[at].depth;
    let past = heads[at + 1..]
        .iter()
        .position(|head| head.depth <= depth)
        .map_or(heads.len(), |offset| at + 1 + offset);
    at..past
}

/// The `#` run a heading opens with, skipping the up-to-three leading spaces ATX
/// allows (four would be a code block, and the scanner would not have styled it).
fn marker_run(text: &str, line_start: usize) -> (usize, usize) {
    let from = line_start + text[line_start..].chars().take_while(|c| *c == ' ').count();
    let to = from + text[from..].chars().take_while(|c| *c == '#').count();
    (from, to)
}

/// Add or remove one level from a heading **and every heading nested under it**,
/// keeping the shape of the subtree: `##` becomes `#` and its `###` children become
/// `##`, so nothing is re-parented by the change (EDGE-S8).
///
/// Only the marker run of each heading is touched. The heading's text, its trailing
/// `^block-id`, and everything under it are left exactly as they were, which is what
/// keeps an anchor resolving through the change (EDGE-S10).
pub fn relevel_section(text: &str, line: usize, delta: isize) -> Result<StructureEdit, Refusal> {
    let heads = heading_lines(text);
    let at = heads
        .iter()
        .position(|head| head.line == line)
        .ok_or(Refusal::NotAHeading)?;
    let under = subtree(&heads, at);

    // The caps are checked across the whole subtree before anything is written: a
    // relevel that promoted three headings and then noticed the fourth had nowhere to
    // go would leave the note half-edited.
    let mut edits = Vec::with_capacity(under.len());
    for head in &heads[under.clone()] {
        let depth = head.depth as isize + delta;
        if depth < 1 {
            return Err(Refusal::AlreadyTop);
        }
        if depth > 6 {
            return Err(Refusal::AlreadyDeepest);
        }
        let (from, to) = marker_run(text, head.line_start);
        edits.push((from, to, "#".repeat(depth as usize)));
    }
    Ok(StructureEdit {
        edits,
        heading_line: line,
    })
}

/// Swap a section with its sibling above (`Up`) or below (`Down`), taking everything
/// nested under it along. The section is moved whole: one replacement over the two
/// sections' joined range, so one `u` puts the note back.
pub fn move_section(text: &str, line: usize, way: Direction) -> Result<StructureEdit, Refusal> {
    let heads = heading_lines(text);
    let at = heads
        .iter()
        .position(|head| head.line == line)
        .ok_or(Refusal::NotAHeading)?;
    let here = &heads[at];
    let sibling = match way {
        Direction::Up => (0..at).rev().find(|k| heads[*k].depth == here.depth),
        Direction::Down => (at + 1..heads.len()).find(|k| heads[*k].depth == here.depth),
    };
    let Some(other) = sibling else {
        return Err(Refusal::NoNeighbour);
    };
    if parent_of(&heads, at) != parent_of(&heads, other) {
        return Err(Refusal::DifferentParent);
    }

    let all = sections_of(text, &heads);
    let (first, second) = if at < other { (at, other) } else { (other, at) };
    // Siblings under one parent are adjacent in the section list: the first ends where
    // the second begins, because the next heading of the same or shallower level *is*
    // the second one.
    let (a, b) = (&all[first], &all[second]);
    // The chunks move verbatim, spacing included, with one exception: the seam between
    // them always gets exactly one blank line. Without that, moving the last section of
    // a note that ends without a trailing blank line would glue a heading onto the line
    // above it, and a move should not cost the note its formatting.
    let moved_down = at < other;
    let mut swapped = text[b.start..b.end].trim_end_matches('\n').to_string();
    swapped.push_str("\n\n");
    swapped.push_str(&text[a.start..a.end]);
    // Where the moved heading ends up, counted rather than guessed: the two sections are
    // almost never the same length, so the sibling's old line number is not where the
    // heading lands. Everything before the pair is untouched, so the line is the newlines
    // before the pair, plus the newlines in whatever now sits in front of the heading.
    let mut heading_line = text[..a.start].matches('\n').count();
    if moved_down {
        heading_line += text[b.start..b.end]
            .trim_end_matches('\n')
            .matches('\n')
            .count()
            + 2;
    }
    Ok(StructureEdit {
        edits: vec![(a.start, b.end, swapped)],
        heading_line,
    })
}

/// The heading this one sits under, as an index into `heads`, or `None` for a top-level
/// section. Two headings are siblings when they share one.
fn parent_of(heads: &[HeadingLine], at: usize) -> Option<usize> {
    (0..at).rev().find(|k| heads[*k].depth < heads[at].depth)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply what an act answered, so a test can read the note it would produce.
    fn apply(text: &str, answer: &StructureEdit) -> String {
        let mut out = text.to_string();
        let mut edits = answer.edits.clone();
        edits.sort_by_key(|(start, _, _)| *start);
        for (start, end, new) in edits.iter().rev() {
            out.replace_range(*start..*end, new);
        }
        out
    }

    fn acted(text: &str, answer: Result<StructureEdit, Refusal>) -> String {
        apply(
            text,
            answer
                .as_ref()
                .expect("the act should have answered with edits"),
        )
    }

    fn shape(text: &str) -> Vec<(usize, u8)> {
        sections(text).iter().map(|s| (s.line, s.depth)).collect()
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_the_same_level_or_shallower() {
        let text = "# A\n\na\n\n## B\n\nb\n\n### C\n\nc\n\n## D\n\nd\n";
        let got = sections(text);
        assert_eq!(shape(text), vec![(0, 1), (4, 2), (8, 3), (12, 2)]);
        // `# A` holds everything: its section is the whole note.
        assert_eq!((got[0].start, got[0].end), (0, text.len()));
        // `## B` takes `### C` with it and stops at `## D`.
        assert_eq!(
            &text[got[1].start..got[1].end],
            "## B\n\nb\n\n### C\n\nc\n\n"
        );
    }

    #[test]
    fn a_move_swaps_two_siblings_and_takes_the_nest_with_it() {
        let text = "# A\n\n## B\n\nb\n\n## C\n\nc\n\n### C1\n\nc1\n";
        let answer = move_section(text, 6, Direction::Up).expect("C should move up");
        assert_eq!(
            apply(text, &answer),
            "# A\n\n## C\n\nc\n\n### C1\n\nc1\n\n## B\n\nb\n\n"
        );
        // The selection follows the heading that moved, not the line it left.
        assert_eq!(answer.heading_line, 2);
    }

    #[test]
    fn the_heading_reports_the_line_it_lands_on_in_both_directions() {
        // The two sections are almost never the same length, so the sibling's old line
        // number is not where the moved heading ends up. A selection keyed to the wrong
        // line moves the wrong section on the next keystroke.
        let text = "# A\n\n## B\n\nb\n\n## C\n\nc\n\n### C1\n\nc1\n";
        let answer = move_section(text, 2, Direction::Down).expect("B should move down");
        let after = apply(text, &answer);
        assert_eq!(answer.heading_line, 10);
        assert_eq!(
            after.lines().nth(answer.heading_line).unwrap(),
            "## B",
            "the line the act names is the line the heading is really on"
        );
        // And the same note moved the other way, where the heading lands at the pair's
        // start whatever the sections' lengths.
        let answer = move_section(text, 6, Direction::Up).expect("C should move up");
        let after = apply(text, &answer);
        assert_eq!(after.lines().nth(answer.heading_line).unwrap(), "## C");
    }

    #[test]
    fn the_seam_between_swapped_sections_gets_one_blank_line() {
        // The chunks move verbatim, but the join is normalised: a note that ran two
        // blank lines between these two sections still reads cleanly after the swap,
        // and the spacing *inside* each section is left exactly as it was.
        let text = "## One\n\none\n\n\n## Two\n\ntwo\n";
        assert_eq!(
            acted(text, move_section(text, 5, Direction::Up)),
            "## Two\n\ntwo\n\n## One\n\none\n\n\n"
        );
    }

    #[test]
    fn a_move_refuses_across_parents() {
        // EDGE-S2: `## E`'s previous same-level heading is `## B`, but B lives under
        // `# A`. Moving E up would lift it out from under `# D`, which is a re-parent,
        // not a reorder.
        let text = "# A\n\n## B\n\nb\n\n# D\n\n## E\n\ne\n";
        assert_eq!(
            move_section(text, 8, Direction::Up),
            Err(Refusal::DifferentParent)
        );
        // And the same note from the other side: `## B` cannot drop out of `# A`.
        assert_eq!(
            move_section(text, 2, Direction::Down),
            Err(Refusal::DifferentParent)
        );
    }

    #[test]
    fn a_move_refuses_when_there_is_no_sibling_on_that_side() {
        let text = "# A\n\n## B\n\nb\n\n## C\n\nc\n";
        assert_eq!(
            move_section(text, 0, Direction::Up),
            Err(Refusal::NoNeighbour)
        );
        assert_eq!(
            move_section(text, 6, Direction::Down),
            Err(Refusal::NoNeighbour)
        );
        // A deeper heading with no same-level peer has nowhere to go either.
        let text = "# A\n\n### Deep\n\nd\n";
        assert_eq!(
            move_section(text, 2, Direction::Up),
            Err(Refusal::NoNeighbour)
        );
    }

    #[test]
    fn a_relevel_moves_the_whole_subtree_and_keeps_its_shape() {
        // EDGE-S8: the subtree is what the scan found *before* the change. Promoting
        // `## B` also promotes `### C`; the `## D` after it is not under B and does
        // not move.
        let text = "# A\n\n## B\n\nb\n\n### C\n\nc\n\n## D\n\nd\n";
        let got = acted(text, relevel_section(text, 2, -1));
        assert_eq!(got, "# A\n\n# B\n\nb\n\n## C\n\nc\n\n## D\n\nd\n");
        // The shape survives: C is still under B, and D is still B's sibling.
        assert_eq!(shape(&got), vec![(0, 1), (2, 1), (6, 2), (10, 2)]);
    }

    #[test]
    fn a_relevel_down_takes_the_subtree_too() {
        let text = "## B\n\nb\n\n### C\n\nc\n\n#### D\n\nd\n";
        assert_eq!(
            acted(text, relevel_section(text, 0, 1)),
            "### B\n\nb\n\n#### C\n\nc\n\n##### D\n\nd\n"
        );
    }

    #[test]
    fn the_level_caps_refuse_rather_than_clamp() {
        // EDGE-S9: `#` has no level above it, and a `######` has no level below it.
        let text = "# A\n\n## B\n\n###### Six\n";
        assert_eq!(relevel_section(text, 0, -1), Err(Refusal::AlreadyTop));
        assert_eq!(relevel_section(text, 2, 1), Err(Refusal::AlreadyDeepest));
        // The deep heading is two levels down, and it is still the reason for the
        // refusal: a note left half-relevelled is worse than one not touched.
        assert_eq!(relevel_section(text, 0, 1), Err(Refusal::AlreadyDeepest));
        let text = "##### Five\n\n###### Six\n";
        assert_eq!(relevel_section(text, 0, 1), Err(Refusal::AlreadyDeepest));
        assert_eq!(relevel_section(text, 2, 1), Err(Refusal::AlreadyDeepest));
    }

    #[test]
    fn a_relevel_touches_only_the_marker_run() {
        // EDGE-S10: `## The Fall ^fall` keeps its id, because the edit is the hashes
        // and nothing else. Rewriting the whole line is what would drop the anchor.
        let text = "## The Fall ^fall\n\nbody\n";
        assert_eq!(
            acted(text, relevel_section(text, 0, -1)),
            "# The Fall ^fall\n\nbody\n"
        );
    }

    #[test]
    fn a_move_carries_the_block_id_with_the_heading() {
        let text = "## A ^one\n\na\n\n## B ^two\n\nb\n";
        assert_eq!(
            acted(text, move_section(text, 4, Direction::Up)),
            "## B ^two\n\nb\n\n## A ^one\n\na\n\n"
        );
    }

    #[test]
    fn a_fenced_hash_is_not_a_section_boundary() {
        // The scanner never styles a `#` inside a fenced block, so a comment in code
        // cannot end a section, and a move does not cut through the fence.
        let text = "## A\n\n```\n# not a heading\n```\n\n## B\n\nb\n";
        assert_eq!(shape(text), vec![(0, 2), (6, 2)]);
        assert_eq!(
            acted(text, move_section(text, 6, Direction::Up)),
            "## B\n\nb\n\n## A\n\n```\n# not a heading\n```\n\n"
        );
    }

    #[test]
    fn a_blockquoted_heading_is_not_a_section() {
        // Quoting somebody's heading is quoting, not sectioning — the same answer the
        // fold, the anchor matcher and the index already give.
        let text = "## A\n\n> ## Quoted\n\n## B\n\nb\n";
        assert_eq!(shape(text), vec![(0, 2), (4, 2)]);
        assert_eq!(
            acted(text, move_section(text, 4, Direction::Up)),
            "## B\n\nb\n\n## A\n\n> ## Quoted\n\n"
        );
    }

    #[test]
    fn a_setext_underline_does_not_become_a_heading() {
        // Shale styles ATX headings only. A `---` under a line is a thematic break,
        // and the acts must not invent a section there — or treat the underline as a
        // boundary when deciding where a section ends.
        let text = "Title\n---\n\n## A\n\na\n\n## B\n\nb\n";
        assert_eq!(shape(text), vec![(3, 2), (7, 2)]);
        assert_eq!(relevel_section(text, 1, -1), Err(Refusal::NotAHeading));
    }

    #[test]
    fn a_heading_inside_a_list_or_table_is_not_a_section() {
        // A `#` that opens a list item or sits in a table cell is styled as text, not
        // as a heading, so it neither ends a section nor moves as one.
        let text = "## A\n\n- # not a heading\n\n| a | b |\n| - | - |\n| # | x |\n\n## B\n\nb\n";
        assert_eq!(shape(text), vec![(0, 2), (8, 2)]);
    }

    #[test]
    fn the_asked_line_has_to_be_a_heading() {
        let text = "# A\n\nbody\n";
        assert_eq!(relevel_section(text, 2, 1), Err(Refusal::NotAHeading));
        assert_eq!(
            move_section(text, 2, Direction::Up),
            Err(Refusal::NotAHeading)
        );
        // A line the note does not have is the same answer.
        assert_eq!(
            move_section(text, 99, Direction::Up),
            Err(Refusal::NotAHeading)
        );
    }

    #[test]
    fn an_indented_hash_is_not_a_section() {
        // Shale's scanner styles a heading only when the `#` opens the line, so an
        // indented `#` is not a heading anywhere else in the app either — not to the
        // fold, not to the index, and not here. The acts inherit that answer rather
        // than keeping a looser one of their own.
        let text = "   ## A\n\nbody\n";
        assert_eq!(shape(text), vec![]);
        assert_eq!(relevel_section(text, 0, -1), Err(Refusal::NotAHeading));
        assert_eq!(
            move_section(text, 0, Direction::Up),
            Err(Refusal::NotAHeading)
        );
    }

    #[test]
    fn a_bare_marker_is_a_section_with_no_name() {
        let text = "##\n\nbody\n\n## B\n\nb\n";
        assert_eq!(
            acted(text, move_section(text, 4, Direction::Up)),
            "## B\n\nb\n\n##\n\nbody\n\n"
        );
    }
}
