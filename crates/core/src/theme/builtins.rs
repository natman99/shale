//! The palettes Shale ships with.
//!
//! Data, not behaviour: every one of them is a [`Palette`] literal, and the list
//! is what the theme picker offers and `appearance.theme` names. The families
//! live in their own files; this is where they are collected, along with Shale's
//! own dark and light.

use super::catppuccin;
use super::dracula;
use super::gruvbox;
use super::nord;
use super::one;
use super::rose_pine;
use super::solarized;
use super::tokyo_night;
use super::Palette;

impl Palette {
    /// The default palette: a dark note on a dark window.
    pub const DARK: Palette = Palette {
        bg: 0x1e1e1e,
        fg: 0xd4d4d4,
        cursor: 0xaeafad,
        // One step up from the note's own background: a band, not a highlight.
        cursor_line_bg: 0x242424,
        selection: 0x264f78,
        link: 0x7aa6f0,
        link_unresolved: 0x7d6f6f,
        dim: 0x6b7280,
        heading: 0xe8e8e8,
        quote: 0x9aa0a6,
        accent: 0x7aa6f0,
        tag: 0x8fbf7f,
        // The palette's error red: the checker's squiggle is the only thing
        // that has to be red in a note, and only because it means "wrong".
        spell: 0xe05e5e,
        code_bg: 0x2f2f2f,
        code_block_bg: 0x262626,
        // Shale's own dark palette has no syntax theme, so the token colours are
        // its existing accents pressed into the jobs tree-sitter will need:
        // the note's text for ordinary code, the comment grey for comments, and
        // the callout accents for the parts of a program a reader scans for.
        code_text: 0xd4d4d4,
        code_comment: 0x6b7280,
        code_keyword: 0xc586c0,
        code_string: 0x8fbf7f,
        code_number: 0xe0a55e,
        code_function: 0x7aa6f0,
        code_type: 0x6fc7c2,
        code_const: 0xe07a5e,
        code_operator: 0x9aa0a6,
        highlight_bg: 0x4a3f1f,
        // The same amber `==highlight==` is drawn on, but as text: it reads on
        // both of the picker's row backgrounds, where the note's own colours do
        // not stand out enough to look like a hit.
        match_text: 0xe0a55e,
        table_line: 0x3f3f46,
        table_header_bg: 0x2a2a2e,
        callout_bg: 0x252a33,
        ui_bg: 0x1e1e1e,
        ui_fg: 0xd4d4d4,
        ui_dim: 0x9aa0a6,
        ui_status_bg: 0x252526,
        ui_border: 0x3c3c3c,
        ui_active_border: 0x7aa6f0,
        ui_hover_bg: 0x2a2a2a,
        ui_active_bg: 0x2f2f2f,
        ui_field_bg: 0x2f2f2f,
        ui_tab_bg: 0x2d2d30,
        ui_tab_active_bg: 0x404046,
        // A panel floats over the note, so it is raised above both the note and
        // the chrome, and its own rule is one step above that.
        popup_bg: 0x303036,
        popup_border: 0x45454b,
        popup_hover_bg: 0x3a3a40,
        graph_node: 0x7aa6f0,
        graph_node_active: 0xe0a55e,
        graph_edge: 0x3f3f46,
        graph_text: 0x9aa0a6,
        callout_note: 0x8ca0b3,
        callout_teal: 0x6fc7c2,
        callout_question: 0xc586c0,
        callout_warning: 0xe0a55e,
        callout_failure: 0xe07a5e,
        callout_danger: 0xe05e5e,
    };

