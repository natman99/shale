//! A link written by an **alias** is a link to the note that declares it.
//!
//! The alias table is `LinkResolver`'s fourth rule, and the link colour, the hover card,
//! the embed door and `ctrl-]` all hand it over. The graph is the fifth door: a picture
//! that has not been taught the aliases draws a dead node where the alias was written and
//! leaves the real note looking like nobody writes about it — the disagreement the alias
//! feature's own comment says it cannot have.
//!
//! What is pinned here is the **wiring** (the window hands the graph the same list it
//! hands the editor) and the one case the incremental path cannot reach: an alias is
//! declared in one note's frontmatter and written in another note's body, so the note that
//! changed is not the note whose link it is.

use super::*;
use gpui::WindowHandle;

/// The counts the strip's row is drawn from.
fn counts(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Option<GraphSummary> {
    window
        .update(cx, |ws, _window, cx| ws.graph_counts(cx))
        .unwrap()
}

/// A window over a vault where `Deep.md` declares an alias and `b.md` writes that alias
/// as a link. The index is on, because the aliases the window hands the graph come from
/// the `props` table.
fn alias_vault(
    cx: &mut TestAppContext,
    deep: &str,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = indexed_vault_with(
        cx,
        &[("Deep.md", deep), ("b.md", "see [[the deep one]]\n")],
        test_config(),
    );
    let cx = VisualTestContext::from_window(window.into(), cx);
    (dir, window, cx)
}

const WITH_ALIAS: &str = "---\naliases: [the deep one]\n---\n\nthe well\n";
const WITHOUT_ALIAS: &str = "the well\n";

/// The graph agrees with the editor: the alias is an edge to `Deep.md`, not a dead node,
/// and `Deep.md` is not an orphan.
#[gpui::test]
fn a_link_written_by_alias_is_not_a_dead_node(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = alias_vault(cx, WITH_ALIAS);
    cx.simulate_keystrokes("space g");
    cx.run_until_parked();
    let counts = counts(&mut cx, &window).expect("the picture is built");
    assert_eq!(counts.notes, 2, "both notes are on screen");
    assert_eq!(
        counts.ghosts, 0,
        "the alias is not a dead node where it was written"
    );
}

/// The ordinary case in the live app: the alias was declared some time ago and the link
/// is typed now, so it reaches the graph through the save door rather than a full scan.
#[gpui::test]
fn a_link_typed_in_the_editor_reaches_the_note_the_alias_names(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault_with(
        cx,
        &[("Deep.md", WITH_ALIAS), ("b.md", "nothing yet\n")],
        test_config(),
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("space g");
    cx.run_until_parked();
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.links),
        Some(0),
        "nothing links yet"
    );

    show(&mut cx, &window, "b.md");
    // Opening a note marks the keyboard owed and the next frame pays it, so the keystrokes
    // below need that frame first — otherwise they are still going to the graph view.
    repaint(&mut cx);
    cx.simulate_keystrokes("A");
    cx.simulate_input(" see [[the deep one]]");
    cx.simulate_keystrokes("escape");
    command_line("w", &mut cx);
    cx.run_until_parked();

    let after = counts(&mut cx, &window).expect("the picture is built");
    assert_eq!(after.links, 1, "the typed alias became an edge to the note");
    assert_eq!(after.ghosts, 0, "and not a dead node");
    assert_eq!(after.ghosts, 0, "and not a dead node");
}

/// The rebuild the alias change starts must not swallow a save that lands while it runs.
/// The scan read the disk before the save, so the save belongs on the queue even though an
/// index is already standing — otherwise the new index arrives without it.
#[gpui::test]
fn a_note_saved_while_the_alias_rebuild_runs_is_not_lost(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = alias_vault(cx, WITHOUT_ALIAS);
    cx.simulate_keystrokes("space g");
    cx.run_until_parked();
    assert!(counts(&mut cx, &window).is_some(), "the index is live");

    std::fs::write(dir.path().join("Deep.md"), WITH_ALIAS).unwrap();
    window
        .update(&mut cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();
    // The re-scan is in flight; a note Shale wrote itself arrives now.
    window
        .update(&mut cx, |ws, _window, _cx| {
            ws.graph_note_saved(
                &PathBuf::from("b.md"),
                "see [[the deep one]] and [[gone]]\n",
            )
        })
        .unwrap();
    cx.run_until_parked();

    let after = counts(&mut cx, &window).expect("the picture is built");
    assert_eq!(
        after.ghosts, 1,
        "the dead end typed during the scan is there"
    );
    assert_eq!(
        after.links, 2,
        "and so are both edges the typed note now writes: the alias and the dead end"
    );
}

/// The case the incremental path cannot reach. `b.md` already writes the alias as a link
/// and nothing about `b.md` changes; `Deep.md` gains the alias. Re-scanning the note that
/// moved would leave the dead node standing, so a moved alias set re-scans the vault.
#[gpui::test]
fn declaring_an_alias_moves_an_edge_written_in_another_note(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = alias_vault(cx, WITHOUT_ALIAS);
    cx.simulate_keystrokes("space g");
    cx.run_until_parked();
    let before = counts(&mut cx, &window).expect("the picture is built");
    assert_eq!(
        before.ghosts, 1,
        "with no alias declared, `the deep one` is a genuine dead end"
    );

    std::fs::write(dir.path().join("Deep.md"), WITH_ALIAS).unwrap();
    window
        .update(&mut cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();
    cx.run_until_parked();

    let after = counts(&mut cx, &window).expect("the picture is still built");
    assert_eq!(after.ghosts, 0, "the dead node is gone");
    assert_eq!(
        after.notes, 2,
        "and the edge is on the note that declares the alias"
    );
}
