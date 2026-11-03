//! The graph tab (roadmap P3.4, `graph-view-plan.md` M1b Task 5).
//!
//! What these pin is the shape the integration contract chose: the graph is a **tab
//! kind**, not a note the window pretends to have, and there is **one per window**
//! because it is a simulation rather than a buffer. So the interesting assertions are
//! about what stops happening — the note's keys stay in the note, the graph tab is
//! never converted by opening a link, and the note's status segments go quiet rather
//! than stale.

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

/// `space g` opens the graph in a tab of its own, beside the note rather than in place
/// of it: the graph is a view over the whole vault, and losing the note to reach it
/// would make it a thing to leave rather than a thing to look at.
#[gpui::test]
fn space_g_opens_the_graph_in_a_tab_of_its_own(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    assert_eq!(tabs(&window, &mut cx), vec![("a.md".into(), false)]);
    assert!(!graph(&window, &mut cx), "the window starts on the note");

    cx.simulate_keystrokes("space g");
    assert!(graph(&window, &mut cx), "the graph is on screen");
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), false), ("—".into(), false)],
        "the note is still on the bar, and the graph is a second tab"
    );
    assert_eq!(
        marker(&window, &mut cx),
        1,
        "and the graph is the one on screen"
    );
    assert_eq!(say(&window, &mut cx), "graph", "the status line says so");
}

/// One graph tab per window, which is the contract's, because the graph is a
/// simulation and a second tab would be a second view of the same physics with its own
/// camera to keep in step. Re-opening it moves the frame instead.
#[gpui::test]
fn the_graph_is_one_tab_per_window(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    // Out and back: `escape` returns to the note, and `space g` again must find the
    // tab that is already there rather than opening another.
    cx.simulate_keystrokes("escape");
    assert!(!graph(&window, &mut cx));
    cx.simulate_keystrokes("space g");
    assert!(graph(&window, &mut cx));
    assert_eq!(
        graph_tabs(&window, &mut cx),
        1,
        "the second `space g` moved the frame, it did not add a tab"
    );
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), false), ("—".into(), false)],
        "the bar still has two tabs"
    );

    // The command line is the other door, and reaches the same one tab.
    cx.simulate_keystrokes("escape");
    command_line("graph", &mut cx);
    assert_eq!(graph_tabs(&window, &mut cx), 1);
    assert!(graph(&window, &mut cx));
}

/// `escape` in the graph is `note.focus-editor`: the same request `ctrl-w l` makes. It
/// cannot hand the keyboard to an editor that is not drawn, so it moves the frame back
/// to the note tab the user was last on.
#[gpui::test]
fn escape_leaves_the_graph_for_the_note(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    assert!(graph(&window, &mut cx));

    cx.simulate_keystrokes("escape");
    assert!(!graph(&window, &mut cx), "the note is on screen again");
    assert_eq!(marker(&window, &mut cx), 0);
    assert_eq!(
        buffer(&window, &mut cx),
        "see [[b]] and [[gone]] and #tag\n"
    );
    // The keyboard came with the frame. A keystroke that went to the graph's context
    // after the graph had left the screen would be the same bug the tab tests call
    // out for a plain tab change.
    cx.simulate_keystrokes("i");
    cx.simulate_input(" typed");
    // The cursor is one character in because `space g` began with vim's `space`
    // motion, which moves right; what matters is that the typing landed here.
    assert!(
        buffer(&window, &mut cx).contains(" typed"),
        "and the keyboard came back with it"
    );
}

/// The note's status segments go quiet on the graph tab rather than keeping the last
/// note's numbers: a bar that still reports a cursor position for a screen with no
/// cursor is the stale state the bar's segments are arranged to avoid.
#[gpui::test]
fn the_graph_tab_reports_no_note_state(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    // Make the note dirty, so the dot is lit before the graph arrives.
    cx.simulate_keystrokes("A typed");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.dirty_indicator(cx))
            .unwrap(),
        "●",
        "the note is ahead of its file"
    );
    cx.simulate_keystrokes("escape");

    cx.simulate_keystrokes("space g");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.dirty_indicator(cx))
            .unwrap(),
        "",
        "the graph tab has no save state to show"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.word_badge(cx))
            .unwrap(),
        "",
        "and no word count"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.mode_label(cx))
            .unwrap(),
        "",
        "and no vim mode, because the graph does not have one"
    );
    // The dirty note is still dirty behind the graph — quiet is not cleared.
    assert!(
        window
            .update(&mut cx, |ws, _window, cx| ws.any_tab_is_dirty(cx))
            .unwrap(),
        "the note is still ahead of its file, just not on screen"
    );
}

/// Opening a note never converts the graph tab. The rule the contract ruled: go to the
/// tab that holds it, else the most recently used note tab, else a new tab.
#[gpui::test]
fn opening_a_note_leaves_the_graph_tab_alone(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    assert!(graph(&window, &mut cx));

    show(&mut cx, &window, "b.md");
    assert!(
        !graph(&window, &mut cx),
        "the note is on screen, in a note tab"
    );
    assert_eq!(
        graph_tabs(&window, &mut cx),
        1,
        "the graph tab is still there, unconverted"
    );
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("b.md".into(), false), ("—".into(), false)],
        "the note went into the note tab, and the graph is still the second tab"
    );
    assert_eq!(
        buffer(&window, &mut cx),
        "back to [[a]]\n",
        "the note went to the tab the frame was last on"
    );

    // And the graph is still where it was, ready to come back. The frame is moved by
    // the window rather than by a keystroke: `open_note` moved it without a window to
    // hand the keyboard with, so the next frame is what re-points the keys, and a test
    // that pressed a key here would be testing a frame it never forced.
    window
        .update(&mut cx, |ws, _window, cx| ws.show_tab_at(1, cx))
        .unwrap();
    assert!(graph(&window, &mut cx));
    assert_eq!(graph_tabs(&window, &mut cx), 1);
}

