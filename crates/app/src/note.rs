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
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

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

/// The window's notes, keyed by path, held weakly (roadmap P4.17).
///
/// The window owns this and hands a clone to every view that needs to ask "is
/// this note already open, and what does it say right now?" — which is what an
/// embed needs in order to show a note as the window has it in hand rather than
/// as the disk has it (P4.0b ruling 12).
#[derive(Default)]
pub(crate) struct Registry {
    entries: RefCell<HashMap<PathBuf, Weak<Note>>>,
}

impl Registry {
    /// The open note for `path`, if a view is holding one.
    pub fn get(&self, path: &Path) -> Option<Rc<Note>> {
        self.entries.borrow().get(path).and_then(Weak::upgrade)
    }

    /// Every note still alive in this window, with the path it is registered under.
    ///
    /// Test-only on purpose: production code always has the path it opened the note
    /// with, and asking by path is exact. A checker that knows only a *name* needs
    /// this, and needs it to find the note whichever form of the path it was opened
    /// with — otherwise a check on a note opened by relative path reads back "no such
    /// note" and passes for the wrong reason.
    #[cfg(test)]
    pub(crate) fn held(&self) -> Vec<(PathBuf, Rc<Note>)> {
        self.entries
            .borrow()
            .iter()
            .filter_map(|(path, weak)| weak.upgrade().map(|note| (path.clone(), note)))
            .collect()
    }

    /// Register a note. Called once per path by `Workspace::note_entry`.
    pub fn insert(&self, path: &Path, note: &Rc<Note>) {
        // Entries for notes no view holds any more are dead weight: a closed tab
        // cannot upgrade them, and the next open re-reads the file. Sweeping on
        // insert keeps the map to the notes on screen without any close path
        // having to remember to unregister.
        let mut entries = self.entries.borrow_mut();
        if entries.len() > 32 {
            entries.retain(|_, living| living.strong_count() > 0);
        }
        entries.insert(path.to_path_buf(), Rc::downgrade(note));
    }

    /// Move an entry when its file moves (a rename).
    ///
    /// The buffer is the same note: its caret, its unsaved edits and its undo history
    /// come with it. Re-keying is what lets an embed that now spells the new name find
    /// *this* buffer instead of re-reading the file and showing a version the author
    /// has already edited past.
    pub(crate) fn rename(&self, from: &Path, to: &Path) {
        let mut entries = self.entries.borrow_mut();
        if let Some(living) = entries.remove(from) {
            entries.insert(to.to_path_buf(), living);
        }
    }

    /// A number that changes whenever the text of *any* open note changes.
    ///
    /// This is how an embed cache stays honest without polling and without a
    /// freshness policy (P4.0b ruling 16): `Buffer::change_id()` counts every
    /// edit, so a note edited in a tab — or in an embed block of itself — moves
    /// the number, and the next paint re-resolves. A handful of open notes,
    /// hashed once per frame.
    pub fn stamp(&self) -> u64 {
        let mut hasher = FnvHasher::default();
        // Sorted so the number does not depend on HashMap iteration order.
        let entries = self.entries.borrow();
        let mut ids: Vec<(&PathBuf, u64)> = entries
            .iter()
            .filter_map(|(path, weak)| {
                let note = weak.upgrade()?;
                // Read the number here rather than in the returned tuple: the borrow
                // guard would otherwise outlive `note`.
                let change = note.buffer.borrow().change_id();
                Some((path, change))
            })
            .collect();
        ids.sort();
        for (path, change) in ids {
            path.hash(&mut hasher);
            change.hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// A small non-cryptographic hasher; the stamp only has to change when its
/// inputs do.
#[derive(Default)]
struct FnvHasher(u64);

impl Hasher for FnvHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ *byte as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.write(&n.to_le_bytes());
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
