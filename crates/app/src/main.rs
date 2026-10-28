//! Shale: an Obsidian-style markdown editor built on GPUI.
//!
//! The window itself lives in [`workspace`]; this file is the command line and
//! the entry point.

mod acts;
mod editor;
mod field;
mod image;
mod line;
mod picker;
mod preview;
mod prompt;
mod table_view;
mod theme;
mod tree;
mod workspace;

use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result};
use clap::Parser;
use gpui::{prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use shale_core::{Config, Vault};

use acts::FocusEditor;

use workspace::{keybindings, start_watcher, Workspace};

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
    let config = Rc::new(Config::load(vault.root())?);
    let initial_note = args.note.clone();

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
                        title: Some("Shale".into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1200.), px(800.)),
                        cx,
                    ))),
                    ..Default::default()
                },
                move |_window, cx| {
                    cx.activate(true);
                    if let Some(note) = note.clone() {
                        ws_for_build.update(cx, |ws, cx| ws.open_note(&note, cx));
                    }
                    ws_for_build
                },
            )
            .unwrap();
        // The note holds the keyboard from the first frame, so the very first
        // keystroke after launch is an edit and not an ignored key.
        window
            .update(cx, |ws, window, cx| {
                ws.focus_editor(&FocusEditor, window, cx)
            })
            .unwrap();

        // The vault can change while Shale is open — Obsidian, git, another
        // terminal — so the tree and the open note follow the disk.
        start_watcher(&workspace, cx);

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
