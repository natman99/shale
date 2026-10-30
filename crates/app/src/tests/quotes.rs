//! Straight quotes drawn curly: what the rendering views paint over the note, and
//! what the note keeps.
//!
//! The file always holds the characters the keyboard typed, so each test checks
//! both halves of that deal. The frame shows curly marks; every mapping between
//! the frame and the file — a pointer's x, the cursor's column, where a wrapped
//! row breaks — still answers with the byte offsets of the straight ones.

use super::*;
use crate::editor::View;

/// A quotation, an apostrophe, and code: the three cases the scan has to tell
/// apart, since a `"` inside a code span is a delimiter in somebody's program and
/// must be drawn exactly as typed.
const NOTE: &str = "say \"hi\" and don't\n\n`let s = \"x\";`\n";

/// A bare editor holding `note`, focused and painted, as `tests::views` does.
fn painted_editor(
    note: &str,
    cx: &mut TestAppContext,
) -> (WindowHandle<MarkdownEditor>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let mut cx = VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(&mut cx, |editor, window, cx| {
            editor.set_text(note.to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    (editor, cx)
}

/// What each display line was painted with.
fn painted(editor: &WindowHandle<MarkdownEditor>, cx: &mut VisualTestContext) -> Vec<String> {
    editor
        .read_with(cx, |editor, _cx| editor.painted_text())
        .unwrap()
}

/// The painted pieces, one entry per display line and one per piece within it.
fn pieces(editor: &WindowHandle<MarkdownEditor>, cx: &mut VisualTestContext) -> Vec<Vec<String>> {
    editor
        .read_with(cx, |editor, _cx| editor.painted_pieces())
        .unwrap()
}

/// Put the editor in `view` and repaint, which is what a view change does.
fn set_view(editor: &WindowHandle<MarkdownEditor>, cx: &mut VisualTestContext, view: View) {
    editor
        .update(cx, |editor, _window, cx| editor.set_view(view, cx))
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// The note's text as the editor holds it.
fn text(editor: &WindowHandle<MarkdownEditor>, cx: &mut VisualTestContext) -> String {
    editor.read_with(cx, |editor, _cx| editor.text()).unwrap()
}

/// The rendering views draw each straight quote as the glyph its position calls
/// for, and leave the note alone.
#[gpui::test]
fn live_preview_draws_quotes_curly(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(NOTE, cx);
    assert_eq!(
        painted(&editor, &mut cx)[0],
        "say \u{201c}hi\u{201d} and don\u{2019}t",
        "an opener, a closer, and an apostrophe inside a word"
    );
    assert_eq!(
        text(&editor, &mut cx),
        NOTE,
        "the note keeps the characters the keyboard typed"
    );
}

/// Preview renders too, so it draws curly; raw shows the file, so it does not.
#[gpui::test]
fn only_the_rendering_views_draw_curly(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(NOTE, cx);
    set_view(&editor, &mut cx, View::Preview);
    assert_eq!(
        painted(&editor, &mut cx)[0],
        "say \u{201c}hi\u{201d} and don\u{2019}t",
        "preview renders, so it renders the quotes too"
    );
    set_view(&editor, &mut cx, View::Raw);
    assert_eq!(
        painted(&editor, &mut cx)[0],
        "say \"hi\" and don't",
        "raw paints the bytes in the file, quotes included"
    );
    // Back in live preview the curly marks come straight back: the frame decides
    // them, nothing has to be recomputed from the note.
    set_view(&editor, &mut cx, View::Live);
    assert_eq!(
        painted(&editor, &mut cx)[0],
        "say \u{201c}hi\u{201d} and don\u{2019}t"
    );
}

/// A quote inside code is a string delimiter, not a quotation: it is drawn as
/// typed, and it does not flip the reading of the quotes around it either.
#[gpui::test]
fn a_quote_in_code_is_left_alone(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(NOTE, cx);
    // Live preview hides the backticks; what is left is the code itself.
    assert_eq!(painted(&editor, &mut cx)[2], "let s = \"x\";");
}

/// Typing a quote puts a straight one in the note even though the frame answers
/// with a curly one, which is the whole point of substituting at paint time.
#[gpui::test]
fn typing_a_quote_keeps_it_straight(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor("", cx);
    cx.simulate_keystrokes("i");
    cx.simulate_input("she said \"yes\"");
    cx.simulate_keystrokes("escape");
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(text(&editor, &mut cx), "she said \"yes\"");
    assert_eq!(painted(&editor, &mut cx)[0], "she said \u{201c}yes\u{201d}");
}

/// A curly glyph is one character and three painted bytes where the note has one
/// byte, so the pointer and the cursor have to be answered in the note's terms.
#[gpui::test]
fn a_pointer_on_a_curly_quote_lands_on_the_straight_one(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor("say \"hi\"", cx);
    // The frame draws the quotation curly; the note still holds two straight bytes.
    assert_eq!(
        painted(&editor, &mut cx),
        vec!["say \u{201c}hi\u{201d}".to_string()]
    );
    // Bytes: `say ` then the opening quote at 4, `hi`, the closing quote at 7.
    let (at_quote, after_quote) = editor
        .read_with(&cx, |editor, _cx| {
            (editor.x_for_offset(0, 4), editor.x_for_offset(0, 5))
        })
        .unwrap();
    let (at_quote, after_quote) = (at_quote.unwrap(), after_quote.unwrap());
    assert!(
        after_quote > at_quote,
        "the glyph takes room of its own: {at_quote:?} then {after_quote:?}"
    );

    let offsets = editor
        .read_with(&cx, |editor, _cx| {
            (
                editor.offset_at_x(0, at_quote),
                editor.offset_at_x(0, after_quote),
            )
        })
        .unwrap();
    assert_eq!(
        offsets.0,
        Some(4),
        "the left edge of the curly glyph is the quote in the note"
    );
    assert_eq!(
        offsets.1,
        Some(5),
        "its right edge is the character after the quote"
    );
}

/// A row that soft-wraps is painted one display line at a time, so the quotes
/// have to be found again for each of them: a mark that falls on a break, or one
/// the line was measured with but not drawn with, shows up as a straight quote
/// somewhere in the middle of a wrapped row.
#[gpui::test]
fn a_wrapped_row_draws_every_quote_curly(cx: &mut TestAppContext) {
    let note = "\"the quick brown fox jumps over the lazy dog\", ".repeat(30);
    let (editor, cx) = painted_editor(&note, cx);
    let (ranges, painted) = editor
        .read_with(&cx, |editor, _cx| {
            (editor.display_ranges(), editor.painted_pieces())
        })
        .unwrap();
    assert!(
        ranges.len() > 3,
        "the row has to wrap, got {} lines",
        ranges.len()
    );
    assert_eq!(
        ranges.first().unwrap().0,
        0,
        "the row starts at the start of the note"
    );
    assert_eq!(ranges.last().unwrap().1, note.len(), "and ends at its end");
    for pair in ranges.windows(2) {
        assert_eq!(
            pair[0].1, pair[1].0,
            "the display lines partition the row without a gap or an overlap"
        );
    }
    // Every display line's painted text, laid end to end, is the row drawn curly:
    // 60 quotes, and not one of them may be missed at a break.
    let mut expected = String::new();
    let mut open = true;
    for ch in note.chars() {
        if ch == '"' {
            expected.push(if open { '\u{201c}' } else { '\u{201d}' });
            open = !open;
        } else {
            expected.push(ch);
        }
    }
    let all: String = painted.iter().map(|segments| segments.concat()).collect();
    assert_eq!(
        all, expected,
        "a wrapped row paints its quotes curly all the way down"
    );
}

/// A table is painted cell by cell, so its cells get their own map and have to be
/// told about the substitutions separately.
#[gpui::test]
fn a_table_cell_draws_its_quote_curly(cx: &mut TestAppContext) {
    let note = "| head | said |\n| --- | --- |\n| a | \"hi\" |\n";
    let (editor, mut cx) = painted_editor(note, cx);
    let cells = pieces(&editor, &mut cx);
    // The cursor is on the header, which brings the delimiter row back, so the
    // body row is the third display line.
    assert_eq!(
        cells.get(2).cloned().unwrap_or_default(),
        vec!["a".to_string(), "\u{201c}hi\u{201d}".to_string()],
        "the grid paints each cell from the same substitutions as an ordinary row"
    );
}
