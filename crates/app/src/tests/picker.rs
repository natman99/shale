//! Quick-open, driven through the same keymap and input paths the live app uses:
//! `ctrl-o` opens the list, typed characters arrive through the picker's input
//! handler and re-rank it, and `enter` opens the highlighted row.

use super::*;

/// What the picker would choose right now, as the note path behind that row.
fn choice(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |ws, _window, cx| ws.picker_choice(cx))
        .unwrap()
}

/// How many rows the picker is offering, or `None` while none is open.
fn matches(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<usize> {
    window
        .update(cx, |ws, _window, cx| ws.picker_matches(cx))
        .unwrap()
}

#[gpui::test]
fn ctrl_o_lists_every_note_and_enter_opens_the_top_row(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        matches(&window, cx),
        Some(2),
        "an empty query is every note"
    );
    assert_eq!(choice(&window, cx).as_deref(), Some("Welcome.md"));

    cx.simulate_keystrokes("enter");
    assert_eq!(matches(&window, cx), None, "and the picker is gone");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "Welcome.md", "the highlighted row was opened");
}

#[gpui::test]
fn typing_narrows_the_list_to_the_best_match(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o");
    cx.simulate_input("anot");
    assert_eq!(matches(&window, cx), Some(1));
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("notes/Another Note.md"),
        "the query is matched against the whole path, not just the name"
    );

    cx.simulate_keystrokes("enter");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "notes/Another Note.md");
}

/// `j`/`k` belong to the picker while it is open: they move the highlighted row
/// rather than the vim cursor, and the note under the cursor does not move.
#[gpui::test]
fn j_and_k_move_the_highlight_not_the_cursor(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o j");
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("notes/Another Note.md")
    );
    cx.simulate_keystrokes("k");
    assert_eq!(choice(&window, cx).as_deref(), Some("Welcome.md"));
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "",
        "the note was never touched"
    );
}

/// Backspace widens the list again, which is how a mistyped query is fixed.
#[gpui::test]
fn backspace_widens_the_list_again(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o");
    cx.simulate_input("zzz");
    assert_eq!(matches(&window, cx), Some(0), "nothing matches");
    assert_eq!(choice(&window, cx), None, "so there is nothing to choose");

    cx.simulate_keystrokes("backspace backspace backspace");
    assert_eq!(matches(&window, cx), Some(2));
}

/// Escape closes the picker and gives the keyboard back to the note, so the next
/// keystroke is a vim motion and not a query character.
#[gpui::test]
fn escape_closes_the_picker_and_gives_the_keyboard_back(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o escape");
    assert_eq!(matches(&window, cx), None);
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.mode_label(cx))
            .unwrap(),
        "NORMAL"
    );
    cx.simulate_keystrokes("i");
    cx.simulate_input("typed");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "typed"
    );
}
