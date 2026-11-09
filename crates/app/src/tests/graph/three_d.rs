//! The graph's second space: the projected 3D mode (roadmap P3.6).
//!
//! What these pin is the binding rule set in PROJECT.md ("2D / 3D mode rules"), because
//! every one of them is a thing that would otherwise quietly regress:
//!
//! * **A mode, not a replacement.** The graph opens flat and `2`/`3` turn it; the flat
//!   view's behaviour is untouched by the existence of the other one.
//! * **No re-seed on a switch.** The arrangement the user was looking at survives the
//!   turn; only the third axis is new.
//! * **The pointer answers about the picture.** The hit test runs on the projected
//!   points the paint just drew, not on world positions the camera has since moved.
//! * **Orientation-only camera memory.** The session keeps the angle and never the
//!   framing, because a camera restored to where the graph *used to be* is a screenful
//!   of nothing — in three axes especially.
use super::*;
use crate::graph::Dim;
use shale_core::graph::proj::{DOLLY_NEAR, FIT_DIST};

/// Which space the window's graph is running in.
fn mode(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Dim {
    window
        .update(cx, |ws, _window, _cx| ws.graph_mode())
        .unwrap()
}

/// The projected camera as `(yaw, pitch, pan, zoom)`.
fn angle(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
) -> (f32, f32, [f32; 2], f32) {
    window
        .update(cx, |ws, _window, cx| ws.graph_camera3(cx))
        .unwrap()
        .expect("the canvas has framed the graph")
}

/// Turn the graph to the projected space and draw it, driving the layout to rest first.
fn turn_to_3d(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("3");
    cx.run_until_parked();
    paint(cx, window);
}

/// Grab empty canvas at `from` and drag to `to` with the left button: the orbit.
fn orbit(
    cx: &mut VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
) {
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        to,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::default());
}

/// The canvas as `[x, y, w, h]`.
fn box_of(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> [f32; 4] {
    window
        .update(cx, |ws, _window, cx| ws.graph_canvas(cx))
        .unwrap()
        .expect("the canvas has a size")
}

/// Whether every node of the graph is inside the canvas.
fn all_on_screen(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> bool {
    let [x, y, w, h] = box_of(window, cx);
    let count = window
        .update(cx, |ws, _window, cx| ws.graph_paths(cx))
        .unwrap()
        .len();
    (0..count).all(|slot| {
        window
            .update(cx, |ws, _window, cx| ws.graph_node_screen(slot, cx))
            .unwrap()
            .is_some_and(|at| at[0] >= x && at[0] <= x + w && at[1] >= y && at[1] <= y + h)
    })
}

// ----- the mode itself ------------------------------------------------------

/// `3` turns the graph into the projected view and `2` turns it back, and the flat view
/// is the one that opens: the third axis is a thing to look at, not a thing the graph
/// becomes on everyone who opens it.
#[gpui::test]
fn three_turns_the_graph_and_two_turns_it_back(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    assert_eq!(mode(&window, &mut cx), Dim::Two, "the graph opens flat");
    assert!(
        window
            .update(&mut cx, |ws, _w, _cx| ws.graph_depths())
            .unwrap()
            .iter()
            .all(|depth| *depth == 0.0),
        "and a flat layout has no third axis"
    );

    turn_to_3d(&window, &mut cx);
    assert_eq!(mode(&window, &mut cx), Dim::Three, "`3` turned it");
    let depths = window
        .update(&mut cx, |ws, _w, _cx| ws.graph_depths())
        .unwrap();
    assert!(
        depths.iter().any(|depth| depth.abs() > 1.0),
        "the third axis opened up rather than stayed flat: {depths:?}"
    );

    cx.simulate_keystrokes("2");
    cx.run_until_parked();
    paint(&mut cx, &window);
    assert_eq!(mode(&window, &mut cx), Dim::Two, "`2` turned it back");
    assert!(
        window
            .update(&mut cx, |ws, _w, _cx| ws.graph_depths())
            .unwrap()
            .iter()
            .all(|depth| *depth == 0.0),
        "and the flat view is flat again, not a 3D layout seen edge-on"
    );
}

/// Turning the graph is not starting it over. The switch copies the positions across by
/// node identity and warms the sim; nothing has moved yet when the build returns, so the
/// two axes the user was reading are exactly where they were, and only the depth is new.
/// A hand-pinned node stays pinned, because an arrangement made by hand is the one thing
/// a mode switch must not lose.
#[gpui::test]
fn turning_the_graph_carries_the_arrangement(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    // Hold one node where it is, then take the beat down so the layout is read at rest
    // rather than mid-tick.
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.graph_drag_node(0, [0.0, 0.0, 0.0], cx)
        })
        .unwrap();
    window
        .update(&mut cx, |ws, _window, _cx| ws.stop_graph_beat())
        .unwrap();
    let before = window
        .update(&mut cx, |ws, _window, _cx| ws.graph_sim_positions())
        .unwrap();

    window
        .update(&mut cx, |ws, _window, cx| ws.graph_set_mode(Dim::Three, cx))
        .unwrap();
    // Read before the executor runs: this is the layout as the switch made it, before
    // the re-heat has had a tick to move anything.
    let after = window
        .update(&mut cx, |ws, _window, _cx| ws.graph_sim_positions())
        .unwrap();
    let depths = window
        .update(&mut cx, |ws, _window, _cx| ws.graph_depths())
        .unwrap();

    assert_eq!(after.len(), before.len(), "the same nodes, not a new graph");
    for (was, now) in before.iter().zip(&after) {
        assert_eq!(was, now, "the arrangement survived the turn");
    }
    assert!(
        depths.iter().any(|depth| *depth != 0.0),
        "and the axis that was missing arrived: {depths:?}"
    );
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_pinned())
            .unwrap()[0],
        "the pinned node is still pinned"
    );
}

