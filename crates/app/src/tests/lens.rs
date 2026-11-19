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
use std::path::PathBuf;

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

// ---------------------------------------------------------------- the status bar

/// Paint a frame, so what a test reads is what the bar **drew** rather than what a
/// function could have drawn. The segment's text is one thing; whether the bar gave it a
/// slot is the thing a later edit can lose.
fn draw(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// The lens segment the bar drew last frame, or `None` when it drew none.
fn bar_lens(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    window.update(cx, |ws, _window, _cx| ws.bar_lens()).unwrap()
}

/// A lens is easy to forget is on, so the bar says it beside the view badge: the one
/// place that answers "is this the whole vault?" without the strip being focused.
#[gpui::test]
fn the_status_bar_names_the_lens_the_graph_is_wearing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'b.md'")],
    );
    on_graph(&mut cx, &window);
    draw(&mut cx);
    assert_eq!(
        bar_lens(&window, &mut cx),
        None,
        "a graph wearing no lens has no lens segment"
    );

    command_line("lens ab", &mut cx);
    draw(&mut cx);
    assert_eq!(
        bar_lens(&window, &mut cx).as_deref(),
        Some("lens ab"),
        "the bar names the lens the picture is wearing"
    );

    command_line("lens none", &mut cx);
    draw(&mut cx);
    assert_eq!(
        bar_lens(&window, &mut cx),
        None,
        "taking the lens off takes the segment with it"
    );
}

/// The segment describes the picture, not the note. With a note tab on screen the bar
/// goes quiet about a lens the graph is still wearing — this is the bar refusing to
/// claim something about what is drawn, not the window forgetting it.
#[gpui::test]
fn the_status_bar_keeps_the_lens_off_a_note_tab(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'b.md'")],
    );
    on_graph(&mut cx, &window);
    command_line("lens ab", &mut cx);
    draw(&mut cx);
    assert_eq!(bar_lens(&window, &mut cx).as_deref(), Some("lens ab"));

    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("b.md"), cx)
        })
        .unwrap();
    cx.run_until_parked();
    draw(&mut cx);
    assert_eq!(
        bar_lens(&window, &mut cx),
        None,
        "a note tab's bar says nothing about the graph's lens"
    );
    assert_eq!(
        worn(&window, &mut cx).map(|(name, _)| name).as_deref(),
        Some("ab"),
        "the lens is still on the graph, segment or no segment"
    );
}

// ------------------------------------------------- the title, the strip, the re-run
//
// M4: a lens is not only a mask. The tab says which graph it is, the strip counts what the
// answer holds (EDGE-L14: one answer, so no two surfaces disagree), and the answer keeps
// up with the vault — re-asked when the index moves or when the file is edited by hand
// (EDGE-L7), never silently replaced by a lens that stopped answering (EDGE-L6), and said
// aloud when the file goes away under it (EDGE-L11).

/// The labels the tab bar **drew** for graph tabs last frame.
fn tab_labels(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, _cx| ws.tab_labels())
        .unwrap()
}

/// What the filter strip **drew** as its lens row last frame, or `None` when it drew no
/// such row — no lens on, or the strip closed.
fn strip_lens(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |ws, _window, cx| ws.strip_lens(cx))
        .unwrap()
}

/// How many times a re-run replaced the worn scope. The layout half of EDGE-L7: re-asking
/// is one SQL call, and only a moved answer is allowed to re-make the picture.
fn replaced(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, _cx| ws.lens_replaced())
        .unwrap()
}

/// `/` opens the strip, and a frame later the strip is part of what was drawn.
fn open_strip(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) {
    cx.simulate_keystrokes("/");
    draw(cx);
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.graph_panel(cx))
            .unwrap()
            .0,
        "the strip is on screen"
    );
}

