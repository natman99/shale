//! Agent edit review (roadmap P2.5a): the queue of proposals an MCP profile left, the
//! strip that shows them, and what accepting or rejecting one does to the note and to
//! the files.
//!
//! The proposals are written by `shale-mcp` (see `crates/mcp/src/pending_tools.rs`), so
//! the tests write them the same way — through [`shale_core::pending`] — rather than
//! inventing a second door into the queue.

use super::*;
use shale_core::diff::hunks;
use shale_core::pending::{self, PendingEdit};
use shale_core::Session;

/// A note with enough lines that a change in the middle carries context, which is what
/// a stale badge is about.
pub(super) const NOTE: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n";
/// …and the same note with one line changed.
pub(super) const EDITED: &str = "one\ntwo\nTHREE\nfour\nfive\nsix\nseven\neight\n";

/// Queue one proposal against `note`, the way the MCP server does: a file in
/// `.shale/pending-edits/` carrying the hunks that turn `current` into `proposed`.
pub(super) fn queue(root: &Path, note: &str, current: &str, proposed: &str) -> PendingEdit {
    queued(root, note, current, proposed, "writing-help")
}

/// The same, with the profile named, for the tests that care whose proposal it is.
pub(super) fn queued(
    root: &Path,
    note: &str,
    current: &str,
    proposed: &str,
    profile: &str,
) -> PendingEdit {
    let edit = PendingEdit {
        id: PendingEdit::new_id(profile, "write_note", 1_700_000_000),
        note: PathBuf::from(note),
        profile: profile.to_string(),
        tool: "write_note".to_string(),
        created_at: 1_700_000_000,
        hunks: hunks(current, proposed),
    };
    pending::write(root, &edit).expect("queueing the proposal");
    edit
}

/// Where a queued proposal lives.
pub(super) fn queue_file(root: &Path, id: &str) -> PathBuf {
    pending::dir(root).join(format!("{id}.toml"))
}

/// Read the queue now, rather than waiting on the beat.
pub(super) fn scan(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) {
    window
        .update(cx, |ws, _window, cx| ws.scan_pending(cx))
        .unwrap();
}

/// The rows the strip is drawing.
pub(super) fn rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.review_rows(cx))
        .unwrap()
}

/// Whether the strip is on screen at all.
pub(super) fn strip_up(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |ws, _window, cx| ws.review_active(cx))
        .unwrap()
}

/// The buffer, which is where an accepted hunk lands when the note is open.
pub(super) fn buffer(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap()
}

/// The status line, where every review command says what it did or refused.
pub(super) fn say(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// Open `a.md` in the window.
pub(super) fn show(cx: &mut TestAppContext, window: &WindowHandle<Workspace>) {
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx)
        })
        .unwrap();
}

