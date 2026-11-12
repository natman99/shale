//! The right-hand strip (roadmap P4.13, sidebar-plan.md M1): the shell, the outline
//! view, and the keyboard that reaches it.
//!
//! The rows are asserted by **which** rows they are — line, depth, text — never by how
//! many: two notes with the same heading count are not the same outline, and a row's
//! line is what both the jump and the current-section mark are keyed to.

use shale_core::Session;

use super::session::vault_with_session;
use super::*;

/// A note with a heading at every depth the outline has to order, and body between them
/// so the caret can be somewhere that is not a heading.
const OUTLINE: &str = "\
# Top

intro

## Second

more

### Third

text

## Fourth

last
";

/// The same note with the headings the strip cannot all show: sixty of them, three lines
/// apart, so the list has a bottom edge to cross.
fn long_note() -> String {
    (0..60)
        .map(|i| format!("## H{i}\n\ntext\n"))
        .collect::<Vec<_>>()
        .join("")
}

/// The rows the last frame drew. The strip is built in the frame that draws it — the
/// tree's rule — so every read of these is after a paint.
fn rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<(usize, u8, String)> {
    window
        .update(cx, |ws, _window, _cx| ws.sidebar_rows())
        .unwrap()
}

fn strip_visible(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |ws, _window, _cx| ws.sidebar_visible())
        .unwrap()
}

/// The heading line the strip's keyboard is on. A **line**, not a row index: typing
/// changes how many rows there are, and a line survives a row being added above it.
fn selection(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<usize> {
    window
        .update(cx, |ws, _window, _cx| ws.sidebar_selection())
        .unwrap()
}

/// The section the note's caret is in, as of the last frame.
fn mark(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<usize> {
    window
        .update(cx, |ws, _window, _cx| ws.sidebar_mark())
        .unwrap()
}

fn caret_row(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| ws.cursor_position(cx).0)
        .unwrap()
}

fn strip_scroll(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> f32 {
    window
        .update(cx, |ws, _window, _cx| ws.sidebar_scroll_offset())
        .unwrap()
}

/// A vault with one note open and the strip drawn: the shape every test here starts
/// from. The keyboard stays in the note, which is what `space b` does.
fn strip_over(
    cx: &mut TestAppContext,
    text: &str,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(cx, &[("a.md", text)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "a.md");
    cx.run_until_parked();
    repaint(&mut cx);
    cx.simulate_keystrokes("space b");
    cx.run_until_parked();
    repaint(&mut cx);
    (dir, window, cx)
}

#[gpui::test]
fn the_strip_opens_and_closes(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", OUTLINE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    show_at(cx, &window, "a.md");
    cx.run_until_parked();
    repaint(cx);
    assert!(
        !strip_visible(&window, cx),
        "the strip starts out of the way, like the tree"
    );

    cx.simulate_keystrokes("space b");
    cx.run_until_parked();
    assert!(strip_visible(&window, cx), "space b opens it");

    cx.simulate_keystrokes("space b");
    cx.run_until_parked();
    assert!(!strip_visible(&window, cx), "and closes it");

    // The command line: bare is the toggle, and a word sets it either way.
    cx.simulate_keystrokes(":");
    cx.simulate_input("sidebar");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(strip_visible(&window, cx), ":sidebar opens it");

    cx.simulate_keystrokes(":");
    cx.simulate_input("sidebar off");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!strip_visible(&window, cx), ":sidebar off closes it");

    cx.simulate_keystrokes(":");
    cx.simulate_input("sidebar on");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(strip_visible(&window, cx), ":sidebar on opens it");
}

#[gpui::test]
fn the_strip_survives_a_session_round_trip(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", OUTLINE)]);
    // The keys are driven inside a block so the visual context is dropped before the
    // session is read back: `vault_with_session` needs the bare test context.
    let snapshot = {
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        window
            .update(cx, |ws, _window, cx| {
                ws.open_note(&PathBuf::from("a.md"), cx)
            })
            .unwrap();
        cx.run_until_parked();
        cx.simulate_keystrokes("space b");
        cx.run_until_parked();
        cx.simulate_keystrokes(":");
        cx.simulate_input("sidebar links");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        window
            .update(cx, |ws, _window, cx| ws.session_snapshot(cx))
            .unwrap()
    };
    assert_eq!(
        snapshot.sidebar_visible,
        Some(true),
        "the strip's visibility is the user's, so it is the session's"
    );
    assert_eq!(
        snapshot.sidebar_view.as_deref(),
        Some("links"),
        "and so is which view was showing"
    );

    // Back through the file: `vault_with_session` writes the session and opens a window
    // over it, which is the whole round trip.
    let (_dir2, restored) = vault_with_session(cx, &[("a.md", OUTLINE)], &snapshot);
    let cx = &mut VisualTestContext::from_window(restored.into(), cx);
    cx.run_until_parked();
    assert!(
        strip_visible(&restored, cx),
        "the strip came back open, in the vault it was opened in"
    );
    assert_eq!(
        restored
            .update(cx, |ws, _window, _cx| ws.sidebar_view())
            .unwrap(),
        "links",
        "on the view it was on"
    );
}

#[gpui::test]
fn a_session_that_predates_the_strip_restores_without_it(cx: &mut TestAppContext) {
    // The fields are `None` in a file written before the strip existed. That is not
    // "closed" — it is "the user never had one", and the difference is that the next
    // `space b` opens it rather than toggling a state nobody chose.
    let session = Session {
        open_note: Some(PathBuf::from("a.md")),
        sidebar_visible: None,
        sidebar_view: None,
        ..Default::default()
    };
    let (_dir, window) = vault_with_session(cx, &[("a.md", OUTLINE)], &session);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    assert!(!strip_visible(&window, cx), "an old session has no strip");
    cx.simulate_keystrokes("space b");
    cx.run_until_parked();
    assert!(
        strip_visible(&window, cx),
        "and the first `space b` opens one"
    );
}

#[gpui::test]
fn the_keyboard_walks_the_strip_and_back_to_where_it_came_from(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);

    // From the note, the strip is the next stop to the right. Focus is proven by what a
    // key does, not by a handle: `j` in the strip moves the selection, `j` in the note
    // moves the caret.
    cx.simulate_keystrokes("ctrl-w l");
    cx.run_until_parked();
    assert_eq!(
        selection(&window, &mut cx),
        Some(0),
        "the strip took the keyboard and seeded its selection at the first row"
    );
    assert_eq!(
        caret_row(&window, &mut cx),
        0,
        "the note's caret did not move"
    );
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    assert_eq!(
        selection(&window, &mut cx),
        Some(4),
        "and `j` walked the outline a row at a time"
    );

    // Back the way they came: the same pane, not the leftmost one.
    cx.simulate_keystrokes("ctrl-w h");
    cx.run_until_parked();
    cx.simulate_keystrokes("j j");
    cx.run_until_parked();
    assert_eq!(
        caret_row(&window, &mut cx),
        2,
        "ctrl-w h returned to the pane that had the keyboard"
    );

    // The tree's edge is the same rule: out to the tree, back to the seat.
    cx.simulate_keystrokes("ctrl-w h");
    cx.run_until_parked();
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.tree_selection())
            .unwrap(),
        Some(PathBuf::from("a.md")),
        "ctrl-w h from the note went to the tree"
    );
    cx.simulate_keystrokes("ctrl-w l");
    cx.run_until_parked();
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    assert_eq!(
        caret_row(&window, &mut cx),
        3,
        "and the tree's ctrl-w l came back to the same pane it left"
    );
}

