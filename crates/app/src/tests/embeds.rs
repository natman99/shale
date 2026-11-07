//! Transclusion (roadmap P4.0a): another note's lines, painted under the `![[…]]` that
//! asks for them, without entering this note's buffer.
//!
//! What these tests pin is the shape of the promise: the body appears where the embed
//! line is, and *nothing about the note changes*. The display model, `j`/`k`, the ruler
//! and the buffer all still see the note as it is on disk — the body is a view.

use super::commands::status;
use super::*;

// ---------------------------------------------------------------------------
// M3: stepping into an embed (roadmap P4.0b).
//
// The promise being pinned here is where the caret is. Everything above this line
// is about a body being *shown*; from here it is about a body being *typed into*,
// and the two must not be confused: the host keeps painting its own note, the
// embedded note keeps its own buffer, and the only thing that moves is the caret.
// ---------------------------------------------------------------------------

/// What the status bar says about which note the caret is in.
fn badge(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.embed_badge(cx))
        .unwrap_or_default()
}

/// A note this window holds: its text and its dirty flag.
fn state(
    window: &WindowHandle<Workspace>,
    name: &str,
    cx: &mut VisualTestContext,
) -> (String, bool) {
    window
        .update(cx, |ws, _window, _cx| ws.note_state(name))
        .unwrap()
        .unwrap_or_default()
}

/// What the note on screen would paint for its transclusions.
fn embeds(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |ws, _window, cx| ws.embed_shape(cx))
        .unwrap()
}

/// The text of the buffer the caret is actually in: the embedded note's while the
/// caret is inside a block, the tab's note's otherwise.
fn caret_text(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |ws, _window, cx| ws.caret_text(cx))
        .unwrap()
}

/// The text each row of the block being edited was painted with. Markers hidden and
/// markers kept are the same bytes in the buffer, so only the paint can tell them.
fn painted_embed(
    window: &WindowHandle<Workspace>,
    cx: &mut VisualTestContext,
) -> Vec<(usize, String)> {
    window
        .update(cx, |ws, _window, cx| {
            ws.editor()
                .unwrap()
                .read_with(cx, |editor, _cx| editor.painted_embed_text())
        })
        .unwrap()
}

/// How many display lines the note on screen is laid out as.
fn rows(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| ws.display_line_count(cx))
        .unwrap()
}

const HOST: &str = "before\n![[gift]]\nafter\n";
const GIFT: &str = "one\ntwo\n";

/// A `![[note]]` line shows that note's lines under itself.
#[gpui::test]
fn a_transclusion_paints_the_other_notes_lines_under_the_line_that_asks_for_them(
    cx: &mut TestAppContext,
) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec!["embed row 1", "  @gift.md:0 one", "  @gift.md:1 two"],
        "the embed's own row, then the body it brings"
    );
}

/// The body is a view, not an edit: the note is still three lines as far as everything
/// that moves through it is concerned.
#[gpui::test]
fn an_embedded_body_does_not_enter_the_notes_display_model(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    // Two rows are painted and zero rows exist. The note is still the three lines it
    // has on disk, which is what keeps `j`, the ruler, search and folds answering for
    // the note rather than for everything the view happens to be showing.
    assert_eq!(
        embeds(&window, &mut cx).len(),
        3,
        "the embed and its two body rows"
    );
    assert_eq!(
        rows(&window, &mut cx),
        4,
        "three lines of text plus the empty line the trailing newline makes — and not          one more for the two rows the embed paints"
    );
}

/// `ctrl-]` on a `![[…]]` line puts the caret in the other note, and the typing
/// follows the caret rather than the tab.
#[gpui::test]
fn ctrl_bracket_steps_into_the_embed_under_the_caret(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        badge(&window, &mut cx),
        "",
        "the caret starts in the note on screen"
    );

    // Down onto the `![[gift]]` line, then in.
    cx.simulate_keystrokes("j ctrl-]");
    assert_eq!(badge(&window, &mut cx), "in ![[gift]]");

    cx.simulate_keystrokes("i X escape");
    assert_eq!(
        state(&window, "gift", &mut cx).0,
        "Xone\ntwo\n",
        "the character landed in the embedded note"
    );
    assert_eq!(
        state(&window, "host", &mut cx).0,
        HOST,
        "and not one character in the note the tab is showing"
    );
}

/// `ctrl-o` walks back out — the same key as the jump chain, taken by the embed
/// while the caret is inside one (ruling 2).
#[gpui::test]
fn ctrl_o_walks_back_out_of_an_embed(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-] i X escape ctrl-o");
    assert_eq!(
        badge(&window, &mut cx),
        "",
        "the caret is back in the note on screen"
    );
    cx.simulate_keystrokes("i Y escape");
    assert_eq!(
        state(&window, "host", &mut cx).0,
        "before\nY![[gift]]\nafter\n",
        "and the host's caret was left exactly where it was parked"
    );
    assert_eq!(
        state(&window, "gift", &mut cx).0,
        "Xone\ntwo\n",
        "leaving did not drop the note the caret had been typing into"
    );
}

