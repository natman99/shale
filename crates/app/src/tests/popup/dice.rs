//! The other card: the one a `!p` line opens.
//!
//! The chip at the end of the line answers "roughly how good?" in the room a margin has.
//! The card answers the longer question — the average, the range, the target's chance, and
//! the totals as bars when there are few enough to read — in the same surface a wiki-link
//! already uses, with no interactions of its own. What these tests pin is that the card is
//! offered by the **line** rather than by the chip (which can be shortened, or pushed into
//! the gutter), and that a line the grammar refuses still gets a card that says why.

use super::*;

/// Resting the pointer on a request line shows the distribution the margin has no room for.
#[gpui::test]
fn resting_on_a_request_line_shows_its_distribution(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", "!p 2d6 vs 8\nplain line\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    // Offset 3 is inside the `2d6` — the expression the note wrote, not the number the
    // app painted at the end of the line.
    hover_at(&mut cx, &window, 0, 3);
    let shown = card(&window, &mut cx).expect("a card for a line that asks a question");
    assert_eq!(
        shown.0, "2d6 vs 8",
        "titled with what the note asked, not how it was marked"
    );
    assert_eq!(
        shown.1[0], "avg 7 \u{00b7} 2\u{2013}12",
        "the summary first"
    );
    assert_eq!(shown.1[1], "41.7% \u{2265} 8", "then the target's chance");
    assert_eq!(shown.1.len(), 2 + 11, "and eleven totals, worth laying out");
    assert!(
        shown.1[2].starts_with("  2 \u{2588}"),
        "the lowest total, with its bar"
    );
    assert_eq!(shown.2, 0, "nothing was left out of eleven totals");
}

/// The card belongs to the line: the pointer can be anywhere on it, including the `!p`
/// itself, and it goes when the pointer leaves.
#[gpui::test]
fn the_card_follows_the_request_line_and_leaves_with_it(cx: &mut TestAppContext) {
    // Tall enough that a row can sit below the card: this one is a title and seven lines,
    // so its band reaches down past the first handful of rows under the request line.
    let host = format!(
        "!p 1d6\n{}",
        (1..=14)
            .map(|n| format!("filler line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let (_dir, window) = vault_with(cx, &[("host.md", &host)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 0);
    assert!(
        card(&window, &mut cx).is_some(),
        "the marker is part of the line the card is about"
    );
    // Row 1 is where the card itself hangs, and the band it occupies is its own: a pointer
    // that has to cross it is travelling to the card, not asking a new question. Row 3 is
    // clear of it.
    hover_at(&mut cx, &window, 12, 2);
    assert_eq!(
        card(&window, &mut cx),
        None,
        "the pointer left the line, so the card left"
    );
}

/// A line the grammar refuses gets the sentence the chip had no room for. This is the
/// other half of the gutter mark: the mark says something was declined, and the card says
/// what and why.
#[gpui::test]
fn a_refused_request_says_why_in_the_card(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", "!p 100000d100000\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 4);
    let shown = card(&window, &mut cx).expect("a refusal is still an answer");
    assert_eq!(shown.1.len(), 1, "one sentence, no numbers");
    assert!(
        !shown.1[0].is_empty(),
        "and it names what was declined: {:?}",
        shown.1[0]
    );
}

/// A `!p` inside a block being edited belongs to the note that holds the block, in another
/// buffer — so the pointer inside that block is not over this view's request line.
#[gpui::test]
fn a_request_inside_another_note_s_block_is_not_this_cards_business(cx: &mut TestAppContext) {
    // The host has a request line of its own, so a hit test that ignored the block
    // boundary would read the pointer's offset out of the host's text and find it.
    let (_dir, window) = vault_with(
        cx,
        &[("host.md", "!p 2d6\n![[gift]]\n"), ("gift.md", "!p 3d6\n")],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    // Into the block: the caret is in gift.md's line, and so is the pointer. Painted row
    // 2 is that line — the host's two rows are above it.
    cx.simulate_keystrokes("j ctrl-]");
    paint(&mut cx);
    hover_at(&mut cx, &window, 2, 3);
    assert_eq!(
        card(&window, &mut cx),
        None,
        "the host's chips are the host's lines; the block's belong to its own note"
    );
}
