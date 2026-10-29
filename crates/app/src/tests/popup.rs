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
        .update(cx, |ws, window, cx| {
            ws.popup_place(window, cx)
                .expect("the panel has a place")
                .bounds
                .bottom()
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

/// Whether the note is what holds the keyboard.
fn note_has_keys(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> bool {
    window
        .update(cx, |ws, window, cx| {
            ws.editor().read(cx).focus_handle.is_focused(window)
        })
        .unwrap()
}

/// A click in the note, outside the panel, ends the panel.
///
/// The click takes the keyboard off the panel whether the window agrees or not,
/// because gpui focuses whatever the pointer clicked. A panel left up and
/// unfocused is the worst of both: `escape` goes to vim rather than closing it,
/// and a hover will not replace a panel that `K` asked for, so no new panel can
/// open either. Closing on the click is what the picker and the command line
/// already do, and a click on one of the panel's rows never reaches here.
#[gpui::test]
fn a_click_in_the_note_ends_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes("^ shift-k");
    assert!(shown(cx, &window), "the panel is up");
    assert!(!note_has_keys(cx, &window), "and it holds the keyboard");
    let (_below, _top, bottom) = place(cx, &window);

    // Clear of the panel both ways, and on the note.
    cx.simulate_click(
        gpui::point(gpui::px(1200.), bottom + gpui::px(80.)),
        gpui::Modifiers::none(),
    );
    assert!(!shown(cx, &window), "the panel made way for the click");
    assert!(
        note_has_keys(cx, &window),
        "and the keyboard came back with it"
    );

    // A hover can ask again, which is what the panel left up and unfocused
    // blocked: it was neither closable nor replaceable.
    hover_at(cx, &window, 0, 2);
    assert!(shown(cx, &window), "hover opens a panel again");
    assert_eq!(
        saying(cx, &window).as_deref(),
        Some("Did you mean to spell `thier` this way?"),
        "about the word under the pointer"
    );
}

/// A picker and a panel are never up together. The picker is modal in a way the
/// panel is not, so it closes the panel on its way in. `ctrl-shift-p` is the key
/// that gets there: the panel holds the keyboard, so the vim chords that live in
/// the note cannot be typed over it.
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
    cx.simulate_keystrokes("ctrl-shift-p");
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

/// How tall the panel is, as the window measures it — the height the pointer's
/// corridor is built from, and the height the render has to deliver.
fn panel_height(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> gpui::Pixels {
    window
        .update(cx, |ws, window, cx| {
            ws.popup_place(window, cx)
                .expect("the panel has a place")
                .bounds
                .size
                .height
        })
        .unwrap()
}

/// The note's text size, which is what sets the panel's row height.
fn text_size(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> f32 {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().read_with(cx, |editor, _cx| editor.font_size())
        })
        .unwrap()
}

/// Which offer rows the panel is drawing.
fn drawn_rows(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
) -> std::ops::Range<usize> {
    window
        .update(cx, |ws, _window, cx| ws.popup_drawn(cx))
        .unwrap()
}

/// Put `n` offers in front of the panel, the way a checker with opinions would.
fn give_offers(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>, n: usize) {
    window
        .update(cx, |ws, _window, cx| {
            ws.offer_test_fixes(
                (0..n)
                    .map(|i| Fix {
                        title: format!("Offer {}", i + 1),
                        edits: Vec::new(),
                        command: None,
                    })
                    .collect(),
                cx,
            )
        })
        .unwrap();
}

/// A panel is placed below the word it is about, so a list of offers that simply
/// grows runs off the bottom of the window and the last offers become unreachable
/// — which is what harper's eighth was. The panel keeps a height it can show.
#[gpui::test]
fn a_long_list_of_offers_does_not_grow_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("^ shift-k");

    let size = text_size(cx, &window);
    give_offers(cx, &window, 12);
    assert_eq!(
        offers(cx, &window).len(),
        12,
        "the panel holds every offer the checker gave"
    );
    assert_eq!(
        panel_height(cx, &window),
        crate::popup::panel_size(crate::popup::VISIBLE_ROWS, size).height,
        "but it is as tall as the rows it draws, not the rows it was offered"
    );
}

/// The panel scrolls to the offer the keyboard is on, so walking `j` down a long
/// list ends on a row that is actually on screen.
#[gpui::test]
fn the_panel_scrolls_to_follow_the_keyboard(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("^ shift-k");
    give_offers(cx, &window, 12);
    assert_eq!(
        drawn_rows(cx, &window),
        0..crate::popup::VISIBLE_ROWS,
        "the first offers, in the checker's order"
    );

    for _ in 0..11 {
        cx.simulate_keystrokes("j");
    }
    let drawn = drawn_rows(cx, &window);
    assert!(
        drawn.contains(&11),
        "the last offer is on the panel rather than below the window: {drawn:?}"
    );
    assert_eq!(
        drawn.len(),
        crate::popup::VISIBLE_ROWS,
        "and the panel did not shrink to do it"
    );
}

/// Scrolling the list must not move the panel. The pointer's corridor — how far
/// below the panel it may go before the panel is dismissed — is the panel's
/// height, so a panel that changed height as the list scrolled would put the
/// corridor somewhere the rows are not.
#[gpui::test]
fn scrolling_the_list_does_not_move_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("^ shift-k");
    give_offers(cx, &window, 12);

    let bottom = panel_bottom(cx, &window);
    cx.simulate_keystrokes("j j j j j j j j j");
    assert_eq!(
        panel_bottom(cx, &window),
        bottom,
        "the panel stayed where it was put"
    );
}

