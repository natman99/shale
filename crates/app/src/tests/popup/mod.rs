//! The panel the checker puts over the note: vim's `K`, a resting pointer, one
//! popup at a time, and closing properly.
//!
//! The notes are put into the window's hands directly rather than through a
//! server: the panel's behaviour — when it opens, that only one is ever up, how
//! it gets out of the way — must not depend on a `harper-ls` process being
//! installed, and the protocol itself is covered by `crate::lsp`'s tests.
//!
//! The fixtures the two files share live here. `keyboard` asks about `K`, the
//! command line and the offers list; `pointer` asks about a resting mouse, which
//! is the harder half, because the panel has to survive the trip from the word to
//! itself and get out of the way of the next word.

use super::*;
use crate::lsp::{Diagnostic, Fix};

mod keyboard;
mod pointer;

/// Put the checker's notes in the window's hands, as though the server had just
/// published them.
fn notes_from(notes: &[(&str, usize, usize)]) -> Vec<Diagnostic> {
    notes
        .iter()
        .map(|(message, start, end)| Diagnostic {
            start: *start,
            end: *end,
            message: message.to_string(),
            code: "SpellCheck".into(),
        })
        .collect()
}

/// Two frames, so the note's lines have painted positions to measure against.
fn paint(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// Rest the pointer at a window position, as gpui delivers a move: no button
/// held, no modifiers down.
fn hover_point(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
}

/// Rest the pointer on buffer offset `offset` of display line `line`, as gpui
/// delivers a move: no button held, no modifiers down.
fn hover_at(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    line: usize,
    offset: usize,
) {
    let at = pointer_over(cx, window, line, offset);
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
}

/// A note open, drawn, and marked by the checker at `(start, end)` with a message.
fn marked_note(
    cx: &mut TestAppContext,
    text: &str,
    notes: &[(&str, usize, usize)],
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let (dir, window) = vault_with(cx, &[("a.md", text)]);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx);
            ws.set_checker_notes(notes_from(notes), cx);
        })
        .unwrap();
    (dir, window)
}

/// Whether a panel is up.
fn shown(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> bool {
    window
        .update(cx, |ws, _window, _cx| ws.has_popup())
        .unwrap()
}

/// What the panel says, if one is up.
fn saying(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Option<String> {
    window
        .update(cx, |ws, _window, cx| ws.popup_message(cx))
        .unwrap()
}

/// What the panel offers, in order.
fn offers(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.popup_rows(cx))
        .unwrap()
}

/// The note's text, as the editor holds it.
fn text(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> String {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().read_with(cx, |editor, _cx| editor.text())
        })
        .unwrap()
}

/// Where display line `line` was painted: its left edge, its top and its bottom,
/// in window coordinates.
fn line_box(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    line: usize,
) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels) {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().read_with(cx, |editor, _cx| {
                let (left, _right) = editor.hit_line_x(line).unwrap();
                let (top, bottom) = editor.hit_line_edges(line).unwrap();
                (left, top, bottom)
            })
        })
        .unwrap()
}

/// The window x of buffer offset `offset` on display line `line`.
fn x_over(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    line: usize,
    offset: usize,
) -> gpui::Pixels {
    let (left, _top, _bottom) = line_box(cx, window, line);
    left + window
        .update(cx, |ws, _window, cx| {
            ws.editor()
                .read_with(cx, |editor, _cx| editor.x_for_offset(line, offset))
        })
        .unwrap()
        .unwrap()
}

/// Where the pointer has to be to rest on buffer offset `offset` of display line
/// `line`: the line's left edge plus where that offset was painted on it.
fn pointer_over(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    line: usize,
    offset: usize,
) -> gpui::Point<gpui::Pixels> {
    let (_left, top, bottom) = line_box(cx, window, line);
    gpui::point(x_over(cx, window, line, offset), (top + bottom) / 2.)
}

/// Where the panel was drawn: the y of its bottom edge, in window coordinates.
fn panel_bottom(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> gpui::Pixels {
    window
        .update(cx, |ws, window, cx| {
            ws.popup_place(window, cx)
                .expect("the panel has a place")
                .bounds
                .bottom()
        })
        .unwrap()
}

/// Whether the note is what holds the keyboard.
fn note_has_keys(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> bool {
    window
        .update(cx, |ws, window, cx| {
            ws.editor().read(cx).focus_handle.is_focused(window)
        })
        .unwrap()
}

/// How tall the panel is, as the window measures it — the height the pointer's
/// corridor is built from, and the height the render has to deliver.
fn panel_height(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> gpui::Pixels {
    window
        .update(cx, |ws, window, cx| {
            ws.popup_place(window, cx)
                .expect("the panel has a place")
                .bounds
                .size
                .height
        })
        .unwrap()
}

/// The note's text size, which is what sets the panel's row height.
fn text_size(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> f32 {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().read_with(cx, |editor, _cx| editor.font_size())
        })
        .unwrap()
}

/// Which offer rows the panel is drawing.
fn drawn_rows(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
) -> std::ops::Range<usize> {
    window
        .update(cx, |ws, _window, cx| ws.popup_drawn(cx))
        .unwrap()
}

/// Put `n` offers in front of the panel, the way a checker with opinions would.
fn give_offers(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>, n: usize) {
    window
        .update(cx, |ws, _window, cx| {
            ws.offer_test_fixes(
                (0..n)
                    .map(|i| Fix {
                        title: format!("Offer {}", i + 1),
                        edits: Vec::new(),
                        command: None,
                    })
                    .collect(),
                cx,
            )
        })
        .unwrap();
}

/// A note long enough that its marked word sits against the bottom of the window.
///
/// Returns the offset of the marked word and its display line. The note ends with
/// a newline, so vim's `G` lands on the empty line *after* it; `G k` is what puts
/// the cursor on the marked line, which is the last one with text on it.
fn bottom_note(
    cx: &mut TestAppContext,
) -> (tempfile::TempDir, WindowHandle<Workspace>, usize, usize) {
    let mut text: String = (1..=119)
        .map(|n| format!("line {n} of the note\n"))
        .collect();
    text.push_str("and the wrogn word at the bottom\n");
    let start = text.find("wrogn").expect("the marked word is there");
    let (_dir, window) = marked_note(
        cx,
        &text,
        &[("Did you mean to spell `wrogn` this way?", start, start + 5)],
    );
    (_dir, window, start, 119)
}

/// Where the panel is: which side of its line it hangs on, and its top and bottom.
fn place(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
) -> (bool, gpui::Pixels, gpui::Pixels) {
    window
        .update(cx, |ws, window, cx| {
            let placed = ws.popup_place(window, cx).expect("a place");
            (placed.below, placed.bounds.top(), placed.bounds.bottom())
        })
        .unwrap()
}
