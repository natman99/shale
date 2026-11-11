//! Giving a block an id (`:block-id`, roadmap P4.9 M4).
//!
//! This is the write half of the block scan. The scan decides which block the caret sits
//! in and where an id for it would land; this file turns that into one buffer edit. The
//! editor never guesses at placement: [`shale_core::mdstyle::with_block_id`] re-reads its
//! own candidate with the same scan and refuses one that would name a different block, so
//! the id the author is handed names the thing they were pointing at.

use gpui::Context;
use shale_core::mdstyle::{
    block_at, block_id, block_map, new_block_id, valid_block_id, with_block_id,
};

use super::MarkdownEditor;

/// What `:block-id` did — and the sentence the window owes for each case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BlockId {
    /// The id, as written into the note.
    Inserted(String),
    /// The block already carries one (EDGE-10): the command refuses to add a second, and
    /// says which one is already there.
    HasId(String),
    /// The caret is not inside anything an id can name — a blank line, a properties
    /// block, the gap between blocks.
    NoBlock,
    /// The typed id is not `[A-Za-z0-9-]+` (ruling 2).
    BadId(String),
    /// The id is in use — in the vault's index, or twice in this very buffer.
    Taken,
    /// The block is real and no spelling of an id would name it: a list item whose child
    /// is nested one space deep has no indent between them for an own-line id to sit at.
    NoRoom,
    /// Preview mode: the text is on screen, not in hand.
    ReadOnly,
}

impl MarkdownEditor {
    /// Give the block under the caret an id.
    ///
    /// `want` is the id the reader typed, or empty for one to generate. `taken` answers
    /// whether the vault already uses an id: the window composes it from the index, and
    /// this buffer's own anchors are checked here, because an id the reader typed a
    /// second ago is not in the index yet (EDGE-13 — the check says what it read, never
    /// invents).
    pub(crate) fn add_block_id(
        &mut self,
        want: &str,
        taken: &dyn Fn(&str) -> bool,
        cx: &mut Context<Self>,
    ) -> BlockId {
        if self.slot.is_read_only() {
            return BlockId::ReadOnly;
        }
        let text = self.buffer.borrow().text().to_string();
        let map = block_map(&text);
        let Some(block) = block_at(&map, self.buffer.borrow().cursor()) else {
            return BlockId::NoBlock;
        };
        if let Some(existing) = block_id(&map, block) {
            return BlockId::HasId(existing.to_string());
        }
        let in_buffer = |id: &str| map.anchors.iter().any(|anchor| anchor.id == id);
        let id = if want.is_empty() {
            match new_block_id(|candidate| taken(candidate) || in_buffer(candidate)) {
                Some(id) => id,
                None => return BlockId::Taken,
            }
        } else {
            if !valid_block_id(want) {
                return BlockId::BadId(want.to_string());
            }
            if taken(want) || in_buffer(want) {
                return BlockId::Taken;
            }
            want.to_string()
        };
        let Some(write) = with_block_id(&text, &id, block) else {
            return BlockId::NoRoom;
        };
        // One edit, so one `u` takes the id back — and the blank line a structured
        // block's id arrives with (EDGE-21).
        if !self.apply_edits(&[(write.at, write.at, write.insert)], cx) {
            return BlockId::NoBlock;
        }
        // Past the token, so the reader can keep typing on the id they were given.
        self.reveal_offset(write.caret, cx);
        BlockId::Inserted(id)
    }
}
