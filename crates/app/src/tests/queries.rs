//! The door to a saved query (roadmap P4.3, M2): `:query`, the rows an answer draws, and
//! every way an answer can fail. The core's own rules (one statement, the shape check,
//! the binding rules) are tested in `shale-core`; what is tested here is the surface's
//! half — that a refusal is a row rather than a silence, that a row naming a note is a
//! door, and that a query note is a file the author can see.

use super::*;

/// The rows the picker is offering, in order.
fn offered(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap()
}

/// Put a note in hand, so a surface that reads the caret's note has one.
fn open_note(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, name: &str) {
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&std::path::PathBuf::from(name), cx)
        })
        .unwrap();
}

/// A vault with two monsters, their `cr` indexed, and one saved query over them.
fn monsters(cx: &mut TestAppContext, query: &str) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n# Goblin\n"),
            ("Wisp.md", "---\ncr: 3\n---\n"),
            ("queries/tough.shq", query),
        ],
    )
}

const TOUGH: &str =
    "+++\n+++\nSELECT note, value FROM props WHERE key = 'cr' ORDER BY CAST(value AS REAL)\n";

/// The door: `:query` lists the vault's `.shq` files, and choosing one answers in the
/// same surface — the columns once, then one row per answer.
#[gpui::test]
fn a_saved_query_answers_with_rows(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("query", cx);
    assert_eq!(
        offered(&window, cx),
        vec!["query queries/tough.shq"],
        "the picker lists saved queries by the file they live in"
    );

    cx.simulate_keystrokes("enter");
    assert_eq!(
        offered(&window, cx),
        vec!["queries/tough.shq", "Wisp.md", "Goblin.md"],
        "the answer, in the order the query asked for it: cr 3 before cr 5"
    );
    let shown = window
        .update(cx, |ws, _window, cx| ws.picker_shown(cx))
        .unwrap();
    assert_eq!(
        shown[0].0, "columns: note · value",
        "the first row still says what the columns are: {shown:?}"
    );
    assert_eq!(
        shown[0].1, "open queries/tough.shq",
        "and says where choosing it goes"
    );
}

/// The row that asks for a query is a link like any other: the window's name set holds
/// the saved query, so the row is coloured as resolved, and `gd` follows it into the
/// `.shq` (ruling 30's condition — the colour, the click and the index move together).
#[gpui::test]
fn a_query_embed_is_a_link_the_window_can_follow(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, TOUGH, "![[tough]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    assert!(
        window
            .update(cx, |ws, _, _| ws.knows_link("tough"))
            .unwrap(),
        "the saved query is in the set the editor styles links with"
    );
    assert!(
        window
            .update(cx, |ws, _, _| ws.knows_link("queries/tough"))
            .unwrap(),
        "and so is the path a reader would type with the extension left off"
    );

    // Onto the embed's own text, then vim's follow.
    cx.simulate_keystrokes("0 l l");
    cx.simulate_keystrokes("g d");
    cx.run_until_parked();
    assert_eq!(
        window.update(cx, |ws, _, _| ws.active_path()).unwrap(),
        Some(std::path::PathBuf::from("queries/tough.shq")),
        "`gd` on `![[tough]]` opened the file that computes the answer"
    );
}

/// The answer's title row is a door too: a reader who does not like an answer goes to
/// the file that computed it — the same rule the query picker's first row follows.
#[gpui::test]
fn an_embedded_answers_title_row_opens_the_query(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, TOUGH, "![[tough]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    let bands = window
        .update(cx, |ws, _window, cx| ws.phantom_bands(cx))
        .unwrap();
    // The title row hangs directly under the row that asked: the first door there is.
    let title = bands.iter().find(|(opens, ..)| *opens).expect("a door row");
    let at = gpui::point(gpui::px(title.1 + 5.), gpui::px((title.2 + title.3) / 2.));
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        window.update(cx, |ws, _, _| ws.active_path()).unwrap(),
        Some(std::path::PathBuf::from("queries/tough.shq")),
        "clicking the answer's title opened the query"
    );
}

/// A query note with no declaration at all is still a query: the fence is optional, and
/// so is the ceremony.
#[gpui::test]
fn the_columns_row_of_an_answer_opens_the_query_file(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    let shown = window
        .update(cx, |ws, _window, cx| ws.picker_shown(cx))
        .unwrap();
    assert_eq!(
        shown[0].0, "columns: note · value",
        "the answer's first row: {shown:?}"
    );

    // The picker opens on its first row, so one more enter chooses the row that
    // names the columns — and that row is the way back to the file that computed
    // them, which is where a reader goes when the answer is not what they wanted.
    cx.simulate_keystrokes("enter");
    let (_, active) = state(&window, cx);
    assert_eq!(
        active, "queries/tough.shq",
        "the columns row should open the query file"
    );
}

#[gpui::test]
fn a_query_note_with_no_declaration_runs_its_body(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(
        cx,
        "SELECT note FROM props WHERE key = 'cr' ORDER BY note\n",
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        offered(&window, cx),
        vec!["queries/tough.shq", "Goblin.md", "Wisp.md"]
    );
}

/// The declaration's `sort` is the reader's override, applied to the answer rather than
/// spliced into the statement — so a query written in one order can be read in the other
/// without editing the file.
#[gpui::test]
fn the_declaration_sorts_the_answer(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(
        cx,
        "+++\nsort = \"-value\"\n+++\nSELECT note, value FROM props WHERE key = 'cr' ORDER BY CAST(value AS REAL)\n",
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        offered(&window, cx),
        vec!["queries/tough.shq", "Goblin.md", "Wisp.md"],
        "the declaration's `-value` outranks the statement's ascending ORDER BY"
    );
}

