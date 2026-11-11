//! Quick-open, the command palette and vault search, driven through the same
//! keymap and input paths the live app uses: the keys open the list, typed
//! characters arrive through the picker's input handler and re-rank it, and
//! `enter` chooses a row — a note to open, a line to jump to, or a command to run.

use super::*;
use crate::command::Command;
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
fn space_f_lists_every_note_and_enter_opens_the_top_row(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space f");
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

/// `ctrl-backspace` in a picker takes the last word of the query, which is the
/// fastest way back from a query that narrowed too far.
#[gpui::test]
fn ctrl_backspace_widens_a_query_a_word_at_a_time(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["today note.md", "today other.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space f");
    assert_eq!(
        matches(&window, cx),
        Some(2),
        "an empty query is every note"
    );

    cx.simulate_input("today note");
    assert_eq!(
        matches(&window, cx),
        Some(1),
        "the second word narrowed the list"
    );

    cx.simulate_keystrokes("ctrl-backspace");
    assert_eq!(
        matches(&window, cx),
        Some(2),
        "and ctrl-backspace took that word back off"
    );
}

/// Two frames, so the frame's hitboxes exist and a click or a hover lands on
/// something the window drew.
fn paint(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// A click outside the picker closes it. The list floats over the note, and a
/// click under it used to belong to nothing at all, so the panel just stayed up
/// until `escape`.
#[gpui::test]
fn clicking_off_the_picker_closes_it(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes("space f");
    assert_eq!(matches(&window, cx), Some(2), "the list is up");

    // Well below the panel, in the note.
    cx.simulate_click(gpui::point(px(500.), px(700.)), gpui::Modifiers::none());
    assert_eq!(
        matches(&window, cx),
        None,
        "a click outside the panel is a way out"
    );
}

/// The sidebar is outside the panel too. The list is the window's, not the note
/// column's, so clicking a row there closes it.
#[gpui::test]
fn clicking_the_sidebar_off_the_picker_closes_it(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes("space f");
    assert_eq!(matches(&window, cx), Some(2));

    // Inside the sidebar, which is 260px wide.
    cx.simulate_click(gpui::point(px(100.), px(400.)), gpui::Modifiers::none());
    assert_eq!(
        matches(&window, cx),
        None,
        "the click was not in the panel, so the panel goes"
    );
}

/// A click inside the panel is not a way out, and not a choice either: the panel
/// stops the click before the window's dismiss handler, so clicking its own label
/// leaves the list exactly where it was.
#[gpui::test]
fn clicking_inside_the_picker_keeps_it(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes("space f");
    assert_eq!(matches(&window, cx), Some(2));

    // The panel starts 8px from the top of the note column — which sits below the
    // 28px tab bar — and its label is the first thing in it: inside the panel, on
    // nothing that answers a click.
    cx.simulate_click(gpui::point(px(500.), px(44.)), gpui::Modifiers::none());
    assert_eq!(
        matches(&window, cx),
        Some(2),
        "a click inside the panel is still inside it"
    );
    assert_eq!(
        choice(&window, cx).as_deref(),
        Some("Welcome.md"),
        "and it chose nothing"
    );
}

/// vim's command line floats at the bottom of the note, and the same rule holds:
/// a click elsewhere closes it.
#[gpui::test]
fn clicking_off_the_command_line_closes_it(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    cx.simulate_keystrokes(":");
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.prompt_text(cx))
            .unwrap()
            .is_some(),
        "the command line is up"
    );

    // Above the line, in the note.
    cx.simulate_click(gpui::point(px(500.), px(300.)), gpui::Modifiers::none());
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.prompt_text(cx))
            .unwrap(),
        None,
        "and a click outside it closes it"
    );
}

