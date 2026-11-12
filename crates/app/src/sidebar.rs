//! The right-hand strip's model: which view it shows, and the rows each built view draws.
//!
//! The strip shows **one view at a time** (sidebar-plan.md ruling 11): the outline, the
//! notes that link this one, the notes this one links, its tags, or its properties. This
//! module is the part that needs no window — the view list, and the outline's rows worked
//! out from a note's text and its fold decisions.
//!
//! The outline reads the **buffer**, never the index (EDGE-S5): a heading typed a second
//! ago is a row immediately, because it is real text the reader is looking at and the
//! index has not caught up with it. The two link views are the other half of the same
//! ruling: they describe *other* notes, which no buffer in this window holds, so they
//! read the index and land when the note is written.
//!
//! This module stays free of windows and databases: the workspace hands it a note's text
//! or the index's rows, and the rules about what a row means are all here.

use std::path::{Path, PathBuf};

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

    /// What a built view with no rows says about itself. Each view's emptiness is a
    /// different claim, so they cannot share one sentence — and none of them is the
    /// sentence a vault with the index off says instead (EDGE-S6).
    pub fn empty_said(self) -> &'static str {
        match self {
            View::Outline => "no sections in this note",
            View::Backlinks => "nothing links here",
            View::Outgoing => "this note links nothing",
            View::Tags | View::Properties => "",
        }
    }

    /// Whether this view answers from the index rather than from the note in hand. With
    /// the index off it has nothing to answer with, and says so.
    pub fn index_backed(self) -> bool {
        matches!(
            self,
            View::Backlinks | View::Outgoing | View::Tags | View::Properties
        )
    }

    /// Whether this view has rows. The rest answer honestly that they are not built
    /// yet, so the verb is complete before the views are (ruling 16).
    pub fn built(self) -> bool {
        matches!(self, View::Outline | View::Backlinks | View::Outgoing)
    }
}

/// One row of the strip, whichever view is showing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The row's **key**: what the keyboard's selection is compared against. What makes
    /// a row itself differs by view, and each view picks a key that survives its own
    /// churn — for the Outline, the heading's buffer line, which outlives a row being
    /// added above it; for a view that names other notes there is no line in *this* note
    /// to key by, so its position in the list, which is stable while the keyboard is in
    /// it. The two are not the same number and are never read as one: a jump asks
    /// [`Act`], never this.
    pub key: usize,
    /// The heading's level, 1..=6: the nesting the row is indented by. A flat view is
    /// all level 1.
    pub depth: u8,
    /// What the row says.
    pub text: String,
    /// The heading has a section the note could fold.
    pub foldable: bool,
    /// That section is folded shut, so its deeper rows are not drawn here either —
    /// the same decision the note's gutter is showing (ruling 7).
    pub closed: bool,
    /// A link whose target the index could not resolve (EDGE-S7). The row is drawn and
    /// marked, because a hole in the picture is a fact about the note; `enter` on it
    /// says so rather than opening a note that does not exist.
    pub ghost: bool,
    /// What `enter` does with this row.
    pub act: Act,
}

/// What a row is about, and what the strip does when it is chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Reveal this buffer line in the note already on screen. Not an open: no tab, no
    /// read, no risk to unsaved edits (ruling 19).
    Reveal(usize),
    /// Open another note — the note that links this one, or the one this note links.
    /// `line` is where in *that* note the row points, when the index knows one: a
    /// backlink row lands on the line that links, an outgoing row names the target and
    /// leaves the reader at its top.
    Open { path: PathBuf, line: Option<usize> },
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
            key: head.line,
            depth: head.depth,
            text: text[head.text_start..head.text_end].trim().to_string(),
            foldable: region.is_some(),
            closed,
            ghost: false,
            act: Act::Reveal(head.line),
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
    let index = rows.iter().rposition(|row| row.key <= caret_line)?;
    // The heading's **line**, not its row index: the mark outlives a frame where a row
    // was added above it, and it is the same key the selection and the jump use.
    Some(rows[index].key)
}

/// The rows for one direction of the link table, from the index's rows for it.
///
/// `ends` is what `Db::backlinks` or `Db::outgoing` returned: `(the other note, what the
/// link resolved to, the line)`. `to_here` decides only which end the row names and where
/// a jump lands:
///
/// - **Links to here**: the row names the note holding the link, and `enter` opens it at
///   the line the link sits on. A backlink row that opened a note at its top would have
///   thrown away the one fact the row exists to show.
/// - **Links from here**: the row names the target, and `enter` opens it. The line the
///   index reports is in *this* note — where the link is — so it marks the row's provenance
///   rather than steering the jump.
///
/// A ghost target keeps its row and is marked (EDGE-S7). The rows are numbered by position
/// because a view that names other notes has no line in this note to key a selection by.
pub fn link_rows(ends: &[(PathBuf, String, usize)], to_here: bool) -> Vec<Row> {
    ends.iter()
        .enumerate()
        .map(|(index, (other, kind, line))| {
            let ghost = kind == shale_core::db::target_kind::GHOST;
            Row {
                key: index,
                depth: 1,
                text: display(other),
                foldable: false,
                closed: false,
                ghost,
                act: Act::Open {
                    path: other.clone(),
                    line: if to_here { Some(*line) } else { None },
                },
            }
        })
        .collect()
}

