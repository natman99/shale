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
//!
//! The `GLYPH_*` set is the chrome's marks, named so they stop being characters
//! each widget picks for itself (`design-language.md` §8). Shale is a text-first
//! app: its marks are unicode the note's own face can draw, not icons, and the set
//! is the whole set -- a new mark is added here before it is used, and the only
//! exception is a surface that paints its own vector mark, which today is a
//! callout's bar, a graph node's kind and the agent's presence mark.
//!
//! Buffer content is never in that set and never icon-ified. A task box, a heading
//! marker, a list bullet, a table pipe and a link's brackets stay visible text:
//! that is what Raw mode is *for*, and iconing them would mean WYSIWYG, which is a
//! different product.

// Needed for the styling and child-adding methods the key-chip treatment below calls;
// the constants above are plain values and never needed them.
use gpui::{ParentElement, Styled};

/// The corner radius the window's chips are drawn with: a tab, a file-tree row, a
/// picker row, and the picker's panel.
///
/// Enough to read as rounded at the sizes these are drawn at without turning a row
/// into a pill. A 20px tab stops being a chip at half its height, so 10px is the
/// ceiling before it becomes a capsule.
pub(crate) const CHIP_RADIUS: gpui::Pixels = gpui::px(6.);

/// The note's left gutter: a column of blank note, inside the editor's scroll clip,
/// reserved for drawn marks that belong to a line rather than to its text.
///
/// It lives on the editor's scroller and not on the workspace's note column for one
/// hard reason: `overflow_scroll` clips to the scroller's own bounds, so a mark painted
/// left of the scroller's content edge is outside the mask and never appears. Padding is
/// inside that mask (gpui subtracts only borders), so padding on the scroller is what
/// makes the gutter paintable at all.
///
/// **This is a shared column, not the callout mark's slot.** A callout's kind mark and a
/// folded heading's disclosure glyph sit in it today ([`crate::line::marks`]); line
/// numbers, a changed-line rule and an agent's provenance mark all want the same place,
/// and each of them would otherwise have to rediscover the clipping rule and re-pick a
/// width. Anything placed here is positioned from [`GUTTER`] so it cannot drift out of
/// the space the layout actually reserved — the test in `line::tests` holds that
/// boundary.
pub(crate) const GUTTER: gpui::Pixels = gpui::px(16.);

/// The corner radius of a pane's card: its rule, its tab bar's top and the
/// note's bottom round by this much, so a tiled window reads as rounded panes
/// inside the compositor's own rounded frame rather than squares inside
/// circles.
pub(crate) const PANE_RADIUS: gpui::Pixels = gpui::px(8.);

/// The side of the square a gutter mark is drawn in.
///
/// Square so several marks can share the column on one row without overlapping, and small
/// enough that [`GUTTER`] leaves real air on both sides rather than crowding the text.
pub(crate) const MARK_SIZE: gpui::Pixels = gpui::px(8.);

/// The least air a gutter mark keeps between itself and the text column.
///
/// A mark that touches the first glyph reads as part of the word rather than as a label
/// for the line.
pub(crate) const MARK_GUT: gpui::Pixels = gpui::px(4.);

/// The font size a **shaped** gutter mark is drawn at, whatever the row beside it is
/// sized at.
///
/// A path mark is handed its size ([`MARK_SIZE`]); a glyph mark has to be shaped at a
/// font size, and that size cannot be the row's. A level-one heading is 22 px, and its
/// `▸` shapes 13.2 px wide — wider than [`GUTTER`] minus [`MARK_GUT`], so gpui crops it
/// at the column's left edge in silence. Every mark in the column is drawn at one size
/// — or at the row's size if the row is smaller, which `LineElement::gutter_glyph_size`
/// decides — which is also what makes the gutter read as one column rather than as a
/// ragged edge echoing the note's headings.
///
/// The size is measured, not guessed: `▸` in the note's monospace shapes at 0.6 em, so
/// at 14 px it is 8.4 px wide and reads as a speck beside a heading (user report,
/// 2026-11-08). At 17 px it is 10.2 px, which leaves 1.8 px of air at the column's left
/// edge — the largest step that still fits, and the reason the cap is 17 rather than the
/// heading's own 22. `line::tests::the_disclosure_glyph_fits_the_gutter` holds that
/// boundary; raising this constant past 18 breaks it.
pub(crate) const GUTTER_GLYPH_SIZE: f32 = 17.0;

/// How heavy a chip's rule is.
///
/// Heavier than the window's other 1 px rules on purpose: a chip is a box around two
/// characters at [`TEXT_XS`], and at that size a hairline reads as a smudge rather than
/// as a key. It is still a rule and not a fill, so §5's elevation levels are untouched.
/// Fractional rather than gpui's `border_2` because the ask was a tad, not twice: at a
/// 2× display this is 3 device pixels where `border_2` would be 4.
pub(crate) const CHIP_RULE: gpui::Pixels = gpui::px(1.5);

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

