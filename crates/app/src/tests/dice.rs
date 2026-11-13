//! The `!p` dice chip (roadmap P4.15 M2, `surfaces-plan.md`): what a line asks for, what
//! the paint answers, and what it refuses to answer.
//!
//! These tests read the readouts a frame **painted**, not the strings the editor could
//! have painted: a chip that is computed but never reaches the screen is a bug of exactly
//! the kind this surface has to survive. The arithmetic itself is pinned in
//! `shale-core`'s `dice` tests; what is pinned here is the wording, the marker, and the
//! places a chip does not belong.

use super::*;

/// A note on screen at a chosen text size. Zoom is the honest lever for "there is no room
/// beside this line": the test window's width is fixed by the harness, and a zoomed note
/// has the same problem a narrow split pane has — the readout is wider than what is left.
fn shown_at(text: &str, size: f32, cx: &mut TestAppContext) -> gpui::WindowHandle<MarkdownEditor> {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let view = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(view, |editor, window, cx| {
            editor.set_text(text.to_string(), cx);
            if size > 0. {
                editor.set_font_size(size, cx);
            }
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        view.update(|window, cx| window.draw(cx).clear(cx));
    }
    editor
}

/// The ordinary window.
fn shown(text: &str, cx: &mut TestAppContext) -> gpui::WindowHandle<MarkdownEditor> {
    shown_at(text, 0.0, cx)
}

/// Rows whose readout fit neither length, so the gutter's die mark is speaking instead.
fn guttered<C: gpui::AppContext>(editor: &gpui::WindowHandle<MarkdownEditor>, cx: &C) -> usize {
    editor
        .read_with(cx, |editor, _cx| {
            editor
                .hit_lines
                .iter()
                .filter(|line| line.chip_gutter)
                .count()
        })
        .unwrap()
}

/// The readouts the last frame painted, in the order the rows were painted.
fn painted<C: gpui::AppContext>(
    editor: &gpui::WindowHandle<MarkdownEditor>,
    cx: &C,
) -> Vec<String> {
    editor
        .read_with(cx, |editor, _cx| {
            editor
                .hit_lines
                .iter()
                .filter_map(|line| line.chip.as_ref().map(|chip| chip.to_string()))
                .collect()
        })
        .unwrap()
}

/// An attack roll says what it is in the table's own word, and the average and the crit
/// come along because a hit chance on its own is half the decision.
#[gpui::test]
fn an_attack_roll_reads_out_its_chance(cx: &mut TestAppContext) {
    let editor = shown("!p d20+3 vs AC16\n", cx);
    assert_eq!(
        painted(&editor, cx),
        vec!["40% hits · avg 13.5 · crit 5%"],
        "meeting 16 with d20+3 is 8 faces in 20, and a natural 20 is 1 in 20"
    );
}

/// `DC` is an alias, not a second rule: the numbers are identical and only the verb moves.
#[gpui::test]
fn a_check_says_passes_with_the_same_numbers(cx: &mut TestAppContext) {
    let editor = shown("!p d20+3 vs DC 16\n", cx);
    assert_eq!(
        painted(&editor, cx),
        vec!["40% passes · avg 13.5 · crit 5%"],
        "the same roll against the same number, in a check's vocabulary"
    );
}

/// Ruling D: with nothing to meet, the chip gives the average and the range, and does not
/// pretend to a probability nobody asked for.
#[gpui::test]
fn a_roll_with_no_target_reads_out_an_average_and_a_range(cx: &mut TestAppContext) {
    let editor = shown("!p 2d8+4\n", cx);
    assert_eq!(painted(&editor, cx), vec!["avg 13 · 6–20"]);
}

/// A bare threshold claims neither verb, so the chip states the relation instead.
#[gpui::test]
fn a_bare_threshold_states_the_relation(cx: &mut TestAppContext) {
    let editor = shown("!p 2d6 vs 8\n", cx);
    assert_eq!(
        painted(&editor, cx),
        vec!["41.7% ≥ 8 · avg 7"],
        "15 of 36 outcomes meet 8, and the decimal is what keeps it exact-looking rather than wrong"
    );
}

/// Advantage is the order statistic, not a second d20 added to the pool: 64% rather than
/// the 40% a single die gives, and a crit that is two dice wide instead of one.
#[gpui::test]
fn advantage_raises_the_chance_it_shows(cx: &mut TestAppContext) {
    let editor = shown("!p 2d20kh1+3 vs AC16\n", cx);
    assert_eq!(
        painted(&editor, cx),
        vec!["64% hits · avg 16.8 · crit 9.8%"],
        "the higher of two d20s meets 13 on 11 faces in 20, and shows a 20 on 3.8%"
    );
}

/// The exceptions outrank the arithmetic in both directions: a roll that cannot reach the
/// number still hits on a natural 20.
#[gpui::test]
fn a_roll_that_cannot_reach_the_number_still_hits_on_a_twenty(cx: &mut TestAppContext) {
    let editor = shown("!p 1d20-5 vs AC25\n", cx);
    assert_eq!(
        painted(&editor, cx),
        vec!["5% hits · avg 5.5 · crit 5%"],
        "no face reaches 25 with a -5, so the only hit left is the natural 20"
    );
}

/// Ruling A: the marker is the line's first word. A dice-looking token in a sentence, and
/// a word that merely starts with the marker, are ordinary writing.
#[gpui::test]
fn prose_that_only_looks_like_a_request_gets_no_chip(cx: &mut TestAppContext) {
    let editor = shown(
        "Roll 2d8+4 for the fireball.\n!private notes about the lair\nsee !p in the margin\n",
        cx,
    );
    assert_eq!(
        painted(&editor, cx),
        Vec::<String>::new(),
        "nothing here asked; a chip on a sentence would be a chip on half the note"
    );
}

/// A line that does not compute says so where the number would have been. A silent line
/// cannot be told apart from a line the app never noticed.
#[gpui::test]
fn a_refusal_speaks_where_the_number_would_be(cx: &mut TestAppContext) {
    let editor = shown("!p 4d6kh9\n", cx);
    let chips = painted(&editor, cx);
    assert_eq!(chips.len(), 1, "the line still gets a readout");
    assert_eq!(
        chips[0], "cannot keep 9 of 4: the roll has fewer dice than that",
        "the refusal names what it declined"
    );
}

/// A `!p` inside a fenced block is a line of a snippet, not a request: code blocks hold
/// other people's text, and the chip would be commentary nobody asked for.
#[gpui::test]
fn a_chip_does_not_paint_inside_a_code_fence(cx: &mut TestAppContext) {
    let editor = shown("```\n!p d20 vs AC10\n```\n", cx);
    assert_eq!(
        painted(&editor, cx),
        Vec::<String>::new(),
        "the fence is the note quoting something else"
    );
}

/// The readout is recomputed when the line changes, and the old number does not linger:
/// the cache is keyed to the text, not to the frame.
#[gpui::test]
fn editing_the_line_changes_the_readout(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let view = &mut VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(view, |editor, window, cx| {
            editor.set_text("!p d20+3 vs AC16\n".to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        view.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(painted(&editor, view).len(), 1);
    editor
        .update(view, |editor, _window, cx| {
            editor.set_text("!p d20+3 vs AC21\n".to_string(), cx);
        })
        .unwrap();
    for _ in 0..2 {
        view.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(
        painted(&editor, view),
        vec!["15% hits · avg 13.5 · crit 5%"],
        "faces 18, 19 and 20 meet 21 with +3, and the chip followed the edit"
    );
}

/// A chip belongs to the line that asked, not to the row it happens to land on: a note
/// with several requests paints one per line, each with its own numbers.
#[gpui::test]
fn each_request_gets_its_own_readout(cx: &mut TestAppContext) {
    let editor = shown("!p d20+5 vs AC18\n!p d20+2 vs DC14\n!p 3d6\n", cx);
    assert_eq!(
        painted(&editor, cx),
        vec![
            "40% hits · avg 15.5 · crit 5%",
            "45% passes · avg 12.5 · crit 5%",
            "avg 10.5 · 3–18",
        ],
        "three lines, three readouts, none of them borrowed from a neighbour"
    );
}

/// A pane too narrow for the whole readout gives the one figure that matters rather than
/// a clipped sentence. The line itself is short — a `!p` line is an expression and nothing
/// else — so this is the split-pane case, not the long-prose case.
#[gpui::test]
fn a_narrow_pane_takes_the_short_form(cx: &mut TestAppContext) {
    // Zoomed far enough that the whole readout — about 30 glyphs — no longer fits beside
    // the 16 glyphs of the expression.
    let editor = shown_at("!p d20+3 vs AC16\n", 70., cx);
    assert_eq!(
        painted(&editor, cx),
        vec!["40%"],
        "the chance survives; the average and the crit wait for the hover card"
    );
    assert_eq!(
        guttered(&editor, cx),
        0,
        "the short form fit, so the gutter stays quiet"
    );
}

/// An average-only roll's short form says what the number *is*: a bare `13` in a narrow
/// pane would read as part of the note.
#[gpui::test]
fn a_narrow_pane_keeps_the_average_labelled(cx: &mut TestAppContext) {
    let editor = shown_at("!p 2d8+4\n", 160., cx);
    assert_eq!(painted(&editor, cx), vec!["avg 13"]);
}