/// The seam: the section's edges are the caret's edges, so `G` and `gg` stop at
/// them and so does every other motion, without knowing they exist.
#[gpui::test]
fn the_seam_stops_motions_at_the_sections_edges(cx: &mut TestAppContext) {
    let gift = "# Top\none\ntwo\n# Below\nnot shown\n";
    let (_dir, window) = vault_with(cx, &[("host.md", "![[gift#Top]]\n"), ("gift.md", gift)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("ctrl-]");
    // The section is rows 0..=2 of gift: the heading and its two lines. `# Below`
    // is in the buffer and not on screen, so it is out of reach.
    cx.simulate_keystrokes("G");
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, cx| ws.cursor_position(cx))
            .unwrap()
            .0,
        2,
        "`G` stopped at the last row the embed shows"
    );
    cx.simulate_keystrokes("j j j");
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, cx| ws.cursor_position(cx))
            .unwrap()
            .0,
        2,
        "and so did walking down past it"
    );
    cx.simulate_keystrokes("g g");
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, cx| ws.cursor_position(cx))
            .unwrap()
            .0,
        0,
        "`gg` stopped at the first row it shows"
    );
}

/// A click inside the body enters that embed. It has to be asked before the click
/// scan looks for a host line, because that scan answers a point in the body with
/// the host row above it.
#[gpui::test]
fn clicking_an_embeds_body_steps_into_it(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    let band = window
        .update(&mut cx, |ws, _w, cx| ws.phantom_rows(cx))
        .unwrap()
        .into_iter()
        .find(|(what, ..)| what.ends_with(":1"))
        .expect("the body's second row is painted");
    let (_, left, top, bottom) = band;
    cx.simulate_click(
        gpui::point(gpui::px(left + 4.0), gpui::px((top + bottom) / 2.0)),
        gpui::Modifiers::default(),
    );
    assert_eq!(badge(&window, &mut cx), "in ![[gift]]");
    cx.simulate_keystrokes("i Z escape");
    assert_eq!(
        state(&window, "gift", &mut cx).0,
        "one\nZtwo\n",
        "the click chose the row; the caret starts at its beginning"
    );
}

/// Dirty follows the caret. The embedded note is the one with unsaved edits, even
/// though no tab is showing it (ruling 17).
#[gpui::test]
fn typing_in_an_embed_dirties_the_embedded_note(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-] i ! escape");
    assert!(state(&window, "gift", &mut cx).1, "the embed is dirty");
    assert!(
        !state(&window, "host", &mut cx).1,
        "the host was not touched, so it is not dirty"
    );
}

// ---------------------------------------------------------------------------
// M4: what a save owes an embed (roadmap P4.0b).
//
// The ruling these pin: an embed is a note, so it has its own file and its own
// dirty flag — but the caret got there *through* the host, so the host's tab is
// the thing that shows the edit is unwritten, and `:w` there is the write that
// owes it. Nothing about that makes the host's text contain the embed: the two
// files stay two files.
// ---------------------------------------------------------------------------

/// `:w` inside an embed writes the embedded note to its own file. The host is
/// written too — it is the tab in hand — but its text is unchanged, which is the
/// point: the edit landed in `gift.md`, not in the line that embeds it.
#[gpui::test]
fn w_inside_an_embed_writes_the_embedded_note_to_its_own_file(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-] A");
    cx.simulate_input(" more");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("gift.md")).unwrap(),
        "one\ntwo\n",
        "typing alone writes nothing"
    );
    type_command("w", &mut cx);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("gift.md")).unwrap(),
        "one more\ntwo\n",
        "the embed was written to the note it belongs to"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("host.md")).unwrap(),
        HOST,
        "and the host's text never grew the embed's words"
    );
}

/// The dot on the host's tab is transitive, and it survives stepping out: the
/// edit came from inside that host, so the host is the place that has to be told
/// it is ahead of the disk — and `:w` there still owes the embed its write.
#[gpui::test]
fn an_unsaved_embed_keeps_the_host_tab_lit_after_the_caret_steps_out(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-] A");
    cx.simulate_input(" more");
    cx.simulate_keystrokes("escape ctrl-o");
    assert!(
        window
            .update(&mut cx, |ws, _w, cx| ws.any_tab_is_dirty(cx))
            .unwrap(),
        "the tab still says there is something unwritten"
    );
    assert!(
        state(&window, "gift", &mut cx).1,
        "and the note holding it is still held, not dropped with the caret"
    );
    type_command("w", &mut cx);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("gift.md")).unwrap(),
        "one more\ntwo\n",
        "a `:w` from the host writes the embed the caret typed into"
    );
    assert!(
        !window
            .update(&mut cx, |ws, _w, cx| ws.any_tab_is_dirty(cx))
            .unwrap(),
        "and the dot goes out, because nothing is ahead of the disk any more"
    );
}