/// The number printed beside an offer is the key that chooses it, on the panel or
/// off it: a scrolled list that renumbered its rows would make `3` mean whatever
/// happened to be third from the top.
#[gpui::test]
fn a_digit_chooses_the_row_it_names(cx: &mut TestAppContext) {
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
                (0..12)
                    .map(|i| Fix {
                        title: format!("Offer {}", i + 1),
                        edits: if i == 2 {
                            vec![(0, 5, "Their".into())]
                        } else {
                            Vec::new()
                        },
                        command: None,
                    })
                    .collect(),
                cx,
            )
        })
        .unwrap();

    // Scroll the third offer off the top of the panel, then name it. Eleven `j`s
    // because the panel keeps the window minimal: it only slides as far as the
    // highlighted row needs.
    cx.simulate_keystrokes("j j j j j j j j j j j");
    assert!(
        !drawn_rows(cx, &window).contains(&2),
        "the third offer is off the panel now"
    );
    cx.simulate_keystrokes("3");
    assert_eq!(
        text(cx, &window),
        "Their spelling is wrogn.\n",
        "and `3` still means the third offer"
    );
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
        .update(cx, |ws, window, cx| ws.popup_place(window, cx))
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
        .update(cx, |ws, window, cx| ws.popup_place(window, cx))
        .unwrap();
    assert_eq!(
        after, None,
        "a line that is not on screen has no panel over it"
    );
}

/// The picker floats over the note, and the note under it does not answer the
/// pointer. Reported live: moving the mouse over a word the checker marked, with
/// the palette open above it, opened a harper panel *underneath* the list, so two
/// panels were up at once and neither would answer for the other.
#[gpui::test]
fn the_note_under_the_picker_does_not_answer_the_pointer(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes("space f");
    // The panel hangs at the top of the note column and the marked word is on the
    // first line, so this is the pointer resting on the word *through* the panel.
    hover_at(cx, &window, 0, 2);
    assert!(!shown(cx, &window), "no harper panel under the picker");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.picker_matches(cx))
            .unwrap(),
        Some(1),
        "and the list is still the list"
    );
}

/// A note long enough that its marked word sits against the bottom of the window.
///
/// Returns the offset of the marked word and its display line. The note ends with
/// a newline, so vim's `G` lands on the empty line *after* it; `G k` is what puts
/// the cursor on the marked line, which is the last one with text on it.
fn bottom_note(
    cx: &mut TestAppContext,
) -> (tempfile::TempDir, WindowHandle<Workspace>, usize, usize) {
    let mut text: String = (1..=119)
        .map(|n| format!("line {n} of the note\n"))
        .collect();
    text.push_str("and the wrogn word at the bottom\n");
    let start = text.find("wrogn").expect("the marked word is there");
    let (_dir, window) = marked_note(
        cx,
        &text,
        &[("Did you mean to spell `wrogn` this way?", start, start + 5)],
    );
    (_dir, window, start, 119)
}

