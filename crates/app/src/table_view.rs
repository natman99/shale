//! Laying a markdown table out as an aligned grid.
//!
//! The buffer keeps the raw markdown — pipes, dashes and all — because that is
//! what the file on disk holds and what [`shale_core::table`] splices when the
//! user adds or removes a row or column. Painting is a different story: each
//! cell's *content* is drawn at an x offset chosen from the widest content in
//! its column, and the pipes become grid lines, so the user sees a table rather
//! than a row of `|` characters.
//!
//! Geometry is expressed in **character cells**, not pixels, because the editor
//! measures everything else in character columns (soft wrap, the ruler, vim's
//! motions). The line element multiplies by the resolved font's character width,
//! which is exact for the monospace family the editor resolves. Cells wider than
//! their column cannot happen: the column width is the maximum.

use shale_core::table::Table;

/// The blank space, in character cells, painted on each side of a cell's
/// content. The vertical grid line falls in the middle of the two-cell gap
/// between neighbouring columns.
pub(crate) const GUTTER: usize = 1;

/// One cell to paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellPaint {
    /// First byte of the cell's content in the buffer.
    pub(crate) start: usize,
    /// End of the content, exclusive.
    pub(crate) end: usize,
    /// Where the content starts, in character cells from the grid's left edge.
    pub(crate) x: usize,
    /// The column's content width, in character cells.
    pub(crate) width: usize,
    /// Column index.
    pub(crate) column: usize,
}

/// One table row's paint plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TablePaint {
    /// Cells left to right, always the table's column count.
    pub(crate) cells: Vec<CellPaint>,
    /// The delimiter row, drawn as a line of its own with the rule along its top
    /// edge. Live preview collapses this row away unless the cursor is on it or
    /// on the header above it, so it is only seen while editing `:---:`.
    pub(crate) delimiter: bool,
    /// The row directly above the delimiter, which carries the rule along its
    /// bottom edge when the delimiter row is collapsed out of the display model.
    pub(crate) rule_below: bool,
    /// The header row (the one above the delimiter), which gets a background.
    pub(crate) header: bool,
    /// x positions (character cells) of the vertical grid lines, the outer edges
    /// included, left to right. Always one more than the column count.
    pub(crate) boundaries: Vec<usize>,
    /// The grid's total width in character cells.
    pub(crate) total: usize,
}

impl TablePaint {
    /// The cell containing a buffer offset, or the nearest one to its left. Used
    /// to place the cursor and to turn a click into a buffer offset.
    pub(crate) fn cell_for(&self, offset: usize) -> Option<&CellPaint> {
        self.cells
            .iter()
            .find(|c| c.start <= offset && offset <= c.end)
            .or_else(|| {
                self.cells
                    .iter()
                    .rev()
                    .find(|c| c.end < offset)
                    .or_else(|| self.cells.first())
            })
    }
}