/// A flat layout that is *turned* has to open, not stay a sheet. The third axis arrives
/// at the scale the layout already has: the new sim's seeded placement is a small sphere
/// (radius ~10·√n) and a settled vault spreads four times wider than that, so using it
/// raw leaves a plane — and a plane is a near-equilibrium, with balanced in-plane forces
/// and no z gradient to escape along, so the switch re-heat alone cannot fix it. On the
/// organic fixtures the raw carry measured an axis-spread ratio of 0.18 (100 notes) and
/// 0.41 (1k) against 0.75 / 0.62 for a layout born in 3D; the scaled carry lands at
/// 0.69 / 0.96.
#[gpui::test]
fn turning_the_graph_opens_the_third_axis(cx: &mut TestAppContext) {
    // A hub and forty notes around it: in the flat view a disc, which is exactly the
    // shape a sheet keeps.
    let hub = (1..=40).map(|i| format!("[[n{i}]] ")).collect::<String>();
    let owned: Vec<(String, String)> = std::iter::once(("hub.md".to_string(), hub))
        .chain((1..=40).map(|i| (format!("n{i}.md"), "back to [[hub]]".to_string())))
        .collect();
    let notes: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (_dir, window) = vault_with(cx, &notes);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "hub.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    window
        .update(&mut cx, |ws, _window, cx| ws.graph_set_mode(Dim::Three, cx))
        .unwrap();
    // Read before the executor runs: this is the layout as the switch made it, before
    // the re-heat has had a tick to move anything.
    let flat = window
        .update(&mut cx, |ws, _window, _cx| ws.graph_sim_positions())
        .unwrap();
    let depths = window
        .update(&mut cx, |ws, _window, _cx| ws.graph_depths())
        .unwrap();
    let spread = |values: &[f32]| -> f32 {
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
    };
    let x = spread(&flat.iter().map(|p| p[0]).collect::<Vec<f32>>());
    let y = spread(&flat.iter().map(|p| p[1]).collect::<Vec<f32>>());
    let z = spread(&depths);
    assert!(
        z >= 0.5 * x.min(y),
        "the third axis opened at the layout's own scale: x={x:.1} y={y:.1} z={z:.1}"
    );
}

// ----- the pointer answers about the picture --------------------------------

/// The hit test runs on the projected points, so aiming the pointer at where a node was
/// painted lands on that node — in a space where a node's world position and its place on
/// screen have nothing to do with each other. This is the thread-38 bug's rule wearing
/// the new costume: the grid is built from what the screen shows.
#[gpui::test]
fn the_projected_view_hits_test_what_it_drew(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    turn_to_3d(&window, &mut cx);

    let paths = window
        .update(&mut cx, |ws, _window, cx| ws.graph_paths(cx))
        .unwrap();
    for (slot, path) in paths.iter().enumerate() {
        let Some(at) = window
            .update(&mut cx, |ws, _window, cx| ws.graph_node_screen(slot, cx))
            .unwrap()
        else {
            continue;
        };
        cx.simulate_mouse_move(
            gpui::point(gpui::px(at[0]), gpui::px(at[1])),
            None,
            gpui::Modifiers::default(),
        );
        assert_eq!(
            window
                .update(&mut cx, |ws, _window, cx| ws.graph_hover(cx))
                .unwrap(),
            Some(slot),
            "the pointer is on the dot it is looking at ({path:?})"
        );
    }
}