/// Closing the graph tab leaves a note tab, and the window keeps its at-least-one-tab
/// invariant. The graph has no note to write, so nothing about closing it can fail.
#[gpui::test]
fn closing_the_graph_tab_leaves_the_note(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    assert_eq!(tabs(&window, &mut cx).len(), 2);

    window
        .update(&mut cx, |ws, _window, cx| ws.close_active_tab(cx))
        .unwrap();
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), false)],
        "the graph tab is gone and the note tab is not"
    );
    assert!(!graph(&window, &mut cx));
    assert_eq!(
        buffer(&window, &mut cx),
        "see [[b]] and [[gone]] and #tag\n",
        "the note is on screen"
    );
}

/// Closing the *last* tab while it is the graph leaves an empty note tab rather than
/// an empty graph tab: the thing to fall back to is a place to write.
#[gpui::test]
fn closing_the_only_graph_tab_leaves_a_place_to_write(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "first\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    // The graph, added beside the empty tab the window starts with.
    window
        .update(&mut cx, |ws, _window, cx| ws.open_graph(cx))
        .unwrap();
    assert_eq!(tabs(&window, &mut cx).len(), 2);
    // Close the empty note tab, so the graph is the only tab the window has.
    window
        .update(&mut cx, |ws, _window, cx| ws.show_tab_at(0, cx))
        .unwrap();
    window
        .update(&mut cx, |ws, _window, cx| ws.close_active_tab(cx))
        .unwrap();
    assert_eq!(tabs(&window, &mut cx), vec![("—".into(), false)]);
    assert!(graph(&window, &mut cx), "the graph is the only tab");

    window
        .update(&mut cx, |ws, _window, cx| ws.close_active_tab(cx))
        .unwrap();
    assert!(
        !graph(&window, &mut cx),
        "the last tab became an empty note tab, not an empty graph"
    );
    assert_eq!(tabs(&window, &mut cx), vec![("—".into(), false)]);
}

/// The panel shows the vault, not a placeholder: the counts the window built come from
/// the same index the canvas will be drawn from.
#[gpui::test]
fn the_panel_shows_the_vault(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.graph_counts(cx))
            .unwrap(),
        Some(GraphSummary {
            notes: 3,
            ghosts: 1,
            tags: 1,
            // `a`→`b` and `b`→`a` are one edge, not two: the graph is undirected and
            // never holds parallel edges.
            links: 3,
        }),
        "three notes, one missing target, one tag, three links"
    );
}

/// The graph follows the vault. A note created outside Shale arrives through the
/// watcher, and the panel's counts move with it — which is what lets the canvas, once
/// it is drawn, be looked at while the vault is being edited somewhere else.
#[gpui::test]
fn the_graph_follows_the_vault(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    let before = window
        .update(&mut cx, |ws, _window, cx| ws.graph_counts(cx))
        .unwrap()
        .expect("the graph is open, so it has counts");

    std::fs::write(dir.path().join("d.md"), "new, and [[gone]]\n").unwrap();
    window
        .update(&mut cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();

    let after = window
        .update(&mut cx, |ws, _window, cx| ws.graph_counts(cx))
        .unwrap();
    assert_eq!(
        after,
        Some(GraphSummary {
            notes: before.notes + 1,
            ghosts: 1,
            tags: 1,
            links: before.links + 1,
        }),
        "the new note joined, and its link to the ghost it shares added one edge"
    );
}

/// A graph tab is remembered by the session and comes back as a graph tab, with its
/// index rebuilt. The camera and filter land with Task 6; this is the tab kind
/// surviving a restart, which is what makes the graph a place the user can leave the
/// window on.
#[gpui::test]
fn a_session_reopens_the_graph_tab(cx: &mut TestAppContext) {
    let session = Session {
        open_note: Some(PathBuf::from("a.md")),
        cursor: None,
        scroll_y: None,
        expanded: vec![],
        view: Some("live".into()),
        focus: None,
        focus_scope: None,
        typewriter: None,
        jumps_back: vec![],
        jumps_forward: vec![],
        tabs: vec![
            Tab {
                path: Some(PathBuf::from("a.md")),
                view: "live".into(),
                graph: false,
            },
            Tab {
                path: None,
                view: String::new(),
                graph: true,
            },
        ],
        active_tab: Some(1),
    };
    let (_dir, window) = session::vault_with_session(
        cx,
        &[("a.md", "see [[b]]\n"), ("b.md", "back to [[a]]\n")],
        &session,
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert_eq!(
        tabs(&window, &mut cx),
        vec![("a.md".into(), false), ("—".into(), false)],
        "both tabs came back, in order"
    );
    assert_eq!(marker(&window, &mut cx), 1, "and the one with the keyboard");
    assert!(
        graph(&window, &mut cx),
        "the graph tab came back as a graph tab"
    );
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_built())
            .unwrap(),
        "and its index was rebuilt on the way in"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.graph_counts(cx))
            .unwrap()
            .map(|counts| counts.notes),
        Some(2),
        "from the notes on disk"
    );
}
