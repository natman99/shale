//! The four Catppuccin flavours, as [`Palette`] values.
//!
//! Catppuccin (https://catppuccin.com) is a pastel palette in four flavours that
//! differ only in how light their backgrounds are: Latte (light), Frappé,
//! Macchiato and Mocha (dark). Every colour here is taken verbatim from
//! `palette.json` v1.8.0 of https://github.com/catppuccin/palette (MIT-licensed,
//! so the hexes may be copied into this Apache-2.0 crate), so a Shale note
//! matches the rest of a user's Catppuccin setup.
//!
//! The palette's 26 named colours do not line up one-to-one with Shale's 53
//! roles, so each role is filled from the flavour by the mapping in Catppuccin's
//! own style guide (https://github.com/catppuccin/catppuccin/blob/main/docs/style-guide.md,
//! kept at `refs/catppuccin/style-guide.md`), with the hexes from
//! `palette.json` v1.8.0:
//!
//! | `Palette` role             | Catppuccin colour                     |
//! |----------------------------|---------------------------------------|
//! | `bg`                       | Base (the "background pane")          |
//! | `fg`, `heading`, `ui_fg`   | Text (body copy and main headlines)   |
//! | `cursor`                   | Rosewater                             |
//! | `cursor_line_bg`           | Text over Base, 10%                   |
//! | `selection`                | Overlay 2 over Base, 25%              |
//! | `link`, `tag`, `ui_*_border`| Blue / Overlay 0 (see below)         |
//! | `link_unresolved`          | Overlay 0 (subtle, not an error red)  |
//! | `dim`                      | Overlay 1 ("subtle", line numbers)    |
//! | `quote`, `ui_dim`          | Subtext 0 (sub-headlines and labels)  |
//! | `accent`                   | Mauve                                 |
//! | `spell`                    | Red ("errors")                        |
//! | `code_bg`                  | Surface 1                             |
//! | `code_block_bg`            | Mantle (a secondary pane)             |
//! | `code_text`                | Text                                  |
//! | `code_comment`             | Overlay 2                             |
//! | `code_keyword`             | Mauve                                 |
//! | `code_string`              | Green                                 |
//! | `code_number`, `code_const`| Peach, Red                          |
//! | `code_function`            | Blue                                  |
//! | `code_type`                | Yellow                              |
//! | `code_operator`            | Sky                                 |
//! | `highlight_bg`             | Yellow over Base, 25%                 |
//! | `match_text`               | Peach, or Mauve for Latte (see below) |
//! | `table_line`               | Surface 2                             |
//! | `table_header_bg`          | Surface 0                             |
//! | `callout_bg`               | Surface 0 (a surface element)         |
//! | `ui_bg`, `ui_status_bg`    | Mantle, Crust (the secondary panes)   |
//! | `ui_border`, `graph_edge`  | Overlay 0 (an inactive border)        |
//! | `ui_active_border`         | Lavender (an active border)           |
//! | `ui_hover_bg`              | Surface 0                             |
//! | `ui_active_bg`             | Surface 1                             |
//! | `ui_field_bg`              | Surface 0 (a control)                 |
//! | `ui_tab_bg`, `popup_bg`    | Surface 0 (a surface element)         |
//! | `ui_tab_active_bg`         | Surface 1                             |
//! | `popup_border`             | Overlay 0                             |
//! | `popup_hover_bg`           | Surface 0 with Text, 12%              |
//! | `graph_node`, `graph_node_active` | Blue, Sky (a link, and a link on hover) |
//! | `graph_text`               | Subtext 0                             |
//! | `callout_note`             | Blue                                  |
//! | `callout_teal`             | Teal                                  |
//! | `callout_question`         | Mauve                                 |
//! | `callout_warning`          | Yellow ("warnings")                   |
//! | `callout_failure`          | Peach                                 |
//! | `callout_danger`           | Red ("errors")                        |
//!
//! The guide states `selection`, `highlight_bg` and `cursor_line_bg` as an
//! accent drawn *over* Base at an opacity, and a `Palette` has no alpha channel,
//! so all three are composited onto Base once here (at the middle of the guide's
//! 20%-30% range for selections) and stored as the flat colour the reader ends
//! up seeing. The panel's highlighted row is the same trick with the guide's own
//! 10%-ish "raised surface" step, because a hover row is not one of the
//! palette's named surfaces.
//!
//! `heading` is Text rather than an accent because that is what the guide says
//! for a main headline; a Shale heading is already larger and bold, so it does
//! not need a hue to separate it from body text.
//!
//! A query's matched characters take Peach, the accent the flavours use for a
//! hit. Latte's Peach (`#fe640b`) measures 2.5:1 on that flavour's highlighted
//! row, which is not enough for a few letters among the rest of a row, so the
//! light flavour falls back to its Mauve accent -- the same fallback the guide
//! permits when a colour cannot carry text.

