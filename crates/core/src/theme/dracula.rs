//! Dracula, the dark palette.
//! Dracula (https://draculatheme.com,
//! https://github.com/dracula/dracula-theme, MIT) publishes eleven colours: a
//! background, a current-line shade that doubles as the selection, a
//! foreground, a comment, and seven accents. There is no third surface in it,
//! so the chrome reuses Background and Current Line and the handful of roles
//! in between are blends of Comment into Background, commented as such. Every
//! accent in the palette has a job here.

use super::Palette;

/// Dracula: the dark pastel palette, on Background `#282a36`.
///
/// Dracula's own CSS sets the caret to the foreground colour, which Shale
/// would draw invisibly, so the cursor block takes Cyan. Comment does duty
/// for punctuation, quotes and unresolved links, as it does upstream.
pub const PALETTE: Palette = Palette {
    bg: 0x282a36,
    fg: 0xf8f8f2,
    cursor: 0x8be9fd,
    selection: 0x44475a,
    link: 0xbd93f9,
    link_unresolved: 0x6272a4,
    dim: 0x6272a4,
    heading: 0xf8f8f2,
    quote: 0x6272a4,
    accent: 0xff79c6,
    tag: 0x50fa7b,
    code_bg: 0x44475a,
    code_block_bg: 0x232530, // Background darkened 12%: Dracula has no deeper pane
    highlight_bg: 0x5a5e4c,  // yellow #f1fa8c at 25% over #282a36
    table_line: 0x3f4762,    // comment #6272a4 at 40% over #282a36
    table_header_bg: 0x394057, // comment #6272a4 at 30% over #282a36
    callout_bg: 0x394057,    // comment #6272a4 at 30% over #282a36
    ui_bg: 0x282a36,
    ui_status_bg: 0x44475a,
    ui_border: 0x3f4762, // comment #6272a4 at 40% over #282a36
    ui_hover_bg: 0x44475a,
    ui_active_bg: 0x4d5470, // comment #6272a4 at 30% over #44475a
    callout_note: 0x8be9fd,
    callout_teal: 0x50fa7b,
    callout_question: 0xbd93f9,
    callout_warning: 0xf1fa8c,
    callout_failure: 0xffb86c,
    callout_danger: 0xff5555,
};
