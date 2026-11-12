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

/// The note on screen, its text, and what the window last said. The link views change
/// all three, so the M3 tests read them side by side.
fn note(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<PathBuf> {
    window
        .update(cx, |ws, _window, _cx| ws.active_path())
        .unwrap()
}

fn text(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap()
}

fn counts(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<Option<usize>> {
    window
        .update(cx, |ws, _window, _cx| ws.sidebar_counts())
        .unwrap()
}

fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
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
fn the_view_keys_cycle_through_every_view(cx: &mut TestAppContext) {
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

    // The command line names a view directly, which is how a view three steps away is
    // reached without walking. The command line is the note's verb, so `escape` first —
    // the strip's own keys are the tree's, and neither strip claims `:`.
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
// ----- the structure acts (sidebar-plan.md M2) ---------------------

/// The note's text as the editor holds it, unsaved edits included: the structure acts
/// edit the buffer, and a test that read the file on disk would be testing a save.
fn body(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap()
}

/// What the status line said last, which is how a refusal is checked: the act's other
/// half is the sentence, and a keystroke that does nothing quietly is the failure mode
/// the plan called out (EDGE-S9).
fn said(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// The strip with the keyboard in it, and its selection walked down `to` rows.
fn enter_strip(cx: &mut VisualTestContext, to: usize) {
    cx.simulate_keystrokes("ctrl-w l");
    cx.run_until_parked();
    for _ in 0..to {
        cx.simulate_keystrokes("j");
        cx.run_until_parked();
    }
    repaint(cx);
}

#[gpui::test]
fn shift_j_moves_a_section_and_everything_nested_under_it(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    enter_strip(&mut cx, 1); // `## Second`
    assert_eq!(selection(&window, &mut cx), Some(4));

    cx.simulate_keystrokes("shift-j");
    cx.run_until_parked();
    repaint(&mut cx);

    assert_eq!(
        body(&window, &mut cx),
        "# Top\n\nintro\n\n## Fourth\n\nlast\n\n## Second\n\nmore\n\n### Third\n\ntext\n\n",
        "the section moved whole: `### Third` went with its parent, not after it"
    );
    // The selection followed the heading to the line it landed on. A selection left on
    // the line it vacated is how the next keystroke moves the wrong section.
    assert_eq!(selection(&window, &mut cx), Some(8));
    // And the rows the strip draws are the note's, in the note's new order.
    assert_eq!(
        rows(&window, &mut cx)
            .iter()
            .map(|(_, _, text)| text.as_str())
            .collect::<Vec<_>>(),
        vec!["Top", "Fourth", "Second", "Third"]
    );
    assert!(
        said(&window, &mut cx).contains("moved"),
        "and the strip said what it did: {}",
        said(&window, &mut cx)
    );
}

#[gpui::test]
fn shift_k_moves_a_section_up_past_its_sibling(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    enter_strip(&mut cx, 3); // `## Fourth` (rows: Top, Second, Third, Fourth)
    assert_eq!(selection(&window, &mut cx), Some(12));

    cx.simulate_keystrokes("shift-k");
    cx.run_until_parked();
    repaint(&mut cx);

    assert_eq!(
        body(&window, &mut cx),
        "# Top\n\nintro\n\n## Fourth\n\nlast\n\n## Second\n\nmore\n\n### Third\n\ntext\n\n",
        "up is the same swap, from the other side"
    );
    assert_eq!(selection(&window, &mut cx), Some(4));
}

#[gpui::test]
fn a_move_never_lifts_a_section_out_from_under_its_parent(cx: &mut TestAppContext) {
    // EDGE-S2: `## E`'s nearest same-level heading above is `## B`, but B lives under
    // `# A`. Moving E up would re-parent it, which is a different act than a reorder.
    const NESTED: &str = "# A\n\n## B\n\nb\n\n# D\n\n## E\n\ne\n";
    let (_dir, window, mut cx) = strip_over(cx, NESTED);
    enter_strip(&mut cx, 3); // `## E`
    assert_eq!(selection(&window, &mut cx), Some(8));

    cx.simulate_keystrokes("shift-k");
    cx.run_until_parked();
    repaint(&mut cx);

    assert_eq!(
        body(&window, &mut cx),
        NESTED,
        "the note is untouched by a move it refused"
    );
    assert_eq!(
        said(&window, &mut cx),
        "“E” has no sibling under the same parent, so it cannot move up"
    );
}

#[gpui::test]
fn tab_relevels_a_heading_and_its_subtree_as_one_edit(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    enter_strip(&mut cx, 1); // `## Second`

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    repaint(&mut cx);

    assert_eq!(
        body(&window, &mut cx),
        "# Top\n\nintro\n\n# Second\n\nmore\n\n## Third\n\ntext\n\n## Fourth\n\nlast\n",
        "the heading went up a level and `### Third` came with it, keeping the shape"
    );
    assert_eq!(
        rows(&window, &mut cx)
            .iter()
            .map(|(_, depth, _)| *depth)
            .collect::<Vec<_>>(),
        vec![1, 1, 2, 2],
        "and the strip's own rows agree with the note"
    );

    // One grouped edit: a single `u` in the note puts the whole subtree back. If the
    // relevel had gone in as four separate edits, this would restore one heading and
    // leave the rest of the subtree changed.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes("u");
    cx.run_until_parked();
    assert_eq!(
        body(&window, &mut cx),
        OUTLINE,
        "one `u`, the whole subtree"
    );
}

#[gpui::test]
fn shift_tab_demotes_a_heading(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    enter_strip(&mut cx, 1); // `## Second`

    cx.simulate_keystrokes("shift-tab");
    cx.run_until_parked();
    repaint(&mut cx);

    assert_eq!(
        body(&window, &mut cx),
        "# Top\n\nintro\n\n### Second\n\nmore\n\n#### Third\n\ntext\n\n## Fourth\n\nlast\n",
        "and down takes the subtree the same way up does"
    );
    assert!(said(&window, &mut cx).contains("###"));
}

#[gpui::test]
fn a_top_level_heading_cannot_be_promoted(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    enter_strip(&mut cx, 0); // `# Top`

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    repaint(&mut cx);

    assert_eq!(
        body(&window, &mut cx),
        OUTLINE,
        "there is no level above `#`"
    );
    assert_eq!(
        said(&window, &mut cx),
        "“Top” is already a top-level heading"
    );
}

#[gpui::test]
fn a_sixth_level_heading_cannot_be_demoted(cx: &mut TestAppContext) {
    // The other end of the scale. The refusal is the same shape — a reason, not silence.
    const DEEP: &str = "###### Six\n\nbody\n";
    let (_dir, window, mut cx) = strip_over(cx, DEEP);
    enter_strip(&mut cx, 0);

    cx.simulate_keystrokes("shift-tab");
    cx.run_until_parked();
    repaint(&mut cx);

    assert_eq!(body(&window, &mut cx), DEEP);
    assert_eq!(
        said(&window, &mut cx),
        "“Six” or something under it is already ######"
    );
}

#[gpui::test]
fn a_relevel_keeps_the_headings_block_id_and_it_still_names_the_block(cx: &mut TestAppContext) {
    // EDGE-S10: the edit is the marker run, not the heading line. Rewriting the line is
    // the shortcut that would silently drop the anchor — and an anchor that stops
    // resolving because a heading changed level is a broken link in every other note.
    const ANCHORED: &str = "## The Fall ^fall\n\nbody\n";
    let (_dir, window, mut cx) = strip_over(cx, ANCHORED);
    enter_strip(&mut cx, 0);

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    repaint(&mut cx);

    let text = body(&window, &mut cx);
    assert_eq!(text, "# The Fall ^fall\n\nbody\n");
    let map = shale_core::mdstyle::block_map(&text);
    assert!(
        map.anchors.iter().any(|anchor| anchor.id == "fall"),
        "and `^fall` still names a block in the relevelled note: {map:?}",
    );
}

#[gpui::test]
fn a_move_carries_the_headings_block_id_with_it(cx: &mut TestAppContext) {
    const ANCHORED: &str = "## A ^one\n\na\n\n## B ^two\n\nb\n";
    let (_dir, window, mut cx) = strip_over(cx, ANCHORED);
    enter_strip(&mut cx, 1); // `## B ^two`

    cx.simulate_keystrokes("shift-k");
    cx.run_until_parked();
    repaint(&mut cx);

    let text = body(&window, &mut cx);
    // The chunks move verbatim, so the blank line that used to separate them now trails
    // the note: the spacing belongs to the section that had it.
    assert_eq!(text, "## B ^two\n\nb\n\n## A ^one\n\na\n\n");
    let map = shale_core::mdstyle::block_map(&text);
    for id in ["two", "one"] {
        assert!(
            map.anchors.iter().any(|anchor| anchor.id == id),
            "`^{id}` still names a block after the move: {map:?}",
        );
    }
}

#[gpui::test]
fn the_structure_keys_refuse_a_read_only_surface(cx: &mut TestAppContext) {
    // A preview is read **to be read**. The buffer's own guard would drop the edit in
    // silence, so the strip asks first and says what it would have done.
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes(":");
    cx.simulate_input("preview");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    enter_strip(&mut cx, 1); // `## Second`

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    repaint(&mut cx);

    assert_eq!(
        body(&window, &mut cx),
        OUTLINE,
        "the preview's text is untouched"
    );
    assert_eq!(
        said(&window, &mut cx),
        "this note is read-only: cannot promote"
    );
}

#[gpui::test]
fn the_structure_keys_are_the_strips_and_not_the_notes(cx: &mut TestAppContext) {
    // `Tab` is bound in the strip's context only. In the note it must still be whatever
    // the note does with it — a context leak here would make `Tab` relevel headings from
    // inside the buffer, which is not what anyone pressed.
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    repaint(&mut cx);
    assert_eq!(
        body(&window, &mut cx),
        OUTLINE,
        "Tab in the note did not relevel anything"
    );
    assert!(
        selection(&window, &mut cx).is_none(),
        "and the strip never acted, because it never had the keyboard"
    );
}

// ---------------------------------------------------------------------------
// M3: the links, both directions
// ---------------------------------------------------------------------------

/// The strip over a vault **with** an index. The link views answer from the index, and
/// the fixture every other sidebar test uses runs with it off — the same reason `:props`
/// says it cannot look there.
fn strip_over_indexed(
    cx: &mut TestAppContext,
    notes: &[(&str, &str)],
    open: &str,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = indexed_vault(cx, notes);
    show_at(cx, &window, open);
    cx.run_until_parked();
    let cx = VisualTestContext::from_window(window.into(), cx);
    (dir, window, cx)
}

/// A note that links, and the note it links to. The link is on line 2.
const LINKER: &str = "intro\n\nlinks to [[Target]] here\n";
const TARGET: &str = "# Target\n\nbeing linked\n";

#[gpui::test]
fn the_backlinks_view_lists_the_note_that_links_here(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over_indexed(
        cx,
        &[("linker.md", LINKER), ("target.md", TARGET)],
        "target.md",
    );
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("]");
    repaint(&mut cx);

    assert_eq!(
        rows(&window, &mut cx),
        vec![(0, 1, "linker".to_string())],
        "the row names the note that links, without the `.md` a reader knows is there"
    );
}

#[gpui::test]
fn a_backlink_row_jumps_to_the_line_that_links(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over_indexed(
        cx,
        &[("linker.md", LINKER), ("target.md", TARGET)],
        "target.md",
    );
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("]");
    repaint(&mut cx);

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(note(&window, &mut cx), Some(PathBuf::from("linker.md")));
    assert_eq!(
        caret_row(&window, &mut cx),
        2,
        "the strip opened the note at the line the link is on, not at its top: the row \\\n         exists to show that line"
    );
}

#[gpui::test]
fn the_outgoing_view_lists_what_this_note_links(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = strip_over_indexed(
        cx,
        &[("linker.md", LINKER), ("target.md", TARGET)],
        "linker.md",
    );
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ]");
    repaint(&mut cx);

    assert_eq!(rows(&window, &mut cx), vec![(0, 1, "target".to_string())]);
    // The row's key is its place in the list, not a line in either note: the line the
    // index holds for this direction sits in the note we came from.
    assert_eq!(selection(&window, &mut cx), Some(0));
}

#[gpui::test]
fn a_ghost_link_is_a_row_and_refuses_to_open(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) =
        strip_over_indexed(cx, &[("g.md", "see [[Nowhere]] here\n")], "g.md");
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ]");
    repaint(&mut cx);

    assert_eq!(
        rows(&window, &mut cx),
        vec![(0, 1, "Nowhere".to_string())],
        "EDGE-S7: the hole in the picture is a fact about the note, and the name shown is \\\n         what the author typed"
    );

    let before = window
        .update(&mut cx, |ws, _window, cx| ws.tab_paths(cx))
        .unwrap();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(
        status(&window, &mut cx),
        "“Nowhere” is not a note in this vault",
        "the row is real and the name is not a note; opening it would invent a file from a typo"
    );
    assert_eq!(note(&window, &mut cx), Some(PathBuf::from("g.md")));
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.tab_paths(cx))
            .unwrap(),
        before,
        "and no tab was invented for a name that has no note"
    );
}