/// A row whose first cell names a note in this vault *is* that note: a saved query is a
/// door, not a printout.
#[gpui::test]
fn a_row_that_names_a_note_opens_it(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    // Down past the columns row onto the first answer, then in.
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |ws, _window, _cx| ws.active_path())
            .unwrap(),
        Some(std::path::PathBuf::from("Wisp.md")),
        "choosing a row that names a note goes to that note"
    );
}

/// EDGE-16, narrowly: a first cell that looks like a note and names none is left as text.
/// It is not dimmed away and not made to look openable — the lens ruling wants a visible
/// plot hole — and choosing it opens nothing.
#[gpui::test]
fn a_row_naming_a_note_that_is_not_there_opens_nothing(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, "SELECT 'Ghost.md' AS note\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(offered(&window, cx), vec!["queries/tough.shq", "Ghost.md"]);
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |ws, _window, _cx| ws.active_path())
            .unwrap(),
        None,
        "a row that names no note must not pretend to open one"
    );
    // What the row is instead: a thing the reader can read out, like any row whose
    // job is to be read. Without this, "opens nothing" and "is a row" look the same.
    assert_eq!(
        window.update(cx, |ws, _window, _cx| ws.status()).unwrap(),
        "Ghost.md",
        "choosing the row says the row, rather than opening nothing"
    );
}

/// EDGE-1: an answer of nothing is still an answer, and it is not the same sentence as a
/// failure to look.
#[gpui::test]
fn a_query_that_answers_nothing_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, "SELECT note FROM props WHERE key = 'no such key'\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(offered(&window, cx), vec!["no note answers this"]);
}

/// EDGE-2: SQL the database refuses comes back as the database's own words, attached to
/// the query that asked.
#[gpui::test]
fn a_query_that_fails_says_why(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, "SELECT FROM WHERE\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    let rows = offered(&window, cx);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].starts_with("the query could not be answered"),
        "{rows:?}"
    );
}

/// EDGE-3 through the surface: a body that is not a question never reaches the database,
/// and the reason names what was wrong with it.
#[gpui::test]
fn a_body_that_is_not_a_question_is_refused(cx: &mut TestAppContext) {
    let (dir, window) = monsters(cx, "DELETE FROM props\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    let rows = offered(&window, cx);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].contains("asks a question"), "{rows:?}");
    // And the vault is untouched, which is the point of refusing before running.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Goblin.md")).unwrap(),
        "---\ncr: 5\n---\n# Goblin\n"
    );
}

/// A file that is not a query note at all — a declaration that opens and never closes —
/// is refused as the file it is, not reported as an empty answer.
#[gpui::test]
fn a_file_that_is_not_a_query_note_is_refused(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, "+++\nsort = \"value\"\nSELECT note FROM notes\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    let rows = offered(&window, cx);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].contains("not a query note"), "{rows:?}");
}

/// EDGE-14: with the index off there is nothing to answer from. "No note answers this"
/// would be a claim about the vault; the honest answer is about the setting.
#[gpui::test]
fn no_index_is_reported_as_no_index(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n"),
            ("queries/tough.shq", TOUGH),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        offered(&window, cx),
        vec!["there is no vault index to read (index.enabled = false)"]
    );
}

/// EDGE-8 at the surface: a NULL and an empty string are drawn apart, because a table
/// that blurs them has thrown away the one thing the runner kept.
#[gpui::test]
fn a_null_and_an_empty_string_are_drawn_apart(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, "SELECT NULL AS missing, '' AS blank\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    let shown = window
        .update(cx, |ws, _window, cx| ws.picker_shown(cx))
        .unwrap();
    let row = shown
        .iter()
        .find(|row| row.0 == "∅")
        .unwrap_or_else(|| panic!("no row drawn for a NULL: {shown:?}"));
    assert_eq!(row.1, "''", "an empty string must not be drawn as nothing");
}

/// EDGE-9: the core reads whatever the query answers; the surface cuts what it draws and
/// says how much it cut. The cut and the admission are one decision, so they cannot
/// disagree about how many rows there were.
#[gpui::test]
fn a_long_answer_is_cut_and_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(
        cx,
        "WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 250) SELECT n FROM c\n",
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("query", cx);
    cx.simulate_keystrokes("enter");
    let rows = offered(&window, cx);
    assert_eq!(rows.len(), 202, "columns + 200 rows + the admission");
    assert_eq!(rows[rows.len() - 1], "…50 more rows not shown");
}

/// The ruling on citizenship: a `.shq` is a file the author owns, so the tree lists it
/// and the sidebar's `enter` opens it in the editor; quick-open stays a note list, so
/// `space f` keeps answering one question instead of two.
#[gpui::test]
fn a_query_note_is_a_tree_row_and_not_a_quick_open_row(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    open_note(&window, cx, "Goblin.md");
    let (tree, _) = state(&window, cx);
    assert!(
        tree.contains(&std::path::PathBuf::from("queries")),
        "the folder holding the saved queries is a tree row: {tree:?}"
    );

    // Folders start closed, so the row under it is reached the way a reader reaches
    // it: hand the sidebar the keyboard, step up to the folder, open it.
    cx.simulate_keystrokes("ctrl-w h k l");
    let (tree, _) = state(&window, cx);
    assert!(
        tree.contains(&std::path::PathBuf::from("queries/tough.shq")),
        "a saved query is a file in the vault, so it is a row in the tree: {tree:?}"
    );

    // …and the row under it opens in the editor, because the author edits these
    // files. The selection is still on the folder, so one step down is the query.
    cx.simulate_keystrokes("j enter");
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |ws, _window, _cx| ws.active_path())
            .unwrap(),
        Some(std::path::PathBuf::from("queries/tough.shq")),
        "a query note opens in the editor like any file the author owns"
    );

    // Quick-open stays a note list, so `space f` keeps answering one question.
    cx.simulate_keystrokes("space f");
    cx.simulate_input("tough");
    let rows = offered(&window, cx);
    assert!(
        !rows.iter().any(|row| row.contains("tough")),
        "quick-open offered a query note: {rows:?}"
    );
}

