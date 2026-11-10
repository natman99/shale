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
