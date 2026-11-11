//! The jump chain: `ctrl-o` back to the note the user came from, `ctrl-i` forward
//! again. The chain itself is [`crate::workspace::jumps`]'s business; what is
//! checked here is the wiring through the window — that following a link records
//! the place being left, that coming back lands on the same byte of it, and that
//! the chain follows the vault when a note is deleted.

use super::*;

/// A note with a wiki-link on its second line. The link does not start at the
/// start of the line, so a chain that remembered only the line would not be
/// caught by these tests.
const A: &str = "line one\nsee [[b]] here\nline three\n";

/// The byte offset of the `[[b]]` span in [`A`].
const LINK: usize = 13;

/// Where the cursor is in the open note, as a buffer byte offset.
fn cursor(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor().unwrap().read(cx).cursor_offset()
        })
        .unwrap()
}

/// What the window said about the last jump attempt.
fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// How far the chain reaches: `(back, forward)`.
fn lens(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    window
        .update(cx, |ws, _window, _cx| ws.jump_lens())
        .unwrap()
}

/// The open note, or `""` while none is.
fn active(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    state(window, cx).1
}

/// How many rows the picker is offering, or `None` while none is open.
fn picker_rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<usize> {
    window
        .update(cx, |ws, _window, cx| ws.picker_matches(cx))
        .unwrap()
}