/// Autosave covers the same ground as `:w`: a tab whose embed holds edits is a
/// tab with edits, and the beat that writes the tab writes the embed too. Fixing
/// one write path and not the other is how an editor ends up lying about the dot.
#[gpui::test]
fn autosave_writes_an_embed_the_caret_typed_into(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-] A");
    cx.simulate_input(" more");
    window
        .update(&mut cx, |ws, _w, cx| ws.autosave_now(cx))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("gift.md")).unwrap(),
        "one more\ntwo\n",
        "the countdown does not stop at the seam"
    );
    assert!(
        !window
            .update(&mut cx, |ws, _w, cx| ws.any_tab_is_dirty(cx))
            .unwrap(),
        "and the dot goes out with it"
    );
}

/// Ruling Q3: the host's own text is an exit. Clicking it puts the caret where it
/// was clicked, in the note that owns those bytes — which is the whole reason the
/// exit happens before the click scan: the scan's offsets are host offsets, and while
/// the caret is inside an embed `self.buffer` is a different note's buffer.
#[gpui::test]
fn a_click_on_the_host_around_an_embed_steps_the_caret_out(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-]");
    assert!(!badge(&window, &mut cx).is_empty(), "inside the embed");
    // The row's own left edge, not a fixed x: the tree panel sits to the left of the
    // text, and a click there opens a note instead of moving a caret.
    let (left, top, bottom) = window
        .update(&mut cx, |ws, _w, cx| ws.line_band(0, cx))
        .unwrap()
        .expect("the host's first row is painted");
    cx.simulate_click(
        gpui::point(gpui::px(left + 20.0), gpui::px((top + bottom) / 2.0)),
        gpui::Modifiers::default(),
    );
    assert_eq!(badge(&window, &mut cx), "", "the click took the caret out");
    cx.simulate_keystrokes("I");
    cx.simulate_input("X");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        state(&window, "host", &mut cx).0,
        "Xbefore\n![[gift]]\nafter\n",
        "and the keystroke landed in the note that was clicked"
    );
    // The embedded note is not held in memory any more — nothing typed into it, and
    // a clean embed with no tab of its own is not a note this window needs to keep.
    assert_eq!(state(&window, "gift", &mut cx), (String::new(), false));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("gift.md")).unwrap(),
        GIFT,
        "and the embed's file was not touched"
    );
}

/// Ruling Q5: a section that stops existing under the caret gives the caret back,
/// with a line saying so. Here the author deletes the heading from inside the block —
/// the common way this happens, since the caret is the one doing the deleting.
#[gpui::test]
fn a_section_that_stops_existing_under_the_caret_gives_the_caret_back(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "![[gift#Part]]\n"),
            ("gift.md", "## Part\nkeep the change\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("ctrl-]");
    assert!(!badge(&window, &mut cx).is_empty(), "inside the section");
    // `d d`, not `dd`: `simulate_keystrokes` splits on spaces into key presses, so
    // "dd" is one press of a key named "dd", which is no key at all.
    cx.simulate_keystrokes("d d");
    repaint(&mut cx);
    assert_eq!(
        badge(&window, &mut cx),
        "",
        "the block the caret was in is no longer there to be in"
    );
    assert!(
        status(&window, &mut cx).contains("no longer holds the section"),
        "and the bar says why the caret moved: {}",
        status(&window, &mut cx)
    );
    assert_eq!(
        state(&window, "gift", &mut cx).0,
        "keep the change\n",
        "the deletion still happened — leaving is not undoing"
    );
}

/// Ruling Q4: the command line follows the caret. `:rename` inside an embed renames
/// the note being edited, the host's `![[…]]` line follows it, and the caret stays
/// where it was — with the block still showing the edits in hand rather than the copy
/// the rename carried to disk.
#[gpui::test]
fn a_rename_inside_an_embed_renames_the_embedded_note(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("host.md", "![[gift]]\n"), ("gift.md", "one\ntwo\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("ctrl-] A");
    cx.simulate_input(" more");
    cx.simulate_keystrokes("ctrl-shift-r");
    // The prompt starts with the current stem, so the old name is erased first.
    cx.simulate_keystrokes("backspace backspace backspace backspace");
    cx.simulate_input("Kept");
    cx.simulate_keystrokes("enter");
    assert!(
        dir.path().join("Kept.md").is_file(),
        "the embedded note is the one that moved"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("host.md")).unwrap(),
        "![[Kept]]\n",
        "and the line that embeds it was rewritten to the new name"
    );
    assert!(
        !badge(&window, &mut cx).is_empty(),
        "the caret is still inside the block it was editing"
    );
    let shape = embeds(&window, &mut cx);
    assert!(
        shape.iter().any(|line| line.contains("one more")),
        "the block shows the edits in hand, not the disk copy the rename moved: {shape:?}"
    );
}

