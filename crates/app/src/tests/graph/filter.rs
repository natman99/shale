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

/// The flat camera as `(zoom, pan)`, for the drift tests that watch it move.
fn cam2d(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> (f32, [f32; 2]) {
    window
        .update(cx, |ws, _window, cx| ws.graph_camera(cx))
        .unwrap()
        .expect("the canvas has framed the graph")
}

/// Which notes the picture is holding. The count is not enough where two
/// neighbourhoods are the same size.
fn shown_paths(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.graph_paths(cx))
        .unwrap()
        .into_iter()
        .flatten()
        .collect()
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
    // The re-centre is deferred to the next frame, so the test has to deliver the
    // frame's callbacks and not just its draws.
    repaint(&mut cx);
    paint(&mut cx, &window);
    // Which notes, not how many: both neighbourhoods are three notes wide, and a test
    // that counts them passes whether the follow happened or not.
    let mut names = shown_paths(&mut cx, &window);
    names.sort();
    assert_eq!(
        names,
        vec![
            "n2.md".to_string(),
            "n3.md".to_string(),
            "n4.md".to_string(),
            "n5.md".to_string()
        ],
        // n4 itself, n3 and n5 a hop away, and n2 two hops out.
        "and now it is the neighbourhood of n4"
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

/// The neighbourhood says which dot it is a neighbourhood *of*. The note the window is
/// showing carries a mark of its own — a halo wider than any hover's, and a name the
/// label budget cannot cut — because a local graph that does not identify its centre
/// makes the user find the note they were just reading by its shape.
#[gpui::test]
fn the_local_graph_marks_the_note_it_is_local_to(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.graph_centre_mark(cx))
            .unwrap(),
        None,
        "the whole vault has no centre to mark"
    );

    cx.simulate_keystrokes("]");
    let paths = window
        .update(&mut cx, |ws, _window, cx| ws.graph_paths(cx))
        .unwrap();
    let marked = window
        .update(&mut cx, |ws, _window, cx| ws.graph_centre_mark(cx))
        .unwrap()
        .expect("a neighbourhood marks its centre");
    assert_eq!(
        paths[marked].as_deref(),
        Some("n1.md"),
        "the mark is on the note being read"
    );

    show(&mut cx, &window, "n4.md");
    repaint(&mut cx);
    paint(&mut cx, &window);
    let paths = window
        .update(&mut cx, |ws, _window, cx| ws.graph_paths(cx))
        .unwrap();
    let marked = window
        .update(&mut cx, |ws, _window, cx| ws.graph_centre_mark(cx))
        .unwrap()
        .expect("still marked after the window moved");
    assert_eq!(
        paths[marked].as_deref(),
        Some("n4.md"),
        "and the mark moved with the window"
    );
}

/// A neighbourhood that changes size moves the camera — `]` widens the cloud and the
/// view drifts to frame it — and the drift is a chase, not a promise: the moment the
/// user takes the camera, it stops.
#[gpui::test]
fn the_local_graph_moves_the_camera_and_stops_when_touched(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("]");
    assert!(
        window
            .update(&mut cx, |ws, _window, cx| ws.graph_reframing(cx))
            .unwrap(),
        "the view is moving itself to the neighbourhood it just changed"
    );

    cx.simulate_keystrokes("f");
    assert!(
        !window
            .update(&mut cx, |ws, _window, cx| ws.graph_reframing(cx))
            .unwrap(),
        "and it stopped the moment the user took the camera"
    );
}