/// Type a command line and press enter.
pub(super) fn command_line(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// A window over a vault holding `NOTE` in `a.md`, with the note open and one proposal
/// against it queued. The strip is up and the marker is on the hunk.
pub(super) fn one_proposal(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(cx, &[("a.md", NOTE)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    queue(dir.path(), "a.md", NOTE, EDITED);
    scan(&window, &mut cx);
    (dir, window, cx)
}

/// A proposal shows a row, and the strip appears by itself: the queue is non-empty, so
/// there is something to review, and a proposal nobody sees is the failure this feature
/// exists to prevent.
#[gpui::test]
fn a_queued_proposal_shows_a_row(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    assert!(strip_up(&window, &mut cx), "the strip is up");
    assert_eq!(rows(&window, &mut cx), vec!["a.md hunk 0"]);
    assert!(
        !buffer(&window, &mut cx).contains("THREE"),
        "nothing has been applied"
    );
}

/// The strip is **raised**, not more chrome (`design-language.md` §5): it paints the
/// raised pair — `popup_bg` behind it and `popup_border` ruling it — rather than the
/// status bar's background, which is what used to make it read as a second status bar
/// sitting under a note it had just been asked about. Pinned here because nothing else in
/// the suite looks at the strip's colours, and §5 is a rule rather than a preference.
#[gpui::test]
fn the_review_strip_paints_the_raised_pair(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    // The first frame has no measured size yet, so paint more than once.
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let (want_bg, want_border) = window
        .update(&mut cx, |ws, _window, _cx| {
            (
                gpui::Hsla::from(gpui::rgb(ws.palette().popup_bg)),
                gpui::Hsla::from(gpui::rgb(ws.palette().popup_border)),
            )
        })
        .unwrap();
    let same = |a: gpui::Hsla, b: gpui::Hsla| {
        (a.h - b.h).abs() < 0.001 && (a.s - b.s).abs() < 0.001 && (a.l - b.l).abs() < 0.001
    };
    let quads = window
        .update(&mut cx, |_ws, window, _cx| window.painted_quads())
        .unwrap();
    assert!(
        quads
            .iter()
            .any(|quad| quad.background.as_solid().is_some_and(|c| same(c, want_bg))),
        "the strip is painted on `popup_bg`"
    );
    assert!(
        quads
            .iter()
            .any(|quad| quad.border_widths.top.as_f32() > 0.0
                && same(quad.border_color, want_border)),
        "the strip rules itself with `popup_border`"
    );
}

/// The strip's rule and the colours it is drawn with: `(bounds, border, background)`.
type Rule = (gpui::Bounds<gpui::ScaledPixels>, gpui::Hsla, gpui::Hsla);

/// The strip's own rule: the one painted quad that is a wide, short box with a border
/// on it. The key chips are boxed too, but they are key-sized; the strip's rule is the
/// width of the window, so a width threshold separates them. Bounds are **device**
/// pixels, which is why the threshold is 500 rather than the logical window width.
///
/// gpui draws a box's fill and its rule as two quads over the same bounds, so the
/// background is read off the quad that shares the bordered quad's bounds rather than
/// off the bordered quad itself, whose own background is empty.
///
/// Returns `(bounds, border_colour, background)`.
fn strip_rule(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Rule {
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let quads = window
        .update(cx, |_ws, window, _cx| window.painted_quads())
        .unwrap();
    let found = quads
        .iter()
        .filter(|quad| quad.border_widths.top.as_f32() > 0.0)
        .find(|quad| quad.bounds.size.width.as_f32() > 500.0);
    let quad = found.expect("the strip rules itself with a box the width of the window");
    let bounds = quad.bounds;
    let near = |a: f32, b: f32| (a - b).abs() <= 1.0;
    let fill = quads
        .iter()
        .filter(|other| {
            near(other.bounds.origin.x.as_f32(), bounds.origin.x.as_f32())
                && near(other.bounds.origin.y.as_f32(), bounds.origin.y.as_f32())
                && near(other.bounds.size.width.as_f32(), bounds.size.width.as_f32())
                && near(
                    other.bounds.size.height.as_f32(),
                    bounds.size.height.as_f32(),
                )
        })
        .find_map(|other| other.background.as_solid())
        .expect("the strip fills its own box");
    (bounds, quad.border_color, fill)
}

/// Colours compare by hue/sat/light, not bit-for-bit: gpui converts a palette's packed
/// `u32` through f32 on the way to the screen, and the same colour asked for twice does
/// not always come back with identical bits.
fn same_colour(a: gpui::Hsla, b: gpui::Hsla) -> bool {
    (a.h - b.h).abs() < 0.001 && (a.s - b.s).abs() < 0.001 && (a.l - b.l).abs() < 0.001
}

/// The strip is painted with the palette **on screen**, not the one the window happened
/// to be built with. It caches its colours the way the editor, the picker and the graph
/// do, so the theme fan-out has to include it; when it was left out the strip kept the
/// startup palette and looked as though it ignored the theme completely.
#[gpui::test]
fn the_strip_follows_a_theme_change(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    let before = window
        .update(&mut cx, |ws, _window, _cx| {
            (
                gpui::Hsla::from(gpui::rgb(ws.palette().popup_bg)),
                gpui::Hsla::from(gpui::rgb(ws.palette().popup_border)),
            )
        })
        .unwrap();
    let (_, border, bg) = strip_rule(&window, &mut cx);
    assert!(same_colour(border, before.1), "it starts on its own theme");
    assert!(same_colour(bg, before.0), "and starts on its own theme");

    cx.simulate_keystrokes("escape :");
    cx.simulate_input("theme gruvbox-dark");
    cx.simulate_keystrokes("enter");
    let after = window
        .update(&mut cx, |ws, _window, _cx| {
            (
                gpui::Hsla::from(gpui::rgb(ws.palette().popup_bg)),
                gpui::Hsla::from(gpui::rgb(ws.palette().popup_border)),
            )
        })
        .unwrap();
    assert!(
        !same_colour(after.0, before.0),
        "the theme did change the popup background"
    );

    let (_, border, bg) = strip_rule(&window, &mut cx);
    assert!(
        same_colour(bg, after.0),
        "the strip is painted on the new theme's `popup_bg`"
    );
    assert!(
        same_colour(border, after.1),
        "and ruled with the new theme's `popup_border`"
    );
}

/// The strip sits **above** the status bar, which is what its module doc has always
/// claimed and what the mount order did not do: the strip was added to the window's
/// column after the bar, so it painted below it.
#[gpui::test]
fn the_strip_sits_above_the_status_bar(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let (bar_bg, height) = window
        .update(&mut cx, |ws, window, _cx| {
            (
                gpui::Hsla::from(gpui::rgb(ws.palette().ui_status_bg)),
                window.bounds().size.height.as_f32(),
            )
        })
        .unwrap();
    let (strip, _, _) = strip_rule(&window, &mut cx);
    let quads = window
        .update(&mut cx, |_ws, window, _cx| window.painted_quads())
        .unwrap();
    // The tab bar wears the same role as the status bar, so the bar is the *lowest*
    // band drawn with it.
    let bar = quads
        .iter()
        .filter(|quad| {
            quad.background
                .as_solid()
                .is_some_and(|c| same_colour(c, bar_bg))
        })
        .map(|quad| quad.bounds)
        .max_by_key(|band| band.origin.y.as_f32() as i64)
        .expect("the status bar is painted");

    // Everything here is in **device** pixels, and the test window is at 2×, so the
    // comparisons are in f32 with a couple of pixels of slack rather than in exact
    // logical lengths.
    let strip_y = strip.origin.y.as_f32();
    let strip_bottom = strip_y + strip.size.height.as_f32();
    let bar_y = bar.origin.y.as_f32();
    let bar_bottom = bar_y + bar.size.height.as_f32();

    assert!(
        strip_y < bar_y,
        "the strip (y={strip_y}) is above the status bar (y={bar_y})"
    );
    assert!(
        strip_bottom <= bar_y + 2.0,
        "and the two bands do not overlap: the strip ends at {strip_bottom}, the bar starts at {bar_y}"
    );
    assert!(
        bar_bottom >= height - 2.0,
        "the status bar is still the last thing in the window (ends at {bar_bottom}, window is {height})"
    );
}

/// The strip says when it holds the keyboard. Its rule turns accent while it has focus
/// and goes back to `popup_border` when the note has it again: the same `y` accepts a
/// hunk in one state and yanks a character in the other, and before this there was
/// nothing on screen that told the user which they were about to do.
#[gpui::test]
fn the_strip_shows_when_it_has_the_keyboard(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    let (accent, quiet) = window
        .update(&mut cx, |ws, _window, _cx| {
            (
                gpui::Hsla::from(gpui::rgb(ws.palette().accent)),
                gpui::Hsla::from(gpui::rgb(ws.palette().popup_border)),
            )
        })
        .unwrap();

    // The note has the keyboard: the rule is the quiet raised-panel line.
    let (_, border, _) = strip_rule(&window, &mut cx);
    assert!(same_colour(border, quiet), "not focused: the quiet rule");

    command_line("review", &mut cx);
    let (_, border, _) = strip_rule(&window, &mut cx);
    assert!(same_colour(border, accent), "focused: the rule is accent");

    cx.simulate_keystrokes("escape");
    let (_, border, _) = strip_rule(&window, &mut cx);
    assert!(
        same_colour(border, quiet),
        "and back to quiet when escape gives the keyboard to the note"
    );
}
/// The legend is **boxed chips**, not a run of prose (`design-language.md` §9). Five
/// key-sized rules are drawn on the strip: that is what makes `j/k` read as a key rather
/// than as the start of a label, and it is the treatment §9 asks for wherever keys are
/// named. Pinned at the size rather than the count, because a chip's width is the key's
/// width and the strip's own rule is the width of the window. The bounds are **device**
/// pixels, so the thresholds are comfortably above the ~43 px a chip is at 2× and well
/// below the 3840 × 100 the strip's own rule is.
#[gpui::test]
fn the_strip_legends_its_keys_with_chips(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let want_border = window
        .update(&mut cx, |ws, _window, _cx| {
            gpui::Hsla::from(gpui::rgb(ws.palette().popup_border))
        })
        .unwrap();
    let chips = window
        .update(&mut cx, |_ws, window, _cx| {
            window
                .painted_quads()
                .into_iter()
                .filter(|quad| {
                    quad.border_widths.top.as_f32() > 0.0
                        && (quad.border_color.h - want_border.h).abs() < 0.001
                        && (quad.border_color.l - want_border.l).abs() < 0.001
                        && quad.bounds.size.width.as_f32() < 200.0
                        && quad.bounds.size.height.as_f32() < 80.0
                })
                .count()
        })
        .unwrap();
    assert!(
        chips >= 5,
        "the five legend keys are boxed: {chips} chip-sized rules were painted"
    );
}

/// `:review` gives the strip the keyboard, which is what the quit prompt points at.
#[gpui::test]
fn review_takes_the_keyboard_and_escape_gives_it_back(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = one_proposal(cx);
    command_line("review", &mut cx);
    // With the strip focused, `i` is not insert mode: it is unbound there.
    cx.simulate_keystrokes("i");
    assert_eq!(buffer(&window, &mut cx), NOTE);
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" typed");
    assert_eq!(
        buffer(&window, &mut cx),
        // `A` appends to the end of the line the cursor is on, which is the note's
        // first — the point here is only that the note has the keyboard again.
        NOTE.replace("one\n", "one typed\n"),
        "escape handed the keyboard back to the note"
    );
}

/// `y` accepts the hunk: the change goes into the buffer — not onto the disk, because
/// the buffer is what the user is looking at and the save path is the save path — and
/// the queue file goes with it.
#[gpui::test]
fn accepting_a_hunk_edits_the_buffer_and_clears_the_file(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = one_proposal(cx);
    let id = pending::list(dir.path()).edits.remove(0).id;
    command_line("review", &mut cx);
    cx.simulate_keystrokes("y");
    assert_eq!(buffer(&window, &mut cx), EDITED);
    assert!(
        !queue_file(dir.path(), &id).exists(),
        "the queue file went with the hunk"
    );
    assert!(
        !strip_up(&window, &mut cx),
        "and the strip went with the file"
    );
    assert!(
        say(&window, &mut cx).contains("accepted"),
        "{}",
        say(&window, &mut cx)
    );
}

/// `x` rejects the hunk. Nothing is written in either direction: the note keeps what it
/// had, and the proposal stops being a proposal.
#[gpui::test]
fn rejecting_a_hunk_leaves_the_note_alone(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = one_proposal(cx);
    let id = pending::list(dir.path()).edits.remove(0).id;
    command_line("review", &mut cx);
    cx.simulate_keystrokes("x");
    assert_eq!(buffer(&window, &mut cx), NOTE, "the note was never touched");
    assert!(!queue_file(dir.path(), &id).exists());
    assert!(!strip_up(&window, &mut cx));
    assert!(say(&window, &mut cx).contains("rejected"));
}

/// The apply-or-stale policy: a hunk whose context the user has walked over cannot be
/// applied, is badged as stale, and accepting it changes nothing at all.
#[gpui::test]
fn a_hunk_the_user_walked_over_is_stale(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = one_proposal(cx);
    let id = pending::list(dir.path()).edits.remove(0).id;
    // The user edits the very line the proposal is about.
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.editor().unwrap().update(cx, |editor, cx| {
                editor.set_text(NOTE.replace("three", "three, mine"), cx);
            });
        })
        .unwrap();
    scan(&window, &mut cx);
    assert_eq!(rows(&window, &mut cx), vec!["a.md hunk 0 stale"]);

    command_line("review", &mut cx);
    cx.simulate_keystrokes("y");
    assert_eq!(
        buffer(&window, &mut cx),
        NOTE.replace("three", "three, mine"),
        "accepting a stale hunk changes nothing"
    );
    assert!(
        queue_file(dir.path(), &id).exists(),
        "and the proposal is still there to be rejected"
    );
    assert!(say(&window, &mut cx).contains("no longer fits"));
}

/// Two proposals over the same lines are flagged, because the user is the one who has
/// to decide which of them lands first.
#[gpui::test]
fn two_proposals_over_the_same_lines_are_flagged(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = one_proposal(cx);
    queued(
        dir.path(),
        "a.md",
        NOTE,
        &NOTE.replace("three", "three?"),
        "other-help",
    );
    scan(&window, &mut cx);
    let shown = rows(&window, &mut cx);
    assert_eq!(shown.len(), 2, "one row per hunk");
    assert!(
        shown.iter().all(|row| row.contains("clash")),
        "both are flagged: {shown:?}"
    );
}

/// The marker is one thing, reached by two layers of keys: bare `j`/`k` while the strip
/// has the keyboard, and `]a`/`[a` while the note does.
#[gpui::test]
fn the_marker_hops_from_either_side(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", LONG)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    queue(dir.path(), "a.md", LONG, &long_edits());
    scan(&window, &mut cx);
    assert_eq!(rows(&window, &mut cx).len(), 2);

    // From the note, with no `:review` and no focus on the strip.
    cx.simulate_keystrokes("] a");
    assert_eq!(marker(&window, &mut cx), Some(("a.md".into(), Some(1))));
    cx.simulate_keystrokes("[ a");
    assert_eq!(marker(&window, &mut cx), Some(("a.md".into(), Some(0))));

    // From the strip, where the same letters are bare.
    command_line("review", &mut cx);
    cx.simulate_keystrokes("j");
    assert_eq!(marker(&window, &mut cx), Some(("a.md".into(), Some(1))));
    cx.simulate_keystrokes("k");
    assert_eq!(marker(&window, &mut cx), Some(("a.md".into(), Some(0))));
}

/// The marker wraps, so a queue of two is not a queue you have to count.
#[gpui::test]
fn the_marker_wraps_at_the_ends(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", LONG)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    queue(dir.path(), "a.md", LONG, &long_edits());
    scan(&window, &mut cx);
    cx.simulate_keystrokes("] a");
    cx.simulate_keystrokes("] a");
    assert_eq!(
        marker(&window, &mut cx),
        Some(("a.md".into(), Some(0))),
        "past the end is back at the beginning"
    );
}

/// Accepting one hunk of a two-hunk proposal leaves the other pending, re-anchored by
/// the exact line shift the accepted hunk caused — so it does not go stale merely
/// because something above it moved.
#[gpui::test]
fn accepting_one_hunk_of_two_keeps_the_other_re_anchored(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", LONG)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    let edit = queue(dir.path(), "a.md", LONG, &long_edits());
    scan(&window, &mut cx);
    command_line("review", &mut cx);
    cx.simulate_keystrokes("y");

    assert_eq!(
        buffer(&window, &mut cx),
        LONG.replace("line 2\n", "TWO\nTWO-2\n"),
        "the first hunk landed"
    );
    let left = pending::read(&queue_file(dir.path(), &edit.id)).unwrap();
    assert_eq!(left.hunks.len(), 1, "the second is still pending");
    assert_eq!(
        rows(&window, &mut cx),
        vec!["a.md hunk 0"],
        "and it is still offered, not stale"
    );

    cx.simulate_keystrokes("y");
    assert_eq!(buffer(&window, &mut cx), long_edits(), "and it applies");
    assert!(
        !queue_file(dir.path(), &edit.id).exists(),
        "the file goes when its last hunk does"
    );
}

/// `za` collapses a proposal to one row, and `y` on that row is the whole proposal:
/// "yes, do what you suggested" should be one key rather than six.
#[gpui::test]
fn collapsing_a_proposal_makes_y_answer_all_of_it(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", LONG)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    let edit = queue(dir.path(), "a.md", LONG, &long_edits());
    scan(&window, &mut cx);
    command_line("review", &mut cx);
    cx.simulate_keystrokes("z a");
    assert_eq!(rows(&window, &mut cx), vec!["a.md collapsed"]);
    cx.simulate_keystrokes("y");
    assert_eq!(buffer(&window, &mut cx), long_edits());
    assert!(
        !queue_file(dir.path(), &edit.id).exists(),
        "both hunks, one key"
    );
}

/// A proposal for a note that is **not** open has no buffer to land in, so it is
/// written to disk — and the index is told, which is what keeps the graph and
/// `:wordcount` from going stale on an accept.
#[gpui::test]
fn accepting_a_proposal_for_a_closed_note_writes_the_disk(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", NOTE), ("b.md", NOTE)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    queue(dir.path(), "b.md", NOTE, EDITED);
    scan(&window, &mut cx);
    command_line("review", &mut cx);
    cx.simulate_keystrokes("y");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.md")).unwrap(),
        EDITED,
        "the closed note got the change"
    );
    assert_eq!(buffer(&window, &mut cx), NOTE, "the open note did not");
}

