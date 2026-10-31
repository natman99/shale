//! Shale's settings, and the file they live in.
//!
//! That file is `<vault>/.shale/config.toml`. Shale's own files belong under
//! `.shale` rather than in the vault's root, where they would sit among the
//! notes as things the user never wrote; the file tree already refuses to show or
//! touch anything under a dot directory (see [`crate::Vault`]), so the config is
//! out of the way of the vault by construction.
//!
//! Everything in it is optional. [`Config::starter`] writes the core settings out
//! once, each with the comment that explains it, and after that a change rewrites
//! only the single line it touches ([`edit`]), so the file can stay exactly as
//! short as the user wants: a list of what they disagree with the defaults about.

mod edit;

pub use edit::{write_setting, Value};

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::theme::Palette;

/// The directory Shale keeps its own files in, under the vault root: the config,
/// the vault's theme files, and the database and cache that will join them.
pub const DIR: &str = ".shale";

/// The config file's name inside [`DIR`].
pub const FILE: &str = "config.toml";

/// Top-level Shale configuration, loaded from `<vault>/.shale/config.toml`
/// (all fields optional; defaults apply when absent).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub spellcheck: SpellcheckConfig,
    #[serde(default)]
    pub index: IndexConfig,
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
    /// Write the note to disk on its own: once [`Self::autosave_after_ms`] has
    /// passed with the typing stopped, when the window loses focus, and before a
    /// quit. On by default — a note editor that can lose work is worse than one
    /// that writes a file the user did not ask it to.
    pub autosave: bool,
    /// How long the note must sit untouched before an autosave writes it. Long
    /// enough that it never lands between two keystrokes.
    pub autosave_after_ms: u64,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            vim_mode: true,
            line_wrap: true,
            show_file_tree: true,
            autosave: true,
            autosave_after_ms: 2000,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpellcheckConfig {
    /// Ask a language server for spelling and grammar notes. On by default; when
    /// the server is not installed Shale simply has nothing to show.
    pub enabled: bool,
    /// The command that starts the server, spoken to over standard input and
    /// output using the Language Server Protocol.
    pub command: String,
    /// Arguments for it. `--stdio` is what moves harper-ls off its default TCP
    /// port and onto the pipes.
    pub args: Vec<String>,
}

