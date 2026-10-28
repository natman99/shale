//! The editor's text metrics, font choice, and how markdown styles become
//! painted text runs.
//!
//! Colours are *not* here: they are [`shale_core::theme::Palette`], a bag of
//! 0xRRGGBB roles that the built-in themes (Shale's `dark`/`light` and the
//! Catppuccin, Tokyo Night, Gruvbox, Nord, Dracula, Solarized, One and Rosé Pine
//! palettes) and any TOML theme file produce, and that the editor hands to every
//! paint site. What this module owns is the part that needs gpui: turning a
//! `shale_core::mdstyle::Style` (which knows nothing about painting) plus a
//! palette into a [`RunStyle`], which is what keeps the parser and the
//! presentation separate.

use gpui::{px, rgb, Font, StrikethroughStyle, TextRun, UnderlineStyle};
use shale_core::mdstyle::Style;
use shale_core::theme::Palette;

/// The text size to shape and wrap at when the vault asks for none.
pub(crate) const DEFAULT_FONT_SIZE: f32 = 14.0;
/// How much taller a display line is than the text it holds, so wrapped rows do
/// not crowd. Every size uses the same ratio.
pub(crate) const LINE_RATIO: f32 = 20.0 / 14.0;
/// Heading levels 1..=6 plus body text (level 0): how many text sizes, and so
/// how many line wrappers, a frame may need.
pub(crate) const SIZES: usize = 7;

/// The heading sizes at the default text size; a larger or smaller base scales
/// all of them by the same factor, so a note's proportions survive theming.
const HEADING_SIZES: [f32; 6] = [22.0, 19.0, 17.0, 16.0, 15.0, 14.0];

/// The text size a heading level is drawn at (level 0 is body text), relative to
/// the editor's base text size `base`.
pub(crate) fn font_size(level: u8, base: f32) -> f32 {
    if level == 0 {
        return base;
    }
    let scale = base / DEFAULT_FONT_SIZE;
    HEADING_SIZES[(level as usize).min(HEADING_SIZES.len()) - 1] * scale
}

/// The height of a display line drawn at `size`.
pub(crate) fn line_height(size: f32) -> f32 {
    size * LINE_RATIO
}

/// Monospace families we know how to find, in preference order. Families with
/// real bold **and** italic faces come first: gpui matches a family name
/// literally against the installed fonts and never synthesizes a missing face,
/// so `font("monospace")` on Linux matches nothing and gpui's fallback stack
/// hands back a proportional UI font that also ignores weight and style. That
/// is why bold and italic looked identical before the family was resolved.
pub(crate) const MONO_PREFERENCES: &[&str] = &[
    "JetBrains Mono",
    "DejaVu Sans Mono",
    "Adwaita Mono",
    "Liberation Mono",
    "Noto Sans Mono",
    "Menlo",
    "SF Mono",
    "Consolas",
    "Courier New",
];

/// The family to draw the editor with: `preferred` when it is installed,
/// otherwise the first installed [`MONO_PREFERENCES`] entry, otherwise
/// `preferred` unchanged (which lets the caller log that nothing matched).
/// Matching ignores case, as fontconfig does.
pub(crate) fn pick_mono_family(available: &[String], preferred: &str) -> String {
    let installed =
        |name: &str| !name.is_empty() && available.iter().any(|f| f.eq_ignore_ascii_case(name));
    if installed(preferred) {
        return preferred.to_string();
    }
    MONO_PREFERENCES
        .iter()
        .find(|name| installed(name))
        .map(|name| name.to_string())
        .unwrap_or_else(|| preferred.to_string())
}

/// How a run of text is drawn, resolved from a markdown [`Style`] and a
/// [`Palette`].
///
/// The editor builds a per-byte map of these for each display line and then
/// coalesces neighbours into `gpui::TextRun`s, so this has to stay cheap to
/// copy and compare.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct RunStyle {
    /// Glyph colour as 0xRRGGBB.
    pub color: u32,
    /// Draw with the font's bold face. Bold glyphs measure wider than plain
    /// ones, which the soft-wrap model has to account for.
    pub bold: bool,
    /// Draw with the font's italic face.
    pub italic: bool,
    /// Background behind the glyphs (inline code, highlights, code blocks).
    pub background: Option<u32>,
    /// Underline the run.
    pub underline: bool,
    /// Wavy-underline the run in this colour: the spell and grammar checker's
    /// note about it. `None` for ordinary text. Kept apart from `underline`,
    /// which markdown uses for links and which must stay straight.
    pub spell: Option<u32>,
    /// Strike the run through (`~~text~~`).
    pub strikethrough: bool,
}

impl RunStyle {
    /// Ordinary note text in `palette`.
    pub(crate) fn plain(palette: &Palette) -> RunStyle {
        RunStyle {
            color: palette.fg,
            bold: false,
            italic: false,
            background: None,
            underline: false,
            spell: None,
            strikethrough: false,
        }
    }

