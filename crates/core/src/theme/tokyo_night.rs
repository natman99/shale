//! Tokyo Night, in its three official variants.
//! Tokyo Night is enkia's palette for working at night
//! (https://github.com/tokyo-night/tokyo-night-vscode-theme): blue-purple sky,
//! cyan and green for the things that matter, plus one daylight variant.
//! Shale ships all three -- `tokyo-night`, `tokyo-night-storm` (a lighter,
//! less blue background) and `tokyo-night-light`.
//!
//! Two upstream files describe this family. The VS Code themes above carry
//! the workbench shades and the alpha the selection is drawn at;
//! tokyonight.nvim (https://github.com/folke/tokyonight.nvim) names the dark
//! palette's colours the way its users say them (`fg_dark`, `bg_highlight`,
//! `fg_gutter`) and gives it the two deeper panes. Its day style is computed
//! by inversion, so the light palette is taken from the VS Code file
//! instead. Both upstreams are MIT-licensed, so the hexes may live in this
//! Apache-2.0 crate.

use super::Palette;

/// Tokyo Night: the default variant, on background `#1a1b26`.
///
/// Named as in `night.lua`; the deeper panes (`bg_dark`, `bg_dark1`), the
/// gutter shade and the link colour come from the official VS Code theme,
/// which also supplies the 30% alpha the selection is drawn at. Tokyo Night
/// has no cursor colour of its own, so the block takes the pale `cyan`.
pub const NIGHT: Palette = Palette {
    bg: 0x1a1b26,
    fg: 0xc0caf5,
    cursor: 0x7dcfff,
    selection: 0x2a2e40, // #515c7e at 30%, the alpha that theme applies
    link: 0x6183bb,      // `textLink.foreground` of the official theme
    link_unresolved: 0x545c7e,
    dim: 0x565f89,
    heading: 0xc0caf5,
    quote: 0xa9b1d6,
    accent: 0xbb9af7,
    tag: 0x9ece6a,
    code_bg: 0x292e42,
    code_block_bg: 0x16161e,
    highlight_bg: 0x4c4036, // yellow #e0af68 at 25% over #1a1b26
    match_text: 0xff9e64,
    table_line: 0x3b4261,
    table_header_bg: 0x292e42,
    callout_bg: 0x292e42,
    ui_bg: 0x16161e,
    ui_status_bg: 0x0c0e14, // `bg_dark1`, the palette's deepest pane
    ui_border: 0x3b4261,
    ui_hover_bg: 0x292e42,
    ui_active_bg: 0x3b4261,
    callout_note: 0x7aa2f7,
    callout_teal: 0x1abc9c,
    callout_question: 0x9d7cd8,
    callout_warning: 0xe0af68,
    callout_failure: 0xff9e64,
    callout_danger: 0xf7768e,
};

/// Tokyo Night Storm: the same palette on a lighter, less blue background,
/// `#24283b`, for people who find the default too dark.
///
/// Everything is from `storm.lua`; its token colours are the same as the
/// default variant's, only the three background shades differ.
pub const STORM: Palette = Palette {
    bg: 0x24283b,
    fg: 0xc0caf5,
    cursor: 0x7dcfff,
    selection: 0x32384f, // #515c7e at 30%, the alpha the official theme applies
    link: 0x6183bb,      // `textLink.foreground` of the official theme
    link_unresolved: 0x545c7e,
    dim: 0x565f89,
    heading: 0xc0caf5,
    quote: 0xa9b1d6,
    accent: 0xbb9af7,
    tag: 0x9ece6a,
    code_bg: 0x292e42,
    code_block_bg: 0x1f2335,
    highlight_bg: 0x534a46, // yellow #e0af68 at 25% over #24283b
    match_text: 0xff9e64,
    table_line: 0x3b4261,
    table_header_bg: 0x292e42,
    callout_bg: 0x292e42,
    ui_bg: 0x1f2335,
    ui_status_bg: 0x1b1e2d, // `bg_dark1`, the palette's deepest pane
    ui_border: 0x3b4261,
    ui_hover_bg: 0x292e42,
    ui_active_bg: 0x3b4261,
    callout_note: 0x7aa2f7,
    callout_teal: 0x1abc9c,
    callout_question: 0x9d7cd8,
    callout_warning: 0xe0af68,
    callout_failure: 0xff9e64,
    callout_danger: 0xf7768e,
};

/// Tokyo Night Light: the day variant, on background `#e6e7ed`.
///
/// tokyonight.nvim derives its day style by inverting the dark palette, so
/// these are the literal values of the official VS Code theme
/// `tokyo-night-light-color-theme.json`: `#d6d8df` chrome, `#c1c2c7`
/// borders, and the theme's own link, comment and token shades.
pub const LIGHT: Palette = Palette {
    bg: 0xe6e7ed,
    fg: 0x343b59,
    cursor: 0x0f4b6e,    // the theme's dark cyan, since its cyan has no light twin
    selection: 0xd8d9e2, // the theme's selection shade at the 25% alpha it applies
    link: 0x2959aa,
    link_unresolved: 0x707280,
    dim: 0x888b94,
    heading: 0x363c4d,
    quote: 0x4e5579,
    accent: 0x65359d,
    tag: 0x385f0d, // the light theme's green: a tag is not a link here
    code_bg: 0xdadce3,
    code_block_bg: 0xd6d8df,
    highlight_bg: 0xd0c5b7, // yellow #8f5e15 at 25% over #e6e7ed
    match_text: 0x965027,
    table_line: 0xc1c2c7,
    table_header_bg: 0xdcdee3,
    callout_bg: 0xdcdee3,
    ui_bg: 0xd6d8df,
    ui_status_bg: 0xc1c2c7, // the deepest shade of the theme's chrome
    ui_border: 0xc1c2c7,
    ui_hover_bg: 0xe1e2e8,
    ui_active_bg: 0xe6e7ed,
    callout_note: 0x0f4b6e,
    callout_teal: 0x33635c,
    callout_question: 0x65359d,
    callout_warning: 0x8f5e15,
    callout_failure: 0x965027,
    callout_danger: 0x942f2f,
};