/// Notice, right now, what changed on disk — including under `.shale/lenses`, which the
/// vault's own walk never sees (EDGE-L1).
fn disk_tick(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) {
    window
        .update(cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();
    cx.run_until_parked();
}

/// Rewrite a lens on disk, as an editor outside Shale would. Every rewrite here changes
/// the **length** as well as the text: the stamp that notices a hand edit carries size and
/// mtime, and a same-length edit could land inside the same clock tick as the file it
/// replaces — which would make the test about the filesystem's clock rather than the rule.
fn rewrite_lens(dir: &tempfile::TempDir, name: &str, text: &str) {
    std::fs::write(
        dir.path()
            .join(".shale")
            .join("lenses")
            .join(format!("{name}.shq")),
        text,
    )
    .unwrap();
}

/// A lens changes which graph the tab *is*, not merely what it is showing, so the tab says
/// it — and says nothing once the lens is off.
#[gpui::test]
fn the_tab_names_the_lens_the_graph_is_wearing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'b.md'")],
    );
    on_graph(&mut cx, &window);
    draw(&mut cx);
    assert_eq!(
        tab_labels(&window, &mut cx),
        vec!["graph"],
        "no lens, no name"
    );

    command_line("lens ab", &mut cx);
    draw(&mut cx);
    assert_eq!(
        tab_labels(&window, &mut cx),
        vec!["graph · ab"],
        "the tab says which graph this is"
    );

    command_line("lens none", &mut cx);
    draw(&mut cx);
    assert_eq!(
        tab_labels(&window, &mut cx),
        vec!["graph"],
        "and stops saying it when the lens comes off"
    );
}

/// The strip's lens row: the name, and how many notes the answer holds. Not a chip — the
/// strip does not toggle a lens.
#[gpui::test]
fn the_strip_names_the_lens_and_counts_what_it_answers(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "ab",
            "SELECT path AS note FROM notes WHERE path IN ('b.md', 'a.md') ORDER BY path",
        )],
    );
    on_graph(&mut cx, &window);
    open_strip(&mut cx, &window);
    assert_eq!(strip_lens(&window, &mut cx), None, "no lens, no row");

    command_line("lens ab", &mut cx);
    open_strip(&mut cx, &window);
    assert_eq!(
        strip_lens(&window, &mut cx).as_deref(),
        Some("lens ab · 2 notes"),
        "the row counts the answer, in the same words the status line and the picker use"
    );
}

/// EDGE-L5 in the strip (ruling 9): an answer naming paths the graph has no node for is
/// counted there too, rather than only in the sentence the command left behind.
#[gpui::test]
fn the_strip_counts_the_answers_with_no_node(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "half",
            "SELECT 'nowhere.md' AS note UNION SELECT 'a.md' AS note",
        )],
    );
    on_graph(&mut cx, &window);
    command_line("lens half", &mut cx);
    open_strip(&mut cx, &window);
    assert_eq!(
        strip_lens(&window, &mut cx).as_deref(),
        Some("lens half · 1 note, 1 with no node"),
        "the picture and the answer disagree by one, and the strip says which one"
    );
}

/// EDGE-L7: the lens re-runs when the index moves. A note added to the vault that the
/// lens's own query would answer joins the picture without the lens being taken off and
/// put back on.
#[gpui::test]
fn a_lens_re_runs_when_the_index_moves(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "under",
            "SELECT path AS note FROM notes WHERE path < 'c.md' ORDER BY path",
        )],
    );
    on_graph(&mut cx, &window);
    command_line("lens under", &mut cx);
    assert_eq!(worn(&window, &mut cx), Some(("under".to_string(), 2)));
    assert_eq!(counts(&mut cx, &window).map(|c| c.notes), Some(2));

    std::fs::write(dir.path().join("a2.md"), "new, and under the same rule\n").unwrap();
    disk_tick(&window, &mut cx);

    assert_eq!(
        worn(&window, &mut cx),
        Some(("under".to_string(), 3)),
        "the answer moved with the vault"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(3),
        "and the picture moved with the answer"
    );
}

