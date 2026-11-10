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
        vec!["columns: note · value", "Wisp.md", "Goblin.md"],
        "the answer, in the order the query asked for it: cr 3 before cr 5"
    );
}

/// A query note with no declaration at all is still a query: the fence is optional, and
/// so is the ceremony.
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
        vec!["columns: note", "Goblin.md", "Wisp.md"]
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
        vec!["columns: note · value", "Goblin.md", "Wisp.md"],
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
    assert_eq!(offered(&window, cx), vec!["columns: note", "Ghost.md"]);
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

/// A bare editor showing `text` as the file `path`, focused. The path matters:
/// it is what makes the view ask whether this is a query note at all.
fn query_editor(cx: &mut TestAppContext, path: &str, text: &str) -> WindowHandle<MarkdownEditor> {
    let window = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    window
        .update(cx, |editor, window, cx| {
            editor.set_text(text.to_string(), cx);
            editor.set_embed_context(Some(Path::new(path)), Vec::new(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    window
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
    let editor = query_editor(cx, "queries/tough.shq", TOUGH);
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    let (lines, markers) = shown(&editor, cx);
    assert_eq!(
        lines, 1,
        "the declaration and the SQL are one fold, and it starts shut"
    );
    assert_eq!(
        markers,
        vec![" tough"],
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
    let editor = query_editor(
        cx,
        "queries/ratings.shq",
        "+++\nname = \"Creature ratings\"\n+++\nSELECT 1\n",
    );
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    let (_lines, markers) = shown(&editor, cx);
    assert_eq!(
        markers,
        vec![" Creature ratings"],
        "the declared name, not the file's"
    );
}

/// `space z` is the keyboard route. It has to exist because preview swallows
/// `za` with every other caret motion, and a query note whose fold starts shut
/// is no use to a reader who cannot open it without a mouse.
#[gpui::test]
fn space_z_opens_the_program_and_shuts_it_again(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let editor = query_editor(cx, "queries/tough.shq", TOUGH);
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
    let editor = query_editor(cx, "queries/tough.shq", TOUGH);
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
    let editor = query_editor(cx, "queries/tough.shq", TOUGH);
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
    let editor = query_editor(cx, "queries/bad.shq", "not a question at all\nnope\n");
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
    let editor = query_editor(
        cx,
        "queries/notes.shq",
        "+++\n+++\nSELECT 1\n# not a heading\nFROM x\n",
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
    let editor = query_editor(
        cx,
        "queries/notes.shq",
        "+++\n+++\nSELECT 1\n# not a heading\nFROM x\n",
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
    let editor = query_editor(cx, "queries/two.shq", "+++\n+++\nSELECT 1; SELECT 2\n");
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
    let editor = query_editor(
        cx,
        "queries/notes.shq",
        "+++\n+++\nSELECT 1\n# not a heading\nFROM x\n",
    );
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);
    cx.simulate_keystrokes("space z");
    assert_eq!(shown(&editor, cx).0, 6, "the program is open");

    // Down to the row that would be a heading in a note, and `za`.
    cx.simulate_keystrokes("g g j j j");
    cx.simulate_keystrokes("z a");
    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 1, "the whole program folded, not one line of it");
    assert_eq!(markers, vec![" notes"], "and the marker is the query's");
}

/// A query note is not markdown: its `#`-looking lines are not sections. The one
/// region it has is its program, so a fold never leaves half a statement on
/// screen with a marker of its own.
#[gpui::test]
fn a_query_note_has_no_heading_sections(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let editor = query_editor(
        cx,
        "queries/notes.shq",
        "+++\n+++\nSELECT 1\n# not a heading\nFROM x\n",
    );
    let cx = &mut VisualTestContext::from_window(editor.into(), cx);

    let (lines, markers) = shown(&editor, cx);
    assert_eq!(lines, 1, "one region, the whole file");
    assert_eq!(markers, vec![" notes"], "one marker, the query's own");
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