impl Default for SpellcheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            command: "harper-ls".to_string(),
            args: vec!["--stdio".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexConfig {
    /// Keep `.shale/shale.db` up to date: the index of notes, tags and links the
    /// word count and (later) the graph, search and agent read. On by default.
    ///
    /// Off means those features have nothing to read, exactly as if the DB file had
    /// been deleted — which is always a safe thing to do to it, because the notes
    /// are the source of truth and the index is a cache.
    pub enabled: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Config {
    /// `<vault>/.shale`: where Shale keeps its own files inside a vault.
    pub fn shale_dir(vault_root: &Path) -> PathBuf {
        vault_root.join(DIR)
    }

    /// `<vault>/.shale/config.toml`.
    pub fn path(vault_root: &Path) -> PathBuf {
        Self::shale_dir(vault_root).join(FILE)
    }

    /// Read the vault's config, or defaults when there is none. Read-only: it
    /// creates nothing. An app opening a vault wants [`Config::load_or_create`].
    pub fn load(vault_root: &Path) -> anyhow::Result<Config> {
        Config::load_file(&Self::path(vault_root))
    }

    /// Read one config file by path, defaults when that file does not exist.
    pub fn load_file(path: &Path) -> anyhow::Result<Config> {
        if !path.is_file() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        Config::from_toml(&text).with_context(|| format!("parsing config {}", path.display()))
    }

    /// Read the vault's config, making sure the file exists first: a vault with
    /// no config at all gets [`Config::starter`] written into
    /// `.shale/config.toml`, and a vault whose config is still the root-level
    /// `shale.toml` Shale used to keep it in has that file **moved** into
    /// `.shale`.
    ///
    /// Moved rather than also read, because there is one config location now: a
    /// second place to read from is a second place to wonder about later.
    pub fn load_or_create(vault_root: &Path) -> anyhow::Result<(Config, ConfigSource)> {
        let path = Self::path(vault_root);
        let shale_dir = Self::shale_dir(vault_root);
        let source = if path.is_file() {
            ConfigSource::File
        } else if let Some(legacy) = legacy_path(vault_root) {
            std::fs::create_dir_all(&shale_dir)
                .with_context(|| format!("creating {}", shale_dir.display()))?;
            std::fs::rename(&legacy, &path)
                .with_context(|| format!("moving {} to {}", legacy.display(), path.display()))?;
            ConfigSource::Moved
        } else {
            std::fs::create_dir_all(&shale_dir)
                .with_context(|| format!("creating {}", shale_dir.display()))?;
            std::fs::write(&path, Self::starter())
                .with_context(|| format!("writing config {}", path.display()))?;
            ConfigSource::Created
        };
        Ok((Config::load_file(&path)?, source))
    }

    pub fn from_toml(text: &str) -> anyhow::Result<Config> {
        Ok(toml::from_str(text)?)
    }

    /// The config file Shale writes for a vault that has none: the core settings,
    /// each at its default, each with the comment that explains it.
    ///
    /// It parses to exactly [`Config::default`], which is what lets it be a
    /// snippet rather than a dump: every line in it is a line the user may
    /// delete, and a setting added later need not appear until it changes.
    pub fn starter() -> &'static str {
        r#"# Shale reads this file when it opens the vault. Every key is optional, and
# every value here is already the default: delete any line you do not care about
# and Shale uses its default for it. Shale rewrites a line when it changes a
# setting of its own (a theme you pick, say), and leaves the rest of the file,
# comments included, exactly as it finds it.

[editor]
# Vim-style normal/insert bindings, soft wrap, and the file tree on startup.
vim_mode = true
line_wrap = true
show_file_tree = true
# The note is written on its own: once typing has stopped for this long, when
# the window loses focus, and before a quit. `:w` still works; it is just rarely
# the difference between keeping the note and losing it.
autosave = true
autosave_after_ms = 2000

[appearance]
# The palette: a built-in, or the name of a file in .shale/themes/<name>.toml.
# The built-ins are Shale's own "dark" and "light" plus the families it ships:
#   Catppuccin   "latte" "frappe" "macchiato" "mocha" (also "catppuccin-mocha")
#   Tokyo Night  "tokyo-night" "tokyo-night-storm" "tokyo-night-light"
#   Gruvbox      "gruvbox-dark" "gruvbox-light"
#   Nord, Dracula   "nord" "dracula"
#   Solarized    "solarized-dark" "solarized-light"
#   One          "one-dark" "one-light"
#   Rose Pine    "rose-pine" "rose-pine-moon" "rose-pine-dawn"
# Picking a theme in the app rewrites this line, so the choice survives a restart.
theme = "mocha"
# Body text size in points; headings and line heights are scaled from it.
font_size = 14.0
# Empty means "pick a monospace family that is actually installed".
font_family = ""
# Hide the markdown syntax of a row the cursor is not on.
live_preview = true

[spellcheck]
# The language server that finds spelling and grammar notes. A server that is not
# installed simply means nothing gets underlined.
enabled = true
command = "harper-ls"
args = ["--stdio"]

[index]
# The vault index: .shale/shale.db, a cache of notes, tags, links and word counts.
# Deleting that file is always safe — the notes are the source of truth and Shale
# rebuilds the index from them. Off leaves it unwritten, and the features that read
# it say so rather than guessing.
enabled = true
"#
    }
}

/// Where the settings a running Shale opened with came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// `<vault>/.shale/config.toml` was already there.
    File,
    /// Written from [`Config::starter`] because the vault had no config at all.
    Created,
    /// Moved out of the vault root, where Shale's config used to sit.
    Moved,
}