#[gpui::test]
fn the_structure_keys_refuse_when_the_strip_is_not_showing_the_outline(cx: &mut TestAppContext) {
    // A link row's key is its place in a list of other notes. Read as a buffer line, that
    // number would move a section nobody pointed at.
    let (_dir, window, mut cx) = strip_over_indexed(cx, &[("a.md", OUTLINE)], "a.md");
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("]");
    repaint(&mut cx);

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(
        status(&window, &mut cx),
        "the strip is showing Links to here, not the outline"
    );
    assert_eq!(text(&window, &mut cx), OUTLINE, "the note was not touched");
}

#[gpui::test]
fn an_index_backed_view_says_there_is_no_index(cx: &mut TestAppContext) {
    // The default fixture runs with the index off. An empty list there would claim
    // "nothing links here" about a vault nothing looked at (EDGE-S6).
    let (_dir, window, mut cx) = strip_over(cx, OUTLINE);
    let said = |view, cx: &mut VisualTestContext| {
        window
            .update(cx, |ws, _window, _cx| ws.sidebar_empty_said(view))
            .unwrap()
    };
    assert_eq!(
        said(crate::sidebar::View::Backlinks, &mut cx),
        shale_core::db::NO_INDEX
    );
    assert_eq!(
        said(crate::sidebar::View::Outline, &mut cx),
        "no sections in this note",
        "the outline reads the buffer, so the index being off changes nothing about it"
    );
}

