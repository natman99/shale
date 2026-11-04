//! Shale: an Obsidian-style markdown editor built on GPUI.
//!
//! The window itself lives in [`workspace`]; this file is the command line and
//! the entry point.

mod acts;
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
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use clap::Parser;
use gpui::{prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use shale_core::{Config, Vault};

use workspace::{keybindings, start_watcher, vault_title, Workspace};

#[derive(Parser, Debug)]
#[command(name = "shale", about = "Obsidian-style markdown editor")]
struct Args {
    /// Path to the vault directory.
    vault: PathBuf,

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
    let vault = Rc::new(Vault::open(&args.vault).context("opening vault")?);
    // The vault's own `.shale/config.toml`: written from the starter when the
    // vault has no config yet, and moved out of the vault root when it is still
    // the old root-level `shale.toml`.
    let (config, source) = Config::load_or_create(vault.root()).context("loading config")?;
    tracing::info!(?source, path = %Config::path(vault.root()).display(), "config");
    let config = Rc::new(config);
    let initial_note = args.note.clone();
    let graph_at_start = args.graph;
    // The window is named for the vault, not for the program: a window list is a
    // list of vaults, and "Shale" is no use to anyone with two open. The first
    // frame extends it to `vault — note` (`workspace::render`).
    let vault_name = vault_title(vault.root());

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

    let app = gpui_platform::application();
    app.run(move |cx: &mut App| {
        cx.bind_keys(keybindings());

        let workspace = cx.new(|cx| Workspace::new(vault.clone(), config.clone(), cx));
        let note = initial_note.clone();
        let ws_for_build = workspace.clone();

        let window = cx
            .open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(vault_name.clone().into()),
                        ..Default::default()
                    }),
                    // Filed under its own name rather than under whatever the
                    // window manager guesses: X11 reads this as WM_CLASS, Wayland
                    // as the xdg app_id, and `assets/shale.desktop` repeats it in
                    // StartupWMClass so the desktop can find the entry, and
                    // through it the mark (§13).
                    app_id: Some(identity::APP_ID.into()),
                    // The mark itself, for the one toolkit that takes it from the
                    // program. A Wayland desktop gets the same mark from the
                    // `.desktop` entry instead.
                    #[cfg(target_os = "linux")]
                    icon: Some(identity::window_icon(64)),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1200.), px(800.)),
                        cx,
                    ))),
                    ..Default::default()
                },
                move |window, cx| {
                    cx.activate(true);
                    // The pointer's position over the note can open a popup, and a
                    // popup taking the keyboard is a focus change, so that part of
                    // the window is wired up here, where there is a window.
                    ws_for_build.update(cx, |ws, cx| ws.attach_window(window, cx));
                    if let Some(note) = note.clone() {
                        ws_for_build.update(cx, |ws, cx| ws.open_note(&note, cx));
                    }
                    if graph_at_start {
                        ws_for_build.update(cx, |ws, cx| ws.open_graph(cx));
                    }
                    ws_for_build
                },
            )
            .unwrap();
        // Whatever the window came up on holds the keyboard from the first frame, so
        // the very first keystroke after launch is an edit and not an ignored key. Not
        // `focus_editor`: a session that ended on the graph tab comes back to the graph
        // tab, and the keyboard has to be where the frame is.
        window
            .update(cx, |ws, window, cx| {
                if args.split {
                    ws.split_pane(workspace::panes::SplitDir::Row, false, window, cx);
                }
                ws.focus_active_tab(window, cx)
            })
            .unwrap();

        // The vault can change while Shale is open — Obsidian, git, another
        // terminal — so the tree and the open note follow the disk.
        start_watcher(&workspace, cx);

        // ...and a `kill` is answered the same way a quit is: the note to the disk,
        // the index checkpointed, then the window.
        workspace.update(cx, |ws, cx| {
            ws.watch_for_termination(terminating.clone(), cx)
        });

        if let Some(secs) = args.open_after {
            let ws = workspace.clone();
            let note = args
                .note
                .clone()
                .unwrap_or_else(|| PathBuf::from("Welcome.md"));
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(secs))
                    .await;
                _ = ws.update(cx, |ws, cx| ws.open_note(&note, cx));
            })
            .detach();
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests;
