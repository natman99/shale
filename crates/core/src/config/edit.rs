//! Patching one setting into a config file without rewriting the file.
//!
//! The config file is the user's. It carries the comments that explain every key
//! ([`Config::starter`](super::Config::starter)), and a user who has hand-edited
//! it — reordered it, added their own notes, deleted the settings they do not
//! care about — expects that work to survive. So a config write is a **splice**:
//! find the line that already sets the key, replace only its value, and leave
//! every other byte alone.
//!
//! When the key is not there, the line is *added*: inside its table if the table
//! exists, and as a new table at the end of the file if it does not. That is what
//! lets the config stay incremental — only what differs from the default needs to
//! be in it, and Shale can change a setting a starter file never mentioned.
//!
//! Deliberately out of scope, because Shale never writes these: dotted keys
//! (`appearance.theme = …` outside the table), array values, and a key set inside
//! a multi-line string.

use std::path::Path;

use anyhow::Context as _;

use super::Config;

/// A value Shale can write into the config file.
///
/// Only the shapes the config actually holds. An array (`args = [...]`) is never
/// written back: Shale only ever changes a scalar it chose itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A TOML basic string; `"` and `\` in it are escaped.
    Str(String),
    /// `true` or `false`.
    Bool(bool),
    /// A number, always written with a decimal point, so a font size of 14 does
    /// not become an integer the `f32` field would reject.
    Float(f64),
}

impl Value {
    /// The TOML text for this value.
    fn to_toml(&self) -> String {
        match self {
            Value::Str(s) => {
                let mut out = String::with_capacity(s.len() + 2);
                out.push('"');
                for c in s.chars() {
                    if c == '"' || c == '\\' {
                        out.push('\\');
                    }
                    out.push(c);
                }
                out.push('"');
                out
            }
            Value::Bool(b) => b.to_string(),
            Value::Float(f) => {
                let text = f.to_string();
                // `to_string` already carries a `.` for anything that needs one,
                // including the exponents; only a whole number needs the `.0`.
                if text.contains(['.', 'e', 'E']) || text.contains("inf") || text.contains("NaN") {
                    text
                } else {
                    format!("{text}.0")
                }
            }
        }
    }
}

/// Patch `key` inside `[table]` to `value`, leaving every other byte of `text` as
/// it was — including the file's line endings.
pub fn set(text: &str, table: &str, key: &str, value: &Value) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    // A file written with CRLF keeps its CRLF; a new line in such a file gets one
    // too, so a patch never leaves the file mixed.
    let cr = if text.contains("\r\n") { "\r" } else { "" };
    let value = value.to_toml();

    let Some(header) = lines.iter().position(|l| table_of(l) == Some(table)) else {
        // The table is not in the file at all: append it, separated from what is
        // already there by one blank line.
        let mut out = text.trim_end_matches(['\n', '\r']).to_string();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&format!("[{table}]{cr}\n{key} = {value}{cr}\n"));
        return out;
    };

    // The table's body runs from its header to the next header, or the file.
    let body_end = lines
        .iter()
        .enumerate()
        .skip(header + 1)
        .find(|(_, l)| table_of(l).is_some())
        .map(|(i, _)| i)
        .unwrap_or(lines.len());

    if let Some(i) = (header + 1..body_end).find(|i| key_of(&lines[*i]) == Some(key)) {
        let line = lines[i].trim_end_matches('\r').to_string();
        let indent = &line[..line.len() - line.trim_start().len()];
        let comment = match trailing_comment(&line) {
            Some(comment) => format!(" {comment}"),
            None => String::new(),
        };
        let cr = if lines[i].ends_with('\r') { "\r" } else { "" };
        lines[i] = format!("{indent}{key} = {value}{comment}{cr}");
    } else {
        // Not set anywhere in the table: add it after the table's last real line,
        // so it joins the block instead of dangling below a blank line.
        let last = (header + 1..body_end)
            .rev()
            .find(|i| !lines[*i].trim().is_empty());
        let at = last.map(|i| i + 1).unwrap_or(header + 1);
        lines.insert(at, format!("{key} = {value}{cr}"));
    }
    lines.join("\n")
}

/// Patch one setting in `<vault>/.shale/config.toml`.
///
/// The file is read, patched with [`set`], and written back atomically, so an
/// interrupted write cannot leave a vault with a half-written config. A vault
/// with no config file gets the one setting written into a fresh
/// [`Config::starter`], which is what a vault opened through
/// [`Config::load_or_create`] would have had anyway.
pub fn write_setting(
    vault_root: &Path,
    table: &str,
    key: &str,
    value: &Value,
) -> anyhow::Result<()> {
    let path = Config::path(vault_root);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Config::starter().to_string(),
        Err(err) => {
            return Err(
                anyhow::Error::new(err).context(format!("reading config {}", path.display()))
            )
        }
    };
    write_file(&path, &set(&text, table, key, value))
}

/// Write `text` to `path` atomically: a temp file in the same directory, then a
/// rename onto the target.
fn write_file(path: &Path, text: &str) -> anyhow::Result<()> {
    let dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} to {}", tmp.display(), path.display()))
}

/// The table a header line names (`[appearance]`, with a trailing comment
/// allowed), or `None` for a line that is not a header.
fn table_of(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix('[')?;
    let end = rest.find(']')?;
    Some(rest[..end].trim())
}

/// The key a line sets: the text before its first `=`. A commented-out line is
/// not a setting, so `# theme = "x"` answers `None` and stays a comment while
/// Shale adds the real line of its own.
fn key_of(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if line.starts_with('#') {
        return None;
    }
    let (key, _) = line.split_once('=')?;
    Some(key.trim())
}

