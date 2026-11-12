//! The right-hand strip's model: which view it shows, and the rows the Outline view
//! draws.
//!
//! The strip shows **one view at a time** (sidebar-plan.md ruling 11): the outline, the
//! notes that link this one, the notes this one links, its tags, or its properties. This
//! module is the part that needs no window — the view list, and the outline's rows worked
//! out from a note's text and its fold decisions.
//!
//! The outline reads the **buffer**, never the index (EDGE-S5): a heading typed a second
//! ago is a row immediately, because it is real text the reader is looking at and the
//! index has not caught up with it.

use shale_core::fold::{fold_regions, FoldState};
use shale_core::mdstyle::object::heading_lines;

/// The key context the strip's bindings live in, active while it has the keyboard
/// (see `Workspace::focus_sidebar`).
pub(crate) const CONTEXT: &str = "sidebar";

/// The views the strip can show, in `[`/`]` order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    /// The outline is the default: it is the view that needs no index, so it is the one
    /// a window can always show.
    #[default]
    Outline,
    Backlinks,
    Outgoing,
    Tags,
    Properties,
}

impl View {
    /// Every view, in the order `[`/`]` walk them.
    pub const ALL: [View; 5] = [
        View::Outline,
        View::Backlinks,
        View::Outgoing,
        View::Tags,
        View::Properties,
    ];

    /// What the header strip says.
    pub fn label(self) -> &'static str {
        match self {
            View::Outline => "Outline",
            View::Backlinks => "Links to here",
            View::Outgoing => "Links from here",
            View::Tags => "Tags",
            View::Properties => "Properties",
        }
    }

    /// The word `:sidebar <word>` takes.
    pub fn word(self) -> &'static str {
        match self {
            View::Outline => "outline",
            View::Backlinks => "backlinks",
            View::Outgoing => "links",
            View::Tags => "tags",
            View::Properties => "props",
        }
    }

    /// `[` and `]`: one view either way, wrapping at the ends. The strip has no
    /// second axis to move along, so a cycle is the whole of it.
    pub fn step(self, delta: isize) -> View {
        let at = Self::ALL.iter().position(|v| *v == self).unwrap_or(0);
        let len = Self::ALL.len() as isize;
        let next = (at as isize + delta).rem_euclid(len);
        Self::ALL[next as usize]
    }

    /// The view a `:sidebar <word>` names. `links` means the notes this note links
    /// to; `backlinks` is the other direction.
    pub fn from_word(word: &str) -> Option<View> {
        // The header says "Outgoing" and "Properties"; the command line gets the short
        // word and the header's word both, so a user who reads the header is never
        // told their command was unknown.
        match word {
            "outgoing" => Some(View::Outgoing),
            "properties" => Some(View::Properties),
            _ => Self::ALL.into_iter().find(|v| v.word() == word),
        }
    }

    /// Whether this view's rows are built in M1. The others answer honestly that they
    /// are not built yet, so the verb is complete before the views are (ruling 16).
    pub fn built(self) -> bool {
        matches!(self, View::Outline)
    }
}

/// One row of the Outline view: a heading, where it sits, and what the note's own
/// folds do to the rows under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The buffer row the heading is on (0-based), which is what a jump asks for.
    pub line: usize,
    /// The heading's level, 1..=6: the nesting the row is indented by.
    pub depth: u8,
    /// The heading's text, with no markers.
    pub text: String,
    /// The heading has a section the note could fold.
    pub foldable: bool,
    /// That section is folded shut, so its deeper rows are not drawn here either —
    /// the same decision the note's gutter is showing (ruling 7).
    pub closed: bool,
}

