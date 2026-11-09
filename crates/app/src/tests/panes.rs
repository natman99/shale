//! Panes (roadmap P3.2): the tiling tree, the `ctrl-w` alphabet, and the
//! rules that only matter once there is more than one pane — tab operations
//! belong to the pane with the keyboard, a note open in another pane is found
//! rather than duplicated, the graph stays one tab per window, and an exit
//! owes every note on every bar.

use super::*;

/// A vault with two notes, `a.md` open.
fn two_notes(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(cx, &[("a.md", "first\n"), ("b.md", "second\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window, "a.md");
    (dir, window, cx)
}

/// Open a note through the same door the tree and the picker use.
fn show<C: gpui::AppContext>(cx: &mut C, window: &WindowHandle<Workspace>, note: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(note), cx))
        .unwrap();
}

fn panes(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, _cx| ws.pane_count())
        .unwrap()
}

fn active(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, _cx| ws.active_pane_ix())
        .unwrap()
}

fn shape(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> &'static str {
    window
        .update(cx, |ws, _window, _cx| ws.layout_shape())
        .unwrap()
}

/// The tab bar of one pane: its notes, in order.
fn pane_tabs(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
    pix: usize,
) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| {
            ws.pane_tab_paths(pix, cx)
                .into_iter()
                .map(|p| {
                    p.map(|p| p.display().to_string())
                        .unwrap_or_else(|| "—".to_string())
                })
                .collect()
        })
        .unwrap()
}