/// Widening the neighbourhood in the projected view grows a cloud that runs off the
/// screen — the complaint that made the drift. It lands where `f` would have put the
/// camera: the drift is a fit that takes a few frames to get there, not a different
/// framing, and it never touches the angle the user is looking from.
#[gpui::test]
fn the_drift_arrives_where_fit_would(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    cx.simulate_keystrokes("3");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("]");
    // Settle the layout the new hops brought in before the camera chases it, so the
    // framing the drift aims at is the one `f` will find.
    paint(&mut cx, &window);

    let mut steps = 0;
    // The step reports whether the camera still has ground to cover; the loop runs while
    // it does, and the step that reports arrival is the one that ends it.
    while window
        .update(&mut cx, |ws, _window, cx| ws.graph_drift_step(cx))
        .unwrap()
    {
        steps += 1;
        assert!(steps < 300, "the drift never arrived");
    }
    let drifted = window
        .update(&mut cx, |ws, _window, cx| ws.graph_camera3(cx))
        .unwrap()
        .expect("the projected view has a camera");
    let drifted_dist = window
        .update(&mut cx, |ws, _window, cx| ws.graph_eye_distance(cx))
        .unwrap()
        .expect("and an eye distance");

    cx.simulate_keystrokes("f");
    paint(&mut cx, &window);
    let fitted = window
        .update(&mut cx, |ws, _window, cx| ws.graph_camera3(cx))
        .unwrap()
        .expect("and `f` frames it");
    let fitted_dist = window
        .update(&mut cx, |ws, _window, cx| ws.graph_eye_distance(cx))
        .unwrap()
        .expect("from outside the cloud");

    let (yaw, pitch, pan, zoom) = drifted;
    let (fyaw, fpitch, fpan, fzoom) = fitted;
    assert_eq!(
        (yaw, pitch),
        (fyaw, fpitch),
        "the drift frames the neighbourhood without taking the angle away"
    );
    assert!(
        (zoom - fzoom).abs() <= fzoom * 0.02,
        "the drift's zoom {zoom} is where a fit would put it ({fzoom})"
    );
    assert!(
        (pan[0] - fpan[0]).abs() < 2.0 && (pan[1] - fpan[1]).abs() < 2.0,
        "the drift's pan {pan:?} is where a fit would put it ({fpan:?})"
    );
    assert!(
        (drifted_dist - fitted_dist).abs() <= fitted_dist * 0.02,
        "and the eye sits where a fit would sit: {drifted_dist} vs {fitted_dist}"
    );
}



/// The spring is critically damped, so the camera must reach its framing without
/// travelling through it. Overshoot is what makes a follow look like a bounce, and a
/// discrete integrator can produce a little of it, so the tolerance is small but real.
#[gpui::test]
fn the_drift_never_overshoots_its_framing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    // Where the camera sits before the neighbourhood widens: that is where the drift
    // leaves from. Read here rather than after the widening, because between the two the
    // view has a *pending* fit for the new frame, and a camera that has not been moved
    // yet is not the same reading.
    let (start_zoom, start_pan) = cam2d(&mut cx, &window);
    cx.simulate_keystrokes("]");
    paint(&mut cx, &window);

    // The honest statement of "no overshoot": on each axis the camera approaches its
    // target and never gets to the far side of it. The target itself is moving (it is the
    // smoothed framing), so measuring the camera against where it started would only show
    // that the start was not where it ended.
    fn sign(v: f32) -> i32 {
        if v > 0.0 {
            1
        } else if v < 0.0 {
            -1
        } else {
            0
        }
    }
    let mut sides: Vec<[i32; 3]> = Vec::new();
    let mut steps = 0;
    while window
        .update(&mut cx, |ws, _window, cx| ws.graph_drift_step(cx))
        .unwrap()
    {
        steps += 1;
        assert!(steps < 300, "the drift never arrived");
        let (zoom, pan) = cam2d(&mut cx, &window);
        let target = window
            .update(&mut cx, |ws, _window, cx| ws.graph_drift_target(cx))
            .unwrap()
            .expect("the drift is running, so it has a target");
        sides.push([
            sign(pan[0] - target.0[0]),
            sign(pan[1] - target.0[1]),
            sign(zoom - target.1),
        ]);
    }
    assert!(steps > 3, "a spring that arrives in one step is a snap: {steps}");
    let (end_zoom, end_pan) = cam2d(&mut cx, &window);
    assert!(
        (end_pan[0] - start_pan[0]).abs() + (end_pan[1] - start_pan[1]).abs() > 10.0
            && (end_zoom - start_zoom).abs() > 0.001,
        "and the drift actually moved the camera: {start_pan:?} -> {end_pan:?}, \
         {start_zoom} -> {end_zoom}"
    );

    for axis in 0..3 {
        for pair in sides.windows(2) {
            let (was, now) = (pair[0][axis], pair[1][axis]);
            assert!(
                !(was > 0 && now < 0) && !(was < 0 && now > 0),
                "axis {axis} crossed its target: {sides:?}"
            );
        }
    }
}