/// EDGE-L7's other half, the one the measurement was for: re-asking is cheap, and the
/// layout is only disturbed when the answer's **node set** moved. An index move the lens
/// does not care about leaves the scope standing.
#[gpui::test]
fn an_index_move_that_changes_no_answer_leaves_the_picture_alone(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "ab",
            "SELECT path AS note FROM notes WHERE path IN ('b.md', 'a.md') ORDER BY path",
        )],
    );
    on_graph(&mut cx, &window);
    command_line("lens ab", &mut cx);
    assert_eq!(
        replaced(&window, &mut cx),
        0,
        "wearing a lens is not a re-run"
    );

    std::fs::write(dir.path().join("z.md"), "outside the answer entirely\n").unwrap();
    disk_tick(&window, &mut cx);

    assert_eq!(
        replaced(&window, &mut cx),
        0,
        "the lens was re-asked and its answer did not move, so the picture was not re-made"
    );
    assert_eq!(worn(&window, &mut cx), Some(("ab".to_string(), 2)));
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(2),
        "the new note is in the vault and not in the lens"
    );
}

/// EDGE-L7's file half: the lens is a file, the file is the authority, and an edit made in
/// an editor outside Shale reaches the graph that is wearing it — the vault's own walk
/// never looks in `.shale/`, so the lens folder is stamped beside it.
#[gpui::test]
fn a_lens_edited_by_hand_re_runs_under_the_picture(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "ab",
            "SELECT path AS note FROM notes WHERE path IN ('b.md', 'a.md') ORDER BY path",
        )],
    );
    on_graph(&mut cx, &window);
    command_line("lens ab", &mut cx);
    assert_eq!(counts(&mut cx, &window).map(|c| c.notes), Some(2));

    rewrite_lens(
        &dir,
        "ab",
        "SELECT path AS note FROM notes WHERE path = 'b.md'",
    );
    disk_tick(&window, &mut cx);

    assert_eq!(worn(&window, &mut cx), Some(("ab".to_string(), 1)));
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(1),
        "the picture followed the edited program without being asked to"
    );
}

/// EDGE-L6 live: a lens edited into something that will not answer keeps the picture it
/// last gave, and the strip says it is not answering rather than leaving the reader to
/// notice the picture has gone stale.
#[gpui::test]
fn a_lens_that_stops_answering_keeps_the_picture_and_says_so(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "ab",
            "SELECT path AS note FROM notes WHERE path IN ('b.md', 'a.md') ORDER BY path",
        )],
    );
    on_graph(&mut cx, &window);
    command_line("lens ab", &mut cx);
    open_strip(&mut cx, &window);

    rewrite_lens(&dir, "ab", "SELECT FROM WHERE");
    disk_tick(&window, &mut cx);

    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(2),
        "a broken program does not empty a graph"
    );
    let row = strip_lens(&window, &mut cx).unwrap_or_default();
    assert!(
        row.starts_with("lens ab · not answering: "),
        "the strip says the picture is standing, in the strip's own words: {row}"
    );
    assert_eq!(
        worn(&window, &mut cx),
        Some(("ab".to_string(), 2)),
        "the mask still holds the last good answer"
    );
}

/// EDGE-L11: the file goes away under a graph that is wearing it. The picture stands, the
/// window says the lens is gone, and the strip keeps saying it — clearing is the next
/// `:lens`, or a restart, not an event reaching into a live view.
#[gpui::test]
fn a_lens_deleted_from_under_the_graph_says_it_is_gone(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(
        cx,
        &[(
            "ab",
            "SELECT path AS note FROM notes WHERE path IN ('b.md', 'a.md') ORDER BY path",
        )],
    );
    on_graph(&mut cx, &window);
    command_line("lens ab", &mut cx);
    open_strip(&mut cx, &window);

    std::fs::remove_file(dir.path().join(".shale").join("lenses").join("ab.shq")).unwrap();
    disk_tick(&window, &mut cx);

    assert_eq!(
        say(&window, &mut cx),
        "lens `ab` is gone; the picture stays as it was"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(2),
        "deleting a lens is not deleting a picture"
    );
    let row = strip_lens(&window, &mut cx).unwrap_or_default();
    assert!(
        row.starts_with("lens ab · not answering: "),
        "and the strip keeps saying it after the sentence has scrolled away: {row}"
    );
}