/// Type a command line and press enter.
fn command_line(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

fn say(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// `ctrl-w s` tiles a second pane under this one: the new pane is empty, the
/// old pane keeps its tabs, and the keyboard follows the split.
#[gpui::test]
fn ctrl_w_s_tiles_a_second_pane_below(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w s");
    assert_eq!(panes(&window, &mut cx), 2);
    assert_eq!(shape(&window, &mut cx), "col", "`s` stacks");
    assert_eq!(
        active(&window, &mut cx),
        1,
        "the keyboard follows the split"
    );
    assert_eq!(pane_tabs(&window, &mut cx, 0), vec!["a.md"]);
    assert_eq!(
        pane_tabs(&window, &mut cx, 1),
        vec!["—"],
        "the new pane starts empty"
    );
}

/// `ctrl-w v` tiles sideways.
#[gpui::test]
fn ctrl_w_v_tiles_a_second_pane_beside(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    assert_eq!(panes(&window, &mut cx), 2);
    assert_eq!(shape(&window, &mut cx), "row", "`v` lays them out in a row");
}

/// `ctrl-w S` splits with the note in hand: the same note in two panes, each
/// with its own tab — the "top here, bottom there" case.
#[gpui::test]
fn ctrl_w_s_splits_with_the_note_in_hand(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w S");
    assert_eq!(pane_tabs(&window, &mut cx, 1), vec!["a.md"]);
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "first\n",
        "the carried pane shows the same note"
    );
}

/// Tab operations belong to the pane with the keyboard: `ctrl-t` there adds a
/// tab there, and leaves the other pane's bar alone.
#[gpui::test]
fn tab_keys_belong_to_the_pane_with_the_keyboard(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    cx.simulate_keystrokes("ctrl-t");
    assert_eq!(
        pane_tabs(&window, &mut cx, 1).len(),
        2,
        "the active pane grew a tab"
    );
    assert_eq!(
        pane_tabs(&window, &mut cx, 0),
        vec!["a.md"],
        "the other bar is untouched"
    );
}

/// A split has to paint both notes, not just hold both buffers: the state
/// tests above prove the model; this one proves the layout. Each pane's first
/// line must have a hit box, and the two boxes must not overlap.
#[gpui::test]
fn a_split_paints_both_notes(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let left = window
        .update(&mut cx, |ws, _w, cx| ws.pane_hit_box(0, cx))
        .unwrap();
    let right = window
        .update(&mut cx, |ws, _w, cx| ws.pane_hit_box(1, cx))
        .unwrap();
    let (ltop, lbot, lx) = left.expect("pane 0 painted its first line");
    let (rtop, rbot, rx) = right.expect("pane 1 painted its first line");
    assert!(lbot > ltop && rbot > rtop, "both boxes have height");
    assert!(rx > lx, "the right pane sits to the right of the left one");
    // The regression this guards: gpui does not stretch flex items the way CSS
    // does, so a split wrapper that only sets the main axis leaves the pane
    // sized to its content — the editor gets zero height and paints nothing
    // while the tab bar stays visible. The layout box must be tall, not just
    // the painted line.
    for pix in 0..2 {
        let h = window
            .update(&mut cx, |ws, _w, cx| ws.pane_editor_height(pix, cx))
            .unwrap();
        assert!(h > 100.0, "pane {pix}'s editor was given {h}px of height");
    }
    // The faded cursor follows the keyboard: the pane that does not have it
    // draws its caret at low alpha.
    let dim = |pix: usize, cx: &mut TestAppContext| {
        window
            .update(cx, |ws, _w, cx| ws.pane_cursor_dim(pix, cx))
            .unwrap()
    };
    assert!(dim(0, &mut cx), "the unfocused pane's cursor is faded");
    assert!(!dim(1, &mut cx), "the focused pane's cursor is solid");
    cx.simulate_keystrokes("ctrl-w h");
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert!(!dim(0, &mut cx), "stepping left makes pane 0 solid");
    assert!(dim(1, &mut cx), "and pane 1 faded");
}

/// `ctrl-w h/l` step sideways between panes.
#[gpui::test]
fn ctrl_w_h_l_step_between_panes(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    assert_eq!(active(&window, &mut cx), 1);
    cx.simulate_keystrokes("ctrl-w h");
    assert_eq!(
        active(&window, &mut cx),
        0,
        "left steps back to the first pane"
    );
    cx.simulate_keystrokes("ctrl-w l");
    assert_eq!(active(&window, &mut cx), 1, "right steps forward again");
    cx.simulate_keystrokes("ctrl-w l");
    assert_eq!(
        active(&window, &mut cx),
        1,
        "the rightmost pane keeps the keyboard"
    );
}

/// `ctrl-w j/k` step down and up a stack.
#[gpui::test]
fn ctrl_w_j_k_step_down_and_up_a_stack(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w s");
    assert_eq!(active(&window, &mut cx), 1);
    cx.simulate_keystrokes("ctrl-w k");
    assert_eq!(active(&window, &mut cx), 0, "up steps to the pane above");
    cx.simulate_keystrokes("ctrl-w j");
    assert_eq!(active(&window, &mut cx), 1, "down steps back");
    cx.simulate_keystrokes("ctrl-w k ctrl-w k");
    assert_eq!(active(&window, &mut cx), 0, "up at the top stays put");
}

/// Stepping left out of the leftmost pane lands in the tree, and stepping
/// right from the tree enters the leftmost pane — one continuous habit from
/// the two-panel window.
#[gpui::test]
fn the_sidebar_sits_at_the_left_edge(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v ctrl-w h ctrl-w h");
    assert!(
        window
            .update(&mut cx, |ws, window, _cx| ws
                .tree_focus()
                .is_focused(window))
            .unwrap(),
        "the tree has the keyboard"
    );
    cx.simulate_keystrokes("ctrl-w l i y");
    assert_eq!(
        active(&window, &mut cx),
        0,
        "the tree's right neighbour is the leftmost pane"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "yfirst\n",
        "and the keyboard is in that note, at the caret it kept"
    );
}

/// Closing a pane writes its notes first — including a note that is not on
/// screen anywhere else.
#[gpui::test]
fn closing_a_pane_writes_its_note(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" edited");
    cx.simulate_keystrokes("ctrl-w q");
    assert_eq!(panes(&window, &mut cx), 1, "the pane is gone");
    let on_disk = std::fs::read_to_string(dir.path().join("b.md")).unwrap();
    assert_eq!(on_disk, "second edited\n", "and its note reached the disk");
}

