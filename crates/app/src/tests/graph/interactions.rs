//! The pointer and the keyboard: hover, drag, click, zoom, pan, and picking a
/// node to open (roadmap P3.4 Task 7).
use super::*;

// ----- Task 7: the pointer and the keyboard --------------------------------
// ----- Task 7: the pointer and the keyboard --------------------------------

/// The slot that stands for `note`, and where its dot is on screen. A test needs the
/// slot because the index renumbers its nodes on every rebuild, so "the second node"
/// is not a thing a test may say.
fn node(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    note: &str,
) -> (usize, gpui::Point<gpui::Pixels>) {
    let paths = window
        .update(cx, |ws, _window, cx| ws.graph_paths(cx))
        .unwrap();
    let slot = paths
        .iter()
        .position(|path| path.as_deref() == Some(note))
        .unwrap_or_else(|| panic!("no node stands for {note}"));
    let at = window
        .update(cx, |ws, _window, cx| ws.graph_node_screen(slot, cx))
        .unwrap()
        .expect("the node is on screen");
    (slot, gpui::point(gpui::px(at[0]), gpui::px(at[1])))
}

/// The camera the canvas is painting with, as `(zoom, pan)`.
fn camera(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> (f32, [f32; 2]) {
    window
        .update(cx, |ws, _window, cx| ws.graph_camera(cx))
        .unwrap()
        .expect("the canvas has framed the graph")
}

/// The button goes down at `at`.
fn press(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
}

/// The button comes up at `at`, with whatever modifiers the user is holding.
fn release(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, modifiers: gpui::Modifiers) {
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, modifiers);
}

/// The pointer moves, with or without the button held.
fn pointer(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, held: bool) {
    cx.simulate_mouse_move(
        at,
        held.then_some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
}

/// The wheel, in lines: positive scrolls down, which zooms out.
fn wheel(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, lines: f32) {
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: at,
        delta: gpui::ScrollDelta::Lines(gpui::point(0.0, lines)),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
}

/// The wheel zooms, and the point under the cursor stays under the cursor. That is the
/// whole reason a graph's zoom is anchored: a zoom about the middle of the canvas moves
/// the thing the user was reaching for off the screen.
#[gpui::test]
fn the_wheel_zooms_around_the_cursor(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    let (_slot, at) = node(&mut cx, &window, "b.md");
    let (zoom, pan) = camera(&mut cx, &window);
    let cursor = [at.x.as_f32(), at.y.as_f32()];
    let before = shale_core::graph::geom::Camera::new(pan, zoom).to_world(cursor);

    wheel(&mut cx, at, -1.0);

    let (zoom_after, pan_after) = camera(&mut cx, &window);
    assert!(zoom_after > zoom, "scrolling up zooms in");
    let after = shale_core::graph::geom::Camera::new(pan_after, zoom_after).to_world(cursor);
    for axis in 0..2 {
        assert!(
            (after[axis] - before[axis]).abs() < 0.01,
            "the world point under the cursor stayed put: {before:?} -> {after:?}"
        );
    }
}

/// Dragging the background moves the view by exactly the pointer's travel — and pins
/// nothing, because there was no node under the button.
#[gpui::test]
fn dragging_the_background_moves_the_view(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    // A point in the middle of the canvas that no node is on: the graph is five dots
    // and the canvas is a few hundred px across.
    let canvas = window
        .update(&mut cx, |ws, _window, cx| ws.graph_canvas(cx))
        .unwrap()
        .expect("the canvas has been laid out");
    let at = gpui::point(
        gpui::px(canvas[0] + canvas[2] * 0.05),
        gpui::px(canvas[1] + canvas[3] * 0.05),
    );
    let (_zoom, pan) = camera(&mut cx, &window);

    press(&mut cx, at);
    pointer(
        &mut cx,
        gpui::point(at.x + gpui::px(40.0), at.y + gpui::px(25.0)),
        true,
    );

    let (_, pan_after) = camera(&mut cx, &window);
    assert!(
        (pan_after[0] - pan[0] - 40.0).abs() < 0.01 && (pan_after[1] - pan[1] - 25.0).abs() < 0.01,
        "the view followed the pointer: {pan:?} -> {pan_after:?}"
    );
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_pinned())
            .unwrap()
            .iter()
            .all(|pinned| !pinned),
        "a background drag pins no node"
    );
}

