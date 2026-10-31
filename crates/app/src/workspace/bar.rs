//! The status bar's own pieces: the words and marks the bar draws, and how long
//! a message lives in it.
//!
//! These are free functions rather than methods because they are pure — a
//! segment's text follows from what the window already holds, and the frame
//! ([`super::render`]) and the test hooks both call the same one, so what a test
//! asserts is what is drawn. They moved out of `mod.rs` when the window itself
//! outgrew the file-size rule; the split is the one the size notes planned.

use std::time::Duration;

/// How long a message stays in the status bar. Long enough to read a sentence
/// about what a command just did — including an error, which is the one message
/// the user may have to act on — and short enough that the bar does not go on
/// reporting something that stopped being news. `pub(crate)` so the tests can
/// wait it out rather than hard-coding a number.
pub(crate) const STATUS_TTL: Duration = Duration::from_secs(8);

/// The status bar's save indicator: a note that is ahead of the disk. Empty when
/// the note is written, so the mark appearing and disappearing is the whole
/// message — and it is drawn in the palette's accent rather than in the text
/// colour, because it is the one thing in the bar the user has to notice without
/// looking for it.
pub(crate) fn save_indicator(dirty: bool) -> &'static str {
    if dirty {
        "●"
    } else {
        ""
    }
}

/// The mode as the status bar shows it.
pub(crate) fn mode_label(mode: shale_core::vim::Mode) -> &'static str {
    match mode {
        shale_core::vim::Mode::Normal => "NORMAL",
        shale_core::vim::Mode::Insert => "INSERT",
        shale_core::vim::Mode::Visual => "VISUAL",
        shale_core::vim::Mode::VisualLine => "V-LINE",
    }
}

/// The status bar's word-count badge for the note on screen, or empty when there is
/// no note to count — an empty segment is still a flex item, so it would put a gap
/// on both sides of nothing.
///
/// The count comes from the **buffer**, not the index: it is the one number in the
/// bar the user watches change as they type, so a count that lagged a save behind
/// would be exactly the stale state the bar's segments are arranged to avoid.
/// `:wordcount` answers the other question — the vault, or one folder of it — from
/// the index, and both go through [`shale_core::word_count`], so they cannot
/// disagree about a note that is saved.
pub(crate) fn word_badge(has_note: bool, words: usize) -> String {
    if !has_note {
        return String::new();
    }
    if words == 1 {
        "1 word".to_string()
    } else {
        format!("{words} words")
    }
}