// ------------------------------------------------------------------- the seat (M6)
//
// The session remembers a lens by **name**. It could remember the paths it answered, and
// that would be a lie by the next morning: the `.shq` file is the authority (EDGE-L1) and
// the vault moves, so the seat holds the question and the next window asks it again.

use shale_core::session::{GraphFilterSeat, Session, Tab};

/// A window restored over `session`, index on, with these lens files written before the
/// window opens. The notes are the graph's usual shape.
fn lens_session(
    cx: &mut TestAppContext,
    lenses: &[(&str, &str)],
    session: &Session,
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("a.md", "see [[b]] and [[gone]] and #tag\n"),
        ("b.md", "back to [[a]]\n"),
        ("c.md", "alone\n"),
    ] {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    write_lenses(&dir, lenses);
    session.save(dir.path()).expect("the session file");
    let window = open_window(cx, dir.path(), test_config());
    cx.run_until_parked();
    (dir, window)
}

/// A session whose only tab is the graph, wearing `lens` (or no lens at all).
fn graph_session(lens: Option<&str>) -> Session {
    Session {
        tabs: vec![Tab {
            path: None,
            view: String::new(),
            pinned: false,
            graph: true,
            camera: None,
            mode: None,
            orientation: None,
        }],
        active_tab: Some(0),
        graph_filter: Some(GraphFilterSeat {
            lens: lens.map(str::to_string),
            ..GraphFilterSeat::default()
        }),
        ..Session::default()
    }
}

