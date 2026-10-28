//! The keymap itself: the keystroke shapes Linux really delivers, what one key
//! resolves to in each context, and the two-stroke chords that wait for their
//! second key. A binding that only works in the core engine's unit tests is
//! not wired up, so the shapes checked here are the load-bearing part.

use super::*;

/// Typing printable characters must reach the buffer through gpui's input
/// handler, not just the keys we bind to actions.
#[gpui::test]
fn typing_in_insert_mode_inserts_text(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));

    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, window, cx| {
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();

    cx.simulate_keystrokes("i");
    cx.simulate_input("hello");

    let (mode, text) = editor
        .read_with(cx, |editor, _cx| (editor.mode(), editor.text()))
        .unwrap();
    assert_eq!(
        mode,
        shale_core::vim::Mode::Insert,
        "`i` should enter insert"
    );
    assert_eq!(text, "hello", "typed characters should land in the buffer");

    // Normal mode swallows typed characters instead of inserting them.
    cx.simulate_keystrokes("escape");
    cx.simulate_input("xyz");
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, "hello", "normal mode should swallow typed input");
}

/// The Linux platform delivers ctrl+z as `key = "z"` with the control
/// modifier and *no* `key_char` (control characters are filtered out of
/// `key_char` by `keystroke_from_xkb`). Check that our keymap really matches
/// that shape, independent of any window or focus state. This is the test
/// that catches modifier syntax mistakes: `simulate_keystrokes("ctrl+z")`
/// parses through the same parser as the binding, so a typo like `+`
/// matches itself and looks fine.
#[test]
fn keymap_matches_the_linux_keystroke_shape() {
    let keymap = gpui::Keymap::new(keybindings());
    let contexts = vec![gpui::KeyContext::try_from("vim-normal").unwrap()];
    for (control, key, action) in [
        (true, "z", "shale_editor::Undo"),
        (true, "r", "shale_editor::Redo"),
        (true, "y", "shale_editor::Redo"),
        (true, "s", "shale::Save"),
        (false, "k", "shale_editor::VimK"),
        (false, "$", "shale_editor::VimDollar"),
        (false, ".", "shale_editor::VimDot"),
        (false, "^", "shale_editor::VimCaret"),
        // vim's command line: Linux delivers shift+`;` as the `:` keysym with
        // the shift modifier dropped, so it is bound as the symbol itself.
        (false, ":", "shale_editor::VimColon"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                control,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "typed {key} (control {control}) should resolve to {action}"
        );
    }

    // The visual context has to carry the same bindings, or the keys stop
    // working the moment `v` is typed.
    let contexts = vec![gpui::KeyContext::try_from("vim-visual").unwrap()];
    for (control, shift, key, action) in [
        (false, false, "k", "shale_editor::VimK"),
        (false, false, "v", "shale_editor::VimV"),
        (false, true, "v", "shale_editor::VimUpperV"),
        (false, true, "h", "shale_editor::VimUpperH"),
        (false, true, "l", "shale_editor::VimUpperL"),
        (false, false, "$", "shale_editor::VimDollar"),
        (true, false, "z", "shale_editor::Undo"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                control,
                shift,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "visual mode: typed {key} should resolve to {action}"
        );
    }

    // The normal-mode-only table edits. A shifted letter arrives as the
    // lowercase key with the shift modifier; a shifted symbol arrives as the
    // symbol itself with no shift, so `|` is bound literally.
    let contexts = vec![gpui::KeyContext::try_from("vim-normal").unwrap()];
    for (shift, key, action) in [
        (true, "d", "shale_editor::VimUpperD"),
        (false, "|", "shale_editor::VimPipe"),
        (false, "\\", "shale_editor::VimBackslash"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                shift,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "normal mode: typed {key} should resolve to {action}"
        );
    }

    // alt+hjkl: unlike shift, alt leaves the letter's keysym alone, so the
    // platform delivers the plain letter with the alt modifier set.
    for (key, action) in [
        ("h", "shale_editor::VimAltH"),
        ("j", "shale_editor::VimAltJ"),
        ("k", "shale_editor::VimAltK"),
        ("l", "shale_editor::VimAltL"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                alt: true,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "alt-{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "alt-{key} should resolve to {action}"
        );
    }

    // The sidebar and the vault edits. A ctrl+shift+letter arrives as the
    // lowercase key with both modifiers set, the same shape ctrl-shift-c needs.
    let contexts = vec![gpui::KeyContext::try_from("tree").unwrap()];
    for (control, shift, key, action) in [
        (true, false, "n", "shale::NewNote"),
        (true, true, "n", "shale::NewFolder"),
        (true, true, "r", "shale::RenameNote"),
        (true, true, "d", "shale::DeleteNote"),
        (true, true, "p", "shale::CommandPalette"),
        (false, false, "j", "shale::TreeDown"),
        (false, false, "k", "shale::TreeUp"),
        (false, false, "enter", "shale::TreeOpen"),
        (false, true, "d", "shale::DeleteNote"),
        (false, false, "escape", "shale::FocusEditor"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                control,
                shift,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "the tree: typed {key} should resolve to {action}"
        );
    }

    // The prompt takes the keys that are not text; printable characters arrive
    // through the input handler instead. The negative half matters most: the
    // prompt is drawn outside the tree's key context, so the tree's `j`/`k` are
    // not in its context stack and cannot steal a typed letter.
    let contexts = vec![gpui::KeyContext::try_from("prompt").unwrap()];
    for (key, action) in [
        ("enter", "shale::FieldSubmit"),
        ("escape", "shale::FieldCancel"),
        ("backspace", "shale::FieldBackspace"),
        ("delete", "shale::FieldDelete"),
        ("left", "shale::FieldLeft"),
        ("right", "shale::FieldRight"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers::default(),
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "the prompt: typed {key} should resolve to {action}"
        );
    }
    for key in ["j", "k", "r", "n", "h", "l"] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers::default(),
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, _) = keymap.bindings_for_input(&[typed], &contexts);
        assert!(
            bindings.is_empty(),
            "the prompt must not answer {key}, which is a character being typed"
        );
    }

    // A picker: the same field keys, plus `j`/`k` for the highlighted row — and
    // nothing else, because every other letter is part of the query being typed.
    let contexts = vec![gpui::KeyContext::try_from("picker").unwrap()];
    for (key, action) in [
        ("enter", "shale::FieldSubmit"),
        ("escape", "shale::FieldCancel"),
        ("backspace", "shale::FieldBackspace"),
        ("delete", "shale::FieldDelete"),
        ("left", "shale::FieldLeft"),
        ("right", "shale::FieldRight"),
        ("j", "shale::PickerDown"),
        ("k", "shale::PickerUp"),
        ("down", "shale::PickerDown"),
        ("up", "shale::PickerUp"),
    ] {
        assert_eq!(
            resolved(&keymap, key, &contexts),
            vec![action.to_string()],
            "the picker: typed {key} should resolve to {action}"
        );
    }
    for key in ["x", "r", "n", "h", "l", "i"] {
        assert!(
            resolved(&keymap, key, &contexts).is_empty(),
            "the picker must not answer {key}, which is a character being typed"
        );
    }
}