    /// A light palette for the same roles: dark text on a warm white, with the
    /// syntax and structural colours darkened enough to keep their contrast.
    pub const LIGHT: Palette = Palette {
        bg: 0xfafafa,
        fg: 0x222228,
        cursor: 0x535353,
        // One step down from the note's own background.
        cursor_line_bg: 0xf2f2f4,
        selection: 0xbfd7f0,
        link: 0x17497c,
        link_unresolved: 0x7d6262,
        dim: 0x686d76,
        heading: 0x1f1f23,
        quote: 0x545960,
        accent: 0x17497c,
        tag: 0x245824,
        spell: 0xc03c3c,
        code_bg: 0xe8e8e8,
        code_block_bg: 0xefefef,
        // The same accents as the dark palette, darkened to keep their contrast
        // against a cream-white block (see `DARK`).
        code_text: 0x222228,
        code_comment: 0x686d76,
        code_keyword: 0x9b4fa0,
        code_string: 0x245824,
        code_number: 0xb07a22,
        code_function: 0x17497c,
        code_type: 0x2f8f89,
        code_const: 0xb5573a,
        code_operator: 0x545960,
        highlight_bg: 0xf5e0a3,
        // Dark amber, as `dark` is light amber: a query's characters have to
        // clear the highlighted row, which a colour this pale would not.
        match_text: 0x7a4d00,
        table_line: 0xc9c9cf,
        table_header_bg: 0xe9e9ee,
        callout_bg: 0xeef1f6,
        ui_bg: 0xf0f0f2,
        ui_fg: 0x222228,
        ui_dim: 0x686d76,
        ui_status_bg: 0xe6e6e9,
        ui_border: 0xcfcfd4,
        ui_active_border: 0x17497c,
        ui_hover_bg: 0xe4e4e8,
        ui_active_bg: 0xd8d8de,
        // A field is a white box cut into the chrome, as it is in every light
        // editor; the panel is a raised sheet, sunk into the note instead.
        ui_field_bg: 0xffffff,
        ui_tab_bg: 0xd8dbe2,
        ui_tab_active_bg: 0xbdc2cd,
        popup_bg: 0xdfe3ec,
        popup_border: 0xb9bcc4,
        popup_hover_bg: 0xd0d5e0,
        graph_node: 0x17497c,
        graph_node_active: 0x7a4d00,
        graph_edge: 0xc9c9cf,
        graph_text: 0x545960,
        callout_note: 0x5b7590,
        callout_teal: 0x2f8f89,
        callout_question: 0x9b4fa0,
        callout_warning: 0xb07a22,
        callout_failure: 0xb5573a,
        callout_danger: 0xc03c3c,
    };

    /// The palette a vault gets when it names none: Catppuccin Mocha, the
    /// darkest flavour and the one most people mean when they say Catppuccin.
    pub const DEFAULT: Palette = catppuccin::MOCHA;

    /// [`Self::DEFAULT`] under the name `appearance.theme` asks for, so the
    /// config default, the loader and the app's fallback cannot drift apart.
    pub const DEFAULT_NAME: &'static str = "mocha";

    /// The built-in palettes, by (lowercase) name: Shale's own two, then the
    /// palettes of the families this module keeps a file for. A family's variants
    /// are listed in the order upstream lists them, which for the families with a
    /// light variant puts the dark one first.
    pub const BUILT_INS: &[(&str, Palette)] = &[
        ("dark", Self::DARK),
        ("light", Self::LIGHT),
        ("latte", catppuccin::LATTE),
        ("frappe", catppuccin::FRAPPE),
        ("macchiato", catppuccin::MACCHIATO),
        ("mocha", catppuccin::MOCHA),
        ("tokyo-night", tokyo_night::NIGHT),
        ("tokyo-night-storm", tokyo_night::STORM),
        ("tokyo-night-light", tokyo_night::LIGHT),
        ("gruvbox-dark", gruvbox::DARK),
        ("gruvbox-light", gruvbox::LIGHT),
        ("nord", nord::PALETTE),
        ("dracula", dracula::PALETTE),
        ("solarized-dark", solarized::DARK),
        ("solarized-light", solarized::LIGHT),
        ("one-dark", one::DARK),
        ("one-light", one::LIGHT),
        ("rose-pine", rose_pine::MAIN),
        ("rose-pine-moon", rose_pine::MOON),
        ("rose-pine-dawn", rose_pine::DAWN),
    ];
}