#[gpui::test]
fn the_strip_has_nothing_above_or_below_it(cx: &mut TestAppContext) {
    // The strip spans the window's height, so `ctrl-w j/k` from it has nowhere to go.
    // It must not fall through to a pane, which is what a naive relative move does.
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    cx.simulate_keystrokes("ctrl-w l");
    cx.run_until_parked();
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    assert_eq!(selection(&window, &mut cx), Some(4));
    cx.simulate_keystrokes("ctrl-w j ctrl-w k");
    cx.run_until_parked();
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    assert_eq!(
        selection(&window, &mut cx),
        Some(8),
        "the strip still has the keyboard after a move with nowhere to go"
    );
}

#[gpui::test]
fn the_outline_lists_a_nested_note_in_order_and_depth(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    assert_eq!(
        rows(&window, &mut cx),
        vec![
            (0, 1, "Top".into()),
            (4, 2, "Second".into()),
            (8, 3, "Third".into()),
            (12, 2, "Fourth".into()),
        ],
        "the outline is the note's own heading spans, in order, with the depth each one \
         is at — not a second numbering of the same headings"
    );
}

#[gpui::test]
fn a_heading_typed_but_unsaved_is_a_row_immediately(cx: &mut TestAppContext) {
    // EDGE-S5: the outline reads the buffer, not the index, so a heading that exists
    // only in the window is already in the list.
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    cx.simulate_keystrokes("G");
    cx.simulate_keystrokes("o");
    cx.simulate_input("## Typed");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    repaint(&mut cx);
    let all = rows(&window, &mut cx);
    assert!(
        all.last().is_some_and(|row| row.1 == 2 && row.2 == "Typed"),
        "the unsaved heading is the last row: {all:?}"
    );
    assert!(
        window
            .update(&mut cx, |ws, _window, cx| ws.buffer_is_dirty(cx))
            .unwrap(),
        "and the note is still ahead of the disk — the row did not come from a save"
    );
}