/// The root-level `shale.toml`, for a vault that still has one.
fn legacy_path(vault_root: &Path) -> Option<PathBuf> {
    let path = vault_root.join("shale.toml");
    path.is_file().then_some(path)
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

    /// Autosave is on, and its pause is long enough to never land between two
    /// keystrokes.
    #[test]
    fn autosave_is_on_and_waits_for_a_pause() {
        let cfg = Config::from_toml("").unwrap();
        assert!(cfg.editor.autosave);
        assert_eq!(cfg.editor.autosave_after_ms, 2000);

        let cfg = Config::from_toml("[editor]\nautosave = false\n").unwrap();
        assert!(!cfg.editor.autosave);
        // Turning it off does not invent a delay, and a delay without a number
        // stays the default.
        assert_eq!(cfg.editor.autosave_after_ms, 2000);

        let cfg = Config::from_toml("[editor]\nautosave_after_ms = 500\n").unwrap();
        assert_eq!(cfg.editor.autosave_after_ms, 500);
        assert!(cfg.editor.autosave);
    }

    /// Focus mode and typewriter scroll are **not** config keys: they are state
    /// the user chose in a running window, which belongs with the session file
    /// (roadmap P1.8), not with the settings they edit by hand. A config that
    /// still names them is refused rather than quietly ignored, because a typo in
    /// a config is the same shape as a removed key and deserves the same message.
    #[test]
    fn focus_and_typewriter_are_not_config_keys() {
        for key in ["focus = ", "focus_scope = ", "typewriter = "] {
            assert!(
                !Config::starter().contains(key),
                "the starter does not offer {key}"
            );
        }
        for key in [
            "focus = true",
            "focus_scope = \"section\"",
            "typewriter = true",
        ] {
            let err = Config::from_toml(&format!("[editor]\n{key}\n")).unwrap_err();
            assert!(
                err.to_string().contains("unknown field"),
                "{key} is refused, not ignored: {err}"
            );
        }
    }

    #[test]
    fn a_theme_can_be_named() {
        let cfg = Config::from_toml("[appearance]\ntheme = \"gruvbox\"\nfont_size = 12.0").unwrap();
        assert_eq!(cfg.appearance.theme, "gruvbox");
        assert_eq!(cfg.appearance.font_size, 12.0);
    }

    /// Spellcheck is on by default and names the server it expects to find.
    #[test]
    fn spellcheck_defaults_to_harper_over_stdio() {
        let cfg = Config::from_toml("").unwrap();
        assert!(cfg.spellcheck.enabled);
        assert_eq!(cfg.spellcheck.command, "harper-ls");
        assert_eq!(cfg.spellcheck.args, vec!["--stdio"]);

        let cfg = Config::from_toml("[spellcheck]\nenabled = false\n").unwrap();
        assert!(!cfg.spellcheck.enabled);
        assert_eq!(cfg.spellcheck.command, "harper-ls");
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

[spellcheck]
command = "my-ls"
args = ["--stdio", "--quiet"]
"#;
        let cfg = Config::from_toml(toml).unwrap();
        assert!(!cfg.editor.vim_mode);
        assert_eq!(cfg.appearance.font_size, 16.5);
        assert_eq!(cfg.appearance.font_family, "Iosevka");
        assert_eq!(cfg.appearance.theme, "light");
        assert_eq!(cfg.spellcheck.command, "my-ls");
        assert_eq!(cfg.spellcheck.args, vec!["--stdio", "--quiet"]);
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

    /// Shale's own files live under `.shale`, not in the vault's root among the
    /// notes.
    #[test]
    fn the_config_is_under_the_shale_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Config::path(dir.path()),
            dir.path().join(".shale").join("config.toml")
        );
        assert!(
            Config::load(dir.path()).is_ok_and(|c| c == Config::default()),
            "and reading a vault with no config is not an error"
        );
    }

    #[test]
    fn opening_a_fresh_vault_writes_the_starter() {
        let dir = tempfile::tempdir().unwrap();
        let (cfg, source) = Config::load_or_create(dir.path()).unwrap();
        assert_eq!(source, ConfigSource::Created);
        assert_eq!(cfg, Config::default());
        assert!(Config::path(dir.path()).is_file());

        // A second opening reads the file rather than writing over it, which is
        // what lets a hand-edit (or a saved theme) survive a restart.
        std::fs::write(Config::path(dir.path()), "[appearance]\ntheme = \"nord\"\n").unwrap();
        let (cfg, source) = Config::load_or_create(dir.path()).unwrap();
        assert_eq!(source, ConfigSource::File);
        assert_eq!(cfg.appearance.theme, "nord");
    }

    /// A vault whose config is still the root-level `shale.toml` has that file
    /// **moved**, not copied and not quietly ignored: the settings follow the
    /// vault, and there is still exactly one place config can come from.
    #[test]
    fn an_old_root_config_is_moved_into_the_shale_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("shale.toml"),
            "[appearance]\ntheme = \"nord\"\n",
        )
        .unwrap();
        let (cfg, source) = Config::load_or_create(dir.path()).unwrap();
        assert_eq!(source, ConfigSource::Moved);
        assert_eq!(cfg.appearance.theme, "nord");
        assert!(
            !dir.path().join("shale.toml").exists(),
            "moved, not copied: nothing is left in the vault root"
        );
        assert!(Config::path(dir.path()).is_file());
    }
}
