//! The editor's colour palette and the theme files that change it.
//!
//! Every colour the editor draws lives here rather than at each paint site, so
//! theming is a data problem: a [`Palette`] is a bag of 0xRRGGBB integers, each
//! built-in theme is one value of it, and a theme file is a partial override of
//! one. Painting (which needs gpui types) stays in the app and just reads the
//! palette it is given.
//!
//! A theme is named in `shale.toml` as `appearance.theme`. The built-in names
//! are `dark` and `light` plus the palettes of seven popular theme families --
//! the four Catppuccin flavours (`latte`, `frappe`, `macchiato`, `mocha`, each
//! also spelled with a `catppuccin-` prefix), three Tokyo Night variants, two
//! Gruvbox modes, Nord, Dracula, both Solarized modes, Atom's One Dark and One
//! Light, and the three Rosé Pine variants. The default is `mocha`; see
//! [`Palette::BUILT_INS`] for the whole list. Any other name is looked up as
//! `<vault>/.shale/themes/<name>.toml`, which may start from any built-in
//! (`base = "light"`) and override the keys it cares about:
//!
//! ```toml
//! base = "dark"
//! bg = "#282828"
//! fg = "#ebdbb2"
//! link = "#83a598"
//! ```

use std::path::{Path, PathBuf};

use anyhow::{bail, Context as _};
use serde::{Deserialize, Deserializer, Serialize};

use crate::mdstyle::CalloutKind;

/// The Catppuccin flavours, as palettes built from the upstream palette file.
pub mod catppuccin;
/// Atom's One Dark and One Light, evaluated from their upstream HSL definitions.
pub mod dracula;
pub mod gruvbox;
pub mod nord;
pub mod one;
pub mod rose_pine;
pub mod solarized;
pub mod tokyo_night;

/// The directory under the vault root where theme files live.
pub const THEME_DIR: &str = ".shale/themes";

