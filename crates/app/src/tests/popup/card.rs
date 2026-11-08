//! The hover preview card: what a link points at, opened by the pointer and dismissed
//! by it, and never taking the keyboard.
//!
//! What these tests pin is the card's *behaviour*, not its pixels: that it opens on a
//! link and only on a link that means a note, that it says what the link means rather
//! than what the author typed, that it goes away when the pointer does, and that the
//! note keeps the keyboard the whole time.

use super::*;

/// The note on screen, as its buffer holds it.
fn note_text(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor()
                .unwrap()
                .read_with(cx, |editor, _cx| editor.text())
        })
        .unwrap()
}

/// What the card says: the title, the lines, and how many it left out. `None` when
/// there is no card.
fn card(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<(String, Vec<String>, usize)> {
    window
        .update(cx, |ws, _window, cx| ws.card_shape(cx))
        .unwrap()
}

const HOST: &str = "see [[gift]] here\nplain line\n";
const GIFT: &str = "alpha\nbeta\n";

/// Resting the pointer on a wiki-link shows the note it points at.
#[gpui::test]
fn resting_on_a_link_shows_what_it_points_at(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    // Offset 8 is inside the `[[gift]]` on the first line.
    hover_at(&mut cx, &window, 0, 8);
    let shown = card(&window, &mut cx).expect("a card for a link that means a note");
    assert_eq!(shown.0, "gift", "named the way the vault names it");
    assert_eq!(shown.1, vec!["alpha", "beta"], "the note's own lines");
    assert_eq!(shown.2, 0, "nothing was left out of a two-line note");
}

/// A link to a note the vault does not have gets no card: the card would be
/// promising a note that is not there.
#[gpui::test]
fn a_link_to_nothing_shows_nothing(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", "see [[nowhere]] here\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    assert_eq!(card(&window, &mut cx), None, "no note, no card");
}

/// The card belongs to the link: moving off it takes the card away, which is what
/// keeps a card from sitting over the note after the pointer has moved on.
#[gpui::test]
fn the_card_goes_when_the_pointer_leaves_the_link(cx: &mut TestAppContext) {
    // The link is on the *second* line so the first line is somewhere the card is not:
    // the card hangs below its link, and a line underneath it is the card's own band,
    // which is where a reader sits to read it.
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "plain line\nsee [[gift]] here\n"),
            ("gift.md", GIFT),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    // Offset 16 is inside the `[[gift]]` that starts at byte 14 of the second line:
    // `hover_at` asks for a buffer offset, not a column.
    hover_at(&mut cx, &window, 1, 16);
    assert!(card(&window, &mut cx).is_some(), "up on the link");
    hover_at(&mut cx, &window, 0, 2);
    assert_eq!(card(&window, &mut cx), None, "gone off the link");
}

/// The card hangs under the link, not at the beginning of the line the link is written
/// on: a card in the middle of a sentence should point at the link, not at the sentence's
/// first word.
#[gpui::test]
fn the_card_hangs_under_the_link(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    let placed = window
        .update(&mut cx, |ws, window, cx| {
            ws.card_place(window, cx).expect("the card has a place")
        })
        .unwrap()
        .bounds;
    // `[[gift]]` starts at byte 4 of the first line.
    let link_x = x_over(&mut cx, &window, 0, 4);
    let (line_left, _, _) = line_box(&mut cx, &window, 0);
    assert_eq!(placed.origin.x, link_x, "under the link's first glyph");
    assert!(
        placed.origin.x > line_left,
        "and not at the line's left edge, which is where \"see\" starts"
    );
}

/// `escape` closes the card. The card belongs to the pointer and has no keyboard of its
/// own, so a reader who starts typing without moving the mouse would otherwise be working
/// under a picture of another note.
#[gpui::test]
fn escape_closes_the_card(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    assert!(card(&window, &mut cx).is_some(), "up on the link");
    cx.simulate_keystrokes("escape");
    assert_eq!(card(&window, &mut cx), None, "escape took it away");
}