/// A proposal that creates a note: the folder it asks for is made, and the note is
/// written whole.
#[gpui::test]
fn accepting_a_proposal_for_a_new_note_makes_the_note(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", NOTE)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    queue(dir.path(), "inbox/new.md", "", "a thought\n");
    scan(&window, &mut cx);
    command_line("review", &mut cx);
    cx.simulate_keystrokes("y");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("inbox/new.md")).unwrap(),
        "a thought\n"
    );
}

/// `o` opens the note the marker is about, so the change can be read where it belongs.
#[gpui::test]
fn o_opens_the_note_the_hunk_is_about(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("a.md", NOTE), ("b.md", NOTE)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    queue(dir.path(), "b.md", NOTE, EDITED);
    scan(&window, &mut cx);
    command_line("review", &mut cx);
    cx.simulate_keystrokes("o");
    window
        .update(&mut cx, |ws, _window, _cx| {
            assert_eq!(
                ws.active_path(),
                Some(PathBuf::from("b.md")),
                "the proposal's note is on screen"
            );
        })
        .unwrap();
}

/// Quitting with a proposal unanswered is refused, once, with the way out named. The
/// files are on disk either way, so nothing is lost — but walking away from an
/// unanswered proposal without knowing you did is the thing this feature is for.
#[gpui::test]
fn q_refuses_while_a_proposal_is_waiting(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = one_proposal(cx);
    let id = pending::list(dir.path()).edits.remove(0).id;
    command_line("q", &mut cx);
    assert!(
        queue_file(dir.path(), &id).exists(),
        "the proposal is still queued"
    );
    let session = Session::load(dir.path()).unwrap();
    assert_eq!(
        session.open_note, None,
        "and the window is still open: {session:?}"
    );
    assert!(
        say(&window, &mut cx).contains(":review"),
        "the bar names the way to look: {}",
        say(&window, &mut cx)
    );
}