/// Every colour the editor draws, or will draw, as 0xRRGGBB.
///
/// The names describe roles, not hues, because a theme will reuse the same role
/// for very different colours: `dim` is syntax punctuation in both the dark and
/// the light palette even though one is grey and the other is a dark warm grey.
///
/// A theme file only has to name the roles it cares about, so a longer list is
/// cheap for a theme author. Some roles are not painted yet -- the `code_*`
/// token colours wait on tree-sitter, the `graph_*` colours on the graph view,
/// `ui_tab_*` on tabs and `cursor_line_bg` on a current-line highlight -- and
/// are here so that adding those features does not mean re-cutting twenty
/// palettes one role at a time. [`crate::theme::tests`] still judges every role
/// in every built-in, so a reserved colour is a usable one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    /// Note background.
    pub bg: u32,
    /// Ordinary note text.
    pub fg: u32,
    /// The normal-mode cursor block.
    pub cursor: u32,
    /// Background band across the line the cursor sits on: vim's `cursorline`.
    pub cursor_line_bg: u32,
    /// Visual-mode selection.
    pub selection: u32,
    /// A wiki-link that names an existing note.
    pub link: u32,
    /// A wiki-link that does not (yet) name an existing note.
    pub link_unresolved: u32,
    /// Syntax punctuation and markers: `#`, `*`, `|`, brackets, table pipes.
    pub dim: u32,
    /// Heading text.
    pub heading: u32,
    /// Blockquote text.
    pub quote: u32,
    /// List bullets, task boxes and other structural accents.
    pub accent: u32,
    /// A `#tag`.
    pub tag: u32,
    /// The wavy underline the spell and grammar checker marks a word with.
    ///
    /// Kept apart from [`Self::callout_danger`] because a checker's squiggle is
    /// a note about the text rather than an error in it: a theme may want it in
    /// a quieter colour, or in the colour of its warning rather than its error.
    pub spell: u32,
    /// Background of an inline code span.
    pub code_bg: u32,
    /// Background of a fenced code block's lines.
    pub code_block_bg: u32,
    /// Ordinary text inside a fenced code block.
    pub code_text: u32,
    /// A comment inside a fenced code block.
    pub code_comment: u32,
    /// A control or declaration keyword inside a fenced code block.
    pub code_keyword: u32,
    /// A string literal inside a fenced code block.
    pub code_string: u32,
    /// A number literal inside a fenced code block.
    pub code_number: u32,
    /// A function name or call inside a fenced code block.
    pub code_function: u32,
    /// A type, class or interface name inside a fenced code block.
    pub code_type: u32,
    /// A constant, builtin or symbol inside a fenced code block.
    pub code_const: u32,
    /// An operator or delimiter inside a fenced code block.
    pub code_operator: u32,
    /// Background of `==highlighted==` text.
    pub highlight_bg: u32,
    /// The characters a picker's query matched, drawn as text in this colour
    /// rather than as a box behind them: quick-open, the command palette and
    /// vault search all read it off the palette. It has to stay legible on
    /// `ui_bg` (an ordinary picker row) and on `ui_active_bg` (the highlighted
    /// one), which is what keeps it a different colour from `fg`.
    pub match_text: u32,
    /// The grid lines drawn around table cells, in place of the source pipes,
    /// and the rule a thematic break is drawn as.
    pub table_line: u32,
    /// Background of a table's header row.
    pub table_header_bg: u32,
    /// Background of an Obsidian callout's box, which spans the editor like a
    /// fenced block's does.
    pub callout_bg: u32,
    /// The window and sidebar background, which frames the note.
    pub ui_bg: u32,
    /// Background of the status bar at the bottom of the window.
    pub ui_status_bg: u32,
    /// The rule between the sidebar and the editor.
    pub ui_border: u32,
    /// A file-tree row under the pointer.
    pub ui_hover_bg: u32,
    /// The file-tree row that names the open note.
    pub ui_active_bg: u32,
    /// Text drawn on a window surface rather than in the note: the sidebar's
    /// rows, a picker's rows, the status bar and a tab's label. It is separate
    /// from [`Self::fg`] because the note's text colour is chosen against the
    /// note's background, and a theme may want its chrome to read quieter.
    pub ui_fg: u32,
    /// Quiet text on a window surface: a picker's hint line, a field's
    /// placeholder, a panel's source label. The chrome's [`Self::dim`].
    pub ui_dim: u32,
    /// Background of the text field a picker or the `:` prompt types into.
    ///
    /// Not [`Self::code_block_bg`], which it used to borrow: a field is a
    /// control in the window, not a span of code in the note.
    pub ui_field_bg: u32,
    /// The rule around whatever the window currently has the keyboard: the
    /// focused pane of a split, the focused field. [`Self::ui_border`] is the
    /// same rule where it does not.
    pub ui_active_border: u32,
    /// A tab that does not hold the open note.
    pub ui_tab_bg: u32,
    /// The tab that holds the open note.
    pub ui_tab_active_bg: u32,
    /// Background of the note's popup panel: the checker's answer to `K`.
    ///
    /// Separate from [`Self::ui_bg`] so a theme can make a panel read
    /// differently from a picker, and so the panel stays legible over the note
    /// it floats above rather than only over the window's chrome.
    pub popup_bg: u32,
    /// The rule around that panel.
    pub popup_border: u32,
    /// The panel's row under the pointer, or the one the keyboard is on.
    pub popup_hover_bg: u32,
    /// A note's dot in the graph view.
    pub graph_node: u32,
    /// The graph's focused or hovered node.
    pub graph_node_active: u32,
    /// A wiki-link's line between two nodes in the graph.
    pub graph_edge: u32,
    /// A node's label in the graph.
    pub graph_text: u32,
    /// Callout accents: the left bar and title of a box, by callout type.
    pub callout_note: u32,
    /// Abstract and example callouts.
    pub callout_teal: u32,
    /// Question and FAQ callouts.
    pub callout_question: u32,
    /// Warning and caution callouts.
    pub callout_warning: u32,
    /// Failure callouts.
    pub callout_failure: u32,
    /// Danger and bug callouts.
    pub callout_danger: u32,
}

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

    /// A built-in palette by name, or `None` for anything else.
    ///
    /// Names are matched case-insensitively. Two spellings are accepted that are
    /// not in [`Self::BUILT_INS`]: a Catppuccin flavour with the `catppuccin-`
    /// prefix (`catppuccin-mocha`), and a dark variant that adds a `-dark`
    /// suffix to a family whose dark variant carries the bare name
    /// (`tokyo-night-dark`, `rose-pine-dark`).
    pub fn built_in(name: &str) -> Option<Palette> {
        let name = without_flavour_prefix(name);
        Self::lookup(name).or_else(|| match trailing_dark(name) {
            Some(stem) => Self::lookup(stem),
            None => None,
        })
    }

    /// A built-in palette by its exact registry name, ignoring case.
    fn lookup(name: &str) -> Option<Palette> {
        Self::BUILT_INS
            .iter()
            .find(|(n, _)| name.eq_ignore_ascii_case(n))
            .map(|(_, p)| *p)
    }

    /// This palette with every field named in `overrides` replaced.
    pub fn apply(&self, overrides: &ThemeFile) -> Palette {
        let mut out = *self;
        let ThemeFile {
            base: _,
            bg,
            fg,
            cursor,
            cursor_line_bg,
            selection,
            link,
            link_unresolved,
            dim,
            heading,
            quote,
            accent,
            tag,
            spell,
            code_bg,
            code_block_bg,
            code_text,
            code_comment,
            code_keyword,
            code_string,
            code_number,
            code_function,
            code_type,
            code_const,
            code_operator,
            highlight_bg,
            match_text,
            table_line,
            table_header_bg,
            callout_bg,
            ui_bg,
            ui_fg,
            ui_dim,
            ui_status_bg,
            ui_border,
            ui_active_border,
            ui_hover_bg,
            ui_active_bg,
            ui_field_bg,
            ui_tab_bg,
            ui_tab_active_bg,
            popup_bg,
            popup_border,
            popup_hover_bg,
            graph_node,
            graph_node_active,
            graph_edge,
            graph_text,
            callout_note,
            callout_teal,
            callout_question,
            callout_warning,
            callout_failure,
            callout_danger,
        } = overrides;
        // Each `Option` replaces its field when the theme file named it.
        for (slot, value) in [
            (&mut out.bg, bg),
            (&mut out.fg, fg),
            (&mut out.cursor, cursor),
            (&mut out.cursor_line_bg, cursor_line_bg),
            (&mut out.selection, selection),
            (&mut out.link, link),
            (&mut out.link_unresolved, link_unresolved),
            (&mut out.dim, dim),
            (&mut out.heading, heading),
            (&mut out.quote, quote),
            (&mut out.accent, accent),
            (&mut out.tag, tag),
            (&mut out.spell, spell),
            (&mut out.code_bg, code_bg),
            (&mut out.code_block_bg, code_block_bg),
            (&mut out.code_text, code_text),
            (&mut out.code_comment, code_comment),
            (&mut out.code_keyword, code_keyword),
            (&mut out.code_string, code_string),
            (&mut out.code_number, code_number),
            (&mut out.code_function, code_function),
            (&mut out.code_type, code_type),
            (&mut out.code_const, code_const),
            (&mut out.code_operator, code_operator),
            (&mut out.highlight_bg, highlight_bg),
            (&mut out.match_text, match_text),
            (&mut out.table_line, table_line),
            (&mut out.table_header_bg, table_header_bg),
            (&mut out.callout_bg, callout_bg),
            (&mut out.ui_bg, ui_bg),
            (&mut out.ui_fg, ui_fg),
            (&mut out.ui_dim, ui_dim),
            (&mut out.ui_status_bg, ui_status_bg),
            (&mut out.ui_border, ui_border),
            (&mut out.ui_active_border, ui_active_border),
            (&mut out.ui_hover_bg, ui_hover_bg),
            (&mut out.ui_active_bg, ui_active_bg),
            (&mut out.ui_field_bg, ui_field_bg),
            (&mut out.ui_tab_bg, ui_tab_bg),
            (&mut out.ui_tab_active_bg, ui_tab_active_bg),
            (&mut out.popup_bg, popup_bg),
            (&mut out.popup_border, popup_border),
            (&mut out.popup_hover_bg, popup_hover_bg),
            (&mut out.graph_node, graph_node),
            (&mut out.graph_node_active, graph_node_active),
            (&mut out.graph_edge, graph_edge),
            (&mut out.graph_text, graph_text),
            (&mut out.callout_note, callout_note),
            (&mut out.callout_teal, callout_teal),
            (&mut out.callout_question, callout_question),
            (&mut out.callout_warning, callout_warning),
            (&mut out.callout_failure, callout_failure),
            (&mut out.callout_danger, callout_danger),
        ] {
            if let Some(value) = value {
                *slot = *value;
            }
        }
        out
    }

    /// The accent colour of a callout: its left bar and its title. Obsidian
    /// colours each callout type, and treats an unknown type like the editor's
    /// own accent.
    pub fn callout_accent(&self, kind: CalloutKind) -> u32 {
        match kind {
            CalloutKind::Note => self.callout_note,
            CalloutKind::Info | CalloutKind::Todo => self.link,
            CalloutKind::Abstract | CalloutKind::Example => self.callout_teal,
            CalloutKind::Tip | CalloutKind::Success => self.tag,
            CalloutKind::Question => self.callout_question,
            CalloutKind::Warning => self.callout_warning,
            CalloutKind::Failure => self.callout_failure,
            CalloutKind::Danger | CalloutKind::Bug => self.callout_danger,
            CalloutKind::Quote => self.quote,
            CalloutKind::Other => self.accent,
        }
    }
}

