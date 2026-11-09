//! The inline review overlay (roadmap P2.5b): the same queued proposals the strip
//! lists, painted in the note they belong to.
//!
//! Split out from `tests/review.rs` because the strip and the note are different
//! surfaces with different rules, and the two sets of tests read better apart. The
//! queue fixtures are shared from there: a proposal is queued the way `shale-mcp`
//! queues one, through [`shale_core::pending`], rather than through a test-only door.

use super::review::{buffer, one_proposal, queue, rows, say, scan, show, strip_up, EDITED, NOTE};
use super::*;
use shale_core::pending;

// ---------------------------------------------------------------------------
// P2.5b: the same proposals, painted in the note they belong to.
//
// The strip is the authoritative surface; the overlay is opportunistic. What these
// tests pin is the ruled shape of that: a removal is the note's own text struck
// through, an addition is a phantom row the buffer never held, and the keys over
// those lines belong to the proposal rather than to vim.
// ---------------------------------------------------------------------------

/// The overlay as the note on screen would paint it.
fn overlay(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.overlay_shape(cx))
        .unwrap()
}

/// How many display lines the note on screen is laid out as.
fn display_rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| ws.display_line_count(cx))
        .unwrap()
}

/// A note long enough that two changes keep their own context: hunks carry three lines
/// either side, so in an eight-line note two changes would merge into one hunk and
/// "which hunk did the key mean?" would have no answer to test.
const FAR: &str = "alpha\nbravo\ncharlie\ndelta\necho\nfoxtrot\ngolf\nhotel\nindia\njuliet\nkilo\nlima\nmike\nnovember\n";
/// \u2026with the second line and the last changed, far enough apart to stay two hunks.
const FAR_EDITED: &str = "alpha\nBRAVO\ncharlie\ndelta\necho\nfoxtrot\ngolf\nhotel\nindia\njuliet\nkilo\nlima\nmike\nNOVEMBER\n";

/// A window over a vault holding `NOTE` in `a.md`, the note open, one proposal queued,
/// and the caret parked on the line the proposal would take away.
fn proposal_under_the_caret(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window, mut cx) = one_proposal(cx);
    cx.simulate_keystrokes("3 G");
    (dir, window, cx)
}

/// A queued proposal is painted in the note: the line it would take away struck, and the
/// line it would bring sitting under that line as a phantom row.
#[gpui::test]
fn a_queued_proposal_is_painted_in_the_note(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    assert_eq!(
        overlay(&window, &mut cx),
        vec![
            "hunk 0: \u{2212}1 +1 [marked]",
            "  strike row 2",
            "  before row 3",
            "    + THREE",
        ],
        "the struck line and the phantom line, in the order the note draws them"
    );
    assert!(
        !buffer(&window, &mut cx).contains("THREE"),
        "painting a proposal applies nothing to the buffer"
    );
}

/// A hunk that no longer fits the note is not painted. The strip still lists it, badged
/// stale, and the note shows nothing: an overlay offering a change the accept would
/// refuse is worse than no overlay.
#[gpui::test]
fn a_stale_hunk_is_not_painted(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    // Take out the line the hunk uses as context, from a line the proposal says
    // nothing about, so the key is vim's.
    cx.simulate_keystrokes("g g d d");
    assert!(strip_up(&window, &mut cx), "the strip still lists it");
    assert_eq!(
        overlay(&window, &mut cx),
        Vec::<String>::new(),
        "and the note paints nothing for it"
    );
}

/// The phantom rows are not in the display model: the note is laid out as the buffer
/// counts itself, and the overlay hangs off that rather than joining it. This is the
/// fact every other consequence rests on \u2014 `j` and `k` skip the additions, the ruler
/// reads the buffer's line, search cannot match one.
#[gpui::test]
fn the_phantom_rows_are_not_in_the_display_model(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", NOTE)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    let before = display_rows(&window, &mut cx);
    queue(dir.path(), "a.md", NOTE, EDITED);
    scan(&window, &mut cx);
    assert_eq!(
        overlay(&window, &mut cx).len(),
        4,
        "the overlay is painting a phantom row"
    );
    assert_eq!(
        display_rows(&window, &mut cx),
        before,
        "the note is laid out exactly as it was; the phantom row is not one more"
    );
    // `j` steps from the struck line to the buffer's next line, not into the phantom.
    cx.simulate_keystrokes("3 G j");
    let row = window
        .update(&mut cx, |ws, _window, cx| ws.cursor_position(cx).0)
        .unwrap();
    assert_eq!(row, 3, "the caret lands on `four`, past the addition");
}