/// The sidebar's prompts are the exception: they are part of the sidebar, not a
/// panel over the note, and a name half typed is work the user has not finished.
#[gpui::test]
fn clicking_the_note_keeps_a_sidebar_prompt(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    paint(cx);

    // The rename acts on the note that is open, so open one first.
    cx.simulate_keystrokes("space f enter ctrl-shift-r");
    cx.simulate_input(" x");
    let typed = window
        .update(cx, |ws, _window, cx| ws.prompt_text(cx))
        .unwrap();
    assert!(
        typed.as_deref().is_some_and(|text| text.ends_with(" x")),
        "the rename prompt is up with what was typed into it, got {typed:?}"
    );

    cx.simulate_click(gpui::point(px(500.), px(300.)), gpui::Modifiers::none());
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.prompt_text(cx))
            .unwrap(),
        typed,
        "and clicking the note did not throw it away"
    );
}

#[gpui::test]
fn typing_narrows_the_list_to_the_best_match(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space f");
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

    cx.simulate_keystrokes("space f");
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

    cx.simulate_keystrokes("space f down");
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

    cx.simulate_keystrokes("space f");
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

    cx.simulate_keystrokes("space f");
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

    cx.simulate_keystrokes("space f escape");
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
    Command::all().count()
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

/// Both vim chords reach quick-open. The vim engine owns them, because `space`
/// is also vim's right motion and gpui would have to hold it back if the keymap
/// treated it as a prefix; `g f` is the second way in now that `ctrl-o` belongs
/// to the jump chain.
#[gpui::test]
fn the_vim_chords_open_quick_open(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md", "notes/Another Note.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space f");
    assert_eq!(matches(&window, cx), Some(2));
    assert_eq!(choice(&window, cx).as_deref(), Some("Welcome.md"));

    cx.simulate_keystrokes("escape");
    assert_eq!(matches(&window, cx), None, "`escape` closed it again");

    cx.simulate_keystrokes("g f");
    assert_eq!(matches(&window, cx), Some(2), "`g f` opens the same picker");
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

    // The bare `l` opens the folder, which a fresh vault starts with closed.
    cx.simulate_keystrokes("ctrl-w h l j");
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

// --- search operators (P4.14) -------------------------------------------------

/// Notes that differ in exactly the ways an operator can ask about: a tag written
/// two ways, a property that is a number in one note and prose in another, and a
/// link from one to the other. The index is on, because an operator is a question
/// about the index.
fn query_vault(cx: &mut TestAppContext) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    indexed_vault(
        cx,
        &[
            (
                "notes/goblin.md",
                "---\ncr: 5\ntags: [monster, goblinoid]\n---\n# Goblin\n\nDigs in a [[lair]].\n",
            ),
            (
                "notes/snare.md",
                "---\ncr: low\ntags: monster\n---\n# Snare\n\nA [[lair]] snare.\n",
            ),
            (
                "notes/lair.md",
                "---\ncr: 8\ntags: [location]\n---\n# Lair\n\nWhere goblins dig.\n",
            ),
        ],
    )
}

/// Every row the picker is offering, in order. A count is a weak oracle: two sets
/// of the same size pass a count and disagree on which notes they named.
fn rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap()
}

