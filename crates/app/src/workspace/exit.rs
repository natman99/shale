//! Leaving the session: what an exit owes the disk (roadmap P1.5).
//!
//! Four ways out reach the same promise, and three of them are here: `quit`
//! (`ctrl-q`, `:q`, the palette's row), `quit_forced` (`:q!`) and `write_quit`
//! (`:wq`) act inside the window. The window manager's close button acts on the
//! window from outside, so `attach_window` keeps the same promise there. A
//! **termination signal** arrives on a thread of the kernel's choosing, which is why
//! it is handled here rather than in a signal handler: the handler does only what a
//! signal context allows — an atomic store and a one-byte write to a self-pipe — and a
//! blocking reader thread turns that byte into a send on a channel. One watcher task
//! per process waits on that channel and runs the exit for every window in the
//! registry, the same walk `:qall` takes. gpui's event loop is not ours to wake from
//! signal context, but a channel send from an ordinary thread is exactly what wakes a
//! gpui task, so no window ever asks "yet?" on a timer and an idle run — however many
//! windows it holds — pays nothing for being killable.
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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{App, BorrowAppContext, Context, Window};

use crate::acts::{CloseTab, CloseTabForce, Quit, QuitForce, WriteAll};

use super::Workspace;
use super::vaults::{self, VaultWindows};

/// The meeting point between the signal handlers and the process's one
/// termination watcher (see [`spawn_watcher`]).
///
/// Cloned freely — the flag and the channel sender are shared, and every clone
/// wakes the same watcher. `main` builds one, hands clones to the signal
/// handlers and the self-pipe reader, and gives the watcher half to
/// [`spawn_watcher`] once per run.
#[derive(Clone)]
pub(crate) struct Termination {
    /// The fact of the signal. Raised from signal context (where only an atomic
    /// store is allowed) and again by the pipe reader; the watcher reads this,
    /// never the channel's payload.
    flag: Arc<AtomicBool>,
    /// The one wake-up message. A channel, not a timer: an idle run pays
    /// nothing for being killable.
    tx: UnboundedSender<()>,
}

/// The watcher half of a [`Termination`]: the channel's only receiver, and the
/// flag. Handed to [`spawn_watcher`] once per run.
pub(crate) struct TerminationWatcher {
    flag: Arc<AtomicBool>,
    rx: UnboundedReceiver<()>,
}

impl Termination {
    /// The pair: the clonable wake-up half and the single watcher half.
    pub(crate) fn new() -> (Termination, TerminationWatcher) {
        let (tx, rx) = unbounded();
        let flag = Arc::new(AtomicBool::new(false));
        (
            Termination {
                flag: Arc::clone(&flag),
                tx,
            },
            TerminationWatcher { flag, rx },
        )
    }

    /// The flag, for the signal handler to raise. A signal handler may not send
    /// on a channel — it runs in signal context — so it stores this and writes
    /// one byte to the self-pipe; [`Termination::wake`] is what the pipe's
    /// reader thread calls when that byte lands.
    ///
    /// Only the unix handlers ask for it, so that is where it is built; a Windows
    /// raiser is a console handler that has not been written yet, and a method
    /// nothing on that side calls is a warning, not a feature.
    #[cfg(unix)]
    pub(crate) fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.flag)
    }

    /// Raise the flag and wake the watcher. Also what a test calls in place of
    /// the signal plumbing.
    pub(crate) fn wake(&self) {
        self.flag.store(true, Ordering::SeqCst);
        // If the watcher is not up yet, the flag still stands: the watcher
        // reads it before it ever sleeps.
        let _ = self.tx.unbounded_send(());
    }
}