/// What the seat would hold for the filter as it stands.
fn seat(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> GraphFilterSeat {
    window
        .update(cx, |ws, _window, cx| ws.session_snapshot(cx))
        .unwrap()
        .graph_filter
        .unwrap_or_default()
}

/// The seat keeps a lens's **name**, and nothing else: no paths, no count.
#[gpui::test]
fn the_seat_remembers_a_lens_by_name(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(
        cx,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'a.md'")],
    );
    on_graph(&mut cx, &window);
    assert_eq!(
        seat(&window, &mut cx).lens,
        None,
        "no lens, nothing to remember (EDGE-L9)"
    );

    command_line("lens ab", &mut cx);
    assert_eq!(
        seat(&window, &mut cx).lens.as_deref(),
        Some("ab"),
        "the seat holds the question, not the answer it gave"
    );
}

/// The next window wears it again — and answers for itself, from the file and the index
/// as they are now.
#[gpui::test]
fn a_worn_lens_comes_back_with_the_session(cx: &mut TestAppContext) {
    let (_dir, window) = lens_session(
        cx,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'a.md'")],
        &graph_session(Some("ab")),
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert_eq!(
        worn(&window, &mut cx),
        Some(("ab".to_string(), 1)),
        "the lens was re-asked, not replayed"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(1),
        "and the picture is under it"
    );
}

/// The seat is restored before any tab opens, and at that moment the vault index has not
/// read the vault yet — so a restored lens is worn **empty** for the length of one index
/// pass, and answers when the pass lands. That is the same door EDGE-L7 walks through for
/// a lens in a live window, and it is worth pinning because the alternative — replaying
/// the paths the last window saw — is the lie the seat refuses to tell.
#[gpui::test]
fn a_restored_lens_answers_when_the_index_arrives(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("a.md", "see [[b]] and [[gone]] and #tag\n"),
        ("b.md", "back to [[a]]\n"),
        ("c.md", "alone\n"),
    ] {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    write_lenses(
        &dir,
        &[("ab", "SELECT path AS note FROM notes WHERE path = 'a.md'")],
    );
    graph_session(Some("ab")).save(dir.path()).unwrap();
    let window = open_window(cx, dir.path(), test_config());

    let worn_now = window
        .update(cx, |ws, _window, _cx| {
            ws.graph_filter()
                .lens
                .as_ref()
                .map(|lens| (lens.name.clone(), lens.paths.len()))
        })
        .unwrap();
    assert_eq!(
        worn_now,
        Some(("ab".to_string(), 0)),
        "the lens is on the window the moment it is built, and holds nothing yet"
    );

    cx.run_until_parked();
    let later = window
        .update(cx, |ws, _window, _cx| {
            ws.graph_filter()
                .lens
                .as_ref()
                .map(|lens| (lens.name.clone(), lens.paths.len()))
        })
        .unwrap();
    assert_eq!(
        later,
        Some(("ab".to_string(), 1)),
        "and it answers with the index"
    );
}

/// The seat's name is not a promise the file keeps. A lens that will not answer leaves
/// the window without one and says which half failed — the same posture the door takes
/// (EDGE-L6), where "the picture stands" at startup can only mean the picture starts
/// unfiltered.
#[gpui::test]
fn a_lens_that_no_longer_answers_does_not_come_back(cx: &mut TestAppContext) {
    let (_dir, window) = lens_session(
        cx,
        &[("broken", "SELECT note FROM wherever")],
        &graph_session(Some("broken")),
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert_eq!(worn(&window, &mut cx), None, "no lens is worn");
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(3),
        "and the picture is the whole vault, not an empty one"
    );
}

/// A lens restored from the seat has not met a graph index yet when it is asked, so its
/// "with no node" count is filled in when the first index arrives — the strip's row is
/// where a reader would notice if it were not.
#[gpui::test]
fn a_restored_lens_counts_its_answers_with_no_node(cx: &mut TestAppContext) {
    let (_dir, window) = lens_session(
        cx,
        &[(
            "half",
            "SELECT 'nowhere.md' AS note UNION SELECT 'a.md' AS note",
        )],
        &graph_session(Some("half")),
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    assert_eq!(
        worn(&window, &mut cx),
        Some(("half".to_string(), 2)),
        "the answer is two paths, one of them a note this vault does not have"
    );
    open_strip(&mut cx, &window);
    assert_eq!(
        strip_lens(&window, &mut cx).as_deref(),
        Some("lens half · 1 note, 1 with no node"),
        "and the strip says so, measured against the index that arrived after the lens did"
    );
}

// ------------------------------------------------------------------- the save door
//
// M5: the way *in* to a lens is a picture the reader is already looking at. The filter
// line's question compiles to the SQL a hand-written `.shq` would hold — the same
// translation keep-a-search uses — so the lens answers what the line answered. The strip's
// chips are not encoded (ruling 1): they stay dimensions, and a lens never freezes a chip
// choice into a file its author will later edit by hand.

/// The name the prompt opened with, if a prompt is open.
fn prompt_name(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |ws, _window, cx| ws.prompt_text(cx))
        .unwrap()
}

/// The lens file's text, as it sits on disk.
fn lens_file(dir: &tempfile::TempDir, name: &str) -> String {
    let here = dir.path().join(".shale").join("lenses");
    let path = here.join(format!("{name}.shq"));
    let listing: Vec<String> = std::fs::read_dir(&here)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no {name}.shq; folder holds {listing:?}"))
}

/// Type into the filter line, with the strip open.
fn type_query(cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
}

/// `alt-s` in the strip, and the name accepted from the prompt it opens.
fn save_from_strip(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("alt-s");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

/// Empty the filter line, leaving the lens on: the picture then belongs to the lens alone.
fn clear_line(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) {
    cx.simulate_keystrokes("escape");
    open_strip(cx, window);
    // More backspaces than any line these tests type: an empty field ignores the rest.
    for _ in 0..32 {
        cx.simulate_keystrokes("backspace");
    }
    draw(cx);
}

/// The strip's `alt-s`: the line's question becomes a file under `.shale/lenses`, and the
/// graph puts it on right away (ruled 2026-11-18).
#[gpui::test]
fn the_strip_saves_its_question_as_a_lens(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    open_strip(&mut cx, &window);
    type_query(&mut cx, "tag:tag");

    cx.simulate_keystrokes("alt-s");
    assert_eq!(
        prompt_name(&window, &mut cx),
        Some("tag".to_string()),
        "the prompt starts with a name made from the question, so the reader edits one"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    let text = lens_file(&dir, "tag");
    assert!(text.contains(r#"name = "tag""#), "{text}");
    assert!(
        text.contains("-- Saved from the graph's filter line: tag:tag"),
        "the file says the question in the words it was typed: {text}"
    );
    assert!(
        text.contains("SELECT path AS note FROM notes n WHERE 1=1 AND EXISTS"),
        "and holds the statement those words compile to: {text}"
    );
    assert_eq!(
        say(&window, &mut cx),
        "saved lens `tag`: 1 note",
        "one sentence says both halves: it was saved, and it is on"
    );
    assert_eq!(worn(&window, &mut cx), Some(("tag".to_string(), 1)));
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(1),
        "the line asks the index here the way the search does, so the lens is saved from \
         a picture that means the same thing"
    );

    clear_line(&mut cx, &window);
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(1),
        "and taking the typed line out leaves the lens answering the same one note"
    );
}

/// `:lens save <name>` is the same door with the name already given — no prompt, because
/// the reader arrived having decided.
#[gpui::test]
fn a_lens_can_be_saved_with_its_name(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    open_strip(&mut cx, &window);
    type_query(&mut cx, "tag:tag");
    cx.simulate_keystrokes("escape");

    command_line("lens save tagged", &mut cx);

    assert!(
        dir.path()
            .join(".shale")
            .join("lenses")
            .join("tagged.shq")
            .is_file(),
        "the file landed in the lens folder"
    );
    assert_eq!(say(&window, &mut cx), "saved lens `tagged`: 1 note");
    assert_eq!(worn(&window, &mut cx), Some(("tagged".to_string(), 1)));
}

/// A question the index cannot be asked again is refused by name, before any file is
/// written — the same three refusals keep-a-search makes, in the same words.
#[gpui::test]
fn a_question_that_cannot_be_kept_is_refused(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    open_strip(&mut cx, &window);
    type_query(&mut cx, "b");

    cx.simulate_keystrokes("alt-s");
    assert_eq!(
        say(&window, &mut cx),
        "plain text searches the lines of notes, and a saved query asks the index",
        "the refusal is core's, said before a name is spent on it"
    );
    assert!(
        prompt_name(&window, &mut cx).is_none(),
        "and no prompt opens for a question that cannot be kept"
    );
    let here = dir.path().join(".shale").join("lenses");
    assert_eq!(
        std::fs::read_dir(here)
            .map(|entries| entries.count())
            .unwrap_or(0),
        0,
        "nothing was written"
    );
}

/// An empty line is not a question. The strip says which half is missing.
#[gpui::test]
fn an_empty_filter_line_saves_nothing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    open_strip(&mut cx, &window);
    cx.simulate_keystrokes("alt-s");
    assert_eq!(
        say(&window, &mut cx),
        "the filter line is empty: type the question first, or name one you have with \
         `:lens save <name>`"
    );

    command_line("lens save", &mut cx);
    assert_eq!(say(&window, &mut cx), "name it: `:lens save <name>`");
}

/// Ruling I, inherited: the file a reader wrote — and may have edited by hand, since it is
/// the authority — outranks the picture someone just saved over it.
#[gpui::test]
fn a_saved_lens_is_never_written_over(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    open_strip(&mut cx, &window);
    type_query(&mut cx, "tag:tag");
    save_from_strip(&mut cx);
    let first = lens_file(&dir, "tag");

    command_line("lens save tag", &mut cx);
    assert!(
        say(&window, &mut cx).contains("will not overwrite"),
        "the second save says what it refused to do: {}",
        say(&window, &mut cx)
    );
    assert_eq!(lens_file(&dir, "tag"), first, "and the first file stands");
}

/// Ruling 1, pinned where it matters: the chips are not encoded into the file. A chip that
/// was hiding something when the lens was saved is still a chip afterwards — switching it
/// back shows what it was hiding, inside the lens. `modified:2020-01-01` answers all three
/// notes (a duration would not save: it would mean something different the next time the
/// file ran), so the orphan the chip was hiding is inside the lens's answer and comes back
/// when the chip does; a lens that had frozen the chip would keep it out.
#[gpui::test]
fn the_strip_s_chips_stay_out_of_the_lens(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(3),
        "three notes, one of them an orphan"
    );

    open_strip(&mut cx, &window);
    type_query(&mut cx, "modified:2020-01-01");
    cx.simulate_keystrokes("alt-o"); // "orphans" off, at the moment of the save
    save_from_strip(&mut cx);

    let text = lens_file(&dir, "2020-01-01");
    assert!(
        text.contains("-- Saved from the graph's filter line: modified:2020-01-01"),
        "the file holds the question: {text}"
    );
    assert!(
        !text.contains("orphan") && !text.contains("degree"),
        "and nothing chip-shaped: {text}"
    );

    clear_line(&mut cx, &window);
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(2),
        "the chip is still a chip, cutting inside the lens (EDGE-L3)"
    );

    // The strip is still open from clearing the line, so the chip key goes straight in.
    cx.simulate_keystrokes("alt-o");
    draw(&mut cx);
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(3),
        "and switching the chip back shows the orphan again, under the same lens"
    );
}