/// `:w <name>` refuses inside an embed, and names the embed: the rule against
/// cloning a note by accident applies to whichever of the two the caret is in.
#[gpui::test]
fn w_with_a_name_refuses_inside_an_embed(cx: &mut TestAppContext) {
    let (dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-]");
    type_command("w twin", &mut cx);
    assert!(
        status(&window, &mut cx).contains("gift"),
        "the refusal names the note the caret is in: {}",
        status(&window, &mut cx)
    );
    assert!(
        !dir.path().join("twin.md").is_file(),
        "and nothing was cloned"
    );
}

/// Ruling Q1: unsaved work always has a visible home. The moment an embed the caret
/// has typed into goes dirty and no tab holds it, the bar gains one — the dot has
/// somewhere to light, and a close or a quit owes it the ordinary dirty-tab warning.
///
/// The tab arrives without taking the keyboard: the caret is in the middle of a word
/// in the host, and the tab bar is reporting an edit, not asking the user to go there.
#[gpui::test]
fn an_embed_typed_into_gains_a_tab_of_its_own(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, cx| ws.tab_paths(cx))
            .unwrap()
            .len(),
        1,
        "one tab, the host"
    );
    cx.simulate_keystrokes("j ctrl-] A");
    cx.simulate_input(" more");
    let bar: Vec<String> = window
        .update(&mut cx, |ws, _w, cx| {
            ws.tab_paths(cx)
                .into_iter()
                .map(|(path, dirty)| {
                    format!(
                        "{}{}",
                        path.map(|p| p.display().to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        if dirty { "*" } else { "" }
                    )
                })
                .collect()
        })
        .unwrap();
    assert_eq!(bar.len(), 2, "the embedded note now has a tab: {bar:?}");
    assert!(
        bar.iter().any(|tab| tab.ends_with("gift.md*")),
        "and it is lit, because it is the note with the edits: {bar:?}"
    );
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, _cx| ws.tab_index())
            .unwrap(),
        0,
        "the host is still the tab on screen"
    );
    assert!(
        !badge(&window, &mut cx).is_empty(),
        "and the caret is still inside the block"
    );
    cx.simulate_keystrokes("escape i");
    cx.simulate_input("Z");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        state(&window, "gift", &mut cx).0,
        "one moreZ\ntwo\n",
        "typing kept landing in the embed, not in the tab that just appeared"
    );
}

/// Once the caret is inside, the block stops being a picture: its rows are painted
/// as line elements over the embedded note's buffer, each answering with a row of
/// *that* note. Before entering there are none — the same rows are phantom then.
#[gpui::test]
fn an_entered_block_is_painted_as_rows_of_the_embedded_note(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, cx| ws.embed_rows(cx))
            .unwrap(),
        Vec::new(),
        "read-only, the body is a picture"
    );
    cx.simulate_keystrokes("j ctrl-]");
    let bands = window
        .update(&mut cx, |ws, _w, cx| ws.embed_rows(cx))
        .unwrap();
    assert_eq!(
        bands.iter().map(|(row, ..)| *row).collect::<Vec<_>>(),
        vec![0, 1],
        "the two rows of gift.md, addressed in gift.md"
    );
    assert_eq!(
        rows(&window, &mut cx),
        4,
        "and the host's display model still counts only its own lines"
    );
}

/// A click inside the block being edited is an ordinary click in a note: it lands on
/// the character under the pointer, in the embedded buffer — not at the row's start,
/// which is where entering by keyboard puts the caret.
#[gpui::test]
fn a_click_inside_the_entered_block_lands_where_it_was_pointed(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-]");
    assert_eq!(
        window
            .update(&mut cx, |ws, _w, cx| ws.cursor_position(cx))
            .unwrap(),
        (0, 0),
        "entering lands at the start of the first shown row"
    );
    let band = window
        .update(&mut cx, |ws, _w, cx| ws.embed_rows(cx))
        .unwrap()
        .into_iter()
        .find(|(row, ..)| *row == 1)
        .expect("gift.md's second row is painted as a real line");
    let (_, left, top, bottom) = band;
    cx.simulate_click(
        gpui::point(gpui::px(left + 18.0), gpui::px((top + bottom) / 2.0)),
        gpui::Modifiers::default(),
    );
    let (line, col) = window
        .update(&mut cx, |ws, _w, cx| ws.cursor_position(cx))
        .unwrap();
    assert_eq!(
        line, 1,
        "the click moved the caret to the row it pointed at"
    );
    assert!(
        col > 0,
        "and to a character partway along it, not its start"
    );
}