// ----- M3: the program folded, the answer where the reader is -----

/// A bare editor showing `text` as the file `path`, focused, with an empty vault's
/// index behind it. The path is what makes the view ask whether this is a query note;
/// the index is what lets it answer, so what a fold test measures is the fold and not
/// a refusal. The `TempDir` has to outlive the window, so the caller keeps it.
fn query_editor(
    cx: &mut TestAppContext,
    path: &str,
    text: &str,
) -> (WindowHandle<MarkdownEditor>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a vault to hold an index");
    let db = std::sync::Arc::new(
        shale_core::db::Db::open(dir.path()).expect("an index the query can read"),
    );
    let window = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    window
        .update(cx, |editor, window, cx| {
            editor.set_text(text.to_string(), cx);
            editor.set_embed_context(Some(Path::new(path)), Vec::new(), Vec::new(), cx);
            editor.set_query_db(Some(db), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    (window, dir)
}

/// The rows on screen and the fold markers painted on them.
fn shown(
    editor: &WindowHandle<MarkdownEditor>,
    cx: &mut VisualTestContext,
) -> (usize, Vec<String>) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    editor
        .read_with(cx, |editor, _cx| {
            (
                editor.display_line_count(),
                editor
                    .painted_fold_markers()
                    .into_iter()
                    .map(|(_line, label)| label)
                    .collect(),
            )
        })
        .unwrap()
}

/// A query note is a program with an answer attached, so it opens with the
/// program folded away — and the marker says what the file computes, not how
/// many rows are in the way.
#[gpui::test]
fn a_query_note_opens_with_its_program_folded(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(cx, "queries/tough.shq", TOUGH);
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    let (lines, markers) = shown(&editor, cx);
    assert_eq!(
        lines, 1,
        "the declaration and the SQL are one fold, and it starts shut"
    );
    assert_eq!(
        markers,
        vec![" tough · 0 rows"],
        "the marker names the query (the file's own stem, with no declaration to say otherwise)"
    );

    // The fold is display state: the file is untouched, which is the same promise
    // a heading's fold makes.
    let text = editor.read_with(cx, |editor, _cx| editor.text()).unwrap();
    assert_eq!(text, TOUGH, "a fold never touches the text");
}

/// A declaration's `name` is what the marker says, because that is the name the
/// reader knows the query by.
#[gpui::test]
fn the_declaration_names_the_fold(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(
        cx,
        "queries/ratings.shq",
        "+++\nname = \"Creature ratings\"\n+++\nSELECT 1\n",
    );
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    let (_lines, markers) = shown(&editor, cx);
    assert_eq!(
        markers,
        vec![" Creature ratings · 1 row"],
        "the declared name, not the file's"
    );
}

/// `space z` is the keyboard route. It has to exist because preview swallows
/// `za` with every other caret motion, and a query note whose fold starts shut
/// is no use to a reader who cannot open it without a mouse.
#[gpui::test]
fn space_z_opens_the_program_and_shuts_it_again(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(cx, "queries/tough.shq", TOUGH);
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    assert_eq!(shown(&editor, cx).0, 1);

    cx.simulate_keystrokes("space z");
    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 4, "the whole program is on screen");
    assert!(markers.is_empty(), "an open fold paints no marker");

    cx.simulate_keystrokes("space z");
    assert_eq!(shown(&editor, cx).0, 1, "and the same key shuts it again");
}

/// Preview swallows `za` along with every other caret motion — it scrolls, it
/// does not move — so the leader chord is the keyboard's way into a folded
/// program. Without it, a query note in preview could only be read shut.
#[gpui::test]
fn preview_opens_the_program_with_the_leader(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(cx, "queries/tough.shq", TOUGH);
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_view(crate::editor::View::Preview, cx)
        })
        .unwrap();
    assert_eq!(shown(&editor, cx).0, 1, "preview folds the program too");

    cx.simulate_keystrokes("z a");
    assert_eq!(
        shown(&editor, cx).0,
        1,
        "`za` is swallowed in a reading view"
    );

    cx.simulate_keystrokes("space z");
    assert_eq!(shown(&editor, cx).0, 4, "the leader chord is not");
}

/// Raw mode is the source. A fold whose job is to hide the program has no
/// business in the view whose whole job is to show it.
#[gpui::test]
fn raw_mode_shows_the_program(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(cx, "queries/tough.shq", TOUGH);
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    assert_eq!(shown(&editor, cx).0, 1);

    editor
        .update(cx, |editor, _window, cx| {
            editor.set_view(crate::editor::View::Raw, cx)
        })
        .unwrap();
    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 4, "raw shows every row of the file");
    assert!(markers.is_empty(), "and folds nothing");

    // Back to live, and the program is folded again: the mode is the question,
    // not a one-way door.
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_view(crate::editor::View::Live, cx)
        })
        .unwrap();
    assert_eq!(shown(&editor, cx).0, 1);
}

