//! Shale: an Obsidian-style markdown editor built on GPUI.
//!
//! The window itself lives in [`workspace`]; this file is the command line and
//! the entry point.

mod acts;
mod card;
mod command;
mod desktop;
mod dim;
mod editor;
mod ex;
mod field;
mod graph;
mod identity;
mod image;
mod line;
mod list;
mod live;
mod lsp;
mod note;
mod picker;
mod popup;
mod preview;
mod prompt;
mod review;
mod sidebar;
mod style;
mod table_view;
mod theme;
mod tooltip;
mod tree;
mod workspace;

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::Parser;
use gpui::App;
use shale_core::RecentVaults;

use workspace::{
    exit::{self, Termination},
    keybindings, vaults,
};

#[derive(Parser, Debug)]
#[command(name = "shale", about = "Obsidian-style markdown editor")]
struct Args {
    /// Path to the vault directory. With none given, Shale opens the vault it
    /// opened last (`:vault` and the picker are how that list grows).
    vault: Option<PathBuf>,

    /// Open this note on startup (vault-relative).
    #[arg(long)]
    note: Option<PathBuf>,

    /// Debug aid: open `--note` (or Welcome.md) this many seconds after the
    /// window is created, to exercise post-paint updates.
    #[arg(long)]
    open_after: Option<u64>,

    /// Debug aid: tile a second pane beside the first at startup. P3.2's
    /// render is what this checks on a live window.
    #[arg(long)]
    split: bool,
    /// Debug aid: open the graph tab at startup. P3.4's exit criteria are
    /// numbers — fps while the layout settles, zero frames once it sleeps —
    /// and checking them should not depend on remembering the chord.
    #[arg(long)]
    graph: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "shale=debug".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    // Which vault: the command line first (kept for testing and for muscle
    // memory), then the vault this app opened last.
    let vault_path = args
        .vault
        .clone()
        .or_else(|| RecentVaults::load().startup_vault())
        .context(
            "no vault: name one on the command line, or open one from \
                  another vault's `:vault` picker first",
        )?;

    // Roadmap P1.5: a termination signal is an exit the user asked for, not a crash.
    // The handler runs in signal context, where the only safe work is an atomic
    // store and a one-byte write to a self-pipe, so it raises the flag and pokes a
    // blocking reader thread, which wakes the windows' exit watchers
    // (`workspace::exit`) — no window asks "yet?" on a timer.
    // SIGKILL cannot be caught, which is why the note writes themselves are atomic
    // (`shale_core::atomic`) and the index is a cache rather than a record.
    // The raiser is only ever held by the unix signal handlers below. On Windows there is
    // no raiser yet — a console handler is the part that would have to be written — so the
    // binding is named for what it is there: something kept alive for the run.
    #[cfg(not(unix))]
    let (_termination, termination_watcher) = Termination::new();
    #[cfg(unix)]
    let (termination, termination_watcher) = Termination::new();
    // Signals are a unix thing, and so are the two crates that raise them; the
    // waiting is the same code on every platform, and a Windows console handler
    // is the part that would have to be written.
    #[cfg(unix)]
    let _signal_handlers;
    #[cfg(unix)]
    {
        // The self-pipe. The write end stays nonblocking so a signal handler is
        // never held by a full pipe; the read end is cleared to blocking so the
        // reader thread sleeps in the kernel until a signal actually lands.
        // The two fds live for the run: they close with the process.
        let mut fds = [0 as libc::c_int; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            anyhow::bail!(
                "opening the termination self-pipe: {}",
                std::io::Error::last_os_error()
            );
        }
        unsafe { libc::fcntl(fds[0], libc::F_SETFL, 0) };
        _signal_handlers = [libc::SIGTERM, libc::SIGHUP, libc::SIGINT]
            .into_iter()
            .map(|signal| {
                let flag = termination.flag();
                let wake_write = fds[1];
                // `register` chains whatever handler was already there — gpui installs
                // none for these — and the action has to be async-signal-safe, which an
                // atomic store and a write to an empty nonblocking pipe are. The
                // handles are kept for the run: dropping one puts the previous handler
                // back, and the previous handler for SIGTERM is "drop the process
                // where it stands".
                unsafe {
                    signal_hook_registry::register(signal, move || {
                        flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        libc::write(wake_write, b"\x01".as_ptr() as *const std::ffi::c_void, 1);
                    })
                }
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("installing the termination handlers")?;
        // The reader thread: sleep on the pipe, and when a byte lands, wake the
        // windows through the channel. It leaves when the pipe closes or errors;
        // a signal that arrives after that has already raised the flag, and a
        // watcher reads the flag before it ever sleeps.
        let woken = termination.clone();
        let read_fd = fds[0];
        std::thread::spawn(move || {
            let mut buf = [0u8; 8];
            loop {
                let n = unsafe {
                    libc::read(
                        read_fd,
                        buf.as_mut_ptr() as *mut std::ffi::c_void,
                        buf.len(),
                    )
                };
                if n > 0 {
                    woken.wake();
                } else if n == 0 {
                    break; // the write end closed: the process is on its way out
                } else if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                    break;
                }
            }
        });
    }

    // Claim the vault before the GUI exists. A vault owned by another Shale
    // is then a message and an exit code, not a window; gpui's loop has no
    // clean way to end before its first window, so the refusal has to happen
    // out here where `return` still means something.
    let root = vault_path
        .canonicalize()
        .with_context(|| format!("{} is not a vault", vault_path.display()))?;
    let claim = vaults::claim_vault(&root)?;

    let options = vaults::OpenOptions {
        note: args.note.clone(),
        graph: args.graph,
        split: args.split,
        open_after: args.open_after,
    };
    let app = gpui_platform::application();
    app.run(move |cx: &mut App| {
        cx.bind_keys(keybindings());
        // The registry of vaults this process has windows for, and the claim
        // files that hold them. One Shale, one window per vault; the vault
        // switcher (`:vault`) adds windows to this rather than starting a
        // second copy of everything.
        cx.set_global(vaults::VaultWindows::new());
        // One watcher for the whole process: it sleeps on the channel and, when a
        // signal lands, exits every window the registry holds.
        exit::spawn_watcher(termination_watcher, cx);
        vaults::seed_claim(root, claim, cx);
        if let Err(e) = vaults::open_vault_window(&vault_path, options, cx) {
            // The window is the app: no window, no app. The reason goes to
            // the terminal the command came from. `quit` cannot end a run
            // that never opened a window, so the process ends directly —
            // nothing is open to clean up.
            eprintln!("shale: {e:#}");
            std::process::exit(1);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests;