/// Wait once for the termination signal, then exit every window this process
/// holds — the same registry walk `:qall` takes, so a `kill` answers for every
/// window, not just the one that started the run.
///
/// Spawned once per run by `main`, after the registry is a global. A test
/// drives the same wait by calling [`Termination::wake`] itself
/// (`tests::exit`), which is how the waiting and the exit promise get tested
/// without anyone sending the test process a signal.
pub(crate) fn spawn_watcher(watch: TerminationWatcher, cx: &mut App) {
    let task = cx.spawn(async move |cx| {
        let mut watch = watch;
        loop {
            // The flag is the fact, and it is read before every wait: a signal
            // that landed before the watcher started still answers.
            if watch.flag.load(Ordering::SeqCst) {
                break;
            }
            // The sender lives as long as the run, so `Err` here means the
            // wake-up is gone with nobody having asked to stop.
            if watch.rx.recv().await.is_err() {
                return;
            }
        }
        cx.update(|cx| {
            for handle in VaultWindows::all_windows(cx) {
                // A window already gone answers nothing; its own teardown is
                // under way.
                let _ = handle.update(cx, |ws, _window, cx| ws.finish_termination(cx));
            }
            // The startup screen is not a window on a vault, so it is not in the
            // per-vault list — but a signal asked to stop the program, and a window
            // left open on a question is the program still running (P4.20 EDGE-V16).
            if let Some(handle) = vaults::take_startup_window(cx) {
                let _ = handle.update(cx, |_this, window, _cx| window.remove_window());
            }
        });
    });
    // A gpui task is cancelled when its handle drops, so the registry — which
    // lives as long as the run — keeps the handle for the wait.
    cx.update_global::<VaultWindows, _>(|registry, _cx| {
        registry.set_termination_watcher(task);
    });
}

/// The first note ahead of its file in any of these windows, named for the status
/// line. A window the handle cannot reach (already closed) answers nothing.
fn first_dirty_note_in(
    handles: &[gpui::WindowHandle<Workspace>],
    cx: &mut Context<Workspace>,
) -> Option<String> {
    handles.iter().find_map(|handle| {
        handle
            .read_with(cx, |ws, cx| ws.first_dirty_note(cx))
            .ok()
            .flatten()
    })
}

impl Workspace {
    /// The exit a signal asked for: the note on screen to the disk, the session
    /// written, the index checkpointed, then the window closed.
    ///
    /// A failed write does not stop the exit. `kill` means the process must go; the
    /// difference between this and dying where told is that it goes having tried, and
    /// the log says what could not be written.
    pub(crate) fn finish_termination(&mut self, cx: &mut Context<Self>) {
        // Every tab, not the one on screen: a signal ends the session, and the
        // session is the notes on the tab bar.
        self.write_all_tabs(cx);
        self.leave(cx);
    }

    /// `:q`: close the tab in hand, and let the cascade go as far as it needs to.
    /// The last tab of a pane closes that pane; the last pane closes the window; the
    /// last window ends the run. `ctrl-q` and the palette's *Quit Shale* are `:qall`,
    /// below — the key was always "end this run", never "lose this tab".
    ///
    /// **A dirty note refuses it.** `:q` promises neither a save nor a discard, so
    /// when the thing about to go is ahead of its file the status bar names that note
    /// and nothing moves. `:wq` writes on purpose; `:q!` discards on purpose. Autosave
    /// does not change the answer: the refusal is one keystroke wide, and a quit that
    /// lands in the middle of the countdown is the one that used to lose work.
    ///
    /// **The check covers exactly what would go.** Closing one tab is not a reason to
    /// argue about a note three tabs away; closing a pane is a reason to ask about
    /// that pane's tabs, and closing the window is a reason to ask about all of them.
    pub(super) fn quit_tab(&mut self, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let last_tab = self.pane().tabs.len() == 1;
        let last_pane = self.panes.len() == 1;
        let dirty = if last_tab && last_pane {
            self.first_dirty_note(cx)
        } else if last_tab {
            self.first_dirty_note_in_pane(self.active_pane, cx)
        } else {
            self.dirty_note_in_tab(self.pane().active, cx)
        };
        if let Some(note) = dirty {
            if !force {
                self.say(
                    format!("unsaved edits in {note}: `:w` to keep them, `:q!` to throw them away"),
                    cx,
                );
                cx.notify();
                return;
            }
            // Findable afterwards: an exit that throws work away should be in the log,
            // and with tabs open it can be more than one note.
            tracing::warn!(note, "close discarded unsaved edits");
        }
        if last_tab && last_pane {
            // The window's turn. `leave` writes the session and drains the index; it
            // does not write notes, because the question above settled that one.
            self.leave(cx);
            return;
        }
        if last_tab {
            self.close_pane(!force, window, cx);
            return;
        }
        self.close_tab(self.pane().active, !force, window, cx);
        self.focus_active_tab(window, cx);
    }

