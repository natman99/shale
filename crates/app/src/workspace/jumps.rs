//! The jump chain: the notes the user came from, and where in them they were.
//!
//! vim and neovim both promise `ctrl-o`/`ctrl-i` walk the chain of places the user
//! has been (the Goals promise). It lives here as workspace state rather than as a
//! stack inside the open-note path because the same record is what the graph view,
//! the bookmarks panel and hover history will read later.
//!
//! What counts as a jump today is **leaving one note for another**: following a
//! wiki-link, choosing a quick-open or vault-search row, opening a row in the
//! tree, or `:e`. The position remembered is the cursor's byte offset in the note
//! being left, so coming back lands the user on the line they were reading rather
//! than at the top of the note. Jumps *within* one note (`gg`, `G`, a `/` search
//! that moves a long way) are the follow-up recorded in PROJECT.md: same chain, a
//! different unit, and the note would not have to be re-read at all.

use std::path::{Path, PathBuf};

use gpui::{Context, Window};

use crate::acts::{JumpBack, JumpForward};

use super::Workspace;

/// One place in the chain.
pub(crate) struct Jump {
    /// The note the user was in.
    pub(crate) path: PathBuf,
    /// Where the cursor was in it, as a buffer byte offset.
    pub(crate) cursor: usize,
}

/// How far the chain reaches in each direction. vim's jumplist holds 100; the same
/// cap keeps a long session bounded without ever cutting off a distance the user
/// could step through by hand.
const LIMIT: usize = 100;

/// The chain, as the two halves around where the user stands — the same shape a
/// browser's back/forward pair uses, which is what makes `ctrl-i` mean "undo my
/// `ctrl-o`" rather than "the next note I happened to visit".
#[derive(Default)]
pub(crate) struct Jumps {
    /// The notes behind the user, oldest first. `ctrl-o` takes from the end.
    back: Vec<Jump>,
    /// The notes ahead of the user, most recently stepped-back-over last, so
    /// `ctrl-i` takes from the end and returns along the same path.
    forward: Vec<Jump>,
}

impl Jumps {
    /// Record leaving a note. The forward half is dropped: choosing somewhere new
    /// makes the jumps the user stepped back over stale, exactly as a browser drops
    /// its forward history.
    pub(crate) fn record(&mut self, from: Jump) {
        self.forward.clear();
        self.back.push(from);
        if self.back.len() > LIMIT {
            self.back.remove(0);
        }
    }

    /// Step along the chain, handing it the place being left so it lands on the
    /// other side. `leaving` is `None` when there is no note on screen to leave —
    /// which is the state right after the open note is deleted, and the moment
    /// `ctrl-o` is most wanted.
    ///
    /// `None` when there is nowhere to go that way, which is what the status line
    /// reports.
    pub(crate) fn step(&mut self, forward: bool, leaving: Option<Jump>) -> Option<Jump> {
        let (from, to) = if forward {
            (&mut self.forward, &mut self.back)
        } else {
            (&mut self.back, &mut self.forward)
        };
        let jump = from.pop()?;
        if let Some(leaving) = leaving {
            to.push(leaving);
            if to.len() > LIMIT {
                to.remove(0);
            }
        }
        Some(jump)
    }

    /// Drop every entry for a note that no longer exists, so `ctrl-o` cannot walk
    /// into a note the user deleted.
    pub(crate) fn forget(&mut self, path: &Path) {
        self.back.retain(|jump| jump.path != path);
        self.forward.retain(|jump| jump.path != path);
    }

    /// Re-point every entry for a note that was renamed. Without this a rename
    /// leaves the chain pointing at a path that no longer resolves.
    pub(crate) fn rename(&mut self, from: &Path, to: &Path) {
        for jump in self.back.iter_mut().chain(self.forward.iter_mut()) {
            if jump.path == from {
                jump.path = to.to_path_buf();
            }
        }
    }

    /// How far the user can still go each way: `(back, forward)`. Only the tests
    /// ask, through [`Workspace::jump_lens`].
    #[cfg(test)]
    pub(crate) fn lens(&self) -> (usize, usize) {
        (self.back.len(), self.forward.len())
    }
}

impl Workspace {
    /// `ctrl-o`: back to the note the user came from, cursor where they left it.
    pub(super) fn jump_back(&mut self, _: &JumpBack, _window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay_owns_keys() {
            return;
        }
        self.step_jump(false, cx);
    }

    /// `ctrl-i`: forward again along the same path.
    pub(super) fn jump_forward(
        &mut self,
        _: &JumpForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        self.step_jump(true, cx);
    }

    /// The note being left, and where the cursor is in it, onto the chain.
    /// Re-opening the note that is already open is not a jump — a chain of the same
    /// note over and over is noise the user would have to step through, and the
    /// disk-reload path calls this for the note that is already on screen.
    pub(super) fn record_jump(&mut self, going_to: &Path, cx: &mut Context<Self>) {
        let Some(current) = self.active.as_deref() else {
            return;
        };
        if current == going_to {
            return;
        }
        let cursor = self.editor.read(cx).cursor_offset();
        self.jumps.record(Jump {
            path: current.to_path_buf(),
            cursor,
        });
    }

    /// One step of the chain, in either direction.
    fn step_jump(&mut self, forward: bool, cx: &mut Context<Self>) {
        // There does not have to be a note on screen. Deleting the open note clears
        // the active tab, and `ctrl-o` is the press the user reaches for at that
        // moment, so the place being left is optional.
        let leaving = self.active.clone().map(|path| Jump {
            path,
            cursor: self.editor.read(cx).cursor_offset(),
        });
        let Some(jump) = self.jumps.step(forward, leaving) else {
            self.status = if forward {
                "no newer jump".into()
            } else {
                "no older jump".into()
            };
            cx.notify();
            return;
        };
        let path = jump.path.clone();
        // The entry has already moved to the other half, so a note that cannot be
        // read (deleted outside Shale, a path the chain outlived) costs the user one
        // press and one step, and the read error is on the status line.
        if self.load_note(&path, cx) {
            self.editor
                .update(cx, |editor, cx| editor.reveal_offset(jump.cursor, cx));
        }
        cx.notify();
    }

    /// The chain as the window holds it: how far back and how far forward. Only the
    /// tests ask.
    #[cfg(test)]
    pub(crate) fn jump_lens(&self) -> (usize, usize) {
        self.jumps.lens()
    }
}
