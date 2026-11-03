//! The graph tab's tests (roadmap P3.4), in three files: `tab.rs` for the tab *kind*
//! (Task 5), `canvas.rs` for the picture and the layout under it (Task 6), and
//! `interactions.rs` for the pointer and the keyboard (Task 7).
//!
//! What these pin is the shape the integration contract chose: the graph is a **tab
//! kind**, not a note the window pretends to have, and there is **one per window**
//! because it is a simulation rather than a buffer. So the interesting assertions are
//! often about what stops happening — the note's keys stay in the note, the graph tab
//! is never converted by opening a link, and the note's status segments go quiet
//! rather than stale.
//!
//! The helpers here are shared by all three; the ones that belong to one file stay in
//! that file.

use super::*;
use crate::graph::GraphSummary;
use shale_core::session::{Session, Tab};

/// The tabs as `(label, dirty)`, with the graph tab drawn as the bar draws it.
fn tabs(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<(String, bool)> {
    window
        .update(cx, |ws, _window, cx| {
            ws.tab_paths(cx)
                .into_iter()
                .map(|(path, dirty)| {
                    (
                        path.map(|path| path.display().to_string())
                            .unwrap_or_else(|| "—".to_string()),
                        dirty,
                    )
                })
                .collect()
        })
        .unwrap()
}

/// Which tab is on screen.
fn marker(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, _cx| ws.tab_index())
        .unwrap()
}

/// Whether the tab on screen is the graph.
fn graph(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |ws, _window, _cx| ws.showing_graph())
        .unwrap()
}

/// How many graph tabs the window holds.
fn graph_tabs(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, _cx| ws.graph_tabs())
        .unwrap()
}

/// The status line.
fn say(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// The buffer of the tab on screen.
fn buffer(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap()
}

/// Open a note through the same door the tree and the picker use.
fn show<C: gpui::AppContext>(cx: &mut C, window: &WindowHandle<Workspace>, note: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(note), cx))
        .unwrap();
}

/// Type a command line and press enter.
fn command_line(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// A vault of three notes, `a.md` open: `a` links to `b`, to a note that is not there,
/// and carries a tag; `b` links back; `c` links nothing.
///
/// The graph that makes is 3 notes, 1 ghost, 1 tag and 3 links — `a`→`b` and `b`→`a`
/// being one edge, which is what the counts test reads back.
fn linked_vault(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(
        cx,
        &[
            ("a.md", "see [[b]] and [[gone]] and #tag\n"),
            ("b.md", "back to [[a]]\n"),
            ("c.md", "alone\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    (dir, window, cx)
}

/// Drive the layout to rest and draw the frames it needs. The first frame of a window
/// has no measured size yet — every paint test here has found that — so the camera's
/// first fit is thrown away and the next one is the real thing.
fn paint(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) {
    window
        .update(cx, |ws, _window, cx| ws.settle_graph(cx))
        .unwrap();
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}
/// The positions the graph tab is painting, in slot order. What they are is the sim's
/// business; what matters here is whether they *moved*.
fn positions(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Vec<[f32; 2]> {
    window
        .update(cx, |ws, _window, cx| ws.graph_positions(cx))
        .unwrap()
}
/// The positions the simulation holds, rather than the ones the tab is painting. A drag
/// moves the sim; the frame follows on the next tick.
fn sim_positions(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Vec<[f32; 2]> {
    window
        .update(cx, |ws, _window, _cx| ws.graph_sim_positions())
        .unwrap()
}

mod canvas;
mod interactions;
mod tab;