/// A two-note vault, with `a.md` open at its first byte.
fn two_notes(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    WindowHandle<Workspace>,
    VisualTestContext,
) {
    let (dir, window) = vault_with(cx, &[("a.md", A), ("b.md", "the linked note\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    open_a(&mut cx, &window);
    (dir, window, cx)
}

/// Opening `a.md` the way quick-open does, with the cursor on the link.
fn open_a(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) {
    cx.simulate_keystrokes("space f enter");
    assert_eq!(active(window, cx), "a.md", "the top row is `a.md`");
    cx.simulate_keystrokes("j l l l l");
    assert_eq!(
        cursor(window, cx),
        LINK,
        "the cursor is on the `[[b]]` span"
    );
}

/// A link to an **alias** is a link to the note that declares it. The vault has no
/// `anon.md`: the name only exists inside `masked.md`'s `aliases:` list, and the index
/// is what knows that.
#[gpui::test]
fn a_link_to_an_alias_opens_the_note_that_declares_it(cx: &mut TestAppContext) {
    // Indexed on purpose: the alias rule reads the index, so this is the first test
    // that asks whether the window's link colouring and the index agree at all.
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("host.md", "Met [[anon]] in the dark.\n"),
            (
                "masked.md",
                "---\naliases:\n  - anon\n---\nThe masked one.\n",
            ),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_keystrokes("space f enter");
    assert_eq!(active(&window, &mut cx), "host.md");
    // The colour first: the window has to believe the link reaches a note before it
    // paints it as anything but a dead end.
    assert!(
        window
            .update(&mut cx, |ws, _, _| ws.knows_link("anon"))
            .unwrap(),
        "the alias is in the set the editor styles links with"
    );
    assert!(
        !window
            .update(&mut cx, |ws, _, _| ws.knows_link("nobody"))
            .unwrap(),
        "and a name nothing declares is still a dead end"
    );
    // Then the jump: `f a` onto the link's text, vim's follow.
    cx.simulate_keystrokes("f a");
    cx.simulate_keystrokes("g d");
    assert_eq!(
        active(&window, &mut cx),
        "masked.md",
        "`gd` on `[[anon]]` landed on the note that declares the alias"
    );
}

/// Following a wiki-link is a jump, and `ctrl-o` returns to the exact byte of the
/// link the user was reading. `ctrl-i` walks back out along the same path.
#[gpui::test]
fn a_link_then_ctrl_o_returns_to_the_same_byte(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x");
    assert_eq!(active(&window, cx), "b.md", "`g x` followed the link");
    assert_eq!(
        lens(&window, cx),
        (1, 0),
        "and left one place to come back to"
    );

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(active(&window, cx), "a.md", "`ctrl-o` came back");
    assert_eq!(cursor(&window, cx), LINK, "on the byte of the link");
    assert_eq!(
        lens(&window, cx),
        (0, 1),
        "and the note just left is now ahead"
    );

    cx.simulate_keystrokes("ctrl-i");
    assert_eq!(active(&window, cx), "b.md", "`ctrl-i` went forward again");
    assert_eq!(cursor(&window, cx), 0, "where it was left");
    assert_eq!(lens(&window, cx), (1, 0), "and the chain flipped back");
}

/// A note in a folder, linking `[[shopping-list]]` — a note that exists nowhere in
/// the vault. Same prefix as [`A`], so the cursor lands on the link the same way.
const UNRESOLVED: &str = "line one\nsee [[shopping-list]] here\nline three\n";

/// `gD` on a link that resolves to nothing: the note is made **beside the one in
/// hand** — a link written inside `areas/bread-v2/a.md` wants a note of that area —
/// then opened, and said about in the status bar. Following it again is now just a
/// follow: the file is not written over.
#[gpui::test]
fn g_d_makes_the_note_a_link_asks_for(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("areas/bread-v2/a.md", UNRESOLVED)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let cx = &mut cx;
    let window = &window;

    cx.simulate_keystrokes("space f enter");
    assert_eq!(active(window, cx), "areas/bread-v2/a.md");
    cx.simulate_keystrokes("j l l l l");
    assert_eq!(
        cursor(window, cx),
        LINK,
        "the cursor is on the `[[shopping-list]]` span"
    );

    cx.simulate_keystrokes("g D");
    assert_eq!(
        active(window, cx),
        "areas/bread-v2/shopping-list.md",
        "made beside the note in hand, and opened"
    );
    assert!(
        dir.path().join("areas/bread-v2/shopping-list.md").exists(),
        "the file is on disk where the status bar says it is"
    );
    assert_eq!(
        status(window, cx),
        "created areas/bread-v2/shopping-list.md",
        "and the window said so"
    );

    // The link resolves now, so `gD` is a follow: the note that exists is opened and
    // left alone.
    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(active(window, cx), "areas/bread-v2/a.md", "`ctrl-o` back");
    std::fs::write(
        dir.path().join("areas/bread-v2/shopping-list.md"),
        "written by hand\n",
    )
    .unwrap();
    cx.simulate_keystrokes("g D");
    assert_eq!(active(window, cx), "areas/bread-v2/shopping-list.md");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("areas/bread-v2/shopping-list.md")).unwrap(),
        "written by hand\n",
        "the second `gD` followed the link rather than re-creating the note"
    );
}

/// Choosing somewhere new makes the jumps the user stepped back over stale, so
/// `ctrl-i` has nothing to offer afterwards — the same shape a browser's forward
/// history has after a new page is visited.
#[gpui::test]
fn a_new_destination_drops_the_forward_half(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x ctrl-o");
    assert_eq!(active(&window, cx), "a.md");
    assert_eq!(lens(&window, cx), (0, 1), "one jump ahead of us");

    cx.simulate_keystrokes("space f");
    cx.simulate_input("b");
    cx.simulate_keystrokes("enter");
    assert_eq!(active(&window, cx), "b.md", "a new destination");
    assert_eq!(lens(&window, cx), (1, 0), "which dropped the forward half");

    cx.simulate_keystrokes("ctrl-i");
    assert_eq!(
        status(&window, cx),
        "no newer jump",
        "so `ctrl-i` says there is nowhere newer to go"
    );
    assert_eq!(active(&window, cx), "b.md", "and nothing moved");
}

/// `ctrl-o` before the user has gone anywhere is a question with no answer: the
/// status bar says so, and the note and cursor stay put.
#[gpui::test]
fn a_jump_back_with_nothing_behind_it_says_so(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;
    let before = cursor(&window, cx);

    cx.simulate_keystrokes("ctrl-o ctrl-i");
    assert_eq!(
        status(&window, cx),
        "no newer jump",
        "the last of the two is the one that reported"
    );
    assert_eq!(active(&window, cx), "a.md", "neither moved the note");
    assert_eq!(cursor(&window, cx), before, "or the cursor");
    assert_eq!(lens(&window, cx), (0, 0), "and nothing was recorded");
}

/// Re-opening the note that is already open is not a jump. The disk-reload path
/// opens the same path too, so without this the chain would fill with the note
/// the user never left.
#[gpui::test]
fn re_opening_the_open_note_records_nothing(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("space f enter");
    assert_eq!(active(&window, cx), "a.md", "the same note opened again");
    assert_eq!(lens(&window, cx), (0, 0), "and the chain did not grow");
}

/// A deleted note must not stay in the chain, and the chain must still work with
/// nothing on screen: `ctrl-o` is the press the user reaches for the moment the
/// note under them disappears.
#[gpui::test]
fn a_deleted_note_leaves_the_chain(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    // a -> b -> a, so the chain behind the open note holds both notes. The cursor
    // is moved deliberately with `l` first: `space` is the leader and not a motion,
    // so it moves nothing on the way to the picker, and the position the chain has
    // to remember is the one `l` left.
    cx.simulate_keystrokes("space f");
    cx.simulate_input("b");
    cx.simulate_keystrokes("enter");
    assert_eq!(active(&window, cx), "b.md");
    cx.simulate_keystrokes("l");
    cx.simulate_keystrokes("space f");
    let left = cursor(&window, cx);
    assert_eq!(
        left, 1,
        "`l` moved the cursor right, and the chord's `space` moved nothing"
    );
    cx.simulate_input("a");
    cx.simulate_keystrokes("enter");
    assert_eq!(active(&window, cx), "a.md");
    assert_eq!(lens(&window, cx), (2, 0), "both notes are behind us");

    cx.simulate_keystrokes("ctrl-shift-d");
    cx.simulate_input("yes");
    cx.simulate_keystrokes("enter");
    assert_eq!(active(&window, cx), "", "the open note is gone");
    assert_eq!(
        lens(&window, cx),
        (1, 0),
        "and its entries went with it, leaving the other note"
    );

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        active(&window, cx),
        "b.md",
        "so `ctrl-o` lands on the note that still exists"
    );
    assert_eq!(cursor(&window, cx), left, "where it was left");
    assert_eq!(
        lens(&window, cx),
        (0, 0),
        "and nothing was pushed ahead, because there was no note to leave"
    );
}

