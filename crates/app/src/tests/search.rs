//! `/` in the open note: the field that takes the query, the matches it lights
//! up as it is typed, and `n`/`N` walking them after the field has closed.
//!
//! The matching itself is [`shale_core::search`]'s business; what is checked here
//! is the wiring — that the keys reach the window, that the window and the editor
//! never disagree about what is drawn, and that the cursor goes where the match is.

use super::*;

/// A note with the same word three times, once capitalised, so smart case has
/// something to answer.
const NOTE: &str = "one two three\nTwo again and two\n";

/// Two frames, so the frame's hitboxes exist and a click lands on something the
/// window drew.
fn paint(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// What the window holds about the search, and what the editor is drawing, read
/// together so a test can check the two never drift apart.
#[derive(Debug, PartialEq)]
struct Search {
    /// The query the window kept.
    query: String,
    /// What it matched, as buffer byte ranges.
    hits: Vec<(usize, usize)>,
    /// Which match the cursor is on.
    index: Option<usize>,
    /// What the editor was told to draw.
    drawn: Vec<(usize, usize)>,
    /// Which of those is the current one.
    active: Option<(usize, usize)>,
}

impl Search {
    /// A search that is not up at all.
    fn empty() -> Self {
        Self {
            query: String::new(),
            hits: Vec::new(),
            index: None,
            drawn: Vec::new(),
            active: None,
        }
    }
}

fn search(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Search {
    window
        .update(cx, |ws, _window, cx| {
            let state = ws.search_state();
            let drawn = ws.editor().read(cx).hits().to_vec();
            let active = ws.editor().read(cx).active_hit();
            Search {
                query: state.query.clone(),
                hits: state.hits.clone(),
                index: state.index,
                drawn: drawn.to_vec(),
                active,
            }
        })
        .unwrap()
}

/// Where the cursor is, as a buffer byte offset.
fn cursor(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| ws.editor().read(cx).cursor_offset())
        .unwrap()
}

/// What the open one-line field holds, or `None` while none is open.
fn prompt(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |ws, _window, cx| ws.prompt_text(cx))
        .unwrap()
}

/// The note from `NOTE`, opened the way quick-open opens it.
fn open_note(cx: &mut VisualTestContext, window: &WindowHandle<Workspace>) {
    cx.simulate_keystrokes("space f enter");
    let (_, active) = state(window, cx);
    assert_eq!(active, "Welcome.md", "the test note is open");
}

/// Typing the query lights up every match and puts the cursor on the first,
/// before `enter` is ever pressed.
#[gpui::test]
fn a_query_lights_up_every_match_as_it_is_typed(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Welcome.md", NOTE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);
    assert_eq!(
        cursor(&window, cx),
        0,
        "a freshly opened note starts at its first byte"
    );

    cx.simulate_keystrokes("/");
    assert_eq!(
        prompt(&window, cx).as_deref(),
        Some(""),
        "`/` opens an empty search field"
    );
    cx.simulate_input("two");

    assert_eq!(
        search(&window, cx),
        Search {
            query: "two".into(),
            hits: vec![(4, 7), (14, 17), (28, 31)],
            index: Some(0),
            drawn: vec![(4, 7), (14, 17), (28, 31)],
            active: Some((4, 7)),
        },
        "every match is drawn and the cursor is on the first"
    );
    assert_eq!(cursor(&window, cx), 4, "the cursor followed the match");
}

/// A query with a capital in it matches exactly, and an all-lowercase one matches
/// either case — vim's smart case, seen through the window.
#[gpui::test]
fn a_capital_in_the_query_makes_it_exact(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Welcome.md", NOTE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);

    cx.simulate_keystrokes("/");
    cx.simulate_input("two");
    assert_eq!(
        search(&window, cx).hits,
        vec![(4, 7), (14, 17), (28, 31)],
        "a lowercase query folds, so it finds `Two` too"
    );

    cx.simulate_keystrokes("escape");
    assert_eq!(
        search(&window, cx),
        Search::empty(),
        "`escape` abandons the search outright"
    );
    assert_eq!(prompt(&window, cx), None, "and the field closes with it");

    cx.simulate_keystrokes("/");
    cx.simulate_input("Two");
    assert_eq!(
        search(&window, cx).hits,
        vec![(14, 17)],
        "a capital in the query asks for that spelling"
    );
}

/// `enter` closes the field but keeps the search, so `n` and `N` can walk the
/// matches — including wrapping past either end of the note.
#[gpui::test]
fn enter_keeps_the_matches_for_n_and_capital_n(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Welcome.md", NOTE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);

    cx.simulate_keystrokes("/");
    cx.simulate_input("two");
    cx.simulate_keystrokes("enter");
    assert_eq!(prompt(&window, cx), None, "the field is gone");
    assert_eq!(
        search(&window, cx).hits,
        vec![(4, 7), (14, 17), (28, 31)],
        "the matches are not"
    );

    cx.simulate_keystrokes("n");
    assert_eq!(
        (search(&window, cx).index, cursor(&window, cx)),
        (Some(1), 14),
        "`n` moved to the second match"
    );
    cx.simulate_keystrokes("n");
    assert_eq!(
        (search(&window, cx).index, cursor(&window, cx)),
        (Some(2), 28),
        "and to the third"
    );
    cx.simulate_keystrokes("n");
    assert_eq!(
        (search(&window, cx).index, cursor(&window, cx)),
        (Some(0), 4),
        "and wrapped to the first"
    );
    cx.simulate_keystrokes("shift-n");
    assert_eq!(
        (search(&window, cx).index, cursor(&window, cx)),
        (Some(2), 28),
        "`N` wraps the other way"
    );
    assert_eq!(
        search(&window, cx).active,
        Some((28, 31)),
        "the editor draws the current match apart from the others"
    );
}

