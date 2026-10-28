//! Whole-row behaviour through the keymap: heading-section folding, and the
//! list, task and quote markers Enter carries onto the next row.

use super::*;

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

/// Enter in insert mode carries a list, task, numbered or quote marker onto the
/// new row (Obsidian's behaviour), and the marker text is ordinary insert text:
/// one undo step takes the newline and the marker together.
#[gpui::test]
fn enter_continues_list_task_and_quote_markers(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(String::new(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("i");
    cx.simulate_input("- one");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("two");
    cx.simulate_keystrokes("escape");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "- one\n- two", "the bullet is carried over");

    // The whole insert session is one undo step, as in vim, and a session that
    // only presses Enter gives up the newline and the marker together.
    cx.simulate_keystrokes("u");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "", "the session is one undo step");

    editor
        .update(cx, |editor, _window, cx| {
            editor.set_text("- one".to_string(), cx);
        })
        .unwrap();
    cx.simulate_keystrokes("A enter escape");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "- one\n- ");
    cx.simulate_keystrokes("u");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "- one", "the marker is not its own undo step");

    // A task starts empty whatever the row being continued held, and a numbered
    // item counts up.
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_text("- [x] done".to_string(), cx);
        })
        .unwrap();
    cx.simulate_keystrokes("A enter escape");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "- [x] done\n- [ ] ", "the new task is unchecked");

    editor
        .update(cx, |editor, _window, cx| {
            editor.set_text("3. third".to_string(), cx);
        })
        .unwrap();
    cx.simulate_keystrokes("A enter escape");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "3. third\n4. ");

    // A quote keeps its markers, and the indent of a nested item is kept.
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_text("  - nested".to_string(), cx);
        })
        .unwrap();
    cx.simulate_keystrokes("A enter escape");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "  - nested\n  - ", "the indent is preserved");
}

/// Enter on a row that holds nothing but its marker removes the marker instead of
/// starting a second empty item, and inside a fenced block markers are literal
/// text.
#[gpui::test]
fn enter_on_a_marker_only_row_ends_the_list(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text("- one\n- ".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    // `A` on the empty item: the marker goes away, leaving a blank row.
    cx.simulate_keystrokes("G A enter");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "- one\n", "the empty marker is removed, not doubled");

    // The next Enter is an ordinary newline, which is how the list is left.
    cx.simulate_keystrokes("enter escape");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "- one\n\n");

    // Inside a fence, `-` is literal text: Enter just breaks the line.
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_text("```\n- one\n```".to_string(), cx);
        })
        .unwrap();
    cx.simulate_keystrokes("j A enter escape");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "```\n- one\n\n```");
}
