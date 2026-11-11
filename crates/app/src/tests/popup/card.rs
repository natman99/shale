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

/// How far one display row is, measured from two painted ones.
fn row_step(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) -> gpui::Pixels {
    let (_, top0, _) = line_box(cx, window, 0);
    let (_, top4, _) = line_box(cx, window, 4);
    (top4 - top0) / 4.
}

/// Scroll the note by `rows` display rows, and run the frames that follow: the re-ask is
/// queued for the next frame, which is when the scrolled paint has recorded where its rows
/// went.
fn scroll_rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, rows: f32) {
    let step = row_step(cx, window);
    window
        .update(cx, |ws, _w, cx| {
            ws.editor()
                .unwrap()
                .update(cx, |e, _cx| e.scroll_by(step * rows))
        })
        .unwrap();
    repaint(cx);
    paint(cx);
}

/// Where the card is on screen.
fn card_box(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
) -> gpui::Bounds<gpui::Pixels> {
    window
        .update(cx, |ws, window, cx| {
            ws.card_place(window, cx).expect("the card has a place")
        })
        .unwrap()
        .bounds
}

/// Scrolling moves the text under a pointer that never moved, and gpui reports no mouse
/// move for it. The card follows its link — its place is worked out from the line every
/// frame — and then the pointer is asked again what it is on.
#[gpui::test]
fn the_card_follows_its_link_when_the_note_scrolls(cx: &mut TestAppContext) {
    let tall = format!(
        "{}see [[gift]] here\n{}",
        "filler line\n".repeat(5),
        "filler line\n".repeat(80)
    );
    let (_dir, window) = vault_with(cx, &[("host.md", tall.as_str()), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    // Byte 68 is inside the `[[gift]]` that starts at byte 64 of the sixth line.
    hover_at(&mut cx, &window, 5, 68);
    let before = card_box(&window, &mut cx).top();
    scroll_rows(&window, &mut cx, 3.);
    let after = card_box(&window, &mut cx).top();
    assert!(
        card(&window, &mut cx).is_some(),
        "the pointer is still on the card, which is a valid place for it to be"
    );
    let step = row_step(&mut cx, &window);
    let moved = before - after;
    assert!(
        (moved - step * 3.).abs() < step,
        "the card moved with its link: {moved:?} for three rows of {step:?}"
    );
}

/// A pointer that ends up on neither the link nor the card ends it. The card follows its
/// link, so a pointer that was on the link ends up *inside* the card after a short scroll
/// and below it after a scroll past the card's own height. The link is twelve rows down, so
/// its row is still on screen afterwards: the rule that drops a surface whose anchor left
/// the viewport cannot be what closed this one.
#[gpui::test]
fn scrolling_off_the_link_closes_the_card(cx: &mut TestAppContext) {
    let tall = format!(
        "{}see [[gift]] here\n{}",
        "filler line\n".repeat(12),
        "filler line\n".repeat(70)
    );
    let (_dir, window) = vault_with(cx, &[("host.md", tall.as_str()), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    // Byte 150 is inside the link on the thirteenth line (twelve twelve-byte rows first).
    hover_at(&mut cx, &window, 12, 12 * 12 + 6);
    assert!(card(&window, &mut cx).is_some(), "up on the link");
    // Past the card's own height, so the pointer ends up under it rather than in it.
    let placed = card_box(&window, &mut cx);
    let step = row_step(&mut cx, &window);
    let rows = (placed.bottom() - placed.top()) / step + 2.;
    scroll_rows(&window, &mut cx, rows);
    assert!(
        line_box(&mut cx, &window, 4).2 > gpui::px(0.),
        "the link's row is still on screen"
    );
    assert_eq!(
        card(&window, &mut cx),
        None,
        "the pointer is below the card now, and the card knows it"
    );
}

/// The band that keeps a card alive while the pointer travels to it is the card's own
/// width, not the window's. A pointer at the card's height on the far side of the note is
/// on the way to nothing.
#[gpui::test]
fn the_card_band_is_the_cards_own_width(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    let placed = card_box(&window, &mut cx);
    hover_point(
        &mut cx,
        gpui::point(placed.right() + gpui::px(80.), placed.top() + gpui::px(10.)),
    );
    assert_eq!(card(&window, &mut cx), None, "beside the card is not on it");
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
    hover_at(&mut cx, &window, 0, 8);
    let shown = card(&window, &mut cx).expect("a card");
    assert_eq!(
        shown.1,
        vec!["alpha", "gamma", "beta"],
        "the card shows the edit, not the file"
    );
}

/// P4.9 M3: a block link previews its **block**, not the whole note, and the card's
/// title carries the caret so the reader sees which kind of link they hovered.
#[gpui::test]
fn resting_on_a_block_link_shows_that_block(cx: &mut TestAppContext) {
    let host = "see [[gift#^goblin]] here\n";
    let gift = "intro\n\nThe goblin waits\nby the door. ^goblin\n";
    let (_dir, window) = vault_with(cx, &[("host.md", host), ("gift.md", gift)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    paint(&mut cx);
    hover_at(&mut cx, &window, 0, 8);
    let shown = card(&window, &mut cx).expect("a block link is still a link-shaped thing");
    assert_eq!(
        shown.0, "gift \u{203a} ^goblin",
        "the title says which anchor"
    );
    assert_eq!(
        shown.1,
        vec!["The goblin waits", "by the door. ^goblin"],
        "the block's lines, and not the note's"
    );
}
