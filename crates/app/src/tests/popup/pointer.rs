//! A resting pointer: opening the panel without being asked, the trip from the word
//! down onto the panel, and the panel getting out of the way of the next word.

use super::*;

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
                .unwrap()
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

/// A note of `rows` filler rows with a marked word embedded in each row listed in
/// `marks`. The word sits at column 7 of its row, so the same column of a neighbouring
/// row is the same place in its word.
fn tall_marked(
    cx: &mut TestAppContext,
    rows: usize,
    marks: &[(usize, &str)],
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let mut text = String::new();
    let mut notes: Vec<(String, usize, usize)> = Vec::new();
    for row in 0..rows {
        let start = text.len();
        match marks.iter().find(|(at, _)| *at == row) {
            Some((_, word)) => {
                text.push_str(&format!("filler {word} filler\n"));
                notes.push((
                    format!("Did you mean to spell `{word}` this way?"),
                    start + 7,
                    start + 7 + word.len(),
                ));
            }
            None => text.push_str("filler line filler\n"),
        }
    }
    let notes: Vec<(&str, usize, usize)> = notes
        .iter()
        .map(|(message, start, end)| (message.as_str(), *start, *end))
        .collect();
    let (dir, window) = vault_with(cx, &[("a.md", text.as_str())]);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx);
            ws.set_checker_notes(notes_from(&notes), cx);
        })
        .unwrap();
    (dir, window)
}

/// How far one display row is, measured from two painted ones.
fn row_step(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> gpui::Pixels {
    let (_, top0, _) = line_box(cx, window, 0);
    let (_, top4, _) = line_box(cx, window, 4);
    (top4 - top0) / 4.
}

/// Scroll the note by `rows` display rows, and run the frames that follow: the re-ask is
/// queued for the next frame, which is when the scrolled paint has recorded where its rows
/// went.
fn scroll_rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, rows: f32) {
    let step = row_step(cx, window);
    window
        .update(cx, |ws, _w, cx| {
            ws.editor()
                .unwrap()
                .update(cx, |e, _cx| e.scroll_by(step * rows))
        })
        .unwrap();
    repaint(cx);
    paint(cx);
}