/// A file that does not parse as a query note has no fold at all. That is the
/// first half of *a refusal reveals its source*: nothing is hidden, so the
/// reader sees exactly what is wrong with it.
#[gpui::test]
fn a_file_that_is_not_a_query_shows_its_source(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(cx, "queries/bad.shq", "not a question at all\nnope\n");
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 3, "every row of a refusal is on screen");
    assert!(markers.is_empty(), "and nothing is folded away");
}

/// Raw mode is the source, and a `.shq` is not markdown: not even in the view
/// that shows the raw text does a `#`-looking line open a section.
#[gpui::test]
fn raw_mode_folds_nothing_in_a_query_note(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(
        cx,
        "queries/notes.shq",
        // A body that runs, and still has a row beginning with `#`: what is under test
        // is the `#`, not a statement the runner would refuse.
        "+++\n+++\nSELECT '\n# not a heading\n' AS x\n",
    );
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_view(crate::editor::View::Raw, cx)
        })
        .unwrap();
    assert_eq!(shown(&editor, cx).0, 6, "raw shows every row");

    cx.simulate_keystrokes("g g j j j");
    cx.simulate_keystrokes("z a");
    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 6, "and a fold command on a `#` row folds nothing");
    assert!(markers.is_empty(), "no marker anywhere");
}

/// `zM` closes every section a note has. A query note has one — its program — so
/// opening the program again afterwards leaves no section marker behind. If its
/// `#`-looking lines counted as sections, this is where the half-folded
/// statement would surface: the program open, a line of it shut.
#[gpui::test]
fn closing_every_section_leaves_no_fake_section_behind(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(
        cx,
        "queries/notes.shq",
        // A body that runs, and still has a row beginning with `#`: what is under test
        // is the `#`, not a statement the runner would refuse.
        "+++\n+++\nSELECT '\n# not a heading\n' AS x\n",
    );
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    cx.simulate_keystrokes("space z");
    assert_eq!(shown(&editor, cx).0, 6, "the program open");
    cx.simulate_keystrokes("z M");
    assert_eq!(shown(&editor, cx).0, 1, "zM closes the one region it has");
    cx.simulate_keystrokes("space z");
    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 6, "and opening it again shows the whole program");
    assert!(
        markers.is_empty(),
        "with no section marker left behind: {markers:?}"
    );
}

/// A body the runner would refuse — two statements in one note — is a refusal as
/// surely as a body that is not a question. The fold asks the runner's own
/// question, so the two doors can never disagree about what is worth folding.
#[gpui::test]
fn a_query_the_runner_would_refuse_shows_its_source(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(cx, "queries/two.shq", "+++\n+++\nSELECT 1; SELECT 2\n");
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    let (lines, markers) = shown(&editor, cx);
    assert_eq!(
        lines, 4,
        "the source of a refusal is the point of showing it"
    );
    assert!(markers.is_empty(), "nothing is folded away");
}

/// The same rule asked with the keyboard: a fold command on a `#`-looking row
/// inside a query note folds the program, because there is no section there to
/// fold. Half a statement left on screen with a marker of its own is the failure
/// this rules out.
#[gpui::test]
fn a_fold_command_folds_the_program_not_a_fake_heading(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(
        cx,
        "queries/notes.shq",
        // A body that runs, and still has a row beginning with `#`: what is under test
        // is the `#`, not a statement the runner would refuse.
        "+++\n+++\nSELECT '\n# not a heading\n' AS x\n",
    );
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    cx.simulate_keystrokes("space z");
    assert_eq!(shown(&editor, cx).0, 6, "the program is open");

    // Down to the row that would be a heading in a note, and `za`.
    cx.simulate_keystrokes("g g j j j");
    cx.simulate_keystrokes("z a");
    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 1, "the whole program folded, not one line of it");
    assert_eq!(
        markers,
        vec![" notes · 1 row"],
        "and the marker is the query's, with the answer's shape"
    );
}

/// A query note is not markdown: its `#`-looking lines are not sections. The one
/// region it has is its program, so a fold never leaves half a statement on
/// screen with a marker of its own.
#[gpui::test]
fn a_query_note_has_no_heading_sections(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (editor, _index) = query_editor(
        cx,
        "queries/notes.shq",
        // A body that runs, and still has a row beginning with `#`: what is under test
        // is the `#`, not a statement the runner would refuse.
        "+++\n+++\nSELECT '\n# not a heading\n' AS x\n",
    );
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 1, "one region, the whole file");
    assert_eq!(
        markers,
        vec![" notes · 1 row"],
        "one marker, the query's own, and no section marker for the `#` row"
    );
}

// ----- M3b: the answer, drawn where the program lives -----

/// The table the file draws under its own program.
fn table(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.query_shape(cx))
        .unwrap_or_default()
}

/// How many of the file's rows are on screen.
fn drawn(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| ws.display_lines(cx))
        .unwrap_or(0)
}

/// The fold marker the file painted, if any.
fn marker(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().map(|editor| {
                editor.read_with(cx, |editor, _cx| {
                    editor
                        .painted_fold_markers()
                        .into_iter()
                        .map(|(_line, label)| label)
                        .collect::<Vec<_>>()
                })
            })
        })
        .unwrap_or_default()
        .unwrap_or_default()
}

/// The file in hand, opened in the editor: the program folded, and its answer drawn
/// under it. This is the whole point of the milestone — the question and its answer
/// in one place, the program out of the way.
#[gpui::test]
fn the_query_note_draws_its_answer_under_the_program(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    assert_eq!(
        table(&window, cx),
        vec![" note       value", " Wisp.md    3", " Goblin.md  5",],
        "the columns once, then one row per answer, in the order the query asked"
    );
    assert_eq!(drawn(&window, cx), 1, "the program itself stays folded");
}

