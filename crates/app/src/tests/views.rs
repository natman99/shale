//! The note's three views: raw source, live preview, and fully rendered preview.
//!
//! What is checked here is what each view decides — which markers reach the
//! frame, and whether the note takes edits — rather than how the frame is built,
//! which `tests::paint` already covers for the default view.

use super::*;
use crate::editor::View;

/// A heading, a bold run and a wiki-link: three kinds of marker, so a view that
/// hides or shows them is obvious in what it paints.
const NOTE: &str = "# Heading\n\nSome **bold** and [[Note]].\n";

/// A bare editor holding [`NOTE`], focused, and painted twice so the frame's
/// decisions are in — the same setup `tests::paint` uses.
fn painted_editor(cx: &mut TestAppContext) -> (WindowHandle<MarkdownEditor>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let mut cx = VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(&mut cx, |editor, window, cx| {
            editor.set_text(NOTE.to_string(), cx);
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

/// Which view the window's editor is in.
fn view(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> View {
    window.update(cx, |ws, _window, cx| ws.view(cx)).unwrap()
}

/// What the window said last.
fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// The open note's text.
fn note_text(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().read_with(cx, |editor, _cx| editor.text())
        })
        .unwrap()
}

/// Live preview — Shale's default — hides the markers on every row but the one
/// the cursor is on. Recorded here so the other two views are told apart from
/// it rather than from each other.
#[gpui::test]
fn live_hides_markers_off_the_cursor_row(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(cx);
    let cx = &mut cx;
    assert_eq!(view_of(&editor, cx), View::Live, "live is the default");
    let rows = painted(&editor, cx);
    assert_eq!(rows[0], "# Heading", "the cursor's row keeps its markers");
    assert_eq!(rows[2], "Some bold and Note.", "the others lose theirs");
}

/// The editor's own view, which the window's tests read through the window.
fn view_of(editor: &WindowHandle<MarkdownEditor>, cx: &mut VisualTestContext) -> View {
    editor.read_with(cx, |editor, _cx| editor.view()).unwrap()
}

/// Raw mode paints every row exactly as the file holds it: no marker hidden on
/// any row, not even the cursor's own.
#[gpui::test]
fn raw_paints_every_marker(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(cx);
    let cx = &mut cx;
    set_view(&editor, cx, View::Raw);

    let (rows, note) = (painted(&editor, cx), text(&editor, cx));
    assert_eq!(rows[0], "# Heading", "the heading marker is painted");
    assert_eq!(
        rows[2], "Some **bold** and [[Note]].",
        "and so are the emphasis and the link brackets"
    );
    assert_eq!(
        note, NOTE,
        "which is a paint decision: the note keeps every byte either way"
    );
}

/// Preview is live preview with the last row rendered too: the cursor's own row
/// hides its markers, which is the whole difference between the two.
#[gpui::test]
fn preview_hides_the_cursor_rows_markers(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(cx);
    let cx = &mut cx;
    set_view(&editor, cx, View::Preview);

    let rows = painted(&editor, cx);
    assert_eq!(
        rows[0], "Heading",
        "the row the cursor is on is rendered like every other row"
    );
    assert_eq!(rows[2], "Some bold and Note.");
    assert_eq!(text(&editor, cx), NOTE, "and the note is still the file");
}

/// Preview takes no edits. The cursor still moves — a note that cannot be read
/// from the top is no use — but nothing that changes the text lands, including
/// undo and redo, which are edits too.
#[gpui::test]
fn preview_takes_no_edits(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(cx);
    let cx = &mut cx;
    set_view(&editor, cx, View::Preview);

    cx.simulate_keystrokes("i");
    cx.simulate_input("typed");
    cx.simulate_keystrokes("escape x d d u ctrl-r");

    let (note, dirty) = editor
        .read_with(cx, |editor, _cx| (editor.text(), editor.is_dirty()))
        .unwrap();
    assert_eq!(note, NOTE, "nothing typed or deleted or undone landed");
    assert!(
        !dirty,
        "and a keystroke that changed nothing did not mark the note dirty"
    );

    // The keyboard still belongs to the note: the cursor moves.
    cx.simulate_keystrokes("j");
    let (row, _) = editor
        .read_with(cx, |editor, _cx| editor.cursor_position())
        .unwrap();
    assert_eq!(row, 1, "a motion still moves the cursor");
}

/// Raw mode still edits, because it is a view of the source rather than a lock
/// on it: the whole point of it is to let the user fix the markdown.
#[gpui::test]
fn raw_still_edits(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(cx);
    let cx = &mut cx;
    set_view(&editor, cx, View::Raw);

    cx.simulate_keystrokes("A");
    cx.simulate_input(" more");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        text(&editor, cx),
        "# Heading more\n\nSome **bold** and [[Note]].\n",
        "`A` appends to the cursor's row, as it does in any other view"
    );
}

/// The command line's three words move the note between views, and the window
/// says which one it moved to.
#[gpui::test]
fn the_command_line_moves_between_views(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", NOTE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("space f enter");
    assert_eq!(view(&window, cx), View::Live, "live is the default");

    // Each step moves to a different view, so the status line reports a change
    // every time; `source` is the alias, checked last so it is not a no-op.
    for (word, want) in [
        ("preview", View::Preview),
        ("raw", View::Raw),
        ("live", View::Live),
        ("source", View::Raw),
    ] {
        cx.simulate_keystrokes(":");
        cx.simulate_input(word);
        cx.simulate_keystrokes("enter");
        assert_eq!(view(&window, cx), want, "`:{word}`");
        assert_eq!(
            status(&window, cx),
            format!("{} view", want.word()),
            "`:{word}`"
        );
    }

    // Asking for the view the note is already in says so rather than pretending
    // to have changed something. The loop left the note in raw.
    cx.simulate_keystrokes(":");
    cx.simulate_input("raw");
    cx.simulate_keystrokes("enter");
    assert_eq!(status(&window, cx), "already in the raw view");
}

/// Preview stops the typing, and `:live` starts it again — the way the user
/// actually reaches this: read the note, then decide to fix it.
#[gpui::test]
fn preview_stops_typing_and_live_starts_it_again(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "one\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("space f enter");

    cx.simulate_keystrokes(":");
    cx.simulate_input("preview");
    cx.simulate_keystrokes("enter");

    cx.simulate_keystrokes("A");
    cx.simulate_input(" typed");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        note_text(&window, cx),
        "one\n",
        "the note did not take the typing"
    );

    cx.simulate_keystrokes(":");
    cx.simulate_input("live");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" typed");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        note_text(&window, cx),
        "one typed\n",
        "and back in the live view it does"
    );
}

/// A view change ends an insert session rather than carrying it across: an open
/// undo group over a buffer that has just stopped taking edits would mark the
/// note dirty for nothing, and the mode the user is in has to match the view
/// they are looking at.
#[gpui::test]
fn a_view_change_ends_insert_mode(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "one\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("space f enter i");
    cx.simulate_input("typed");
    assert_eq!(
        note_text(&window, cx),
        "typedone\n",
        "insert mode was typing"
    );

    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("preview");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| {
                ws.editor().read_with(cx, |editor, _cx| editor.mode())
            })
            .unwrap(),
        shale_core::vim::Mode::Normal,
        "and the palette's preview row left normal mode behind"
    );
    assert_eq!(
        note_text(&window, cx),
        "typedone\n",
        "with nothing added after it"
    );
}
