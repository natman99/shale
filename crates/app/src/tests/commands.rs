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

/// `:e` is the quick-open picker, reached by spelling instead of by `ctrl-o`.
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