/// The marker row says what the file computed and how much of it there is, so a
/// folded file still answers a question about itself.
#[gpui::test]
fn the_marker_says_what_the_file_computed(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    assert_eq!(marker(&window, cx), vec![" tough · 2 rows"]);
}

/// A cell spelled as a vault path *is* that note — the same rule the picker's rows
/// follow, so the same answer cannot be a door in one surface and a dead end in the
/// other.
#[gpui::test]
fn a_table_row_that_names_a_note_opens_it(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let band = window
        .update(cx, |ws, _window, cx| ws.phantom_rows(cx))
        .unwrap()
        .into_iter()
        .find(|(what, ..)| what == "badge:Goblin.md")
        .expect("the row naming Goblin.md is painted and addressable");
    let (_, left, top, bottom) = band;
    cx.simulate_click(
        gpui::point(gpui::px(left + 4.0), gpui::px((top + bottom) / 2.0)),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap_or_default(),
        "---\ncr: 5\n---\n# Goblin\n",
        "the click opened the note the row names"
    );
}

/// An answer with no rows says so. A table that draws only its header would read as
/// a query that worked and a vault with nothing in it.
#[gpui::test]
fn an_empty_answer_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, "SELECT note FROM props WHERE key = 'no such key'\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    assert_eq!(table(&window, cx), vec!["the query answers nothing"]);
}

/// A refusal says why, in the runner's own words, **and opens the program**: a
/// folded region with no table is a blank page, and a blank page is a refusal the
/// reader cannot read.
#[gpui::test]
fn a_refusal_says_why_and_unfolds_the_program(cx: &mut TestAppContext) {
    // A statement the runner accepts the shape of and refuses at run time: the fold
    // exists, and the answer is what says it has to open.
    let (_dir, window) = monsters(cx, "+++\n+++\nSELECT nope FROM props\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let rows = table(&window, cx);
    assert_eq!(rows.len(), 1, "one line, the reason");
    assert!(
        rows[0].contains("no such column"),
        "the runner's own words: {rows:?}"
    );
    assert_eq!(
        drawn(&window, cx),
        4,
        "and the program is on screen, because the table is not"
    );
}

/// The cut is the same number the picker cuts at, and the row that admits it says how
/// many it left out. The runner is uncapped: what the query answers is what the query
/// answers — this is a surface running out of screen, not a door running out of rows.
#[gpui::test]
fn a_long_answer_in_the_note_is_cut_and_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(
        cx,
        "WITH RECURSIVE x(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM x WHERE n < 250) \
         SELECT n FROM x\n",
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let rows = table(&window, cx);
    assert_eq!(
        rows.len(),
        shale_core::db::MAX_ROWS + 2,
        "header, the rows it drew, and the line that admits the rest"
    );
    assert_eq!(
        rows.last().expect("the admission"),
        "…50 more rows not shown"
    );
}

/// The declaration's `sort` orders the table exactly as it orders the picker's rows,
/// because both call the same rule in the core.
#[gpui::test]
fn the_declaration_sorts_the_table(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(
        cx,
        // The statement asks for ascending; the declaration asks for descending. Only
        // one of them can be what the table shows.
        "+++\nsort = \"-value\"\n+++\nSELECT note, value FROM props WHERE key = 'cr' \
         ORDER BY CAST(value AS REAL)\n",
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    assert_eq!(
        table(&window, cx),
        vec![" note       value", " Goblin.md  5", " Wisp.md    3",],
        "descending on the declared column"
    );
}

/// A table row is not a line of the file, so the caret cannot land in one. The rows
/// that name no note are not addressable at all — they are captions — and a click
/// beside them belongs to the note, which is the program's row they happen to sit
/// under.
#[gpui::test]
fn clicking_a_row_that_names_no_note_does_not_put_the_caret_in_the_answer(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    // The folded program is one row on screen; the answer hangs below it.
    let (left, top, bottom) = window
        .update(cx, |ws, _window, cx| ws.line_band(0, cx))
        .unwrap()
        .expect("the marker row is painted");
    let height = bottom - top;
    // Half a row down is the table's header: the one row of the answer that names
    // nothing and therefore opens nothing.
    let in_the_answer = bottom + height * 0.5;

    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws
                .origin_at((left + 4.0, in_the_answer), cx))
            .unwrap(),
        None,
        "the answer's rows belong to no note but this one"
    );
    cx.simulate_click(
        gpui::point(gpui::px(left + 4.0), gpui::px(in_the_answer)),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap_or_default(),
        TOUGH,
        "the note under the click is still the query note"
    );
    let (row, _column) = window
        .update(cx, |ws, _window, cx| ws.cursor_position(cx))
        .unwrap_or_default();
    assert_eq!(row, 0, "the caret landed on the program, not in the answer");
}

/// The answer follows the program: change the SQL in the note and the table asks
/// again. The cache is keyed on the buffer, so this is the same invalidation that
/// keeps the fold honest after an edit.
#[gpui::test]
fn editing_the_program_re_runs_the_answer(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();
    assert_eq!(table(&window, cx).len(), 3, "header and two rows");

    // Open the fold and break the statement.
    cx.simulate_keystrokes("space z");
    cx.simulate_keystrokes("0 f w i f r o m");
    cx.run_until_parked();

    let rows = table(&window, cx);
    assert_eq!(rows.len(), 1, "one line, the reason: {rows:?}");
    assert!(
        rows[0].starts_with("the query could not be answered"),
        "{rows:?}"
    );
}