/// A rename re-points the chain instead of leaving it aimed at a path that no
/// longer resolves.
#[gpui::test]
fn a_rename_moves_the_chain_with_the_note(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x");
    assert_eq!(active(&window, cx), "b.md");

    // The note behind us is `a.md`; renaming it must not strand the jump. The
    // tree opens on the open note, so `k` walks the selection up to the other one.
    cx.simulate_keystrokes("ctrl-w h k r");
    // The field starts with the current stem, so only the change is typed.
    cx.simulate_input("lpha");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        status(&window, cx),
        "renamed to alpha.md",
        "the rename went through"
    );

    cx.simulate_keystrokes("ctrl-w l ctrl-o");
    assert_eq!(
        active(&window, cx),
        "alpha.md",
        "`ctrl-o` follows the note to its new name"
    );
    assert_eq!(cursor(&window, cx), LINK, "and to the byte it was left at");
}

/// The jump keys are bound with no key context, so they arrive while a picker is
/// open. The handler has to stand down, or the note under the picker would change
/// while the user is still choosing.
#[gpui::test]
fn the_jump_keys_stand_down_while_an_overlay_is_open(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x");
    assert_eq!(active(&window, cx), "b.md");

    cx.simulate_keystrokes("space f");
    assert_eq!(picker_rows(&window, cx), Some(2), "the picker is up");

    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        picker_rows(&window, cx),
        Some(2),
        "`ctrl-o` did not close it"
    );
    assert_eq!(
        active(&window, cx),
        "b.md",
        "and did not change the note underneath it"
    );
}

// ---------------------------------------------------------------------------
// P4.6a: `gd` follows a heading anchor, not only the note.
// ---------------------------------------------------------------------------

/// The caret's row in the open note.
fn caret_row(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| ws.cursor_position(cx))
        .unwrap()
        .0
}

/// A note whose second line links a section of another note.
const ANCHORED: &str = "line one\nsee [[gift#The middle]] here\nline three\n";
/// \u2026the note it links, with the wanted heading on row 2.
const SECTIONS: &str = "# Gift\nnot this\n## The middle\nthe part\n## The end\nnope\n";

/// Park the caret inside the link on the second line.
fn on_the_link(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("2 G ^ l l l l");
}

/// `gd` on `[[gift#The middle]]` opens `gift` **and** lands on that heading. Opening
/// the note was only half of what the link asked for.
#[gpui::test]
fn gd_lands_on_the_heading_a_link_names(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", ANCHORED), ("gift.md", SECTIONS)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("host.md"), cx)
        })
        .unwrap();
    on_the_link(&mut cx);
    cx.simulate_keystrokes("g d");
    assert_eq!(active(&window, &mut cx), "gift.md", "the note opened");
    assert_eq!(
        caret_row(&window, &mut cx),
        2,
        "and the caret is on `## The middle`, not at the top of the note"
    );
}