/// A query that matches nothing says so where the user is looking, and leaves the
/// cursor alone.
#[gpui::test]
fn a_query_that_matches_nothing_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Welcome.md", NOTE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);
    let before = cursor(&window, cx);

    cx.simulate_keystrokes("/");
    cx.simulate_input("zebra");

    assert_eq!(
        search(&window, cx),
        Search {
            query: "zebra".into(),
            hits: Vec::new(),
            index: None,
            drawn: Vec::new(),
            active: None,
        },
        "nothing is drawn for a query that found nothing"
    );
    assert_eq!(cursor(&window, cx), before, "and nothing moved");
    assert_eq!(
        window
            .update(cx, |ws, _window, _cx| ws.search_label())
            .unwrap()
            .to_string(),
        "no match for zebra",
        "the status bar says which query came up empty"
    );
}

/// `n` before anything has been searched for is a question with no answer: the
/// status bar says so and the cursor stays put.
#[gpui::test]
fn stepping_without_a_search_does_nothing(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Welcome.md", NOTE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);
    let before = cursor(&window, cx);

    cx.simulate_keystrokes("n shift-n");
    assert_eq!(cursor(&window, cx), before, "there is nowhere to step");
    assert_eq!(
        window.update(cx, |ws, _window, _cx| ws.status()).unwrap(),
        "nothing searched for yet",
        "and the window says why"
    );
}

/// The matches are byte offsets into the note, so editing the note re-runs the
/// query instead of letting the highlights drift onto whatever moved into place.
#[gpui::test]
fn editing_the_note_moves_the_matches(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Welcome.md", NOTE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);

    cx.simulate_keystrokes("/");
    cx.simulate_input("two");
    cx.simulate_keystrokes("enter");

    // `A` appends at the end of the row the cursor is on — the first row, where
    // the first match left it — so the new `two` lands between the old first and
    // second, and everything after them moves four bytes along.
    cx.simulate_keystrokes("A");
    cx.simulate_input(" two");
    cx.simulate_keystrokes("escape");

    assert_eq!(
        search(&window, cx).hits,
        vec![(4, 7), (14, 17), (18, 21), (32, 35)],
        "the query was re-run against the edited note"
    );
}

/// A search belongs to the note it was run in: opening another one ends it,
/// because its matches were offsets into the note that closed.
#[gpui::test]
fn opening_another_note_ends_the_search(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Welcome.md", NOTE), ("inbox/Other.md", "two\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);

    cx.simulate_keystrokes("/");
    cx.simulate_input("two");
    cx.simulate_keystrokes("enter");
    assert!(!search(&window, cx).hits.is_empty());

    cx.simulate_keystrokes("space f");
    cx.simulate_input("other");
    cx.simulate_keystrokes("enter");
    let (_, active) = state(&window, cx);
    assert_eq!(active, "inbox/Other.md", "the other note opened");
    assert_eq!(
        search(&window, cx),
        Search::empty(),
        "and the old note's matches went with it"
    );
}

/// Clicking in the note closes the field — it floats over the note, and a click
/// under it belongs to the note — but keeps the matches, because the click was
/// the user moving the cursor, not abandoning the query.
#[gpui::test]
fn clicking_in_the_note_closes_the_field_and_keeps_the_matches(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Welcome.md", NOTE)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(cx, &window);
    paint(cx);

    cx.simulate_keystrokes("/");
    cx.simulate_input("two");
    assert!(prompt(&window, cx).is_some(), "the field is up");

    // Well above the field, which sits at the bottom of the note column.
    cx.simulate_click(gpui::point(px(700.), px(300.)), gpui::Modifiers::none());
    assert_eq!(
        prompt(&window, cx),
        None,
        "the click closed the field it was not in"
    );
    assert_eq!(
        search(&window, cx).hits,
        vec![(4, 7), (14, 17), (28, 31)],
        "and left the matches for `n`"
    );
}

/// `/` and `n`/`N` are bound in the note's normal mode, and nowhere that takes
/// typed text — otherwise a query could never contain an `n`.
#[test]
fn the_search_keys_are_bound_in_normal_mode_only() {
    let keymap = gpui::Keymap::new(keybindings());
    let normal = vec![gpui::KeyContext::try_from("vim-normal").unwrap()];
    for (shift, key, action) in [
        (false, "/", "shale::SearchBuffer"),
        (false, "n", "shale::SearchNext"),
        (true, "n", "shale::SearchPrev"),
    ] {
        let typed = gpui::Keystroke {
            modifiers: gpui::Modifiers {
                shift,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        };
        let (bindings, pending) = keymap.bindings_for_input(&[typed], &normal);
        assert!(!pending, "{key} should not leave pending input");
        assert_eq!(
            bindings
                .iter()
                .map(|b| b.action().name())
                .collect::<Vec<_>>(),
            vec![action],
            "normal mode: typed {key} should resolve to {action}"
        );
    }

    // The prompt and the picker are text fields: a query may hold any letter, so
    // the same keys must stay unbound there.
    for context in ["prompt", "picker"] {
        let contexts = vec![gpui::KeyContext::try_from(context).unwrap()];
        for key in ["n", "/"] {
            let typed = gpui::Keystroke {
                modifiers: gpui::Modifiers::none(),
                key: key.to_string(),
                key_char: None,
            };
            let (bindings, _) = keymap.bindings_for_input(&[typed], &contexts);
            assert!(
                bindings.is_empty(),
                "{key} must stay free to type in the {context} context"
            );
        }
    }
}