use super::Palette;

/// Catppuccin Latte: the light flavour, on Base `#eff1f5`.
pub const LATTE: Palette = Palette {
    bg: 0xeff1f5,
    fg: 0x4c4f69,
    cursor: 0xdc8a78,
    // Text `#4c4f69` over Base at 10%, the opacity the guide gives a cursor line.
    cursor_line_bg: 0xdfe1e7,
    // Overlay 2 `#7c7f93` over Base at 25%.
    selection: 0xd2d4dc,
    link: 0x1e66f5,
    link_unresolved: 0x9ca0b0,
    dim: 0x8c8fa1,
    heading: 0x4c4f69,
    quote: 0x6c6f85,
    accent: 0x8839ef,
    tag: 0x1e66f5,
    // Red: the guide's colour for errors, which is what a checker's squiggle is.
    spell: 0xd20f39,
    code_bg: 0xbcc0cc,
    code_block_bg: 0xe6e9ef,
    // The guide's editor mapping, colour for colour. Latte's accents are pale
    // against its own code-block pane -- Yellow types measure 2.15:1 there --
    // but that is what the flavour's own syntax theme paints, and a token only
    // has to be distinguishable from the code around it, so it is kept.
    code_text: 0x4c4f69,
    code_comment: 0x7c7f93,
    code_keyword: 0x8839ef,
    code_string: 0x40a02b,
    code_number: 0xfe640b,
    code_function: 0x1e66f5,
    code_type: 0xdf8e1d,
    code_const: 0xd20f39,
    code_operator: 0x04a5e5,
    // Yellow `#df8e1d` over Base at 25%.
    highlight_bg: 0xebd8bf,
    // Latte's Peach is too faint as text on a highlighted row, so the light
    // flavour matches with its Mauve accent instead.
    match_text: 0x8839ef,
    table_line: 0xacb0be,
    table_header_bg: 0xccd0da,
    callout_bg: 0xccd0da,
    ui_bg: 0xe6e9ef,
    ui_status_bg: 0xdce0e8,
    ui_border: 0x9ca0b0,
    ui_hover_bg: 0xccd0da,
    ui_active_bg: 0xbcc0cc,
    ui_fg: 0x4c4f69,
    ui_dim: 0x6c6f85,
    ui_field_bg: 0xccd0da,
    ui_active_border: 0x7287fd,
    ui_tab_bg: 0xccd0da,
    ui_tab_active_bg: 0xbcc0cc,
    popup_bg: 0xccd0da,
    popup_border: 0x9ca0b0,
    // Surface 0 `#ccd0da` with Text `#4c4f69` at 12%: the panel's own
    // background raised, rather than a surface borrowed from the chrome.
    popup_hover_bg: 0xbdc1cc,
    graph_node: 0x1e66f5,
    graph_node_active: 0x04a5e5,
    graph_edge: 0x9ca0b0,
    graph_text: 0x6c6f85,
    callout_note: 0x1e66f5,
    callout_teal: 0x179299,
    callout_question: 0x8839ef,
    callout_warning: 0xdf8e1d,
    callout_failure: 0xfe640b,
    callout_danger: 0xd20f39,
};

