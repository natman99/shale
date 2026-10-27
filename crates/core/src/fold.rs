//! Folding: which regions of a note can be collapsed, and which of them are
//! closed right now.
//!
//! A fold never changes the note's text. It changes only which rows the display
//! model draws, so the cursor, the scroll position and vim's `j`/`k` step over a
//! collapsed section exactly as the user sees it (see [`FoldView`] and
//! [`crate::buffer::Buffer::fold_view`], which the UI ORs into the rows live
//! preview already hides).
//!
//! The first pass covers heading sections, which is what the user asked for; the
//! region model is deliberately not heading-specific (`level` is the only field
//! that knows about headings), so a fenced block, a callout or a table can
//! become a region later without changing anything else.

use std::collections::{HashMap, HashSet};

use crate::mdstyle::{style_spans, Style};

/// How a fold is remembered across edits.
///
/// Rows and byte offsets both move when the user types, so a fold is keyed on
/// the text of the row that opens it plus which copy of that row it is. An edit
/// anywhere else in the note leaves both alone; renaming a heading drops its
/// fold, which is the honest outcome — there is nothing left to fold it onto.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FoldKey {
    /// The text of the row that opens the region, trimmed.
    text: String,
    /// Which copy of that row, counted over the note's headings, for the case
    /// where the same heading text appears more than once.
    ordinal: usize,
}

/// One region that can be collapsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldRegion {
    /// The row that opens the region.
    pub start_row: usize,
    /// The region's last row, inclusive. Always greater than `start_row`: a
    /// heading with nothing under it has nothing to fold.
    pub end_row: usize,
    /// The heading's level (1..=6), which is what makes regions nest.
    pub level: u8,
    /// What identifies this region across edits.
    pub key: FoldKey,
}

impl FoldRegion {
    /// Whether `row` is inside the region but is not the row that opens it, which
    /// is what a closed fold hides.
    pub fn hides(&self, row: usize) -> bool {
        self.start_row < row && row <= self.end_row
    }
}

/// The foldable regions of a note, in buffer order (outermost first, because a
/// heading always comes before the sections nested under it).
pub fn fold_regions(text: &str) -> Vec<FoldRegion> {
    let starts = line_starts(text);
    let headings = heading_rows(text, &starts);

    // Number repeated heading texts in buffer order so two sections with the
    // same title can be folded apart.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut opened: Vec<(usize, u8, FoldKey)> = Vec::with_capacity(headings.len());
    for &(row, level) in &headings {
        let title = row_text(text, &starts, row).trim().to_string();
        let ordinal = seen.entry(title.clone()).or_insert(0);
        let key = FoldKey {
            text: title,
            ordinal: *ordinal,
        };
        *ordinal += 1;
        opened.push((row, level, key));
    }

    let mut out = Vec::new();
    for (i, &(row, level, _)) in opened.iter().enumerate() {
        // A heading owns every row below it until the next heading of its own
        // level or a shallower one; the row that ends the run belongs to the
        // neighbour, not to it.
        let next = opened[i + 1..]
            .iter()
            .find(|(_, next_level, _)| *next_level <= level)
            .map_or(starts.len(), |(next_row, _, _)| *next_row);
        let end_row = next.saturating_sub(1);
        if end_row <= row {
            continue;
        }
        out.push(FoldRegion {
            start_row: row,
            end_row,
            level,
            key: opened[i].2.clone(),
        });
    }
    out
}

/// Which regions are closed. Held by the buffer: the vim engine runs the fold
/// commands and the display model reads the result, and both reach the buffer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldState {
    /// The closed regions, by key.
    closed: HashSet<FoldKey>,
}

impl FoldState {
    /// Whether `region` is closed.
    pub fn is_closed(&self, region: &FoldRegion) -> bool {
        self.closed.contains(&region.key)
    }

    /// Whether any fold is closed, which lets the UI skip the fold work
    /// altogether for the common case of a note with no folds.
    pub fn any_closed(&self) -> bool {
        !self.closed.is_empty()
    }

    /// Close `region`.
    pub fn close(&mut self, region: &FoldRegion) {
        self.closed.insert(region.key.clone());
    }

    /// Open `region`. Because a fold is a closed region rather than a stored
    /// definition, this is also what vim's `zd` and `zE` come to: once the
    /// section is open there is nothing left to remove.
    pub fn open(&mut self, region: &FoldRegion) {
        self.closed.remove(&region.key);
    }

    /// Close every region, outermost first, so a nested fold inside a section
    /// that is already closed does not get a marker of its own.
    pub fn close_all(&mut self, regions: &[FoldRegion]) {
        for region in regions {
            self.close(region);
        }
    }

    /// Open every region.
    pub fn open_all(&mut self) {
        self.closed.clear();
    }

    /// Forget every fold, which is what loading another note does.
    pub fn clear(&mut self) {
        self.closed.clear();
    }
}

/// How the closed folds change this frame's display model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldView {
    /// Per buffer row: the row is hidden inside a closed fold and must take no
    /// display line, exactly like a code block's fence line.
    pub collapsed: Vec<bool>,
    /// Per buffer row: on the row that opens a closed fold, how many rows it
    /// hides — what the UI paints as the fold marker. `None` everywhere else.
    pub marker: Vec<Option<usize>>,
}