/// SQLite quotes the statement it refused, newlines and all, and a table row is one
/// line: gpui refuses a text run that holds a newline outright. The message arrives
/// flattened, so the reason survives without taking the row apart.
#[gpui::test]
fn a_refusal_that_spans_lines_is_drawn_on_one(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(
        cx,
        // A syntax error, which is the kind SQLite quotes the statement for.
        "SELECT 1\n# not a heading\nFROM x\n",
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let rows = table(&window, cx);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].starts_with("the query could not be answered"),
        "{rows:?}"
    );
    assert!(!rows[0].contains('\n'), "one line per row: {:?}", rows[0]);
}

/// `G` asks for the last row of the file, and on a query note that row is inside the
/// fold. The caret cannot come to rest in a row nothing paints — the status bar would
/// say one thing and the screen another, and the next `j` would leave from where the
/// reader cannot see — so it lands on the row the fold is drawn on, which is what vim
/// does with a closed fold too.
#[gpui::test]
fn the_end_of_the_file_lands_on_a_row_the_reader_can_see(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();
    assert_eq!(drawn(&window, cx), 1, "the program is folded");

    cx.simulate_keystrokes("G");
    cx.run_until_parked();
    let (row, _) = window
        .update(cx, |ws, _window, cx| ws.cursor_position(cx))
        .unwrap();
    assert_eq!(
        row, 0,
        "the last row of the file is inside the fold, so the caret lands on the row \
         the fold is drawn on"
    );
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.cursor_line_is_visible(cx))
            .unwrap(),
        "and the caret is on a row the view paints"
    );
}

/// Preview's `G` is a reading motion, not a caret motion: it goes to the end of what
/// the view shows. A query note hangs its answer below the program, and the rows of an
/// answer belong to no line of the file — so the last row of the file is not the last
/// row on screen, and reaching it takes the scroller, not the caret.
#[gpui::test]
fn preview_jumps_to_the_last_row_of_the_answer(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let long = "WITH RECURSIVE x(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM x WHERE n < 250) \
         SELECT n FROM x\n";
    let (_dir, window) = monsters(cx, long);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().map(|editor| {
                editor.update(cx, |editor, cx| {
                    editor.set_view(crate::editor::View::Preview, cx)
                })
            })
        })
        .unwrap();
    cx.run_until_parked();

    // (offset, how far the view can go, the pane's height). The answer is 200 rows
    // plus the line admitting the rest, so the view has somewhere to jump to.
    let probe = |ws: &mut Workspace, cx: &mut gpui::Context<Workspace>| {
        ws.editor()
            .map(|e| e.read_with(cx, |e, _| e.scroll_probe()))
            .expect("a note in the editor")
    };
    let (offset, reach, height, _) = window.update(cx, |ws, _window, cx| probe(ws, cx)).unwrap();
    assert!(
        reach > height * 2.,
        "the answer should overflow the pane by a wide margin: {reach} over {height}"
    );
    assert!(offset > -2., "the view starts at the top: {offset}");

    cx.simulate_keystrokes("G");
    cx.run_until_parked();
    let (offset, reach, height, _) = window.update(cx, |ws, _window, cx| probe(ws, cx)).unwrap();
    assert!(
        offset <= -(reach - 2.),
        "the view should be at the bottom of the answer: {offset} of -{reach} (pane {height})"
    );

    // And `gg` still goes to the top, which is the half that already worked.
    cx.simulate_keystrokes("g g");
    cx.run_until_parked();
    let offset = window
        .update(cx, |ws, _window, cx| probe(ws, cx))
        .unwrap()
        .0;
    assert!(offset > -2., "the view is back at the top: {offset}");
}

/// A database message that quotes the statement it refused is longer than any pane,
/// and a block has no viewport of its own — the host scrolls. So the refusal wraps:
/// the reason arrives in pieces, and every piece of it is on screen.
#[gpui::test]
fn a_query_fold_survives_clicking_a_row_and_coming_back(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();
    assert_eq!(drawn(&window, cx), 1, "the program starts folded");

    cx.simulate_keystrokes("space z");
    cx.run_until_parked();
    assert_eq!(drawn(&window, cx), 4, "the reader opened it");

    // The door: a row of the answer, clicked.
    let bands = window
        .update(cx, |ws, _window, cx| ws.phantom_bands(cx))
        .unwrap();
    let door = bands
        .iter()
        .find(|band| band.0)
        .expect("a row that opens a note");
    let at = gpui::point(gpui::px(door.1 + 5.), gpui::px((door.2 + door.3) / 2.));
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    let opened = window
        .update(cx, |ws, _window, _cx| ws.active_path())
        .unwrap();
    assert_ne!(
        opened,
        Some(PathBuf::from("queries/tough.shq")),
        "the click did not leave the query note"
    );

    cx.simulate_keystrokes("ctrl-o");
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |ws, _window, _cx| ws.active_path())
            .unwrap(),
        Some(PathBuf::from("queries/tough.shq")),
        "ctrl-o should come back to the query"
    );
    assert_eq!(
        drawn(&window, cx),
        4,
        "and find the program as the reader left it"
    );
}

