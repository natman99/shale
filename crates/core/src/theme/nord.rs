//! Nord, the arctic palette.
//! Nord (https://nordtheme.com, https://github.com/nordtheme/nord, MIT) is 16
//! colours in four groups, and its own source says what each group is for:
//! Polar Night (nord0-3) is the interface and its shaded layers, Snow Storm
//! (nord4-6) carries text, Frost (nord7-10) accents user-interface parts, and
//! Aurora (nord11-15) marks things -- errors, warm warnings, attention,
//! success, and the purple that means magic. Shale's roles line up with that.

use super::Palette;

/// Nord: arctic blue-grey, on Polar Night nord0 `#2e3440`.
///
/// Nord has no colour lighter than nord4 for quiet text, and nord3 on
/// nord0 measures 1.7:1, which is fine for a punctuation mark beside body
/// text and not fine for a paragraph or a link. The three quiet text roles
/// therefore use nord3 lifted toward nord4, by increasing amounts.
pub const PALETTE: Palette = Palette {
    bg: 0x2e3440,
    fg: 0xd8dee9,
    cursor: 0x88c0d0,
    cursor_line_bg: 0x3c424e, // nord4 barely into nord0: a band, not a surface
    selection: 0x4c566a,
    link: 0x81a1c1,
    link_unresolved: 0x767f90, // nord4 #d8dee9 at 30% over #4c566a
    dim: 0x7d8696,             // nord4 #d8dee9 at 35% over #4c566a
    heading: 0xeceff4,
    quote: 0x8b93a3, // nord3 lifted 45% toward nord4: too faint alone for a paragraph
    accent: 0x88c0d0,
    tag: 0xa3be8c,
    spell: 0xbf616a,
    code_bg: 0x434c5e,
    code_block_bg: 0x3b4252,
    code_text: 0xd8dee9,
    code_comment: 0x7d8696, // nord4 #d8dee9 at 35% over #4c566a
    code_keyword: 0xb48ead,
    code_string: 0xa3be8c,
    code_number: 0xebcb8b,
    code_function: 0x88c0d0,
    code_type: 0x8fbcbb,
    code_const: 0xd08770,
    code_operator: 0x5e81ac,
    highlight_bg: 0x5d5a53, // nord13 #ebcb8b at 25% over #2e3440
    match_text: 0x88c0d0,
    table_line: 0x434c5e,
    table_header_bg: 0x3b4252,
    callout_bg: 0x3b4252,
    ui_bg: 0x3b4252,
    ui_fg: 0xd8dee9,
    ui_dim: 0x8b93a3,       // nord4 #d8dee9 at 45% over #4c566a
    ui_status_bg: 0x3b4252, // Nord has no pane deeper than nord1
    ui_border: 0x4c566a,
    ui_active_border: 0x88c0d0,
    ui_hover_bg: 0x434c5e,
    ui_active_bg: 0x4c566a,
    ui_field_bg: 0x434c5e,
    ui_tab_bg: 0x4c566a,
    ui_tab_active_bg: 0x687183, // nord4 #d8dee9 at 20% over #4c566a
    popup_bg: 0x434c5e,
    popup_border: 0x777f8f,   // nord4 #d8dee9 at 35% over #434c5e
    popup_hover_bg: 0x555e6f, // nord4 #d8dee9 at 12% over #434c5e
    graph_node: 0x88c0d0,
    graph_node_active: 0xebcb8b,
    graph_edge: 0x434c5e,
    graph_text: 0x8b93a3, // nord4 #d8dee9 at 45% over #4c566a
    callout_note: 0x81a1c1,
    callout_teal: 0x8fbcbb,
    callout_question: 0xb48ead,
    callout_warning: 0xebcb8b,
    callout_failure: 0xd08770,
    callout_danger: 0xbf616a,
};
