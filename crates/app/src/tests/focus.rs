//! Focus mode and typewriter scroll: the two ways the frame can follow the cursor
//! instead of the other way round (roadmap P0.8).
//!
//! Focus mode is checked through what the frame recorded about its own paint
//! (`painted_dims`), because dimming changes colours and the paint tests read
//! text. Typewriter scroll is checked the way a reader would notice it: after a
//! jump, is the caret on screen at all?

use super::*;
use crate::editor::View;

/// Three paragraphs, so there is somewhere near the cursor and somewhere far from
/// it. The cursor starts in the first.
const NOTE: &str = "first para\nsecond line\n\nsecond para\n\nthird para\n";

/// A note long enough that its middle is nowhere near the top of the window.
/// Every line is short, so buffer rows and display lines are the same number.
fn long_note() -> String {
    "line\n".repeat(200)
}

/// The display line `50G` puts the caret on (line 50 of 1-based vim, 0-based
/// here), shared by the typewriter pair so they test the same jump.
const CARET_AFTER_JUMP: usize = 49;

/// A bare editor holding `note`, focused and painted twice so the frame's
/// decisions are in, as `tests::views` does.
fn painted_editor(
    note: &str,
    cx: &mut TestAppContext,
) -> (WindowHandle<MarkdownEditor>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(keybindings()));
    let editor = cx.add_window(|_window, cx| MarkdownEditor::new(cx));
    let mut cx = VisualTestContext::from_window(editor.into(), cx);
    editor
        .update(&mut cx, |editor, window, cx| {
            editor.set_text(note.to_string(), cx);
            let focus = editor.focus_handle(cx);
            window.focus(&focus, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    (editor, cx)
}

/// How much of each display line's colour survived the frame's dimming.
fn dims(editor: &WindowHandle<MarkdownEditor>, cx: &mut VisualTestContext) -> Vec<f32> {
    editor
        .read_with(cx, |editor, _cx| editor.painted_dims())
        .unwrap()
}

/// Turn focus mode on, or set its scope, and repaint.
fn set_focus(editor: &WindowHandle<MarkdownEditor>, cx: &mut VisualTestContext, on: bool) {
    editor
        .update(cx, |editor, _window, cx| editor.set_focus_mode(on, cx))
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
}

/// A window over a vault holding one note, with the note open.
fn window_with(
    cx: &mut TestAppContext,
    text: &str,
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let (dir, window) = vault_with(cx, &[("a.md", text)]);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx);
        })
        .unwrap();
    (dir, window)
}