/// The last pane refuses to close rather than leaving a window with nowhere
/// to type, and says so.
#[gpui::test]
fn the_last_pane_refuses_to_close(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w q");
    assert_eq!(panes(&window, &mut cx), 1);
    assert_eq!(say(&window, &mut cx), "this is the only pane");
}

/// `ctrl-w o` keeps the pane in hand and closes the rest.
#[gpui::test]
fn only_keeps_the_pane_in_hand(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v ctrl-w v");
    assert_eq!(panes(&window, &mut cx), 3);
    show(&mut cx, &window, "b.md");
    cx.simulate_keystrokes("ctrl-w o");
    assert_eq!(panes(&window, &mut cx), 1);
    assert_eq!(
        shape(&window, &mut cx),
        "leaf",
        "and the tree collapses back to one pane"
    );
    assert_eq!(
        pane_tabs(&window, &mut cx, 0),
        vec!["b.md"],
        "the pane that had the keyboard is the one kept, tabs and all"
    );
}

/// Opening a note that another pane already shows focuses that pane rather
/// than putting the note on two bars at once.
#[gpui::test]
fn a_note_open_in_another_pane_is_stolen_not_duplicated(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    // Standing in pane 1, ask for a.md — which pane 0 already shows.
    show(&mut cx, &window, "a.md");
    assert_eq!(
        active(&window, &mut cx),
        1,
        "the keyboard stayed where the user was standing"
    );
    assert_eq!(
        pane_tabs(&window, &mut cx, 1),
        vec!["b.md".to_string(), "a.md".to_string()],
        "the tab came here, and no second copy appeared"
    );
    assert_eq!(
        pane_tabs(&window, &mut cx, 0),
        vec!["—".to_string()],
        "the pane it left keeps a blank tab rather than closing"
    );
}

/// The tab travels **whole**: the editor it owns comes with its edits, its cursor and
/// its undo history, so stealing is a move and not a re-open.
#[gpui::test]
fn a_stolen_tab_keeps_its_editor(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "a.md");
    assert_eq!(
        buffer(&window, &mut cx),
        "first\n",
        "the stolen tab is on screen"
    );
    // The keyboard follows the tab one frame later, as it does after any tab change.
    repaint(&mut cx);

    cx.simulate_keystrokes("i x escape");
    assert_eq!(buffer(&window, &mut cx), "xfirst\n");

    // Walk to the other pane — now blank — and ask for the same note again.
    cx.simulate_keystrokes("ctrl-w h");
    assert_eq!(active(&window, &mut cx), 0);
    show(&mut cx, &window, "a.md");
    repaint(&mut cx);
    assert_eq!(
        buffer(&window, &mut cx),
        "xfirst\n",
        "the edit travelled with the tab"
    );
    cx.simulate_keystrokes("u");
    assert_eq!(
        buffer(&window, &mut cx),
        "first\n",
        "and so did the undo history: the editor was moved, not reloaded"
    );
}

/// What the note in the pane on screen is, straight from its editor.
fn buffer(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor()
                .map(|editor| editor.read_with(cx, |editor, _cx| editor.text()))
                .unwrap_or_default()
        })
        .unwrap()
}

/// The graph stays one tab per *window*: `space g` from a second pane focuses
/// the pane already showing it instead of opening a second view of the same
/// physics.
#[gpui::test]
fn the_graph_stays_one_tab_across_panes(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w s");
    cx.simulate_keystrokes("space g");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_tabs())
            .unwrap(),
        1,
        "one graph tab"
    );
    assert!(window
        .update(&mut cx, |ws, _window, _cx| ws.showing_graph())
        .unwrap());
    cx.simulate_keystrokes("ctrl-w k");
    assert!(!window
        .update(&mut cx, |ws, _window, _cx| ws.showing_graph())
        .unwrap());
    cx.simulate_keystrokes("space g");
    assert_eq!(
        active(&window, &mut cx),
        1,
        "the graph pane took the keyboard back"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.graph_tabs())
            .unwrap(),
        1,
        "still one graph tab"
    );
}