/// The dead zone: a framing the camera is already inside is not chased. `:graph-local 5`
/// on a five-note chain is the whole vault wearing a neighbourhood's clothes, so the
/// framing does not move at all — and the drift should notice, rather than micro-moving
/// the camera every frame until the cap runs out.
#[gpui::test]
fn a_framing_the_camera_is_already_inside_is_not_chased(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    let (zoom, pan) = cam2d(&mut cx, &window);

    command_line("graph-local 5", &mut cx);
    paint(&mut cx, &window);

    let mut steps = 0;
    while window
        .update(&mut cx, |ws, _window, cx| ws.graph_drift_step(cx))
        .unwrap()
    {
        steps += 1;
        assert!(steps < 300, "the drift never settled");
    }
    // The step that finds the third still frame is the one that ends the drift, so it
    // reports "no more ground to cover" and is not counted.
    assert_eq!(
        steps + 1,
        crate::graph::DRIFT_STILL_FRAMES as usize,
        "the drift gives up after the dead zone has held for its still-frame budget"
    );
    let (zoom_after, pan_after) = cam2d(&mut cx, &window);
    assert!(
        (pan_after[0] - pan[0]).abs() < 1.0 && (pan_after[1] - pan[1]).abs() < 1.0,
        "the camera did not move for a framing it was already inside: {pan:?} -> {pan_after:?}"
    );
    assert!(
        (zoom_after - zoom).abs() <= zoom * 0.005,
        "and neither did the zoom: {zoom} -> {zoom_after}"
    );
}

/// The idle contract, applied to the camera: the drift is a chain of next-frame
/// callbacks, and a chain that never stops is a window that never stops painting. It has
/// to end on its own and leave no frame demand behind.
#[gpui::test]
fn the_drift_parks_itself_when_it_arrives(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = chain_vault(cx, "n1.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("]");

    let mut frames = 0;
    while window.update(&mut cx, |ws, _window, cx| ws.graph_reframing(cx)).unwrap() {
        frames += 1;
        assert!(frames < 60, "the drift is still running after a minute of frames");
        repaint(&mut cx);
    }
    assert!(frames > 3, "the drift ran for real frames: {frames}");
    let pending = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(
        pending, 0,
        "the drift left no next-frame work behind, so the window can idle"
    );
}


/// A hub vault: sixty notes all hanging off one, so a depth step is a thirtyfold change
/// in what the picture holds — the case the user reported as "the camera zooms in before
/// zooming out" and "the dots pulsate as the camera moves".
fn hub_vault(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let leaves = 60;
    let mut notes: Vec<(String, String)> = vec![(
        "hub.md".into(),
        (1..=leaves)
            .map(|i| format!("[[leaf{i}.md]] "))
            .collect::<Vec<_>>()
            .join("\n"),
    )];
    for i in 1..=leaves {
        notes.push((format!("leaf{i}.md"), "[[hub.md]]\n".into()));
    }
    let notes: Vec<(&str, &str)> = notes
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (dir, window) = vault_with(cx, &notes);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "leaf1.md");
    (dir, window, cx)
}

