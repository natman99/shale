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

/// Every colour the editor draws, as 0xRRGGBB.
///
/// The names describe roles, not hues, because a theme will reuse the same role
/// for very different colours: `dim` is syntax punctuation in both the dark and
/// the light palette even though one is grey and the other is a dark warm grey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    /// Note background.
    pub bg: u32,
    /// Ordinary note text.
    pub fg: u32,
    /// The normal-mode cursor block.
    pub cursor: u32,
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
    /// Background of an inline code span.
    pub code_bg: u32,
    /// Background of a fenced code block's lines.
    pub code_block_bg: u32,
    /// Background of `==highlighted==` text.
    pub highlight_bg: u32,
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
        selection: 0x264f78,
        link: 0x7aa6f0,
        link_unresolved: 0x7d6f6f,
        dim: 0x6b7280,
        heading: 0xe8e8e8,
        quote: 0x9aa0a6,
        accent: 0x7aa6f0,
        tag: 0x8fbf7f,
        code_bg: 0x2f2f2f,
        code_block_bg: 0x262626,
        highlight_bg: 0x4a3f1f,
        table_line: 0x3f3f46,
        table_header_bg: 0x2a2a2e,
        callout_bg: 0x252a33,
        ui_bg: 0x1e1e1e,
        ui_status_bg: 0x252526,
        ui_border: 0x3c3c3c,
        ui_hover_bg: 0x2a2a2a,
        ui_active_bg: 0x2f2f2f,
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
        selection: 0xbfd7f0,
        link: 0x17497c,
        link_unresolved: 0x7d6262,
        dim: 0x686d76,
        heading: 0x1f1f23,
        quote: 0x545960,
        accent: 0x17497c,
        tag: 0x245824,
        code_bg: 0xe8e8e8,
        code_block_bg: 0xefefef,
        highlight_bg: 0xf5e0a3,
        table_line: 0xc9c9cf,
        table_header_bg: 0xe9e9ee,
        callout_bg: 0xeef1f6,
        ui_bg: 0xf0f0f2,
        ui_status_bg: 0xe6e6e9,
        ui_border: 0xcfcfd4,
        ui_hover_bg: 0xe4e4e8,
        ui_active_bg: 0xd8d8de,
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
            selection,
            link,
            link_unresolved,
            dim,
            heading,
            quote,
            accent,
            tag,
            code_bg,
            code_block_bg,
            highlight_bg,
            table_line,
            table_header_bg,
            callout_bg,
            ui_bg,
            ui_status_bg,
            ui_border,
            ui_hover_bg,
            ui_active_bg,
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
            (&mut out.selection, selection),
            (&mut out.link, link),
            (&mut out.link_unresolved, link_unresolved),
            (&mut out.dim, dim),
            (&mut out.heading, heading),
            (&mut out.quote, quote),
            (&mut out.accent, accent),
            (&mut out.tag, tag),
            (&mut out.code_bg, code_bg),
            (&mut out.code_block_bg, code_block_bg),
            (&mut out.highlight_bg, highlight_bg),
            (&mut out.table_line, table_line),
            (&mut out.table_header_bg, table_header_bg),
            (&mut out.callout_bg, callout_bg),
            (&mut out.ui_bg, ui_bg),
            (&mut out.ui_status_bg, ui_status_bg),
            (&mut out.ui_border, ui_border),
            (&mut out.ui_hover_bg, ui_hover_bg),
            (&mut out.ui_active_bg, ui_active_bg),
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
    /// Background of an inline code span.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_bg: Option<u32>,
    /// Background of a fenced code block's lines.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub code_block_bg: Option<u32>,
    /// Background of `==highlighted==` text.
    #[serde(default, deserialize_with = "deserialize_color")]
    pub highlight_bg: Option<u32>,
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
    /// text matches its background is not a theme. The thresholds are the WCAG
    /// body-text floor for the roles a reader has to parse (note text, headings,
    /// links, tags) and a lower bar for the roles that are meant to recede
    /// (syntax markers, unresolved links, quoted text).
    #[test]
    fn the_built_in_palettes_are_readable() {
        for (name, palette) in Palette::BUILT_INS {
            for (role, colour, floor) in [
                ("note text", palette.fg, 4.5),
                ("heading text", palette.heading, 4.5),
                ("a resolved link", palette.link, 3.0),
                ("a tag", palette.tag, 3.0),
                ("quoted text", palette.quote, 2.5),
                ("syntax markers", palette.dim, 2.0),
                ("an unresolved link", palette.link_unresolved, 2.0),
            ] {
                assert!(
                    contrast(palette.bg, colour) > floor,
                    "{name}: {role} is too faint on the note background",
                );
            }
            assert!(
                contrast(palette.ui_status_bg, palette.fg) > 4.5,
                "{name}: the status bar draws the note's foreground colour",
            );
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
