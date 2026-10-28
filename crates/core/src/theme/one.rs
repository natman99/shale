//! Atom's One Dark and One Light.
//! One (https://github.com/atom/one-dark-syntax, one-light-syntax and the
//! matching *-ui packages, MIT) is the palette Atom shipped for a decade and
//! much of the editor world still defaults to. Unusually, it is defined in
//! HSL: a hue, saturation and brightness, a three-step monochrome ramp, eight
//! hues, and shades LESS derives from them -- the gutter, the guide, the
//! selection, the app background levels, and an accent it also uses as the
//! caret. This module evaluates those definitions, so the hexes here are the
//! upstream ones and not a port's approximation.

use super::Palette;

/// One Dark: Atom's palette, on `hsl(220, 13%, 18%)` = `#282c34`.
///
/// Atom defines One in HSL rather than in hex, so every value here is that
/// definition evaluated: the mono-1..3 text ramp, the eight hues, and the
/// shades LESS derives from them -- `@syntax-gutter`, the
/// `lighten(@syntax-bg, 10%)` selection, the `@level-3-color` app
/// background, `@text-color-highlight` headings, and the accent that
/// `@syntax-cursor-color` names as the caret. The callout accents are the
/// theme's own git colours: renamed, added, modified and removed.
pub const DARK: Palette = Palette {
    bg: 0x282c34,               // hsl(220, 13%, 18%)
    fg: 0xabb2bf,               // mono-1: hsl(220, 14%, 71%)
    cursor: 0x528bff,           // @syntax-accent, the theme's caret colour
    cursor_line_bg: 0x353b45,   // the note background one step off: a band
    selection: 0x3e4451,        // the selection shade the theme derives
    link: 0x61afef,             // blue
    link_unresolved: 0x636d83,  // @syntax-gutter
    dim: 0x5c6370,              // mono-3, the comment colour
    heading: 0xe5e7eb,          // @text-color-highlight
    quote: 0x828997,            // mono-2
    accent: 0x528bff,           // the theme's accent, which it also uses as the caret
    tag: 0x98c379,              // green
    spell: 0xe06c75,            // red
    code_bg: 0x3a3f4b,          // one step off the note background
    code_block_bg: 0x21252b,    // the app-level pane shade
    code_text: 0xabb2bf,        // mono-1
    code_comment: 0x5c6370,     // mono-3, the comment colour
    code_keyword: 0xc678dd,     // purple
    code_string: 0x98c379,      // green
    code_number: 0xd19a66,      // orange
    code_function: 0x61afef,    // blue
    code_type: 0xe0c285,        // the theme's modified colour
    code_const: 0xe5c07b,       // orange's lighter twin
    code_operator: 0x56b6c2,    // cyan
    highlight_bg: 0x575146,     // #e5c07b at 25% over the background
    match_text: 0xe0c285,       // the theme's modified colour
    table_line: 0x3c4049,       // the 15% foreground fade the theme uses for guides
    table_header_bg: 0x31363f,  // the raised panel shade
    callout_bg: 0x31363f,       // the raised panel shade
    ui_bg: 0x21252b,            // the app background
    ui_fg: 0xabb2bf,            // mono-1
    ui_dim: 0x828997,           // mono-2
    ui_status_bg: 0x21252b,     // the app background
    ui_border: 0x3c4049,        // a rule is what the theme draws as a guide
    ui_active_border: 0x528bff, // the theme's accent
    ui_hover_bg: 0x2f333d,
    ui_active_bg: 0x282c34,
    ui_field_bg: 0x3a3f4b, // one step off the note background
    ui_tab_bg: 0x31363f,
    ui_tab_active_bg: 0x3e4451,
    popup_bg: 0x31363f,
    popup_border: 0x5c616c,      // #abb2bf at 35% over the panel
    popup_hover_bg: 0x40454e,    // the panel with mono-1 at 12%
    graph_node: 0xc678dd,        // purple
    graph_node_active: 0xe0c285, // the theme's modified colour
    graph_edge: 0x3c4049,        // a rule is what the theme draws as a guide
    graph_text: 0xabb2bf,        // mono-1
    callout_note: 0x33a0ff,      // the theme's renamed colour
    callout_teal: 0x43d08a,      // the theme's added colour
    callout_question: 0xc678dd,  // purple
    callout_warning: 0xe0c285,   // the theme's modified colour
    callout_failure: 0xd19a66,   // orange
    callout_danger: 0xe06c75,    // red
};