/// The actions one plain keystroke resolves to in the given key contexts.
fn resolved(keymap: &gpui::Keymap, key: &str, contexts: &[gpui::KeyContext]) -> Vec<String> {
    let typed = gpui::Keystroke {
        modifiers: gpui::Modifiers::default(),
        key: key.to_string(),
        key_char: None,
    };
    let (bindings, pending) = keymap.bindings_for_input(&[typed], contexts);
    assert!(!pending, "{key} should not leave pending input");
    bindings
        .iter()
        .map(|b| b.action().name().to_string())
        .collect()
}

/// `ctrl-w h` and `ctrl-w l` are two-stroke chords, which gpui matches natively:
/// the first stroke is held as pending input and the second decides. Checking
/// the held half matters, because a chord that resolved on the first stroke
/// would move the keyboard before the user finished typing.
#[test]
fn the_window_chords_wait_for_their_second_stroke() {
    let keymap = gpui::Keymap::new(keybindings());
    let contexts = vec![gpui::KeyContext::try_from("vim-normal").unwrap()];
    let typed = |modifiers: gpui::Modifiers, key: &str| gpui::Keystroke {
        modifiers,
        key: key.to_string(),
        key_char: None,
    };
    let names = |bindings: &[gpui::KeyBinding]| {
        bindings
            .iter()
            .map(|b| b.action().name().to_string())
            .collect::<Vec<_>>()
    };

    let (bindings, pending) =
        keymap.bindings_for_input(&[typed(gpui::Modifiers::control(), "w")], &contexts);
    assert!(pending, "`ctrl-w` waits for the key that finishes it");
    assert!(bindings.is_empty(), "and runs nothing until it arrives");

    for (key, action) in [("h", "shale::WindowLeft"), ("l", "shale::WindowRight")] {
        let (bindings, pending) = keymap.bindings_for_input(
            &[
                typed(gpui::Modifiers::control(), "w"),
                typed(gpui::Modifiers::none(), key),
            ],
            &contexts,
        );
        assert!(!pending, "`ctrl-w {key}` is a complete chord");
        assert_eq!(names(&bindings), vec![action], "`ctrl-w {key}`");
    }

    // `space` is deliberately *not* a keymap prefix: it is vim's right motion,
    // and a `space f` chord here would make gpui hold every space until the
    // next key. That is why the chord lives in the vim engine instead.
    let (bindings, pending) =
        keymap.bindings_for_input(&[typed(gpui::Modifiers::none(), "space")], &contexts);
    assert!(!pending, "`space` is a motion, not a prefix");
    assert_eq!(names(&bindings), vec!["shale_editor::VimSpace"]);
}