/// The paint plan for every buffer row that belongs to a table, indexed by
/// buffer row (`None` for ordinary rows). `line_count` is the buffer's line
/// count, which sizes the result.
///
/// `collapsed` marks the rows live preview gives no display line at all, which
/// decides where the rule under the header is painted: on the delimiter row's
/// top edge when that row is drawn, and on the header's bottom edge when it is
/// not. The two are the same line, so the rule is painted exactly once either
/// way.
pub(crate) fn plans_by_row(
    text: &str,
    tables: &[Table],
    line_count: usize,
    collapsed: &[bool],
) -> Vec<Option<TablePaint>> {
    let mut out: Vec<Option<TablePaint>> = vec![None; line_count];
    for table in tables {
        let widths = table.column_widths(text);
        // Each column owns its content plus a gutter on both sides, so the
        // boundary between two columns sits in the middle of their shared gap.
        let mut boundaries = Vec::with_capacity(widths.len() + 1);
        let mut offsets = Vec::with_capacity(widths.len());
        let mut x = 0usize;
        boundaries.push(x);
        for width in &widths {
            offsets.push(x + GUTTER);
            x += width + 2 * GUTTER;
            boundaries.push(x);
        }
        let total = x;
        // Whether the delimiter row is drawn this frame, which moves the rule
        // between it and the header above.
        let rule_on_header = table
            .rows
            .get(table.delimiter)
            .map(|row| collapsed.get(row.row).copied().unwrap_or(true))
            .unwrap_or(true);
        for (index, row) in table.rows.iter().enumerate() {
            if row.row >= out.len() {
                continue;
            }
            let cells = row
                .cells
                .iter()
                .enumerate()
                .map(|(column, cell)| CellPaint {
                    start: cell.start,
                    end: cell.end,
                    x: *offsets.get(column).unwrap_or(&0),
                    width: *widths.get(column).unwrap_or(&0),
                    column,
                })
                .collect();
            out[row.row] = Some(TablePaint {
                cells,
                delimiter: index == table.delimiter,
                rule_below: index + 1 == table.delimiter && rule_on_header,
                header: index < table.delimiter,
                boundaries: boundaries.clone(),
                total,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shale_core::table::tables;

    /// The painted content of each row, as `(column, text, x)` triples.
    fn painted(plans: &[Option<TablePaint>], text: &str) -> Vec<Vec<(usize, String, usize)>> {
        plans
            .iter()
            .map(|plan| {
                plan.as_ref()
                    .map(|p| {
                        p.cells
                            .iter()
                            .map(|c| (c.column, text[c.start..c.end].to_string(), c.x))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect()
    }

    #[test]
    fn columns_line_up_on_the_widest_content() {
        let text = "| a | bb |\n| --- | --- |\n| 111 | 2 |\n";
        let plans = plans_by_row(text, &tables(text), 4, &[]);
        // Column 0 is as wide as `111`, so every row puts column 1's content at
        // the same offset.
        let xs: Vec<usize> = painted(&plans, text)
            .iter()
            .filter(|row| !row.is_empty())
            .map(|row| row.iter().find(|c| c.0 == 1).unwrap().2)
            .collect();
        assert_eq!(xs, vec![6, 6, 6], "one gutter, three chars, one gutter");
        assert_eq!(
            painted(&plans, text)[2],
            vec![(0, "111".to_string(), 1), (1, "2".to_string(), 6),]
        );
    }

    #[test]
    fn the_delimiter_row_is_marked_and_the_grid_has_one_line_per_edge() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n";
        // The delimiter row is collapsed out of the display model.
        let plans = plans_by_row(text, &tables(text), 3, &[false, true, false]);
        let flags: Vec<(bool, bool)> = plans
            .iter()
            .map(|p| {
                p.as_ref()
                    .map(|p| (p.header, p.delimiter))
                    .unwrap_or((false, false))
            })
            .collect();
        assert_eq!(
            flags,
            vec![(true, false), (false, true), (false, false)],
            "the header is the row above the delimiter"
        );
        let rules: Vec<bool> = plans
            .iter()
            .map(|p| p.as_ref().is_some_and(|p| p.rule_below))
            .collect();
        assert_eq!(
            rules,
            vec![true, false, false],
            "only the row above the delimiter carries the rule when that row is collapsed"
        );
        for plan in plans.iter().flatten() {
            assert_eq!(plan.boundaries.len(), 3, "two columns, three lines");
            assert_eq!(plan.boundaries[0], 0, "the grid starts at the left edge");
            assert_eq!(plan.boundaries.last(), Some(&plan.total));
        }
    }

    #[test]
    fn ordinary_rows_have_no_plan() {
        let text = "prose\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let plans = plans_by_row(text, &tables(text), 5, &[]);
        assert_eq!(plans.len(), 5);
        assert!(plans[0].is_none() && plans[1].is_none());
        assert!(plans[2].is_some() && plans[4].is_some());
    }

    #[test]
    fn a_drawn_delimiter_row_carries_the_rule_itself() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n";
        // The cursor is on the header, so live preview draws the delimiter row:
        // the rule belongs to that row's top edge, not the header's bottom.
        let plans = plans_by_row(text, &tables(text), 3, &[false, false, false]);
        let rules: Vec<(bool, bool)> = plans
            .iter()
            .map(|p| {
                p.as_ref()
                    .map(|p| (p.rule_below, p.delimiter))
                    .unwrap_or((false, false))
            })
            .collect();
        assert_eq!(
            rules,
            vec![(false, false), (false, true), (false, false)],
            "the rule is painted once, on the row that is drawn"
        );
    }

    #[test]
    fn a_ragged_row_still_gets_every_column() {
        let text = "| a | b | c |\n| --- | --- | --- |\n| 1 |\n";
        let plans = plans_by_row(text, &tables(text), 3, &[]);
        let last = plans[2].as_ref().unwrap();
        assert_eq!(last.cells.len(), 3);
        assert_eq!(last.cells[2].start, last.cells[2].end, "empty cell");
        // Every row shares the grid, so the empty cell still lands in its column.
        assert_eq!(last.cells[2].x, plans[0].as_ref().unwrap().cells[2].x);
    }

    #[test]
    fn a_cursor_on_a_pipe_or_padding_resolves_to_the_nearest_cell() {
        let text = "| a | bb |\n| --- | --- |\n| 1 | 2 |\n";
        let plans = plans_by_row(text, &tables(text), 3, &[]);
        let row = plans[2].as_ref().unwrap();
        let line_start = text.find("| 1 |").unwrap();
        // The leading pipe sits left of the first cell's content.
        assert_eq!(row.cell_for(line_start).unwrap().column, 0);
        // The trailing pipe sits right of the last cell's content.
        assert_eq!(row.cell_for(line_start + 6).unwrap().column, 1);
    }
}