/// Type a command line and press enter.
fn type_command(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// The status line, where a command reports what it did.
fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// Nothing is dimmed until someone asks. This is the frame's resting state, and
/// the test that keeps focus mode from becoming a default nobody chose.
#[gpui::test]
fn the_frame_dims_nothing_to_start_with(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(NOTE, cx);
    assert!(
        dims(&editor, &mut cx).iter().all(|&d| d == 1.0),
        "every line keeps its colour: {:?}",
        dims(&editor, &mut cx)
    );
}

/// Focus mode lights the block the cursor is in and lets the rest fall away, one
/// step at a time rather than all at once.
#[gpui::test]
fn focus_mode_dims_the_rows_away_from_the_cursor(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(NOTE, cx);
    set_focus(&editor, &mut cx, true);
    let d = dims(&editor, &mut cx);
    assert_eq!(d.len(), 7, "the note's seven display lines");
    assert_eq!(d[0], 1.0, "the cursor's own line");
    assert_eq!(d[1], 1.0, "and the line that continues its paragraph");
    assert!(d[2] < 1.0, "the blank line after it is already quieter");
    assert!(
        d[3] < d[2],
        "the next paragraph quieter still: {:?} then {:?}",
        d[2],
        d[3]
    );
    assert_eq!(
        d[5], d[6],
        "and the far rows hold at the floor rather than vanishing: {d:?}"
    );
    assert!(d[6] > 0.0, "the rest of the note stays readable");
}

/// Raw mode is the file as it sits on disk, so it draws no fade even with focus
/// mode on — the same rule that keeps its markdown syntax, tables and embeds.
#[gpui::test]
fn raw_mode_never_dims(cx: &mut TestAppContext) {
    let (editor, mut cx) = painted_editor(NOTE, cx);
    set_focus(&editor, &mut cx, true);
    editor
        .update(&mut cx, |editor, _window, cx| {
            editor.set_view(View::Raw, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert!(
        dims(&editor, &mut cx).iter().all(|&d| d == 1.0),
        "raw mode shows the note as it is: {:?}",
        dims(&editor, &mut cx)
    );
}

/// `space d` is the way in and the way out, so the mode never needs the command
/// line to leave.
#[gpui::test]
fn space_d_turns_focus_mode_around(cx: &mut TestAppContext) {
    let (_dir, window) = window_with(cx, NOTE);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    assert!(
        !window
            .update(cx, |ws, _window, cx| ws.focus_mode(cx))
            .unwrap(),
        "off to start with"
    );
    cx.simulate_keystrokes("space d");
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.focus_mode(cx))
            .unwrap(),
        "the chord turned it on"
    );
    cx.simulate_keystrokes("space d");
    assert!(
        !window
            .update(cx, |ws, _window, cx| ws.focus_mode(cx))
            .unwrap(),
        "and the same two keys turned it off"
    );
    // `space d` is a chord, not `d` followed by `space`: the note is untouched.
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        NOTE
    );
}

/// `:focus` spells it out for the command line: both halves by name, and a bare
/// word that turns it around.
#[gpui::test]
fn the_command_line_turns_focus_mode_both_ways(cx: &mut TestAppContext) {
    let (_dir, window) = window_with(cx, NOTE);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("focus on", cx);
    assert!(window
        .update(cx, |ws, _window, cx| ws.focus_mode(cx))
        .unwrap());
    type_command("focus off", cx);
    assert!(!window
        .update(cx, |ws, _window, cx| ws.focus_mode(cx))
        .unwrap());
    type_command("focus", cx);
    assert!(
        window
            .update(cx, |ws, _window, cx| ws.focus_mode(cx))
            .unwrap(),
        "a bare `:focus` turns it around"
    );
    type_command("dim off", cx);
    assert!(
        !window
            .update(cx, |ws, _window, cx| ws.focus_mode(cx))
            .unwrap(),
        "`dim` is the same command under the other name"
    );
    // The badge is the only place the mode is reported, so the command line keeps
    // the status line for everything else.
    assert!(
        !status(&window, cx).contains("focus"),
        "no status message to sit next to the badge: {:?}",
        status(&window, cx)
    );
}

/// The scope is a separate setting, and `:focus` is how it is set now that no
/// config key is: a scope word changes how wide the light reaches and leaves the
/// mode where it was.
#[gpui::test]
fn the_command_line_widens_and_narrows_the_lit_region(cx: &mut TestAppContext) {
    let (_dir, window) = window_with(cx, NOTE);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("focus section", cx);
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.focus_scope(cx))
            .unwrap(),
        "section",
        "the scope moved"
    );
    assert!(
        !window
            .update(cx, |ws, _window, cx| ws.focus_mode(cx))
            .unwrap(),
        "and the mode was left alone"
    );
    assert!(
        status(&window, cx).contains("focus scope section"),
        "and says so, because no badge is up to say it: {:?}",
        status(&window, cx)
    );
    type_command("dim block", cx);
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.focus_scope(cx))
            .unwrap(),
        "block",
        "`dim` takes the scope words too"
    );
    // `sentence` is named on the roadmap and not built. The command accepts it and
    // admits what it actually did rather than reporting a scope it does not have.
    type_command("focus sentence", cx);
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.focus_scope(cx))
            .unwrap(),
        "block",
        "the unbuilt scope leaves the default"
    );
    assert!(
        status(&window, cx).contains("not built yet"),
        "{status:?}",
        status = status(&window, cx)
    );
    assert_eq!(
        status(&window, cx),
        "focus scope sentence is not built yet; using block",
        "the whole sentence, so the user knows which scope they got"
    );
}