/// `:q!` is the way out the refusal points at: the exit happens, and the queue stays
/// exactly where it was for the next window to offer again.
#[gpui::test]
fn q_bang_leaves_the_queue_for_the_next_window(cx: &mut TestAppContext) {
    let (dir, _window, mut cx) = one_proposal(cx);
    let id = pending::list(dir.path()).edits.remove(0).id;
    command_line("q!", &mut cx);
    let session = Session::load(dir.path()).unwrap();
    assert_eq!(
        session.open_note.as_deref(),
        Some(Path::new("a.md")),
        "the exit happened"
    );
    assert!(
        queue_file(dir.path(), &id).exists(),
        "and the proposal is waiting for the next window"
    );
}

/// `:review` with nothing waiting says so rather than focusing a strip that is not
/// drawn, which would leave the keyboard somewhere invisible.
#[gpui::test]
fn review_with_nothing_waiting_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", NOTE)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    command_line("review", &mut cx);
    assert!(!strip_up(&window, &mut cx));
    assert!(say(&window, &mut cx).contains("nothing is waiting"));
    // The keyboard is still the note's, so the next key does what it always did.
    cx.simulate_keystrokes("A");
    cx.simulate_input(" typed");
    assert!(buffer(&window, &mut cx).starts_with("one typed"));
}

