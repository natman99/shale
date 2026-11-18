//! Resting the pointer on a node: the card a graph owes a pointer that stopped moving.
//!
//! This is P6.2's other direction. The editor's card puts a note's lines under a link;
//! this one puts them under a dot. Both are the same widget for the same reason — one
//! look for "what is this?" wherever the pointer asks it — and the difference is the
//! **wait**: prose holds few links, so the editor answers the moment the pointer finds
//! one, while a graph is dots packed together and sweeping across one is ordinary. The
//! dwell is what keeps a card from flashing at every dot on the way past, and it is why
//! the card is not simply drawn from the graph's hover state.
//!
//! Each test brings its own two-note vault, because the interesting question is which
//! nodes answer (a note, a note with aliases, a note being edited) and which refuse (a
//! ghost, a tag, a pointer that never stopped).

use super::*;
use crate::card::NODE_DWELL;
use crate::tests::popup::card;

/// The graph tab, settled and painted.
fn on_graph(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) {
    cx.simulate_keystrokes("space g");
    paint(cx, window);
}

/// The pointer, moved to `at` with nothing held.
fn pointer(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, gpui::Modifiers::default());
}

/// The slot that stands for `note`, and where its dot is on screen. The slot comes from
/// the picture rather than from counting: the index renumbers nodes on every rebuild.
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

/// The first slot that names no note — a ghost or a tag.
fn unnamed_node(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> usize {
    let paths = window
        .update(cx, |ws, _window, cx| ws.graph_paths(cx))
        .unwrap();
    paths
        .iter()
        .position(|path| path.is_none())
        .expect("the vault has a link to nothing")
}

/// Draw the frames a hover change asks for. A real window draws them on its own — the
/// view is dirty, so the window renders, and rendering is where the window notices where
/// the pointer came to rest — but the harness only paints when a test asks.
fn frame(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// Rest the pointer on the node and let the wait run out.
fn rest(cx: &mut VisualTestContext, clock: &Clock, at: gpui::Point<gpui::Pixels>) {
    pointer(cx, at);
    frame(cx);
    clock.pause(cx, NODE_DWELL);
}

/// Where the card is, if it is anywhere.
fn place(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    window
        .update(cx, |ws, view, cx| ws.card_place(view, cx))
        .unwrap()
        .map(|placed| placed.bounds)
}

/// A pointer that has stopped moving on a node gets the note it stands for: the name the
/// vault calls it by, and the note's own lines.
#[gpui::test]
fn resting_on_a_node_shows_its_note(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("a.md", "first line of a\nsecond line of a\n"),
            ("b.md", "back to [[a]]\n"),
        ],
    );
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");

    rest(&mut cx, &clock, at);
    let shown = card(&window, &mut cx).expect("a card for a node that is a note");
    assert_eq!(shown.0, "a", "named the way the vault names it");
    assert_eq!(shown.1, vec!["first line of a", "second line of a"]);
    assert_eq!(shown.2, 0, "nothing was left out of a two-line note");
}

/// The dwell is the whole difference between this card and the editor's, so it is pinned
/// from both sides: nothing during the wait, the card once it runs out.
#[gpui::test]
fn a_hover_shorter_than_the_dwell_shows_nothing(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "alpha\n"), ("b.md", "beta\n")]);
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");

    pointer(&mut cx, at);
    frame(&mut cx);
    clock.pause(&mut cx, NODE_DWELL / 2);
    assert_eq!(
        card(&window, &mut cx),
        None,
        "a pointer still travelling owes no card"
    );

    clock.pause(&mut cx, NODE_DWELL / 2);
    assert!(
        card(&window, &mut cx).is_some(),
        "the same pointer, having stopped, does"
    );
}