/// Catppuccin Frappé: the lightest dark flavour, on Base `#303446`.
pub const FRAPPE: Palette = Palette {
    bg: 0x303446,
    fg: 0xc6d0f5,
    cursor: 0xf2d5cf,
    // Text `#c6d0f5` over Base at 10%.
    cursor_line_bg: 0x3f4458,
    // Overlay 2 `#949cbb` over Base at 25%.
    selection: 0x494e63,
    link: 0x8caaee,
    link_unresolved: 0x737994,
    dim: 0x838ba7,
    heading: 0xc6d0f5,
    quote: 0xa5adce,
    accent: 0xca9ee6,
    tag: 0x8caaee,
    spell: 0xe78284,
    code_bg: 0x51576d,
    code_block_bg: 0x292c3c,
    // The guide's editor mapping: Mauve keywords, Green strings, Peach
    // constants and numbers, Blue functions, Yellow types, Red builtins, Sky
    // operators, Overlay 2 comments.
    code_text: 0xc6d0f5,
    code_comment: 0x949cbb,
    code_keyword: 0xca9ee6,
    code_string: 0xa6d189,
    code_number: 0xef9f76,
    code_function: 0x8caaee,
    code_type: 0xe5c890,
    code_const: 0xe78284,
    code_operator: 0x99d1db,
    // Yellow `#e5c890` over Base at 25%.
    highlight_bg: 0x5d5958,
    // Peach: the flavour's accent for a hit.
    match_text: 0xef9f76,
    table_line: 0x626880,
    table_header_bg: 0x414559,
    callout_bg: 0x414559,
    ui_bg: 0x292c3c,
    ui_status_bg: 0x232634,
    ui_border: 0x737994,
    ui_hover_bg: 0x414559,
    ui_active_bg: 0x51576d,
    ui_fg: 0xc6d0f5,
    ui_dim: 0xa5adce,
    ui_field_bg: 0x414559,
    ui_active_border: 0xbabbf1,
    ui_tab_bg: 0x414559,
    ui_tab_active_bg: 0x51576d,
    popup_bg: 0x414559,
    popup_border: 0x737994,
    // Surface 0 `#414559` with Text `#c6d0f5` at 12%.
    popup_hover_bg: 0x51566c,
    graph_node: 0x8caaee,
    graph_node_active: 0x99d1db,
    graph_edge: 0x737994,
    graph_text: 0xa5adce,
    callout_note: 0x8caaee,
    callout_teal: 0x81c8be,
    callout_question: 0xca9ee6,
    callout_warning: 0xe5c890,
    callout_failure: 0xef9f76,
    callout_danger: 0xe78284,
};

/// Catppuccin Macchiato: the middle dark flavour, on Base `#24273a`.
pub const MACCHIATO: Palette = Palette {
    bg: 0x24273a,
    fg: 0xcad3f5,
    cursor: 0xf4dbd6,
    // Text `#cad3f5` over Base at 10%.
    cursor_line_bg: 0x35384d,
    // Overlay 2 `#939ab7` over Base at 25%.
    selection: 0x404459,
    link: 0x8aadf4,
    link_unresolved: 0x6e738d,
    dim: 0x8087a2,
    heading: 0xcad3f5,
    quote: 0xa5adcb,
    accent: 0xc6a0f6,
    tag: 0x8aadf4,
    spell: 0xed8796,
    code_bg: 0x494d64,
    code_block_bg: 0x1e2030,
    // The guide's editor mapping (see `LATTE`).
    code_text: 0xcad3f5,
    code_comment: 0x939ab7,
    code_keyword: 0xc6a0f6,
    code_string: 0xa6da95,
    code_number: 0xf5a97f,
    code_function: 0x8aadf4,
    code_type: 0xeed49f,
    code_const: 0xed8796,
    code_operator: 0x91d7e3,
    // Yellow `#eed49f` over Base at 25%.
    highlight_bg: 0x565253,
    // Peach: the flavour's accent for a hit.
    match_text: 0xf5a97f,
    table_line: 0x5b6078,
    table_header_bg: 0x363a4f,
    callout_bg: 0x363a4f,
    ui_bg: 0x1e2030,
    ui_status_bg: 0x181926,
    ui_border: 0x6e738d,
    ui_hover_bg: 0x363a4f,
    ui_active_bg: 0x494d64,
    ui_fg: 0xcad3f5,
    ui_dim: 0xa5adcb,
    ui_field_bg: 0x363a4f,
    ui_active_border: 0xb7bdf8,
    ui_tab_bg: 0x363a4f,
    ui_tab_active_bg: 0x494d64,
    popup_bg: 0x363a4f,
    popup_border: 0x6e738d,
    // Surface 0 `#363a4f` with Text `#cad3f5` at 12%.
    popup_hover_bg: 0x484c63,
    graph_node: 0x8aadf4,
    graph_node_active: 0x91d7e3,
    graph_edge: 0x6e738d,
    graph_text: 0xa5adcb,
    callout_note: 0x8aadf4,
    callout_teal: 0x8bd5ca,
    callout_question: 0xc6a0f6,
    callout_warning: 0xeed49f,
    callout_failure: 0xf5a97f,
    callout_danger: 0xed8796,
};

