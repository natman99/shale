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
    /// `focus on|1` / `focus off|0`, or bare `focus` to turn it around: dim
    /// everything but the block the cursor is in. `dim` is the same command under
    /// the name the feature is usually called.
    Focus(Option<bool>),
    /// `focus block` / `focus section`: how far the lit region reaches. The mode
    /// itself is left alone, because asking how wide the light should be is not a
    /// request to turn it on.
    FocusScope(String),
    /// `typewriter on|1` / `typewriter off|0`, or bare `typewriter` to turn it
    /// around: keep the cursor's line near the middle of the window.
    Typewriter(Option<bool>),
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
    /// `wordcount` / `wc [folder]`: how many words the vault — or one folder of it,
    /// as typed — holds. The count comes from the vault index, so it is the index's
    /// answer rather than a fresh read of every note.
    WordCount(String),
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
        // Both are a preference the user moves in and out of while writing, so a
        // bare word turns it around rather than asking which half was meant — the
        // opposite of `harper`, which starts and stops a process and would be
        // annoying to do by accident. A word that is neither yes nor no is still a
        // typo, and says which.
        "focus" | "dim" => match bool_word(rest) {
            Some(on) => Ex::Focus(Some(on)),
            None if rest.is_empty() => Ex::Focus(None),
            // A scope word is the other thing `:focus` can be asked for. Spelled
            // here rather than through `dim::Scope::from_word` so a typo stays a
            // typo: `:focus blockk` runs nothing, while a scope that exists but is
            // not implemented yet (`sentence`) is accepted and answered with the
            // default.
            None if matches!(
                rest.to_ascii_lowercase().as_str(),
                "block" | "section" | "sentence"
            ) =>
            {
                Ex::FocusScope(rest.to_ascii_lowercase())
            }
            None => Ex::Unknown(format!("{word} {rest}")),
        },
        "typewriter" => match bool_word(rest) {
            Some(on) => Ex::Typewriter(Some(on)),
            None if rest.is_empty() => Ex::Typewriter(None),
            None => Ex::Unknown(format!("{word} {rest}")),
        },
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
        // The folder is whatever follows the word, so `:wordcount notes/daily` is
        // one argument; an unknown word after it is not guessed at.
        "wordcount" | "wc" => Ex::WordCount(rest.to_string()),
        _ => Ex::Unknown(word),
    }
}

/// The yes and no a command line's second word spells, or `None` for anything
/// else — including nothing at all, which is how a bare command says "turn it
/// around".
fn bool_word(rest: &str) -> Option<bool> {
    match rest.to_ascii_lowercase().as_str() {
        "on" | "1" | "true" | "yes" => Some(true),
        "off" | "0" | "false" | "no" => Some(false),
        _ => None,
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
    fn focus_and_typewriter_take_yes_and_no_and_a_bare_word_toggles() {
        for on in ["focus on", "focus 1", "dim ON", "typewriter yes"] {
            let want = if on.starts_with("typewriter") {
                Ex::Typewriter(Some(true))
            } else {
                Ex::Focus(Some(true))
            };
            assert_eq!(parse(on), want, "{on}");
        }
        for off in ["focus off", "focus 0", "dim Off", "typewriter no"] {
            let want = if off.starts_with("typewriter") {
                Ex::Typewriter(Some(false))
            } else {
                Ex::Focus(Some(false))
            };
            assert_eq!(parse(off), want, "{off}");
        }
        // A bare word is a toggle, not a guess.
        assert_eq!(parse("focus"), Ex::Focus(None));
        assert_eq!(parse("dim"), Ex::Focus(None));
        assert_eq!(parse("typewriter"), Ex::Typewriter(None));
        // A scope word is the other thing `:focus` answers, and it is not a
        // toggle: asking how wide the light is does not turn it on.
        assert_eq!(parse("focus block"), Ex::FocusScope("block".into()));
        assert_eq!(parse("dim SECTION"), Ex::FocusScope("section".into()));
        // Named on the roadmap, not built yet: accepted, and the window answers
        // with the default rather than pretending otherwise.
        assert_eq!(parse("focus sentence"), Ex::FocusScope("sentence".into()));
        // A word that is neither yes nor no nor a scope is a typo, and says which.
        assert_eq!(
            parse("focus sideways"),
            Ex::Unknown("focus sideways".into())
        );
    }

    #[test]
    fn an_unknown_word_is_named_rather_than_guessed() {
        assert_eq!(parse("xyzzy"), Ex::Unknown("xyzzy".into()));
        assert_eq!(parse(""), Ex::Empty);
        assert_eq!(parse("   "), Ex::Empty);
    }
}