/// The chrome's motion tokens, and the contract they arrive with.
///
/// **The chrome does not animate, except toasts** (`design-language.md` §7): no
/// fade on a tab switch, no sliding panel, no easing on a resize. That is true
/// today because nothing in the chrome animates at all -- the only motion in the
/// window is the graph's idle layout, which has its own ruled budget (M5a) and
/// is not chrome.
///
/// The tokens are for the exception. A toast is the one thing allowed to move,
/// and when it does it takes one of these two and a reason in the commit
/// message; a third duration, or an easing invented at a paint site, is the
/// drift this section exists to stop. gpui's `Animation` takes a
/// [`std::time::Duration`] and an easing function, and its `ease_in_out` is the
/// one to reach for unless there is a reason not to.
///
/// A surface that wants to animate something else is amending §7, not writing a
/// line of code.
#[allow(dead_code)]
pub(crate) const MOTION_FAST: std::time::Duration = std::time::Duration::from_millis(120);

/// How long something that travels across the window takes: 240 ms. See
/// [`MOTION_FAST`] for what may use it.
#[allow(dead_code)]
pub(crate) const MOTION_SLOW: std::time::Duration = std::time::Duration::from_millis(240);

/// The chrome's text sizes, as rem steps.
///
/// The note's type scale lives with the note: a base size from the config, a
/// proportional heading scale, one line ratio. The chrome's is these three steps
/// and no surface picking its own (`design-language.md` §6). They are gpui's
/// `text_xs`, `text_sm` and `text_lg` spelled out as rems rather than called as
/// helpers, so that one module holds the number and a density setting (§16) has
/// one multiplier to reach for instead of eight call sites to find.
pub(crate) const TEXT_XS: gpui::Rems = gpui::rems(0.75);
/// A chrome label that is not a row: 0.875 rem, gpui's `text_sm`.
pub(crate) const TEXT_SM: gpui::Rems = gpui::rems(0.875);
/// The one large label in the chrome, on the graph's empty canvas: 1.125 rem,
/// gpui's `text_lg`.
#[allow(dead_code)]
pub(crate) const TEXT_LG: gpui::Rems = gpui::rems(1.125);

/// A folder the file tree has closed.
pub(crate) const GLYPH_COLLAPSED: &str = "▸";

/// A folder the file tree has opened.
pub(crate) const GLYPH_EXPANDED: &str = "▾";

/// A note whose buffer holds edits the disk does not.
///
/// Drawn in `Palette::accent`, so the state is carried twice, by the mark and by
/// the colour (`design-language.md` §10).
pub(crate) const GLYPH_DIRTY: &str = "●";

/// Closing one tab.
pub(crate) const GLYPH_CLOSE: &str = "×";

/// One key-chip: the key in a small boxed rule, then what it does in plain words.
///
/// `design-language.md` §9 asks for keymap legends to render as boxed chips rather than
/// prose wherever they appear, so the key reads as a key and the word beside it reads as
/// the explanation rather than as more of the key. This is the whole treatment, so a
/// second legend cannot invent a second look.
///
/// The colours are arguments rather than a `Palette` import because this module owns the
/// *treatment* and never decides which role a surface takes: the caller passes the roles
/// it already owns, and the chip follows the surface it is drawn on.
///
/// A chip is a rule and a corner, not a fill. It delimits text; it does not claim to be
/// above the surface, so it takes none of §5's elevation and needs no contrast floor of
/// its own — the key is judged against whatever the chip sits on, exactly as the meaning
/// beside it already is. The rule is [`CHIP_RULE`], a little heavier than the window's
/// other hairlines, because a 1 px box around two characters at [`TEXT_XS`] reads as a
/// smudge rather than as a key.
pub(crate) fn key_chip(
    key: &str,
    meaning: &str,
    rule: u32,
    key_fg: u32,
    meaning_fg: u32,
) -> gpui::Div {
    gpui::div()
        .flex()
        .items_center()
        .gap(SPACE_1)
        .child(
            gpui::div()
                .px(SPACE_1)
                .border(CHIP_RULE)
                .border_color(gpui::rgb(rule))
                // The same corner as every other chip in the window, so a key chip and a
                // tab and a picker row are one shape language.
                .rounded(CHIP_RADIUS)
                .text_color(gpui::rgb(key_fg))
                .child(key.to_string()),
        )
        .child(
            gpui::div()
                .text_color(gpui::rgb(meaning_fg))
                .child(meaning.to_string()),
        )
}

/// A whole legend: `(key, meaning)` pairs, each a [`key_chip`], with [`SPACE_2`] between
/// pairs and [`SPACE_1`] inside one so a chip and its word read as a unit and the pairs
/// read as a list.
pub(crate) fn key_legend(
    legend: &[(&str, &str)],
    rule: u32,
    key_fg: u32,
    meaning_fg: u32,
) -> gpui::Div {
    gpui::div().flex().items_center().gap(SPACE_2).children(
        legend
            .iter()
            .map(|(key, meaning)| key_chip(key, meaning, rule, key_fg, meaning_fg)),
    )
}
