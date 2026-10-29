//! The panel the checker puts over the note: vim's `K`, a resting pointer, one
//! popup at a time, and closing properly.
//!
//! The notes are put into the window's hands directly rather than through a
//! server: the panel's behaviour — when it opens, that only one is ever up, how
//! it gets out of the way — must not depend on a `harper-ls` process being
//! installed, and the protocol itself is covered by `crate::lsp`'s tests.

use super::*;
use crate::lsp::{Diagnostic, Fix};

/// Put the checker's notes in the window's hands, as though the server had just
/// published them.
fn notes_from(notes: &[(&str, usize, usize)]) -> Vec<Diagnostic> {
    notes
        .iter()
        .map(|(message, start, end)| Diagnostic {
            start: *start,
            end: *end,
            message: message.to_string(),
            code: "SpellCheck".into(),
        })
        .collect()
}

/// Two frames, so the note's lines have painted positions to measure against.
fn paint(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// Rest the pointer at a window position, as gpui delivers a move: no button
/// held, no modifiers down.
fn hover_point(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
}

/// Rest the pointer on buffer offset `offset` of display line `line`, as gpui
/// delivers a move: no button held, no modifiers down.
fn hover_at(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    line: usize,
    offset: usize,
) {
    let at = pointer_over(cx, window, line, offset);
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
}

/// A note open, drawn, and marked by the checker at `(start, end)` with a message.
fn marked_note(
    cx: &mut TestAppContext,
    text: &str,
    notes: &[(&str, usize, usize)],
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let (dir, window) = vault_with(cx, &[("a.md", text)]);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx);
            ws.set_checker_notes(notes_from(notes), cx);
        })
        .unwrap();
    (dir, window)
}

/// Whether a panel is up.
fn shown(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> bool {
    window
        .update(cx, |ws, _window, _cx| ws.has_popup())
        .unwrap()
}

/// What the panel says, if one is up.
fn saying(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Option<String> {
    window
        .update(cx, |ws, _window, cx| ws.popup_message(cx))
        .unwrap()
}

/// What the panel offers, in order.
fn offers(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.popup_rows(cx))
        .unwrap()
}

/// The note's text, as the editor holds it.
fn text(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> String {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().read_with(cx, |editor, _cx| editor.text())
        })
        .unwrap()
}

/// Where display line `line` was painted: its left edge, its top and its bottom,
/// in window coordinates.
fn line_box(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    line: usize,
) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels) {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().read_with(cx, |editor, _cx| {
                let (left, _right) = editor.hit_line_x(line).unwrap();
                let (top, bottom) = editor.hit_line_edges(line).unwrap();
                (left, top, bottom)
            })
        })
        .unwrap()
}

/// The window x of buffer offset `offset` on display line `line`.
fn x_over(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    line: usize,
    offset: usize,
) -> gpui::Pixels {
    let (left, _top, _bottom) = line_box(cx, window, line);
    left + window
        .update(cx, |ws, _window, cx| {
            ws.editor()
                .read_with(cx, |editor, _cx| editor.x_for_offset(line, offset))
        })
        .unwrap()
        .unwrap()
}

/// Where the pointer has to be to rest on buffer offset `offset` of display line
/// `line`: the line's left edge plus where that offset was painted on it.
fn pointer_over(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
    line: usize,
    offset: usize,
) -> gpui::Point<gpui::Pixels> {
    let (_left, top, bottom) = line_box(cx, window, line);
    gpui::point(x_over(cx, window, line, offset), (top + bottom) / 2.)
}

