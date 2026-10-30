//! vim's command line: `:` and one word, matched exactly, and what each word does
//! to the window. The palette's fuzzy list is `tests::picker`'s subject; the two
//! are separate entry points precisely so that a one-letter command cannot become
//! a fuzzy guess.

use super::*;
use shale_core::theme::Palette;

/// The status line, which is where a command reports what it did — or refused to.
fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// The palette the window is drawn with, to check that a switch changed it and
/// that a bad name did not.
fn palette(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Palette {
    *window.update(cx, |ws, _window, _cx| ws.palette()).unwrap()
}

/// Type a command line and press enter. `:` is a normal-mode key, and a test that
/// has just been typing is in insert mode, so the line starts by leaving it.
fn type_command(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// A note open in the vault, so a write has something to write.
fn open_note(cx: &mut TestAppContext, window: &WindowHandle<Workspace>) {
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx);
        })
        .unwrap();
}

/// `:w` writes the open note. Shale has no autosave, so the reflex key does the
/// real thing rather than nothing — which is also what makes it safe to type.
#[gpui::test]
fn w_writes_the_note(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", "one line\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);

    cx.simulate_keystrokes("A");
    cx.simulate_input(" more");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "one line\n",
        "typing alone writes nothing"
    );

    type_command("w", cx);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "one line more\n",
        "the word reached the disk"
    );
    assert!(
        status(&window, cx).starts_with("saved "),
        "and said so: {}",
        status(&window, cx)
    );
}

/// A word the command line does not know runs nothing at all. This is the reason
/// `:` is not the command palette: in a fuzzy list `:w` would be a row that merely
/// contains a `w`, and enter would run it.
#[gpui::test]
fn an_unknown_command_runs_nothing(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", "one line\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);
    cx.simulate_keystrokes("A");
    cx.simulate_input(" unsaved");

    type_command("wibble", cx);
    assert_eq!(status(&window, cx), "unknown command: wibble");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "one line\n",
        "nothing was written"
    );
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.prompt_text(cx))
            .unwrap(),
        None,
        "the line closed rather than waiting to be corrected"
    );
}

/// `:e` is the quick-open picker, reached by spelling instead of by a chord.
#[gpui::test]
fn e_opens_the_note_picker(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("e", cx);
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.picker_matches(cx))
            .unwrap(),
        Some(2),
        "the picker took the command line's place"
    );
}

/// `:theme <name>` switches the whole window over without a restart.
#[gpui::test]
fn theme_names_a_palette_and_switches_to_it(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let before = palette(&window, cx);

    type_command("theme gruvbox-dark", cx);
    let now = palette(&window, cx);
    assert_eq!(now, Palette::built_in("gruvbox-dark").unwrap());
    assert_ne!(now, before, "and it was not already that");
    assert_eq!(status(&window, cx), "theme: gruvbox-dark");

    // A name that is not a theme says so and changes nothing.
    type_command("theme not-a-theme", cx);
    assert_eq!(
        palette(&window, cx),
        now,
        "a bad name leaves the palette alone"
    );
    assert!(
        status(&window, cx).starts_with("no theme"),
        "{}",
        status(&window, cx)
    );
}

/// A theme the user picks is written back into the vault's config, so the next
/// launch opens in it. The write is a splice: a config the user hand-edited keeps
/// its comments, its other settings, and the note written beside the line that
/// changed.
#[gpui::test]
fn a_chosen_theme_is_saved_to_the_vault_config(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    // The tests build a window straight from defaults, so the config file the
    // live app writes when it opens a vault is written here instead.
    let path = Config::path(dir.path());
    std::fs::create_dir_all(Config::shale_dir(dir.path())).unwrap();
    std::fs::write(
        &path,
        "[appearance]\n# the palette I look at all day\ntheme = \"mocha\" # warm\n\n[editor]\nvim_mode = true\n",
    )
    .unwrap();

    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("theme gruvbox-dark", cx);
    assert_eq!(status(&window, cx), "theme: gruvbox-dark");

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("theme = \"gruvbox-dark\" # warm"),
        "the value changes and the note beside it stays: {text}"
    );
    assert!(text.contains("# the palette I look at all day"), "{text}");
    assert!(text.contains("[editor]\nvim_mode = true"), "{text}");
    assert_eq!(
        Config::load(dir.path()).unwrap().appearance.theme,
        "gruvbox-dark",
        "and the file says what the window is drawn with"
    );
}

/// `:harper off` is understood, and switching a checker that is not running off is
/// a report rather than an error. (Starting one for real needs a `harper-ls`
/// process, which the tests deliberately do not spawn — see `tests::spell` and
/// `crate::lsp`, which cover the server itself.)
#[gpui::test]
fn harper_off_is_a_report_when_there_is_no_checker(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("harper off", cx);
    assert_eq!(status(&window, cx), "the spellchecker is already off");
    assert!(!window
        .update(cx, |ws, _window, _cx| ws.has_checker())
        .unwrap());
}

/// The way out to other programs. A plain `yy` stays in the vim register; `space y`
/// is what offers it to the system clipboard, and the status bar says what went.
#[gpui::test]
fn space_y_offers_the_note_to_other_programs(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "first line\nsecond line\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);

    cx.simulate_keystrokes("escape g g y y");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        None,
        "a plain `yy` touches nothing outside the engine"
    );

    cx.simulate_keystrokes("space y");
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("first line\n"),
        "`space y` is the door out"
    );
    assert_eq!(
        status(&window, cx),
        "copied 11 chars to the clipboard",
        "and the status bar reports what went"
    );
}

/// The way back in: `:paste` puts the clipboard at the caret *and* fills the vim
/// register, so `p` can carry the same text on.
#[gpui::test]
fn paste_brings_the_clipboard_in_and_fills_the_register(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "one\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("extra\n".into()));

    type_command("paste", cx);
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "extra\none\n",
        "the clipboard lands at the caret"
    );

    cx.simulate_keystrokes("p");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "extra\none\nextra\n",
        "and `p` repeats it, because the paste filled the vim register too"
    );
}