/// A theme file: `<vault>/.shale/themes/<name>.toml`.
///
/// Every colour is optional; the palette it starts from is `base` (default
/// [`Palette::DEFAULT_NAME`], the same palette `appearance.theme` starts from).
/// Unknown keys are rejected so a typo is a message rather than a colour that
/// silently stays default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeFile {
    /// The built-in palette to start from.
    pub base: Option<String>,
    /// Note background.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub bg: Option<u32>,
    /// Ordinary note text.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub fg: Option<u32>,
    /// The normal-mode cursor block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub cursor: Option<u32>,
    /// Background band across the cursor's line.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub cursor_line_bg: Option<u32>,
    /// Visual-mode selection.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub selection: Option<u32>,
    /// A wiki-link that names an existing note.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub link: Option<u32>,
    /// A wiki-link that does not name an existing note.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub link_unresolved: Option<u32>,
    /// Syntax punctuation and markers.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub dim: Option<u32>,
    /// Heading text.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub heading: Option<u32>,
    /// Blockquote text.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub quote: Option<u32>,
    /// List bullets, task boxes and other structural accents.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub accent: Option<u32>,
    /// A `#tag`.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub tag: Option<u32>,
    /// The checker's wavy underline.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub spell: Option<u32>,
    /// Background of an inline code span.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_bg: Option<u32>,
    /// Background of a fenced code block's lines.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_block_bg: Option<u32>,
    /// Ordinary text in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_text: Option<u32>,
    /// A comment in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_comment: Option<u32>,
    /// A keyword in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_keyword: Option<u32>,
    /// A string literal in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_string: Option<u32>,
    /// A number literal in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_number: Option<u32>,
    /// A function name in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_function: Option<u32>,
    /// A type name in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_type: Option<u32>,
    /// A constant or builtin in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_const: Option<u32>,
    /// An operator or delimiter in a fenced code block.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_operator: Option<u32>,
    /// Background of `==highlighted==` text.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub highlight_bg: Option<u32>,
    /// A picker's matched characters, as text colour.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub match_text: Option<u32>,
    /// Table grid lines and the thematic-break rule.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub table_line: Option<u32>,
    /// Background of a table's header row.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub table_header_bg: Option<u32>,
    /// Background of a callout's box.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub callout_bg: Option<u32>,
    /// The window and sidebar background.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_bg: Option<u32>,
    /// Background of the status bar.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_status_bg: Option<u32>,
    /// The rule between the sidebar and the editor.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_border: Option<u32>,
    /// A file-tree row under the pointer.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_hover_bg: Option<u32>,
    /// The file-tree row that names the open note.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_active_bg: Option<u32>,
    /// Text on a window surface rather than in the note.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_fg: Option<u32>,
    /// Quiet text on a window surface.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_dim: Option<u32>,
    /// Background of a picker's or the prompt's text field.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_field_bg: Option<u32>,
    /// The rule around the focused pane or field.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_active_border: Option<u32>,
    /// A tab that does not hold the open note.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_tab_bg: Option<u32>,
    /// The tab that holds the open note.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub ui_tab_active_bg: Option<u32>,
    /// Background of the note's popup panel.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub popup_bg: Option<u32>,
    /// The rule around that panel.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub popup_border: Option<u32>,
    /// That panel's highlighted row.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub popup_hover_bg: Option<u32>,
    /// A note's dot in the graph view.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub graph_node: Option<u32>,
    /// The graph's focused or hovered node.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub graph_node_active: Option<u32>,
    /// A graph edge.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub graph_edge: Option<u32>,
    /// A graph node's label.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub graph_text: Option<u32>,
    /// Note callout accent.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub callout_note: Option<u32>,
    /// Abstract and example callout accent.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub callout_teal: Option<u32>,
    /// Question callout accent.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub callout_question: Option<u32>,
    /// Warning callout accent.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub callout_warning: Option<u32>,
    /// Failure callout accent.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub callout_failure: Option<u32>,
    /// Danger and bug callout accent.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub callout_danger: Option<u32>,
}

