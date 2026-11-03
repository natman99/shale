//! The canvas and the layout under it: dots, settling, the camera, and the
/// session's memory of it (roadmap P3.4 Task 6).
use super::*;

/// The graph is drawn, not described: every node is a disc, and the kinds are told
/// apart by colour — a tag takes the graph's own tag colour, and a link to a note
/// that is not there is a dim disc rather than nothing at all.
#[gpui::test]
fn the_graph_paints_a_dot_for_every_node(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    let (node, tag, ghost) = window
        .update(&mut cx, |ws, _window, _cx| ws.graph_colors())
        .unwrap();
    let painted = circles(&mut cx, &window);
    assert_eq!(
        painted.len(),
        5,
        "3 notes, 1 tag and 1 ghost, one disc each"
    );
    assert_eq!(
        filled(&painted, &[ghost]),
        1,
        "the missing note is drawn dim rather than absent"
    );
    assert_eq!(
        filled(&painted, &[node, tag]),
        4,
        "and the notes and the tag carry the graph's own colours"
    );
}

/// The graph is clipped to its pane. The camera maps world units to window-absolute px,
/// and a layout is routinely larger than the pane it is fitted into, so before the clip
/// everything outside it was painted anyway: links across the file tree and the tab bar,
/// which is what it looks like for a graph to bleed out of its pane.
#[gpui::test]
fn the_graph_is_clipped_to_its_pane(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    let canvas = window
        .update(&mut cx, |ws, _window, cx| ws.graph_canvas(cx))
        .unwrap()
        .expect("the canvas has been laid out");
    let scale = window
        .update(&mut cx, |_ws, window, _cx| window.scale_factor())
        .unwrap();
    let painted = circles(&mut cx, &window);
    assert!(!painted.is_empty(), "there is a graph to check");

    // A painted quad carries the mask it will be clipped to, in scaled px. The graph's
    // own primitives must carry the pane's rectangle, not the window's.
    for disc in &painted {
        let mask = disc.content_mask.bounds;
        let (x, y) = (mask.origin.x.as_f32(), mask.origin.y.as_f32());
        let (w, h) = (mask.size.width.as_f32(), mask.size.height.as_f32());
        assert!(
            x >= canvas[0] * scale - 1.0
                && y >= canvas[1] * scale - 1.0
                && x + w <= (canvas[0] + canvas[2]) * scale + 1.0
                && y + h <= (canvas[1] + canvas[3]) * scale + 1.0,
            "a graph primitive is masked to ({x}, {y}) {w}x{h}, outside the pane at {canvas:?}"
        );
    }
}

/// The layout runs and then stops: a graph that never settles is a window that never
/// stops asking for frames, which is the idling rule broken.
#[gpui::test]
fn the_layout_settles_and_then_stops(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    assert!(
        !window
            .update(&mut cx, |ws, _window, _cx| ws.graph_settled())
            .unwrap(),
        "a fresh layout has work to do"
    );
    let before = positions(&mut cx, &window);
    paint(&mut cx, &window);
    let after = positions(&mut cx, &window);

    assert_eq!(before.len(), 5, "3 notes, 1 tag, 1 ghost");
    assert!(
        before != after,
        "the forces moved the nodes out of their starting spiral"
    );
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_settled())
            .unwrap(),
        "and the layout came to rest"
    );
}

/// The rule that keeps a graph usable: typing a word in a note must not rearrange the
/// picture. The index is rebuilt on every save and its arena indexes do not survive
/// that, so the sim is re-slotted — and when the nodes and the links are the same, the
/// layout is carried across untouched rather than re-heated.
#[gpui::test]
fn an_edit_that_changes_no_links_leaves_the_layout_alone(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    let before = positions(&mut cx, &window);

    // `escape` hands the keyboard to the note and moves the frame with it; the graph
    // tab stays on the bar, and so does its picture.
    cx.simulate_keystrokes("escape");
    assert!(!window
        .update(&mut cx, |ws, _w, _cx| ws.showing_graph())
        .unwrap());
    cx.simulate_keystrokes("A");
    cx.simulate_input(" and more prose");
    cx.simulate_keystrokes("escape ctrl-s");

    let after = positions(&mut cx, &window);
    assert_eq!(before, after, "a save that changed no links moved no node");
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_settled())
            .unwrap(),
        "and the layout did not re-heat either"
    );
}

/// When the graph *does* change, the nodes that were already there keep their places
/// and only the new arrangement has to be found: the picture jumps on a new link, but
/// it does not start over.
#[gpui::test]
fn a_new_link_carries_the_old_positions(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    let before = positions(&mut cx, &window);

    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" and [[c]]");
    cx.simulate_keystrokes("escape ctrl-s");
    // The save re-heats the layout and starts its beat; take the beat down so what is
    // read is the frame the change shipped, not whatever the next tick made of it.
    window
        .update(&mut cx, |ws, _window, _cx| ws.stop_graph_beat())
        .unwrap();

    let after = positions(&mut cx, &window);
    assert_eq!(before.len(), after.len(), "the same five nodes");
    assert_eq!(before, after, "and each of them is where it was");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.graph_counts(cx))
            .unwrap()
            .map(|counts| counts.links),
        Some(4),
        "the new link is in the graph"
    );
    assert!(
        !window
            .update(&mut cx, |ws, _window, _cx| ws.graph_settled())
            .unwrap(),
        "and the layout was warmed up to make room for it"
    );
}

/// A graph the user has panned is state they chose in a window, so the session file
/// remembers it — same rule as a note's scroll, and the reason there is no `[graph]`
/// section in the config.
#[gpui::test]
fn a_painted_graph_records_where_it_is_looking(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    assert!(
        window
            .update(&mut cx, |ws, _window, cx| ws.tab_state(cx))
            .unwrap()
            .iter()
            .all(|tab| tab.camera.is_none()),
        "a window on a note has no camera to remember"
    );
    cx.simulate_keystrokes("space g");

    paint(&mut cx, &window);
    let camera = window
        .update(&mut cx, |ws, _window, cx| ws.tab_state(cx))
        .unwrap()
        .iter()
        .find(|tab| tab.graph)
        .and_then(|tab| tab.camera)
        .expect("a painted graph knows where it is looking");
    assert!(
        camera.zoom > 0.0,
        "the fitted camera has a real zoom, got {camera:?}"
    );
}

/// …and the next window opens the graph where the last one left it, rather than
/// re-framing the whole vault on a user who had zoomed in on one corner of it.
#[gpui::test]
fn the_graphs_camera_comes_back_with_the_session(cx: &mut TestAppContext) {
    use shale_core::session::GraphSeat;

    let session = Session {
        tabs: vec![
            Tab {
                path: Some(PathBuf::from("a.md")),
                view: "live".into(),
                graph: false,
                camera: None,
            },
            Tab {
                path: None,
                view: String::new(),
                graph: true,
                camera: Some(GraphSeat {
                    pan: [120.0, 40.0],
                    zoom: 2.0,
                }),
            },
        ],
        active_tab: Some(1),
        ..Session::default()
    };
    let (_dir, window) = session::vault_with_session(
        cx,
        &[("a.md", "see [[b]]\n"), ("b.md", "back to [[a]]\n")],
        &session,
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let camera = window
        .update(&mut cx, |ws, _window, cx| ws.tab_state(cx))
        .unwrap()
        .iter()
        .find(|tab| tab.graph)
        .and_then(|tab| tab.camera)
        .expect("the restored graph tab kept its camera");
    assert_eq!(camera.pan, [120.0, 40.0]);
    assert_eq!(camera.zoom, 2.0, "and its zoom");
}