#[gpui::test]
fn a_row_that_opens_a_note_says_so_to_the_pointer(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let bands = window
        .update(cx, |ws, _window, cx| ws.phantom_bands(cx))
        .unwrap();
    let over = |band: &(bool, f32, f32, f32)| {
        gpui::point(gpui::px(band.1 + 5.), gpui::px((band.2 + band.3) / 2.))
    };

    let door = bands
        .iter()
        .find(|band| band.0)
        .expect("a row that opens a note");
    cx.simulate_mouse_move(over(door), None, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.cursor_style(cx))
            .unwrap(),
        "hand",
        "the pointer over a row that opens a note stayed a caret: {bands:?}"
    );

    // And off again, below the table: a pointer that only ever turns into a hand
    // would leave the note unreadable as soon as the reader moves away from a row.
    // (Only addressable rows record a band at all, so "a row that opens nothing" is
    // not in this list — the space past the last row is the honest way out.)
    let last = bands.iter().fold(0f32, |low, band| low.max(band.3));
    cx.simulate_mouse_move(
        gpui::point(gpui::px(bands[0].1 + 5.), gpui::px(last + 30.)),
        None,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.cursor_style(cx))
            .unwrap(),
        "ibeam",
        "the pointer past the last row kept the hand: {bands:?}"
    );
}

#[gpui::test]
fn the_answer_wears_a_grid_and_no_rule_lands_on_a_letter(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let grid = window
        .update(cx, |ws, _window, cx| ws.query_grid(cx))
        .unwrap();
    // One grid per cell row: the columns and the two answers. A sentence the answer says
    // about itself is not a cell, so it wears no grid at all.
    assert_eq!(grid.len(), 3, "one grid per cell row: {grid:?}");
    for (text, boundaries, total, _) in &grid {
        for x in boundaries {
            let cell = text.chars().nth(*x);
            assert!(
                cell.is_none_or(|c| c == ' '),
                "a rule at column {x} lands on {cell:?} in {text:?}"
            );
        }
        assert!(
            *total >= text.chars().count(),
            "the grid holds the row it is drawn over: {text:?} is {} cells, the grid is {total}",
            text.chars().count()
        );
    }
    assert!(
        grid[0].3,
        "the row that names the columns is the shaded one"
    );
    assert!(
        grid[1..].iter().all(|row| !row.3),
        "an answer is not a header"
    );
}

#[gpui::test]
fn a_refusal_wears_no_grid(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, "+++\n+++\nSELECT nope FROM props\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let grid = window
        .update(cx, |ws, _window, cx| ws.query_grid(cx))
        .unwrap();
    assert!(
        grid.is_empty(),
        "a refusal is a sentence, not a cell: {grid:?}"
    );
    let rows = table(&window, cx);
    assert!(
        rows[0].contains("nope"),
        "and the sentence is still drawn: {rows:?}"
    );
}

#[gpui::test]
fn a_refusal_longer_than_the_pane_wraps(cx: &mut TestAppContext) {
    let long = "SELECT note, value, note AS again, nope, value AS again2, note AS again3 \
              FROM props WHERE key = 'cr' AND value > 0 AND note IS NOT NULL ORDER BY value\n";
    let (_dir, window) = monsters(cx, long);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let rows = table(&window, cx);
    assert_eq!(rows.len(), 1, "one row, the reason: {rows:?}");
    let pieces = window
        .update(cx, |ws, window, cx| ws.query_pieces(window, cx))
        .unwrap();
    assert!(
        pieces.len() > 1,
        "a {}-character reason drawn as one line: {pieces:?}",
        rows[0].chars().count()
    );
    assert_eq!(
        pieces.concat(),
        rows[0],
        "breaking the row loses nothing between the pieces"
    );
    assert!(
        pieces.iter().all(|piece| !piece.trim().is_empty()),
        "every piece has something in it: {pieces:?}"
    );
}

/// A table row is not prose: breaking one mid-cell would leave the continuation lines
/// with nothing to align to, so the columns stay whole and the cell clip does the
/// bounding instead.
#[gpui::test]
fn a_table_row_keeps_its_columns_whole(cx: &mut TestAppContext) {
    // Four cells at the clip's own width: the row is longer than the test's pane by a
    // wide margin, which is exactly why the rule has to be visible in the test.
    let sql = format!(
        "SELECT '{}' AS one, '{}' AS two, '{}' AS three, '{}' AS four",
        "x".repeat(60),
        "y".repeat(60),
        "z".repeat(60),
        "w".repeat(60)
    );
    let (_dir, window) = monsters(cx, &sql);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let rows = table(&window, cx);
    assert!(
        rows.last().expect("the answer row").chars().count() > 200,
        "the row is meant to be wider than the pane: {rows:?}"
    );
    let pieces = window
        .update(cx, |ws, window, cx| ws.query_pieces(window, cx))
        .unwrap();
    assert_eq!(pieces, rows, "one piece per row, header included");
}

/// The palette is the door for someone who does not remember the word.
#[gpui::test]
fn the_palette_offers_a_saved_query(cx: &mut TestAppContext) {
    let (_dir, window) = monsters(cx, TOUGH);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    for query in ["run saved query", "saved query", "query"] {
        cx.simulate_keystrokes("ctrl-shift-p");
        cx.simulate_input(query);
        let rows = offered(&window, cx);
        assert!(
            rows.iter().any(|row| row == "Run a saved query"),
            "{query} offers: {rows:?}"
        );
        cx.simulate_keystrokes("escape");
    }
}

// --- A query embedded in a note (roadmap P4.3 M4) ------------------------------------

/// The vault with its two monsters, its saved query, and a note that asks for it.
fn lair(
    cx: &mut TestAppContext,
    query: &str,
    ask: &str,
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n# Goblin\n"),
            ("Wisp.md", "---\ncr: 3\n---\n"),
            ("queries/tough.shq", query),
            ("Lair.md", ask),
        ],
    )
}