/// Where the panel was drawn: the y of its bottom edge, in window coordinates.
fn panel_bottom(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> gpui::Pixels {
    window
        .update(cx, |ws, _window, cx| {
            ws.popup_place(cx).expect("the panel has a place").bottom()
        })
        .unwrap()
}

/// vim's `K` on a word the checker marked: the complaint appears over the note,
/// in the checker's words.
#[gpui::test]
fn k_over_a_marked_word_shows_the_checkers_note(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("^");
    cx.simulate_keystrokes("shift-k");
    assert!(shown(cx, &window), "the panel is up");
    assert_eq!(
        saying(cx, &window).as_deref(),
        Some("Did you mean to spell `thier` this way?"),
        "in the checker's own words"
    );
}

/// `K` where nothing is marked says nothing, rather than opening an empty panel.
#[gpui::test]
fn k_on_a_plain_word_shows_nothing(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The cursor on `spelling`, which the checker left alone.
    cx.simulate_keystrokes("w w");
    cx.simulate_keystrokes("shift-k");
    assert!(!shown(cx, &window));
}

/// Only one panel is ever up: asking about a second word replaces the first
/// rather than laying a second panel over the note.
#[gpui::test]
fn asking_about_another_word_replaces_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[
            ("Did you mean to spell `thier` this way?", 0, 5),
            ("Did you mean to spell `wrogn` this way?", 17, 22),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("^ shift-k");
    assert!(shown(cx, &window));
    // The panel has the keyboard, so the cursor moves after it steps aside.
    // `wrogn` starts at byte 17, which `3w` from the line's start reaches.
    cx.simulate_keystrokes("escape 0 3 w shift-k");
    assert!(shown(cx, &window), "still exactly one panel");
    assert_eq!(
        saying(cx, &window).as_deref(),
        Some("Did you mean to spell `wrogn` this way?"),
        "and it is about the word under the cursor now"
    );
}

/// `escape` closes the panel *and* gives the keyboard back: the next `x` deletes
/// a character of the note, which is only true if the note has the focus again.
#[gpui::test]
fn escape_closes_the_panel_and_returns_the_keyboard(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("^ shift-k");
    assert!(shown(cx, &window));
    cx.simulate_keystrokes("escape");
    assert!(!shown(cx, &window), "the panel is gone");

    cx.simulate_keystrokes("x");
    assert_eq!(
        text(cx, &window),
        "hier spelling is wrogn.\n",
        "and the note has the keyboard back"
    );
}

/// A picker and a panel are never up together. The picker is modal in a way the
/// panel is not, so it closes the panel on its way in.
#[gpui::test]
fn opening_a_picker_closes_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("^ shift-k");
    assert!(shown(cx, &window));
    cx.simulate_keystrokes("ctrl-o");
    assert!(!shown(cx, &window), "the panel made way for the picker");
    cx.simulate_keystrokes("escape");
    assert!(!shown(cx, &window), "and it does not come back");
}

/// Choosing an offer rewrites the note and closes the panel. One undo step, so
/// `u` puts the word back whole.
#[gpui::test]
fn choosing_an_offer_rewrites_the_note(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("^ shift-k");
    window
        .update(cx, |ws, _window, cx| {
            ws.offer_test_fixes(
                vec![
                    Fix {
                        title: "Replace with: “Their”".into(),
                        edits: vec![(0, 5, "Their".into())],
                        command: None,
                    },
                    Fix {
                        title: "Ignore this error".into(),
                        edits: Vec::new(),
                        command: None,
                    },
                ],
                cx,
            )
        })
        .unwrap();
    assert_eq!(
        offers(cx, &window),
        vec!["Replace with: “Their”", "Ignore this error"],
        "the offers are the checker's own titles"
    );

    cx.simulate_keystrokes("1");
    assert_eq!(
        text(cx, &window),
        "Their spelling is wrogn.\n",
        "the first offer was applied"
    );
    assert!(!shown(cx, &window), "and the panel is gone");

    cx.simulate_keystrokes("u");
    assert_eq!(
        text(cx, &window),
        "Thier spelling is wrogn.\n",
        "the whole fix is one undo step"
    );
}

/// A row with nothing to rewrite is still a thing the checker can do; choosing it
/// closes the panel and leaves the text alone.
#[gpui::test]
fn an_offer_with_no_edit_still_closes_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("^ shift-k");
    window
        .update(cx, |ws, _window, cx| {
            ws.offer_test_fixes(
                vec![Fix {
                    title: "Ignore this error".into(),
                    edits: Vec::new(),
                    command: None,
                }],
                cx,
            )
        })
        .unwrap();
    cx.simulate_keystrokes("1");
    assert_eq!(text(cx, &window), "Thier spelling is wrogn.\n");
    assert!(!shown(cx, &window));
}

/// Resting the pointer on a marked word asks the same question as `K`.
#[gpui::test]
fn the_pointer_on_a_marked_word_shows_the_note(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    hover_at(cx, &window, 0, 2);
    assert!(shown(cx, &window), "the pointer is over `Thier`");
    assert_eq!(
        saying(cx, &window).as_deref(),
        Some("Did you mean to spell `thier` this way?")
    );
}

