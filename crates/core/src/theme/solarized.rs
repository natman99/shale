//! Solarized, in its dark and light variants.
//! Solarized (https://ethanschoonover.com/solarized/, the palette mirrored at
//! https://github.com/altercation/solarized) is built as a ladder: base03 and
//! base02 are backgrounds, base01 the comments and faintest text, base00 and
//! base0 the secondary and primary body text on a dark background (base01 and
//! base00 on a light one), and base1..base3 the same values upside down. Its
//! accents carry meaning -- red errors, orange and yellow warnings, green and
//! cyan emphasis, blue information, violet and magenta reserved -- which is how
//! they land in Shale's callout roles.
//!
//! Eight neutrals do not cover as many surfaces as Shale has roles, so the code
//! span and the table header are blends of one base shade into another rather
//! than invented colours, and each one says so. The tone table rates body text
//! on the secondary background as less than optimal reading, so the status bar
//! stays on the note's own background.

use super::Palette;

/// Solarized dark: base0 `#839496` on base03 `#002b36`.
///
/// Eight neutrals do not cover as many surfaces as Shale has roles, so the
/// code span, the table header and the status bar's rows are blends of one
/// base shade into another. And the tone table rates body text on the
/// secondary background as less than optimal reading, so the status bar --
/// it draws body text -- stays on the note's own background.
pub const DARK: Palette = Palette {
    bg: 0x002b36,
    fg: 0x839496,
    cursor: 0xeee8d5,
    selection: 0x073642,
    link: 0x268bd2,
    link_unresolved: 0x586e75,
    dim: 0x586e75,
    heading: 0x93a1a1,
    quote: 0x657b83,
    accent: 0xd33682,
    tag: 0x859900,
    code_bg: 0x1b444f, // base02 with base01 lifted into it; base02 alone is the code block
    code_block_bg: 0x073642,
    highlight_bg: 0x2d4228, // yellow #b58900 at 25% over #002b36
    match_text: 0xb58900,
    table_line: 0x586e75,
    table_header_bg: 0x1b444f, // base01 #586e75 at 25% over #073642
    callout_bg: 0x1b444f,      // base01 #586e75 at 25% over #073642
    ui_bg: 0x073642,
    ui_status_bg: 0x002b36,
    ui_border: 0x586e75,
    ui_hover_bg: 0x1b444f,  // base01 #586e75 at 25% over #073642
    ui_active_bg: 0x30525c, // base01 #586e75 at 50% over #073642
    callout_note: 0x268bd2,
    callout_teal: 0x2aa198,
    callout_question: 0x6c71c4,
    callout_warning: 0xb58900,
    callout_failure: 0xcb4b16,
    callout_danger: 0xdc322f,
};

/// Solarized light: the ladder upside down, base01 `#586e75` on base3
/// `#fdf6e3`.
///
/// Its green and cyan are too faint against a cream background to carry a
/// tag, so a tag takes blue -- the same conclusion Catppuccin's style guide
/// reaches for its light flavour.
pub const LIGHT: Palette = Palette {
    bg: 0xfdf6e3,
    fg: 0x586e75,
    cursor: 0x073642,
    selection: 0xeee8d5,
    link: 0x268bd2,
    link_unresolved: 0x93a1a1,
    dim: 0x93a1a1,
    heading: 0x073642,
    quote: 0x657b83,
    accent: 0xd33682,
    tag: 0x268bd2,
    code_bg: 0xeee8d5,
    code_block_bg: 0xeee8d5,
    highlight_bg: 0xebdbaa, // yellow #b58900 at 25% over #fdf6e3
    match_text: 0xcb4b16,
    table_line: 0x93a1a1,
    table_header_bg: 0xd3d3c5, // base1 #93a1a1 at 30% over #eee8d5
    callout_bg: 0xd3d3c5,      // base1 #93a1a1 at 30% over #eee8d5
    ui_bg: 0xeee8d5,
    ui_status_bg: 0xfdf6e3,
    ui_border: 0x93a1a1,
    ui_hover_bg: 0xeee8d5,
    ui_active_bg: 0xc5c8be, // base1 #93a1a1 at 45% over #eee8d5
    callout_note: 0x268bd2,
    callout_teal: 0x2aa198,
    callout_question: 0x6c71c4,
    callout_warning: 0xb58900,
    callout_failure: 0xcb4b16,
    callout_danger: 0xdc322f,
};
