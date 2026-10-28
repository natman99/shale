//! The spell and grammar checker's half of the window: what happens when the
//! server is not there, and what its notes do to a painted frame. The protocol
//! itself is tested where it lives (`crate::lsp`), so these tests are about the
//! editor and the window around it.

use super::*;

/// A checker that is not installed is a missing feature, not a broken editor:
/// the window opens, the note opens, it paints, and nothing is underlined.
#[gpui::test]
fn a_missing_checker_leaves_the_editor_alone(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.md"), "Thier spelling is wrogn.\n").unwrap();
    let vault = Rc::new(Vault::open(dir.path()).unwrap());
    let mut config = Config::default();
    config.spellcheck.command = "definitely-not-installed".into();

    cx.update(|cx| cx.bind_keys(keybindings()));
    let window = cx.add_window(|_window, cx| Workspace::new(vault.clone(), Rc::new(config), cx));
    // A visual context, so the window can actually be drawn and the frame's
    // assertions about painting are real ones.
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    window
        .update(cx, |ws, window, cx| {
            ws.focus_editor(&FocusEditor, window, cx);
        })
        .unwrap();
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx)
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let (checker, notes, text) = window
        .read_with(cx, |ws, cx| {
            (
                ws.has_checker(),
                ws.spell_note_count(),
                ws.editor().read_with(cx, |editor, _cx| editor.text()),
            )
        })
        .unwrap();
    assert!(!checker, "a server that will not start is no server");
    assert_eq!(notes, 0);
    assert!(
        text.starts_with("Thier spelling"),
        "the note opened regardless: {text}"
    );
}

/// The checker's ranges are buffer offsets, so they have to survive everything
/// that changes what is actually painted: live preview hiding a heading's `#`,
/// a wiki-link's brackets, soft wrap splitting a row. gpui asserts that runs
/// tile the text exactly, so a range that lands wrong panics the frame rather
/// than quietly underlining the wrong word.
#[gpui::test]
fn checker_notes_survive_the_painted_text(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    let text = "# Thier heading\n\nA sentence with **wrogn** words and [[a link]] in it.\n";
    let thier = text.find("Thier").unwrap();
    let wrogn = text.find("wrogn").unwrap();
    editor
        .update(cx, |editor, window, cx| {
            editor.set_text(text.to_string(), cx);
            editor.set_marks(vec![(thier, thier + 5), (wrogn, wrogn + 5)], cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    let marks = editor
        .read_with(cx, |editor, _cx| editor.marks().to_vec())
        .unwrap();
    assert_eq!(
        marks,
        vec![(thier, thier + 5), (wrogn, wrogn + 5)],
        "the frame painted without losing or moving the ranges"
    );

    // Opening another note is a different document: its underlines start empty.
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_text("A new note.\n".into(), cx)
        })
        .unwrap();
    let marks = editor
        .read_with(cx, |editor, _cx| editor.marks().to_vec())
        .unwrap();
    assert!(marks.is_empty(), "a new note inherits no underlines");
}