/// The rows the frame hangs under a transclusion row, badge first.
fn hung(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    let shape = window
        .update(cx, |ws, _window, cx| ws.embed_shape(cx))
        .unwrap();
    // The row that asked is not part of the answer; the tests read what came back.
    shape
        .into_iter()
        .filter(|line| !line.starts_with("embed row ") || line.contains(": query "))
        .collect()
}

/// `![[tough]]` in a note is the same query, answered the same way, drawn under the row
/// that asked for it (EDGE-10). The program is not copied into the note: the note borrows
/// the answer.
#[gpui::test]
fn a_note_that_embeds_a_query_draws_the_answer(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, TOUGH, "![[tough]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    let rows = hung(&window, cx);
    assert_eq!(
        rows.first().map(|line| line.trim_end()),
        Some("embed row 0: query 2 rows"),
        "the badge carries the answer's shape: {rows:?}"
    );
    assert!(
        !rows[0].contains("tough"),
        "the row above the block already names the query; the badge does not say it twice: {rows:?}"
    );
    let body = rows.join("\n");
    assert!(
        body.contains("Wisp.md") && body.contains("Goblin.md"),
        "both monsters answer, in the order the query asked: {rows:?}"
    );
    assert!(
        body.find("Wisp.md") < body.find("Goblin.md"),
        "and the declaration's order survives the embed: {rows:?}"
    );
}

/// A row naming a note is that note, in an embed as in the query note's own view: the
/// door is the rule, not the surface.
#[gpui::test]
fn a_row_of_an_embedded_answer_that_names_a_note_opens_it(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, TOUGH, "![[tough]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    let bands = window
        .update(cx, |ws, _window, cx| ws.phantom_bands(cx))
        .unwrap();
    let doors = bands.iter().filter(|(opens, ..)| *opens).count();
    assert_eq!(
        doors, 3,
        "the two rows that name a note, and the title row that opens the query itself: {bands:?}"
    );
}

/// A query the parser refused has no answer to draw, and an embed has no source of its
/// own to unfold — so the reason is drawn in its place (EDGE-11). A blank block would
/// leave the reader guessing whether the query is missing, broken, or empty.
#[gpui::test]
fn an_embedded_query_that_does_not_parse_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, "+++\nname = = 3\n+++\nSELECT 1\n", "![[tough]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    let rows = hung(&window, cx);
    let body = rows.join("\n");
    assert!(
        body.contains("this is not a query note"),
        "the block says what is wrong with the program: {rows:?}"
    );
    assert!(
        body.contains("refused"),
        "and the badge refuses rather than counting nothing: {rows:?}"
    );
}

/// A statement the runner refuses is drawn the same way: the reader sees the database's
/// own words, which is the only answer this block can give.
#[gpui::test]
fn an_embedded_query_the_runner_refuses_shows_the_refusal(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, "+++\n+++\nSELECT nope FROM props\n", "![[tough]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    let rows = hung(&window, cx);
    let body = rows.join("\n");
    assert!(
        body.contains("nope"),
        "the database's own words, quoted: {rows:?}"
    );
}

/// An empty answer is not an error: the block draws nothing but the badge, and the badge
/// counts zero. (Ruling 17's shape, where a query that answers nothing keeps its fold
/// shut — here there is no fold, so the count is the whole statement.)
#[gpui::test]
fn an_embedded_query_that_answers_nothing_counts_zero(cx: &mut TestAppContext) {
    let (_dir, window) = lair(
        cx,
        "+++\n+++\nSELECT note FROM props WHERE key = 'no such key'\n",
        "![[tough]]\n",
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    let rows = hung(&window, cx);
    assert_eq!(
        rows.first().map(|line| line.trim_end()),
        Some("embed row 0: query 0 rows"),
        "the badge says what came back, which is nothing: {rows:?}"
    );
}

/// A query note has no sections: its `#` lines are SQL comments. An anchor asks for
/// something the file cannot have, and the sentence is the one the note branch already
/// uses for the same mistake.
#[gpui::test]
fn an_anchor_on_a_query_embed_says_there_is_no_section(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, TOUGH, "![[tough#hp]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    let shape = window
        .update(cx, |ws, _window, cx| ws.embed_shape(cx))
        .unwrap();
    assert_eq!(
        shape,
        vec!["embed row 0: badge no section \"hp\" in tough"],
        "the anchor is refused, in the words the note branch uses: {shape:?}"
    );
}

/// A target that names no note and no query is unchanged: the badge it always drew, in
/// brackets, with no table under it.
#[gpui::test]
fn an_embed_of_nothing_is_unchanged(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, TOUGH, "![[nope]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.run_until_parked();

    let shape = window
        .update(cx, |ws, _window, cx| ws.embed_shape(cx))
        .unwrap();
    assert!(
        shape.is_empty(),
        "no query, no note: nothing is drawn, and the raw line stays as the honest \
         rendering of a broken link: {shape:?}"
    );
}

/// The query note's own view is unchanged by all of this: opening the `.shq` directly
/// still draws the fold and the table, not an embed of itself.
#[gpui::test]
fn a_query_note_opened_directly_is_not_an_embed(cx: &mut TestAppContext) {
    let (_dir, window) = lair(cx, TOUGH, "![[tough]]\n");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "queries/tough.shq");
    cx.run_until_parked();

    let shape = window
        .update(cx, |ws, _window, cx| ws.embed_shape(cx))
        .unwrap();
    assert!(
        shape.is_empty(),
        "the program has no transclusion in it: {shape:?}"
    );
    let rows = window
        .update(cx, |ws, _window, cx| ws.query_shape(cx))
        .unwrap();
    assert_eq!(
        rows.len(),
        3,
        "and its own table is still there, header and two rows: {rows:?}"
    );
}