/// An anchor that names no heading does not cancel the jump: the note opens at the
/// top and the status line says which heading was missing. Ruled 2026-11-06 \u2014 the
/// note is almost certainly the right destination, and refusing would leave the
/// reader with nothing for a link one rename away from working.
#[gpui::test]
fn a_missing_heading_opens_the_note_anyway_and_says_so(cx: &mut TestAppContext) {
    let host = "line one\nsee [[gift#Nowhere]] here\nline three\n";
    let (_dir, window) = vault_with(cx, &[("host.md", host), ("gift.md", SECTIONS)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("host.md"), cx)
        })
        .unwrap();
    on_the_link(&mut cx);
    cx.simulate_keystrokes("g d");
    assert_eq!(active(&window, &mut cx), "gift.md", "the note still opened");
    assert_eq!(caret_row(&window, &mut cx), 0, "at the top");
    assert_eq!(
        status(&window, &mut cx),
        "no heading \u{201c}Nowhere\u{201d} in gift.md",
        "and the reader is told which half of the link failed"
    );
}

/// `gD` on a link with an anchor creates the note the **target** names: a heading
/// cannot become part of a file name.
#[gpui::test]
fn g_d_creates_the_note_without_the_anchor_in_its_name(cx: &mut TestAppContext) {
    let host = "line one\nsee [[nowhere#Heading]] here\nline three\n";
    let (dir, window) = vault_with(cx, &[("host.md", host)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("host.md"), cx)
        })
        .unwrap();
    on_the_link(&mut cx);
    cx.simulate_keystrokes("g D");
    assert_eq!(
        active(&window, &mut cx),
        "nowhere.md",
        "the note was made and opened"
    );
    assert!(
        dir.path().join("nowhere.md").exists(),
        "on disk, named for the target alone"
    );
    assert!(
        !dir.path().join("nowhere#Heading.md").exists(),
        "the anchor never reached the file name"
    );
}

/// `[[#a heading in this note]]` has no target, so the jump stays put and scrolls:
/// the same heading lookup, applied to the note already on screen.
#[gpui::test]
fn a_heading_link_with_no_target_scrolls_within_the_note(cx: &mut TestAppContext) {
    let note = "# Top\nintro\n## The middle\nsee [[#The middle]] here\n";
    let (_dir, window) = vault_with(cx, &[("host.md", note)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("host.md"), cx)
        })
        .unwrap();
    cx.simulate_keystrokes("4 G ^ l l l l");
    cx.simulate_keystrokes("g d");
    assert_eq!(active(&window, &mut cx), "host.md", "no note was opened");
    assert_eq!(
        caret_row(&window, &mut cx),
        2,
        "the caret moved to that heading"
    );
}

/// Somewhere over the note, for a mouse press that is not looking for a widget.
fn middle() -> gpui::Point<gpui::Pixels> {
    gpui::point(gpui::px(300.), gpui::px(300.))
}

/// The mouse's side buttons are the jump list, the way a browser's are. The
/// buttons reach the window as gpui's `Navigate` buttons, which is what
/// `gpui_linux` reports for `BTN_SIDE`/`BTN_EXTRA` (and X11 buttons 8/9), so the
/// test presses exactly what a real mouse sends.
#[gpui::test]
fn the_mouse_side_buttons_walk_the_jump_list(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x");
    assert_eq!(active(&window, cx), "b.md", "`g x` followed the link");

    cx.simulate_mouse_down(
        middle(),
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Back),
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_up(
        middle(),
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Back),
        gpui::Modifiers::none(),
    );
    assert_eq!(
        active(&window, cx),
        "a.md",
        "the back button came back to the note the link was read in"
    );
    assert_eq!(
        cursor(&window, cx),
        LINK,
        "and landed on the byte of the link that was being read"
    );

    cx.simulate_mouse_down(
        middle(),
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward),
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_up(
        middle(),
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward),
        gpui::Modifiers::none(),
    );
    assert_eq!(
        active(&window, cx),
        "b.md",
        "the forward button walked the same chain the other way"
    );
}