    /// `:qall` / `ctrl-q`: quit every window this process holds, in every vault.
    ///
    /// One decision about the whole run, so the dirty check is the whole run's: a note
    /// ahead of its file in *any* window refuses the quit and names the first one.
    /// `:qall!` is the bang that says they know. A queue of agent proposals is refused
    /// too, for the reason in P2.5a: walking away from unanswered proposals without
    /// knowing is the failure that feature exists to prevent.
    pub(super) fn quit_all(&mut self, force: bool, cx: &mut Context<Self>) {
        // The window that asked is one of the run's windows, and this code runs
        // *inside* it. gpui refuses both ways of reaching across: a read of a window
        // already on the stack panics ("attempted to read a window that is already on
        // the stack"), and an update to it is dropped without running. So `self`
        // answers its own questions and the registry is asked only about the OTHER
        // windows — see [`Workspace::other_windows`].
        let others = self.other_windows(cx);
        // A window the registry never learned about (a test built without it) is its
        // own whole run, so the questions below are asked of it directly.
        let dirty = self
            .first_dirty_note(cx)
            .or_else(|| first_dirty_note_in(&others, cx));
        if let Some(note) = dirty.as_ref() {
            if !force {
                self.say(
                    format!(
                        "unsaved edits in {note}: `:w` to keep them, `:qall!` to throw them away"
                    ),
                    cx,
                );
                cx.notify();
                return;
            }
            tracing::warn!(note = %note, "quit discarded unsaved edits");
        }
        // A dirty run never reaches here without `force`: plain `:qall` refused above
        // and told the user to write or bang, so there is no write to do on the way out
        // — `:wqall` is the word that writes, and it does that through
        // [`Workspace::write_all_windows`] before asking this question again.
        // Read the queue rather than trusting the last beat: a proposal that landed a
        // moment ago is exactly the one the user is about to walk away from.
        if !force {
            self.scan_pending(cx);
            if self.review_active(cx) {
                self.say(
                    "agent edits are waiting in the review strip: `:review` to see them, `:qall!` to leave them queued",
                    cx,
                );
                cx.notify();
                return;
            }
        }
        // Each window leaves by its own door, so each writes its own session and
        // drains what it owns; the run ends when the last window goes. This window
        // leaves by direct call — its handle would be refused, as above.
        self.leave(cx);
        for handle in &others {
            let _ = handle.update(cx, |ws, _window, cx| ws.leave(cx));
        }
    }

    /// `:wall` / `:wa`: every dirty note in every window to the disk, and nothing
    /// else changes. The count is the point of saying anything: a `:wall` that writes
    /// four notes should report four, and one that writes nothing should not look like
    /// it did something.
    pub(super) fn write_all_windows(&mut self, cx: &mut Context<Self>) {
        // The asking window answers about itself and writes its own tabs; the
        // registry is only asked about the windows gpui is not already inside.
        let others = self.other_windows(cx);
        let written = self.dirty_tab_count(cx)
            + others.iter().fold(0usize, |sum, handle| {
                sum + handle
                    .read_with(cx, |ws, cx| ws.dirty_tab_count(cx))
                    .unwrap_or(0)
            });
        self.write_all_tabs(cx);
        for handle in &others {
            let _ = handle.update(cx, |ws, _window, cx| ws.write_all_tabs(cx));
        }
        self.say(
            match written {
                0 => "nothing to write".to_string(),
                1 => "wrote 1 note".to_string(),
                n => format!("wrote {n} notes"),
            },
            cx,
        );
        cx.notify();
    }