/// Catppuccin Mocha: the darkest flavour and the palette most Catppuccin users
/// reach for, on Base `#1e1e2e`.
pub const MOCHA: Palette = Palette {
    bg: 0x1e1e2e,
    fg: 0xcdd6f4,
    cursor: 0xf5e0dc,
    // Text `#cdd6f4` over Base at 10%.
    cursor_line_bg: 0x303042,
    // Overlay 2 `#9399b2` over Base at 25%.
    selection: 0x3b3d4f,
    link: 0x89b4fa,
    link_unresolved: 0x6c7086,
    dim: 0x7f849c,
    heading: 0xcdd6f4,
    quote: 0xa6adc8,
    accent: 0xcba6f7,
    tag: 0x89b4fa,
    spell: 0xf38ba8,
    code_bg: 0x45475a,
    code_block_bg: 0x181825,
    // The guide's editor mapping (see `LATTE`).
    code_text: 0xcdd6f4,
    code_comment: 0x9399b2,
    code_keyword: 0xcba6f7,
    code_string: 0xa6e3a1,
    code_number: 0xfab387,
    code_function: 0x89b4fa,
    code_type: 0xf9e2af,
    code_const: 0xf38ba8,
    code_operator: 0x89dceb,
    // Yellow `#f9e2af` over Base at 25%.
    highlight_bg: 0x554f4e,
    // Peach: the flavour's accent for a hit.
    match_text: 0xfab387,
    table_line: 0x585b70,
    table_header_bg: 0x313244,
    callout_bg: 0x313244,
    ui_bg: 0x181825,
    ui_status_bg: 0x11111b,
    ui_border: 0x6c7086,
    ui_hover_bg: 0x313244,
    ui_active_bg: 0x45475a,
    ui_fg: 0xcdd6f4,
    ui_dim: 0xa6adc8,
    ui_field_bg: 0x313244,
    ui_active_border: 0xb4befe,
    ui_tab_bg: 0x313244,
    ui_tab_active_bg: 0x45475a,
    popup_bg: 0x313244,
    popup_border: 0x6c7086,
    // Surface 0 `#313244` with Text `#cdd6f4` at 12%.
    popup_hover_bg: 0x444659,
    graph_node: 0x89b4fa,
    graph_node_active: 0x89dceb,
    graph_edge: 0x6c7086,
    graph_text: 0xa6adc8,
    callout_note: 0x89b4fa,
    callout_teal: 0x94e2d5,
    callout_question: 0xcba6f7,
    callout_warning: 0xf9e2af,
    callout_failure: 0xfab387,
    callout_danger: 0xf38ba8,
};

/// The four flavours by (lowercase, ASCII) name, lightest first. The names are
/// the flavours' own names; [`super::Palette::built_in`] also accepts them with
/// a `catppuccin-` prefix.
pub const FLAVORS: &[(&str, Palette)] = &[
    ("latte", LATTE),
    ("frappe", FRAPPE),
    ("macchiato", MACCHIATO),
    ("mocha", MOCHA),
];
