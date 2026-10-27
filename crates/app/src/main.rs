//! Shale: an Obsidian-style markdown editor built on GPUI.

mod editor;
mod image;
mod line;
mod preview;
mod table_view;
mod theme;
mod tree;

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result};
use clap::Parser;
use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Entity, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, SharedString, StatefulInteractiveElement,
    Styled, Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use shale_core::vault::VaultEntry;
use shale_core::{theme::Palette, Config, Vault};

use editor::{EditorEvent, MarkdownEditor};

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
            VimF,
            VimUpperF,
            VimT,
            VimUpperT,
            VimSpace,
            VimZero,
            VimCaret,
            VimDollar,
            VimG,
            VimUpperG,
            VimX,
            VimD,
            VimC,
            VimY,
            VimP,
            VimUpperP,
            VimU,
            VimDigit1,
            VimDigit2,
            VimDigit3,
            VimDigit4,
            VimDigit5,
            VimDigit6,
            VimDigit7,
            VimDigit8,
            VimDigit9,
            VimI,
            VimUpperI,
            VimA,
            VimUpperA,
            VimO,
            VimUpperO,
            VimV,
            VimUpperV,
            VimUpperH,
            VimUpperJ,
            VimUpperK,
            VimUpperL,
            VimUpperD,
            VimPipe,
            VimBackslash,
            VimAltH,
            VimAltJ,
            VimAltK,
            VimAltL,
            VimZ,
            VimUpperR,
            VimUpperM,
            VimUpperE,
            VimDot,
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

    /// Debug aid: open `--note` (or Welcome.md) this many seconds after the
    /// window is created, to exercise post-paint updates.
    #[arg(long)]
    open_after: Option<u64>,
}

struct Workspace {
    focus_handle: gpui::FocusHandle,
    vault: Rc<Vault>,
    #[allow(dead_code)]
    config: Rc<Config>,
    /// The palette the whole window is drawn with, shared with the editor so a
    /// theme change repaints everything at once.
    palette: Rc<Palette>,
    entries: Vec<VaultEntry>,
    expanded: HashSet<PathBuf>,
    active: Option<PathBuf>,
    status: SharedString,
    editor: Entity<MarkdownEditor>,
    _editor_events: Subscription,
    _keystroke_observer: Subscription,
}

