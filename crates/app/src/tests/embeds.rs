//! Transclusion (roadmap P4.0a): another note's lines, painted under the `![[…]]` that
//! asks for them, without entering this note's buffer.
//!
//! What these tests pin is the shape of the promise: the body appears where the embed
//! line is, and *nothing about the note changes*. The display model, `j`/`k`, the ruler
//! and the buffer all still see the note as it is on disk — the body is a view.

use super::*;


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
        vec!["embed row 1", "  one", "  two"],
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
    assert_eq!(embeds(&window, &mut cx).len(), 3, "the embed and its two body rows");
    assert_eq!(
        rows(&window, &mut cx),
        4,
        "three lines of text plus the empty line the trailing newline makes — and not          one more for the two rows the embed paints"
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
            "  ## Middle",
            "  the part",
            "  ### Deeper",
            "  still the part",
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
    let (_dir, window) = vault_with(cx, &[
        ("host.md", "![[a]]\n"),
        ("a.md", "a-text\n![[b]]\n"),
        ("b.md", "b-text\n![[c]]\n"),
        ("c.md", "c-text\n![[d]]\n"),
        ("d.md", "never painted\n"),
    ]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "host.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec![
            "embed row 0",
            "  a-text",
            "    b-text",
            "      c-text",
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
    assert!(!embeds(&window, &mut cx).is_empty(), "live preview shows the body");
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
        vec!["embed row 1", "  one", "  two"],
        "the line just typed is an embed the very next frame shows"
    );
}

/// Opening a note in a tab that was showing another one re-points the resolver at the
/// new host, so a self-embed is caught against the note now on screen.
#[gpui::test]
fn switching_notes_repoints_the_embed_resolver(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[
        ("a.md", "![[b]]\n"),
        ("b.md", "text\n![[b]]\n"),
    ]);
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    show_at(&mut cx, &window, "a.md");
    assert_eq!(
        embeds(&window, &mut cx),
        vec!["embed row 0", "  text", "    [b]"],
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
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from(name), cx)
        })
        .unwrap();
}