/// `![[note#Heading]]` is that section, not the whole note.
#[gpui::test]
fn an_anchored_transclusion_paints_only_the_section_it_names(cx: &mut TestAppContext) {
    let gift = "# Top\nnot this\n## Middle\nthe part\n### Deeper\nstill the part\n## End\nnope\n";
    let (_dir, window) = vault_with(cx, &[("host.md", "![[gift#Middle]]\n"), ("gift.md", gift)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec![
            "embed row 0",
            "  @gift.md:2 ## Middle",
            "  @gift.md:3 the part",
            "  @gift.md:4 ### Deeper",
            "  @gift.md:5 still the part",
        ],
        "the section runs to the next heading of its own level"
    );
}

/// An embed of a note that is not there, and an embed of an attachment, contribute
/// nothing: the first stays the honest text of a broken link, the second belongs to the
/// image renderer.
#[gpui::test]
fn an_embed_that_resolves_to_no_note_paints_no_body(cx: &mut TestAppContext) {
    let text = "![[nowhere]]\n![[diagram.png]]\n";
    let (dir, window) = vault_with(cx, &[("host.md", text)]);
    std::fs::write(dir.path().join("diagram.png"), b"not really a png").unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    window
        .update(&mut cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();
    assert_eq!(
        embeds(&window, &mut cx),
        Vec::<String>::new(),
        "neither a broken link nor a picture is a note body"
    );
}

/// Embeds inside an embed step right, and the design's cap turns the level past three
/// into a name rather than more content — which is what makes an embed loop impossible
/// instead of merely expensive.
#[gpui::test]
fn nested_transclusions_step_deeper_and_the_cap_stops_at_a_name(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "![[a]]\n"),
            ("a.md", "a-text\n![[b]]\n"),
            ("b.md", "b-text\n![[c]]\n"),
            ("c.md", "c-text\n![[d]]\n"),
            ("d.md", "never painted\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec![
            "embed row 0",
            "  @a.md:0 a-text",
            "    @b.md:0 b-text",
            "      @c.md:0 c-text",
            "      [d]",
        ],
        "three levels of content, and the fourth is a name the reader can act on"
    );
}

/// A note that embeds itself does not paint itself inside itself.
#[gpui::test]
fn a_note_that_embeds_itself_badges_instead_of_repeating(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", "text\n![[host]]\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(embeds(&window, &mut cx), vec!["embed row 1: badge host"]);
}

/// Raw mode is the file as it sits on disk, and an embed's body is not in this file.
#[gpui::test]
fn raw_mode_shows_the_embed_line_and_no_body(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert!(
        !embeds(&window, &mut cx).is_empty(),
        "live preview shows the body"
    );
    cx.simulate_keystrokes(": r a w enter");
    assert_eq!(
        embeds(&window, &mut cx),
        Vec::<String>::new(),
        "raw mode shows the source, which is the whole point of raw mode"
    );
}

/// `gd` on the `![[…]]` line and `ctrl-]` are different doors on purpose: `gd` is
/// "take me to that note" — the same thing it does to any other link, so the tab
/// turns into the embedded note and `ctrl-o` walks back — and `ctrl-]` is "let me
/// write in it here".
#[gpui::test]
fn gd_on_the_embed_line_opens_a_tab_instead_of_editing_in_place(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[("host.md", "![[gift]]\n"), ("gift.md", "embedded body\n")],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("g d");
    window
        .update(&mut cx, |ws, _window, cx| {
            assert_eq!(ws.caret_text(cx), "embedded body\n", "gd went there");
            assert_eq!(
                ws.tab_paths(cx)
                    .iter()
                    .map(|(p, _)| p.clone())
                    .collect::<Vec<_>>(),
                vec![Some(PathBuf::from("gift.md"))],
                "in the tab that was showing the host, which is what a link does"
            );
        })
        .unwrap();
    cx.simulate_keystrokes("ctrl-o");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.caret_text(cx))
            .unwrap(),
        "![[gift]]\n",
        "and the host is one `ctrl-o` away, the same as any other jump"
    );
}

/// Raw mode draws the `![[…]]` line and no body, so a caret inside a block has to
/// come out with the mode. Left in, it sits inside something the frame refuses to
/// paint: nothing on screen says where it is, and nothing can be done to what it is
/// on. The way out should not be a second keystroke the user has to know.
#[gpui::test]
fn raw_mode_exits_the_embed_it_cannot_show(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[("host.md", "![[gift]]\n"), ("gift.md", "embedded body\n")],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("ctrl-]");
    assert_eq!(caret_text(&window, &mut cx), "embedded body\n");
    cx.simulate_keystrokes(": r a w enter");
    assert_eq!(
        embeds(&window, &mut cx),
        Vec::<String>::new(),
        "raw mode shows the source, which is the whole point of raw mode"
    );
    assert_eq!(
        caret_text(&window, &mut cx),
        "![[gift]]\n",
        "the caret is back in the note the tab is showing, not parked inside a block that is not drawn"
    );
    assert!(
        status(&window, &mut cx).contains("raw mode"),
        "and the frame says why the caret moved: {}",
        status(&window, &mut cx)
    );
    cx.simulate_keystrokes("A Z escape");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "![[gift]]Z\n",
        "typing goes to the note on screen"
    );
    assert_eq!(
        std::fs::read_to_string(_dir.path().join("gift.md")).unwrap(),
        "embedded body\n",
        "and not to the note that was being edited"
    );
}