/// The status line, where a refusal goes once the picker is gone.
fn status_line(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// An operator-only query asks about notes, so it answers with one row per note
/// rather than one row per line.
#[gpui::test]
fn an_operator_query_answers_with_notes_not_lines(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("tag:monster");
    assert_eq!(
        rows(&window, cx),
        vec!["notes/goblin.md", "notes/snare.md", "save tag:monster"],
        "a frontmatter list and a bare scalar are the same tag, and the last row is the \
         offer to keep the question (P4.3 M6)"
    );

    cx.simulate_keystrokes("enter");
    let (_, active) = state(&window, cx);
    assert_eq!(
        active, "notes/goblin.md",
        "choosing a row opens the note it names"
    );
}

/// The operators choose *which notes*; the text still chooses the line.
#[gpui::test]
fn an_operator_narrows_the_lines_that_get_searched(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("tag:location dig");
    assert_eq!(
        rows(&window, cx),
        vec!["notes/lair.md:7"],
        "the goblin also digs, and is not a location"
    );
}

/// The same vault asked with text alone: both notes dig, so the operator in the
/// test above narrowed *which notes*, and nothing else about how text is matched.
#[gpui::test]
fn text_alone_reaches_every_note(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("dig");
    assert_eq!(
        rows(&window, cx),
        vec!["notes/goblin.md:7", "notes/lair.md:7"],
        "one row per line, in the order the notes were read"
    );
}

/// The rule the `kind` column exists for, seen from the search box: a numeric
/// comparison must not rank the note that wrote `cr: low`, because SQLite reads
/// `CAST('low' AS REAL)` as 0 and says nothing about it.
#[gpui::test]
fn a_number_question_does_not_answer_a_note_that_wrote_prose(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("prop:cr<=5");
    assert_eq!(
        rows(&window, cx),
        vec!["notes/goblin.md", "save prop:cr<=5"]
    );
}

/// The prose row is not invisible; it answers a different question. Splitting the
/// two asks into their own tests rather than editing one query in place keeps the
/// keystrokes out of the way of what is being proved.
#[gpui::test]
fn a_text_question_answers_the_note_that_wrote_prose(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("prop:cr=low");
    assert_eq!(
        rows(&window, cx),
        vec!["notes/snare.md", "save prop:cr=low"]
    );
}

/// A near-miss operator is said out loud. Searched as prose it would answer "0 of
/// 0 matching", which reads as an empty vault rather than a typo.
#[gpui::test]
fn a_misspelled_operator_says_so_instead_of_answering_zero(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("tags:monster");
    let offered = rows(&window, cx);
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert!(
        offered[0].starts_with("not an operator: tags:monster"),
        "{offered:?}"
    );

    cx.simulate_keystrokes("enter");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "", "the refusal row opens nothing");
    assert!(
        status_line(&window, cx).starts_with("not an operator"),
        "and the reason outlives the picker: {}",
        status_line(&window, cx)
    );
}

/// With the index off an operator has no answer. Returning nothing would lie the
/// same way an empty vault does, so the picker says which half is missing.
#[gpui::test]
fn an_operator_without_an_index_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = needle_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("tag:monster");
    let offered = rows(&window, cx);
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert!(offered[0].contains("vault index"), "{offered:?}");

    // Plain text is unaffected — `ctrl_shift_f_finds_the_lines_that_hold_the_query`
    // already proves that on this same index-less vault. What changed is only that a
    // question the index cannot answer now says so instead of answering zero.
}

// --- what a row shows about why it is there ----------------------------------

/// A row that answers an operator names a note. Without the note's own words beside
/// it there is no way to tell *why* — `tag:monster` and `tag:goblinoid` would look
/// identical. So the row quotes the line that makes the operator true, and forwards
/// the part the query named.
fn shown(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
) -> Vec<crate::picker::ShownRow> {
    window
        .update(cx, |ws, _window, cx| ws.picker_shown(cx))
        .unwrap()
}

/// The detail rebuilt from the runs the row paints, with `[` `]` around the run
/// drawn in the match colour. These are the painted runs, not a parallel field: the
/// row and this helper call the same function.
fn marked(detail: &str, runs: &[(usize, bool)]) -> String {
    let mut out = String::new();
    let mut at = 0;
    for &(len, forward) in runs {
        let chunk = &detail[at..at + len];
        assert!(!chunk.is_empty(), "a zero-length run paints nothing");
        if forward {
            out.push('[');
            out.push_str(chunk);
            out.push(']');
        } else {
            out.push_str(chunk);
        }
        at += len;
    }
    assert_eq!(
        at,
        detail.len(),
        "the runs must cover the detail: {detail:?}"
    );
    out
}

