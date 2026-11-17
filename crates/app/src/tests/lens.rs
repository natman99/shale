//! The lens door (roadmap P4.16 M3): `:lens`, the picker, and what each answer does to
//! the picture. The dimension is core's (`graph::filter::LensScope`) and so are the
//! files (`lens`); what lives here is the moment a name becomes a mask — and the
//! refusals, which leave the picture standing exactly as it was (EDGE-L6).
//!
//! The vault these run on is the graph's usual shape: `a.md` linking `b.md` and a note
//! that is not there, one tag, and one orphan, which is three notes and two links. The
//! index is on, because a lens answers from it — and one test turns it off on purpose.

use super::commands::run_palette;
use super::review::{command_line, say};
use super::*;
use gpui::WindowHandle;

/// The counts the strip's row is drawn from: what the picture is holding.
fn counts(
    cx: &mut VisualTestContext,
    window: &WindowHandle<Workspace>,
) -> Option<crate::graph::GraphSummary> {
    window
        .update(cx, |ws, _window, cx| ws.graph_counts(cx))
        .unwrap()
}

/// What the graph is wearing, if anything: the lens's name and how many paths it answers.
fn worn(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<(String, usize)> {
    window
        .update(cx, |ws, _window, _cx| {
            ws.graph_filter()
                .lens
                .map(|lens| (lens.name, lens.paths.len()))
        })
        .unwrap()
}

/// The rows the open picker shows, as `(label, detail)`.
fn rows(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> Vec<(String, String)> {
    window
        .update(cx, |ws, _window, cx| {
            ws.picker_shown(cx)
                .into_iter()
                .map(|(label, detail, _runs)| (label, detail))
                .collect()
        })
        .unwrap()
}

/// A window over that vault, with the lens files this test names written under
/// `.shale/lenses`. The index is on: a lens answers from it.
fn lens_vault(
    cx: &mut TestAppContext,
    lenses: &[(&str, &str)],
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = indexed_vault_with(
        cx,
        &[
            ("a.md", "see [[b]] and [[gone]] and #tag\n"),
            ("b.md", "back to [[a]]\n"),
            ("c.md", "alone\n"),
        ],
        test_config(),
    );
    write_lenses(&dir, lenses);
    let cx = VisualTestContext::from_window(window.into(), cx);
    (dir, window, cx)
}

fn write_lenses(dir: &tempfile::TempDir, lenses: &[(&str, &str)]) {
    let here = dir.path().join(".shale").join("lenses");
    std::fs::create_dir_all(&here).unwrap();
    for (name, text) in lenses {
        std::fs::write(here.join(format!("{name}.shq")), text).unwrap();
    }
}

/// The graph tab, open and built.
fn on_graph(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) {
    cx.simulate_keystrokes("space g");
    cx.run_until_parked();
    assert!(counts(cx, window).is_some(), "the picture is built");
}

/// A lens is a name, and the picture becomes what it answered.
#[gpui::test]
fn a_lens_narrows_the_picture_to_what_it_answered(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "ab",
            "SELECT path AS note FROM notes WHERE path IN ('b.md', 'a.md') ORDER BY path",
        )],
    );
    on_graph(&mut cx, &window);
    assert_eq!(
        counts(&mut cx, &window).map(|counts| counts.notes),
        Some(3),
        "the whole vault is on the screen first"
    );

    command_line("lens ab", &mut cx);
    assert_eq!(
        worn(&window, &mut cx),
        Some(("ab".to_string(), 2)),
        "the lens is on the graph"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|counts| counts.notes),
        Some(2),
        "and the picture is the answer, not a dimmed version of itself"
    );
    assert_eq!(say(&window, &mut cx), "lens `ab`: 2 notes");
}

/// `none` is a name that means no lens (EDGE-L9), and saying it with nothing worn says
/// so rather than pretending to undo something.
#[gpui::test]
fn none_is_a_name_that_takes_the_lens_off(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'a.md'")],
    );
    on_graph(&mut cx, &window);
    command_line("lens ab", &mut cx);
    assert_eq!(counts(&mut cx, &window).map(|c| c.notes), Some(1));

    command_line("lens none", &mut cx);
    assert_eq!(worn(&window, &mut cx), None, "no lens is worn");
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(3),
        "the whole vault is back"
    );
    assert_eq!(say(&window, &mut cx), "lens off");

    command_line("lens none", &mut cx);
    assert_eq!(say(&window, &mut cx), "no lens is on the graph");
}

/// A name with no file behind it is a sentence, not a change.
#[gpui::test]
fn an_unknown_lens_says_so_and_changes_nothing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    command_line("lens goblins", &mut cx);
    assert!(
        say(&window, &mut cx).contains("no lens named `goblins`"),
        "{}",
        say(&window, &mut cx)
    );
    assert_eq!(worn(&window, &mut cx), None);
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(3),
        "the picture stands"
    );
}