/// An exit owes every note on every bar: `:wq` writes the notes in the
/// inactive panes too, and `:q` with a dirty note in *any* pane refuses.
#[gpui::test]
fn an_exit_owes_every_pane_its_write(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = two_notes(cx);
    // Autosave off: the refusal this test checks is the `:q`-with-dirty path,
    // and autosave on would quietly write and quit instead.
    window
        .update(&mut cx, |ws, _window, _cx| ws.set_autosave(false))
        .unwrap();
    cx.simulate_keystrokes("A");
    cx.simulate_input(" one");
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" two");
    // `:q` asks about dirty tabs window-wide, not just the active pane's.
    command_line("q", &mut cx);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "first\n",
        "a plain :q with a dirty note in another pane does not write it"
    );
    command_line("wq", &mut cx);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "first one\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.md")).unwrap(),
        "second two\n"
    );
}

/// The command-line spellings reach the same commands: `:split` stacks,
/// `:vsplit` tiles sideways, `:only` keeps one.
#[gpui::test]
fn the_command_line_spells_the_same_commands(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    command_line("split", &mut cx);
    assert_eq!(panes(&window, &mut cx), 2);
    assert_eq!(shape(&window, &mut cx), "col");
    command_line("vsplit", &mut cx);
    assert_eq!(panes(&window, &mut cx), 3);
    command_line("only", &mut cx);
    assert_eq!(panes(&window, &mut cx), 1);
}

// --- Resizing a pane (roadmap P3.2) ------------------------------------------

/// The width a pane has been given, as a fraction of the editing area.
fn width(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, pix: usize) -> f32 {
    window
        .update(cx, |ws, _window, _cx| ws.pane_rects())
        .unwrap()
        .into_iter()
        .find(|(ix, ..)| *ix == pix)
        .map(|(_, _, _, w, _)| w)
        .unwrap()
}

fn height(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, pix: usize) -> f32 {
    window
        .update(cx, |ws, _window, _cx| ws.pane_rects())
        .unwrap()
        .into_iter()
        .find(|(ix, ..)| *ix == pix)
        .map(|(_, _, _, _, h)| h)
        .unwrap()
}

/// `ctrl-w >` and `ctrl-w <` move the divider between the pane in hand and the rest of
/// its split. Fractions are relative, so the other pane keeps its number and takes
/// proportionally less of what is left.
#[gpui::test]
fn ctrl_w_grows_and_shrinks_the_pane_in_hand(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    assert!(
        (width(&window, &mut cx, 0) - width(&window, &mut cx, 1)).abs() < 0.001,
        "a fresh split is even"
    );

    cx.simulate_keystrokes("ctrl-w >");
    let grown = width(&window, &mut cx, 1);
    assert!(
        grown > 0.5 + 0.05,
        "the pane with the keyboard took more of the row: {grown}"
    );
    assert!(
        width(&window, &mut cx, 0) < 0.5 - 0.02,
        "and the other gave the space up"
    );

    cx.simulate_keystrokes("ctrl-w <");
    assert!(
        (width(&window, &mut cx, 1) - 0.5).abs() < 0.001,
        "one press back is the same distance"
    );
}

/// The divider belongs to the split the pane sits **directly** in. In a stack inside a
/// row, growing the bottom pane makes it taller, not wider — the row it hangs in is
/// not the split it lives in.
#[gpui::test]
fn a_resize_moves_the_split_the_pane_sits_in(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    cx.simulate_keystrokes("ctrl-w s");
    assert_eq!(panes(&window, &mut cx), 3);

    let wide = width(&window, &mut cx, 2);
    let tall = height(&window, &mut cx, 2);
    cx.simulate_keystrokes("ctrl-w >");
    assert!(
        height(&window, &mut cx, 2) > tall + 0.05,
        "the stack grew around the pane in hand"
    );
    assert!(
        (width(&window, &mut cx, 2) - wide).abs() < 0.001,
        "and the row it hangs in did not move"
    );
}

