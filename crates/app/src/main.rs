//! Shale: an Obsidian-style markdown editor built on GPUI.

mod editor;
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
use shale_core::{Config, Vault};

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
        let editor_events = cx.subscribe(&editor, |ws, _editor, event, cx| match event {
            EditorEvent::FollowLink { target } => ws.follow_link(target, cx),
        });
        // Keybinding bugs on Linux are almost always "what did the platform
        // actually deliver, and which action did it resolve to", so log both.
        // The subscription has to be kept alive or the observer is dropped.
        let keystroke_observer = cx.observe_keystrokes(|_ws, event, _window, _cx| {
            tracing::debug!(
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
        for node in &tree {
            push_node(&mut rows, node, 0, &active, expanded, &mut idx, cx);
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
    // Normal mode only: `u` is undo here (vim uses it to lowercase in visual
    // mode), and `enter` is the next-line motion rather than a newline.
    bind!("vim-normal", [
        "u" => VimU,
        "enter" => NextLine,
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
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};

    /// Typing printable characters must reach the buffer through gpui's input
    /// handler, not just the keys we bind to actions.
    #[gpui::test]
    fn typing_in_insert_mode_inserts_text(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();

        cx.simulate_keystrokes("i");
        cx.simulate_input("hello");

        let (mode, text) = editor
            .read_with(cx, |editor, _cx| (editor.mode(), editor.text()))
            .unwrap();
        assert_eq!(
            mode,
            shale_core::vim::Mode::Insert,
            "`i` should enter insert"
        );
        assert_eq!(text, "hello", "typed characters should land in the buffer");

        // Normal mode swallows typed characters instead of inserting them.
        cx.simulate_keystrokes("escape");
        cx.simulate_input("xyz");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "hello", "normal mode should swallow typed input");
    }

    /// Soft wrap: a long row must break into several display lines, and `j`
    /// has to step across them (neovim-style) even though it is one buffer row.
    #[gpui::test]
    fn soft_wrap_steps_across_display_lines(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                editor.set_text("word ".repeat(200), cx);
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();

        // The first frame has no measured width yet, so paint twice.
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        let (rows, displays) = editor
            .read_with(cx, |editor, _cx| {
                (editor.line_count(), editor.display_line_count())
            })
            .unwrap();
        assert_eq!(rows, 1, "the note is a single buffer row");
        // 1000 characters at ~150 chars per line in a maximized test window.
        assert!(
            (4..=12).contains(&displays),
            "a long row should wrap into a sensible number of display lines, got {displays}"
        );

        cx.simulate_keystrokes("j");
        let (row, col) = editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap();
        assert_eq!(row, 0, "still the same buffer row");
        assert!(
            (40..1000).contains(&col),
            "j should land on the next display line, got col {col}"
        );
    }

    /// The Linux platform delivers ctrl+z as `key = "z"` with the control
    /// modifier and *no* `key_char` (control characters are filtered out of
    /// `key_char` by `keystroke_from_xkb`). Check that our keymap really matches
    /// that shape, independent of any window or focus state. This is the test
    /// that catches modifier syntax mistakes: `simulate_keystrokes("ctrl+z")`
    /// parses through the same parser as the binding, so a typo like `+`
    /// matches itself and looks fine.
    #[test]
    fn keymap_matches_the_linux_keystroke_shape() {
        let keymap = gpui::Keymap::new(keybindings());
        let contexts = vec![gpui::KeyContext::try_from("vim-normal").unwrap()];
        for (control, key, action) in [
            (true, "z", "shale_editor::Undo"),
            (true, "r", "shale_editor::Redo"),
            (true, "y", "shale_editor::Redo"),
            (true, "s", "shale::Save"),
            (false, "k", "shale_editor::VimK"),
            (false, "$", "shale_editor::VimDollar"),
            (false, "^", "shale_editor::VimCaret"),
        ] {
            let typed = gpui::Keystroke {
                modifiers: gpui::Modifiers {
                    control,
                    ..Default::default()
                },
                key: key.to_string(),
                key_char: None,
            };
            let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
            assert!(!pending, "{key} should not leave pending input");
            assert_eq!(
                bindings
                    .iter()
                    .map(|b| b.action().name())
                    .collect::<Vec<_>>(),
                vec![action],
                "typed {key} (control {control}) should resolve to {action}"
            );
        }

        // The visual context has to carry the same bindings, or the keys stop
        // working the moment `v` is typed.
        let contexts = vec![gpui::KeyContext::try_from("vim-visual").unwrap()];
        for (control, shift, key, action) in [
            (false, false, "k", "shale_editor::VimK"),
            (false, false, "v", "shale_editor::VimV"),
            (false, true, "v", "shale_editor::VimUpperV"),
            (false, false, "$", "shale_editor::VimDollar"),
            (true, false, "z", "shale_editor::Undo"),
        ] {
            let typed = gpui::Keystroke {
                modifiers: gpui::Modifiers {
                    control,
                    shift,
                    ..Default::default()
                },
                key: key.to_string(),
                key_char: None,
            };
            let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
            assert!(!pending, "{key} should not leave pending input");
            assert_eq!(
                bindings
                    .iter()
                    .map(|b| b.action().name())
                    .collect::<Vec<_>>(),
                vec![action],
                "visual mode: typed {key} should resolve to {action}"
            );
        }
    }

    /// Diagnostic: compare the wrap model against what shaping actually
    /// produces, to see whether painted segments overflow the wrap width.
    #[gpui::test]
    fn wrap_model_versus_shaped_widths(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                editor.set_text(
                    "The quick brown fox jumps over the lazy dog. ".repeat(20),
                    cx,
                );
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }

        let (ranges, width, text) = editor
            .read_with(cx, |editor, _cx| {
                (editor.display_ranges(), editor.wrap_width(), editor.text())
            })
            .unwrap();
        let painted = editor
            .read_with(cx, |editor, _cx| editor.painted_lines())
            .unwrap();

        cx.update(|window, _cx| {
            let ts = window.text_system();
            let shaped = |s: &str| {
                let run = gpui::TextRun {
                    len: s.len(),
                    font: gpui::font("monospace"),
                    color: gpui::black().into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                ts.shape_line(s.to_string().into(), px(editor::FONT_SIZE), &[run], None)
                    .width()
            };
            println!("wrap width = {width:?}; display lines = {}", ranges.len());
            println!("painted lines from the last paint ({}):", painted.len());
            let mut overflow = 0;
            for (s, e, w) in &painted {
                let expected = shaped(&text[*s..*e]);
                if *w > width {
                    overflow += 1;
                }
                let flag = if *w == expected { "ok" } else { "MISMATCH" };
                println!("  bytes {s}..{e} painted {w:?} vs monospace {expected:?} {flag}");
            }
            assert_eq!(overflow, 0, "no painted segment may exceed the wrap width");
        });
    }

    /// The wrap model must stay accurate for the text a markdown note actually
    /// contains: tabs, accents, CJK (which wrap without spaces) and emoji.
    #[gpui::test]
    fn wrap_stays_accurate_for_tabs_and_unicode(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                editor.set_text(
                    "indented\tcode\tand héllo wörld 你好世界 🎉 emoji ".repeat(30),
                    cx,
                );
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
        }

        let (ranges, width, text) = editor
            .read_with(cx, |editor, _cx| {
                (editor.display_ranges(), editor.wrap_width(), editor.text())
            })
            .unwrap();
        let painted = editor
            .read_with(cx, |editor, _cx| editor.painted_lines())
            .unwrap();
        assert_eq!(ranges.len(), painted.len(), "model and paint agree");
        // The buffer's display-line index space must be the painted one, or the
        // cursor and `j`/`k` land on a different row than the user sees.
        for (model, paint) in ranges.iter().zip(&painted) {
            assert_eq!(
                (model.0, model.1),
                (paint.0, paint.1),
                "the buffer's display lines must be exactly the painted ones"
            );
        }

        cx.update(|window, _cx| {
            let ts = window.text_system();
            let shaped = |s: &str| {
                let run = gpui::TextRun {
                    len: s.len(),
                    font: gpui::font("monospace"),
                    color: gpui::black().into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                ts.shape_line(s.to_string().into(), px(editor::FONT_SIZE), &[run], None)
                    .width()
            };
            let mut widest = px(0.);
            for (i, (s, e, w)) in painted.iter().enumerate() {
                widest = widest.max(*w);
                assert!(
                    *w <= width,
                    "display line {i} is {w:?} wide, over the {width:?} wrap width"
                );
                // The painted line must measure exactly what the wrapper measured
                // for the same bytes, or the break points drift away from what
                // the user sees.
                assert_eq!(
                    *w,
                    shaped(&text[*s..*e]),
                    "display line {i} was painted at a different width than the wrap model measured"
                );
            }
            println!(
                "{} display lines, widest {widest:?} of {width:?}",
                painted.len()
            );
            // Every character must be reachable: the segments have to tile the row.
            let ranges2 = ranges.clone();
            assert_eq!(ranges2.first().unwrap().0, 0);
            assert_eq!(ranges2.last().unwrap().1, text.len());
            for pair in ranges2.windows(2) {
                assert_eq!(pair[0].1, pair[1].0, "segments must tile the row");
            }
        });
    }

    /// Counts, `$`, and `u` must work through the real keymap, not just the
    /// core engine's unit tests.
    #[gpui::test]
    fn ctrl_z_and_ctrl_r_undo_redo(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                editor.set_text("start".to_string(), cx);
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();

        cx.simulate_keystrokes("i");
        cx.simulate_input("abc");
        cx.simulate_keystrokes("escape");
        assert_eq!(
            editor.read_with(cx, |editor, _cx| editor.text()).unwrap(),
            "abcstart"
        );

        // `ctrl-z` (dashes) is what the platform actually delivers: key "z" with
        // the control modifier. `ctrl+z` would parse as one literal key and
        // match a `ctrl+z` binding, hiding the mistake.
        cx.simulate_keystrokes("ctrl-z");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "start", "ctrl-z should undo");

        cx.simulate_keystrokes("ctrl-r");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "abcstart", "ctrl-r should redo");

        // ctrl+shift+z and ctrl+y are redo aliases too.
        cx.simulate_keystrokes("ctrl-z");
        cx.simulate_keystrokes("ctrl-shift-z");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "abcstart", "ctrl-shift-z should redo");

        cx.simulate_keystrokes("ctrl-z ctrl-y");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "abcstart", "ctrl-y should redo");
    }

    /// `f`/`t` and space must work through the real keymap: space is both vim's
    /// `l` alias and the argument of `f `.
    #[gpui::test]
    fn normal_mode_find_motions(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                editor.set_text("one two three four\nsecond line".to_string(), cx);
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();

        cx.simulate_keystrokes("0 f r");
        let pos = editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap();
        assert_eq!(pos, (0, 10), "`fr` should land on the r of three");

        cx.simulate_keystrokes("0 f space");
        let pos = editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap();
        assert_eq!(pos, (0, 3), "`f ` should land on the first space");

        cx.simulate_keystrokes("0 2 space");
        let pos = editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap();
        assert_eq!(pos, (0, 2), "space should move right like `l`");

        cx.simulate_keystrokes("0 d f r");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "ee four\nsecond line", "`dfr` deletes through the r");
    }

    #[gpui::test]
    fn normal_mode_counts_and_edits(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                editor.set_text("one two three four\nsecond line".to_string(), cx);
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();

        cx.simulate_keystrokes("d 2 w");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(
            text, "three four\nsecond line",
            "`d2w` should delete two words"
        );

        // `u` undoes it again.
        cx.simulate_keystrokes("u");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "one two three four\nsecond line", "`u` should undo");

        // `$` lands on the last character, and `2j` moves two display lines
        // down (clamped at the last line here).
        cx.simulate_keystrokes("0 $");
        let pos = editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap();
        assert_eq!(pos, (0, 17), "`$` should land on the last char");

        cx.simulate_keystrokes("^");
        let pos = editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap();
        assert_eq!(pos, (0, 0), "`^` should land on the first non-blank");

        // `cw` replaces a word and enters insert mode.
        cx.simulate_keystrokes("escape 0 c w");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(
            text, "two three four\nsecond line",
            "`cw` should delete the word"
        );
        cx.simulate_input("X");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "Xtwo three four\nsecond line");
    }

    /// Text objects through the real keymap: `i`/`a` are bound actions and the
    /// scope key arrives as the following keystroke.
    #[gpui::test]
    fn normal_mode_text_objects(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                editor.set_text("one two three\n\nnext para".to_string(), cx);
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();

        cx.simulate_keystrokes("0 d i w");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(
            text, " two three\n\nnext para",
            "`diw` should delete just the word"
        );

        // A fresh buffer, so each form is checked from the same starting point.
        editor
            .update(cx, |editor, _window, cx| {
                editor.set_text("one two three\n\nnext para".to_string(), cx);
            })
            .unwrap();
        cx.simulate_keystrokes("escape 0 d a w");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(
            text, "two three\n\nnext para",
            "`daw` should delete the trailing space too"
        );

        cx.simulate_keystrokes("escape 0 c i p");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(
            text, "\n\nnext para",
            "`cip` should delete the whole paragraph"
        );
        cx.simulate_input("X");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "X\n\nnext para", "`cip` should enter insert mode");
    }

    /// Visual mode through the real keymap: the `vim-visual` context has to
    /// carry the motion bindings, and operators act on the selection.
    #[gpui::test]
    fn visual_mode_selection_and_operators(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(keybindings()));

        let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
        let cx = &mut VisualTestContext::from_window(editor.into(), cx);
        editor
            .update(cx, |editor, window, cx| {
                editor.set_text("one two three\nsecond para\nthird".to_string(), cx);
                let focus = editor.focus_handle(cx);
                window.focus(&focus, cx);
            })
            .unwrap();

        cx.simulate_keystrokes("v l l");
        let mode = editor.read_with(cx, |editor, _cx| editor.mode()).unwrap();
        assert_eq!(
            mode,
            shale_core::vim::Mode::Visual,
            "`v` should enter visual mode"
        );

        cx.simulate_keystrokes("d");
        let (mode, text) = editor
            .read_with(cx, |editor, _cx| (editor.mode(), editor.text()))
            .unwrap();
        assert_eq!(
            text, " two three\nsecond para\nthird",
            "`d` should delete the selection"
        );
        assert_eq!(mode, shale_core::vim::Mode::Normal, "and leave visual mode");

        // Linewise visual: `V` + `j` + `d` removes two whole lines.
        cx.simulate_keystrokes("escape shift-v j d");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "third", "`Vjd` should delete two lines");

        // `y` + `p` in visual swaps the selection for the register.
        cx.simulate_keystrokes("escape 0 v l l y");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "third", "yanking leaves the text alone");
        cx.simulate_keystrokes("$ h v l p");
        let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
        assert_eq!(text, "thithi", "`p` should replace the selection");
    }
}