fn ask(query: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input(query);
}

#[gpui::test]
fn a_tag_row_quotes_the_line_that_holds_the_tag(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("tag:monster", cx);

    let rows = shown(&window, cx);
    let goblin = rows.iter().find(|(label, _, _)| label == "goblin").unwrap();
    assert_eq!(
        marked(&goblin.1, &goblin.2),
        "tags: [[monster], goblinoid] · notes",
        "the frontmatter line the note wrote, the tag forward, the folder last"
    );
}

#[gpui::test]
fn a_prop_row_forwards_the_value_the_note_wrote(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("prop:cr<=5", cx);

    let rows = shown(&window, cx);
    assert_eq!(
        rows.len(),
        2,
        "snare's cr is prose, not a number; the other row is the offer to keep the search"
    );
    assert_eq!(marked(&rows[0].1, &rows[0].2), "cr: [5] · notes");
}

#[gpui::test]
fn a_link_row_quotes_the_line_holding_the_link(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("link:lair", cx);

    let rows = shown(&window, cx);
    let goblin = rows.iter().find(|(label, _, _)| label == "goblin").unwrap();
    assert_eq!(
        marked(&goblin.1, &goblin.2),
        "Digs in a [[[lair]]]. · notes",
        "the line as written, brackets and full stop and all, with the target forward"
    );
}

/// Two operators, two fragments, in the order the query asked. The row explains the
/// **and**, which is the question a GM is actually asking.
#[gpui::test]
fn two_operators_answer_with_two_fragments(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("tag:monster link:lair", cx);

    let rows = shown(&window, cx);
    let goblin = rows.iter().find(|(label, _, _)| label == "goblin").unwrap();
    assert_eq!(
        marked(&goblin.1, &goblin.2),
        "tags: [[monster], goblinoid] · Digs in a [[[lair]]]. · notes"
    );
}

/// `path:` and `modified:` answer with facts about the file rather than lines of
/// the note, and the row says the fact anyway: a row with nothing after the name is
/// a row the user cannot check.
#[gpui::test]
fn a_path_row_and_a_modified_row_still_say_something(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("path:notes/*.md", cx);
    let rows = shown(&window, cx);
    assert_eq!(rows.len(), 3);
    for (_, detail, _) in &rows {
        assert_eq!(detail, "notes", "the folder is the answer to `path:`");
    }

    cx.simulate_keystrokes("escape");
    ask("modified:2020-01-01", cx);
    // The offer to keep the search (P4.3 M6) is a row about the question rather than an
    // answer about a note, so it is not this loop's business; `tests/queries.rs` tests it.
    let answers: Vec<_> = shown(&window, cx)
        .into_iter()
        .filter(|(label, ..)| !label.starts_with("save "))
        .collect();
    for (label, detail, runs) in &answers {
        assert!(
            detail.starts_with("modified ") && detail.ends_with(" · notes"),
            "{label}: {detail}"
        );
        assert!(
            runs.iter().all(|&(_, forward)| !forward),
            "a date has nothing in the note to forward: {detail:?}"
        );
    }
}