/// The alias line is the graph's answer to a question the graph otherwise cannot ask: a
/// note reached by the names it declares looks exactly like every other note.
#[gpui::test]
fn the_node_card_names_the_aliases_the_note_declares(cx: &mut TestAppContext) {
    // The alias line reads the window's alias table, and that table is filed by the index
    // pass — so these notes have to be on disk before the window opens, the way the alias
    // graph tests arrange them.
    let (_dir, window) = indexed_vault_with(
        cx,
        &[
            (
                "Deep.md",
                "---\naliases: [the deep one, deep]\n---\n\nprose about depth\n",
            ),
            ("b.md", "see [[the deep one]]\n"),
        ],
        test_config(),
    );
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "b.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "Deep.md");

    rest(&mut cx, &clock, at);
    let shown = card(&window, &mut cx).expect("a card");
    assert_eq!(shown.0, "Deep");
    assert_eq!(
        shown.1,
        vec!["also known as: the deep one, deep", "prose about depth"],
        "which note this is comes before what the note says"
    );
}

/// A note that declares nothing does not spend a row saying so.
#[gpui::test]
fn a_note_that_declares_no_aliases_spends_no_row_on_them(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "alpha\n"), ("b.md", "beta\n")]);
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");

    rest(&mut cx, &clock, at);
    assert_eq!(card(&window, &mut cx).unwrap().1, vec!["alpha"]);
}

/// A ghost names a link nobody resolved and a tag names a word: neither is a note, so
/// neither has lines to preview. The dot is still worth seeing; the card is not.
#[gpui::test]
fn a_node_that_names_no_note_shows_no_card(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = linked_vault(cx);
    let clock = Clock(cx.executor());
    on_graph(&mut cx, &window);
    let slot = unnamed_node(&mut cx, &window);
    let at = window
        .update(&mut cx, |ws, _window, cx| ws.graph_node_screen(slot, cx))
        .unwrap()
        .expect("the node is on screen");

    rest(
        &mut cx,
        &clock,
        gpui::point(gpui::px(at[0]), gpui::px(at[1])),
    );
    assert_eq!(
        card(&window, &mut cx),
        None,
        "a ghost or a tag has no note to preview"
    );
}

/// The card is opened by the pointer and dismissed by it, exactly like the editor's: it
/// never takes the keyboard, so the only thing that can end it is the pointer moving on.
#[gpui::test]
fn the_card_goes_when_the_pointer_leaves_the_node(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "alpha\n"), ("b.md", "beta\n")]);
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");
    rest(&mut cx, &clock, at);
    assert!(card(&window, &mut cx).is_some());

    // Off the dot: a node's dot is a few px across, so this is a small move and a
    // different answer.
    pointer(
        &mut cx,
        gpui::point(at.x + gpui::px(200.), at.y + gpui::px(200.)),
    );
    frame(&mut cx);
    assert_eq!(
        card(&window, &mut cx),
        None,
        "the pointer left, so the card left"
    );
}

/// Pressing is the end of resting, wherever the pointer happened to be: a hand going down
/// on the canvas is about to drag it, and a card about wherever a pan stopped would be a
/// card the user never asked for.
#[gpui::test]
fn pressing_the_canvas_ends_the_rest(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "alpha\n"), ("b.md", "beta\n")]);
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");

    pointer(&mut cx, at);
    frame(&mut cx);
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    frame(&mut cx);
    clock.pause(&mut cx, NODE_DWELL);
    assert_eq!(card(&window, &mut cx), None, "a press is not a rest");
}

/// The lines come from the buffer while a tab holds the note, so a card cannot disagree
/// with the note the user is looking at.
#[gpui::test]
fn the_card_shows_the_note_as_it_is_being_edited(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "alpha\nbeta\n"), ("b.md", "beta\n")]);
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    repaint(&mut cx);
    cx.simulate_keystrokes("o gamma");
    cx.simulate_keystrokes("escape");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");

    rest(&mut cx, &clock, at);
    assert_eq!(
        card(&window, &mut cx).unwrap().1,
        vec!["alpha", "gamma", "beta"],
        "the card shows the edit, not the file"
    );
}

/// A heading in a card is a heading, and the frontmatter block is the note's bookkeeping
/// rather than its prose — the same two rules the note's own rows, an embed and the
/// checker already follow.
#[gpui::test]
fn the_card_hides_the_markers_and_the_frontmatter(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("a.md", "---\ntags: [x]\n---\n\n# Heading and **bold**\n"),
            ("b.md", "beta\n"),
        ],
    );
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");

    rest(&mut cx, &clock, at);
    assert_eq!(
        card(&window, &mut cx).unwrap().1,
        vec!["Heading and bold"],
        "no fence, no hash, no asterisks"
    );
}