/// The display effect of `state` over `regions` in a note of `line_count` rows.
pub fn fold_view(regions: &[FoldRegion], state: &FoldState, line_count: usize) -> FoldView {
    let mut out = FoldView {
        collapsed: vec![false; line_count],
        marker: vec![None; line_count],
    };
    if !state.any_closed() {
        return out;
    }
    // Regions are in buffer order, so an outer section is walked before the ones
    // nested under it: once its rows are collapsed, a nested closed fold inside
    // it has no visible row left to carry a marker.
    for region in regions {
        if !state.is_closed(region) {
            continue;
        }
        let open_row_is_visible = out
            .collapsed
            .get(region.start_row)
            .is_some_and(|hidden| !*hidden);
        for row in (region.start_row + 1)..=region.end_row {
            if let Some(slot) = out.collapsed.get_mut(row) {
                *slot = true;
            }
        }
        if open_row_is_visible {
            out.marker[region.start_row] = Some(region.end_row - region.start_row);
        }
    }
    out
}

/// The byte offset where each row of `text` begins. The row count is
/// `starts.len()`, and a note that does not end with a newline still counts its
/// last row.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// The text of `row`, without its newline.
fn row_text<'a>(text: &'a str, starts: &[usize], row: usize) -> &'a str {
    starts
        .get(row)
        .map(|&start| text[start..].split('\n').next().unwrap_or(""))
        .unwrap_or("")
}

/// `(row, level)` for every heading, in buffer order.
///
/// The markdown scanner decides what a heading is, which is what keeps a `#`
/// inside a fenced code block from opening a section.
fn heading_rows(text: &str, starts: &[usize]) -> Vec<(usize, u8)> {
    let mut out: Vec<(usize, u8)> = Vec::new();
    for span in style_spans(text) {
        let level = match span.style {
            Style::Heading { level } => level,
            // A heading with no text after the markers gets only the marker
            // span, so the level is the run of `#` itself.
            Style::HeadingMarker => {
                text[span.start..].chars().take_while(|&c| c == '#').count() as u8
            }
            _ => continue,
        };
        if level == 0 || level > 6 {
            continue;
        }
        let row = starts
            .binary_search(&span.start)
            .unwrap_or_else(|i| i.saturating_sub(1));
        if let Some((last_row, last_level)) = out.last_mut() {
            if *last_row == row {
                *last_level = (*last_level).max(level);
                continue;
            }
        }
        out.push((row, level));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A note with two top-level sections, one of which holds two subsections.
    const NOTE: &str = "# One\nintro\n## Two\nbody\n### Three\ndeep\n## Four\nlast\n# Five\nend\n";

    fn rows(text: &str) -> Vec<(usize, usize, u8)> {
        fold_regions(text)
            .into_iter()
            .map(|r| (r.start_row, r.end_row, r.level))
            .collect()
    }

    #[test]
    fn a_heading_owns_rows_until_the_next_heading_of_its_level() {
        assert_eq!(
            rows(NOTE),
            vec![(0, 7, 1), (2, 5, 2), (4, 5, 3), (6, 7, 2), (8, 10, 1),],
            "`# One` owns everything up to `# Five`, and `## Two` stops at `## Four`"
        );
    }

    #[test]
    fn a_heading_with_nothing_under_it_is_not_a_region() {
        // The row after `# A` is another heading, so `# A` owns nothing; the
        // trailing newline is a real (empty) row, which `# B` does own.
        assert_eq!(rows("# A\n# B\ntext\n"), vec![(1, 3, 1)]);
    }

    #[test]
    fn a_hash_inside_a_fenced_block_is_not_a_heading() {
        let text = "# Real\n```\n# fake\n```\nafter\n";
        assert_eq!(rows(text), vec![(0, 5, 1)]);
    }

    #[test]
    fn repeated_headings_are_told_apart() {
        let text = "## Same\none\n## Same\ntwo\n";
        let regions = fold_regions(text);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].key.ordinal, 0);
        assert_eq!(regions[1].key.ordinal, 1);
        assert_eq!(regions[0].key.text, "## Same");
    }

    #[test]
    fn closing_a_fold_hides_its_rows_and_marks_its_heading() {
        let regions = fold_regions(NOTE);
        let mut state = FoldState::default();
        state.close(&regions[1]); // `## Two`, rows 2..=5
        let view = fold_view(&regions, &state, 11);
        assert_eq!(
            view.collapsed,
            vec![false, false, false, true, true, true, false, false, false, false, false],
            "the heading row stays; the rows under it go"
        );
        assert_eq!(view.marker[2], Some(3));
        assert_eq!(view.marker.iter().filter(|m| m.is_some()).count(), 1);
    }

    #[test]
    fn a_closed_section_hides_a_nested_fold_with_it() {
        let regions = fold_regions(NOTE);
        let mut state = FoldState::default();
        state.close(&regions[0]); // `# One`, rows 0..=7
        state.close(&regions[1]); // `## Two`, inside it
        let view = fold_view(&regions, &state, 11);
        assert_eq!(view.marker[0], Some(7));
        assert_eq!(
            view.marker[2], None,
            "the nested fold has no visible row to carry a marker"
        );
        assert!(!view.collapsed[8], "`# Five` is outside");
    }

    #[test]
    fn a_fold_state_starts_empty() {
        let state = FoldState::default();
        assert!(!state.any_closed());
        let view = fold_view(&fold_regions(NOTE), &state, 11);
        assert!(view.collapsed.iter().all(|c| !c));
        assert!(view.marker.iter().all(|m| m.is_none()));
    }
}
