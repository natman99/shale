//! Quick-open and the command palette, driven through the same keymap and input
//! paths the live app uses: the keys open the list, typed characters arrive
//! through the picker's input handler and re-rank it, and `enter` chooses a row —
//! a note to open, or a command to run.

use super::*;
use crate::workspace::commands::Command;

/// What the picker would choose right now, as the note path behind that row.
fn choice(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |ws, _window, cx| ws.picker_choice(cx))
        .unwrap()
}

/// How many rows the picker is offering, or `None` while none is open.
fn matches(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<usize> {
    window
        .update(cx, |ws, _window, cx| ws.picker_matches(cx))
        .unwrap()
}

#[gpui::test]
fn ctrl_o_lists_every_note_and_enter_opens_the_top_row(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        matches(&window, cx),
        Some(2),
        "an empty query is every note"
    );
    assert_eq!(choice(&window, cx).as_deref(), Some("Welcome.md"));

    cx.simulate_keystrokes("enter");
    assert_eq!(matches(&window, cx), None, "and the picker is gone");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "Welcome.md", "the highlighted row was opened");
}

#[gpui::test]
fn typing_narrows_the_list_to_the_best_match(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o");
    cx.simulate_input("anot");
    assert_eq!(matches(&window, cx), Some(1));
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("notes/Another Note.md"),
        "the query is matched against the whole path, not just the name"
    );

    cx.simulate_keystrokes("enter");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "notes/Another Note.md");
}

/// `j`/`k` belong to the picker while it is open: they move the highlighted row
/// rather than the vim cursor, and the note under the cursor does not move.
#[gpui::test]
fn j_and_k_move_the_highlight_not_the_cursor(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o j");
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("notes/Another Note.md")
    );
    cx.simulate_keystrokes("k");
    assert_eq!(choice(&window, cx).as_deref(), Some("Welcome.md"));
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "",
        "the note was never touched"
    );
}

/// Backspace widens the list again, which is how a mistyped query is fixed.
#[gpui::test]
fn backspace_widens_the_list_again(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o");
    cx.simulate_input("zzz");
    assert_eq!(matches(&window, cx), Some(0), "nothing matches");
    assert_eq!(choice(&window, cx), None, "so there is nothing to choose");

    cx.simulate_keystrokes("backspace backspace backspace");
    assert_eq!(matches(&window, cx), Some(2));
}

/// Escape closes the picker and gives the keyboard back to the note, so the next
/// keystroke is a vim motion and not a query character.
#[gpui::test]
fn escape_closes_the_picker_and_gives_the_keyboard_back(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o escape");
    assert_eq!(matches(&window, cx), None);
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.mode_label(cx))
            .unwrap(),
        "NORMAL"
    );
    cx.simulate_keystrokes("i");
    cx.simulate_input("typed");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "typed"
    );
}

/// How many commands the palette offers.
fn command_count() -> usize {
    Command::all().len()
}

/// `ctrl-shift-p` opens the palette with every command in it, labelled with the
/// keys that reach the same action.
#[gpui::test]
fn ctrl_shift_p_lists_every_command(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-p");
    assert_eq!(matches(&window, cx), Some(command_count()));
    assert_eq!(choice(&window, cx).as_deref(), Some("Save the note"));
}

/// `:` is vim's command line, and the palette is what answers it.
#[gpui::test]
fn colon_in_normal_mode_opens_the_palette(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes(":");
    assert_eq!(matches(&window, cx), Some(command_count()));
    // Escape puts the note back, so the next key is a motion again.
    cx.simulate_keystrokes("escape j");
    assert_eq!(matches(&window, cx), None);
}

/// Choosing a row runs the command as if its key had been typed: here the
/// palette's "New note" reaches the prompt, and the prompt creates the file.
#[gpui::test]
fn a_palette_row_runs_the_command_it_names(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("new note");
    assert_eq!(matches(&window, cx), Some(1), "the query narrows the list");
    assert_eq!(choice(&window, cx).as_deref(), Some("New note"));
    cx.simulate_keystrokes("enter");

    assert_eq!(
        matches(&window, cx),
        None,
        "the palette is gone once a row is chosen"
    );
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.prompt_text(cx))
            .unwrap()
            .is_some(),
        "and the command's own prompt is up"
    );
    cx.simulate_input("From the palette");
    cx.simulate_keystrokes("enter");
    assert!(
        dir.path().join("From the palette.md").is_file(),
        "the command ran"
    );
}

/// A palette command that opens another picker replaces the first one, which is
/// what keeps the keyboard on the list the user is working through.
#[gpui::test]
fn a_palette_row_can_open_the_note_picker(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-p");
    cx.simulate_input("open a note");
    assert_eq!(choice(&window, cx).as_deref(), Some("Open a note"));
    cx.simulate_keystrokes("enter");
    assert_eq!(
        matches(&window, cx),
        Some(2),
        "the note picker took the palette's place"
    );
    assert_eq!(choice(&window, cx).as_deref(), Some("Welcome.md"));
}

/// `space f` is the other way in. The vim engine owns the chord, because `space`
/// is also vim's right motion and gpui would have to hold it back if the keymap
/// treated it as a prefix.
#[gpui::test]
fn space_f_opens_quick_open(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space f");
    assert_eq!(matches(&window, cx), Some(2));
    assert_eq!(choice(&window, cx).as_deref(), Some("Welcome.md"));
}

/// `ctrl-w h` hands the keyboard to the sidebar and `ctrl-w l` takes it back.
#[gpui::test]
fn ctrl_w_moves_the_keyboard_between_panels(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/First.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-w h j");
    assert_eq!(
        window
            .update(cx, |ws, _window, _cx| ws.tree_selection())
            .unwrap(),
        Some(PathBuf::from("notes/First.md")),
        "the sidebar answered `j`, so it has the keyboard"
    );

    cx.simulate_keystrokes("ctrl-w l i");
    cx.simulate_input("typed");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "typed",
        "and the note has it back"
    );
}
