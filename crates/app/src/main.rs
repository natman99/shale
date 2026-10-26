//! Shale: an Obsidian-style markdown editor built on GPUI.
//!
//! This is the workspace shell: a file tree sidebar and a markdown editor.

use std::rc::Rc;

use anyhow::{Context as _, Result};
use clap::Parser;
use gpui::{
    actions, div, prelude::*, px, relative, App, AppContext, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    TextEditor, Titlebar, Window, WindowBounds, WindowOptions,
};
use shale_core::{Config, Vault};

#[derive(Parser, Debug)]
#[command(name = "shale", about = "Obsidian-style markdown editor")]
struct Args {
    /// Path to the vault directory.
    vault: std::path::PathBuf,

    /// Override the config file location.
    #[arg(long)]
    config: Option<std::path::PathBuf>,
}

actions!(
    shale,
    [
        /// Save the current file.
        Save,
        /// Reveal the current file in the file tree.
        RevealInTree,
    ]
);

struct Workspace {
    vault: Rc<Vault>,
    config: Rc<Config>,
    entries: Vec<shale_core::vault::VaultEntry>,
    expanded_dirs: Vec<std::path::PathBuf>,
    active_path: Option<std::path::PathBuf>,
    editor: Entity<TextEditor>,
}

impl Workspace {
    fn new(vault: Rc<Vault>, config: Rc<Config>, cx: &mut Context<Self>) -> Self {
        let entries = vault.list_markdown().unwrap_or_default();
        let editor = cx.new(|_cx| TextEditor::default());
        Self {
            vault,
            config,
            entries,
            expanded_dirs: Vec::new(),
            active_path: None,
            editor,
        }
    }

    fn open_note(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let Ok(text) = self.vault.read(path) else {
            return;
        };
        self.active_path = Some(path.to_path_buf());
        self.editor.update(cx, |editor, cx| {
            editor.set_value(text, cx);
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active_path.clone() else {
            return;
        };
        let text = self
            .editor
            .read_with(cx, |editor, _cx| editor.value().to_string());
        if let Err(err) = self.vault.write(&path, &text) {
            eprintln!("shale: {err:#}");
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.entries.clone();
        let active = self.active_path.clone();

        div()
            .size_full()
            .flex()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xd4d4d4))
            .child(
                // File tree sidebar.
                div()
                    .w(px(260.))
                    .h_full()
                    .overflow_y_scroll()
                    .border_r_1()
                    .border_color(rgb(0x3c3c3c))
                    .p_2()
                    .child(
                        div().mb_2().font_bold().text_sm().child(
                            self.vault
                                .root()
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                        ),
                    )
                    .children(entries.iter().map(|entry| {
                        let path = entry.path.clone();
                        let is_active = active.as_deref() == Some(path.as_path());
                        let label = path.display().to_string();
                        div()
                            .id(("note", label.clone()))
                            .cursor_pointer()
                            .px_1()
                            .py_0p5()
                            .text_sm()
                            .when(is_active, |d| d.bg(rgb(0x2f2f2f)))
                            .child(label)
                            .on_mouse_up(gpui::MouseButton::Left, {
                                let path = path.clone();
                                cx.listener(move |this, _, _, cx| {
                                    this.open_note(&path, cx);
                                })
                            })
                    })),
            )
            .child(
                // Editor pane.
                div().flex_1().size_full().p_2().child(self.editor.clone()),
            )
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let vault = Rc::new(Vault::open(&args.vault).context("opening vault")?);
    let config_dir = args
        .config
        .as_deref()
        .map(std::path::Path::new)
        .unwrap_or(vault.root());
    let config = Rc::new(Config::load(config_dir)?);

    let app = App::new()?;

    app.run(|cx: &mut App| {
        let workspace = cx.new(|cx| Workspace::new(vault.clone(), config.clone(), cx));

        cx.on_action(|_: &Save, cx: &mut App| {
            // Handled via the workspace below; global fallback does nothing.
            let _ = cx;
        });

        cx.open_window(
            WindowOptions {
                titlebar: Some(Titlebar {
                    title: Some("Shale".into()),
                    ..Default::default()
                }),
                window_bounds: WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.), px(800.)),
                    cx,
                )),
                ..Default::default()
            },
            move |window, cx| {
                cx.activate(true);
                cx.bind_keys([KeyBinding::new("ctrl+s", Save, None)]);
                window.set_focus_handle(workspace.focus_handle(cx));
                workspace.clone()
            },
        )?;
    });
    Ok(())
}