// ---------------------------------------------------------------------------
// M4: tags and properties
// ---------------------------------------------------------------------------

/// A note with two tags in its body and one in its frontmatter, and another note that
/// carries one of them.
const TAGGED: &str = "---\ntags: prep\n---\n\nnotes #work and #solo\n";

#[gpui::test]
fn the_tags_view_lists_the_notes_tags_with_their_census(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) =
        strip_over_indexed(cx, &[("a.md", TAGGED), ("b.md", "more #work\n")], "a.md");
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ] ]");
    repaint(&mut cx);

    assert_eq!(
        rows(&window, &mut cx),
        vec![
            (0, 1, "work".to_string()),
            (1, 1, "solo".to_string()),
            (2, 1, "prep".to_string()),
        ],
        "the body's tags first, then the frontmatter's, in the order the note carries them"
    );
    assert_eq!(
        counts(&window, &mut cx),
        vec![Some(1), Some(0), Some(0)],
        "how many **other** notes carry each: `work` is in b.md, and a lonely tag says so \
         with a real zero"
    );
}

#[gpui::test]
fn a_tag_row_hands_its_tag_to_the_search(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) =
        strip_over_indexed(cx, &[("a.md", TAGGED), ("b.md", "more #work\n")], "a.md");
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ] ]");
    repaint(&mut cx);

    // The view opens on its first row, so `enter` asks that one.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let picked = window
        .update(&mut cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        picked,
        vec!["a.md", "b.md", "save tag:work"],
        "the tag goes to the surface that answers vault questions, with its query already \
         in the field — the `:keys` picker's rule, not a second list of notes"
    );
}

