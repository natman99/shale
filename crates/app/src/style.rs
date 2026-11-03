//! The chrome geometry the window shares: the corner radius its chips and panels are
//! drawn with, so "rounded" is one decision rather than one per widget, and the
//! spacing scale its gaps are picked from.
//!
//! It lives apart from `shale_core::theme` on purpose: that crate is the palette as
//! data, and a radius is not a colour role. It also has to be reachable from the
//! editor's own elements, which must not reach into `workspace`.
//!
//! The surfaces that take the radius are the ones gpui draws from a `Style` — a tab, a
//! file-tree row, a picker row, the review strip's rows, the checker's panel, the
//! picker's panel, and the text field's box (top corners only, because the underline
//! *is* the field's edge) — plus the graph's nodes and filter chips, which pass it to
//! `paint_quad` themselves.
//!
//! Not everything is rounded, and this module is where that is decided rather than
//! left to each painter (roadmap P6.9). Deliberately square: the caret and the
//! selection, because vim's block cursor has to sit on a cell; a 1px rule (a table
//! grid, a thematic break, the field's underline) and the 2px callout bar, where a
//! 6px radius is larger than the thing it would round; and the note's full-width
//! bands (the cursor's line, a fenced block, a callout box), which are one quad per
//! row and would come out as a stack of pills. Rounding a block's *ends* needs the
//! block to know which of its rows is first and last, which the layout pass does not
//! say today.

/// The corner radius the window's chips are drawn with: a tab, a file-tree row, a
/// picker row, and the picker's panel.
///
/// Enough to read as rounded at the sizes these are drawn at without turning a row
/// into a pill. A 20px tab stops being a chip at half its height, so 10px is the
/// ceiling before it becomes a capsule.
pub(crate) const CHIP_RADIUS: gpui::Pixels = gpui::px(6.);

/// The spacing scale the chrome's gaps are picked from: 4, 8, 12 and 16 px.
///
/// A surface that needs a gap takes the nearest step rather than inventing a
/// number, and an off-scale number in a diff is a review comment
/// (`design-language.md` §2). These are gpui's own quarter-rems at the default
/// text size -- `p_1` is [`SPACE_1`], `p_2` is [`SPACE_2`] -- so a div can be
/// written either way and mean the same thing. The names are for the places
/// that are not a div, and for the places where the step is the thing being
/// said: [`SPACE_1`] is "inside a row", [`SPACE_2`] is "between rows", and
/// [`SPACE_4`] is "this is a region of its own".
pub(crate) const SPACE_1: gpui::Pixels = gpui::px(4.);
/// The second step of [`SPACE_1`]'s scale: 8 px.
pub(crate) const SPACE_2: gpui::Pixels = gpui::px(8.);
/// The third step of [`SPACE_1`]'s scale: 12 px.
///
/// Nothing claims it yet. The scale is defined whole so that a surface picking a
/// step has the step to pick -- an unclaimed step is not dead code, and a
/// surface that invents 11 px instead is the thing this module exists to stop.
#[allow(dead_code)]
pub(crate) const SPACE_3: gpui::Pixels = gpui::px(12.);
/// The fourth step of [`SPACE_1`]'s scale: 16 px.
///
/// Named for the same reason as [`SPACE_3`]. Where a 16 px gap is already in
/// the chrome it is written as gpui's `gap_4`, which is the same quarter-rems.
#[allow(dead_code)]
pub(crate) const SPACE_4: gpui::Pixels = gpui::px(16.);