/// Turning the world moves the picture, which is the whole point of the mode: a left
/// drag over empty canvas orbits rather than pans, and the nodes come out somewhere they
/// were not.
#[gpui::test]
fn a_left_drag_over_empty_canvas_turns_the_world(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    turn_to_3d(&window, &mut cx);

    let before = angle(&window, &mut cx);
    let [x, y, _, _] = box_of(&window, &mut cx);
    orbit(
        &mut cx,
        gpui::point(gpui::px(x + 2.0), gpui::px(y + 2.0)),
        gpui::point(gpui::px(x + 72.0), gpui::px(y + 52.0)),
    );
    let after = angle(&window, &mut cx);
    assert_ne!(after.0, before.0, "the drag yawed the world");
    assert_ne!(after.1, before.1, "and pitched it");

    // The middle button over the same spot pans instead: its meaning does not move with
    // the mode, which is what keeps a hand that already knows the graph from having to
    // learn it again.
    let [x, y, _, _] = box_of(&window, &mut cx);
    let turned = angle(&window, &mut cx);
    cx.simulate_mouse_down(
        gpui::point(gpui::px(x + 2.0), gpui::px(y + 2.0)),
        gpui::MouseButton::Middle,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_move(
        gpui::point(gpui::px(x + 52.0), gpui::px(y + 32.0)),
        Some(gpui::MouseButton::Middle),
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(
        gpui::point(gpui::px(x + 52.0), gpui::px(y + 32.0)),
        gpui::MouseButton::Middle,
        gpui::Modifiers::default(),
    );
    let panned = angle(&window, &mut cx);
    assert_eq!(
        (panned.0, panned.1),
        (turned.0, turned.1),
        "the middle button did not turn the world"
    );
    assert_ne!(panned.2, turned.2, "and it moved the view");
}

/// `f` frames the graph again without taking the angle away. It is the key a user reaches
/// for when they cannot find the graph, and a key that also reset which way they were
/// looking would be a way to get more lost rather than less.
#[gpui::test]
fn f_reframes_the_graph_and_keeps_the_angle(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    turn_to_3d(&window, &mut cx);

    let [x, y, _, _] = box_of(&window, &mut cx);
    orbit(
        &mut cx,
        gpui::point(gpui::px(x + 2.0), gpui::px(y + 2.0)),
        gpui::point(gpui::px(x + 90.0), gpui::px(y + 40.0)),
    );
    let turned = angle(&window, &mut cx);

    cx.simulate_keystrokes("f");
    paint(&mut cx, &window);
    let framed = angle(&window, &mut cx);
    assert_eq!(
        (framed.0, framed.1),
        (turned.0, turned.1),
        "`f` kept the angle the user was looking at"
    );
    assert!(
        all_on_screen(&window, &mut cx),
        "and put the whole graph back inside the canvas"
    );
}

// ----- what the session keeps -----------------------------------------------

/// The session writes the mode and the **angle**, and no framing for the projected view:
/// a remembered pan and zoom would point the next window at wherever the graph used to
/// be, and in three axes that is an empty canvas rather than a graph slightly off centre.
#[gpui::test]
fn the_session_writes_the_angle_and_not_the_framing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    turn_to_3d(&window, &mut cx);
    let [x, y, _, _] = box_of(&window, &mut cx);
    orbit(
        &mut cx,
        gpui::point(gpui::px(x + 2.0), gpui::px(y + 2.0)),
        gpui::point(gpui::px(x + 60.0), gpui::px(y + 25.0)),
    );

    let seat = window
        .update(&mut cx, |ws, _window, cx| ws.session_snapshot(cx))
        .unwrap();
    let tab = seat
        .tabs
        .iter()
        .find(|tab| tab.graph)
        .expect("the graph tab");
    assert_eq!(
        tab.mode.as_deref(),
        Some("3d"),
        "the space the tab was in is remembered"
    );
    let seat_angle = tab.orientation.expect("the angle is remembered");
    let (yaw, pitch, _, _) = angle(&window, &mut cx);
    assert_eq!(
        (seat_angle.yaw, seat_angle.pitch),
        (yaw, pitch),
        "as the angle it was at"
    );
    assert_ne!(seat_angle.yaw, 0.0, "which was the one the drag made");
}

/// …and the next window opens turned to that angle, with the framing come from the layout
/// rather than from the file.
#[gpui::test]
fn a_restored_session_opens_the_graph_turned(cx: &mut TestAppContext) {
    let session = Session {
        tabs: vec![Tab {
            path: None,
            view: String::new(),
            graph: true,
            camera: None,
            mode: Some("3d".into()),
            orientation: Some(shale_core::session::Orientation {
                yaw: 0.8,
                pitch: 0.2,
            }),
        }],
        active_tab: Some(0),
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
    paint(&mut cx, &window);

    assert_eq!(
        mode(&window, &mut cx),
        Dim::Three,
        "the graph comes back in the space it was left in"
    );
    let (yaw, pitch, _, _) = angle(&window, &mut cx);
    assert_eq!((yaw, pitch), (0.8, 0.2), "at the angle it was left at");
    assert!(
        all_on_screen(&window, &mut cx),
        "and framed to the layout it has now, not to the one the file remembered"
    );
}

/// The idle rule, in the other space. A projected paint writes a `Projection` and a
/// fit cell, which is exactly where "the paint asks for the next paint" would begin;
/// a settled world must draw nothing until the user moves it.
#[gpui::test]
fn a_settled_projected_graph_stops_painting(cx: &mut TestAppContext) {
    let clock = Clock(cx.executor());
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    repaint(&mut cx);
    cx.simulate_keystrokes("3");
    assert_eq!(
        mode(&window, &mut cx),
        Dim::Three,
        "the turn landed on the view that has the beat"
    );
    assert!(
        window
            .update(&mut cx, |ws, _w, _cx| ws.graph_beat_running())
            .unwrap(),
        "turning the graph re-heats it, so the beat has work to do"
    );

    clock.pause(&mut cx, std::time::Duration::from_secs(10));
    assert!(
        window
            .update(&mut cx, |ws, _w, _cx| ws.graph_settled())
            .unwrap(),
        "the projected layout came to rest on its own"
    );
    assert!(
        !window
            .update(&mut cx, |ws, _w, _cx| ws.graph_beat_running())
            .unwrap(),
        "and the beat stopped rather than ticking a still world"
    );

    // Frames at rest must not restart it: the projection is written *by* the paint, so
    // a paint that notified would keep the world turning forever.
    repaint(&mut cx);
    assert!(
        !window
            .update(&mut cx, |ws, _w, _cx| ws.graph_beat_running())
            .unwrap(),
        "painting a settled projected graph did not restart the beat"
    );
}

/// How far the projected camera's eye sits from the origin, in world units.
fn eye(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<f32> {
    window
        .update(cx, |ws, _window, cx| ws.graph_eye_distance(cx))
        .unwrap()
}

/// The wheel, in lines — the same synthesized event the flat view's tests use.
fn wheel(
    cx: &mut VisualTestContext,
    at: gpui::Point<gpui::Pixels>,
    lines: f32,
    modifiers: gpui::Modifiers,
) {
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: at,
        delta: gpui::ScrollDelta::Lines(gpui::point(0.0, lines)),
        modifiers,
        touch_phase: gpui::TouchPhase::Moved,
    });
}

/// The bare wheel magnifies in **both** spaces — one gesture, one meaning, so a user
/// does not relearn the wheel every time they press `3` — and the projected space's
/// travel lives on `ctrl`: the eye moves along the view axis, the screen scale does
/// not, and the travel is bounded by the cloud's radius so "zoom in" can never become
/// "be inside it". (The bare wheel dollied in the first live cut; the user's word for
/// how that felt was "fisheye" — perspective changing under a zoom reads as
/// distortion, not as travel.)
#[gpui::test]
fn the_projected_wheel_magnifies_and_ctrl_travels(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    turn_to_3d(&window, &mut cx);

    let [x, y, w, h] = box_of(&window, &mut cx);
    let at = gpui::point(gpui::px(x + w / 2.0), gpui::px(y + h / 2.0));
    let fitted = eye(&window, &mut cx).expect("the canvas framed the graph");
    let zoom0 = angle(&window, &mut cx).3;

    wheel(&mut cx, at, 1.0, gpui::Modifiers::default());
    assert_eq!(
        eye(&window, &mut cx),
        Some(fitted),
        "the bare wheel left the eye where the fit put it"
    );
    assert!(
        angle(&window, &mut cx).3 > zoom0,
        "and magnified the picture: {zoom0} -> {}",
        angle(&window, &mut cx).3
    );

    let ctrl = gpui::Modifiers {
        control: true,
        ..Default::default()
    };
    wheel(&mut cx, at, 1.0, ctrl);
    let nearer = eye(&window, &mut cx).expect("the camera is still there");
    assert!(
        nearer < fitted,
        "ctrl + wheel up travels the eye in: {fitted} -> {nearer}"
    );

    // The fit sits the eye at `FIT_DIST` times the cloud's radius, so that product is
    // the radius the dolly clamps against.
    let radius = fitted / FIT_DIST;
    for _ in 0..24 {
        wheel(&mut cx, at, 1.0, ctrl);
    }
    let bounded = eye(&window, &mut cx).expect("the camera is still there");
    assert!(
        bounded >= radius * DOLLY_NEAR - 0.01,
        "the eye crossed into the cloud: {bounded} < {}",
        radius * DOLLY_NEAR
    );
}
