//! The editor's palette and how markdown styles become painted text runs.
//!
//! Colours live in one place rather than at each paint site so the theming goal
//! only has to swap a single table: a TOML theme will fill in this palette
//! instead of the dark defaults below. The mapping from a `shale_core::mdstyle::
//! Style` (which knows nothing about painting) to a [`RunStyle`] is what keeps
//! the parser and the presentation separate.

use gpui::{px, rgb, Font, StrikethroughStyle, TextRun, UnderlineStyle};
use shale_core::mdstyle::Style;

/// Height of one display line, in pixels.
pub(crate) const LINE_HEIGHT: f32 = 20.0;
/// Text size the editor shapes and wraps at.
pub(crate) const FONT_SIZE: f32 = 14.0;
/// Heading levels 1..=6 plus body text (level 0): how many text sizes, and so
/// how many line wrappers, a frame may need.
pub(crate) const SIZES: usize = 7;

/// The text size a heading level is drawn at (level 0 is body text).
pub(crate) fn font_size(level: u8) -> f32 {
    match level {
        1 => 22.0,
        2 => 19.0,
        3 => 17.0,
        4 => 16.0,
        5 => 15.0,
        // Level 6 headings are body-sized but still bold.
        _ => FONT_SIZE,
    }
}

/// The height of a display line drawn at `size`, keeping the body line's
/// leading ratio so wrapped text does not crowd.
pub(crate) fn line_height(size: f32) -> f32 {
    size * (LINE_HEIGHT / FONT_SIZE)
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

/// Note background.
pub(crate) const BG: u32 = 0x1e1e1e;
/// Ordinary note text.
pub(crate) const FG: u32 = 0xd4d4d4;
/// The normal-mode cursor block.
pub(crate) const CURSOR: u32 = 0xaeafad;
/// Visual-mode selection.
pub(crate) const SELECTION: u32 = 0x264f78;
/// A wiki-link that names an existing note.
pub(crate) const LINK: u32 = 0x7aa6f0;
/// A wiki-link that does not (yet) name an existing note.
pub(crate) const LINK_UNRESOLVED: u32 = 0x7d6f6f;
/// Syntax punctuation and markers: `#`, `*`, `|`, brackets, table pipes.
pub(crate) const DIM: u32 = 0x6b7280;
/// Heading text.
pub(crate) const HEADING: u32 = 0xe8e8e8;
/// Blockquote text.
pub(crate) const QUOTE: u32 = 0x9aa0a6;
/// List bullets, task boxes and other structural accents.
pub(crate) const ACCENT: u32 = 0x7aa6f0;
/// A `#tag`.
pub(crate) const TAG: u32 = 0x8fbf7f;
/// Background of an inline code span.
pub(crate) const CODE_BG: u32 = 0x2f2f2f;
/// Background of a fenced code block's lines.
pub(crate) const CODE_BLOCK_BG: u32 = 0x262626;
/// Background of `==highlighted==` text.
pub(crate) const HIGHLIGHT_BG: u32 = 0x4a3f1f;

/// How a run of text is drawn, resolved from a markdown [`Style`].
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
    /// Strike the run through (`~~text~~`).
    pub strikethrough: bool,
}

impl RunStyle {
    /// Ordinary note text.
    pub(crate) const PLAIN: RunStyle = RunStyle {
        color: FG,
        bold: false,
        italic: false,
        background: None,
        underline: false,
        strikethrough: false,
    };

    /// How a markdown span of `style` is drawn.
    pub(crate) fn for_style(style: Style) -> Self {
        let mut run = Self::PLAIN;
        match style {
            Style::Heading { .. } => {
                run.color = HEADING;
                run.bold = true;
            }
            Style::HeadingMarker => {
                run.color = DIM;
                run.bold = true;
            }
            Style::Quote => {
                run.color = QUOTE;
                run.italic = true;
            }
            Style::QuoteMarker => run.color = DIM,
            Style::ListMarker => run.color = ACCENT,
            Style::TaskMark { done } => {
                run.color = if done { ACCENT } else { DIM };
                run.bold = true;
            }
            Style::Strong => run.bold = true,
            Style::Emphasis => run.italic = true,
            Style::Strike => run.strikethrough = true,
            Style::Highlight => run.background = Some(HIGHLIGHT_BG),
            Style::Code => run.background = Some(CODE_BG),
            Style::CodeBlock => run.background = Some(CODE_BLOCK_BG),
            Style::CodeFence => {
                run.color = DIM;
                run.background = Some(CODE_BLOCK_BG);
            }
            Style::LinkText => run.color = LINK,
            Style::LinkUrl => {
                run.color = DIM;
                run.underline = true;
            }
            // The editor overrides the colour of wiki-link spans with
            // resolved/unresolved colours, so this is only the fallback.
            Style::WikiLink | Style::WikiEmbed => run.color = LINK,
            Style::Tag => run.color = TAG,
            Style::TablePipe | Style::TableRule | Style::Rule => run.color = DIM,
            Style::Punctuation => run.color = DIM,
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
            underline: self.underline.then_some(UnderlineStyle {
                thickness: px(1.),
                color: None,
                wavy: false,
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