/// A panel the pointer opened belongs to the word it was opened over: moving off
/// that word closes it.
#[gpui::test]
fn moving_off_the_word_closes_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    hover_at(cx, &window, 0, 2);
    assert!(shown(cx, &window));
    // Byte 14 is the `i` of `is`, which the checker left alone.
    hover_at(cx, &window, 0, 14);
    assert!(
        !shown(cx, &window),
        "the pointer left the word, so the panel left"
    );
}

/// The panel hangs *below* the word, so the pointer has to cross text the checker
/// never marked to reach one of its rows. A panel that closed on that trip could
/// never be clicked, which is the whole point of having rows.
#[gpui::test]
fn the_pointer_can_travel_from_the_word_onto_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    hover_at(cx, &window, 0, 2);
    assert!(shown(cx, &window));

    // Straight down from the word, through the strip between the line and the
    // panel — which belongs to the line below, and is not marked.
    let (_left, _top, bottom) = line_box(cx, &window, 0);
    let x = x_over(cx, &window, 0, 2);
    hover_point(cx, gpui::point(x, bottom + gpui::px(2.)));
    assert!(
        shown(cx, &window),
        "the way down to a row is not a way away from it"
    );

    // And on into the rows themselves.
    hover_point(cx, gpui::point(x, bottom + gpui::px(20.)));
    assert!(shown(cx, &window), "the panel is still there to be clicked");
}

