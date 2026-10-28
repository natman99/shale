use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::theme::Palette;

/// Top-level Shale configuration, loaded from `shale.toml` in the vault
/// (all fields optional; defaults apply when absent).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorConfig {
    /// Vim-style normal/insert mode bindings.
    pub vim_mode: bool,
    /// Soft-wrap long lines.
    pub line_wrap: bool,
    /// Show the file tree on startup.
    pub show_file_tree: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            vim_mode: true,
            line_wrap: true,
            show_file_tree: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppearanceConfig {
    /// The palette the editor draws with: a built-in (`dark`, `light`, or one of
    /// the family palettes — Catppuccin, Tokyo Night, Gruvbox, Nord, Dracula,
    /// Solarized, One and Rosé Pine; see [`crate::theme::Palette::BUILT_INS`] for
    /// the names), or the name of a `<vault>/.shale/themes/<name>.toml` file (see
    /// [`crate::theme`]). The default is [`Palette::DEFAULT_NAME`].
    pub theme: String,
    /// UI font size in points.
    pub font_size: f32,
    /// Font family for the editor. Empty means "pick a monospace family that is
    /// actually installed" (see the app's `theme::pick_mono_family`): on Linux a
    /// family name is matched literally, so the generic name `monospace` matches
    /// no family and the platform falls back to a proportional UI font.
    pub font_family: String,
    /// Live preview (render markdown while editing).
    pub live_preview: bool,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: Palette::DEFAULT_NAME.to_string(),
            font_size: 14.0,
            font_family: String::new(),
            live_preview: true,
        }
    }
}

impl Config {
    /// Load from `shale.toml` inside `dir`, falling back to defaults when
    /// the file does not exist.
    pub fn load(dir: &Path) -> anyhow::Result<Config> {
        let path = dir.join("shale.toml");
        if !path.is_file() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading config {}", path.display()))?;
        Config::from_toml(&text).with_context(|| format!("parsing config {}", path.display()))
    }

    pub fn from_toml(text: &str) -> anyhow::Result<Config> {
        Ok(toml::from_str(text)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_missing_fields() {
        let cfg = Config::from_toml("[editor]\nvim_mode = false").unwrap();
        assert!(!cfg.editor.vim_mode);
        assert!(cfg.editor.line_wrap);
        assert!(cfg.appearance.live_preview);
        assert_eq!(cfg.appearance.font_size, 14.0);
        assert_eq!(cfg.appearance.font_family, "");
        assert_eq!(cfg.appearance.theme, Palette::DEFAULT_NAME);
    }

    #[test]
    fn a_theme_can_be_named() {
        let cfg = Config::from_toml("[appearance]\ntheme = \"gruvbox\"\nfont_size = 12.0").unwrap();
        assert_eq!(cfg.appearance.theme, "gruvbox");
        assert_eq!(cfg.appearance.font_size, 12.0);
    }

    #[test]
    fn full_config_roundtrips() {
        let toml = r#"
[editor]
vim_mode = false
line_wrap = false
show_file_tree = false

[appearance]
font_size = 16.5
font_family = "Iosevka"
live_preview = false
theme = "light"
"#;
        let cfg = Config::from_toml(toml).unwrap();
        assert!(!cfg.editor.vim_mode);
        assert_eq!(cfg.appearance.font_size, 16.5);
        assert_eq!(cfg.appearance.font_family, "Iosevka");
        assert_eq!(cfg.appearance.theme, "light");
    }

    #[test]
    fn unknown_keys_rejected() {
        assert!(Config::from_toml("[editor]\nnope = true").is_err());
    }

    #[test]
    fn load_missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(dir.path()).unwrap();
        assert_eq!(cfg, Config::default());
    }
}