/// The Outline view's rows for a note's text, honouring the note's fold decisions.
///
/// A folded heading hides its deeper headings here exactly as it hides their rows in
/// the note, which is what lets one `h`/`l` serve both surfaces: the strip has no fold
/// state of its own to keep in step.
pub fn outline(text: &str, folds: &FoldState) -> Vec<Row> {
    let regions = fold_regions(text);
    let mut out: Vec<Row> = Vec::new();
    // The last row hidden by a closed section. Headings arrive in buffer order, so
    // one closed region covers everything nested under it until its end row, and the
    // next heading past that end is a row again.
    let mut hidden_until: Option<usize> = None;
    for head in heading_lines(text) {
        if let Some(until) = hidden_until {
            if head.line <= until {
                continue;
            }
            hidden_until = None;
        }
        let region = regions.iter().find(|region| region.start_row == head.line);
        let closed = region.is_some_and(|region| folds.is_closed(region));
        out.push(Row {
            line: head.line,
            depth: head.depth,
            text: text[head.text_start..head.text_end].trim().to_string(),
            foldable: region.is_some(),
            closed,
        });
        if closed {
            hidden_until = region.map(|region| region.end_row);
        }
    }
    out
}

/// Which row the caret is inside: the last heading at or above `caret_line`.
///
/// `None` above the first heading — there is no section the caret is in, and marking
/// nothing is the honest answer rather than marking the first heading the note has.
pub fn section_of(rows: &[Row], caret_line: usize) -> Option<usize> {
    let index = rows.iter().rposition(|row| row.line <= caret_line)?;
    // The heading's **line**, not its row index: the mark outlives a frame where a row
    // was added above it, and it is the same key the selection and the jump use.
    Some(rows[index].line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shale_core::fold::fold_regions;

    fn rows(text: &str) -> Vec<Row> {
        outline(text, &FoldState::default())
    }

    #[test]
    fn the_outline_is_the_headings_in_order_with_their_depth() {
        let text = "# Top\n\ntext\n\n## Second\n\ntext\n\n### Third\n";
        let got = rows(text);
        assert_eq!(
            got.iter()
                .map(|row| (row.line, row.depth, row.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, 1, "Top"), (4, 2, "Second"), (8, 3, "Third")]
        );
    }

    #[test]
    fn a_folded_heading_hides_the_headings_nested_under_it() {
        // The fold is closed through the note's own region model, not a hand-built
        // key: the region the note would fold is the one closed here.
        let text = "# Top\n\n## A\n\nx\n\n## B\n";
        let regions = fold_regions(text);
        let mut folds = FoldState::default();
        folds.close(regions.iter().find(|r| r.start_row == 0).unwrap());
        let got = outline(text, &folds);
        assert_eq!(
            got.iter().map(|row| row.line).collect::<Vec<_>>(),
            vec![0],
            "the folded section's headings are not rows"
        );
        assert!(got[0].closed && got[0].foldable);
    }

    #[test]
    fn a_folded_inner_heading_hides_only_its_own_subtree() {
        let text = "# Top\n\n## A\n\n### Deep\n\n## B\n";
        let regions = fold_regions(text);
        let mut folds = FoldState::default();
        folds.close(regions.iter().find(|r| r.start_row == 2).unwrap());
        let got = outline(text, &folds);
        assert_eq!(
            got.iter().map(|row| row.line).collect::<Vec<_>>(),
            vec![0, 2, 6],
            "`### Deep` is hidden, `## B` is not"
        );
    }

    #[test]
    fn the_mark_is_the_last_heading_at_or_above_the_caret() {
        let got = rows("# Top\n\n## A\n\n### Deep\n\n## B\n");
        assert_eq!(section_of(&got, 0), Some(0));
        assert_eq!(section_of(&got, 4), Some(4));
        assert_eq!(section_of(&got, 7), Some(6));
    }

    #[test]
    fn there_is_no_section_above_the_first_heading() {
        let got = rows("prose\n\n# Top\n");
        assert_eq!(section_of(&got, 0), None);
        assert_eq!(section_of(&got, 2), Some(2));
    }

    #[test]
    fn the_view_cycle_covers_every_view_and_wraps() {
        let mut seen = Vec::new();
        let mut view = View::Outline;
        for _ in 0..View::ALL.len() {
            seen.push(view);
            view = view.step(1);
        }
        assert_eq!(seen, View::ALL.to_vec());
        assert_eq!(View::Outline.step(-1), View::Properties);
        assert_eq!(View::Properties.step(1), View::Outline);
    }

    #[test]
    fn every_view_is_named_by_one_word() {
        for view in View::ALL {
            assert_eq!(View::from_word(view.word()), Some(view));
        }
        assert_eq!(View::from_word("nonsense"), None);
    }
}