/// A word near the right end of a long line sits above the *space beside* the
/// panel rather than above its text. Walking straight down from it is still the
/// way to a row, so the panel has to survive the walk.
#[gpui::test]
fn walking_down_from_a_word_beside_the_panel_keeps_it(cx: &mut TestAppContext) {
    let prefix = "padding words to push the marked word past the panel's width";
    let text = format!("{prefix} thier is misspelled\n");
    let start = prefix.len() + 1;
    let (_dir, window) = marked_note(
        cx,
        &text,
        &[("Did you mean to spell `thier` this way?", start, start + 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    hover_at(cx, &window, 0, start + 2);
    assert!(shown(cx, &window));
    let (_left, _top, bottom) = line_box(cx, &window, 0);
    let x = x_over(cx, &window, 0, start + 2);
    hover_point(cx, gpui::point(x, bottom + gpui::px(2.)));
    assert!(
        shown(cx, &window),
        "straight down from a word beside the panel still reaches its rows"
    );
}

/// Two marked words on consecutive lines. The way down from the upper one to a
/// row of its panel crosses the lower word, and reopening the panel for that word
/// moves the panel out from under the row the pointer was reaching for — which is
/// what the user saw: the top warning closing just as the mouse got to its fixes.
#[gpui::test]
fn a_marked_word_under_the_panel_does_not_move_it(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\nAnd anotehr mistake here.\n",
        &[
            ("Did you mean to spell `thier` this way?", 0, 5),
            ("Did you mean to spell `anotehr` this way?", 29, 36),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    hover_at(cx, &window, 0, 2);
    assert_eq!(
        saying(cx, &window).as_deref(),
        Some("Did you mean to spell `thier` this way?")
    );

    // Straight down from the word, through the strip between the line and its
    // panel — which belongs to the line below, and is marked there.
    let (_left, _top, bottom) = line_box(cx, &window, 0);
    let x = x_over(cx, &window, 1, 32);
    hover_point(cx, gpui::point(x, bottom + gpui::px(2.)));
    assert_eq!(
        saying(cx, &window).as_deref(),
        Some("Did you mean to spell `thier` this way?"),
        "the panel the pointer is on the way to is still the one that is up"
    );

    // Walking away below it still ends it: the guard covers the panel's own
    // height, not the space under it.
    let bottom = panel_bottom(cx, &window);
    hover_point(cx, gpui::point(x, bottom + gpui::px(60.)));
    assert!(!shown(cx, &window), "walking away below the panel ends it");
}

/// A panel the pointer opened has to end when the pointer walks away below it,
/// not just when it walks away above: the panel swallows the moves over its own
/// rows, and the space under it belongs to no line, so nothing else would ever
/// tell it to go.
#[gpui::test]
fn the_panel_ends_when_the_pointer_walks_off_its_bottom(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    hover_at(cx, &window, 0, 2);
    assert!(shown(cx, &window));
    let bottom = panel_bottom(cx, &window);
    let x = x_over(cx, &window, 0, 2);

    // A little past the edge is still the way to a row: the panel's own border and
    // the gap below it belong to the trip, not to leaving.
    hover_point(cx, gpui::point(x, bottom + gpui::px(4.)));
    assert!(
        shown(cx, &window),
        "the leeway below the panel is still its own"
    );

    // Well past it, the pointer has left, and nothing below the panel will ever
    // report another move: the panel swallows the ones over its rows, and the
    // space under it belongs to no line.
    hover_point(cx, gpui::point(x, bottom + gpui::px(60.)));
    assert!(!shown(cx, &window), "walking away below the panel ends it");
}

/// The same walk away from a panel the keyboard asked for: `K` is the user
/// asking, and moving the mouse is not their answer to it, wherever it goes.
#[gpui::test]
fn a_panel_asked_for_with_k_survives_the_pointer_leaving_below_it(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes("^ shift-k");
    let bottom = panel_bottom(cx, &window);
    let x = x_over(cx, &window, 0, 2);
    hover_point(cx, gpui::point(x, bottom + gpui::px(60.)));
    assert!(
        shown(cx, &window),
        "the keyboard asked, so the pointer does not answer"
    );
}

/// Typing is not hovering. In insert mode the pointer is next to the text the
/// user is writing, and a panel that took the keyboard then would be worse than
/// saying nothing.
#[gpui::test]
fn hovering_while_typing_says_nothing(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes("i");
    hover_at(cx, &window, 0, 2);
    assert!(!shown(cx, &window), "insert mode keeps its keyboard");
    cx.simulate_keystrokes("escape");

    // The same pointer position in normal mode does open one, so the test above
    // is about the mode and not about where the pointer was.
    hover_at(cx, &window, 0, 2);
    assert!(shown(cx, &window));
}

/// A panel asked for with `K` is the user asking. Moving the mouse past it is not
/// their answer, so it stays until `escape` or a row says otherwise.
#[gpui::test]
fn a_panel_asked_for_with_k_outlives_the_pointer(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes("^ shift-k");
    assert!(shown(cx, &window));
    hover_at(cx, &window, 0, 14);
    assert!(
        shown(cx, &window),
        "the keyboard asked, so the pointer does not answer"
    );
}

/// The checker taking its note back — the word was fixed by hand, or the server
/// changed its mind — takes the panel with it.
#[gpui::test]
fn a_note_withdrawn_takes_the_panel_with_it(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("^ shift-k");
    assert!(shown(cx, &window));
    window
        .update(cx, |ws, _window, cx| ws.set_checker_notes(Vec::new(), cx))
        .unwrap();
    paint(cx);
    assert!(!shown(cx, &window), "nothing to say, nothing to show");
}

/// Opening another note ends the panel: it was about a word in the note that is
/// gone, and a panel left over a different note is a panel about nothing.
#[gpui::test]
fn opening_another_note_ends_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    std::fs::write(_dir.path().join("b.md"), "Clean text.\n").unwrap();
    window
        .update(cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();

    cx.simulate_keystrokes("^ shift-k");
    assert!(shown(cx, &window));
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("b.md"), cx)
        })
        .unwrap();
    assert!(!shown(cx, &window));

    // The keyboard went back to the note even though that close had no window to
    // do it with, so the next keystroke is an edit.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("x");
    assert_eq!(text(cx, &window), "lean text.\n");
}

/// The panel is drawn where its line is, so it follows the note when the note
/// scrolls rather than floating where the word used to be.
#[gpui::test]
fn the_panel_follows_its_line(cx: &mut TestAppContext) {
    let long: String = (1..=120)
        .map(|n| format!("line {n} of the note\n"))
        .collect();
    let (_dir, window) = marked_note(
        cx,
        &long,
        &[("Did you mean to spell `wrogn` this way?", 0, 4)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("^ shift-k");
    let before = window
        .update(cx, |ws, _window, cx| ws.popup_place(cx))
        .unwrap();
    assert!(before.is_some(), "the panel has a place");

    // Scroll the note so the marked line is off the top, and the panel with it.
    // The panel has the keyboard by then, so the scroll is asked for through the
    // editor rather than the keys.
    window
        .update(cx, |ws, _window, cx| {
            ws.editor()
                .update(cx, |editor, cx| editor.reveal_line(119, cx))
        })
        .unwrap();
    paint(cx);
    let after = window
        .update(cx, |ws, _window, cx| ws.popup_place(cx))
        .unwrap();
    assert_eq!(
        after, None,
        "a line that is not on screen has no panel over it"
    );
}
