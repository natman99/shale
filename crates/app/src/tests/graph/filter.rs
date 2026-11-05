//! The filter strip (roadmap P3.4 Task 8): what the picture holds, and what the
//! keyboard means while a query is being typed.
//!
//! The vault these run on is [`linked_vault`]'s: three notes, one link to a note that
//! is not there, one tag, and one note that links nothing. The graph that makes is five
//! nodes and three links, so every filter has something obvious to take away — and the
//! assertions are mostly about *discs*, because a filtered-out node is absent from the
//! physics and the paint rather than drawn faintly.

use super::*;
use shale_core::session::GraphFilterSeat;

/// Whether the strip is showing, and what its search line holds.
fn panel(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> (bool, String) {
    window
        .update(cx, |ws, _window, cx| ws.graph_panel(cx))
        .unwrap()
}

/// The counts the strip's row is drawn from.
fn counts(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Option<GraphSummary> {
    window
        .update(cx, |ws, _window, cx| ws.graph_counts(cx))
        .unwrap()
}

/// The node the keyboard has chosen, if the graph has one.
fn chosen(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Option<usize> {
    window
        .update(cx, |ws, _window, cx| ws.graph_selected(cx))
        .unwrap()
}

/// `/` opens the strip and takes the keyboard; `escape` gives it back without leaving
/// the tab, and the filter stays in force either way — closing the strip is not undoing
/// the picture.
#[gpui::test]
fn the_strip_opens_on_slash_and_closes_on_escape(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    assert_eq!(
        panel(&mut cx, &window),
        (false, String::new()),
        "no strip until it is asked for"
    );

    cx.simulate_keystrokes("/");
    cx.simulate_input("b");
    assert_eq!(
        panel(&mut cx, &window),
        (true, "b".into()),
        "the letter went into the field"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|counts| counts.notes),
        Some(1),
        "and the picture followed it as it was typed"
    );

    cx.simulate_keystrokes("escape");
    assert!(
        !window
            .update(&mut cx, |ws, _window, cx| ws.graph_panel(cx))
            .unwrap()
            .0,
        "escape closes the strip"
    );
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.showing_graph())
            .unwrap(),
        "and leaves the graph on screen"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|counts| counts.notes),
        Some(1),
        "the query is still in force: closing the strip is not an undo"
    );

    cx.simulate_keystrokes("/");
    assert_eq!(
        panel(&mut cx, &window),
        (true, "b".into()),
        "and opening it again shows what is filtering the graph"
    );
    cx.simulate_keystrokes("enter");
    assert!(
        !window
            .update(&mut cx, |ws, _window, cx| ws.graph_panel(cx))
            .unwrap()
            .0,
        "`enter` closes it too, since there is nothing to accept that `escape` undoes"
    );
}

/// The reason the strip has a key context of its own: every letter the graph answers to
/// has to be typeable in a query.
#[gpui::test]
fn the_strips_letters_are_text_and_not_graph_commands(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    cx.simulate_keystrokes("j");
    assert!(
        chosen(&mut cx, &window).is_some(),
        "with the graph's keyboard, `j` chooses a node"
    );

    cx.simulate_keystrokes("/");
    cx.simulate_input("aj");
    assert_eq!(
        panel(&mut cx, &window).1,
        "aj",
        "with the strip's, the same letters are a query"
    );
    assert!(
        chosen(&mut cx, &window).is_none(),
        "and no node was chosen: the graph's `j` is not in the stack"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|counts| counts.notes),
        Some(0),
        "which is what narrowed the picture to nothing"
    );
}

/// Turning orphans off takes the lonely note out of the simulation as well as the
/// picture, which is the difference between a filter and a dimming.
#[gpui::test]
fn hiding_orphans_takes_the_lonely_note_out(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    assert_eq!(circles(&mut cx, &window).len(), 5);

    cx.simulate_keystrokes("/");
    cx.simulate_keystrokes("alt-o");
    paint(&mut cx, &window);

    assert_eq!(
        circles(&mut cx, &window).len(),
        4,
        "the note that links nothing is out of the picture"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|counts| (counts.notes, counts.links)),
        Some((2, 3)),
        "and the row describes what is on screen: two notes, and every link still reaches something"
    );
    assert!(
        !window
            .update(&mut cx, |ws, _window, _cx| ws.graph_filter())
            .unwrap()
            .orphans,
        "the window, not just the view, knows the filter changed"
    );
}