#[gpui::test]
fn enter_on_a_heading_reveals_it_in_the_note_without_a_new_tab(cx: &mut TestAppContext) {
    // sidebar-plan.md ruling 19: the heading is in the note already on screen, so this
    // is a reveal, not an open. A tab that re-opened the note would hand back a
    // different buffer even with identical text — and would lose the unsaved work.
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    // Dirty the note first, at the end of a heading line so the heading itself survives:
    // the bug this guards is a jump that replaces a tab holding unsaved edits.
    cx.simulate_keystrokes("A");
    cx.simulate_input(" more");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let buffer_before = window
        .update(&mut cx, |ws, _window, cx| ws.buffer_identity(cx))
        .unwrap()
        .expect("a note is open");

    cx.simulate_keystrokes("ctrl-w l");
    cx.run_until_parked();
    cx.simulate_keystrokes("j j");
    cx.run_until_parked();
    assert_eq!(
        selection(&window, &mut cx),
        Some(8),
        "the selection is on the `Third` row"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    repaint(&mut cx);

    let (tabs, buffer_after, cursor) = window
        .update(&mut cx, |ws, _window, cx| {
            (
                ws.tab_paths(cx).len(),
                ws.buffer_identity(cx),
                ws.cursor_position(cx),
            )
        })
        .unwrap();
    assert_eq!(tabs, 1, "the jump opened no tab");
    assert_eq!(
        buffer_after,
        Some(buffer_before),
        "and no new buffer: the same `Rc` the unsaved edits are in"
    );
    assert_eq!(
        cursor.0, 8,
        "the caret is on the heading it jumped to (`### Third`)"
    );
    assert!(
        window
            .update(&mut cx, |ws, _window, cx| ws.buffer_is_dirty(cx))
            .unwrap(),
        "the unsaved edit is still there"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap()
            .lines()
            .next()
            .unwrap(),
        "# Top more",
        "the typed text survived the jump"
    );
    assert_eq!(
        selection(&window, &mut cx),
        Some(8),
        "and the keyboard followed into the note, leaving the selection where it was"
    );
}

#[gpui::test]
fn the_mark_follows_the_caret_across_a_section_boundary(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    assert_eq!(
        mark(&window, &mut cx),
        Some(0),
        "the caret at the top of the note is in the first section"
    );

    // `6 G` is display line 6, which is the body under `## Second`.
    cx.simulate_keystrokes("6 G");
    cx.run_until_parked();
    repaint(&mut cx);
    assert_eq!(
        caret_row(&window, &mut cx),
        5,
        "the caret is in the body under `## Second`"
    );
    assert_eq!(
        mark(&window, &mut cx),
        Some(4),
        "the mark moved to the heading of the section the caret walked into"
    );

    // Down into a deeper section: the mark is the nearest heading at or above the caret,
    // so `### Third` claims the body under it.
    // Spelled digit by digit: `simulate_keystrokes` reads a token as one key,
    // so `10` would be a key named "10" rather than a count of ten.
    cx.simulate_keystrokes("1 0 G");
    cx.run_until_parked();
    repaint(&mut cx);
    assert_eq!(
        mark(&window, &mut cx),
        Some(8),
        "a deeper heading takes the mark from its parent"
    );
}

#[gpui::test]
fn the_list_scrolls_for_the_mark_once_and_never_over_a_manual_scroll(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, &long_note());
    assert_eq!(
        strip_scroll(&window, &mut cx),
        0.,
        "the list starts at the top"
    );

    // The caret to a section far past the bottom of the list: the mark moves, so the
    // list brings it back.
    cx.simulate_keystrokes("1 2 1 G");
    cx.run_until_parked();
    repaint(&mut cx);
    assert_eq!(
        mark(&window, &mut cx),
        Some(120),
        "the caret is in the section at line 120"
    );
    let auto = strip_scroll(&window, &mut cx);
    assert!(
        auto < 0.,
        "the list scrolled to show the section the caret is in: {auto}"
    );

    // Now the user scrolls the list themselves, and then moves the caret **within the
    // same section**. The mark has not moved, so the list leaves their scroll alone —
    // even though the mark's row is off screen again. This is the whole anti-fight rule
    // (EDGE-S11): a test that only checked "the mark is visible" would stay green here
    // while the live strip fought the wheel.
    let size = window
        .update(&mut cx, |_ws, window, _cx| window.bounds().size)
        .unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(
            gpui::px(size.width.as_f32() - 130.),
            gpui::px(size.height.as_f32() / 2.),
        ),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(120.))),
        ..Default::default()
    });
    repaint(&mut cx);
    let manual = strip_scroll(&window, &mut cx);
    assert!(
        manual > auto,
        "the wheel moved the list back up: {manual} after {auto}"
    );

    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    repaint(&mut cx);
    assert_eq!(
        mark(&window, &mut cx),
        Some(120),
        "the caret is still in the same section"
    );
    assert_eq!(
        strip_scroll(&window, &mut cx),
        manual,
        "and the list did not scroll over the user's own scroll"
    );
}

