//! The four Catppuccin flavours, as [`Palette`] values.
//!
//! Catppuccin (https://catppuccin.com) is a pastel palette in four flavours that
//! differ only in how light their backgrounds are: Latte (light), Frappé,
//! Macchiato and Mocha (dark). Every colour here is taken verbatim from
//! `palette.json` v1.8.0 of https://github.com/catppuccin/palette (MIT-licensed,
//! so the hexes may be copied into this Apache-2.0 crate), so a Shale note
//! matches the rest of a user's Catppuccin setup.
//!
//! The palette's 26 named colours do not line up with Shale's 28 roles, so each
//! role is filled from the flavour by the mapping in Catppuccin's own style
//! guide (https://github.com/catppuccin/catppuccin/blob/main/docs/style-guide.md):
//!
//! | `Palette` role      | Catppuccin colour                     |
//! |---------------------|---------------------------------------|
//! | `bg`                | Base (the "background pane")          |
//! | `fg`, `heading`     | Text (body copy and main headlines)   |
//! | `cursor`            | Rosewater                           |
//! | `selection`         | Overlay 2 over Base, 25%              |
//! | `link`, `tag`       | Blue (links/URLs, tags and pills)     |
//! | `link_unresolved`   | Overlay 0 (subtle, not an error red)  |
//! | `dim`               | Overlay 1 ("subtle", line numbers)    |
//! | `quote`             | Subtext 0 (sub-headlines and labels)  |
//! | `accent`            | Mauve                               |
//! | `code_bg`           | Surface 1                           |
//! | `code_block_bg`     | Mantle (a secondary pane)           |
//! | `highlight_bg`      | Yellow over Base, 25%               |
//! | `table_line`        | Surface 2                           |
//! | `table_header_bg`   | Surface 0                           |
//! | `callout_bg`        | Surface 0 (a surface element)       |
//! | `ui_bg`             | Mantle (a secondary pane)           |
//! | `ui_status_bg`      | Crust (the darkest secondary pane)  |
//! | `ui_border`         | Overlay 0 (an inactive border)      |
//! | `ui_hover_bg`       | Surface 0                           |
//! | `ui_active_bg`      | Surface 1                           |
//! | `callout_note`      | Blue                                |
//! | `callout_teal`      | Teal                                |
//! | `callout_question`  | Mauve                               |
//! | `callout_warning`   | Yellow ("warnings")                 |
//! | `callout_failure`   | Peach                               |
//! | `callout_danger`    | Red ("errors")                      |
//!
//! The guide states `selection` and `highlight_bg` as an accent drawn *over*
//! Base at an opacity, and a `Palette` has no alpha channel, so both are
//! composited onto Base once here (at the middle of the guide's 20%-30% range
//! for selections) and stored as the flat colour the reader ends up seeing.
//!
//! `heading` is Text rather than an accent because that is what the guide says
//! for a main headline; a Shale heading is already larger and bold, so it does
//! not need a hue to separate it from body text.

use super::Palette;

/// Catppuccin Latte: the light flavour, on Base `#eff1f5`.
pub const LATTE: Palette = Palette {
    bg: 0xeff1f5,
    fg: 0x4c4f69,
    cursor: 0xdc8a78,
    // Overlay 2 `#7c7f93` over Base at 25%.
    selection: 0xd2d4dc,
    link: 0x1e66f5,
    link_unresolved: 0x9ca0b0,
    dim: 0x8c8fa1,
    heading: 0x4c4f69,
    quote: 0x6c6f85,
    accent: 0x8839ef,
    tag: 0x1e66f5,
    code_bg: 0xbcc0cc,
    code_block_bg: 0xe6e9ef,
    // Yellow `#df8e1d` over Base at 25%.
    highlight_bg: 0xebd8bf,
    table_line: 0xacb0be,
    table_header_bg: 0xccd0da,
    callout_bg: 0xccd0da,
    ui_bg: 0xe6e9ef,
    ui_status_bg: 0xdce0e8,
    ui_border: 0x9ca0b0,
    ui_hover_bg: 0xccd0da,
    ui_active_bg: 0xbcc0cc,
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
    // Overlay 2 `#949cbb` over Base at 25%.
    selection: 0x494e63,
    link: 0x8caaee,
    link_unresolved: 0x737994,
    dim: 0x838ba7,
    heading: 0xc6d0f5,
    quote: 0xa5adce,
    accent: 0xca9ee6,
    tag: 0x8caaee,
    code_bg: 0x51576d,
    code_block_bg: 0x292c3c,
    // Yellow `#e5c890` over Base at 25%.
    highlight_bg: 0x5d5958,
    table_line: 0x626880,
    table_header_bg: 0x414559,
    callout_bg: 0x414559,
    ui_bg: 0x292c3c,
    ui_status_bg: 0x232634,
    ui_border: 0x737994,
    ui_hover_bg: 0x414559,
    ui_active_bg: 0x51576d,
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
    // Overlay 2 `#939ab7` over Base at 25%.
    selection: 0x404459,
    link: 0x8aadf4,
    link_unresolved: 0x6e738d,
    dim: 0x8087a2,
    heading: 0xcad3f5,
    quote: 0xa5adcb,
    accent: 0xc6a0f6,
    tag: 0x8aadf4,
    code_bg: 0x494d64,
    code_block_bg: 0x1e2030,
    // Yellow `#eed49f` over Base at 25%.
    highlight_bg: 0x565253,
    table_line: 0x5b6078,
    table_header_bg: 0x363a4f,
    callout_bg: 0x363a4f,
    ui_bg: 0x1e2030,
    ui_status_bg: 0x181926,
    ui_border: 0x6e738d,
    ui_hover_bg: 0x363a4f,
    ui_active_bg: 0x494d64,
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
    // Overlay 2 `#9399b2` over Base at 25%.
    selection: 0x3b3d4f,
    link: 0x89b4fa,
    link_unresolved: 0x6c7086,
    dim: 0x7f849c,
    heading: 0xcdd6f4,
    quote: 0xa6adc8,
    accent: 0xcba6f7,
    tag: 0x89b4fa,
    code_bg: 0x45475a,
    code_block_bg: 0x181825,
    // Yellow `#f9e2af` over Base at 25%.
    highlight_bg: 0x554f4e,
    table_line: 0x585b70,
    table_header_bg: 0x313244,
    callout_bg: 0x313244,
    ui_bg: 0x181825,
    ui_status_bg: 0x11111b,
    ui_border: 0x6c7086,
    ui_hover_bg: 0x313244,
    ui_active_bg: 0x45475a,
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