/// The comment that ends a line — a `#` outside any quoted string — so rewriting
/// a value keeps the note written beside it.
fn trailing_comment(line: &str) -> Option<&str> {
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return Some(&line[i..]),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config the way a user would leave it: comments, blank lines, a note
    /// beside a value, a commented-out key, and a second table.
    const HAND_EDITED: &str = "# My vault. Do not touch.\n\n[appearance]\n# The palette I look at all day.\ntheme = \"mocha\" # warm\nfont_size = 14.0\n\n# theme = \"dark\"\n\n[editor]\nvim_mode = true\n";

    #[test]
    fn a_value_is_replaced_and_nothing_else_moves() {
        let out = set(
            HAND_EDITED,
            "appearance",
            "theme",
            &Value::Str("gruvbox-dark".into()),
        );
        assert_eq!(
            out,
            "# My vault. Do not touch.\n\n[appearance]\n# The palette I look at all day.\ntheme = \"gruvbox-dark\" # warm\nfont_size = 14.0\n\n# theme = \"dark\"\n\n[editor]\nvim_mode = true\n"
        );
        // The commented-out `theme` is not a setting, and was not touched or
        // mistaken for one; the file still parses, and says what it now means.
        let cfg = Config::from_toml(&out).unwrap();
        assert_eq!(cfg.appearance.theme, "gruvbox-dark");
        assert!(cfg.editor.vim_mode);
    }

    #[test]
    fn a_key_the_starter_never_mentioned_is_added_to_its_table() {
        let text = "[appearance]\ntheme = \"mocha\"\n\n[editor]\nvim_mode = true\n";
        let out = set(text, "appearance", "font_size", &Value::Float(16.5));
        assert_eq!(
            out, "[appearance]\ntheme = \"mocha\"\nfont_size = 16.5\n\n[editor]\nvim_mode = true\n",
            "inside the table, not at the end of the file"
        );
        assert_eq!(Config::from_toml(&out).unwrap().appearance.font_size, 16.5);
    }

    #[test]
    fn a_table_that_is_not_there_is_appended() {
        let text = "[appearance]\ntheme = \"mocha\"\n";
        let out = set(text, "spellcheck", "enabled", &Value::Bool(false));
        assert_eq!(
            out,
            "[appearance]\ntheme = \"mocha\"\n\n[spellcheck]\nenabled = false\n"
        );
        assert!(!Config::from_toml(&out).unwrap().spellcheck.enabled);
    }

    #[test]
    fn a_key_in_another_table_is_left_alone() {
        let text = "[appearance]\nname = \"note\"\n\n[editor]\nname = \"idea\"\n";
        let out = set(text, "editor", "name", &Value::Str("draft".into()));
        assert!(out.contains("[appearance]\nname = \"note\""));
        assert!(out.contains("[editor]\nname = \"draft\""));
    }

    #[test]
    fn an_empty_table_and_an_empty_file_both_work() {
        let out = set("[editor]\n", "editor", "vim_mode", &Value::Bool(false));
        assert_eq!(out, "[editor]\nvim_mode = false\n");
        let out = set("", "editor", "vim_mode", &Value::Bool(false));
        assert_eq!(out, "[editor]\nvim_mode = false\n");
        // A file with nothing but a comment still gets its table.
        let out = set("# nothing yet", "editor", "vim_mode", &Value::Bool(false));
        assert_eq!(out, "# nothing yet\n\n[editor]\nvim_mode = false\n");
    }

    #[test]
    fn values_are_written_the_way_toml_wants_them() {
        let quote = set("", "appearance", "theme", &Value::Str("a\"b\\c".into()));
        assert_eq!(quote, "[appearance]\ntheme = \"a\\\"b\\\\c\"\n");
        assert_eq!(
            Config::from_toml(&quote).unwrap().appearance.theme,
            "a\"b\\c"
        );
        // A whole float keeps a decimal point, so an f32 field accepts it.
        assert_eq!(
            set("", "appearance", "font_size", &Value::Float(14.0)),
            "[appearance]\nfont_size = 14.0\n"
        );
    }

    #[test]
    fn a_crlf_file_stays_a_crlf_file() {
        let text = "[appearance]\r\ntheme = \"mocha\"\r\n";
        let out = set(text, "appearance", "theme", &Value::Str("nord".into()));
        assert_eq!(out, "[appearance]\r\ntheme = \"nord\"\r\n");
        let out = set(
            text,
            "appearance",
            "font_family",
            &Value::Str("Iosevka".into()),
        );
        assert_eq!(
            out,
            "[appearance]\r\ntheme = \"mocha\"\r\nfont_family = \"Iosevka\"\r\n"
        );
    }

    #[test]
    fn writing_a_setting_creates_the_file_from_the_starter() {
        let dir = tempfile::tempdir().unwrap();
        write_setting(
            dir.path(),
            "appearance",
            "theme",
            &Value::Str("one-light".into()),
        )
        .unwrap();
        let cfg = Config::load(dir.path()).unwrap();
        assert_eq!(cfg.appearance.theme, "one-light");
        // The rest of the starter is intact, comments and all, and the temp file
        // the atomic write used is gone.
        let text = std::fs::read_to_string(Config::path(dir.path())).unwrap();
        assert!(text.contains("# The palette: a built-in"));
        assert_eq!(text.matches("theme = ").count(), 1);
        assert!(!Config::shale_dir(dir.path())
            .join("config.toml.tmp")
            .exists());
    }

    /// The starter is a snippet, not a dump: it says exactly what the defaults
    /// are, so deleting any of it changes nothing.
    #[test]
    fn the_starter_is_the_defaults() {
        let cfg = Config::from_toml(Config::starter()).unwrap();
        assert_eq!(cfg, Config::default());
    }
}