#[gpui::test]
fn scrubbing_moves_the_notes_scroll_and_not_its_caret(cx: &mut TestAppContext) {
    // sidebar-plan.md ruling Q: the strip previews a section without moving the caret,
    // so backing out of the strip leaves the note exactly where it was.
    let (_dir, window, mut cx) = strip_over(cx, &long_note());
    cx.simulate_keystrokes("ctrl-w l");
    cx.run_until_parked();

    let (before_scroll, before_caret) = window
        .update(&mut cx, |ws, _window, cx| {
            (ws.note_scroll_offset(cx), ws.cursor_position(cx))
        })
        .unwrap();
    assert_eq!(before_caret.0, 0, "the caret is at the top of the note");

    for _ in 0..30 {
        cx.simulate_keystrokes("j");
    }
    cx.run_until_parked();
    repaint(&mut cx);

    let (after_scroll, after_caret) = window
        .update(&mut cx, |ws, _window, cx| {
            (ws.note_scroll_offset(cx), ws.cursor_position(cx))
        })
        .unwrap();
    assert_eq!(
        after_caret, before_caret,
        "scrubbing never moves the caret — that is the point of it"
    );
    assert!(
        after_scroll < before_scroll,
        "the note scrolled to the section under the strip's selection: {before_scroll} -> \
         {after_scroll}"
    );
    assert_eq!(
        selection(&window, &mut cx),
        Some(90),
        "the selection is the heading's buffer line, which survives a row added above it"
    );
}

#[gpui::test]
fn the_view_keys_cycle_and_an_unbuilt_view_says_so(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    let view = |cx: &mut VisualTestContext| {
        window
            .update(cx, |ws, _window, _cx| ws.sidebar_view())
            .unwrap()
    };
    assert_eq!(view(&mut cx), "outline");

    cx.simulate_keystrokes("ctrl-w l");
    cx.run_until_parked();
    cx.simulate_keystrokes("]");
    cx.run_until_parked();
    assert_eq!(view(&mut cx), "backlinks");
    cx.simulate_keystrokes("]");
    cx.run_until_parked();
    assert_eq!(view(&mut cx), "links");

    // `[` walks back one view at a time, and wraps at the top of the list rather than
    // stopping there.
    cx.simulate_keystrokes("] ]");
    cx.run_until_parked();
    assert_eq!(view(&mut cx), "props");
    for word in ["tags", "links", "backlinks", "outline", "props"] {
        cx.simulate_keystrokes("[");
        cx.run_until_parked();
        assert_eq!(view(&mut cx), word, "walking back from Properties");
    }
    assert_eq!(
        selection(&window, &mut cx),
        None,
        "and the outline's selection did not follow: a heading row and a link row are \
         not the same kind of thing"
    );

    // A view M1 does not build is still a view the user can select: the header names it
    // and the body says what is missing, rather than the key doing nothing. The command
    // line is the note's verb, so `escape` first — the strip's own keys are the tree's,
    // and neither strip claims `:`.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes(":");
    cx.simulate_input("sidebar tags");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(view(&mut cx), "tags");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.status())
            .unwrap(),
        "sidebar: Tags",
        "and the window names the view it switched to"
    );
}

#[gpui::test]
fn the_strip_refuses_the_keyboard_when_it_has_no_note_to_describe(cx: &mut TestAppContext) {
    // EDGE-S1: a graph tab has no outline. The strip stays out of the keyboard's way
    // rather than focusing an empty surface, so `ctrl-w l` from a graph pane still
    // behaves the way the pane tests already assert.
    let (_dir, window) = workspace(cx, &["a.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("space b");
    cx.run_until_parked();
    cx.simulate_keystrokes("space g g");
    cx.run_until_parked();
    repaint(cx);

    cx.simulate_keystrokes("ctrl-w l");
    cx.run_until_parked();
    cx.simulate_keystrokes("j");
    cx.run_until_parked();
    assert_eq!(
        selection(&window, cx),
        None,
        "the strip never took the keyboard from a graph tab"
    );
    assert!(
        !window
            .update(cx, |ws, window, cx| ws
                .sidebar_focus()
                .contains_focused(window, cx))
            .unwrap(),
        "and the strip never holds the window's focus either"
    );
}