    /// Every window in this run except the one asking.
    ///
    /// gpui hands a window to its own update and refuses to be reached through a
    /// handle while it is on the stack: a read of it panics ("attempted to read a
    /// window that is already on the stack") and an update to it is dropped without
    /// running. A whole-run command therefore answers for itself and asks the registry
    /// only about the rest.
    pub(super) fn other_windows(&self, cx: &gpui::App) -> Vec<gpui::WindowHandle<Workspace>> {
        let here = self.self_window.map(|handle| handle.window_id());
        VaultWindows::all_windows(cx)
            .into_iter()
            .filter(|handle| Some(handle.window_id()) != here)
            .collect()
    }

    /// `:wq`: the write the user asked for, then the same cascade `:q` runs. Not one
    /// action — the write is unconditional and the exit depends on it having worked.
    /// If a note is still dirty the write failed, `write_note` already said why, and
    /// the tab stays open with that message in it.
    pub(super) fn write_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Every tab this window holds, not only the note under the cursor: the
        // cascade can take a pane or a window, and those carry tabs with them.
        if !self.write_all_tabs(cx) {
            cx.notify();
            return;
        }
        self.quit_tab(false, window, cx);
    }

    /// The `Quit` action: `:qall` / `:qa`, and `ctrl-q`. The key was always "end this
    /// run", which is why the tab cascade never took it.
    pub(super) fn quit(&mut self, _: &Quit, _window: &mut Window, cx: &mut Context<Self>) {
        self.quit_all(false, cx);
    }

    /// The `QuitForce` action: `:qall!` / `:qa!`.
    pub(super) fn quit_forced(
        &mut self,
        _: &QuitForce,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.quit_all(true, cx);
    }

    /// The `CloseTab` action: `:q` / `:quit`.
    pub(super) fn close_tab_quitting(
        &mut self,
        _: &CloseTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.quit_tab(false, window, cx);
    }

    /// The `CloseTabForce` action: `:q!` / `:quit!`.
    pub(super) fn close_tab_forced(
        &mut self,
        _: &CloseTabForce,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.quit_tab(true, window, cx);
    }

    /// The `WriteAll` action: `:wall` / `:wa`.
    pub(super) fn write_all_notes(
        &mut self,
        _: &WriteAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.write_all_windows(cx);
    }

    /// The part of every exit that is not the note: the index drained and
    /// checkpointed, the session written, the live MCP channel taken down, and the
    /// queue beat stopped.
    /// Roadmap P1.5 is the first of those — an exit leaves one DB file rather than
    /// a WAL full of unapplied writes — P1.8 the second, so the next window opens
    /// on this note, at this place, in this view, and P2.3 the third, so the next
    /// `shale-mcp` finds no port and no lock rather than a stale hint. P2.5a's part is
    /// nothing at all now that the queue beat belongs to the vault: the queue itself is
    /// *meant* to outlive the window — the files stay, and the next launch reads them
    /// again.
    pub(super) fn leave(&mut self, cx: &mut Context<Self>) {
        self.flush_index();
        self.save_session(cx);
        // The live channel belongs to the vault's services now; it comes down
        // with the last window, not with this one.
        // One process can hold several vault windows: leaving closes *this*
        // window, and the run ends on its own when the last one goes. The
        // close is deferred because `leave` often runs from inside this very
        // window's update — a signal handler, a picker's answer — and gpui
        // refuses to yank the window out from under its own frame. A
        // workspace that never learned its window (a test built without the
        // registry) quits the app the way a single-window Shale always did.
        match self.self_window {
            Some(handle) => cx.defer(move |cx| {
                let _ = handle.update(cx, |_ws, window, _cx| window.remove_window());
            }),
            None => cx.quit(),
        }
    }
}
