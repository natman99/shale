//! Transclusion (roadmap P4.0a): another note's lines, painted under the `![[…]]` that
//! asks for them, without entering this note's buffer.
//!
//! What these tests pin is the shape of the promise: the body appears where the embed
//! line is, and *nothing about the note changes*. The display model, `j`/`k`, the ruler
//! and the buffer all still see the note as it is on disk — the body is a view.

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

/// Open a note by name rather than the fixture's `a.md`.
fn show_at(cx: &mut TestAppContext, window: &WindowHandle<Workspace>, name: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(name), cx))
        .unwrap();
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
        vec!["embed row 1", "  @gift.md:0 edited one", "  @gift.md:1 two",],
        "the embed follows the unsaved edit, and the row it paints is still gift's row 0"
    );
    assert_eq!(
        std::fs::read_to_string(_dir.path().join("gift.md")).unwrap(),
        GIFT,
        "and none of this touched the file: the edit is still only in the buffer"
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
