//! Leaving the session: what an exit owes the disk (roadmap P1.5).
//!
//! Four ways out reach the same promise, and three of them are here: `quit`
//! (`ctrl-q`, `:q`, the palette's row), `quit_forced` (`:q!`) and `write_quit`
//! (`:wq`) act inside the window. The window manager's close button acts on the
//! window from outside, so `attach_window` keeps the same promise there. A
//! **termination signal** arrives on a thread of the kernel's choosing, which is why
//! it is handled here rather than in a signal handler: the handler does the only
//! thing a signal context allows — an atomic store — and the window polls that flag.
//! gpui's event loop is not ours to wake, so polling is where the two meet.
//!
//! What the promise is: the note the user can see reaches the disk, the session
//! file says where they were in it, the index is drained and checkpointed, and
//! then the process goes. SIGKILL is out of scope by definition — nothing runs —
//! which is why the note writes themselves are atomic
//! ([`shale_core::atomic`]) and the index is a cache rather than a record.
//!
//! One thing can stop an exit: `:q` on a dirty note with autosave off, which is
//! refused until the user saves or says `:q!`. An exit the user did not type — the
//! window manager, a signal — never refuses and never discards.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::{Context, Window};

use crate::acts::{Quit, QuitForce};

use super::Workspace;

/// How often the window looks to see whether the process was asked to stop. Long
/// enough to be invisible in a profile — one atomic load per poll — short enough
/// that a `kill` is answered before a person notices they asked.
const TERMINATION_POLL: Duration = Duration::from_millis(200);

impl Workspace {
    /// Start watching the flag a signal handler raises.
    ///
    /// `main` installs the handlers; a test drives the same watch with a flag of its
    /// own (`tests::exit`), which is how the polling and the exit promise get tested
    /// without anyone sending the test process a signal.
    pub(crate) fn watch_for_termination(&mut self, flag: Arc<AtomicBool>, cx: &mut Context<Self>) {
        self._termination = Some(cx.spawn(async move |ws, cx| {
            loop {
                cx.background_executor().timer(TERMINATION_POLL).await;
                if !flag.load(Ordering::SeqCst) {
                    continue;
                }
                // The weak handle: if the window is already gone, its own teardown
                // is under way and there is nothing to add.
                let _ = ws.update(cx, |ws, cx| ws.finish_termination(cx));
                break;
            }
        }));
    }

    /// The exit a signal asked for: the note on screen to the disk, the session
    /// written, the index checkpointed, then the window closed.
    ///
    /// A failed write does not stop the exit. `kill` means the process must go; the
    /// difference between this and dying where told is that it goes having tried, and
    /// the log says what could not be written.
    pub(crate) fn finish_termination(&mut self, cx: &mut Context<Self>) {
        if self.editor.read_with(cx, |editor, _cx| editor.is_dirty()) {
            self.write_note(cx);
        }
        self.leave(cx);
    }

    /// Quit, writing the open note first when it holds unsaved edits.
    ///
    /// Autosave usually has already written it, but a quit can come in the middle
    /// of the countdown, and work the user can see on screen must not be lost to a
    /// keystroke timing. `ctrl-q`, `:q` and the palette all come through here
    /// because the danger is the same. Closing the window from outside Shale — the
    /// window manager's button — does not reach this action, so `attach_window`
    /// makes the same promise there.
    ///
    /// **With autosave off, a dirty note is refused.** Autosave on means the disk is
    /// never more than a pause behind the screen, so writing on the way out is what
    /// the user meant. Autosave off means they chose to save by hand, and a quit
    /// that quietly writes — or quietly drops — a note they have not saved is a
    /// surprise either way. `:q!` is the way out they asked for, and `:wq` writes on
    /// purpose.
    pub(super) fn quit(&mut self, _: &Quit, _window: &mut Window, cx: &mut Context<Self>) {
        let dirty = self.editor.read_with(cx, |editor, _cx| editor.is_dirty());
        if dirty && !self.config.editor.autosave {
            self.say(
                "unsaved edits: `:w` to keep them, `:q!` to throw them away",
                cx,
            );
            cx.notify();
            return;
        }
        if dirty {
            self.write_note(cx);
            // The save said why it failed in the status bar. Quitting now would
            // throw the note away, so the user gets to see that first.
            let still = self.editor.read_with(cx, |editor, _cx| editor.is_dirty());
            if still {
                cx.notify();
                return;
            }
        }
        self.leave(cx);
    }

    /// `:q!`: quit and throw the open note's unsaved edits away. The bang is the
    /// user saying they know, so nothing is asked and nothing is written; the note
    /// on the disk stays as it was. The session still goes, because it is where the
    /// window stood, not what the note said.
    pub(super) fn quit_forced(
        &mut self,
        _: &QuitForce,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.read_with(cx, |editor, _cx| editor.is_dirty()) {
            // Logged rather than left unsaid: an exit that throws work away should
            // be findable in the log afterwards.
            tracing::warn!(
                path = self
                    .active
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                "quit discarded unsaved edits"
            );
        }
        self.leave(cx);
    }

    /// `:wq`: the write the user asked for, then the exit. Not one action — the
    /// write is unconditional and the exit depends on it having worked. If the note
    /// is still dirty the write failed, `write_note` already said why, and the
    /// window stays open with that message in it.
    pub(super) fn write_quit(&mut self, cx: &mut Context<Self>) {
        self.write_note(cx);
        if self.editor.read_with(cx, |editor, _cx| editor.is_dirty()) {
            cx.notify();
            return;
        }
        self.leave(cx);
    }

    /// The part of every exit that is not the note: the index drained and
    /// checkpointed, and the session written. Roadmap P1.5 is the first of those —
    /// an exit leaves one DB file rather than a WAL full of unapplied writes — and
    /// P1.8 the second, so the next window opens on this note, at this place, in
    /// this view.
    fn leave(&mut self, cx: &mut Context<Self>) {
        self.flush_index();
        self.save_session(cx);
        cx.quit();
    }
}