/// Dragging a node moves that node and pins it while the button is held — an unpinned
/// node is fought by every force in the layout, and the pointer would lose — and letting
/// go hands it back to the forces.
#[gpui::test]
fn dragging_a_node_moves_it_and_letting_go_unpins_it(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    let (slot, at) = node(&mut cx, &window, "b.md");
    let (zoom, _) = camera(&mut cx, &window);
    let before = sim_positions(&mut cx, &window)[slot];

    press(&mut cx, at);
    let moved = gpui::point(at.x + gpui::px(60.0), at.y - gpui::px(20.0));
    pointer(&mut cx, moved, true);

    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_pinned())
            .unwrap()[slot],
        "the dragged node is held against the forces"
    );
    let during = sim_positions(&mut cx, &window)[slot];
    // The pointer asked for 60 px right and 20 up, which is that much divided by the
    // zoom in world units. The node may not have caught up with the cursor in one tick,
    // but it went the way it was told and most of the way there.
    let wanted = [60.0 / zoom, -20.0 / zoom];
    let gone = [during[0] - before[0], during[1] - before[1]];
    assert!(
        gone[0] * wanted[0] > 0.0 && gone[1] * wanted[1] > 0.0,
        "the node moved the way the pointer did: {before:?} -> {during:?}"
    );
    let travelled = (gone[0] * gone[0] + gone[1] * gone[1]).sqrt();
    let asked = (wanted[0] * wanted[0] + wanted[1] * wanted[1]).sqrt();
    assert!(
        travelled > asked * 0.5,
        "and most of the way: {travelled} of {asked}"
    );

    release(&mut cx, moved, gpui::Modifiers::default());
    assert!(
        !window
            .update(&mut cx, |ws, _window, _cx| ws.graph_pinned())
            .unwrap()[slot],
        "letting go hands the node back to the forces"
    );
}

/// A shift-drag leaves the node where the user put it: pinning is how a graph gets
/// arranged by hand, and it is the one thing the plain drag deliberately does not do.
#[gpui::test]
fn a_shift_drag_leaves_a_node_pinned(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    let (slot, at) = node(&mut cx, &window, "b.md");
    let modifiers = gpui::Modifiers {
        shift: true,
        ..Default::default()
    };

    press(&mut cx, at);
    let moved = gpui::point(at.x + gpui::px(50.0), at.y + gpui::px(10.0));
    pointer(&mut cx, moved, true);
    release(&mut cx, moved, modifiers);

    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_pinned())
            .unwrap()[slot],
        "shift means leave it there"
    );
}

/// A click on a node opens the note it stands for — the graph's reason to exist as a
/// navigation surface rather than a picture.
#[gpui::test]
fn clicking_a_node_opens_its_note(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    assert!(graph(&window, &mut cx), "the graph is on screen");

    let (_, at) = node(&mut cx, &window, "b.md");
    cx.simulate_click(at, gpui::Modifiers::default());

    assert!(
        tabs(&window, &mut cx)
            .iter()
            .any(|(label, _)| label == "b.md"),
        "the note the node stands for is open: {:?}",
        tabs(&window, &mut cx)
    );
    assert!(
        !graph(&window, &mut cx),
        "and the frame moved to it, rather than the graph tab becoming a note"
    );
}

/// The pointer's hover is the graph's "which one am I pointing at", and it lets go when
/// the pointer moves off the dot.
#[gpui::test]
fn the_pointer_hovers_the_node_it_is_over(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    let (slot, at) = node(&mut cx, &window, "b.md");
    pointer(&mut cx, at, false);
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.graph_hover(cx))
            .unwrap(),
        Some(slot),
        "the node under the pointer is the hovered one"
    );

    let canvas = window
        .update(&mut cx, |ws, _window, cx| ws.graph_canvas(cx))
        .unwrap()
        .unwrap();
    pointer(
        &mut cx,
        gpui::point(
            gpui::px(canvas[0] + canvas[2] * 0.05),
            gpui::px(canvas[1] + canvas[3] * 0.05),
        ),
        false,
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.graph_hover(cx))
            .unwrap(),
        None,
        "and it is not hovered once the pointer has moved away"
    );
}