/// `:lens` alone asks (ruling 5), and each row carries what its lens answers **now**
/// (EDGE-L10) — including a lens that will not answer, which says so and still opens.
#[gpui::test]
fn the_picker_lists_the_lenses_with_what_they_answer_now(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[
            (
                "ab",
                "SELECT path AS note FROM notes WHERE path IN ('a.md', 'b.md') ORDER BY path",
            ),
            (
                "lonely",
                "SELECT path AS note FROM notes WHERE path = 'c.md'",
            ),
            ("broken", "SELECT path AS note FROM nope"),
        ],
    );
    on_graph(&mut cx, &window);
    command_line("lens", &mut cx);
    let shown = rows(&mut cx, &window);
    assert_eq!(
        shown,
        vec![
            ("ab".to_string(), "2 notes".to_string()),
            ("broken".to_string(), "\u{2014} not answering".to_string()),
            ("lonely".to_string(), "1 note".to_string()),
        ],
        "three lenses, each measured at the moment the picker opened"
    );

    // Choosing a row puts that lens on: the row's target is the lens's name.
    cx.simulate_keystrokes("enter");
    assert_eq!(worn(&window, &mut cx), Some(("ab".to_string(), 2)));
    assert_eq!(counts(&mut cx, &window).map(|c| c.notes), Some(2));
}

/// A lens already on the graph offers its way off as the picker's first row.
#[gpui::test]
fn the_picker_offers_the_way_off_when_a_lens_is_on(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'a.md'")],
    );
    on_graph(&mut cx, &window);
    command_line("lens ab", &mut cx);
    command_line("lens", &mut cx);
    let shown = rows(&mut cx, &window);
    assert_eq!(
        shown
            .first()
            .map(|(label, detail)| (label.as_str(), detail.as_str())),
        Some(("no lens", "currently wearing `ab`")),
        "the way off is the first row, and it says what is on"
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(worn(&window, &mut cx), None);
}

/// EDGE-L6: a lens that will not answer gets the same sentence the picker and an
/// embedded table give, and the picture does not change.
#[gpui::test]
fn a_lens_that_will_not_answer_leaves_the_picture_standing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(cx, &[("broken", "SELECT path AS note FROM nope")]);
    on_graph(&mut cx, &window);
    command_line("lens broken", &mut cx);
    assert!(
        say(&window, &mut cx).contains("could not be answered"),
        "{}",
        say(&window, &mut cx)
    );
    assert_eq!(
        worn(&window, &mut cx),
        None,
        "a lens that will not answer is not worn"
    );
    assert_eq!(counts(&mut cx, &window).map(|c| c.notes), Some(3));
}

/// An empty answer is an answer. Unlike a local scope whose centre the index does not
/// know — where the window owns that centre and keeps the picture — a lens's nothing is
/// the lens's own, so the picture goes empty.
#[gpui::test]
fn an_empty_answer_empties_the_picture(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) =
        lens_vault(cx, &[("empty", "SELECT path AS note FROM notes WHERE 0")]);
    on_graph(&mut cx, &window);
    command_line("lens empty", &mut cx);
    assert_eq!(worn(&window, &mut cx), Some(("empty".to_string(), 0)));
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(0),
        "an empty answer draws nothing"
    );
    assert_eq!(say(&window, &mut cx), "lens `empty`: no note answers this");
}

/// EDGE-L5: an answer can name paths the graph has no node for. They are counted out
/// loud rather than dropped.
#[gpui::test]
fn an_answer_naming_a_note_the_graph_lacks_is_counted_out_loud(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "half",
            "SELECT 'nowhere.md' AS note UNION SELECT 'a.md' AS note",
        )],
    );
    on_graph(&mut cx, &window);
    command_line("lens half", &mut cx);
    assert_eq!(say(&window, &mut cx), "lens `half`: 1 note, 1 with no node");
    assert_eq!(counts(&mut cx, &window).map(|c| c.notes), Some(1));
}

/// EDGE-L12: with the index off the sentence is about the setting, not a claim that no
/// note answers — and no lens is put on.
#[gpui::test]
fn a_lens_needs_the_index_it_answers_from(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", "see [[b]]\n")]);
    write_lenses(
        &dir,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'a.md'")],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    on_graph(&mut cx, &window);
    command_line("lens ab", &mut cx);
    assert!(
        say(&window, &mut cx).contains("no vault index"),
        "{}",
        say(&window, &mut cx)
    );
    assert_eq!(worn(&window, &mut cx), None);
    assert_eq!(counts(&mut cx, &window).map(|c| c.notes), Some(1));
}

/// The palette row is the same door as `:lens` with nothing typed: it asks.
#[gpui::test]
fn the_palette_row_asks_which_lens_to_wear(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'a.md'")],
    );
    on_graph(&mut cx, &window);
    run_palette("lens", &mut cx);
    let shown = rows(&mut cx, &window);
    assert_eq!(
        shown,
        vec![("ab".to_string(), "1 note".to_string())],
        "the row opened the lens picker rather than acting"
    );
}