/// A note in preview mode refuses edits, so an accept there is refused out loud rather
/// than accepted into a buffer that would quietly drop it.
#[gpui::test]
fn preview_mode_refuses_an_accept(cx: &mut TestAppContext) {
    let (dir, window, mut cx) = one_proposal(cx);
    let id = pending::list(dir.path()).edits.remove(0).id;
    cx.simulate_keystrokes("escape space p");
    command_line("review", &mut cx);
    cx.simulate_keystrokes("y");
    assert_eq!(buffer(&window, &mut cx), NOTE, "nothing was applied");
    assert!(
        queue_file(dir.path(), &id).exists(),
        "and the proposal is still queued"
    );
    assert!(say(&window, &mut cx).contains("preview"));
}

/// The strip stays a strip. A twelve-hunk proposal does not turn it into a panel that
/// pushes the note off screen: it draws its cap, and `j` slides the drawn window along
/// the queue while keeping the marked row on it.
#[gpui::test]
fn a_long_queue_stays_a_short_strip(cx: &mut TestAppContext) {
    let note: String = (1..=80).map(|n| format!("line {n}\n")).collect();
    // Changes seven lines apart, so each is a hunk of its own: the diff merges hunks
    // whose context touches, at twice the context width.
    let edited = [2, 9, 16, 23, 30, 37, 44, 51, 58, 65, 72, 79]
        .into_iter()
        .fold(note.clone(), |text, n| {
            text.replace(&format!("line {n}\n"), &format!("CHANGED {n}\n"))
        });
    let (dir, window) = vault_with(cx, &[("a.md", note.as_str())]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show(&mut cx, &window);
    queue(dir.path(), "a.md", &note, &edited);
    scan(&window, &mut cx);

    let all = window
        .update(&mut cx, |ws, _window, cx| ws.review_hunks(cx))
        .unwrap();
    assert_eq!(all, 12, "the proposal really is twelve hunks");

    command_line("review", &mut cx);
    assert!(strip_up(&window, &mut cx));
    assert_eq!(
        rows(&window, &mut cx).len(),
        crate::review::VISIBLE_ROWS,
        "the strip draws its cap, not the queue"
    );

    // Walking the whole queue: never more than the cap on screen, and the row the
    // keyboard is on is always one of them — otherwise `y` would act on a row the
    // user cannot see.
    for _ in 0..=12 {
        let drawn = rows(&window, &mut cx);
        let marked = marker(&window, &mut cx).expect("a row is marked");
        let label = format!(
            "{} hunk {}",
            marked.0,
            marked.1.expect("the queue is not collapsed")
        );
        assert!(
            drawn.len() <= crate::review::VISIBLE_ROWS && drawn.contains(&label),
            "marked {label} is not on a {drawn:?} strip"
        );
        cx.simulate_keystrokes("j");
    }
}

// ---- the helpers the tests above share -----------------------------------

/// A note long enough that two changes far apart are two hunks rather than one.
const LONG: &str = "\
line 1
line 2
line 3
line 4
line 5
line 6
line 7
line 8
line 9
line 10
line 11
line 12
line 13
line 14
line 15
line 16
line 17
line 18
line 19
line 20
";

/// `LONG` with one change near the top and one near the end: two hunks.
fn long_edits() -> String {
    LONG.replace("line 2\n", "TWO\nTWO-2\n")
        .replace("line 18\n", "SEVEN\n")
}

/// The marker, as the strip sees it.
fn marker(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<(String, Option<usize>)> {
    window
        .update(cx, |ws, _window, cx| ws.review_marker(cx))
        .unwrap()
}

/// The queue is read once per vault and handed to every window on it. It used to be a
/// beat per window, so three windows over one vault read the same almost-always-empty
/// folder three times a second — three times the work, not three opinions about it.
/// What stays per window is the *judgement*: a hunk is stale against that window's
/// buffers, which may be holding a note another window has already written out.
///
/// The two windows come through the registry, because that is what shares a vault's
/// services: `Workspace::new` makes its own, so a test that opened two windows that way
/// would have two beats and prove nothing.
#[gpui::test]
fn one_queue_reaches_every_window_on_the_vault(cx: &mut TestAppContext) {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("vault");
    std::fs::create_dir_all(root.join(".shale")).unwrap();
    std::fs::write(root.join("One.md"), NOTE).unwrap();
    std::fs::write(
        root.join(".shale").join("config.toml"),
        "[spellcheck]\nenabled = false\n[index]\nenabled = false\n[mcp]\nlive = false\n",
    )
    .unwrap();
    cx.update(|cx| {
        cx.bind_keys(keybindings());
        cx.set_global(crate::workspace::vaults::VaultWindows::new(
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        ));
    });

    let a = cx
        .update(|cx| {
            crate::workspace::vaults::open_vault_window(
                &root,
                crate::workspace::vaults::OpenOptions::default(),
                cx,
            )
        })
        .unwrap();
    let b = cx
        .update(|cx| crate::workspace::vaults::open_extra_window(&root, cx))
        .unwrap();

    // The proposal arrives after both windows have done their startup scan, so what
    // puts it on screen can only be the vault's one beat.
    queue(&root, "One.md", NOTE, EDITED);
    let clock = Clock(cx.executor());
    {
        let mut vcx = VisualTestContext::from_window(a.into(), cx);
        clock.pause(&mut vcx, Duration::from_millis(1_200));
    }

    let strip_up = |w: &WindowHandle<Workspace>, cx: &mut TestAppContext| {
        w.update(cx, |ws, _window, cx| ws.review_active(cx))
            .unwrap()
    };
    assert!(
        strip_up(&a, cx),
        "the window the beat was driven through sees it"
    );
    assert!(
        strip_up(&b, cx),
        "and so does the other window on the same vault, unasked"
    );
}
