//! The graph tab's tests (roadmap P3.4), in four files: `tab.rs` for the tab *kind*
//! (Task 5), `canvas.rs` for the picture and the layout under it (Task 6),
//! `interactions.rs` for the pointer and the keyboard (Task 7), and `filter.rs` for the
//! strip that decides what the picture holds (Task 8).
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
    // The index scan runs on the background executor, and gpui's test harness only pumps
    // that executor when a keystroke is simulated. A test that opened the graph some
    // other way — a session restore, an MCP call — has to pump it here, or it paints a
    // graph that is still on its way.
    cx.run_until_parked();
    window
        .update(cx, |ws, _window, cx| ws.settle_graph(cx))
        .unwrap();
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// Draw frames *and* deliver the next-frame callbacks they schedule, which is what a
/// real window does and `draw().clear()` on its own does not. The keyboard and the hover
/// move to the tab on screen one frame after the tab does (`Workspace::focus_stale`), so
/// a test that presses a key straight after a tab change without this is pressing it
/// against a view that has already left the tree — the keystroke goes nowhere.
fn repaint(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        // Not through a window handle: the callback updates the workspace, and an
        // update of the workspace from inside one is a double borrow.
        cx.update(|window, cx| window.simulate_next_frame(cx));
    }
}

/// Wait for the index scan to land, for the tests that reach the graph without pressing
/// a key. The scan runs on the background executor, and a simulated keystroke is what
/// pumps it in gpui's test harness; without this, the assertions below are about a graph
/// that is still being read off the disk.
fn scan_landed(cx: &mut VisualTestContext) {
    cx.run_until_parked();
}

/// The round things the last frame painted. A quad that is square with its corners
/// rounded to half its width is a disc, which is how a node is drawn — and the only
/// discs in a window that is showing the graph are the graph's. The tolerances are
/// loose because a painted quad is recorded in *scaled* pixels, which rounds: a node
/// drawn 43.25 px wide at a 2x scale factor comes back as 87 by 86.
///
/// Shared by the canvas and the filter tests, which is why it lives here: a helper in
/// one of the child modules is invisible to the others, and a name imported through
/// `use super::*` would collide with the module of the same name.
fn circles(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Vec<gpui::Quad> {
    window
        .update(cx, |_ws, window, _cx| {
            window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    let w = quad.bounds.size.width.as_f32();
                    let h = quad.bounds.size.height.as_f32();
                    w > 0.0
                        && (w - h).abs() <= 1.0
                        && (quad.corner_radii.top_left.as_f32() * 2.0 - w).abs() <= 2.0
                })
                .collect()
        })
        .unwrap()
}

/// How many of the painted discs were filled with one of `colors`.
fn filled(quads: &[gpui::Quad], colors: &[u32]) -> usize {
    let wanted: Vec<gpui::Background> = colors
        .iter()
        .map(|color| gpui::Background::from(gpui::rgb(*color)))
        .collect();
    quads
        .iter()
        .filter(|quad| wanted.contains(&quad.background))
        .count()
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
mod filter;
mod interactions;
// Timing sweeps, every test in it `#[ignore]`d: see the module's own note on how to run.
mod paint_bench;
mod tab;