/// How a vault-relative path reads in a list: the path itself, without the `.md` a reader
/// already knows is there. A ghost's name is what the author typed, kept exactly — showing
/// `Nowhere` is the fact; showing a path that does not exist would be a guess.
fn display(path: &Path) -> String {
    let text = path.display().to_string();
    text.strip_suffix(".md")
        .filter(|stem| !stem.is_empty())
        .map(str::to_string)
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shale_core::db::target_kind;
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
                .map(|row| (row.key, row.depth, row.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, 1, "Top"), (4, 2, "Second"), (8, 3, "Third")]
        );
        // The key and the act are the same line for an outline row, and the act is what
        // a jump asks for — never the key.
        assert_eq!(got[1].act, Act::Reveal(4));
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
            got.iter().map(|row| row.key).collect::<Vec<_>>(),
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
            got.iter().map(|row| row.key).collect::<Vec<_>>(),
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

    #[test]
    fn a_backlink_row_names_the_linking_note_and_opens_it_at_the_line() {
        let ends = vec![(
            PathBuf::from("Notes/Session 3.md"),
            target_kind::NOTE.to_string(),
            12,
        )];
        let got = link_rows(&ends, true);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].text, "Notes/Session 3",
            "the `.md` a reader knows is there"
        );
        assert_eq!(
            got[0].act,
            Act::Open {
                path: PathBuf::from("Notes/Session 3.md"),
                line: Some(12),
            },
            "the line that links, not the top of the note"
        );
        assert!(!got[0].ghost);
    }

    #[test]
    fn an_outgoing_row_names_the_target_and_opens_it_without_a_line() {
        // The line the index reports for this direction sits in the note we came from,
        // so it cannot steer a jump into the target.
        let ends = vec![(PathBuf::from("b.md"), target_kind::NOTE.to_string(), 4)];
        let got = link_rows(&ends, false);
        assert_eq!(
            got[0].act,
            Act::Open {
                path: PathBuf::from("b.md"),
                line: None,
            }
        );
    }

    #[test]
    fn a_ghost_target_keeps_its_row_and_is_marked() {
        // EDGE-S7: a hole in the picture is a fact about the note. The name shown is what
        // the author typed, kept exactly.
        let ends = vec![(PathBuf::from("Nowhere"), target_kind::GHOST.to_string(), 2)];
        let got = link_rows(&ends, false);
        assert_eq!(got[0].text, "Nowhere");
        assert!(got[0].ghost);
    }

    #[test]
    fn an_attachment_and_a_query_target_are_rows_like_any_other() {
        let ends = vec![
            (
                PathBuf::from("map.png"),
                target_kind::ATTACHMENT.to_string(),
                1,
            ),
            (
                PathBuf::from("queries/tough.shq"),
                target_kind::QUERY.to_string(),
                3,
            ),
        ];
        let got = link_rows(&ends, false);
        assert_eq!(
            got.iter()
                .map(|row| (row.text.as_str(), row.ghost))
                .collect::<Vec<_>>(),
            vec![("map.png", false), ("queries/tough.shq", false)],
            "a `.shq` is the author's own file, not a missing one"
        );
    }

    #[test]
    fn link_rows_are_keyed_by_position_because_they_name_other_notes() {
        // There is no line in *this* note to key a selection by, so the list's own order
        // is the key — and it is stable while the keyboard is in the view.
        let ends = vec![
            (PathBuf::from("a.md"), target_kind::NOTE.to_string(), 0),
            (PathBuf::from("b.md"), target_kind::NOTE.to_string(), 5),
        ];
        assert_eq!(
            link_rows(&ends, true)
                .iter()
                .map(|row| row.key)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn each_view_says_its_own_emptiness() {
        assert_eq!(View::Outline.empty_said(), "no sections in this note");
        assert_eq!(View::Backlinks.empty_said(), "nothing links here");
        assert_eq!(View::Outgoing.empty_said(), "this note links nothing");
        // The two index-backed views that are not built yet never reach the sentence.
        assert!(View::Backlinks.index_backed() && !View::Outline.index_backed());
    }
}
