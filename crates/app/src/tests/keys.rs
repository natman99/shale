//! Tests for the keys: the keymap's real keystroke shapes, the vim command
//! round-trip, and the edits that go through them.

use super::*;

/// Typing printable characters must reach the buffer through gpui's input
/// handler, not just the keys we bind to actions.
#[gpui::test]
fn typing_in_insert_mode_inserts_text(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("i");
    cx.simulate_input("hello");

    let (mode, text) = editor
        .read_with(cx, |editor, _cx| (editor.mode(), editor.text()))
        .unwrap();
    assert_eq!(
        mode,
        shale_core::vim::Mode::Insert,
        "`i` should enter insert"
    );
    assert_eq!(text, "hello", "typed characters should land in the buffer");

    // Normal mode swallows typed characters instead of inserting them.
    cx.simulate_keystrokes("escape");
    cx.simulate_input("xyz");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "hello", "normal mode should swallow typed input");
}

/// The Linux platform delivers ctrl+z as `key = "z"` with the control
/// modifier and *no* `key_char` (control characters are filtered out of
/// `key_char` by `keystroke_from_xkb`). Check that our keymap really matches
/// that shape, independent of any window or focus state. This is the test
/// that catches modifier syntax mistakes: `simulate_keystrokes("ctrl+z")`
/// parses through the same parser as the binding, so a typo like `+`
/// matches itself and looks fine.
#[test]
fn keymap_matches_the_linux_keystroke_shape() {
    let keymap = gpui::Keymap::new(keybindings());
    let contexts = vec![gpui::KeyContext::try_from("vim-normal").unwrap()];
    for (control, key, action) in [
        (true, "z", "shale_editor::Undo"),
        (true, "r", "shale_editor::Redo"),
        (true, "y", "shale_editor::Redo"),
        (true, "s", "shale::Save"),
        (false, "k", "shale_editor::VimK"),
        (false, "$", "shale_editor::VimDollar"),
        (false, ".", "shale_editor::VimDot"),
        (false, "^", "shale_editor::VimCaret"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                control,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "typed {key} (control {control}) should resolve to {action}"
        );
    }

    // The visual context has to carry the same bindings, or the keys stop
    // working the moment `v` is typed.
    let contexts = vec![gpui::KeyContext::try_from("vim-visual").unwrap()];
    for (control, shift, key, action) in [
        (false, false, "k", "shale_editor::VimK"),
        (false, false, "v", "shale_editor::VimV"),
        (false, true, "v", "shale_editor::VimUpperV"),
        (false, true, "h", "shale_editor::VimUpperH"),
        (false, true, "l", "shale_editor::VimUpperL"),
        (false, false, "$", "shale_editor::VimDollar"),
        (true, false, "z", "shale_editor::Undo"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                control,
                shift,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "visual mode: typed {key} should resolve to {action}"
        );
    }

    // The normal-mode-only table edits. A shifted letter arrives as the
    // lowercase key with the shift modifier; a shifted symbol arrives as the
    // symbol itself with no shift, so `|` is bound literally.
    let contexts = vec![gpui::KeyContext::try_from("vim-normal").unwrap()];
    for (shift, key, action) in [
        (true, "d", "shale_editor::VimUpperD"),
        (false, "|", "shale_editor::VimPipe"),
        (false, "\\", "shale_editor::VimBackslash"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                shift,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "normal mode: typed {key} should resolve to {action}"
        );
    }

    // alt+hjkl: unlike shift, alt leaves the letter's keysym alone, so the
    // platform delivers the plain letter with the alt modifier set.
    for (key, action) in [
        ("h", "shale_editor::VimAltH"),
        ("j", "shale_editor::VimAltJ"),
        ("k", "shale_editor::VimAltK"),
        ("l", "shale_editor::VimAltL"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                alt: true,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "alt-{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "alt-{key} should resolve to {action}"
        );
    }
}

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

/// Folding is display state: `za` takes a section's rows out of the display
/// model, `j` steps over them, `zM`/`zR` do it to every section, and the note's
/// text never changes.
#[gpui::test]
fn folding_a_section_steps_over_its_rows(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("# Top\none\ntwo\n# Other\nlast".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let lines = editor
        .read_with(cx, |editor, _cx| editor.display_line_count())
        .unwrap();
    assert_eq!(lines, 5, "every row is drawn before anything is folded");

    cx.simulate_keystrokes("z a");
    let (lines, painted, text) = editor
        .read_with(cx, |editor, _cx| {
            (
                editor.display_line_count(),
                editor.painted_text(),
                editor.text(),
            )
        })
        .unwrap();
    assert_eq!(lines, 3, "`one` and `two` are folded away");
    assert_eq!(
        painted[1], "Other",
        "the next heading moves up (and live preview hides its `#`)"
    );
    assert_eq!(
        text, "# Top\none\ntwo\n# Other\nlast",
        "a fold never touches the text"
    );

    // `j` from the heading goes to the next visible row, not into the fold.
    cx.simulate_keystrokes("j");
    let pos = editor
        .read_with(cx, |editor, _cx| editor.cursor_position())
        .unwrap();
    assert_eq!(pos, (3, 0), "j lands on `# Other`, past the hidden rows");

    // `k` comes back to the heading, and `za` opens the section again.
    cx.simulate_keystrokes("k z a");
    let lines = editor
        .read_with(cx, |editor, _cx| editor.display_line_count())
        .unwrap();
    assert_eq!(lines, 5, "za again opens the section");

    // `zM` closes every section, `zR` opens them all again.
    cx.simulate_keystrokes("g g z M");
    let lines = editor
        .read_with(cx, |editor, _cx| editor.display_line_count())
        .unwrap();
    assert_eq!(lines, 2, "both headings keep their row, their bodies go");
    cx.simulate_keystrokes("z R");
    let lines = editor
        .read_with(cx, |editor, _cx| editor.display_line_count())
        .unwrap();
    assert_eq!(lines, 5);
}

/// A fold command on a row with no section around it does nothing, and a bare
/// `z` waits for its command letter without touching the buffer.
#[gpui::test]
fn a_fold_command_needs_a_section_under_the_cursor(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("no headings\nhere".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    cx.simulate_keystrokes("z");
    let hint = editor
        .read_with(cx, |editor, _cx| editor.pending_hint())
        .unwrap();
    assert_eq!(hint, " z", "the status bar shows the half-typed command");
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("z a");
    let (lines, painted) = editor
        .read_with(cx, |editor, _cx| {
            (editor.display_line_count(), editor.painted_fold_markers())
        })
        .unwrap();
    assert_eq!(lines, 2);
    assert!(painted.is_empty(), "nothing is folded");
}
