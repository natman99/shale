//! Gruvbox, in its dark and light variants.
//! Gruvbox (https://github.com/morhetz/gruvbox, MIT) is a retro-groove palette:
//! sixteen saturated accents over a warm grey-brown background ramp. Its
//! source works through relative names -- bg0..bg4 from background to border,
//! fg0..fg4 from brightest text to faintest, and the seven accents -- and then
//! re-binds those names for light mode, where the ramp turns to cream and the
//! accents become the darker `faded` family. The palettes here follow those
//! bindings, including gruvbox's own choice of bg3 for a selection and bg4 for
//! line numbers.

use super::Palette;

/// Gruvbox dark: `Normal`'s fg1 `#ebdbb2` on bg0 `#282828`.
///
/// The role mapping follows the upstream highlight groups: `Visual` is bg3,
/// `LineNr` bg4, `Comment` gray, and `CursorLineNr` yellow. Dark mode binds
/// the saturated `bright` accents; light mode the `faded` ones.
pub const DARK: Palette = Palette {
    bg: 0x282828,
    fg: 0xebdbb2,
    cursor: 0xfabd2f,
    selection: 0x665c54,
    link: 0x83a598,
    link_unresolved: 0x7c6f64,
    dim: 0x7c6f64,
    heading: 0xfbf1c7,
    quote: 0x928374,
    accent: 0xd3869b,
    tag: 0xb8bb26,
    code_bg: 0x504945,
    code_block_bg: 0x3c3836,
    highlight_bg: 0x5c4d2a, // yellow #fabd2f at 25% over #282828
    match_text: 0xfabd2f,
    table_line: 0x665c54,
    table_header_bg: 0x3c3836,
    callout_bg: 0x3c3836,
    ui_bg: 0x3c3836,
    ui_status_bg: 0x504945,
    ui_border: 0x665c54,
    ui_hover_bg: 0x504945,
    ui_active_bg: 0x665c54,
    callout_note: 0x83a598,
    callout_teal: 0x8ec07c,
    callout_question: 0xd3869b,
    callout_warning: 0xfabd2f,
    callout_failure: 0xfe8019,
    callout_danger: 0xfb4934,
};

/// Gruvbox light: the same relative names re-bound, as upstream re-binds
/// them in light mode, onto light0 `#fbf1c7` with the `faded` accents.
pub const LIGHT: Palette = Palette {
    bg: 0xfbf1c7, // gruvbox's light0
    fg: 0x3c3836,
    cursor: 0xb57614,
    selection: 0xbdae93,
    link: 0x076678,
    link_unresolved: 0xa89984,
    dim: 0xa89984,
    heading: 0x282828,
    quote: 0x928374,
    accent: 0x8f3f71,
    tag: 0x79740e,
    code_bg: 0xd5c4a1,
    code_block_bg: 0xebdbb2,
    highlight_bg: 0xead29a, // yellow #b57614 at 25% over #fbf1c7
    match_text: 0xaf3a03,
    table_line: 0xbdae93,
    table_header_bg: 0xebdbb2,
    callout_bg: 0xebdbb2,
    ui_bg: 0xebdbb2,
    ui_status_bg: 0xd5c4a1,
    ui_border: 0xbdae93,
    ui_hover_bg: 0xd5c4a1,
    ui_active_bg: 0xbdae93,
    callout_note: 0x076678,
    callout_teal: 0x427b58,
    callout_question: 0x8f3f71,
    callout_warning: 0xb57614,
    callout_failure: 0xaf3a03,
    callout_danger: 0x9d0006,
};
