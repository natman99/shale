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
fn ctrl_w_S_splits_with_the_note_in_hand(cx: &mut TestAppContext) {
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
fn a_note_open_in_another_pane_is_found_not_duplicated(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    cx.simulate_keystrokes("ctrl-w v");
    show(&mut cx, &window, "b.md");
    show(&mut cx, &window, "a.md");
    assert_eq!(
        active(&window, &mut cx),
        0,
        "the pane showing a.md took the keyboard"
    );
    assert_eq!(
        pane_tabs(&window, &mut cx, 1),
        vec!["b.md"],
        "no third copy appeared"
    );
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
