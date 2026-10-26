//! Shale: an Obsidian-style markdown editor built on GPUI.

mod editor;
mod tree;

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result};
use clap::Parser;
use gpui::{
    div, prelude::*, px, size, App, Application, Bounds, Context, Entity, Focusable,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use shale_core::vault::VaultEntry;
use shale_core::{Config, Vault};

use editor::MarkdownEditor;

mod acts {
    use gpui::actions;

    actions!(
        shale_editor,
        [
            VimH,
            VimJ,
            VimK,
            VimL,
            VimW,
            VimB,
            VimE,
            VimZero,
            VimCaret,
            VimDollar,
            VimG,
            VimUpperG,
            VimX,
            VimD,
            VimI,
            VimUpperI,
            VimA,
            VimUpperA,
            VimO,
            VimUpperO,
            VimEscape,
            Newline,
            NextLine,
            MoveLeft,
            MoveRight,
            MoveUp,
            MoveDown,
            Backspace,
            DeleteForward,
            Undo,
            Redo,
            CopyText,
            PasteText,
        ]
    );
    actions!(shale, [Save, Quit]);
}

use acts::*;

#[derive(Parser, Debug)]
#[command(name = "shale", about = "Obsidian-style markdown editor")]
struct Args {
    /// Path to the vault directory.
    vault: PathBuf,

    /// Open this note on startup (vault-relative).
    #[arg(long)]
    note: Option<PathBuf>,
}

struct Workspace {
    focus_handle: gpui::FocusHandle,
    vault: Rc<Vault>,
    #[allow(dead_code)]
    config: Rc<Config>,
    entries: Vec<VaultEntry>,
    expanded: HashSet<PathBuf>,
    active: Option<PathBuf>,
    status: SharedString,
    editor: Entity<MarkdownEditor>,
}

impl Workspace {
    fn new(vault: Rc<Vault>, config: Rc<Config>, cx: &mut Context<Self>) -> Self {
        let entries = vault.list_markdown().unwrap_or_default();
        let expanded = entries
            .iter()
            .filter_map(|e| e.path.parent().map(|p| p.to_path_buf()))
            .collect();
        let editor = cx.new(|cx| MarkdownEditor::new(cx));
        Self {
            focus_handle: cx.focus_handle(),
            vault,
            config,
            entries,
            expanded,
            active: None,
            status: "".into(),
            editor,
        }
    }

    fn open_note(&mut self, path: &PathBuf, cx: &mut Context<Self>) {
        match self.vault.read(path) {
            Ok(text) => {
                tracing::debug!(path = %path.display(), bytes = text.len(), "opened note");
                self.active = Some(path.clone());
                self.editor
                    .update(cx, |editor, cx| editor.set_text(text, cx));
                self.status = path.display().to_string().into();
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), "failed to read note: {err:#}");
                self.status = format!("{err:#}").into();
            }
        }
        cx.notify();
    }

    fn toggle_dir(&mut self, path: PathBuf) {
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
    }

    fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.active.clone() else {
            self.status = "no file open".into();
            cx.notify();
            return;
        };
        let text = self.editor.read_with(cx, |editor, _cx| editor.text());
        match self.vault.write(&path, &text) {
            Ok(()) => {
                self.editor.update(cx, |editor, _cx| editor.mark_clean());
                self.status = format!("saved {}", path.display()).into();
            }
            Err(err) => {
                self.status = format!("{err:#}").into();
            }
        }
        cx.notify();
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tree = tree::build_tree(&self.entries);
        let active = self.active.clone();
        let expanded = &self.expanded;

        // Build sidebar rows.
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        let mut idx = 0usize;
        for node in &tree {
            push_node(&mut rows, node, 0, &active, expanded, &mut idx, cx);
        }

        let mode = self.editor.read(cx).mode();
        let mode_str = match mode {
            shale_core::vim::Mode::Normal => "NORMAL",
            shale_core::vim::Mode::Insert => "INSERT",
        };
        let dirty = if self.editor.read(cx).is_dirty() {
            "●"
        } else {
            ""
        };
        let (line, col) = self.editor.read(cx).cursor_position();

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(0x1e1e1e))
            .text_color(gpui::rgb(0xd4d4d4))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(|_, _: &Quit, _, cx| cx.quit()))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .id("tree-scroll")
                            .flex_none()
                            .w(px(260.))
                            .h_full()
                            .overflow_scroll()
                            .border_r_1()
                            .border_color(gpui::rgb(0x3c3c3c))
                            .p_2()
                            .text_sm()
                            .child(
                                div().mb_2().font_weight(gpui::FontWeight::BOLD).child(
                                    self.vault
                                        .root()
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string(),
                                ),
                            )
                            .children(rows),
                    )
                    .child(div().flex_1().min_w_0().h_full().child(self.editor.clone())),
            )
            .child(
                div()
                    .h(px(24.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_4()
                    .text_xs()
                    .bg(gpui::rgb(0x252526))
                    .child(format!("{mode_str}{dirty}"))
                    .child(self.status.clone())
                    .child(format!("{}:{}", line + 1, col + 1)),
            )
    }
}