/// A long note says what the card left out, so a preview never looks like the whole of it.
#[gpui::test]
fn a_long_node_card_says_what_was_left_out(cx: &mut TestAppContext) {
    let body: String = (1..=20)
        .map(|n| format!("line {n}\n"))
        .collect::<Vec<_>>()
        .join("");
    let (_dir, window) = vault_with(cx, &[("a.md", body.as_str()), ("b.md", "beta\n")]);
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");

    rest(&mut cx, &clock, at);
    let shown = card(&window, &mut cx).unwrap();
    assert_eq!(shown.1.len(), crate::card::VISIBLE_LINES);
    assert_eq!(
        shown.2,
        20 - crate::card::VISIBLE_LINES,
        "the card counts what it hid"
    );
}

/// A graph tab sitting behind a note tab is not on screen, and its hover state is the last
/// one the pointer left there. A card that reads it anyway answers a question nobody asked,
/// over a note that was never under the pointer.
#[gpui::test]
fn a_graph_hidden_behind_a_note_tab_shows_no_card(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "alpha\n"), ("b.md", "beta\n")]);
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (_, at) = node(&mut cx, &window, "a.md");
    pointer(&mut cx, at);
    frame(&mut cx);

    // The note tab comes back to the front; the pointer never moves.
    cx.simulate_keystrokes("g t");
    frame(&mut cx);
    assert!(
        !window
            .update(&mut cx, |ws, _w, _cx| ws.showing_graph())
            .unwrap()
    );

    clock.pause(&mut cx, NODE_DWELL * 2);
    assert_eq!(
        card(&window, &mut cx),
        None,
        "the picture is not on screen, so it asks for no card"
    );
}

/// The card is placed against the node every frame rather than once when it opened, so a
/// view that moves under a still pointer takes the card with it. The wheel is the test's
/// way to move the view without a press.
#[gpui::test]
fn the_card_travels_with_the_node(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "alpha\n"), ("b.md", "beta\n")]);
    let clock = Clock(cx.executor());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    on_graph(&mut cx, &window);
    let (slot, at) = node(&mut cx, &window, "a.md");
    rest(&mut cx, &clock, at);
    let before = place(&mut cx, &window).expect("the card has a place");

    // Zoom about a point 40px to the node's left — inside the canvas, but not on the dot,
    // because a zoom anchored on the dot holds it in place and proves nothing. The wheel
    // goes **out**: a two-note graph fits itself to `MAX_ZOOM`, so zooming in from there
    // is the one wheel the view refuses.
    let anchor = gpui::point(at.x - gpui::px(40.), at.y);
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: anchor,
        delta: gpui::ScrollDelta::Lines(gpui::point(0.0, -1.0)),
        modifiers: gpui::Modifiers::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    // Draw, do not settle: settling re-fits the view, which would put the dot back where
    // it started and make the test prove nothing.
    frame(&mut cx);
    let (zoom, _pan) = window
        .update(&mut cx, |ws, _window, cx| ws.graph_camera(cx))
        .unwrap()
        .expect("the canvas has framed the graph");
    assert!(zoom < 8.0, "the wheel zoomed the view out (zoom {zoom})");

    let moved = window
        .update(&mut cx, |ws, _window, cx| ws.graph_node_screen(slot, cx))
        .unwrap()
        .expect("the node is still on screen");
    assert_ne!(
        [at.x.as_f32(), at.y.as_f32()],
        moved,
        "the dot itself moved on screen"
    );
    let after = place(&mut cx, &window).expect("the card still has a place");
    assert_ne!(
        (before.origin.x.as_f32(), before.origin.y.as_f32()),
        (after.origin.x.as_f32(), after.origin.y.as_f32()),
        "the view moved, so the card moved"
    );
    // Centred under the dot it belongs to, in both positions.
    let centre = (after.origin.x + after.size.width / 2.).as_f32();
    assert!(
        (centre - moved[0]).abs() <= 1.0,
        "the card sits under its node: card centre {centre}, node x {}",
        moved[0]
    );
}
