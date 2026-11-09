//! The panel over a word that lives inside a block being edited.
//!
//! The checker follows the caret, so a note typed into through an embed is the
//! note being checked and its underlines are drawn inside the block. Asking what
//! the checker says about one of those words anchors a panel on a row counted in
//! the embedded note, not in the host's display list. Asked as a bare number, that
//! row is looked up among the lines this window painted: it finds either the wrong
//! line or, past the end of the host, no line at all — and a panel with no line to
//! hang off is a panel nothing renders, with the keyboard still pointed at it.

use super::*;

/// What a note this window holds says about itself: text and dirty flag.
fn state(
    window: &WindowHandle<Workspace>,
    name: &str,
    cx: &mut VisualTestContext,
) -> (String, bool) {
    window
        .update(cx, |ws, _window, _cx| ws.note_state(name))
        .unwrap()
        .unwrap_or_default()
}

/// Where the panel was placed, as window coordinates: `(top, bottom)`.
fn placed(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<(f32, f32)> {
    window
        .update(cx, |ws, window, cx| {
            ws.popup_place(window, cx).map(|placed| {
                (
                    placed.bounds.top().as_f32(),
                    placed.bounds.bottom().as_f32(),
                )
            })
        })
        .unwrap()
}

/// A host of one line, and an embedded note long enough that its rows run past the
/// end of the host's display list.
const HOST: &str = "![[gift]]\n";
const GIFT: &str = "one\ntwo\nthree\nfour\nfive\n";

#[gpui::test]
fn a_word_deep_in_the_block_gets_a_panel_over_the_row_it_is_on(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    // In, and three rows down: the caret is on `four`, row 3 of the embedded note,
    // a number the host's one-line display list has no line for.
    cx.simulate_keystrokes("ctrl-] j j j");
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.set_checker_notes(notes_from(&[("misspelt", 12, 16)]), cx);
        })
        .unwrap();
    paint(&mut cx);

    cx.simulate_keystrokes("K");
    paint(&mut cx);

    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.has_popup())
            .unwrap(),
        "K over a marked word should raise a panel"
    );
    // The panel is over the row it is about, not over whatever line of the host
    // happens to share the number.
    let panel = placed(&window, &mut cx).expect("the panel has to be placed");
    let rows = window
        .update(&mut cx, |ws, _window, cx| {
            ws.editor().unwrap().read_with(cx, |editor, _cx| {
                editor
                    .embed_rows()
                    .into_iter()
                    .map(|(row, _left, top, bottom)| (row, top, bottom))
                    .collect::<Vec<_>>()
            })
        })
        .unwrap();
    let (_, row_top, row_bottom) = rows
        .iter()
        .find(|(row, _, _)| *row == 3)
        .expect("row 3 of the embedded note is painted");
    assert!(
        panel.0 >= *row_top && panel.0 <= row_bottom + 40.,
        "the panel hangs below row 3 (painted at {row_top}..{row_bottom}), \
         but its top is at {}",
        panel.0
    );

    // And the keyboard is the editor's to take back.
    cx.simulate_keystrokes("escape");
    paint(&mut cx);
    assert!(
        !window
            .update(&mut cx, |ws, _window, _cx| ws.has_popup())
            .unwrap(),
        "escape should close the panel"
    );
    cx.simulate_keystrokes("i ! escape");
    assert_eq!(
        state(&window, "gift", &mut cx).0,
        "one\ntwo\nthree\n!four\nfive\n",
        "the keystroke after the panel closed should reach the embedded note"
    );
}

#[gpui::test]
fn leaving_the_block_while_its_panel_is_up_gives_the_keyboard_back(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("ctrl-]");
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.set_checker_notes(notes_from(&[("misspelt", 0, 3)]), cx);
        })
        .unwrap();
    paint(&mut cx);

    // The pointer asks about a word inside the block the way it asks about the
    // note's own — the row it reports belongs to the other note.
    let band = window
        .update(&mut cx, |ws, _window, _cx| {
            ws.editor()
                .unwrap()
                .read_with(_cx, |editor, _cx| editor.embed_rows().into_iter().next())
        })
        .unwrap()
        .expect("the block's first row is painted");
    let (_, left, top, bottom) = band;
    cx.simulate_mouse_move(
        gpui::point(gpui::px(left + 2.), gpui::px((top + bottom) / 2.)),
        None,
        gpui::Modifiers::none(),
    );
    paint(&mut cx);
    assert!(
        window
            .update(&mut cx, |ws, _window, _cx| ws.has_popup())
            .unwrap(),
        "hovering a marked word inside the block should raise a panel"
    );

    // Out of the block by way of the mouse: clicking the host's own text leaves
    // (ruling Q3). The row the panel hangs off stops being one this window paints
    // as a line, so the panel goes with it rather than keeping the keyboard.
    let (left, top, bottom) = line_box(&mut cx, &window, 0);
    cx.simulate_click(
        gpui::point(left + gpui::px(4.), (top + bottom) / 2.),
        gpui::Modifiers::default(),
    );
    paint(&mut cx);
    assert!(
        !window
            .update(&mut cx, |ws, _window, _cx| ws.has_popup())
            .unwrap(),
        "a panel with nothing to hang off goes rather than keeps the keyboard"
    );
    cx.simulate_keystrokes("i X escape");
    assert_eq!(
        state(&window, "host", &mut cx).0,
        "X![[gift]]\n",
        "the keystroke reaches the note on screen"
    );
}
