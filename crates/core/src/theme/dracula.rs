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
    cursor_line_bg: 0x44475a,
    selection: 0x44475a,
    link: 0xbd93f9,
    link_unresolved: 0x6272a4,
    dim: 0x6272a4,
    heading: 0xf8f8f2,
    quote: 0x6272a4,
    accent: 0xff79c6,
    tag: 0x50fa7b,
    spell: 0xff5555,
    code_bg: 0x44475a,
    code_block_bg: 0x232530, // Background darkened 12%: Dracula has no deeper pane
    code_text: 0xf8f8f2,
    code_comment: 0x6272a4,
    code_keyword: 0xbd93f9,
    code_string: 0x50fa7b,
    code_number: 0xffb86c,
    code_function: 0x8be9fd,
    code_type: 0xf1fa8c,
    code_const: 0xff79c6,
    code_operator: 0xff5555,
    highlight_bg: 0x5a5e4c, // yellow #f1fa8c at 25% over #282a36
    match_text: 0xffb86c,
    table_line: 0x3f4762,      // comment #6272a4 at 40% over #282a36
    table_header_bg: 0x394057, // comment #6272a4 at 30% over #282a36
    callout_bg: 0x394057,      // comment #6272a4 at 30% over #282a36
    ui_bg: 0x282a36,
    ui_fg: 0xf8f8f2,
    ui_dim: 0x96a1bf, // Comment lifted 35% toward Foreground
    ui_status_bg: 0x44475a,
    ui_border: 0x3f4762, // comment #6272a4 at 40% over #282a36
    ui_active_border: 0xff79c6,
    ui_hover_bg: 0x44475a,
    ui_active_bg: 0x4d5470, // comment #6272a4 at 30% over #44475a
    ui_field_bg: 0x44475a,
    ui_tab_bg: 0x44475a,
    ui_tab_active_bg: 0x525a7b, // comment #6272a4 at 45% over #44475a
    popup_bg: 0x3c435c,         // comment #6272a4 at 35% over #282a36
    popup_border: 0x4b5578,     // comment #6272a4 at 60% over #282a36
    popup_hover_bg: 0x485272,   // comment #6272a4 at 55% over #282a36
    graph_node: 0xbd93f9,
    graph_node_active: 0xffb86c,
    graph_edge: 0x3f4762, // comment #6272a4 at 40% over #282a36
    graph_text: 0x6272a4,
    callout_note: 0x8be9fd,
    callout_teal: 0x50fa7b,
    callout_question: 0xbd93f9,
    callout_warning: 0xf1fa8c,
    callout_failure: 0xffb86c,
    callout_danger: 0xff5555,
};