/// A window with one pane has no divider to move: the keys do nothing rather than
/// inventing a split or complaining.
#[gpui::test]
fn a_single_pane_has_no_divider_to_move(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w > ctrl-w < ctrl-w + ctrl-w -");
    assert_eq!(panes(&window, &mut cx), 1, "no split appeared");
    assert!(
        (width(&window, &mut cx, 0) - 1.0).abs() < 0.001,
        "and the one pane still has the whole window"
    );
}

/// The floor: a pane can be squeezed to a sliver, but not out of existence. A pane the
/// user cannot see is a pane they cannot give the keyboard back to.
#[gpui::test]
fn a_pane_cannot_be_squeezed_out_of_existence(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    for _ in 0..40 {
        cx.simulate_keystrokes("ctrl-w <");
    }
    let sliver = width(&window, &mut cx, 1);
    assert!(sliver > 0.05, "still a pane, not a line: {sliver}");
    assert!(sliver <= 0.25, "and stopped at the floor: {sliver}");
    cx.simulate_keystrokes("ctrl-w <");
    assert_eq!(
        width(&window, &mut cx, 1),
        sliver,
        "the floor holds against further pressing"
    );
}

// --- `:q` and the last tab belong to the pane (2026-11-05) -------------------

/// `:q` closes the pane it was typed in when there is more than one — vim's `:quit`
/// closes the window it was typed in too — and the window goes only when the last pane
/// does.
#[gpui::test]
fn q_closes_the_pane_in_hand(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    assert_eq!(panes(&window, &mut cx), 2);

    command_line("q", &mut cx);
    assert_eq!(panes(&window, &mut cx), 1, "the pane in hand closed");
    assert_eq!(
        pane_tabs(&window, &mut cx, 0),
        vec!["a.md".to_string()],
        "the window is still here, with the note that was left"
    );
    assert_eq!(active(&window, &mut cx), 0, "and the keyboard followed it");
    assert!(
        !dir.path().join(".shale/session.json").exists(),
        "and it did not leave: no session was written on the way out"
    );
}

/// The other half: with one pane there is nothing between the key and the window, so
/// `:q` still quits — the session it writes on the way out is the proof the window path
/// ran rather than the pane path.
#[gpui::test]
fn q_in_the_last_pane_still_quits(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = two_notes(cx);
    command_line("q", &mut cx);
    assert!(
        dir.path().join(".shale/session.json").exists(),
        "the window left, and an exit writes the session"
    );
    let _ = window;
}

/// The last tab in a pane closes the pane (the user's ruling, and Obsidian's rule): a
/// split should not fill up with panes nobody is looking at. The last pane keeps its
/// blank tab — see `tabs.rs` — so a window never quits because a tab closed.
#[gpui::test]
fn closing_the_last_tab_closes_the_pane(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    assert_eq!(pane_tabs(&window, &mut cx, 1), vec!["b.md".to_string()]);

    command_line("tabclose", &mut cx);
    assert_eq!(
        panes(&window, &mut cx),
        1,
        "the pane went with the last tab it held"
    );
    assert_eq!(
        pane_tabs(&window, &mut cx, 0),
        vec!["a.md".to_string()],
        "the surviving pane kept its own tab"
    );
    assert_eq!(active(&window, &mut cx), 0, "and the keyboard followed it");
}

// --- The session remembers the tiling tree (roadmap P3.2) --------------------

use shale_core::session::{LayoutSeat, PaneSeat, Session, Tab as SeatTab};

fn seat_tab(path: &str) -> SeatTab {
    SeatTab {
        path: Some(PathBuf::from(path)),
        view: "live".into(),
        graph: false,
        camera: None,
        mode: None,
        orientation: None,
    }
}

fn seat(paths: &[&str], active: usize) -> PaneSeat {
    PaneSeat {
        tabs: paths.iter().map(|p| seat_tab(p)).collect(),
        active_tab: Some(active),
    }
}

fn seat_paths(seat: &PaneSeat) -> Vec<String> {
    seat.tabs
        .iter()
        .map(|tab| {
            tab.path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "—".to_string())
        })
        .collect()
}