/// The same for the holes in the vault and for tags: one chip each, and the counts and
/// the discs agree about what is left.
#[gpui::test]
fn the_other_two_chips_take_their_own_kind_out(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    cx.simulate_keystrokes("/");
    cx.simulate_keystrokes("alt-x");
    paint(&mut cx, &window);
    assert_eq!(
        circles(&mut cx, &window).len(),
        4,
        "`missing` off is core's `existing_only` on"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|counts| counts.ghosts),
        Some(0),
        "the ghost is gone, and its link with it"
    );

    cx.simulate_keystrokes("alt-t");
    paint(&mut cx, &window);
    assert_eq!(circles(&mut cx, &window).len(), 3, "and the tag joins it");
    assert_eq!(
        counts(&mut cx, &window).map(|counts| (counts.notes, counts.tags, counts.links)),
        Some((3, 0, 1)),
        "leaving the three notes and the one link between two of them"
    );
}

/// The search line is a filter, not a jump: it narrows what is drawn rather than moving
/// the camera to what matches.
#[gpui::test]
fn the_query_narrows_the_picture(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    cx.simulate_keystrokes("/");
    cx.simulate_input("gone");
    paint(&mut cx, &window);
    assert_eq!(
        circles(&mut cx, &window).len(),
        1,
        "only the missing note the query names"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|counts| (counts.notes, counts.ghosts, counts.links)),
        Some((0, 1, 0)),
        "and no links, because both ends have to be on screen"
    );

    cx.simulate_keystrokes("ctrl-backspace");
    paint(&mut cx, &window);
    assert_eq!(
        panel(&mut cx, &window).1,
        "",
        "`ctrl-backspace` takes the word, and an empty line filters nothing"
    );
    assert_eq!(
        circles(&mut cx, &window).len(),
        5,
        "so the whole graph is back"
    );
}

/// A filter is a decision made inside a window, so the session file remembers it and the
/// config does not — and everything-on is the *absence* of a seat, which is what lets a
/// session file written before the strip exist restore a whole graph.
#[gpui::test]
fn a_filter_change_is_remembered_by_the_session(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.session_snapshot(cx))
            .unwrap()
            .graph_filter,
        None,
        "the default filter is not a decision worth a line in the file"
    );

    cx.simulate_keystrokes("/");
    cx.simulate_keystrokes("alt-o");
    cx.simulate_input("idea");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.session_snapshot(cx))
            .unwrap()
            .graph_filter,
        Some(GraphFilterSeat {
            orphans: false,
            existing_only: false,
            tags: true,
            search: Some("idea".into()),
            local: None,
        }),
        "and a filter the user chose is"
    );
}

/// …and the next window opens the graph the last one was looking at, with the search line
/// showing the query that is filtering it.
#[gpui::test]
fn the_filter_comes_back_with_the_session(cx: &mut TestAppContext) {
    let session = Session {
        tabs: vec![Tab {
            path: None,
            view: String::new(),
            graph: true,
            camera: None,
            mode: None,
            orientation: None,
        }],
        active_tab: Some(0),
        graph_filter: Some(GraphFilterSeat {
            orphans: false,
            existing_only: true,
            tags: false,
            search: None,
            local: None,
        }),
        ..Session::default()
    };
    let (_dir, window) = session::vault_with_session(
        cx,
        &[
            ("a.md", "see [[b]] and [[gone]] and #tag\n"),
            ("b.md", "back to [[a]]\n"),
            ("c.md", "alone\n"),
        ],
        &session,
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.showing_graph())
            .unwrap(),
        "the graph tab came back"
    );
    paint(&mut cx, &window);
    assert_eq!(
        circles(&mut cx, &window).len(),
        2,
        "with the filter on it: no orphan, no ghost, no tag"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|counts| (counts.notes, counts.links)),
        Some((2, 1)),
        "and the row agrees"
    );
}