impl Workspace {
    fn new(vault: Rc<Vault>, config: Rc<Config>, cx: &mut Context<Self>) -> Self {
        let entries = vault.list_markdown().unwrap_or_default();
        let expanded = entries
            .iter()
            .filter_map(|e| e.path.parent().map(|p| p.to_path_buf()))
            .collect();
        let editor = cx.new(|cx| MarkdownEditor::new(cx));
        let known = note_keys(&entries);
        editor.update(cx, |editor, cx| editor.set_known_links(known, cx));
        // The editor resolves `![[image.png]]` against the vault's attachments,
        // so it needs the vault itself and everything in it that is not a note.
        let attachments = vault.list_attachments().unwrap_or_default();
        editor.update(cx, |editor, cx| {
            editor.set_vault(vault.clone(), attachments, cx);
        });
        editor.update(cx, |editor, cx| {
            editor.set_font_family(&config.appearance.font_family, cx);
        });
        // The palette comes from `appearance.theme`: a built-in name or a
        // `<vault>/.shale/themes/<name>.toml` file. A theme that cannot be found
        // or parsed must not stop the editor from starting, so it is reported and
        // the default palette is used.
        let palette = match shale_core::theme::palette_for(vault.root(), &config.appearance.theme) {
            Ok(palette) => palette,
            Err(err) => {
                tracing::warn!("could not load the configured theme: {err:#}");
                Palette::DEFAULT
            }
        };
        let palette = Rc::new(palette);
        editor.update(cx, |editor, cx| {
            editor.set_palette(Rc::clone(&palette), cx);
            editor.set_font_size(config.appearance.font_size, cx);
        });
        let editor_events = cx.subscribe(&editor, |ws, _editor, event, cx| match event {
            EditorEvent::FollowLink { target } => ws.follow_link(target, cx),
        });
        // Keybinding bugs on Linux are almost always "what did the platform
        // actually deliver, and which action did it resolve to", so log both —
        // at trace level, since this fires for every keystroke and floods the
        // console at debug. Enable with RUST_LOG=shale=trace.
        // The subscription has to be kept alive or the observer is dropped.
        let keystroke_observer = cx.observe_keystrokes(|_ws, event, _window, _cx| {
            tracing::trace!(
                key = event.keystroke.key.as_str(),
                modifiers = ?event.keystroke.modifiers,
                action = event.action.as_ref().map(|action| action.name()),
                contexts = ?event.context_stack,
                "keystroke"
            );
        });
        Self {
            focus_handle: cx.focus_handle(),
            vault,
            config,
            palette,
            entries,
            expanded,
            active: None,
            status: "".into(),
            editor,
            _editor_events: editor_events,
            _keystroke_observer: keystroke_observer,
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

    /// Open the note a wiki-link target refers to, if it exists.
    fn follow_link(&mut self, target: &str, cx: &mut Context<Self>) {
        match self.vault.resolve_link(target, &self.entries) {
            Ok(Some(entry)) => {
                let path = entry.path.clone();
                tracing::debug!(target, path = %path.display(), "following wiki-link");
                self.open_note(&path, cx);
            }
            Ok(None) => {
                tracing::debug!(target, "unresolved wiki-link");
                self.status = format!("no note named {target}").into();
                cx.notify();
            }
            Err(err) => {
                tracing::warn!(target, "failed to resolve link: {err:#}");
                self.status = format!("{err:#}").into();
                cx.notify();
            }
        }
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
        let state = TreeState {
            active: &active,
            expanded,
            palette: &self.palette,
        };
        for node in &tree {
            push_node(&mut rows, node, 0, &state, &mut idx, cx);
        }

        let mode = self.editor.read(cx).mode();
        let mode_str = match mode {
            shale_core::vim::Mode::Normal => "NORMAL",
            shale_core::vim::Mode::Insert => "INSERT",
            shale_core::vim::Mode::Visual => "VISUAL",
            shale_core::vim::Mode::VisualLine => "V-LINE",
        };
        let dirty = if self.editor.read(cx).is_dirty() {
            "●"
        } else {
            ""
        };
        let pending = self.editor.read(cx).pending_hint();
        let (line, col) = self.editor.read(cx).display_position();

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(self.palette.ui_bg))
            .text_color(gpui::rgb(self.palette.fg))
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
                            .border_color(gpui::rgb(self.palette.ui_border))
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
                    .bg(gpui::rgb(self.palette.ui_status_bg))
                    .child(format!("{mode_str}{pending}{dirty}"))
                    .child(self.status.clone())
                    .child(format!("{}:{}", line + 1, col + 1)),
            )
    }
}

/// Lowercased link targets that resolve to an existing note: file stems and
/// vault-relative paths (with and without the `.md` extension).
fn note_keys(entries: &[VaultEntry]) -> HashSet<String> {
    let mut keys = HashSet::new();
    for entry in entries {
        keys.insert(entry.stem().to_lowercase());
        if let Some(s) = entry.path.to_str() {
            let lower = s.to_lowercase();
            keys.insert(lower.clone());
            if let Some(stripped) = lower.strip_suffix(".md") {
                keys.insert(stripped.to_string());
            }
        }
    }
    keys
}

/// What the sidebar rows are drawn from, bundled so `push_node` does not take
/// an argument per question it asks.
struct TreeState<'a> {
    /// The note the sidebar marks as current.
    active: &'a Option<PathBuf>,
    /// The directories the user has opened.
    expanded: &'a HashSet<PathBuf>,
    /// The theme's colours.
    palette: &'a Palette,
}

fn push_node(
    rows: &mut Vec<gpui::AnyElement>,
    node: &tree::TreeNode,
    depth: usize,
    state: &TreeState,
    idx: &mut usize,
    cx: &mut Context<Workspace>,
) {
    let indent = px(12. * depth as f32);
    let is_dir = !node.is_file;
    let is_open = state.expanded.contains(&node.path);
    let is_active = state.active.as_deref() == Some(node.path.as_path());
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
        .when(is_active, |d| d.bg(gpui::rgb(state.palette.ui_active_bg)))
        .hover(|s| s.bg(gpui::rgb(state.palette.ui_hover_bg)))
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
            push_node(rows, child, depth + 1, state, idx, cx);
        }
    }
}