/// Editing the host is enough to change what is embedded, with no disk event in
/// between: the reference scan runs against the buffer, not against a cached list.
#[gpui::test]
fn typing_an_embed_shows_it_on_the_next_frame(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", "before\n"), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert!(embeds(&window, &mut cx).is_empty(), "nothing embedded yet");
    cx.simulate_keystrokes("o ! [ [ g i f t ] ] escape");
    assert_eq!(
        embeds(&window, &mut cx),
        vec!["embed row 1", "  @gift.md:0 one", "  @gift.md:1 two"],
        "the line just typed is an embed the very next frame shows"
    );
}

/// Opening a note in a tab that was showing another one re-points the resolver at the
/// new host, so a self-embed is caught against the note now on screen.
#[gpui::test]
fn switching_notes_repoints_the_embed_resolver(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "![[b]]\n"), ("b.md", "text\n![[b]]\n")]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "a.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec!["embed row 0", "  @b.md:0 text", "    [b]"],
        "b is on the chain once, so its self-embed badges — one level deeper, where its          content would have been"
    );
    show_at(&mut cx, &window, "b.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec!["embed row 1: badge b"],
        "now b is the host, and its own embed of itself badges at once"
    );
}

/// An embed shows a note as this window has it, not as the disk has it (roadmap
/// P4.0b ruling 12). A note open in another tab with unsaved edits embeds *with*
/// those edits: one window must not show two versions of one note, and the
/// version the author is looking at is the one that wins.
#[gpui::test]
fn an_embed_follows_a_note_being_edited_in_another_tab(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, _cx| ws.set_autosave(false))
        .unwrap();
    show_at(&mut cx, &window, "host.md");
    // A second tab, then the other note put into it through the picker — the road
    // an author actually takes, and the one that leaves the keyboard where the
    // typing has to land.
    cx.simulate_keystrokes("ctrl-t");
    cx.simulate_keystrokes(":");
    cx.simulate_input("e");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("gift");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("I");
    cx.simulate_input("edited ");
    cx.simulate_keystrokes("escape");
    // Back to the host tab. Its embed reads the window's copy of `gift` — the one
    // with the word just typed into it — and not the file, which still says `one`.
    // Back to the host tab.
    window
        .update(&mut cx, |ws, _window, cx| ws.show_tab_at(0, cx))
        .unwrap();
    assert_eq!(
        embeds(&window, &mut cx),
        vec![
            "embed row 1",
            "  ●@gift.md:0 edited one",
            "  @gift.md:1 two",
        ],
        // The dot is not noise: `gift` is ahead of its file in this window, and this is
        // the only surface that says so besides the other tab.
        "the embed follows the unsaved edit, and says where the unsaved edit is"
    );
    assert_eq!(
        std::fs::read_to_string(_dir.path().join("gift.md")).unwrap(),
        GIFT,
        "and none of this touched the file: the edit is still only in the buffer"
    );
}

// ---------------------------------------------------------------------------
// The gutter's share of the save story (roadmap P4.0b).
//
// A tab's dot answers "which tab has unwritten work". Inside an embed that question
// has no tab to answer it — an embedded note is not a tab — so the note's own rows
// carry the mark, in the gutter column the callout's kind glyph and a folded heading's
// disclosure already live in.
// ---------------------------------------------------------------------------

/// A note shown inside another note, ahead of its file, wears a dot in the gutter on the
/// row where its run inside the block starts (roadmap P4.0b).
///
/// One dot per run rather than one per row: the mark says *this note is unwritten*, and a
/// stripe of it down the body would say *these lines are different*, which is a different
/// claim and a noisier one.
#[gpui::test]
fn a_note_ahead_of_its_file_wears_a_dot_in_the_gutter(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, _cx| ws.set_autosave(false))
        .unwrap();
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec!["embed row 1", "  @gift.md:0 one", "  @gift.md:1 two"],
        "written, so the gutter has nothing to say"
    );
    cx.simulate_keystrokes("j ctrl-] A");
    cx.simulate_input(" more");
    cx.simulate_keystrokes("escape ctrl-o");
    assert_eq!(
        embeds(&window, &mut cx),
        vec!["embed row 1", "  ●@gift.md:0 one more", "  @gift.md:1 two"],
        "the dot is on the row where the run starts, and on that row only"
    );
    type_command("w", &mut cx);
    assert_eq!(
        embeds(&window, &mut cx),
        vec!["embed row 1", "  @gift.md:0 one more", "  @gift.md:1 two"],
        "and `:w` wrote the embed to its own file, so the mark is gone"
    );
    assert_eq!(
        std::fs::read_to_string(_dir.path().join("gift.md")).unwrap(),
        "one more\ntwo\n",
        "which it only does because the dot's note is the note the caret typed into"
    );
}