/// The scope is a separate setting: the same caret can light one paragraph or the
/// whole section it sits under.
#[gpui::test]
fn section_scope_lights_the_whole_section(cx: &mut TestAppContext) {
    let note = "# Top\n\nfirst para\n\nsecond para\n";
    let (editor, mut cx) = painted_editor(note, cx);
    set_focus(&editor, &mut cx, true);
    let block = dims(&editor, &mut cx);
    editor
        .update(&mut cx, |editor, _window, cx| {
            editor.set_focus_scope(crate::dim::Scope::Section, cx);
        })
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let section = dims(&editor, &mut cx);
    assert!(
        block[4] < 1.0,
        "block scope leaves the body dimmed: {block:?}"
    );
    assert!(
        section.iter().all(|&d| d == 1.0),
        "section scope lights the heading and everything under it: {section:?}"
    );
}

/// Typewriter scroll is the part a reader feels rather than sees: after a jump to
/// the middle of a long note, the caret is in the middle of the window, so there
/// is still note below it. Plain scroll-into-view would put the caret at the
/// bottom edge instead, where the reader has nothing to read next.
#[gpui::test]
fn typewriter_scroll_keeps_the_cursor_in_the_middle(cx: &mut TestAppContext) {
    let (_dir, window) = window_with(cx, &long_note());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    window
        .update(cx, |ws, _window, cx| ws.set_typewriter(true, cx))
        .unwrap();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    cx.simulate_keystrokes("5 0 G");
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let row = window
        .update(cx, |ws, _window, cx| ws.cursor_position(cx).0)
        .unwrap();
    assert_eq!(row, CARET_AFTER_JUMP, "`50G` jumped to the middle");
    let below = window
        .update(cx, |ws, _window, cx| {
            ws.display_line_is_visible(cx, CARET_AFTER_JUMP + 5)
        })
        .unwrap();
    assert!(
        below,
        "the caret jumped to line {CARET_AFTER_JUMP} and the note below it came with it"
    );
}

/// Without it, the frame still pulls the caret on screen — it just puts it at the
/// edge, which is the difference the mode is for. The reader lands at the bottom
/// of the window with the next thing they might read off the top of it.
#[gpui::test]
fn without_typewriter_scroll_a_jump_lands_the_cursor_at_the_edge(cx: &mut TestAppContext) {
    let (_dir, window) = window_with(cx, &long_note());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    cx.simulate_keystrokes("5 0 G");
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let row = window
        .update(cx, |ws, _window, cx| ws.cursor_position(cx).0)
        .unwrap();
    assert_eq!(row, CARET_AFTER_JUMP, "the same jump as its pair above");
    let caret = window
        .update(cx, |ws, _window, cx| ws.cursor_line_is_visible(cx))
        .unwrap();
    let below = window
        .update(cx, |ws, _window, cx| {
            ws.display_line_is_visible(cx, CARET_AFTER_JUMP + 5)
        })
        .unwrap();
    assert!(caret, "the caret is on screen either way");
    assert!(
        !below,
        "but the frame only pulled it to the edge rather than centring it"
    );
}

/// `:typewriter` reports itself in the status line, because unlike focus mode
/// there is nothing on screen that shows it is on.
#[gpui::test]
fn the_command_line_reports_typewriter_scroll(cx: &mut TestAppContext) {
    let (_dir, window) = window_with(cx, NOTE);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("typewriter on", cx);
    assert!(window
        .update(cx, |ws, _window, cx| ws.typewriter(cx))
        .unwrap());
    assert!(
        status(&window, cx).contains("typewriter scroll on"),
        "{:?}",
        status(&window, cx)
    );
    type_command("typewriter", cx);
    assert!(
        !window
            .update(cx, |ws, _window, cx| ws.typewriter(cx))
            .unwrap(),
        "a bare `:typewriter` turns it around"
    );
    type_command("typewriter off", cx);
    assert!(
        status(&window, cx).contains("already off"),
        "and says so when it was already off: {:?}",
        status(&window, cx)
    );
}

/// The window comes up with both modes off and the scope at `block`. These are
/// not config keys — they are state the user chose in a running window, and they
/// belong with the session file (roadmap P1.8) rather than the settings they edit
/// by hand — so what a test can ask is that nothing turns them on by itself.
#[gpui::test]
fn the_window_starts_with_both_modes_off(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["a.md"]);
    cx.update(|cx| cx.bind_keys(keybindings()));
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| {
                (ws.focus_mode(cx), ws.focus_scope(cx), ws.typewriter(cx))
            })
            .unwrap(),
        (false, "block".to_string(), false),
        "off, off, and the scope nobody asked to widen"
    );
}