fn keybindings() -> Vec<KeyBinding> {
    // gpui's keystroke syntax separates modifiers with `-`, not `+`: `ctrl-z`.
    // A `+` is not a separator, so "ctrl+z" parses as one literal key named
    // "ctrl+z" that can never match anything the platform delivers.
    let mut keys = vec![
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-q", Quit, None),
    ];
    macro_rules! bind {
        ($ctx:expr, [ $( $key:literal => $action:ident ),* $(,)? ]) => {
            $( keys.push(KeyBinding::new($key, $action, Some($ctx))); )*
        };
    }
    // Motions and operators are bound in both normal and visual mode; the vim
    // engine decides what each key means for the current mode.
    for context in ["vim-normal", "vim-visual"] {
        bind!(context, [
            "h" => VimH,
            "j" => VimJ,
            "k" => VimK,
            "l" => VimL,
            "w" => VimW,
            "b" => VimB,
            "e" => VimE,
            "f" => VimF,
            "shift-f" => VimUpperF,
            "t" => VimT,
            "shift-t" => VimUpperT,
            "space" => VimSpace,
            "0" => VimZero,
            "^" => VimCaret,
            "$" => VimDollar,
            "g" => VimG,
            "shift-g" => VimUpperG,
            "d" => VimD,
            "c" => VimC,
            "y" => VimY,
            "p" => VimP,
            "shift-p" => VimUpperP,
            "x" => VimX,
            "i" => VimI,
            "shift-i" => VimUpperI,
            "a" => VimA,
            "shift-a" => VimUpperA,
            "o" => VimO,
            "shift-o" => VimUpperO,
            "v" => VimV,
            "shift-v" => VimUpperV,
            // Table cell motions (see `shale_core::table`): the shifted forms
            // jump columns and rows. They are bound in visual mode too, where
            // they extend the selection.
            "shift-h" => VimUpperH,
            "shift-j" => VimUpperJ,
            "shift-k" => VimUpperK,
            "shift-l" => VimUpperL,
            "escape" => VimEscape,
            "1" => VimDigit1,
            "2" => VimDigit2,
            "3" => VimDigit3,
            "4" => VimDigit4,
            "5" => VimDigit5,
            "6" => VimDigit6,
            "7" => VimDigit7,
            "8" => VimDigit8,
            "9" => VimDigit9,
            "ctrl-z" => Undo,
            "ctrl-r" => Redo,
            "ctrl-shift-z" => Redo,
            "ctrl-y" => Redo,
            "ctrl-shift-c" => CopyText,
            "ctrl-shift-v" => PasteText,
        ]);
    }
    // Normal mode only: `u` is undo here (vim uses it in visual for
    // lowercasing), and `enter` is the next-line motion rather than a newline.
    // The table edits (`DD` deletes a column, `|` and `\` insert one) are
    // normal-mode commands; in visual mode `D` is the selection operator.
    // `alt+hjkl` moves a whole row or column of the table under the cursor.
    bind!("vim-normal", [
        "u" => VimU,
        "." => VimDot,
        "enter" => NextLine,
        "shift-d" => VimUpperD,
        "|" => VimPipe,
        "\\" => VimBackslash,
        "alt-h" => VimAltH,
        "alt-j" => VimAltJ,
        "alt-k" => VimAltK,
        "alt-l" => VimAltL,
        // Folding (`za` toggle, `zc` close, `zo` open, `zd` remove, `zM` close
        // all, `zR`/`zE` open all). `z` opens the command and the second key is
        // read by the vim engine; `a`, `c`, `o` and `d` are already bound, so
        // only the shifted command letters need their own actions.
        "z" => VimZ,
        "shift-r" => VimUpperR,
        "shift-m" => VimUpperM,
        "shift-e" => VimUpperE,
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
        "ctrl-z" => Undo,
        "ctrl-r" => Redo,
        "ctrl-shift-z" => Redo,
        "ctrl-y" => Redo,
        "ctrl-shift-c" => CopyText,
        "ctrl-shift-v" => PasteText,
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
        let editor_focus = workspace.read(cx).editor.read(cx).focus_handle(cx);
        window
            .update(cx, |_ws, window, cx| window.focus(&editor_focus, cx))
            .unwrap();

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