// ----- the local graph (roadmap M2b) ----------------------------------------

/// A chain of five notes, `n1` → `n2` → … → `n5`: the shape where a hop count is
/// legible, because each depth adds exactly one note.
fn chain_vault(
    cx: &mut TestAppContext,
    open: &str,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let owned: Vec<(String, String)> = (1..=5)
        .map(|i| {
            (
                format!("n{i}.md"),
                if i < 5 {
                    format!("on to [[n{}]]\n", i + 1)
                } else {
                    "the end of the chain\n".to_string()
                },
            )
        })
        .collect();
    let notes: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (dir, window) = vault_with(cx, &notes);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, open);
    (dir, window, cx)
}

/// How many notes the picture is holding.
fn shown(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> usize {
    counts(cx, window).map_or(0, |counts| counts.notes)
}

/// `]` and `[` are the depth, and the depth is a hop count from the note on screen —
/// three hops of a five-note chain is three notes, not three links and a cloud.
#[gpui::test]
fn the_local_graph_reaches_the_note_on_screen(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    assert_eq!(shown(&mut cx, &window), 5, "the whole vault is five notes");

    cx.simulate_keystrokes("]");
    assert_eq!(
        shown(&mut cx, &window),
        3,
        "the default two hops from n1 are n1, n2 and n3"
    );
    cx.simulate_keystrokes("]");
    assert_eq!(shown(&mut cx, &window), 4, "a third hop adds one note");
    cx.simulate_keystrokes("[");
    assert_eq!(shown(&mut cx, &window), 3, "and takes it back off");
    cx.simulate_keystrokes("\\");
    assert_eq!(shown(&mut cx, &window), 5, "`\\` is the whole vault again");
}

/// The neighbourhood is *of the note being read*: move to another note and the picture
/// moves with it, rather than staying a map of a note the user has left.
#[gpui::test]
fn the_local_graph_follows_the_note_being_read(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("]");
    assert_eq!(shown(&mut cx, &window), 3, "two hops from n1");

    show(&mut cx, &window, "n4.md");
    cx.run_until_parked();
    assert_eq!(
        shown(&mut cx, &window),
        3,
        "and now it is two hops from n4 — n3, n4, n5"
    );
}

/// The depth is a view decision, so it rides the session seat like the rest of the
/// filter. The centre deliberately does not: a remembered centre can be a note the user
/// deleted on the way out.
#[gpui::test]
fn the_local_depth_is_remembered_by_the_session(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("] ] ]");
    let seat = window
        .update(&mut cx, |ws, _window, cx| ws.session_snapshot(cx))
        .unwrap()
        .graph_filter
        .expect("a filter the user chose is a seat");
    assert_eq!(seat.local, Some(4), "three steps from the default of two");
}

/// …and the next window opens the graph narrowed the way the last one left it.
#[gpui::test]
fn the_local_depth_comes_back_with_the_session(cx: &mut TestAppContext) {
    let session = Session {
        tabs: vec![
            Tab {
                path: Some(PathBuf::from("n1.md")),
                view: String::new(),
                graph: false,
                camera: None,
                mode: None,
                orientation: None,
            },
            Tab {
                path: None,
                view: String::new(),
                graph: true,
                camera: None,
                mode: None,
                orientation: None,
            },
        ],
        active_tab: Some(1),
        graph_filter: Some(GraphFilterSeat {
            orphans: true,
            existing_only: false,
            tags: true,
            search: None,
            local: Some(2),
        }),
        ..Session::default()
    };
    let owned: Vec<(String, String)> = (1..=5)
        .map(|i| {
            (
                format!("n{i}.md"),
                if i < 5 {
                    format!("on to [[n{}]]\n", i + 1)
                } else {
                    "the end of the chain\n".to_string()
                },
            )
        })
        .collect();
    let notes: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (_dir, window) = session::vault_with_session(cx, &notes, &session);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    paint(&mut cx, &window);
    assert_eq!(
        shown(&mut cx, &window),
        3,
        "the graph opens narrowed to two hops of the note the session was on"
    );
}