    /// How a markdown span of `style` is drawn with `palette`.
    pub(crate) fn for_style(style: Style, palette: &Palette) -> Self {
        let mut run = Self::plain(palette);
        match style {
            Style::Heading { .. } => {
                run.color = palette.heading;
                run.bold = true;
            }
            Style::HeadingMarker => {
                run.color = palette.dim;
                run.bold = true;
            }
            Style::Quote => {
                run.color = palette.quote;
                run.italic = true;
            }
            Style::QuoteMarker => run.color = palette.dim,
            Style::ListMarker => run.color = palette.accent,
            Style::TaskMark { done } => {
                run.color = if done { palette.accent } else { palette.dim };
                run.bold = true;
            }
            Style::Strong => run.bold = true,
            Style::Emphasis => run.italic = true,
            Style::Strike => run.strikethrough = true,
            Style::Highlight => run.background = Some(palette.highlight_bg),
            Style::Code => run.background = Some(palette.code_bg),
            Style::CodeBlock => run.background = Some(palette.code_block_bg),
            Style::CodeFence => {
                run.color = palette.dim;
                run.background = Some(palette.code_block_bg);
            }
            Style::LinkText => run.color = palette.link,
            Style::LinkUrl => {
                run.color = palette.dim;
                run.underline = true;
            }
            // The hidden half of `[[target|alias]]`: drawn dimly only while the
            // row is being edited (see `shale_core::mdlayout`).
            Style::LinkTarget => run.color = palette.dim,
            // The editor overrides the colour of wiki-link spans with
            // resolved/unresolved colours, so this is only the fallback.
            Style::WikiLink | Style::WikiEmbed => run.color = palette.link,
            Style::Tag => run.color = palette.tag,
            // The box itself is painted by the line element (a background quad
            // and an accent bar), so its text stays ordinary.
            Style::Callout { .. } => {}
            Style::CalloutTitle { kind } => {
                run.color = palette.callout_accent(kind);
                run.bold = true;
            }
            Style::TablePipe | Style::TableRule | Style::Rule => run.color = palette.dim,
            Style::Punctuation => run.color = palette.dim,
        }
        run
    }
    /// Whether this style is drawn with a face that measures wider than the
    /// regular one (bold, or italic for some fonts). Used to pick the font the
    /// soft wrapper measures a row with.
    pub(crate) fn is_wide(&self) -> bool {
        self.bold || self.italic
    }

    /// A `gpui::TextRun` covering `len` utf-8 bytes, drawn in this style on top
    /// of `base` (the line's font).
    pub(crate) fn text_run(&self, len: usize, base: &Font) -> TextRun {
        let mut font = base.clone();
        if self.bold {
            font = font.bold();
        }
        if self.italic {
            font = font.italic();
        }
        TextRun {
            len,
            font,
            color: rgb(self.color).into(),
            background_color: self.background.map(|c| rgb(c).into()),
            // A markdown underline (a link) and a checker's note can land on the
            // same run; the checker's squiggle wins, since it is the one the user
            // is meant to act on.
            underline: self
                .spell
                .map(|color| UnderlineStyle {
                    thickness: px(1.),
                    color: Some(rgb(color).into()),
                    wavy: true,
                })
                .or_else(|| {
                    self.underline.then_some(UnderlineStyle {
                        thickness: px(1.),
                        color: None,
                        wavy: false,
                    })
                }),
            strikethrough: self.strikethrough.then_some(StrikethroughStyle {
                thickness: px(1.),
                color: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The heading sizes follow the base text size, so a vault that asks for
    /// bigger or smaller text keeps the same proportions.
    #[test]
    fn sizes_scale_with_the_base_text_size() {
        assert_eq!(font_size(0, 14.0), 14.0);
        assert_eq!(font_size(1, 14.0), 22.0);
        assert_eq!(font_size(6, 14.0), 14.0);
        // A 50% larger base scales every level the same way.
        assert_eq!(font_size(1, 21.0), 33.0);
        assert_eq!(font_size(0, 21.0), 21.0);
        // A level past six is still body text.
        assert_eq!(font_size(9, 14.0), 14.0);
        // The line height keeps one ratio at every size.
        assert!((line_height(14.0) - 20.0).abs() < 1e-5);
        assert!((line_height(21.0) - 30.0).abs() < 1e-5);
    }

    /// A run's colours come from the palette it is given, which is what makes a
    /// theme a data change rather than a paint change.
    #[test]
    fn a_run_style_reads_the_palette() {
        let dark = RunStyle::for_style(Style::Heading { level: 1 }, &Palette::DARK);
        let light = RunStyle::for_style(Style::Heading { level: 1 }, &Palette::LIGHT);
        assert!(
            dark.bold && light.bold,
            "the face does not depend on the theme"
        );
        assert_eq!(dark.color, Palette::DARK.heading);
        assert_eq!(light.color, Palette::LIGHT.heading);
        assert_ne!(dark.color, light.color);
        // Ordinary text is the palette's foreground, with no background.
        let plain = RunStyle::plain(&Palette::LIGHT);
        assert_eq!(plain.color, Palette::LIGHT.fg);
        assert_eq!(plain.background, None);
    }

    /// The family choice has to land on something installed *and* able to draw
    /// bold and italic, because gpui will not synthesize either.
    #[test]
    fn picks_an_installed_monospace_family() {
        let available = vec![
            "Adwaita Sans".to_string(),
            "Noto Sans Mono".to_string(),
            "JetBrains Mono".to_string(),
        ];
        // A configured family wins when it exists.
        assert_eq!(
            pick_mono_family(&available, "Noto Sans Mono"),
            "Noto Sans Mono"
        );
        // Otherwise the best preference, in preference order.
        assert_eq!(pick_mono_family(&available, ""), "JetBrains Mono");
        // Case-insensitive, like fontconfig.
        assert_eq!(
            pick_mono_family(&available, "jetbrains mono"),
            "jetbrains mono"
        );
        // Nothing installed: hand back the request so the caller can log it.
        assert_eq!(pick_mono_family(&[], "Iosevka"), "Iosevka");
        assert_eq!(pick_mono_family(&[], ""), "");
    }
}
