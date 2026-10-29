//! Quick-open, the command palette and vault search, driven through the same
//! keymap and input paths the live app uses: the keys open the list, typed
//! characters arrive through the picker's input handler and re-rank it, and
//! `enter` chooses a row — a note to open, a line to jump to, or a command to run.

use super::*;
use crate::workspace::commands::Command;
use shale_core::theme::Palette;

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

/// A vault bigger than the cap the ranking used to stop at. The scan is every
/// note either way — `ranked` has to see all of them to know which are best — so
/// a low cap buys nothing except a list that quietly leaves the note the user was
/// looking for out.
#[gpui::test]
fn a_big_vault_is_not_truncated_at_the_old_cap(cx: &mut TestAppContext) {
    let names: Vec<String> = (0..300).map(|i| format!("note{i}.md")).collect();
    let names: Vec<&str> = names.iter().map(|name| name.as_str()).collect();
    let (_dir, window) = workspace(cx, &names);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        matches(&window, cx),
        Some(300),
        "an empty query is every note"
    );

    cx.simulate_input("note");
    assert_eq!(
        matches(&window, cx),
        Some(300),
        "and a query that matches all of them keeps all of them, past the 200 the list used to stop at"
    );
}

/// The arrows move the highlighted row, and the note under the cursor is never
/// touched: the picker is a list, not the buffer.
#[gpui::test]
fn the_arrow_keys_move_the_highlight(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o down");
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("notes/Another Note.md")
    );
    cx.simulate_keystrokes("up");
    assert_eq!(choice(&window, cx).as_deref(), Some("Welcome.md"));
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "",
        "the note was never touched"
    );
}

/// `j` and `k` are letters, and a picker is a text field: they have to be typed,
/// not spent on moving the selection. A vault with a note called `json.md` is the
/// reason — with `j` bound, that note cannot be reached by typing its name.
#[gpui::test]
fn j_and_k_are_typing_in_a_picker(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/json.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(matches(&window, cx), Some(2));
    cx.simulate_keystrokes("j");
    assert_eq!(
        matches(&window, cx),
        Some(1),
        "`j` narrowed the list, so it went into the query"
    );
    cx.simulate_keystrokes("s o n");
    assert_eq!(choice(&window, cx).as_deref(), Some("notes/json.md"));
    cx.simulate_keystrokes("enter");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "notes/json.md", "and the query chose it");
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

/// `:` is vim's command line, and it is deliberately *not* the palette: a fuzzy
/// list would turn `:w` into whatever ranks best for the letter `w`. What the word
/// itself means is `tests::commands`' business; here it is only that no list opens.
#[gpui::test]
fn colon_opens_a_command_line_rather_than_the_palette(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes(":");
    assert_eq!(matches(&window, cx), None, "no list is open");
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.prompt_text(cx))
            .unwrap()
            .is_some(),
        "the command line is"
    );
    // Escape puts the note back, so the next key is a motion again.
    cx.simulate_keystrokes("escape j");
    assert!(window
        .update(cx, |ws, _window, cx| ws.prompt_text(cx))
        .unwrap()
        .is_none());
}