fn push_node(
    rows: &mut Vec<gpui::AnyElement>,
    node: &tree::TreeNode,
    depth: usize,
    active: &Option<PathBuf>,
    expanded: &HashSet<PathBuf>,
    idx: &mut usize,
    cx: &mut Context<Workspace>,
) {
    let indent = px(12. * depth as f32);
    let is_dir = !node.is_file;
    let is_open = expanded.contains(&node.path);
    let is_active = active.as_deref() == Some(node.path.as_path());
    let label = format!(
        "{} {}",
        if is_dir {
            if is_open {
                "▾"
            } else {
                "▸"
            }
        } else {
            " "
        },
        node.name
    );
    let my_idx = *idx;
    *idx += 1;

    let row = div()
        .id(("tree-item", my_idx))
        .cursor_pointer()
        .pl(indent)
        .pr_1()
        .py_0p5()
        .when(is_active, |d| d.bg(gpui::rgb(0x2f2f2f)))
        .hover(|s| s.bg(gpui::rgb(0x2a2a2a)))
        .child(label)
        .on_click({
            let path = node.path.clone();
            cx.listener(move |this, _: &gpui::ClickEvent, _window, cx| {
                tracing::debug!(path = %path.display(), is_dir, "tree click");
                if !is_dir && path.extension().map(|e| e == "md").unwrap_or(false) {
                    this.open_note(&path, cx);
                } else {
                    this.toggle_dir(path.clone());
                }
            })
        });
    rows.push(row.into_any());

    if is_dir && is_open {
        for child in &node.children {
            push_node(rows, child, depth + 1, active, expanded, idx, cx);
        }
    }
}

fn keybindings() -> Vec<KeyBinding> {
    let mut keys = vec![
        KeyBinding::new("ctrl+s", Save, None),
        KeyBinding::new("ctrl+q", Quit, None),
    ];
    macro_rules! bind {
        ($ctx:expr, [ $( $key:literal => $action:ident ),* $(,)? ]) => {
            $( keys.push(KeyBinding::new($key, $action, Some($ctx))); )*
        };
    }
    bind!("vim-normal", [
        "h" => VimH,
        "j" => VimJ,
        "k" => VimK,
        "l" => VimL,
        "w" => VimW,
        "b" => VimB,
        "e" => VimE,
        "0" => VimZero,
        "shift-6" => VimCaret,
        "shift-4" => VimDollar,
        "g" => VimG,
        "shift-g" => VimUpperG,
        "d" => VimD,
        "x" => VimX,
        "i" => VimI,
        "shift-i" => VimUpperI,
        "a" => VimA,
        "shift-a" => VimUpperA,
        "o" => VimO,
        "shift-o" => VimUpperO,
        "escape" => VimEscape,
        "enter" => NextLine,
        "ctrl+z" => Undo,
        "ctrl+r" => Redo,
        "ctrl+shift+c" => CopyText,
        "ctrl+shift+v" => PasteText,
    ]);
    bind!("vim-insert", [
        "escape" => VimEscape,
        "enter" => Newline,
        "backspace" => Backspace,
        "delete" => DeleteForward,
        "left" => MoveLeft,
        "right" => MoveRight,
        "up" => MoveUp,
        "down" => MoveDown,
        "ctrl+z" => Undo,
        "ctrl+r" => Redo,
        "ctrl+shift+c" => CopyText,
        "ctrl+shift+v" => PasteText,
    ]);
    keys
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

    let app = Application::new();
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
        let editor_focus = workspace.read(cx).editor.read(cx).focus_handle(cx);
        window
            .update(cx, |_ws, window, _cx| window.focus(&editor_focus))
            .unwrap();
    });
    Ok(())
}