/// Scrolling moves the text under a pointer that never moved, and gpui reports no mouse
/// move for it. The panel follows its word — its place is worked out from the line every
/// frame — and then the pointer is asked again what it is on.
#[gpui::test]
fn the_panel_follows_its_word_when_the_note_scrolls(cx: &mut TestAppContext) {
    let (_dir, window) = tall_marked(cx, 90, &[(5, "thier")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);
    hover_at(cx, &window, 5, 5 * 19 + 9);
    assert!(shown(cx, &window), "up on the word");
    let (_, before, _) = place(cx, &window);
    // One row: enough to move the panel, short enough that the pointer is still on it.
    scroll_rows(&window, cx, 1.);
    let (_, after, _) = place(cx, &window);
    assert!(
        shown(cx, &window),
        "the pointer is still on the panel, which is a valid place for it to be"
    );
    let step = row_step(cx, &window);
    let moved = before - after;
    assert!(
        (moved - step).abs() < step,
        "the panel moved with its word: {moved:?} for one row of {step:?}"
    );
}

/// The second half of the rule: once the panel has moved, the pointer is asked again. Four
/// rows of scroll put a *different* marked word under a pointer that never moved, and the
/// panel goes with the answer.
#[gpui::test]
fn scrolling_onto_another_marked_word_moves_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = tall_marked(cx, 90, &[(5, "thier"), (9, "anotehr")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);
    hover_at(cx, &window, 5, 5 * 19 + 9);
    assert_eq!(
        saying(cx, &window).as_deref(),
        Some("Did you mean to spell `thier` this way?")
    );
    scroll_rows(&window, cx, 4.);
    assert_eq!(
        saying(cx, &window).as_deref(),
        Some("Did you mean to spell `anotehr` this way?"),
        "the pointer is over the other word now, and the panel followed the pointer"
    );
}

/// A pointer that ends up on neither the word nor the panel ends it. The scroll is chosen
/// from the panel's own height so the pointer lands below it, and the word's row is still
/// on screen afterwards: the rule that drops a surface whose anchor left the viewport
/// cannot be what closed this one.
#[gpui::test]
fn scrolling_off_the_word_closes_the_panel(cx: &mut TestAppContext) {
    let (_dir, window) = tall_marked(cx, 90, &[(30, "thier")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);
    hover_at(cx, &window, 30, 30 * 19 + 9);
    assert!(shown(cx, &window), "up on the word");
    let step = row_step(cx, &window);
    let bottom = panel_bottom(cx, &window);
    let (_, word_top, _) = line_box(cx, &window, 30);
    let rows = (bottom - word_top) / step + 2.;
    assert!(
        rows < 28.,
        "the scroll has to leave the word's row on screen; the panel is {rows:?} rows tall"
    );
    scroll_rows(&window, cx, rows);
    assert!(
        line_box(cx, &window, 2).2 > gpui::px(0.),
        "the word's row is still on screen"
    );
    assert!(!shown(cx, &window), "the pointer is below the panel now");
}

/// The band that keeps a panel alive while the pointer travels to it is the panel's own
/// width plus the column of the word it is about — not the whole window. A pointer at the
/// panel's height on the far side of the note is on the way to nothing.
#[gpui::test]
fn the_panel_band_is_not_the_whole_window(cx: &mut TestAppContext) {
    let (_dir, window) = marked_note(
        cx,
        "Thier spelling is wrogn.\nfiller line\nfiller line\n",
        &[("Did you mean to spell `thier` this way?", 0, 5)],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);
    hover_at(cx, &window, 0, 2);
    assert!(shown(cx, &window), "up on the word");
    let bounds = window
        .update(cx, |ws, window, cx| {
            ws.popup_place(window, cx).expect("a place").bounds
        })
        .unwrap();
    let (_, top2, bottom2) = line_box(cx, &window, 2);
    hover_point(
        cx,
        gpui::point(bounds.right() + gpui::px(80.), (top2 + bottom2) / 2.),
    );
    assert!(
        !shown(cx, &window),
        "beside the panel, over text that is not marked, is not on the way to a row"
    );
}

/// The wheel scrolls the note, and gpui dirties only the view that owns the scroller. The
/// note has to report the scroll, because the window is what places a card or a panel over
/// the note and re-runs the pointer's collision — and in the live frame loop a frame that
/// only dirties the note never renders the window at all. (The test harness re-renders
/// every view regardless, so the consequence is not visible headlessly; what is visible is
/// that the wheel reaches the window.)
#[gpui::test]
fn the_note_reports_a_wheel_scroll_to_the_window(cx: &mut TestAppContext) {
    let (_dir, window) = tall_marked(cx, 90, &[(30, "thier")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);
    let at = pointer_over(cx, &window, 30, 30 * 19 + 9);
    hover_point(cx, at);
    assert!(shown(cx, &window), "up on the word");
    let before = window
        .update(cx, |ws, _w, _cx| ws.scroll_reports())
        .unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: at,
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-60.))),
        ..Default::default()
    });
    paint(cx);
    let after = window
        .update(cx, |ws, _w, _cx| ws.scroll_reports())
        .unwrap();
    assert!(after > before, "the wheel never reached the window");
    assert!(
        window
            .update(cx, |ws, _w, _cx| ws.surface_scroll())
            .unwrap()
            < 0.0,
        "and the note really did scroll"
    );
}

/// The frame after a scroll re-*places* the surface, not only re-asks the pointer.
/// A surface is placed from the note's last painted rows, and within a frame the window
/// renders before the note paints — so the frame that noticed the scroll placed the
/// surface where its word used to be, and a surface the pointer is still on kept that
/// stale place. (Whether the window re-renders is the live frame loop's decision, which
/// the harness does not model; what is pinned here is that the scroll's frame contains
/// the re-place step at all.)
#[gpui::test]
fn a_scroll_frame_replaces_the_surface(cx: &mut TestAppContext) {
    let (_dir, window) = tall_marked(cx, 90, &[(30, "thier")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);
    let at = pointer_over(cx, &window, 30, 30 * 19 + 9);
    hover_point(cx, at);
    assert!(shown(cx, &window), "up on the word");
    let before = window
        .update(cx, |ws, _w, _cx| ws.surface_replace())
        .unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: at,
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-60.))),
        ..Default::default()
    });
    repaint(cx);
    assert!(
        window
            .update(cx, |ws, _w, _cx| ws.surface_replace())
            .unwrap()
            > before,
        "the scroll re-asked the pointer but never placed the surface again"
    );
}