/// The bare `y` over a line under review accepts that hunk. The note has the keyboard,
/// so the key belongs to the proposal rather than to vim's yank.
#[gpui::test]
fn y_over_a_struck_line_accepts_it(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = proposal_under_the_caret(cx);
    cx.simulate_keystrokes("y");
    assert!(
        buffer(&window, &mut cx).contains("THREE"),
        "the hunk landed in the buffer, where the user is looking"
    );
    assert!(
        say(&window, &mut cx).contains("accepted"),
        "and the note says so: {:?}",
        say(&window, &mut cx)
    );
    assert!(!strip_up(&window, &mut cx), "leaving nothing to review");
}

/// The bare `x` over a line under review rejects that hunk: the queue file goes and the
/// note is left exactly as it was.
#[gpui::test]
fn x_over_a_struck_line_rejects_it(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = proposal_under_the_caret(cx);
    cx.simulate_keystrokes("x");
    assert_eq!(buffer(&window, &mut cx), NOTE, "nothing was applied");
    assert!(
        pending::list(dir.path()).edits.is_empty(),
        "and the proposal was taken out of the queue"
    );
    assert!(!strip_up(&window, &mut cx), "leaving nothing to review");
}

/// Everywhere else, `y` is vim's yank. The two-layer rule is a fence around the lines a
/// proposal speaks for, not a takeover of the note.
#[gpui::test]
fn y_over_an_ordinary_line_is_still_vims_yank(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    cx.simulate_keystrokes("y y p");
    assert_eq!(
        buffer(&window, &mut cx),
        "one\none\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n",
        "the line was yanked and pasted, which is vim's `y y p`"
    );
    assert!(
        strip_up(&window, &mut cx),
        "and the proposal is still queued, untouched"
    );
}

/// A count means the user is spelling a vim command, so vim wins even over a line under
/// review: `2y` is the start of `2yy`, not two accepts.
#[gpui::test]
fn a_count_over_a_struck_line_belongs_to_vim(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = proposal_under_the_caret(cx);
    cx.simulate_keystrokes("2 y");
    assert_eq!(buffer(&window, &mut cx), NOTE, "nothing was applied");
    assert!(strip_up(&window, &mut cx), "and nothing was accepted");
}

/// `za` over a line under review folds the proposal's additions away to a count, in the
/// note. The strip's own collapse of the same proposal is a different widget and is left
/// alone.
#[gpui::test]
fn za_over_a_struck_line_collapses_the_additions(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = proposal_under_the_caret(cx);
    cx.simulate_keystrokes("z a");
    assert_eq!(
        overlay(&window, &mut cx),
        vec![
            "hunk 0: \u{2212}1 +1 [collapsed] [marked]",
            "  strike row 2",
        ],
        "the phantom row is gone; the struck line stays, because it is the note's own text"
    );
    assert_eq!(
        rows(&window, &mut cx),
        vec!["a.md hunk 0"],
        "and the strip still shows the hunk as its own row"
    );
    cx.simulate_keystrokes("z a");
    assert_eq!(
        overlay(&window, &mut cx).len(),
        4,
        "and `za` again brings the additions back"
    );
}

/// `za` anywhere else is vim's fold. The overlay only owns the key over its own lines.
#[gpui::test]
fn za_over_an_ordinary_line_is_still_vims_fold(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    cx.simulate_keystrokes("z a");
    assert_eq!(
        overlay(&window, &mut cx).len(),
        4,
        "the additions are still painted in full"
    );
}

/// Two hunks, and the caret on the second: the note's `y` accepts *that* one, and the
/// strip's marker moves to it. The strip's `y` means the marked hunk; the note's means
/// the hunk under the caret — and afterwards the strip is pointing at what the user just
/// acted on, which is the honest thing for it to show.
#[gpui::test]
fn the_notes_y_names_the_hunk_under_the_caret(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", FAR)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    queue(dir.path(), "a.md", FAR, FAR_EDITED);
    scan(&window, &mut cx);
    assert_eq!(rows(&window, &mut cx), vec!["a.md hunk 0", "a.md hunk 1"]);
    // The caret on the second hunk's line, while the strip's marker sits on the first.
    cx.simulate_keystrokes("1 4 G");
    assert_eq!(
        overlay(&window, &mut cx),
        vec![
            "hunk 0: \u{2212}1 +1 [marked]",
            "  strike row 1",
            "  before row 2",
            "    + BRAVO",
            "hunk 1: \u{2212}1 +1",
            "  strike row 13",
            "  before row 14",
            "    + NOVEMBER",
        ],
        "both hunks painted, and only the strip's is marked"
    );
    cx.simulate_keystrokes("y");
    let text = buffer(&window, &mut cx);
    assert!(text.contains("NOVEMBER"), "the hunk under the caret landed");
    assert!(
        !text.contains("BRAVO"),
        "and the one under the marker did not"
    );
    assert_eq!(
        rows(&window, &mut cx),
        vec!["a.md hunk 0"],
        "leaving only the other hunk queued"
    );
}
