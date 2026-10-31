//! Asking the checker with `K`, and what the panel does with the keyboard: escape,
//! enter, a digit naming a row, and the list scrolling under it.

use super::*;

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

/// The preview view shows no checker at all: a panel that was up does not survive
/// the view change, and neither `K` nor the pointer can open one while preview is
/// on. Back in the default view both work again, because preview hides the
/// checker's notes rather than dropping them.
#[gpui::test]
fn the_preview_view_shows_no_checker(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    // A panel is up, and it holds the keyboard — so the way to the preview view
    // from here is the palette, whose key is bound with no context.
    cx.simulate_keystrokes("^ shift-k");
    assert!(shown(cx, &window), "a panel is up in the default view");
    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("preview");
    cx.simulate_keystrokes("enter");
    assert!(
        !shown(cx, &window),
        "the panel did not outlive the view it cannot belong to"
    );

    cx.simulate_keystrokes("shift-k");
    assert!(!shown(cx, &window), "`K` asks for nothing in preview");
    hover_at(cx, &window, 0, 2);
    assert!(!shown(cx, &window), "and neither does the pointer");

    // The note has the keyboard back (the panel closed and returned it), so the
    // command line reaches it.
    cx.simulate_keystrokes(":");
    cx.simulate_input("live");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("^ shift-k");
    assert!(
        shown(cx, &window),
        "and the default view shows the checker again"
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