/// The arrows move the view, and their shift forms move it three times as far.
#[gpui::test]
fn the_arrows_move_the_view(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    let (_, pan) = camera(&mut cx, &window);
    cx.simulate_keystrokes("right");
    let (_, pan_one) = camera(&mut cx, &window);
    assert!(
        (pan_one[0] - pan[0] - crate::graph::PAN_STEP).abs() < 0.01,
        "one arrow is one step: {pan:?} -> {pan_one:?}"
    );

    cx.simulate_keystrokes("shift-right");
    let (_, pan_fast) = camera(&mut cx, &window);
    assert!(
        (pan_fast[0] - pan_one[0] - crate::graph::PAN_STEP * crate::graph::PAN_FAST).abs() < 0.01,
        "shift is three steps: {pan_one:?} -> {pan_fast:?}"
    );
}

/// `hjkl` picks a node without the mouse. The vault here is nothing but notes, so
/// whichever one the cone search lands on has a note behind it, and the assertion can be
/// about the choice rather than about which node happened to be chosen.
#[gpui::test]
fn hjkl_picks_a_node_and_enter_opens_it(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("a.md", "see [[b]]\n"),
            ("b.md", "back to [[a]] and over to [[c]]\n"),
            ("c.md", "and [[b]] again\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.graph_selected(cx))
            .unwrap(),
        None,
        "nothing is chosen before the user asks for one"
    );
    cx.simulate_keystrokes("l");
    let chosen = window
        .update(&mut cx, |ws, _window, cx| ws.graph_selected(cx))
        .unwrap();
    assert!(
        chosen.is_some_and(|slot| slot < 3),
        "`l` chooses one of the three nodes: {chosen:?}"
    );

    cx.simulate_keystrokes("enter");
    assert!(
        !graph(&window, &mut cx),
        "and `enter` moved the frame to that node's note"
    );
    let on_screen = tabs(&window, &mut cx)[marker(&window, &mut cx)].0.clone();
    assert!(
        on_screen.ends_with(".md"),
        "the tab on screen holds a note, not the graph: {on_screen}"
    );
}

/// `enter` opens the node the pointer is on, which is the keyboard's way to use what the
/// mouse found.
#[gpui::test]
fn enter_opens_the_hovered_node(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);

    let (_, at) = node(&mut cx, &window, "b.md");
    pointer(&mut cx, at, false);
    cx.simulate_keystrokes("enter");

    assert!(
        tabs(&window, &mut cx)
            .iter()
            .any(|(label, _)| label == "b.md"),
        "the hovered node's note is open: {:?}",
        tabs(&window, &mut cx)
    );
}

/// `f` frames the whole graph again. The camera is the user's once they have moved it,
/// so `f` is the way back, and the way back is the same framing the tab started with.
#[gpui::test]
fn f_frames_the_whole_graph_again(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    paint(&mut cx, &window);
    let (zoom, _) = camera(&mut cx, &window);

    let canvas = window
        .update(&mut cx, |ws, _window, cx| ws.graph_canvas(cx))
        .unwrap()
        .unwrap();
    let middle = gpui::point(
        gpui::px(canvas[0] + canvas[2] / 2.0),
        gpui::px(canvas[1] + canvas[3] / 2.0),
    );
    wheel(&mut cx, middle, -1.0);
    wheel(&mut cx, middle, -1.0);
    let (zoomed, _) = camera(&mut cx, &window);
    assert!(zoomed > zoom, "the wheel zoomed in");

    cx.simulate_keystrokes("f");
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let (refit, _) = camera(&mut cx, &window);
    assert!(
        (refit - zoom).abs() < 0.01,
        "`f` put the whole graph back in view: {zoom} -> {zoomed} -> {refit}"
    );
}

/// The beat is what makes a graph a moving picture rather than a snapshot: opening the
/// tab starts it, and it ends by itself once the layout has come to rest — which is the
/// idling rule, a settled graph must draw zero frames.
#[gpui::test]
fn the_beat_runs_the_layout_to_rest_and_stops(cx: &mut TestAppContext) {
    // The beat's timer is on the app's background executor, so the test drives the
    // clock rather than waiting on it — the same door the autosave tests use.
    let clock = Clock(cx.executor());
    let (_dir, window, mut cx) = linked_vault(cx);
    cx.simulate_keystrokes("space g");
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_beat_running())
            .unwrap(),
        "opening the graph starts the beat"
    );

    clock.pause(&mut cx, std::time::Duration::from_secs(10));
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_settled())
            .unwrap(),
        "the layout came to rest on its own"
    );
    assert!(
        !window
            .update(&mut cx, |ws, _window, _cx| ws.graph_beat_running())
            .unwrap(),
        "and the beat stopped rather than ticking a still graph"
    );
}
