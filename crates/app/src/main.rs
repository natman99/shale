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
mod style;
mod table_view;
mod theme;
mod tree;
mod workspace;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use clap::Parser;
use gpui::App;
use shale_core::RecentVaults;

use workspace::{keybindings, vaults};

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
    // store, so it raises a flag and the window polls it (`workspace::exit`).
    // SIGKILL cannot be caught, which is why the note writes themselves are atomic
    // (`shale_core::atomic`) and the index is a cache rather than a record.
    let terminating = Arc::new(AtomicBool::new(false));
    // Signals are a unix thing, and so are the two crates that raise them; the
    // flag stays, because the window's poll is the same code on every platform,
    // and a Windows console handler is the part that would have to be written.
    #[cfg(unix)]
    let _signal_handlers = [libc::SIGTERM, libc::SIGHUP, libc::SIGINT]
        .into_iter()
        .map(|signal| {
            let flag = Arc::clone(&terminating);
            // `register` chains whatever handler was already there — gpui installs
            // none for these — and the action has to be async-signal-safe, which an
            // atomic store is. The handles are kept for the run: dropping one puts
            // the previous handler back, and the previous handler for SIGTERM is
            // "drop the process where it stands".
            unsafe {
                signal_hook_registry::register(signal, move || {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst)
                })
            }
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("installing the termination handlers")?;

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
        cx.set_global(vaults::VaultWindows::new(terminating.clone()));
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
