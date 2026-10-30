//! vim's command line: `:` and a word, matched exactly.
//!
//! The command palette fuzzy-matches what you type, which is right for a list you
//! look through and wrong for a command you spell. In a fuzzy list `:w` is whatever
//! sorts best for the letter `w` — and `enter` runs it. So the two have separate
//! entry points (`ctrl-shift-p` for the list, `:` for the line), and this module is
//! the line's whole vocabulary: a word that is not here does nothing, and says so.
//!
//! The names are vim's, because the reflex is vim's. `w` is a real save rather
//! than a no-op: it costs nothing when the note is already clean, and it is what
//! the user types when they do not trust the autosave to have run yet.

use crate::editor::View;

/// What a typed command line asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ex {
    /// Nothing was typed: `:` then `enter` simply closes.
    Empty,
    /// `w` / `write`: write the open note.
    Write,
    /// `q` / `quit`: close Shale, writing the note first if it is unsaved.
    Quit,
    /// `wq` / `x`: write, then close.
    WriteQuit,
    /// `e` / `open`: the quick-open picker.
    Open,
    /// `harper on|1` / `harper off|0`: the spell and grammar checker.
    Checker(bool),
    /// `theme` with nothing after it: the theme picker.
    ThemePicker,
    /// `theme <name>`: that palette, now.
    Theme(String),
    /// `raw`, `live` or `preview`: how the note is presented, and whether it can
    /// be edited. `:live` is the way back to Shale's default view.
    View(View),
    /// `y` / `yank` / `clip`: the selection, or the last yank, out to the system
    /// clipboard. vim's `:yank` copies the current line into its own register;
    /// Shale's command line has no addresses yet, and the copy a user reaches for
    /// here is the one other programs can see.
    YankClipboard,
    /// `p` / `paste`: the system clipboard in at the caret, and into the vim
    /// register, so `p` and `.` can carry the same text on.
    PasteClipboard,
    /// Anything else, with the word that was typed. Nothing runs: an unknown word
    /// is a typo, not something to guess at.
    Unknown(String),
}

/// Read one command line. Case-insensitive, like vim's command abbreviations.
pub(crate) fn parse(line: &str) -> Ex {
    let line = line.trim();
    if line.is_empty() {
        return Ex::Empty;
    }
    let (word, rest) = match line.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (line, ""),
    };
    let word = word.to_ascii_lowercase();
    match word.as_str() {
        "w" | "write" => Ex::Write,
        "q" | "quit" => Ex::Quit,
        "wq" | "x" | "xit" | "exit" => Ex::WriteQuit,
        "e" | "edit" | "open" => Ex::Open,
        "harper" | "spellcheck" | "spell" => match rest.to_ascii_lowercase().as_str() {
            "on" | "1" | "true" | "yes" => Ex::Checker(true),
            "off" | "0" | "false" | "no" => Ex::Checker(false),
            _ => Ex::Unknown(format!("harper {rest}").trim().to_string()),
        },
        "theme" => {
            if rest.is_empty() {
                Ex::ThemePicker
            } else {
                Ex::Theme(rest.to_string())
            }
        }
        // The three views. `source` and `src` are there because that is the word
        // other markdown editors use for the same view, and `view` because the
        // user reaching for a view is asking for the one they can read in.
        "raw" | "source" | "src" => Ex::View(View::Raw),
        "live" => Ex::View(View::Live),
        "preview" | "view" => Ex::View(View::Preview),
        // The clipboard, both directions. `y` and `p` are the letters the reflex
        // already knows; `clip` and `paste` spell it out for the rest.
        "y" | "yank" | "clip" | "copy" => Ex::YankClipboard,
        "p" | "paste" => Ex::PasteClipboard,
        _ => Ex::Unknown(word),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vim_aliases_are_the_short_ones() {
        // The point of these tests: `:w` is `write`, and never whatever a fuzzy
        // list happens to rank first for the letter `w`.
        assert_eq!(parse("w"), Ex::Write);
        assert_eq!(parse("write"), Ex::Write);
        assert_eq!(parse("q"), Ex::Quit);
        assert_eq!(parse("quit"), Ex::Quit);
        assert_eq!(parse("wq"), Ex::WriteQuit);
        assert_eq!(parse("x"), Ex::WriteQuit);
        assert_eq!(parse("e"), Ex::Open);
        assert_eq!(parse("open"), Ex::Open);
    }

    #[test]
    fn the_checker_takes_both_spellings_of_yes_and_no() {
        for on in ["harper on", "harper 1", "harper ON", "spellcheck yes"] {
            assert_eq!(parse(on), Ex::Checker(true), "{on}");
        }
        for off in ["harper off", "harper 0", "harper Off", "spellcheck no"] {
            assert_eq!(parse(off), Ex::Checker(false), "{off}");
        }
        // `harper` on its own is not a guess about which half was meant.
        assert_eq!(parse("harper"), Ex::Unknown("harper".into()));
    }

    #[test]
    fn theme_names_a_palette_and_bare_theme_asks_for_the_picker() {
        assert_eq!(parse("theme"), Ex::ThemePicker);
        assert_eq!(
            parse("theme   gruvbox-dark  "),
            Ex::Theme("gruvbox-dark".into())
        );
    }

    #[test]
    fn the_three_views_are_named_not_guessed() {
        for raw in ["raw", "source", "src", "RAW"] {
            assert_eq!(parse(raw), Ex::View(View::Raw), "{raw}");
        }
        assert_eq!(parse("live"), Ex::View(View::Live));
        for preview in ["preview", "view", "Preview"] {
            assert_eq!(parse(preview), Ex::View(View::Preview), "{preview}");
        }
    }

    #[test]
    fn the_clipboard_goes_both_ways() {
        for yank in ["y", "yank", "clip", "copy", "YANK"] {
            assert_eq!(parse(yank), Ex::YankClipboard, "{yank}");
        }
        for paste in ["p", "paste", "Paste"] {
            assert_eq!(parse(paste), Ex::PasteClipboard, "{paste}");
        }
    }

    #[test]
    fn an_unknown_word_is_named_rather_than_guessed() {
        assert_eq!(parse("xyzzy"), Ex::Unknown("xyzzy".into()));
        assert_eq!(parse(""), Ex::Empty);
        assert_eq!(parse("   "), Ex::Empty);
    }
}
