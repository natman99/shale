//! Rosé Pine, in its three variants.
//! Rosé Pine (https://github.com/rose-pine/palette, MIT) publishes one palette
//! file with three variants whose upstream codenames are `main`, `moon` and
//! `dawn`; Shale calls them `rose-pine`, `rose-pine-moon` and `rose-pine-dawn`,
//! which is the spelling that palette's README recommends for file names.
//!
//! Its colour names describe roles -- base and surface are backgrounds,
//! overlay the subtle elements, muted and subtle the quiet text, text the body,
//! love the interactive pink, gold highlights and emphasis, rose borders and
//! muted accents, pine links, foam code and notifications, iris tags and
//! underlines -- which map almost directly onto Shale's roles.

use super::Palette;

/// Rosé Pine, upstream codename `main`, on base `#191724`.
///
/// The palette names its colours for roles, and the mapping follows them:
/// base and surface are the two backgrounds, overlay the subtle elements,
/// muted and subtle the quiet text, text the body, gold highlights, pine
/// links, iris tags, foam code and notifications, love the interactive pink.
///
/// One upstream advice is ignored: `rose` is meant for borders, which makes a
/// table grid shout, so grid lines and window rules take `overlay`.
pub const MAIN: Palette = Palette {
    bg: 0x191724,
    fg: 0xe0def4,
    cursor: 0xf6c177,
    selection: 0x26233a,
    link: 0x31748f,
    link_unresolved: 0x6e6a86,
    dim: 0x6e6a86,
    heading: 0xe0def4,
    quote: 0x908caa,
    accent: 0xeb6f92,
    tag: 0xc4a7e7,
    code_bg: 0x26233a,
    code_block_bg: 0x1f1d2e,
    highlight_bg: 0x504239, // gold #f6c177 at 25% over #191724
    table_line: 0x26233a,
    table_header_bg: 0x26233a,
    callout_bg: 0x26233a,
    ui_bg: 0x1f1d2e,
    ui_status_bg: 0x26233a,
    ui_border: 0x26233a,
    ui_hover_bg: 0x26233a,
    ui_active_bg: 0x3c3950, // text #e0def4 at 12% over #26233a
    callout_note: 0x31748f,
    callout_teal: 0x9ccfd8,
    callout_question: 0xc4a7e7,
    callout_warning: 0xf6c177,
    callout_failure: 0xebbcba,
    callout_danger: 0xeb6f92,
};

/// Rosé Pine Moon, upstream codename `moon`: the same palette on a warmer,
/// lighter base, with a brighter pine and rose.
pub const MOON: Palette = Palette {
    bg: 0x232136,
    fg: 0xe0def4,
    cursor: 0xf6c177,
    selection: 0x393552,
    link: 0x3e8fb0,
    link_unresolved: 0x6e6a86,
    dim: 0x6e6a86,
    heading: 0xe0def4,
    quote: 0x908caa,
    accent: 0xeb6f92,
    tag: 0xc4a7e7,
    code_bg: 0x393552,
    code_block_bg: 0x2a273f,
    highlight_bg: 0x584946, // gold #f6c177 at 25% over #232136
    table_line: 0x393552,
    table_header_bg: 0x393552,
    callout_bg: 0x393552,
    ui_bg: 0x2a273f,
    ui_status_bg: 0x393552,
    ui_border: 0x393552,
    ui_hover_bg: 0x393552,
    ui_active_bg: 0x4d4965, // text #e0def4 at 12% over #393552
    callout_note: 0x3e8fb0,
    callout_teal: 0x9ccfd8,
    callout_question: 0xc4a7e7,
    callout_warning: 0xf6c177,
    callout_failure: 0xea9a97,
    callout_danger: 0xeb6f92,
};

/// Rosé Pine Dawn, upstream codename `dawn`: the light variant, on base
/// `#faf4ed`.
///
/// Dawn inverts the shades as well as the hues -- its raised pane `surface`
/// is lighter than its base and `overlay` is the recessed one -- so the
/// panes swap roles relative to the other two variants.
pub const DAWN: Palette = Palette {
    bg: 0xfaf4ed,
    fg: 0x464261,
    cursor: 0xea9d34,
    selection: 0xf2e9e1,
    link: 0x286983,
    link_unresolved: 0x9893a5,
    dim: 0x9893a5,
    heading: 0x464261,
    quote: 0x797593,
    accent: 0xb4637a,
    tag: 0x907aa9,
    code_bg: 0xf2e9e1,
    code_block_bg: 0xf2e9e1,
    highlight_bg: 0xf6debf, // gold #ea9d34 at 25% over #faf4ed
    table_line: 0xf2e9e1,
    table_header_bg: 0xfffaf3,
    callout_bg: 0xfffaf3,
    ui_bg: 0xfffaf3,
    ui_status_bg: 0xf2e9e1,
    ui_border: 0xf2e9e1,
    ui_hover_bg: 0xf2e9e1,
    ui_active_bg: 0xddd5d2, // text #464261 at 12% over #f2e9e1
    callout_note: 0x286983,
    callout_teal: 0x56949f,
    callout_question: 0x907aa9,
    callout_warning: 0xea9d34,
    callout_failure: 0xd7827e,
    callout_danger: 0xb4637a,
};