/// A lens name is one name. The door the graph offers is the same door `:lens` reads
/// through, so `../` is refused as a path rather than resolved as one.
#[gpui::test]
fn a_lens_name_is_never_a_path(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    open_strip(&mut cx, &window);
    type_query(&mut cx, "tag:tag");
    cx.simulate_keystrokes("escape");

    command_line("lens save ../evil", &mut cx);
    assert!(
        say(&window, &mut cx).contains("one name, not a path"),
        "the refusal names what is wrong with the name: {}",
        say(&window, &mut cx)
    );
    assert!(
        !dir.path().parent().unwrap().join("evil.shq").exists(),
        "and nothing was written outside the lens folder"
    );
}

// ------------------------------------------------------- one grammar (P4.16 follow-up)
//
// The strip's line and the search's line are read one way: the operator tokens ask the
// index, and the words left over are what the node labels are held to. These two pin the
// seam that used to sit between them — a line the picture read as a string of letters and
// the save door read as a question, so that `tag:tag` emptied the graph and then saved a
// lens answering one note.

/// A line holding an operator is a question in the strip exactly as it is in the search,
/// which is what makes the lens saved from a picture agree with that picture.
#[gpui::test]
fn a_lens_saved_from_an_operated_line_agrees_with_the_picture(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(3),
        "the whole vault is on screen to begin with"
    );
    open_strip(&mut cx, &window);
    type_query(&mut cx, "tag:tag");
    draw(&mut cx);
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(1),
        "the line narrowed the picture by asking the index, not by holding its letters to labels"
    );

    save_from_strip(&mut cx);
    assert!(
        lens_file(&dir, "tag").contains("EXISTS"),
        "the file holds the same question the line asked"
    );
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(1),
        "and putting the lens on changes nothing you could see: the two agree"
    );
}

/// A line with words **and** operators says which half the file cannot carry: the lens
/// keeps the question, and the words stay a label filter the picture applies by itself.
#[gpui::test]
fn a_line_with_words_says_what_the_lens_leaves_out(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = lens_vault(cx, &[]);
    on_graph(&mut cx, &window);
    open_strip(&mut cx, &window);
    type_query(&mut cx, "tag:tag a");
    draw(&mut cx);
    assert_eq!(
        counts(&mut cx, &window).map(|c| c.notes),
        Some(1),
        "the answer chooses the note, and the word narrows inside it"
    );

    cx.simulate_keystrokes("alt-s");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(
        say(&window, &mut cx).contains("the line's words stay outside"),
        "the sentence names the half the lens cannot hold: {}",
        say(&window, &mut cx)
    );
}
