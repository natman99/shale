//! The display model: soft-wrapped and folded rows.
//!
//! `crate::mdlayout` decides where a long row breaks; the view is told the
//! segments and then the buffer answers what the motions and the paint ask:
//! which display line an offset is on, where that line starts and ends, what
//! column a character sits at.
//!
//! Every method here takes the [`ViewSlot`] it is answering for. That is the
//! point of the signature: a display question without a view attached is a
//! question two views of one note would each answer for the other.

use super::{Buffer, ViewSlot};

impl Buffer {
    /// The absolute offsets at which row `row`'s display lines begin, in this
    /// view. Empty when the row is collapsed: it has no display line, so the
    /// vertical motions step over it and an offset inside it resolves to the
    /// line that follows.
    pub(super) fn segment_starts(&self, view: &ViewSlot, row: usize) -> Vec<usize> {
        view.segments()
            .get(row)
            .cloned()
            .unwrap_or_else(|| vec![self.line_start(row)])
    }
    /// The byte offset where the display line containing `offset` begins.
    pub fn display_start(&self, view: &ViewSlot, offset: usize) -> usize {
        let offset = self.clamp(offset);
        let (row, _) = self.row_col(offset);
        let fallback = self.line_start(row);
        *self
            .segment_starts(view, row)
            .iter()
            .rev()
            .find(|&&s| s <= offset)
            .unwrap_or(&fallback)
    }
    /// The byte range `[start, end)` of the display line at flat index `index`.
    pub fn display_range(&self, view: &ViewSlot, index: usize) -> Option<(usize, usize)> {
        let mut i = 0usize;
        for row in 0..self.line_count() {
            let segs = self.segment_starts(view, row);
            let line_end = self.line_end(row);
            for (k, start) in segs.iter().enumerate() {
                let end = segs.get(k + 1).copied().unwrap_or(line_end);
                if i == index {
                    return Some((*start, end));
                }
                i += 1;
            }
        }
        None
    }
    /// Total number of display lines, wrapped rows included.
    pub fn display_line_count(&self, view: &ViewSlot) -> usize {
        (0..self.line_count())
            .map(|row| self.segment_starts(view, row).len())
            .sum()
    }
    /// Flat display-line index containing `offset`.
    pub fn display_index_for(&self, view: &ViewSlot, offset: usize) -> usize {
        let offset = self.clamp(offset);
        let (row, _) = self.row_col(offset);
        let mut index = 0usize;
        for r in 0..row {
            index += self.segment_starts(view, r).len();
        }
        let segs = self.segment_starts(view, row);
        let within = segs
            .iter()
            .filter(|&&s| s <= offset)
            .count()
            .saturating_sub(1);
        index + within
    }
    /// The byte offset just past the end of the display line containing
    /// `offset` (the next segment start, or the end of the buffer row).
    pub fn display_end(&self, view: &ViewSlot, offset: usize) -> usize {
        let offset = self.clamp(offset);
        let (row, _) = self.row_col(offset);
        let start = self.display_start(view, offset);
        self.segment_starts(view, row)
            .iter()
            .filter(|&&s| s > start)
            .copied()
            .next()
            .unwrap_or_else(|| self.line_end(row))
    }
    /// The character column of `offset` within its display line.
    pub fn display_col(&self, view: &ViewSlot, offset: usize) -> usize {
        let offset = self.clamp(offset);
        let start = self.display_start(view, offset);
        self.text[start..offset].chars().count()
    }
    /// The byte offset `col` characters into `[start, end)`, clamped to `end`.
    pub(super) fn offset_for_char_col(&self, start: usize, end: usize, col: usize) -> usize {
        let mut offset = self.clamp(start).min(end);
        for _ in 0..col {
            if offset >= end {
                break;
            }
            offset = self.next_char_boundary(offset);
        }
        offset.min(end)
    }
}