/// The dot belongs to a note, not to a block: a nested embed's rows are a *different*
/// note's text, so an unwritten inner note is marked where its own run starts, and the
/// outer block's top row stays clean.
#[gpui::test]
fn a_nested_embed_gets_its_own_dot_rather_than_its_hosts(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "![[outer]]\n"),
            ("outer.md", "top\n![[inner]]\n"),
            ("inner.md", "deep\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, _cx| ws.set_autosave(false))
        .unwrap();
    show_at(&mut cx, &window, "host.md");
    // The inner note made dirty the way an author makes it dirty: open, type, and leave
    // it unwritten. Two levels of *editing* are not on offer, so this is the only road in.
    cx.simulate_keystrokes("ctrl-t");
    // The picker's road, not `:e inner`: `:e` with no argument asks which note, and an
    // argument it does not parse would leave the typing in a blank tab.
    cx.simulate_keystrokes(":");
    cx.simulate_input("e");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("inner");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("A");
    cx.simulate_input("ZZ");
    cx.simulate_keystrokes("escape");
    window
        .update(&mut cx, |ws, _window, cx| ws.show_tab_at(0, cx))
        .unwrap();
    let shape = embeds(&window, &mut cx);
    assert!(
        shape
            .iter()
            .any(|line| line.contains("●") && line.contains("@inner.md:0")),
        "the inner note's run carries the mark: {shape:?}"
    );
    assert!(
        !shape
            .iter()
            .any(|line| line.contains("●") && line.contains("@outer.md")),
        "and the outer note, which is written, does not: {shape:?}"
    );
}

/// The checker follows the caret into an embed. It holds one document, so while the
/// caret is inside, the server is looking at the embedded note — and its notes are bytes
/// of *that* buffer. Filed among the host's, the same byte offset would underline the
/// wrong note's word.
#[gpui::test]
fn the_checkers_notes_land_on_the_note_being_edited(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-]");
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.caret_text(cx))
            .unwrap(),
        GIFT,
        "the buffer a change is sent as is the one being typed into, not the tab's"
    );
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.set_checker_notes(notes_from(&[("spelled wrong", 0, 3)]), cx)
        })
        .unwrap();
    repaint(&mut cx);
    let marks = window
        .update(&mut cx, |ws, _window, cx| ws.painted_marks(cx))
        .unwrap();
    assert_eq!(
        marks.1,
        vec![0],
        "the embedded note's first row drew the squiggle over its `one`"
    );
    assert_eq!(
        marks.0,
        Vec::<usize>::new(),
        "and no row of the host did: byte 0 of the host is `b`, not a misspelling"
    );
    // Out again: the server is handed the host back, and its next answer belongs to it.
    cx.simulate_keystrokes("escape ctrl-o");
    window
        .update(&mut cx, |ws, _window, cx| {
            ws.set_checker_notes(notes_from(&[("spelled wrong", 0, 6)]), cx)
        })
        .unwrap();
    repaint(&mut cx);
    let marks = window
        .update(&mut cx, |ws, _window, cx| ws.painted_marks(cx))
        .unwrap();
    assert_eq!(
        marks.0,
        vec![0],
        "the host's first row drew the next answer over its `before`"
    );
}

/// A wiki-link inside the block being edited is a link, not a picture of one: `gd`
/// follows it from the caret (roadmap P4.0b parity).
///
/// The offset the caret carries is a byte of the *embedded* note, and the host's link
/// list is cut from the host — so this is the same split the `buffer` / `note.buffer`
/// separation exists for, one record type later. Asked out of the host's list, the
/// lookup finds nothing at that byte and the keystroke does nothing at all.
#[gpui::test]
fn a_link_inside_the_entered_block_is_followed_from_the_caret(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "before\n![[gift]]\nafter\n"),
            ("gift.md", "one [[target]]\ntwo\n"),
            ("target.md", "there\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-]");
    assert!(
        badge(&window, &mut cx).contains("gift"),
        "the caret is inside the embedded note: {}",
        badge(&window, &mut cx)
    );
    // Onto the link's own text, then vim's follow.
    cx.simulate_keystrokes("f t");
    cx.simulate_keystrokes("g d");
    let bar: Vec<String> = window
        .update(&mut cx, |ws, _w, cx| {
            ws.tab_paths(cx)
                .into_iter()
                .map(|(path, _)| {
                    path.map(|p| p.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                })
                .collect()
        })
        .unwrap();
    assert!(
        bar.iter().any(|tab| tab.ends_with("target.md")),
        "the link inside the embed was followed, and its target opened: {bar:?}"
    );
}

