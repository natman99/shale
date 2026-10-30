//! Vim behaviour driven through the real keymap: the table keys, counts, find
//! motions, text objects, visual mode, `.` repeat and the undo/redo chords.
//! These are the tests that catch a binding that works in the core engine and
//! never reaches a window.

use super::*;

/// The table keys have to work through the real keymap, not just the core
/// engine: a shifted letter arrives as the lowercase key with the shift
/// modifier, and `|` / `\` arrive as the symbols themselves with no shift.
#[gpui::test]
fn table_edits_through_the_keymap(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("| a | b |\n| --- | --- |\n| 1 | 2 |\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    let text =
        |cx: &mut VisualTestContext| editor.read_with(cx, |editor, _cx| editor.text()).unwrap();

    cx.simulate_keystrokes("L");
    assert_eq!(
        editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap(),
        (0, 6),
        "shift-L should land on the second column"
    );
    cx.simulate_keystrokes("J");
    assert_eq!(
        editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap(),
        (2, 6),
        "shift-J skips the rule line and steps into the body row's cell"
    );
    // Plain `j`/`k` are the ordinary line motions, which follow the display
    // model: the rule row is collapsed while the cursor is below it, and drawn
    // again once the cursor is on the header — where plain `j` stops on it, so
    // its `:---:` markers can be edited.
    cx.simulate_keystrokes("k");
    assert_eq!(
        editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap(),
        (0, 6),
        "plain k steps over the collapsed rule row"
    );
    cx.simulate_keystrokes("j");
    assert_eq!(
        editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap(),
        (1, 6),
        "plain j stops on the rule row, which the header's cursor brought back"
    );
    cx.simulate_keystrokes("j");
    assert_eq!(
        editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap(),
        (2, 6),
        "and the next j moves on into the body"
    );

    // Back to the first column, then drop the row. Only the header and the rule
    // are left, and the cursor stays on the header's cell rather than the rule.
    cx.simulate_keystrokes("H d d");
    assert_eq!(text(cx), "| a | b |\n| --- | --- |\n");
    assert_eq!(
        editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap(),
        (0, 2),
        "the cell that took the deleted row's place is selected"
    );

    cx.simulate_keystrokes("D");
    assert_eq!(text(cx), "| b |\n| --- |\n", "shift-D drops a column");
    cx.simulate_keystrokes("|");
    assert_eq!(text(cx), "| b |  |\n| --- | --- |\n", "a column goes after");
    cx.simulate_keystrokes("\\");
    assert_eq!(
        text(cx),
        "| b |  |  |\n| --- | --- | --- |\n",
        "and before, and the delimiter row stays valid"
    );

    // shift-I empties the cell under the cursor and drops into insert mode. The
    // cursor is on one of the empty cells the last two edits added, so step back
    // to the column that has text.
    cx.simulate_keystrokes("H shift-i");
    assert_eq!(
        editor.read_with(cx, |editor, _cx| editor.mode()).unwrap(),
        shale_core::vim::Mode::Insert,
        "shift-I clears the cell and enters insert mode"
    );
    assert_eq!(
        text(cx),
        "|  |  |  |\n| --- | --- | --- |\n",
        "only the current cell was emptied"
    );
    cx.simulate_keystrokes("escape u");
    assert_eq!(
        text(cx),
        "| b |  |  |\n| --- | --- | --- |\n",
        "one undo puts the emptied cell back"
    );
}

/// `alt+hjkl` moves a whole row or column of the table, and each move is one
/// undo step even though it splices every row.
#[gpui::test]
fn alt_hjkl_moves_table_rows_and_columns(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let original = "| a | b |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n";
    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(original.to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    let text =
        |cx: &mut VisualTestContext| editor.read_with(cx, |editor, _cx| editor.text()).unwrap();

    // Down into the first body row, so the cursor is inside the table. shift-J
    // is the cell-to-cell motion; plain j would stop on the rule row.
    cx.simulate_keystrokes("J");
    assert_eq!(
        editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap(),
        (2, 2),
        "the cursor is on `1`"
    );

    cx.simulate_keystrokes("alt-j");
    assert_eq!(
        text(cx),
        "| a | b |\n| --- | --- |\n| 3 | 4 |\n| 1 | 2 |\n",
        "alt-j moves the row down"
    );
    cx.simulate_keystrokes("u");
    assert_eq!(text(cx), original, "one undo puts the rows back");

    cx.simulate_keystrokes("alt-l");
    assert_eq!(
        text(cx),
        "| b | a |\n| --- | --- |\n| 2 | 1 |\n| 4 | 3 |\n",
        "alt-l moves the column right"
    );
    cx.simulate_keystrokes("u");
    assert_eq!(text(cx), original, "and one undo puts the columns back");

    cx.simulate_keystrokes("alt-k");
    assert_eq!(
        text(cx),
        "| 1 | 2 |\n| --- | --- |\n| a | b |\n| 3 | 4 |\n",
        "alt-k moves the header down the list, keeping the rule under it"
    );
}

/// Counts, `$`, and `u` must work through the real keymap, not just the
/// core engine's unit tests.
#[gpui::test]
fn ctrl_z_and_ctrl_r_undo_redo(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("start".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("i");
    cx.simulate_input("abc");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        editor.read_with(cx, |editor, _cx| editor.text()).unwrap(),
        "abcstart"
    );

    // `ctrl-z` (dashes) is what the platform actually delivers: key "z" with
    // the control modifier. `ctrl+z` would parse as one literal key and
    // match a `ctrl+z` binding, hiding the mistake.
    cx.simulate_keystrokes("ctrl-z");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "start", "ctrl-z should undo");

    cx.simulate_keystrokes("ctrl-r");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "abcstart", "ctrl-r should redo");

    // ctrl+shift+z and ctrl+y are redo aliases too.
    cx.simulate_keystrokes("ctrl-z");
    cx.simulate_keystrokes("ctrl-shift-z");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "abcstart", "ctrl-shift-z should redo");

    cx.simulate_keystrokes("ctrl-z ctrl-y");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "abcstart", "ctrl-y should redo");
}