/// `space s` is the leader's other chord: the vault search. Like `space f`, the
/// vim engine owns it, because `space` is also the right motion.
#[gpui::test]
fn space_s_opens_vault_search(cx: &mut TestAppContext) {
    let (_dir, window) = needle_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space s");
    assert_eq!(
        matches(&window, cx),
        Some(0),
        "the search picker is open, and has read nothing yet"
    );
    cx.simulate_input("needle");
    assert_eq!(matches(&window, cx), Some(2), "one row per matching line");
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

/// The palette the window is drawn with, to check that a preview changed it and
/// that `escape` put it back.
fn palette(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Palette {
    *window.update(cx, |ws, _window, _cx| ws.palette()).unwrap()
}

/// The theme picker opens on the theme in use, previews the row it points at,
/// keeps the row that is chosen, and puts back the palette it was opened from
/// when nothing was chosen.
#[gpui::test]
fn the_theme_picker_previews_and_keeps(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["Welcome.md"]);
    // The config file the live app writes when it opens a vault, so the saves the
    // picker does have something to splice into.
    std::fs::create_dir_all(Config::shale_dir(dir.path())).unwrap();
    std::fs::write(Config::path(dir.path()), Config::starter()).unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Not the first built-in, which is the case that matters: a picker that opens
    // on a theme the user is not looking at puts a row they did not choose at the
    // top, and the theme they meant is somewhere under their own selection.
    cx.simulate_keystrokes(":");
    cx.simulate_input("theme gruvbox-dark");
    cx.simulate_keystrokes("enter");
    let in_use = Palette::built_in("gruvbox-dark").unwrap();
    assert_eq!(palette(&window, cx), in_use, "the window is drawn in it");

    cx.simulate_keystrokes(":");
    cx.simulate_input("theme");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        matches(&window, cx),
        Some(Palette::BUILT_INS.len()),
        "every built-in is a row"
    );
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("gruvbox-dark"),
        "the theme on screen is the row the picker opens highlighted on"
    );
    assert_eq!(palette(&window, cx), in_use, "opening changes nothing");

    // The arrows move the highlight, which previews the row they land on.
    cx.simulate_keystrokes("down");
    let previewed = choice(&window, cx).expect("a row to preview");
    assert_ne!(previewed, "gruvbox-dark", "a different row");
    let painted = palette(&window, cx);
    assert_ne!(painted, in_use, "the preview is on screen");

    cx.simulate_keystrokes("escape");
    assert_eq!(matches(&window, cx), None);
    assert_eq!(palette(&window, cx), in_use, "escape was not an answer");
    assert!(
        std::fs::read_to_string(Config::path(dir.path()))
            .unwrap()
            .contains("theme = \"gruvbox-dark\""),
        "a preview is not a choice, so the file still names the theme that was chosen"
    );

    // Choosing a row keeps it.
    cx.simulate_keystrokes(":");
    cx.simulate_input("theme");
    cx.simulate_keystrokes("enter down enter");
    assert_eq!(palette(&window, cx), painted);
    assert_eq!(
        window.update(cx, |ws, _window, _cx| ws.status()).unwrap(),
        format!("theme: {previewed}")
    );
    // And a choice is saved, so the next launch opens in the theme on screen.
    assert!(
        std::fs::read_to_string(Config::path(dir.path()))
            .unwrap()
            .contains(&format!("theme = \"{previewed}\"")),
        "the config the next launch reads names the chosen theme"
    );
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

/// The notes every search test uses: the query is in one line of each, in
/// different folders, so a hit has to carry both its note and its line.
fn needle_vault(cx: &mut TestAppContext) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    vault_with(
        cx,
        &[
            ("Welcome.md", "first line\nthe needle is here\nlast\n"),
            ("notes/Deep.md", "another needle, deeper down\n"),
        ],
    )
}

/// `ctrl-shift-f` has to read every note to find its rows, so unlike the other
/// two pickers it is given a row source and asks it on each keystroke.
#[gpui::test]
fn ctrl_shift_f_finds_the_lines_that_hold_the_query(cx: &mut TestAppContext) {
    let (_dir, window) = needle_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    assert_eq!(
        matches(&window, cx),
        Some(0),
        "nothing has been typed, so no note has been read"
    );

    cx.simulate_input("needle");
    assert_eq!(matches(&window, cx), Some(2), "one row per matching line");
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("Welcome.md:2"),
        "a row is the note and the line, in the order the notes were read"
    );
    cx.simulate_keystrokes("down");
    assert_eq!(choice(&window, cx).as_deref(), Some("notes/Deep.md:1"));

    cx.simulate_keystrokes("backspace backspace backspace backspace backspace backspace");
    assert_eq!(
        matches(&window, cx),
        Some(0),
        "erasing the query leaves no rows, rather than every line in the vault"
    );
}

/// Choosing a hit opens its note with the cursor on its line, which is the whole
/// point of the exercise: the user was reading that line in the picker.
#[gpui::test]
fn a_search_row_opens_the_note_on_its_line(cx: &mut TestAppContext) {
    let (_dir, window) = needle_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("needle");
    cx.simulate_keystrokes("enter");

    let (_, active) = state(&window, cx);
    assert_eq!(active, "Welcome.md");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.cursor_position(cx))
            .unwrap(),
        (1, 0),
        "the cursor is at the start of the line the row showed, not the top of the note"
    );
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "first line\nthe needle is here\nlast\n"
    );

    cx.simulate_keystrokes("i");
    cx.simulate_input("x");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "first line\nxthe needle is here\nlast\n",
        "the keyboard is back on the note, so the next key is a vim command"
    );
}

/// A query nothing matches offers nothing: `enter` has to be inert rather than
/// open an arbitrary note.
#[gpui::test]
fn a_search_with_no_hits_has_nothing_to_choose(cx: &mut TestAppContext) {
    let (_dir, window) = needle_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("zzzz");
    assert_eq!(matches(&window, cx), Some(0));
    assert_eq!(choice(&window, cx), None);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        matches(&window, cx),
        Some(0),
        "enter on an empty list leaves the picker up"
    );
    let (_, active) = state(&window, cx);
    assert_eq!(active, "", "and no note was opened");
}