/// The index can be ahead of the file. When the note no longer says what the index
/// claims, the row shows no evidence rather than a line the note does not hold —
/// the same honesty rule as the kind census.
#[gpui::test]
fn a_stale_index_shows_no_evidence_rather_than_a_wrong_one(cx: &mut TestAppContext) {
    let (dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    std::fs::write(
        dir.path().join("notes/goblin.md"),
        "---\ncr: 5\n---\n# Goblin\n\nNo tags any more.\n",
    )
    .unwrap();

    // The index still claims goblin and the note no longer says it. The row keeps
    // its place and quotes nothing — a line the note does not hold would be a lie
    // with a highlight on it. (When the watcher catches up the claim goes away
    // altogether, which the index tests already pin down; the debounce here is real
    // time, and parking the executor does not spend it.)
    ask("tag:monster", cx);
    let rows = shown(&window, cx);
    let goblin = rows.iter().find(|(label, _, _)| label == "goblin").unwrap();
    assert_eq!(
        goblin.1, "notes",
        "the folder is all the row can stand behind"
    );
    assert!(
        goblin.2.iter().all(|&(_, forward)| !forward),
        "nothing on this row is brought forward: {:?}",
        goblin.2
    );

    // The note that still says it is unaffected.
    let snare = rows.iter().find(|(label, _, _)| label == "snare").unwrap();
    assert_eq!(marked(&snare.1, &snare.2), "tags: [monster] · notes");
}

/// The runs a row paints cover its detail exactly, and every run it brings forward
/// holds text. A clipped fragment can leave a range past the end of what is shown;
/// the row drops that range rather than painting a highlight onto nothing, and this
/// checks the invariant over every kind of answer at once.
#[gpui::test]
fn the_painted_runs_cover_the_detail_exactly(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    for query in [
        "tag:monster",
        "prop:cr<=5",
        "link:lair",
        "tag:monster link:lair",
        "path:notes/*.md",
    ] {
        ask(query, cx);
        for (label, detail, runs) in shown(&window, cx) {
            // `marked` is the assertion: it walks the runs across the detail and
            // fails if they leave a byte uncovered or forward an empty stretch.
            marked(&detail, &runs);
            let _ = label;
            cx.simulate_keystrokes("escape");
        }
    }
}

/// The crash this feature shipped with: a 78-character line quoted at 60 characters
/// had its forwarded range landing inside the `…` the clip appended, and gpui reads
/// a run that ends mid-character as an `invalid text run` panic. Reproduced here on
/// the demo vault's own notes, because that is how the user found it: `space s`,
/// then `link:Welcome`, and the window died on the last letter.
#[gpui::test]
fn a_long_line_cannot_panic_on_its_own_ellipsis(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            (
                "notes/Another Note.md",
                "---\nalias: anon\ncr: 3\n---\nBack to [[Welcome]].\n",
            ),
            (
                "Welcome.md",
                "# Welcome\n\nThis is [[Another Note]].\n",
            ),
            (
                "Markdown Tour.md",
                "# Markdown Tour\n\nLive-preview styling, one construct per line. Links: [[Welcome]], [[Another Note]],\n[[alias|an aliased link]] and [[A Note That Does Not Exist]].\n",
            ),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    // Typed a character at a time: the row only exists once the operator is whole,
    // which is the moment the panic arrived.
    cx.simulate_keystrokes("ctrl-shift-f");
    for ch in "link:Welcome".chars() {
        cx.simulate_input(&ch.to_string());
    }
    let rows = shown(&window, cx);
    let tour = rows
        .iter()
        .find(|(label, _, _)| label == "Markdown Tour")
        .unwrap_or_else(|| panic!("no row for the long line: {rows:?}"));
    assert!(
        tour.1.contains("[[Welcome]]"),
        "the match is the point of the row: {:?}",
        tour.1
    );
    // `marked` slices the detail at every run boundary, so a boundary landing inside
    // a character would panic here exactly as it did in the render.
    marked(&tour.1, &tour.2);
}

/// The head of a long line is not the interesting part. A line longer than the row's
/// budget is cut **around the match**, with an ellipsis saying what was left out.
#[gpui::test]
fn a_long_line_is_cut_around_the_match_not_at_its_start(cx: &mut TestAppContext) {
    let filler = "and then a great deal of prose that nobody is looking for, ".repeat(3);
    let (_dir, window) = indexed_vault(
        cx,
        &[("deep.md", &format!("{filler}buried deep in it: [[lair]]\n"))],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("link:lair", cx);

    let rows = shown(&window, cx);
    let (label, detail, runs) = &rows[0];
    assert_eq!(label, "deep");
    assert!(
        detail.contains("[[lair]]"),
        "the key evidence survived the cut: {detail:?}"
    );
    assert!(detail.starts_with('…'), "the cut is marked: {detail:?}");
    assert!(
        detail.chars().count() <= 90 + 2,
        "and the row stays a row: {} chars",
        detail.chars().count()
    );
    // The window runs to the end of the line, because the match is there: centring
    // can only pull the window back from the head, not past the tail.
    let marked = marked(detail, runs);
    assert!(
        marked.ends_with("buried deep in it: [[[lair]]]"),
        "the match sits at the end of what is shown: {marked}"
    );
    assert!(marked.starts_with('…'), "the cut is marked: {marked}");
}

/// Two operators, two quotes, and **both** keys visible: the budget is split rather
/// than spent by whichever operator the query happened to name first.
#[gpui::test]
fn two_operators_both_keep_their_key_visible(cx: &mut TestAppContext) {
    let filler = "prose that goes on long enough to matter, ".repeat(4);
    let (_dir, window) = indexed_vault(
        cx,
        &[(
            "long.md",
            &format!("---\ncr: {}\n---\n{filler}and the tag #monster lives out here\n\n{filler}and a link to [[lair]] lives further out\n", 9),
        )],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("tag:monster link:lair", cx);

    let rows = shown(&window, cx);
    let (_, detail, runs) = &rows[0];
    assert!(detail.contains("#monster"), "first key: {detail:?}");
    assert!(detail.contains("[[lair]]"), "second key: {detail:?}");
    marked(detail, runs);
    assert_eq!(
        runs.iter().filter(|&&(_, forward)| forward).count(),
        2,
        "both forwarded, not just the first: {runs:?}"
    );
}

/// The shape that actually crashed, at the size that made it crash: a match
/// straddling where the cut falls. The old code measured the forwarded range against
/// the clipped string *including* the `…` it had appended, so the run ended inside
/// the ellipsis — and gpui reads a run boundary that is not a character boundary as
/// an `invalid text run` panic, in the render, with no way back.
#[gpui::test]
fn a_match_straddling_the_cut_cannot_end_a_run_mid_character(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        // 84 filler characters, then a link whose *target* runs two bytes past the
        // ninetieth character — the shape that put a run boundary inside the `…`.
        &[("straddle.md", &format!("{}[[lairx]]\n", "x".repeat(84)))],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("link:lairx", cx);

    let rows = shown(&window, cx);
    let (_, detail, runs) = &rows[0];
    // Slicing the detail at every run boundary comes first: a boundary inside a
    // character panics here exactly as it did in the render.
    let marked = marked(detail, runs);
    assert!(
        marked.contains("[[[lairx]]]"),
        "the key survived the cut: {marked}"
    );
}

/// A cut that lands inside a multi-byte character is the same panic in another
/// costume: the runs are byte lengths, so the boundary has to be a character
/// boundary even when the line is full of em dashes and accents.
#[gpui::test]
fn a_cut_never_lands_inside_a_character(cx: &mut TestAppContext) {
    let filler = "café — naïve — résumé — “quoted” — ".repeat(6);
    let (_dir, window) = indexed_vault(cx, &[("unicode.md", &format!("{filler}[[lair]]\n"))]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    ask("link:lair", cx);

    let rows = shown(&window, cx);
    let (_, detail, runs) = &rows[0];
    assert!(detail.contains("[[lair]]"), "{detail:?}");
    // Slicing at every boundary is the assertion.
    marked(detail, runs);
}

// ----- selection and the clipboard in a field (P4.x six-item round) -----

/// What the picker's field holds. The rows are a weak oracle for this: two different
/// queries can answer with the same rows.
fn field(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.picker_field(cx))
        .unwrap()
        .unwrap_or_default()
}

/// `shift-left` extends a selection from the caret, and typing replaces it. This is
/// the whole point of a selection in a search box: fix the end of a wrong query
/// without retyping it.
#[gpui::test]
fn shift_arrows_select_and_typing_replaces_them(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("goblin");
    assert_eq!(field(&window, cx), "goblin");

    cx.simulate_keystrokes("shift-left shift-left");
    cx.simulate_input("x");
    assert_eq!(
        field(&window, cx),
        "goblx",
        "the two selected characters went away and `x` arrived in their place"
    );
}

/// `ctrl-a` is the whole line, so a search can be replaced in two keys. This is a
/// deliberate departure from a shell's line editor, where `ctrl-a` is start-of-line;
/// `home` is that here, and it is bound too.
#[gpui::test]
fn ctrl_a_takes_the_whole_line(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("tag:monster");

    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("z");
    assert_eq!(field(&window, cx), "z", "the selection went under one key");
}

/// The clipboard is the system one, so a query can leave Shale and come back. Cut
/// empties the field; paste puts the same text back over whatever was selected.
#[gpui::test]
fn cut_and_paste_move_the_query_through_the_clipboard(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("tag:monster");

    cx.simulate_keystrokes("ctrl-a ctrl-x");
    assert_eq!(field(&window, cx), "", "cut took the line out of the field");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("tag:monster".into()),
        "and put it where another program can see it"
    );

    // A paste onto a selection replaces it, which is what a paste means.
    cx.simulate_input("link:100%");
    cx.simulate_keystrokes("ctrl-a ctrl-v");
    assert_eq!(
        field(&window, cx),
        "tag:monster",
        "the paste replaced the selected line rather than joining it"
    );
}

/// A stray arrow with a selection out leaves the selection at the end it was heading
/// for and does not step. Stepping *and* clearing would be two changes for one key.
#[gpui::test]
fn a_plain_arrow_leaves_the_selection_where_it_was(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("goblin");
    cx.simulate_keystrokes("shift-left shift-left left");
    assert_eq!(field(&window, cx), "goblin", "the arrow moved nothing");

    // The caret is left at the selection's left edge, so the next `backspace` eats
    // the character in front of that edge rather than the selection: the arrow's
    // whole job was to put the caret where the selection started.
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        field(&window, cx),
        "gobin",
        "`l` went, which is the character before the edge the selection started at"
    );
}

/// `home` and `end` are the ends of the line, which is what makes `shift-up` and
/// `shift-down` the same two requests rather than a motion onto a line that does
/// not exist.
#[gpui::test]
fn home_and_end_move_the_caret_to_the_ends(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("monster");

    cx.simulate_keystrokes("home");
    cx.simulate_input("g");
    assert_eq!(field(&window, cx), "gmonster", "`home` went to the start");

    cx.simulate_keystrokes("end shift-up");
    cx.simulate_input("x");
    assert_eq!(
        field(&window, cx),
        "x",
        "`shift-up` from the end of the line selected all of it, and typing replaced it"
    );
}

/// `ctrl`+`shift`+arrow selects a word at a time, which is how a search gets edited:
/// one key reaches the operator, not the six characters in front of it. `ctrl`+arrow
/// alone moves the caret by a word and selects nothing.
#[gpui::test]
fn ctrl_shift_arrows_select_whole_words(cx: &mut TestAppContext) {
    let (_dir, window) = query_vault(cx);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("ctrl-shift-f");
    cx.simulate_input("tag:monster");

    cx.simulate_keystrokes("ctrl-shift-left");
    cx.simulate_input("goblin");
    assert_eq!(
        field(&window, cx),
        "tag:goblin",
        "one key took `monster` and the typing replaced it"
    );

    // The caret motion beside it: a word at a time, selecting nothing.
    cx.simulate_keystrokes("ctrl-left");
    cx.simulate_input("x");
    assert_eq!(
        field(&window, cx),
        "tag:xgoblin",
        "`ctrl-left` stepped to the start of `goblin` without taking a selection"
    );
}