/// A picker's rows are answers about the note as it is now. Jumping underneath an
/// open picker would leave the list describing a note that is no longer on screen,
/// so the side buttons wait until the field lets go of the keyboard.
#[gpui::test]
fn the_side_buttons_wait_while_a_picker_is_open(cx: &mut TestAppContext) {
    let (_dir, window, mut cx) = two_notes(cx);
    let cx = &mut cx;

    cx.simulate_keystrokes("g x");
    assert_eq!(active(&window, cx), "b.md");
    cx.simulate_keystrokes("space s");
    assert!(
        picker_rows(&window, cx).is_some(),
        "the vault search picker is open"
    );

    cx.simulate_mouse_down(
        middle(),
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Back),
        gpui::Modifiers::none(),
    );
    assert_eq!(
        active(&window, cx),
        "b.md",
        "the note under the picker did not move"
    );
    assert!(
        picker_rows(&window, cx).is_some(),
        "and the picker is still the thing being read"
    );

    // Let go of the field, and the same button answers.
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_down(
        middle(),
        gpui::MouseButton::Navigate(gpui::NavigationDirection::Back),
        gpui::Modifiers::none(),
    );
    assert_eq!(
        active(&window, cx),
        "a.md",
        "with the field closed, it walks back"
    );
}

// ---------------------------------------------------------------------------
// P4.9 M3: `gd` follows a block anchor, and lands on the block's first line.
// ---------------------------------------------------------------------------

/// The note it links, with the block on rows 2 and 3 and its id at the end of row 3.
const BLOCKS: &str = "intro\n\nThe goblin waits\nby the door. ^goblin\n\n- one\n";

/// `gd` on `[[gift#^goblin]]` opens `gift` and lands on the **top** of the block, not on
/// the line the id was written at: a jump wants the thing the id names in view.
#[gpui::test]
fn gd_lands_on_the_block_a_link_names(cx: &mut TestAppContext) {
    let host = "line one\nsee [[gift#^goblin]] here\nline three\n";
    let (_dir, window) = vault_with(cx, &[("host.md", host), ("gift.md", BLOCKS)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("host.md"), cx)
        })
        .unwrap();
    on_the_link(&mut cx);
    cx.simulate_keystrokes("g d");
    assert_eq!(active(&window, &mut cx), "gift.md", "the note opened");
    assert_eq!(
        caret_row(&window, &mut cx),
        2,
        "the caret is on the block's first line, not on the id's line"
    );
}

/// A block id the note does not carry does not cancel the jump — the same ruling as a
/// missing heading — and the sentence is the embed's own wording, with the caret in it,
/// so following a link and embedding it cannot disagree about what went wrong.
#[gpui::test]
fn a_missing_block_anchor_says_which_block_is_missing(cx: &mut TestAppContext) {
    let host = "line one\nsee [[gift#^gone]] here\nline three\n";
    let (_dir, window) = vault_with(cx, &[("host.md", host), ("gift.md", BLOCKS)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("host.md"), cx)
        })
        .unwrap();
    on_the_link(&mut cx);
    cx.simulate_keystrokes("g d");
    assert_eq!(active(&window, &mut cx), "gift.md", "the note still opened");
    assert_eq!(caret_row(&window, &mut cx), 0, "at the top");
    assert_eq!(
        status(&window, &mut cx),
        r#"no block "^gone" in gift.md"#,
        "and the reader is told it was the block that was missing"
    );
}

/// EDGE-27: a heading that carries a block id is still the section its name names.
/// Adding an id to a heading must not break every `#Heading` link to it, so the anchor
/// matcher cuts the id off the title before comparing.
#[gpui::test]
fn a_heading_with_a_block_id_still_answers_its_name(cx: &mut TestAppContext) {
    let host = "line one\nsee [[gift#The Fall]] here\nline three\n";
    // The heading is not the note's first line: a jump that missed would leave the
    // caret at the top, and the test would pass for the wrong reason.
    let gift = "preamble\n\n## The Fall ^fall\nthe city went under\n";
    let (_dir, window) = vault_with(cx, &[("host.md", host), ("gift.md", gift)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("host.md"), cx)
        })
        .unwrap();
    on_the_link(&mut cx);
    cx.simulate_keystrokes("g d");
    assert_eq!(active(&window, &mut cx), "gift.md");
    assert_eq!(
        caret_row(&window, &mut cx),
        2,
        "the heading the id sits on is still the heading the link names"
    );
}
