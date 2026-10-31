//! Leaving the session: what an exit owes the disk (roadmap P1.5).
//!
//! Two ways out reach the same promise. `quit` (`ctrl-q`, `:q`, `:wq`) and the
//! window manager's close act inside the window. A **termination signal** arrives on
//! a thread of the kernel's choosing, which is why it is handled here rather than in
//! a signal handler: the handler does the only thing a signal context allows — an
//! atomic store — and the window polls that flag. gpui's event loop is not ours to
//! wake, so polling is where the two meet.
//!
//! What the promise is: the note the user can see reaches the disk, the session
//! file says where they were in it, the index is drained and checkpointed, and
//! then the process goes. SIGKILL is out of scope by definition — nothing runs —
//! which is why the note writes themselves are atomic
//! ([`shale_core::atomic`]) and the index is a cache rather than a record.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::Context;

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
        self.save_session(cx);
        self.flush_index();
        cx.quit();
    }
}