/// Where the panel is: which side of its line it hangs on, and its top and bottom.
fn place(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
) -> (bool, gpui::Pixels, gpui::Pixels) {
    window
        .update(cx, |ws, window, cx| {
            let placed = ws.popup_place(window, cx).expect("a place");
            (placed.below, placed.bounds.top(), placed.bounds.bottom())
        })
        .unwrap()
}

/// Near the bottom of the window there is no room under the line, and a panel
/// whose rows run off the edge has rows nobody can reach. It goes above the line
/// instead, and stays inside the window.
#[gpui::test]
fn a_panel_near_the_bottom_of_the_window_goes_above_its_line(cx: &mut TestAppContext) {
    let (_dir, window, _start, _line) = bottom_note(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // `G k` to the last line with text on it, `^ w w` onto the marked word, ask
    // about it, then give the checker enough to say that the panel cannot fit.
    cx.simulate_keystrokes("G k ^ w w shift-k");
    give_offers(cx, &window, 12);
    paint(cx);
    assert!(shown(cx, &window), "the panel is up");

    let (below, top, bottom) = place(cx, &window);
    assert!(!below, "there is no room under a line at the bottom");

    let (window_top, window_bottom) = window
        .update(cx, |_ws, window, _cx| {
            (window.bounds().top(), window.bounds().bottom())
        })
        .unwrap();
    assert!(
        bottom <= window_bottom && top >= window_top,
        "the whole panel is drawn inside the window rather than off its edge"
    );
}

/// A panel that flipped above its line is walked away from *upwards*. Its bottom
/// edge is against its own word, so watching that edge would close the panel the
/// moment the pointer moved down onto the word it is about — the very move the
/// corridor exists to allow.
#[gpui::test]
fn a_flipped_panel_is_walked_away_from_upwards(cx: &mut TestAppContext) {
    let (_dir, window, start, line) = bottom_note(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // The line has to be on screen before the pointer can rest on it, and the
    // only way to put the last line there is to take the cursor to it.
    cx.simulate_keystrokes("G k");
    paint(cx);

    hover_at(cx, &window, line, start + 2);
    assert!(shown(cx, &window), "the pointer is over the word");
    give_offers(cx, &window, 12);
    let (below, top, _) = place(cx, &window);
    assert!(!below, "the panel had to go above the line");

    // Walk the selection down the list first. A panel that gets closed and
    // reopened under the pointer comes back at the top of its list, so this is how
    // one that was merely left alone can be told from one that was not.
    cx.simulate_keystrokes("j j j j j j j j");
    let slid = drawn_rows(cx, &window);
    assert_eq!(slid.start, 1, "the list has scrolled down a row");

    let x = x_over(cx, &window, line, start + 2);
    let (_left, _top, line_bottom) = line_box(cx, &window, line);

    // Down onto the word's own line, near its bottom edge: that is the trip from
    // the panel to the word it is about, not leaving. It is also well past the
    // panel's bottom edge, so watching *that* edge would end the panel with the
    // pointer still on the word it belongs to.
    hover_point(cx, gpui::point(x, line_bottom - gpui::px(1.)));
    assert!(
        shown(cx, &window),
        "the panel's own word is below it, and that is not leaving"
    );
    assert_eq!(
        drawn_rows(cx, &window),
        slid,
        "and the panel was not closed and reopened under the pointer"
    );
    assert!(
        shown(cx, &window),
        "the panel's own word is below it, and that is not leaving"
    );

    // Up past the panel's top edge is the way away from it, and the note's lines
    // are up there to report the move.
    hover_point(cx, gpui::point(x, top - gpui::px(60.)));
    assert!(
        !shown(cx, &window),
        "walking away above a flipped panel ends it"
    );
}