impl ThemeFile {
    /// Parse theme TOML. Colours are `#rrggbb` strings.
    pub fn from_toml(text: &str) -> anyhow::Result<ThemeFile> {
        Ok(toml::from_str(text)?)
    }

    /// The file for `name` inside `vault_root`.
    pub fn path(vault_root: &Path, name: &str) -> PathBuf {
        vault_root.join(THEME_DIR).join(format!("{name}.toml"))
    }
}

/// `name` with a case-insensitive `catppuccin-` prefix removed, so that
/// `catppuccin-mocha` and `mocha` name the same built-in palette.
fn without_flavour_prefix(name: &str) -> &str {
    const PREFIX: &str = "catppuccin-";
    if name.len() > PREFIX.len() && name[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        &name[PREFIX.len()..]
    } else {
        name
    }
}

/// The family name behind a `-dark` suffix, for the families whose dark variant
/// is the bare name: `tokyo-night-dark` and `rose-pine-dark` both ask for the
/// variant that is listed without a suffix. `None` if `name` does not end that
/// way, so a family that spells its own variants `-dark`/`-light` (Gruvbox,
/// Solarized, One) is left to its exact names.
fn trailing_dark(name: &str) -> Option<&str> {
    const SUFFIX: &str = "-dark";
    if name.len() > SUFFIX.len() && name[name.len() - SUFFIX.len()..].eq_ignore_ascii_case(SUFFIX) {
        Some(&name[..name.len() - SUFFIX.len()])
    } else {
        None
    }
}