#[gpui::test]
fn an_unsaved_tag_is_a_row_before_the_index_hears_of_it(cx: &mut TestAppContext) {
    // EDGE-S5 for the Tags view: the tags are the note's own, so a tag typed a second
    // ago is a row immediately. Only the census waits for the index.
    let (_dir, window, mut cx) = strip_over(cx, "notes\n");
    cx.simulate_keystrokes("G");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" #fresh");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    repaint(&mut cx);
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ] ]");
    repaint(&mut cx);

    assert_eq!(rows(&window, &mut cx), vec![(0, 1, "fresh".to_string())]);
}

#[gpui::test]
fn a_tag_view_with_no_index_still_lists_the_tags(cx: &mut TestAppContext) {
    // The rows are the buffer's; only the number is the vault's answer. No index means
    // no number — not a screenful of zeros, which would claim nobody else writes them.
    let (_dir, window, mut cx) = strip_over(cx, "notes #work\n");
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ] ]");
    repaint(&mut cx);

    assert_eq!(rows(&window, &mut cx), vec![(0, 1, "work".to_string())]);
    assert_eq!(counts(&window, &mut cx), vec![None]);
}

const PROPS: &str = "---\ncr: 6\nally: [[Goblin]]\n---\n\n# Goblin\n";

#[gpui::test]
fn the_properties_view_lists_the_frontmatter_in_authored_order(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) =
        strip_over_indexed(cx, &[("a.md", PROPS), ("Goblin.md", "# Goblin\n")], "a.md");
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ] ] ]");
    repaint(&mut cx);

    assert_eq!(
        rows(&window, &mut cx),
        vec![
            (0, 1, "cr: 6".to_string()),
            (1, 1, "ally: [[Goblin]]".to_string()),
        ],
        "as authored, in authored order — the value keeps the brackets the author wrote"
    );
}

