//! The one list-window model the popup and the picker share.
//!
//! Neither list is a scrolling element: both draw a *window* over their rows, and
//! that window is derived from which row is highlighted. It is what lets a
//! keyboard-driven list need no scroll state of its own, and it is exactly why the
//! arithmetic has to live in one place — two copies of it are two chances for a
//! widget to scroll its own selection off the panel.
//!
//! The window is *minimal*: it moves only when the selection would otherwise fall
//! outside it, so a list that fits shows all of itself, and a list that does not
//! slides one row at a time.

use std::ops::Range;

/// The first row to draw so that `selected` is inside a window of `visible` rows.
///
/// `total` may be smaller than `visible`, and `selected` may be anywhere in
/// `total` — including past the end, which happens the moment a list shrinks under
/// the selection. Neither is the caller's problem.
pub(crate) fn window_start(selected: usize, total: usize, visible: usize) -> usize {
    let visible = visible.max(1);
    selected
        .min(total.saturating_sub(1))
        .saturating_sub(visible - 1)
        // The window never runs past the end of the list: a row short at the top
        // is better than a gap of nothing at the bottom.
        .min(total.saturating_sub(visible))
}

/// The half-open range of rows to draw, never longer than `visible` and never
/// past `total`.
pub(crate) fn window(selected: usize, total: usize, visible: usize) -> Range<usize> {
    let start = window_start(selected, total, visible);
    start..(start + visible).min(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A list that fits is shown whole, whatever is highlighted.
    #[test]
    fn a_short_list_never_scrolls() {
        for total in 0..=8 {
            for selected in 0..total {
                assert_eq!(window_start(selected, total, 8), 0);
                assert_eq!(window(selected, total, 8), 0..total);
            }
        }
    }

    /// A long list slides by one row, and the selection is always inside.
    #[test]
    fn the_window_follows_the_selection() {
        assert_eq!(window(0, 12, 8), 0..8);
        assert_eq!(window(7, 12, 8), 0..8);
        assert_eq!(window(8, 12, 8), 1..9);
        assert_eq!(window(11, 12, 8), 4..12);
        for selected in 0..12 {
            let drawn = window(selected, 12, 8);
            assert!(drawn.contains(&selected), "row {selected} is off panel");
            assert_eq!(drawn.len(), 8, "the panel is always full height");
        }
    }

    /// A selection past the end — a list that just shrank — still draws a sane
    /// window rather than an empty one.
    #[test]
    fn a_stale_selection_is_clamped_not_wrapped() {
        assert_eq!(window(9, 3, 8), 0..3);
        assert_eq!(window(0, 0, 8), 0..0);
    }
}