/// `#rrggbb` (the `#` optional) as a 0xRRGGBB integer.
fn parse_color(text: &str) -> anyhow::Result<u32> {
    let hex = text.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("expected a six-digit hex colour like \"#aabbcc\", got {text:?}");
    }
    u32::from_str_radix(hex, 16).map_err(|err| err.into())
}

/// serde helper that reads a colour key as a hex string. The field is an
/// `Option<u32>`, so this returns the option: serde only calls it when the key
/// is present.
fn deserialize_color<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    parse_color(&text)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

/// The palette a vault asks for.
///
/// `name` (from `appearance.theme`) is a built-in -- `dark`, `light`, or one of
/// the family palettes in [`Palette::BUILT_INS`] -- or the stem of
/// `<vault>/.shale/themes/<name>.toml`, whose `base` says which built-in to
/// start from. An empty name is [`Palette::DEFAULT_NAME`]. An unknown name is an
/// error, which lets the caller tell the user their theme was not found instead
/// of quietly drawing the default.
pub fn palette_for(vault_root: &Path, name: &str) -> anyhow::Result<Palette> {
    let name = name.trim();
    let name = if name.is_empty() {
        Palette::DEFAULT_NAME
    } else {
        name
    };
    if let Some(palette) = Palette::built_in(name) {
        return Ok(palette);
    }
    let path = ThemeFile::path(vault_root, name);
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "theme {name:?} is not a built-in ({}) and no {}",
            Palette::BUILT_INS
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", "),
            path.display(),
        )
    })?;
    let theme =
        ThemeFile::from_toml(&text).with_context(|| format!("parsing theme {}", path.display()))?;
    let base = theme.base.as_deref().unwrap_or(Palette::DEFAULT_NAME);
    let base = Palette::built_in(base)
        .with_context(|| format!("unknown theme base {base:?} in {}", path.display()))?;
    Ok(base.apply(&theme))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_parse_with_or_without_the_hash() {
        assert_eq!(parse_color("#aabbcc").unwrap(), 0xaabbcc);
        assert_eq!(parse_color("aabbcc").unwrap(), 0xaabbcc);
        assert_eq!(parse_color(" #FFF000 ").unwrap(), 0xfff000);
        assert!(parse_color("red").is_err());
        assert!(parse_color("#abc").is_err());
        assert!(parse_color("#gggggg").is_err());
    }

    #[test]
    fn a_theme_file_overrides_only_what_it_names() {
        let theme = ThemeFile::from_toml(
            "base = \"light\"\nbg = \"#282828\"\nfg = \"#ebdbb2\"\nlink = \"#83a598\"\n",
        )
        .unwrap();
        let base = Palette::built_in("light").unwrap();
        let out = base.apply(&theme);
        assert_eq!(out.bg, 0x282828);
        assert_eq!(out.fg, 0xebdbb2);
        assert_eq!(out.link, 0x83a598);
        assert_eq!(out.tag, base.tag, "everything else is inherited");
    }

    #[test]
    fn a_theme_file_rejects_a_bad_key_or_colour() {
        assert!(ThemeFile::from_toml("base = \"light\"\nbak = \"#282828\"").is_err());
        assert!(ThemeFile::from_toml("bg = \"pink\"").is_err());
    }

    #[test]
    fn palette_for_resolves_built_ins_and_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(palette_for(dir.path(), "dark").unwrap(), Palette::DARK);
        assert_eq!(palette_for(dir.path(), "Light").unwrap(), Palette::LIGHT);
        // No name at all is the default palette, which is a flavour rather than
        // Shale's own dark value.
        assert_eq!(palette_for(dir.path(), "").unwrap(), Palette::DEFAULT);
        assert_eq!(Palette::DEFAULT, catppuccin::MOCHA);
        assert_eq!(
            Palette::built_in(Palette::DEFAULT_NAME),
            Some(Palette::DEFAULT)
        );
        // An unknown name with no file is an error the caller can report.
        assert!(palette_for(dir.path(), "gruvbox").is_err());

        let path = ThemeFile::path(dir.path(), "gruvbox");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "base = \"dark\"\nbg = \"#282828\"\n").unwrap();
        let out = palette_for(dir.path(), "gruvbox").unwrap();
        assert_eq!(out.bg, 0x282828);
        assert_eq!(out.fg, Palette::DARK.fg);

        std::fs::write(&path, "base = \"nonsense\"\n").unwrap();
        assert!(palette_for(dir.path(), "gruvbox").is_err());
    }

    /// WCAG relative luminance: the sRGB channels linearized and combined with
    /// the Rec. 709 weights. The channels have to be linearized first; weighting
    /// the raw 0-1 values (the obvious shortcut) under-reports mid-tone contrast
    /// badly enough to fail a palette that is perfectly readable — Catppuccin
    /// Latte's text on its own background is 7:1, which the shortcut calls 2.7:1.
    /// This is still only here to catch a theme whose text matches its
    /// background, not to certify a palette.
    fn luminance(rgb: u32) -> f32 {
        let channel = |byte: u32| {
            let c = byte as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel((rgb >> 16) & 0xff)
            + 0.7152 * channel((rgb >> 8) & 0xff)
            + 0.0722 * channel(rgb & 0xff)
    }

    /// The WCAG contrast ratio of two colours: 1 is identical, 21 is black on
    /// white, and 4.5 is the usual floor for body text.
    fn contrast(a: u32, b: u32) -> f32 {
        let x = luminance(a).max(luminance(b));
        let y = luminance(a).min(luminance(b));
        (x + 0.05) / (y + 0.05)
    }

    /// The built-in palettes have to be usable in their own right: a theme whose
    /// text matches its background is not a theme.
    ///
    /// Every role is judged against the surface it is actually drawn on, which
    /// is the note's background for most of them, the code block's for the
    /// `code_*` tokens, and the window's for the `ui_*` ones. The thresholds are
    /// the WCAG body-text floor for the roles a reader has to parse (note text,
    /// headings, links, tags, code) and a lower bar for the roles that are meant
    /// to recede (syntax markers, unresolved links, quoted text, comments) or
    /// only to be *distinguishable* rather than read (a rule, a tab's edge, a
    /// graph's edges). Roles that are not painted yet are still checked, so a
    /// reserved colour cannot quietly be a useless one.
    #[test]
    fn the_built_in_palettes_are_readable() {
        // (role, the role it is drawn over, floor)
        let roles = |p: &Palette| -> Vec<(&str, u32, &str, u32, f32)> {
            vec![
                ("note text", p.fg, "the note background", p.bg, 4.5),
                ("heading text", p.heading, "the note background", p.bg, 4.5),
                ("a resolved link", p.link, "the note background", p.bg, 3.0),
                ("a tag", p.tag, "the note background", p.bg, 3.0),
                ("quoted text", p.quote, "the note background", p.bg, 2.5),
                ("syntax markers", p.dim, "the note background", p.bg, 2.0),
                (
                    "an unresolved link",
                    p.link_unresolved,
                    "the note background",
                    p.bg,
                    2.0,
                ),
                (
                    "the checker's squiggle",
                    p.spell,
                    "the note background",
                    p.bg,
                    2.0,
                ),
                (
                    "ordinary code",
                    p.code_text,
                    "the code block",
                    p.code_block_bg,
                    4.0,
                ),
                (
                    "a code comment",
                    p.code_comment,
                    "the code block",
                    p.code_block_bg,
                    2.0,
                ),
                // A token colour only has to be distinguishable from the code
                // around it: `code_text` carries the body-text floor, and every
                // family's own syntax theme is the authority on its tokens.
                (
                    "a keyword",
                    p.code_keyword,
                    "the code block",
                    p.code_block_bg,
                    2.0,
                ),
                (
                    "a string",
                    p.code_string,
                    "the code block",
                    p.code_block_bg,
                    2.0,
                ),
                (
                    "a number",
                    p.code_number,
                    "the code block",
                    p.code_block_bg,
                    2.0,
                ),
                (
                    "a function name",
                    p.code_function,
                    "the code block",
                    p.code_block_bg,
                    2.0,
                ),
                (
                    "a type name",
                    p.code_type,
                    "the code block",
                    p.code_block_bg,
                    2.0,
                ),
                (
                    "a constant",
                    p.code_const,
                    "the code block",
                    p.code_block_bg,
                    2.0,
                ),
                (
                    "an operator",
                    p.code_operator,
                    "the code block",
                    p.code_block_bg,
                    2.0,
                ),
                ("window text", p.ui_fg, "the window", p.ui_bg, 4.5),
                ("a hint line", p.ui_dim, "the window", p.ui_bg, 2.0),
                (
                    "a focused rule",
                    p.ui_active_border,
                    "the window",
                    p.ui_bg,
                    1.2,
                ),
                ("a tab", p.ui_tab_bg, "the window", p.ui_bg, 1.1),
                (
                    "the open tab",
                    p.ui_tab_active_bg,
                    "the window",
                    p.ui_bg,
                    1.25,
                ),
                ("the panel", p.popup_bg, "the note", p.bg, 1.1),
                (
                    "the panel's rule",
                    p.popup_border,
                    "the panel",
                    p.popup_bg,
                    1.2,
                ),
                ("a graph node", p.graph_node, "the note", p.bg, 1.5),
                (
                    "the focused node",
                    p.graph_node_active,
                    "the note",
                    p.bg,
                    2.0,
                ),
                ("a graph edge", p.graph_edge, "the note", p.bg, 1.1),
                ("a node label", p.graph_text, "the note", p.bg, 2.5),
            ]
        };
        // A background role and the text role drawn on it, with the floor that
        // pair has to clear. A highlighted row gets a lower floor than body
        // text: it is transient, its text is short, and it has to be *visible*
        // against the surface it sits on, which for a palette whose text is a
        // mid-tone pulls the other way. Solarized's own tone table rates body
        // text on its secondary background as less than optimal reading, which
        // is why the cursor's line is held to 4.0 rather than 4.5.
        let pairs = |p: &Palette| -> Vec<(&str, u32, &str, u32, f32)> {
            vec![
                (
                    "the status bar's text",
                    p.ui_fg,
                    "the status bar",
                    p.ui_status_bg,
                    4.5,
                ),
                (
                    "what you type",
                    p.ui_fg,
                    "the prompt field",
                    p.ui_field_bg,
                    4.5,
                ),
                (
                    "the open tab's label",
                    p.ui_fg,
                    "the open tab",
                    p.ui_tab_active_bg,
                    3.0,
                ),
                ("the panel's text", p.ui_fg, "the panel", p.popup_bg, 4.5),
                (
                    "a panel row's text",
                    p.ui_fg,
                    "the highlighted row",
                    p.popup_hover_bg,
                    3.5,
                ),
                (
                    "text on the cursor's line",
                    p.fg,
                    "the cursor's line",
                    p.cursor_line_bg,
                    4.0,
                ),
            ]
        };
        for (name, palette) in Palette::BUILT_INS {
            for (role, colour, on, against, floor) in roles(palette) {
                assert!(
                    contrast(against, colour) > floor,
                    "{name}: {role} is too faint on {on}",
                );
            }
            // The cursor's line is a band, not a highlight: it has to be
            // visible without competing with the text on it.
            assert!(
                contrast(palette.bg, palette.cursor_line_bg) > 1.05,
                "{name}: the cursor's line is indistinguishable from the note",
            );
            // A picker's rows are drawn on the window's background, and the
            // highlighted one on `ui_active_bg`. Matched characters are short,
            // so they get the floor for short text on an ordinary row and a
            // lower one on the highlighted row, where the rest of the row is
            // still body text at full contrast.
            assert!(
                contrast(palette.ui_bg, palette.match_text) > 3.0,
                "{name}: a query's matched characters are too faint on a picker row",
            );
            assert!(
                contrast(palette.ui_active_bg, palette.match_text) > 2.5,
                "{name}: a query's matched characters are too faint on the chosen row",
            );
            for (what, colour, on, against, floor) in pairs(palette) {
                assert!(
                    contrast(against, colour) > floor,
                    "{name}: {what} is too faint on {on}",
                );
            }
        }
        assert_ne!(Palette::DARK.bg, Palette::LIGHT.bg);
        // Every callout role reads off the palette it was given, so a theme can
        // retune one type without touching the others.
        let dark = Palette::DARK;
        assert_eq!(
            dark.callout_accent(CalloutKind::Warning),
            dark.callout_warning
        );
        assert_eq!(dark.callout_accent(CalloutKind::Quote), dark.quote);
        assert_eq!(dark.callout_accent(CalloutKind::Other), dark.accent);
    }

    /// The Catppuccin flavours are selectable by name, by their `catppuccin-`
    /// prefixed alias, and as the `base` of a theme file, so a Catppuccin user
    /// can pick a flavour or tune one without leaving the palette they know.
    #[test]
    fn the_catppuccin_flavours_are_built_ins() {
        let dir = tempfile::tempdir().unwrap();
        for (name, palette) in catppuccin::FLAVORS {
            assert_eq!(Palette::built_in(name), Some(*palette));
            assert_eq!(
                Palette::built_in(&format!("catppuccin-{name}")),
                Some(*palette)
            );
            // Case does not matter, and the alias works through the loader too.
            let upper = name.to_uppercase();
            assert_eq!(palette_for(dir.path(), &upper).unwrap(), *palette);
            assert_eq!(
                palette_for(dir.path(), &format!("Catppuccin-{upper}")).unwrap(),
                *palette
            );
        }
        // The flavours are the four upstream ones, lightest first, and they are
        // distinct palettes rather than one palette copied four times.
        assert_eq!(catppuccin::FLAVORS.len(), 4);
        assert_eq!(catppuccin::FLAVORS[0].0, "latte");
        assert_eq!(catppuccin::FLAVORS[3].0, "mocha");
        assert_eq!(catppuccin::MOCHA.bg, 0x1e1e2e);
        assert_eq!(catppuccin::LATTE.bg, 0xeff1f5);

        // A flavour is a legal `base`, so a theme file can be a small edit to it.
        let path = ThemeFile::path(dir.path(), "mocha-warm");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "base = \"mocha\"\nbg = \"#1e1e28\"\n").unwrap();
        let out = palette_for(dir.path(), "mocha-warm").unwrap();
        assert_eq!(out.bg, 0x1e1e28);
        assert_eq!(out.link, catppuccin::MOCHA.link, "the rest is Mocha");
    }

    /// The other theme families are selectable by name and as the `base` of a
    /// theme file, in the spellings their own users type: mixed case, and for a
    /// family whose dark variant carries the bare name, a `-dark` suffix.
    #[test]
    fn the_other_families_are_built_ins() {
        let named: &[(&str, Palette)] = &[
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
        let dir = tempfile::tempdir().unwrap();
        let path = ThemeFile::path(dir.path(), "mine");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for (name, palette) in named {
            assert_eq!(Palette::built_in(name), Some(*palette), "{name}");
            assert_eq!(
                palette_for(dir.path(), &name.to_uppercase()).unwrap(),
                *palette,
                "{name} is found whatever case it is written in"
            );
            // Any of them can be the base a vault's own theme file edits.
            std::fs::write(&path, format!("base = \"{name}\"\nbg = \"#010203\"\n")).unwrap();
            let out = palette_for(dir.path(), "mine").unwrap();
            assert_eq!(out.bg, 0x010203, "{name} as a theme file's base");
            assert_eq!(out.accent, palette.accent, "the rest of {name} is kept");
        }

        // The two spellings people reach for that are not themselves names.
        assert_eq!(
            Palette::built_in("Tokyo-Night-Dark"),
            Some(tokyo_night::NIGHT)
        );
        assert_eq!(Palette::built_in("rose-pine-dark"), Some(rose_pine::MAIN));
        assert_eq!(
            Palette::built_in("solarized-dark"),
            Some(solarized::DARK),
            "a family that spells its own variants -dark keeps that name"
        );
        // A family name on its own would have to guess between its variants.
        assert_eq!(Palette::built_in("gruvbox"), None);
        assert_eq!(Palette::built_in("one"), None);
    }

    /// The registry the picker will list has to be a set: no two entries sharing
    /// a name, and no two entries that are the same colours under different names.
    #[test]
    fn the_built_in_list_is_a_set() {
        for (i, (name, palette)) in Palette::BUILT_INS.iter().enumerate() {
            for (other, other_palette) in &Palette::BUILT_INS[i + 1..] {
                assert!(!name.eq_ignore_ascii_case(other), "{name} is listed twice");
                assert_ne!(*palette, *other_palette, "{name} and {other} are identical");
            }
        }
        // A name is a file stem, so nothing in the list may need quoting.
        for (name, _) in Palette::BUILT_INS {
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{name} is not a plain lowercase file stem"
            );
        }
    }
}