/// `f`/`t` and space must work through the real keymap: space is both vim's
/// `l` alias and the argument of `f `.
#[gpui::test]
fn normal_mode_find_motions(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("one two three four\nsecond line".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("0 f r");
    let pos = editor
        .read_with(cx, |editor, _cx| editor.cursor_position())
        .unwrap();
    assert_eq!(pos, (0, 10), "`fr` should land on the r of three");

    cx.simulate_keystrokes("0 f space");
    let pos = editor
        .read_with(cx, |editor, _cx| editor.cursor_position())
        .unwrap();
    assert_eq!(pos, (0, 3), "`f ` should land on the first space");

    cx.simulate_keystrokes("0 2 space");
    let pos = editor
        .read_with(cx, |editor, _cx| editor.cursor_position())
        .unwrap();
    assert_eq!(pos, (0, 2), "space should move right like `l`");

    cx.simulate_keystrokes("0 d f r");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "ee four\nsecond line", "`dfr` deletes through the r");
}

#[gpui::test]
fn normal_mode_counts_and_edits(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("one two three four\nsecond line".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("d 2 w");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(
        text, "three four\nsecond line",
        "`d2w` should delete two words"
    );

    // `u` undoes it again.
    cx.simulate_keystrokes("u");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "one two three four\nsecond line", "`u` should undo");

    // `$` lands on the last character, and `2j` moves two display lines
    // down (clamped at the last line here).
    cx.simulate_keystrokes("0 $");
    let pos = editor
        .read_with(cx, |editor, _cx| editor.cursor_position())
        .unwrap();
    assert_eq!(pos, (0, 17), "`$` should land on the last char");

    cx.simulate_keystrokes("^");
    let pos = editor
        .read_with(cx, |editor, _cx| editor.cursor_position())
        .unwrap();
    assert_eq!(pos, (0, 0), "`^` should land on the first non-blank");

    // `cw` replaces a word and enters insert mode.
    cx.simulate_keystrokes("escape 0 c w");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(
        text, "two three four\nsecond line",
        "`cw` should delete the word"
    );
    cx.simulate_input("X");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "Xtwo three four\nsecond line");
}

/// Text objects through the real keymap: `i`/`a` are bound actions and the
/// scope key arrives as the following keystroke.
#[gpui::test]
fn normal_mode_text_objects(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("one two three\n\nnext para".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("0 d i w");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(
        text, " two three\n\nnext para",
        "`diw` should delete just the word"
    );

    // A fresh buffer, so each form is checked from the same starting point.
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_text("one two three\n\nnext para".to_string(), cx);
        })
        .unwrap();
    cx.simulate_keystrokes("escape 0 d a w");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(
        text, "two three\n\nnext para",
        "`daw` should delete the trailing space too"
    );

    cx.simulate_keystrokes("escape 0 c i p");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(
        text, "\n\nnext para",
        "`cip` should delete the whole paragraph"
    );
    cx.simulate_input("X");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "X\n\nnext para", "`cip` should enter insert mode");
}