/// What a split window writes is the tree plus **every** pane's tabs — not the single
/// pane that had the keyboard, which is all a window used to remember.
#[gpui::test]
fn a_split_window_remembers_every_pane(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    assert_eq!(panes(&window, &mut cx), 2, "a pane beside this one");

    window
        .update(&mut cx, |ws, _window, cx| ws.save_session(cx))
        .unwrap();
    let session = Session::load(dir.path()).unwrap();
    let seats = session.pane_seats();
    assert_eq!(seats.len(), 2, "both panes are in the file");
    assert_eq!(seat_paths(&seats[0]), vec!["a.md".to_string()]);
    assert_eq!(seat_paths(&seats[1]), vec!["b.md".to_string()]);
    assert_eq!(
        session.active_pane,
        Some(1),
        "the new pane had the keyboard"
    );

    match session
        .layout
        .as_ref()
        .expect("the tree itself is in the file")
    {
        LayoutSeat::Split { dir, frac, kids } => {
            assert_eq!(dir, "row", "`ctrl-w v` divides side by side");
            assert_eq!(kids.len(), 2);
            assert!(
                matches!(kids[0], LayoutSeat::Leaf) && matches!(kids[1], LayoutSeat::Leaf),
                "two leaves, and the leaves are the `panes` list"
            );
            assert_eq!(frac.len(), 2);
            assert!(frac.iter().all(|f| *f > 0.), "every kid got some space");
        }
        other => panic!("expected a split, got {other:?}"),
    }
}

/// The fractions are why the tree is worth storing: a window that was resized before it
/// closed writes the divider where the user left it, and a window opened on that file
/// lays its panes out in that proportion.
#[gpui::test]
fn a_resized_split_remembers_where_the_divider_was(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    cx.simulate_keystrokes("ctrl-w > ctrl-w >");

    window
        .update(&mut cx, |ws, _window, cx| ws.save_session(cx))
        .unwrap();
    let session = Session::load(dir.path()).unwrap();
    match session.layout.as_ref().expect("the tree is in the file") {
        LayoutSeat::Split { frac, .. } => {
            assert_eq!(frac.len(), 2);
            assert!(
                (frac[0] - 1.0).abs() < 0.001,
                "the pane that was not touched kept its share: {:?}",
                frac
            );
            assert!(
                (frac[1] - 1.5).abs() < 0.001,
                "two presses of a quarter share each: {:?}",
                frac
            );
        }
        other => panic!("expected a split, got {other:?}"),
    }
}

/// The same file read the other way: the fractions in it decide where the divider
/// sits, not an even split.
#[gpui::test]
fn a_sessions_fractions_lay_out_the_panes(cx: &mut TestAppContext) {
    let session = Session {
        panes: vec![seat(&["a.md"], 0), seat(&["b.md"], 0)],
        layout: Some(LayoutSeat::Split {
            dir: "row".into(),
            frac: vec![1.0, 1.5],
            kids: vec![LayoutSeat::Leaf, LayoutSeat::Leaf],
        }),
        active_pane: Some(0),
        ..Session::default()
    };
    let (_dir, window) =
        session::vault_with_session(cx, &[("a.md", "first\n"), ("b.md", "second\n")], &session);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert!(
        (width(&window, &mut cx, 0) - 0.4).abs() < 0.001,
        "1 of 2.5 is 40% of the row: {}",
        width(&window, &mut cx, 0)
    );
    assert!(
        (width(&window, &mut cx, 1) - 0.6).abs() < 0.001,
        "and 1.5 of 2.5 is 60%: {}",
        width(&window, &mut cx, 1)
    );
}