/// Scrolling moves text under a pointer that never moved, and gpui reports no mouse move
/// for it — so the pointer's question is asked again against the new paint, and a card
/// about a link that is no longer under the pointer goes away.
#[gpui::test]
fn scrolling_asks_the_pointer_again(cx: &mut TestAppContext) {
    // The link is five lines down and the note goes on for eighty more, so a three-line
    // scroll puts different text under the pointer while the link's own line is still on
    // screen. That is the case the old rule misses: it drops a card whose line left the
    // viewport, and this one did not.
    let tall = format!(
        "{}see [[gift]] here\n{}",
        "filler line\n".repeat(5),
        "filler line\n".repeat(80)
    );
    let (_dir, window) = vault_with(cx, &[("host.md", tall.as_str()), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    // Byte 68 is inside the `[[gift]]` that starts at byte 64 of the sixth line (five
    // twelve-byte filler lines come first).
    hover_at(&mut cx, &window, 5, 68);
    assert!(card(&window, &mut cx).is_some(), "up on the link");
    // Three lines' worth of scroll, measured from where two rows were painted.
    let (_, top0, _) = line_box(&mut cx, &window, 0);
    let (_, top3, _) = line_box(&mut cx, &window, 3);
    let step = (top3 - top0) / 3.;
    window
        .update(&mut cx, |ws, _w, cx| {
            ws.editor()
                .unwrap()
                .update(cx, |e, _cx| e.scroll_by(step * 3.))
        })
        .unwrap();
    // The re-ask is queued for the next frame, which is when the scrolled paint has
    // recorded where its rows went.
    repaint(&mut cx);
    paint(&mut cx);
    assert_eq!(
        card(&window, &mut cx),
        None,
        "the pointer is over filler now, and the card knows it"
    );
}

/// The card never takes the keyboard. It is opened by a pointer crossing a link, and
/// a surface that ate the keys at that moment would be worse than no card: the reader
/// is reading, and their next keystroke is meant for the note.
#[gpui::test]
fn the_card_leaves_the_note_the_keyboard(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    assert!(card(&window, &mut cx).is_some(), "the card is up");
    assert!(
        note_has_keys(&mut cx, &window),
        "and the note still has keys"
    );
    // A keystroke goes to the note, which is the whole point: `dw` deletes a word
    // rather than doing nothing while a card holds the keyboard.
    cx.simulate_keystrokes("d w");
    assert_eq!(
        note_text(&window, &mut cx),
        "[[gift]] here\nplain line\n",
        "the keystroke reached the note"
    );
}

/// A section link previews the section, not the top of the note — the same rule an
/// embed and `gd` follow, so a card, a block and a jump cannot disagree about what
/// `[[note#heading]]` means.
#[gpui::test]
fn a_section_link_previews_its_section(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "see [[gift#Two]] here\n"),
            ("gift.md", "# One\nfirst\n# Two\nsecond\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    let shown = card(&window, &mut cx).expect("a card for the section");
    assert_eq!(
        shown.0, "gift › Two",
        "the card says which half it is showing"
    );
    assert_eq!(shown.1, vec!["Two", "second"], "and shows it");
}

/// The card reads as the note reads, not as the file is written: the same rendering
/// rule the note's own rows and an embed's rows follow.
#[gpui::test]
fn the_card_hides_the_markers(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "see [[gift]] here\n"),
            ("gift.md", "# Heading and **bold**\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    let shown = card(&window, &mut cx).expect("a card");
    assert_eq!(
        shown.1,
        vec!["Heading and bold"],
        "a heading in the card is a heading, not a `#`"
    );
}

/// A card that cannot show the whole note says so, rather than letting a preview look
/// like the whole of it.
#[gpui::test]
fn a_long_note_says_what_was_left_out(cx: &mut TestAppContext) {
    let long: String = (1..=12).map(|n| format!("line {n}\n")).collect();
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "see [[gift]] here\n"),
            ("gift.md", long.as_str()),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    let shown = card(&window, &mut cx).expect("a card");
    assert_eq!(shown.1.len(), 8, "the card's ceiling");
    assert_eq!(shown.2, 4, "and it says what it left off");
}

/// The card shows the note as Shale has it in hand, not as the disk has it — the same
/// rule an embedded block follows, so one window cannot display two versions of one
/// note at once.
#[gpui::test]
fn the_card_shows_the_note_as_it_is_being_edited(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    // The target note is open and changed, unsaved — and left open in a tab, which is
    // what makes it a note Shale has in hand rather than a file on disk.
    show_at(&mut cx, &window, "gift.md");
    // `repaint`, not `paint`: a new tab re-hooks the pointer's subscription on the next
    // frame, and only `repaint` runs what a frame queued.
    repaint(&mut cx);
    cx.simulate_keystrokes("ctrl-t");
    repaint(&mut cx);
    cx.simulate_keystrokes("o gamma");
    cx.simulate_keystrokes("escape");
    // The host takes the second tab, and the pointer goes to the link.
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    eprintln!("NOTE: {:?}", note_text(&window, &mut cx));
    hover_at(&mut cx, &window, 0, 8);
    let shown = card(&window, &mut cx).expect("a card");
    assert_eq!(
        shown.1,
        vec!["alpha", "gamma", "beta"],
        "the card shows the edit, not the file"
    );
}
