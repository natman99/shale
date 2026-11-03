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
const NOTE: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n";
/// …and the same note with one line changed.
const EDITED: &str = "one\ntwo\nTHREE\nfour\nfive\nsix\nseven\neight\n";

/// Queue one proposal against `note`, the way the MCP server does: a file in
/// `.shale/pending-edits/` carrying the hunks that turn `current` into `proposed`.
fn queue(root: &Path, note: &str, current: &str, proposed: &str) -> PendingEdit {
    queued(root, note, current, proposed, "writing-help")
}

/// The same, with the profile named, for the tests that care whose proposal it is.
fn queued(root: &Path, note: &str, current: &str, proposed: &str, profile: &str) -> PendingEdit {
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
fn queue_file(root: &Path, id: &str) -> PathBuf {
    pending::dir(root).join(format!("{id}.toml"))
}

/// Read the queue now, rather than waiting on the beat.
fn scan(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) {
    window
        .update(cx, |ws, _window, cx| ws.scan_pending(cx))
        .unwrap();
}

/// The rows the strip is drawing.
fn rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.review_rows(cx))
        .unwrap()
}

/// Whether the strip is on screen at all.
fn strip_up(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |ws, _window, cx| ws.review_active(cx))
        .unwrap()
}

/// The buffer, which is where an accepted hunk lands when the note is open.
fn buffer(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap()
}

/// The status line, where every review command says what it did or refused.
fn say(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// Open `a.md` in the window.
fn show(cx: &mut TestAppContext, window: &WindowHandle<Workspace>) {
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx)
        })
        .unwrap();
}

/// Type a command line and press enter.
fn command_line(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// A window over a vault holding `NOTE` in `a.md`, with the note open and one proposal
/// against it queued. The strip is up and the marker is on the hunk.
fn one_proposal(
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