/// A pane that was blank when the window quit is still a pane, and a pane always has a
/// tab: `Pane::active_tab` is total only while that holds, and a restored window of
/// blank panes used to come back with none and panic on the first look.
#[gpui::test]
fn a_session_of_blank_panes_still_has_a_tab_in_each(cx: &mut TestAppContext) {
    let session = Session {
        panes: vec![seat(&[], 0), seat(&[], 0)],
        layout: Some(LayoutSeat::Split {
            dir: "row".into(),
            frac: vec![1.0, 1.0],
            kids: vec![LayoutSeat::Leaf, LayoutSeat::Leaf],
        }),
        active_pane: Some(1),
        ..Session::default()
    };
    let (_dir, window) = session::vault_with_session(cx, &[("a.md", "first\n")], &session);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert_eq!(panes(&window, &mut cx), 2, "the split came back");
    assert_eq!(
        pane_tabs(&window, &mut cx, 0),
        vec!["—".to_string()],
        "a blank pane came back with its one blank tab"
    );
    assert_eq!(pane_tabs(&window, &mut cx, 1), vec!["—".to_string()]);
}

/// The other half: a window opened on that file comes back **split**, with each note
/// in the pane that held it and the keyboard where the user left it.
#[gpui::test]
fn a_session_with_two_panes_comes_back_split(cx: &mut TestAppContext) {
    let session = Session {
        open_note: Some(PathBuf::from("b.md")),
        tabs: vec![seat_tab("b.md")],
        active_tab: Some(0),
        panes: vec![seat(&["a.md"], 0), seat(&["b.md", "a.md"], 1)],
        layout: Some(LayoutSeat::Split {
            dir: "column".into(),
            // Relative: this is a 2:1 stack, and the fractions are the point of
            // storing them (roadmap P3.2's resize half).
            frac: vec![2.0, 1.0],
            kids: vec![LayoutSeat::Leaf, LayoutSeat::Leaf],
        }),
        active_pane: Some(1),
        ..Session::default()
    };
    let (_dir, window) =
        session::vault_with_session(cx, &[("a.md", "first\n"), ("b.md", "second\n")], &session);
    let mut cx = VisualTestContext::from_window(window.into(), cx);

    assert_eq!(panes(&window, &mut cx), 2, "the window is split again");
    assert_eq!(shape(&window, &mut cx), "col", "the way it was split");
    assert_eq!(
        pane_tabs(&window, &mut cx, 0),
        vec!["a.md".to_string()],
        "the top pane has its own tab"
    );
    assert_eq!(
        pane_tabs(&window, &mut cx, 1),
        vec!["b.md".to_string(), "a.md".to_string()],
        "and the bottom pane has both of its tabs, in order"
    );
    assert_eq!(
        active(&window, &mut cx),
        1,
        "the keyboard went back to its pane"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.status())
            .unwrap(),
        "a.md",
        "and that pane's own keyboard was on its second tab — the per-pane active tab \
         is restored, not assumed to be the first"
    );
}

/// A session file is plain text a person can edit, and a tree that does not describe
/// the panes it ships with is a log line and one pane — not a window that paints
/// nonsense, and not a panic on an out-of-range pane index.
#[gpui::test]
fn a_pane_tree_that_does_not_fit_its_panes_is_dropped(cx: &mut TestAppContext) {
    let session = Session {
        panes: vec![seat(&["a.md"], 0), seat(&["b.md"], 0)],
        layout: Some(LayoutSeat::Split {
            dir: "row".into(),
            frac: vec![1.0, 1.0],
            // Three leaves for two panes: the shape cannot be built.
            kids: vec![LayoutSeat::Leaf, LayoutSeat::Leaf, LayoutSeat::Leaf],
        }),
        active_pane: Some(7),
        ..Session::default()
    };
    let (_dir, window) =
        session::vault_with_session(cx, &[("a.md", "first\n"), ("b.md", "second\n")], &session);
    let mut cx = VisualTestContext::from_window(window.into(), cx);

    assert_eq!(
        panes(&window, &mut cx),
        1,
        "the tree that did not fit was dropped"
    );
    assert_eq!(shape(&window, &mut cx), "leaf");
    assert_eq!(active(&window, &mut cx), 0, "and the pane index with it");
    assert_eq!(
        pane_tabs(&window, &mut cx, 0),
        vec!["a.md".to_string(), "b.md".to_string()],
        "the tabs are not lost: they land in the one pane the window has"
    );
}