/// One Light: the same definitions inverted, on `hsl(230, 1%, 98%)`.
///
/// LESS cannot lighten a 98%-light background, so where the dark UI lifts a
/// surface the light one lowers it; every other value is the formula.
pub const LIGHT: Palette = Palette {
    bg: 0xfafafa,               // hsl(230, 1%, 98%)
    fg: 0x383a42,               // mono-1: hsl(230, 8%, 24%)
    cursor: 0x526eff,           // @syntax-accent, the theme's caret colour
    cursor_line_bg: 0xededed,   // the note background one step off: a band
    selection: 0xe5e5e6,        // the selection shade the theme derives
    link: 0x4078f2,             // blue
    link_unresolved: 0x9d9d9f,  // @syntax-gutter
    dim: 0xa0a1a7,              // mono-3, the comment colour
    heading: 0x1c1d21,          // @text-color-highlight
    quote: 0x696c77,            // mono-2
    accent: 0x526eff,           // the theme's accent, which it also uses as the caret
    tag: 0x50a14f,              // green
    spell: 0xe45649,            // red
    code_bg: 0xe5e5e6,          // one step off the note background
    code_block_bg: 0xeaeaeb,    // the app-level pane shade
    code_text: 0x383a42,        // mono-1
    code_comment: 0xa0a1a7,     // mono-3, the comment colour
    code_keyword: 0xa626a4,     // purple
    code_string: 0x50a14f,      // green
    code_number: 0x986801,      // orange
    code_function: 0x4078f2,    // blue
    code_type: 0x986801,        // orange
    code_const: 0xc18401,       // orange's lighter twin
    code_operator: 0x0184bc,    // cyan
    highlight_bg: 0xecdcbc,     // #c18401 at 25% over the background
    match_text: 0x986801,       // orange
    table_line: 0xd3d4d5,       // the 20% foreground fade the theme uses for guides
    table_header_bg: 0xf0f0f0,  // the raised panel shade
    callout_bg: 0xf0f0f0,       // the raised panel shade
    ui_bg: 0xeaeaeb,            // the app background
    ui_fg: 0x383a42,            // mono-1
    ui_dim: 0x696c77,           // mono-2
    ui_status_bg: 0xeaeaeb,     // the app background
    ui_border: 0xd3d4d5,        // a rule is what the theme draws as a guide
    ui_active_border: 0x526eff, // the theme's accent
    ui_hover_bg: 0xdbdbdc,
    ui_active_bg: 0xe3e3e3,
    ui_field_bg: 0xe5e5e6, // one step off the note background
    ui_tab_bg: 0xdbdbdc,
    ui_tab_active_bg: 0xc6c7c7,
    popup_bg: 0xdbdbdc,
    popup_border: 0xa2a3a6,      // #383a42 at 35% over the panel
    popup_hover_bg: 0xc7c8ca,    // the panel with mono-1 at 12%
    graph_node: 0xa626a4,        // purple
    graph_node_active: 0x986801, // orange
    graph_edge: 0xd3d4d5,        // a rule is what the theme draws as a guide
    graph_text: 0x383a42,        // mono-1
    callout_note: 0x52aeff,      // the theme's renamed colour
    callout_teal: 0x2db448,      // the theme's added colour
    callout_question: 0xa626a4,  // purple
    callout_warning: 0xf2a60d,   // the theme's modified colour
    callout_failure: 0x986801,   // orange
    callout_danger: 0xe45649,    // red
};
