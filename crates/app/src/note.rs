//! One note, one buffer, per window (roadmap P4.17).
//!
//! A note used to be copied into every tab that showed it. That was invisible
//! while a note could only appear once, and wrong the moment it could appear
//! twice: two copies of one file's text drift apart, and whichever tab saves
//! last quietly throws away the other's edits. P4.0b makes the case ordinary —
//! a note open in a tab *and* embedded in the note beside it — so the window
//! now keeps exactly one buffer per note and every view of it borrows that one.
//!
//! **The registry holds `Weak` handles, not owned ones.** A note is alive while
//! some view holds it: a tab, or (from P4.0b) an embed. When the last view lets
//! go, the buffer goes with it, and there is no release bookkeeping to forget —
//! no "did every close path remember to drop the entry?" question, which in
//! this repo has historically been the question. A stale `Weak` is simply
//! skipped, and the next open re-reads the file.
//!
//! **What is shared, and what is not.** The text, the caret and the undo
//! history are the note's: one file has one undo stack, and `u` in one view
//! undoes the same edit the other view would have undone. What stays per-view is
//! in [`shale_core::buffer::ViewSlot`] — the wrap points and the folds — and on
//! the tab: which note it is, its scroll position, its view mode. The caret is
//! the one thing per-view here that it arguably shouldn't be (two tabs of one
//! note share a caret today); lifting it into the slot is a deliberate later
//! move, not an oversight — see the note in `transclusion-plan.md`.
//!
//! **`dirty` lives here, not on the editor.** It is a fact about the note's text
//! compared to the file, so two views of one note cannot disagree about it, and a
//! tab whose embedded note has unsaved edits can light its dot from the same
//! flag (P4.0b's transitive dot).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use shale_core::buffer::Buffer;

/// One note's shared state within one window.
pub(crate) struct Note {
    /// The text, caret and undo history. Shared by every view of the note.
    pub buffer: Rc<RefCell<Buffer>>,
    /// Whether the buffer differs from the file. `Cell` because it is a `bool`
    /// read from the tab bar, the status bar and the exit check while the editor
    /// that set it is still borrowed.
    pub dirty: Cell<bool>,
}

impl Note {
    /// A note read from disk, clean.
    pub fn from_text(text: String) -> Rc<Self> {
        Rc::new(Self {
            buffer: Rc::new(RefCell::new(Buffer::new(text))),
            dirty: Cell::new(false),
        })
    }

    /// A note that has no file yet: the blank tab's text, until `:w` names it.
    /// Not in the registry — there is no path to register it under.
    pub fn blank() -> Rc<Self> {
        Rc::new(Self {
            buffer: Rc::new(RefCell::new(Buffer::default())),
            dirty: Cell::new(false),
        })
    }
}