/// The dot is not only in the frame — it is on the screen. The entered block's rows are
/// line elements over the embedded note's buffer, and the first of them records the mark
/// it actually painted, which is the guarantee that a mark reaching the element also
/// reaches the gutter. Same record the callout tests lean on, and the reason the gutter's
/// tenants share one painter: one record path, not one per feature.
#[gpui::test]
fn the_entered_block_paints_the_dot_on_its_first_row(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut cx, |ws, _window, _cx| ws.set_autosave(false))
        .unwrap();
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("j ctrl-] A");
    cx.simulate_input(" more");
    repaint(&mut cx);
    let dots = window
        .update(&mut cx, |ws, _window, cx| ws.gutter_dots(cx))
        .unwrap();
    assert_eq!(
        dots.1,
        vec![0],
        "the entered block's first row painted the dot, and no other row of it did"
    );
    assert_eq!(
        dots.0,
        Vec::<usize>::new(),
        "and the host's own rows did not: it is written, and its embed's state is not its own"
    );
}

/// A painted embed row is not just a picture: the band it was drawn in answers with
/// the note and row it is a line of (roadmap P4.0b). This is what a click on an
/// embed has to find before it can enter one, and what a hover card and
/// click-to-open need for the same reason.
#[gpui::test]
fn a_painted_embed_row_answers_with_where_it_came_from(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("host.md", HOST), ("gift.md", GIFT)]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    let rows = window
        .update(&mut cx, |ws, _window, cx| ws.phantom_rows(cx))
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|(what, ..)| what.clone())
            .collect::<Vec<_>>(),
        vec!["gift.md:0", "gift.md:1"],
        "the two rows the embed paints, in the order they were painted"
    );
    // The bands belong between the note's own rows: the embed asks on host row 1, and
    // host row 2 is `after`. A hit record that landed anywhere else would send a click
    // to the wrong place in the note.
    let asking = window
        .update(&mut cx, |ws, _window, cx| ws.line_band(1, cx))
        .unwrap()
        .unwrap();
    let after = window
        .update(&mut cx, |ws, _window, cx| ws.line_band(2, cx))
        .unwrap()
        .unwrap();
    for (_, _, top, bottom) in &rows {
        assert!(
            *top >= asking.1 && *bottom <= after.1,
            "the embed's row at {top}..{bottom} must sit between the asking row \
             ({asking:?}) and the note's next row ({after:?})"
        );
    }
    // And the point-in-band query finds it: the middle of the first body row.
    let (what, left, top, bottom) = &rows[0];
    let at = (left + 20., (top + bottom) / 2.);
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws.origin_at(at, cx))
            .unwrap(),
        Some(what.clone()),
        "a point inside the band names the row it was painted for"
    );
    // A point on the note's own text is not an embed row.
    let own = window
        .update(&mut cx, |ws, _window, cx| ws.line_band(0, cx))
        .unwrap()
        .unwrap();
    assert_eq!(
        window
            .update(&mut cx, |ws, _window, cx| ws
                .origin_at((at.0, (own.0 + own.1) / 2.), cx))
            .unwrap(),
        None,
        "the note's own first row belongs to the note"
    );
}

/// An embed of a note that exists but has no such section says so, in the same
/// words a followed link uses (ruled 2026-11-06). An empty block would read as a
/// successful embed; the raw line hides the reason inside a typo.
#[gpui::test]
fn an_embed_of_a_missing_section_names_the_section_that_is_not_there(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[("host.md", "![[gift#Nope]]\n"), ("gift.md", "# Top\n")],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec![r#"embed row 0: badge no section "Nope" in gift"#],
        "the note is there; the section is what is missing, and the row says which"
    );
}

/// Live preview reaches into the block being edited: the row the caret is on shows
/// its syntax and every other row reads rendered, exactly as the note's own rows
/// do. Hiding the markers on the line someone is typing on hides what they are
/// typing.
#[gpui::test]
fn the_row_being_typed_in_shows_its_markers(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[
            ("host.md", "![[gift]]\n"),
            ("gift.md", "**one**\n**two**\n"),
        ],
    );
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    cx.simulate_keystrokes("ctrl-]");
    assert_eq!(
        painted_embed(&window, &mut cx),
        vec![(0, "**one**".to_string()), (1, "two".to_string())],
        "the caret's row shows the syntax; the block's other rows read rendered"
    );
    cx.simulate_keystrokes("j");
    assert_eq!(
        painted_embed(&window, &mut cx),
        vec![(0, "one".to_string()), (1, "**two**".to_string())],
        "and the reveal follows the caret down"
    );
}