/// Drive a depth change and the layout together, one drift step per couple of sim ticks.
/// `paint` runs the layout to rest in one go, which hides the thing being tested: the
/// drift is only ever interesting while the layout is *still moving*, and a widened
/// neighbourhood spends its first half-second flying outward.
fn drift_under_physics(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
) -> Vec<(f32, f32, f32)> {
    let mut trace: Vec<(f32, f32, f32)> = Vec::new();
    let mut steps = 0;
    while window
        .update(cx, |ws, _window, cx| ws.graph_drift_step(cx))
        .unwrap()
    {
        steps += 1;
        assert!(steps < 300, "the drift never arrived");
        window
            .update(cx, |ws, _window, cx| ws.graph_run_for(2, cx))
            .unwrap();
        let zoom = window
            .update(cx, |ws, _window, cx| ws.graph_camera3(cx))
            .unwrap()
            .unwrap()
            .3;
        let dot = window
            .update(cx, |ws, _window, cx| ws.graph_drawn_node("hub.md", cx))
            .unwrap()
            .unwrap()
            .1;
        let spread = window
            .update(cx, |ws, _window, cx| ws.graph_drawn_spread(cx))
            .unwrap()
            .unwrap();
        trace.push((zoom, dot, spread));
    }
    assert!(steps > 10, "a drift this short proves nothing: {steps}");
    trace
}

/// Widening a local graph is the case the user reported as "the camera zooms in before it
/// zooms out". The eye distance is the one thing the drift springs; the framing is fitted
/// at it, so the two can never pull the picture in opposite directions, and the zoom
/// travels one way for the whole move.
#[gpui::test]
fn a_widening_moves_the_camera_one_way_only(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = hub_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    command_line("graph-local 1", &mut cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("3");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("] ]");
    let trace = drift_under_physics(&window, &mut cx);

    // What the user sees is the picture, not the camera's numbers. Widening must not
    // make it bigger: the report was "when I increase the number of links the camera
    // seems to zoom in before zooming out", and what was happening was the picture
    // ballooning 36% past its size while the framing stalled, then being pulled back.
    // Out-and-back on screen is the jolt, whatever the zoom did on the way.
    let start = trace[0].2;
    let peak = trace.iter().fold(0.0f32, |a, t| a.max(t.2));
    assert!(
        peak <= start * 1.02,
        "the picture ballooned {:.1}% past its starting size before settling: {:?}",
        (peak / start - 1.0) * 100.0,
        trace.iter().map(|t| t.2).collect::<Vec<_>>()
    );
    assert!(
        trace.last().unwrap().2 < start * 0.9,
        "and a widening ends with the picture framed, not merely un-balloonéd: {start} -> {}",
        trace.last().unwrap().2
    );
}

/// The other half of the report: "the circles change size... they pulsate". A node's dot
/// and the picture it sits in are the same projection read two ways, so they may differ by
/// the damped size perspective and nothing more — the dot may never move against its
/// picture. Measured before this was fixed: the picture shrank 42% across one widening
/// while this node's dot grew 4.5%.
#[gpui::test]
fn a_nodes_dot_never_fights_its_picture(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = hub_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    command_line("graph-local 1", &mut cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("3");
    paint(&mut cx, &window);
    cx.simulate_keystrokes("] ]");
    let trace = drift_under_physics(&window, &mut cx);

    let dots: Vec<f32> = trace.iter().map(|t| t.1).collect();
    // A node's size may change because the node itself moved in depth — that is honest
    // 3D, and the hub does. What it may not do is *vibrate*: change direction more than
    // once across the whole move. Two springs pulling a node's size in opposite
    // directions is exactly that, and it is what the report called pulsation.
    let mut turns = 0;
    let mut rising = 0.0f32;
    for pair in dots.windows(2) {
        let step = pair[1] - pair[0];
        if step.abs() < 0.005 {
            continue;
        }
        let sign = step.signum();
        if sign != rising && rising != 0.0 {
            turns += 1;
        }
        rising = sign;
    }
    assert!(turns <= 1, "the dot changed direction {turns} times as the camera moved: {dots:?}");
    // And it may not end up changed by more than the picture it sits in.
    let dot_move = (dots.last().unwrap() - dots[0]).abs() / dots[0];
    let picture_move = (trace.last().unwrap().2 - trace[0].2).abs() / trace[0].2;
    assert!(
        dot_move <= picture_move,
        "the dot changed by {:.1}% while the whole picture changed by {:.1}%",
        dot_move * 100.0,
        picture_move * 100.0
    );
}

