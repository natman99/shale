//! The chrome geometry the window shares: the corner radius its chips and panels are
//! drawn with, so "rounded" is one decision rather than one per widget.
//!
//! It lives apart from `shale_core::theme` on purpose: that crate is the palette as
//! data, and a radius is not a colour role. It also has to be reachable from the
//! editor's own elements, which must not reach into `workspace`.
//!
//! The surfaces that take it today are the ones gpui draws from a `Style` — a tab, a
//! file-tree row, a picker row, the picker's panel, and the text field's box. The
//! rest of the look is painted by Shale's own renderer (the line element's
//! backgrounds and highlights, the table grid, the checker's underlines, the fold
//! markers, the review strip's badges), and those need a radius passed into
//! `paint_quad` rather than a style method: that pass is roadmap P6.9, and this is
//! where its radius will come from.

/// The corner radius the window's chips are drawn with: a tab, a file-tree row, a
/// picker row, and the picker's panel.
///
/// Enough to read as rounded at the sizes these are drawn at without turning a row
/// into a pill. A 20px tab stops being a chip at half its height, so 10px is the
/// ceiling before it becomes a capsule.
pub(crate) const CHIP_RADIUS: gpui::Pixels = gpui::px(6.);
