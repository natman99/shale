//! The editor's colour palette and the theme files that change it.
//!
//! Every colour the editor draws lives here rather than at each paint site, so
//! theming is a data problem: a [`Palette`] is a bag of 0xRRGGBB integers, each
//! built-in theme is one value of it, and a theme file is a partial override of
//! one. Painting (which needs gpui types) stays in the app and just reads the
//! palette it is given.
//!
//! A theme is named in `.shale/config.toml` as `appearance.theme`. The built-in
//! names are `dark` and `light` plus the palettes of seven popular theme families --
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

mod builtins;
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

/// The names the vault's own theme files answer to: the stems of
/// `<vault>/.shale/themes/*.toml`, in sorted order.
///
/// A vault with no theme folder is not an error — it simply has no themes of its
/// own, which is the common case. A file whose stem is also a built-in name is
/// listed anyway, because the file is what [`palette_for`] resolves, and hiding
/// the name would make the file unreachable.
pub fn theme_names(vault_root: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(vault_root.join(THEME_DIR)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .strip_suffix(".toml")
                .map(|stem| stem.to_string())
        })
        .filter(|stem| !stem.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}
#[cfg(test)]
mod tests;
