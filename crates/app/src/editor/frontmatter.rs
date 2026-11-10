//! The one command that writes frontmatter: put a properties block at the top of
//! the note (the palette's `note.add-properties`).
//!
//! Two things this is careful about, both of which are the whole feature:
//!
//! - **It is one undo step.** The block arrives through the same grouped-edit path
//!   a checker's fix uses, so one `u` takes the whole thing away. A command that
//!   wrote the file behind the buffer would leave an undo history pointing at text
//!   that no longer exists.
//! - **It never writes a second block.** A note with two frontmatter blocks at the
//!   top is a note whose properties are a coin flip, and the parser reads the first
//!   one. So the command moves the caret into the block that is already there and
//!   says so, rather than stacking another.

use gpui::Context;
use shale_core::frontmatter;

use super::MarkdownEditor;

/// What the command did, so the window can say the right thing about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Frontmatter {
    /// A block was written and the caret is inside it.
    Added,
    /// The note already had one; the caret moved into it.
    Already,
    /// The view cannot be written — preview mode. Said rather than reported as a
    /// success, because the buffer would have refused the edit in silence.
    ReadOnly,
    /// There is no note to write (an empty pane).
    NoNote,
}

/// The body of a new frontmatter block: the fences, and one key left empty.
///
/// `aliases` earns its place. It is the one property every note can use, the one the
/// Properties UI puts in front of people, and in Shale it is load-bearing rather than
/// decorative: a note answers to its aliases as a wiki-link target, so this field is
/// the difference between `[[Goblin]]` resolving and `[[the green one]]` not. It is
/// left **empty** because the command is "start the properties", not "here are some
/// properties" — and an empty value is not an alias, it is the shape of one waiting
/// for a name (`Db::aliases` skips empty values, so an unfilled field cannot make an
/// empty link name resolve).
const EMPTY_BLOCK: &str = "---\naliases:\n---\n";

/// The key a new block opens with, spelled as it is written.
const FIRST_KEY: &str = "aliases:";

/// Where the caret lands in a block that was just written: after the key's colon, so
/// the first thing typed is the value rather than a jump past a word.
fn first_key_offset(block_start: usize) -> usize {
    block_start + frontmatter::FENCE.len() + 1 + FIRST_KEY.len()
}

/// Where the caret lands in a block that was already there: the start of the body,
/// which is the first key's line. Not after `FIRST_KEY` — an existing block's first
/// line is whatever its author wrote, and counting past it would land the caret in
/// the closing fence.
fn body_offset(block_start: usize) -> usize {
    // Past the opening fence and its newline.
    block_start + frontmatter::FENCE.len() + 1
}

impl MarkdownEditor {
    /// Add a frontmatter block to the top of this note, or step into the one it
    /// already has. See [`Frontmatter`] for the outcomes.
    pub(crate) fn ensure_frontmatter(&mut self, cx: &mut Context<Self>) -> Frontmatter {
        if self.slot.is_read_only() {
            return Frontmatter::ReadOnly;
        }

        let text = self.buffer.borrow().text().to_string();
        if let Some(block) = frontmatter::block(&text) {
            self.reveal_offset(body_offset(block.bytes.0), cx);
            return Frontmatter::Already;
        }

        // One insertion, so one `u` takes the whole block away. Split across two
        // edits and the undo leaves a fence behind — which is what the test
        // `one_undo_takes_the_whole_block_back` proves by splitting it on purpose.
        if !self.apply_edits(&[(0, 0, EMPTY_BLOCK.to_string())], cx) {
            return Frontmatter::NoNote;
        }
        self.reveal_offset(first_key_offset(0), cx);
        Frontmatter::Added
    }
}