/// The markdown text objects through the real keymap and the real input path.
/// `L` is a bound key (shift-l), but `#`, `*` and the backtick are not: they
/// reach the engine as typed input, the same route `f#` takes. That is the part
/// an engine-only test cannot catch — an object that works in `core` and never
/// reaches a window.
#[gpui::test]
fn markdown_text_objects(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    const NOTE: &str = "# Top\nintro with [[Two]] here\n\n> [!warning] Watch out\n> first\n\n```\nlet x = 1;\n```\n\n# Other\ntail\n";
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(NOTE.to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    // Each form starts from the same note. The focus is set once, just above: the
    // edits below never move the keyboard, so it stays where it was put.
    let reset = |editor: &WindowHandle<MarkdownEditor>, cx: &mut VisualTestContext| {
        editor
            .update(cx, |editor, _window, cx| {
                editor.set_text(NOTE.to_string(), cx)
            })
            .unwrap();
    };
    let text =
        |cx: &mut VisualTestContext| editor.read_with(cx, |editor, _cx| editor.text()).unwrap();

    // `diL` on the link: `f[` is the literal route, `L` the bound one.
    reset(&editor, cx);
    cx.simulate_keystrokes("j f [");
    assert_eq!(
        editor
            .read_with(cx, |editor, _cx| editor.cursor_position())
            .unwrap(),
        (1, 11),
        "`f[` should land on the link's first bracket"
    );
    cx.simulate_keystrokes("d i L");
    assert_eq!(
        text(cx).lines().nth(1),
        Some("intro with [[]] here"),
        "`diL` should take the name out of the link"
    );

    // `da#` from the heading: the whole section, up to the next one.
    reset(&editor, cx);
    cx.simulate_keystrokes("0 d a #");
    assert_eq!(
        text(cx),
        "# Other\ntail\n",
        "`da#` should take the heading and everything it opens"
    );

    // `da*` from a body line of the callout.
    reset(&editor, cx);
    cx.simulate_keystrokes("0 j j j d a *");
    assert_eq!(
        text(cx),
        "# Top\nintro with [[Two]] here\n\n\n```\nlet x = 1;\n```\n\n# Other\ntail\n",
        "`da*` should remove the box, header and markers included"
    );

    // `` da` `` from inside the fenced block. The backtick is not in the keymap,
    // so it arrives as typed text — which is the route being checked. Six `j`s,
    // not seven: live preview hides the fence lines while the cursor is outside
    // the block, and `j` counts the lines the user can see.
    reset(&editor, cx);
    cx.simulate_keystrokes("0 6 j d a");
    cx.simulate_input("`");
    assert_eq!(
        text(cx),
        "# Top\nintro with [[Two]] here\n\n> [!warning] Watch out\n> first\n\n\n# Other\ntail\n",
        "`` da` `` should take the fences with the code"
    );
}

/// A table cell object through the keymap: `t` is a bound key (the find motion),
/// so the same letter has to read as the object's scope after `i` or `a`.
#[gpui::test]
fn table_cell_text_objects(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("| a | b |\n| --- | --- |\n| 1 | 2 |\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    let row = |cx: &mut VisualTestContext| {
        editor
            .read_with(cx, |editor, _cx| editor.text())
            .unwrap()
            .lines()
            .nth(2)
            .unwrap_or_default()
            .to_string()
    };

    // `j` inside a table lands on the cell, and the delimiter row is hidden while
    // the cursor is on the header, so two `j`s are enough to reach the body row.
    cx.simulate_keystrokes("j j c i t");
    assert_eq!(row(cx), "|  | 2 |", "`cit` should empty the cell");
    cx.simulate_input("X");
    assert_eq!(
        row(cx),
        "| X | 2 |",
        "and leave the cursor ready to type in it"
    );

    editor
        .update(cx, |editor, _window, cx| {
            editor.set_text("| a | b |\n| --- | --- |\n| 1 | 2 |\n".to_string(), cx)
        })
        .unwrap();
    cx.simulate_keystrokes("escape j j d a t");
    assert_eq!(
        row(cx),
        "| 2 |",
        "`dat` should take the cell and the pipe that closes it"
    );
}

/// Visual mode through the real keymap: the `vim-visual` context has to
/// carry the motion bindings, and operators act on the selection.
#[gpui::test]
fn visual_mode_selection_and_operators(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("one two three\nsecond para\nthird".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("v l l");
    let mode = editor.read_with(cx, |editor, _cx| editor.mode()).unwrap();
    assert_eq!(
        mode,
        shale_core::vim::Mode::Visual,
        "`v` should enter visual mode"
    );

    cx.simulate_keystrokes("d");
    let (mode, text) = editor
        .read_with(cx, |editor, _cx| (editor.mode(), editor.text()))
        .unwrap();
    assert_eq!(
        text, " two three\nsecond para\nthird",
        "`d` should delete the selection"
    );
    assert_eq!(mode, shale_core::vim::Mode::Normal, "and leave visual mode");

    // Linewise visual: `V` + `j` + `d` removes two whole lines.
    cx.simulate_keystrokes("escape shift-v j d");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "third", "`Vjd` should delete two lines");

    // `y` + `p` in visual swaps the selection for the register.
    cx.simulate_keystrokes("escape 0 v l l y");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "third", "yanking leaves the text alone");
    cx.simulate_keystrokes("$ h v l p");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "thithi", "`p` should replace the selection");
}

/// `.` replays through the real keymap, including a typed insert session.
#[gpui::test]
fn dot_repeats_the_last_change(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("one two three\nsecond para".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("0 d w");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "two three\nsecond para");
    cx.simulate_keystrokes(".");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "three\nsecond para", "`.` should repeat the delete");

    cx.simulate_keystrokes("i");
    cx.simulate_input("XY");
    cx.simulate_keystrokes("escape");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "XYthree\nsecond para");
    cx.simulate_keystrokes(".");
    let (mode, text) = editor
        .read_with(cx, |editor, _cx| (editor.mode(), editor.text()))
        .unwrap();
    assert_eq!(text, "XYXYthree\nsecond para", "`.` repeats typed text");
    assert_eq!(
        mode,
        shale_core::vim::Mode::Normal,
        "and stays in normal mode"
    );
}