#[gpui::test]
fn a_link_shaped_property_opens_the_note_it_names(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) =
        strip_over_indexed(cx, &[("a.md", PROPS), ("Goblin.md", "# Goblin\n")], "a.md");
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ] ] ]");
    repaint(&mut cx);
    cx.simulate_keystrokes("j");
    repaint(&mut cx);

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(note(&window, &mut cx), Some(PathBuf::from("Goblin.md")));
}

#[gpui::test]
fn a_property_that_is_not_a_link_does_not_open_anything(cx: &mut TestAppContext) {
    // The indexer's rule, kept in the strip: only the `[[…]]` spelling is a link. A
    // statblock that says `name: Goblin` in a vault that has a `Goblin.md` is a string.
    let (_dir, window, mut cx) = strip_over_indexed(
        cx,
        &[
            ("a.md", "---\nname: Goblin\n---\n\nplain\n"),
            ("Goblin.md", "# Goblin\n"),
        ],
        "a.md",
    );
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);
    cx.simulate_keystrokes("] ] ] ]");
    repaint(&mut cx);

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        note(&window, &mut cx),
        Some(PathBuf::from("a.md")),
        "the note did not move"
    );
    let picked = window
        .update(&mut cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert!(
        picked.last().is_some_and(|row| row == "save prop:name"),
        "the row asked its key instead: {picked:?}"
    );
}

// ---------------------------------------------------------------------------
// M5: the pin
// ---------------------------------------------------------------------------

/// What the strip is stuck to, if anything: the fact the header's mark carries.
fn pinned(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<PathBuf> {
    window
        .update(cx, |ws, _window, _cx| ws.sidebar_pinned())
        .unwrap()
}

#[gpui::test]
fn the_strip_pins_to_a_note_and_stops_following(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", OUTLINE), ("b.md", "# Other\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "a.md");
    cx.run_until_parked();
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);

    cx.simulate_keystrokes("p");
    cx.run_until_parked();
    assert_eq!(pinned(&window, &mut cx), Some(PathBuf::from("a.md")));
    assert_eq!(status(&window, &mut cx), "the strip is pinned to a.md");

    // A tab switch is the whole reason the pin exists: the note on screen moves on, and
    // the strip does not.
    show_at(&mut cx, &window, "b.md");
    repaint(&mut cx);
    assert_eq!(note(&window, &mut cx), Some(PathBuf::from("b.md")));
    assert_eq!(
        rows(&window, &mut cx),
        vec![
            (0, 1, "Top".to_string()),
            (4, 2, "Second".to_string()),
            (8, 3, "Third".to_string()),
            (12, 2, "Fourth".to_string()),
        ],
        "the rows are still a.md's outline, not the note that is on screen now"
    );
}

#[gpui::test]
fn pinned_again_lets_the_strip_follow_the_note_again(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", OUTLINE), ("b.md", "# Other\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "a.md");
    cx.run_until_parked();
    cx.simulate_keystrokes("space b");
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-w l");
    repaint(&mut cx);

    cx.simulate_keystrokes("p p");
    cx.run_until_parked();
    assert_eq!(pinned(&window, &mut cx), None);
    assert_eq!(status(&window, &mut cx), "the strip follows the note again");

    show_at(&mut cx, &window, "b.md");
    repaint(&mut cx);
    assert_eq!(rows(&window, &mut cx), vec![(0, 1, "Other".to_string())]);
}

#[gpui::test]
fn the_pin_survives_a_session_round_trip(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", OUTLINE)]);
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
        cx.simulate_keystrokes("ctrl-w l");
        cx.run_until_parked();
        cx.simulate_keystrokes("p");
        cx.run_until_parked();
        window
            .update(cx, |ws, _window, cx| ws.session_snapshot(cx))
            .unwrap()
    };
    assert_eq!(
        snapshot.sidebar_pinned.as_deref(),
        Some("a.md"),
        "a pinned subject is a decision the user made, so it rides the session"
    );

    let (_dir2, restored) = vault_with_session(cx, &[("a.md", OUTLINE)], &snapshot);
    let cx = &mut VisualTestContext::from_window(restored.into(), cx);
    cx.run_until_parked();
    assert_eq!(
        restored
            .update(cx, |ws, _window, _cx| ws.sidebar_pinned())
            .unwrap(),
        Some(PathBuf::from("a.md")),
        "and it is pinned again in the window that opens from the file"
    );
}
