//! The session file: where the window was, and where the next one starts.
//!
//! The core tests (`shale_core::session`) cover the file itself — round trip,
//! partial, corrupt. These cover the *window*: that it reads the file on the way
//! up and writes it on the beats the roadmap names — the autosave's pause and a
//! graceful quit.

use shale_core::session::Jump;
use shale_core::Session;
use std::time::Duration;

use super::*;
use crate::dim::Scope;
use crate::editor::View;

/// The autosave pause, spelled out so a test that waits for it says what it
/// waits for (same arrangement as `tests::autosave`).
const PAUSE: Duration = Duration::from_millis(2000);

/// A vault holding `notes` with real text, and a session file as a previous
/// Shale would have left it — written before the window exists, so the restore
/// is the window's first act rather than a race with it.
fn vault_with_session(
    cx: &mut TestAppContext,
    notes: &[(&str, &str)],
    session: &Session,
) -> (tempfile::TempDir, WindowHandle<Workspace>) {
    let mut config = Config::default();
    config.spellcheck.enabled = false;
    config.index.enabled = false;
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in notes {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
    }
    session.save(dir.path()).unwrap();
    let window = open_window(cx, dir.path(), config);
    (dir, window)
}

/// The whole point of the feature: the window comes up on the note the user was
/// reading, at their cursor, in their view, with their folders open and their
/// `ctrl-o` still working back through notes from before the restart.
#[gpui::test]
fn a_session_reopens_the_vault_where_the_user_left_it(cx: &mut TestAppContext) {
    let session = Session {
        open_note: Some(PathBuf::from("notes/b.md")),
        cursor: Some(7),
        scroll_y: None,
        expanded: vec![PathBuf::from("notes")],
        view: Some("preview".into()),
        focus: Some(true),
        focus_scope: Some("section".into()),
        typewriter: Some(true),
        jumps_back: vec![Jump {
            path: PathBuf::from("a.md"),
            cursor: 6,
        }],
        jumps_forward: vec![],
    };
    let (_dir, window) = vault_with_session(
        cx,
        &[
            ("a.md", "the first note"),
            ("notes/b.md", "hello, restored"),
        ],
        &session,
    );

    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let (active, rows) = window
        .update(cx, |ws, _window, _cx| (ws.active_path(), ws.tree_rows()))
        .unwrap();
    assert_eq!(active.as_deref(), Some(Path::new("notes/b.md")));
    assert!(
        rows.contains(&PathBuf::from("notes/b.md")),
        "the folder the user had open is open: {rows:?}"
    );

    let (cursor, view, focus, scope, typewriter) = window
        .update(cx, |ws, _window, cx| {
            ws.editor().read_with(cx, |editor, _cx| {
                (
                    editor.cursor_offset(),
                    editor.view(),
                    editor.focus_mode(),
                    editor.focus_scope(),
                    editor.typewriter(),
                )
            })
        })
        .unwrap();
    assert_eq!(cursor, 7, "the cursor is where they left it");
    assert_eq!(view, View::Preview);
    assert!(focus, "focus mode came back on");
    assert_eq!(scope, Scope::Section);
    assert!(typewriter, "and typewriter scroll with it");

    // The jump chain survived the restart: `ctrl-o` goes back to the note from
    // *before* Shale closed, which is what the roadmap calls the feature for.
    cx.simulate_keystrokes("ctrl-o");
    let back = window
        .update(cx, |ws, _window, _cx| ws.active_path())
        .unwrap();
    assert_eq!(back.as_deref(), Some(Path::new("a.md")));
}

/// A session naming a note that has since been deleted is a memory, not an
/// error: the window opens with nothing, and nothing else about the session is
/// thrown away rudely — the folders and the view still restore.
#[gpui::test]
fn a_session_naming_a_deleted_note_opens_nothing(cx: &mut TestAppContext) {
    let session = Session {
        open_note: Some(PathBuf::from("gone.md")),
        cursor: Some(3),
        expanded: vec![PathBuf::from("keep")],
        ..Session::default()
    };
    let (_dir, window) = vault_with_session(cx, &[("keep/here.md", "still here")], &session);

    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let (active, rows) = window
        .update(cx, |ws, _window, _cx| (ws.active_path(), ws.tree_rows()))
        .unwrap();
    assert_eq!(active, None, "the missing note is not opened");
    assert!(
        rows.contains(&PathBuf::from("keep/here.md")),
        "the rest of the session still restored: {rows:?}"
    );
}

/// The autosave's beat is also the session's beat: a pause of typing puts both
/// files on the disk without anything being pressed.
#[gpui::test]
fn the_autosave_beat_saves_the_session_too(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx)
        })
        .unwrap();
    let clock = Clock(cx.executor());
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("i");
    cx.simulate_input("five more words");
    assert!(Session::load(dir.path()).unwrap().open_note.is_none());

    clock.pause(cx, PAUSE + Duration::from_millis(100));
    let session = Session::load(dir.path()).unwrap();
    assert_eq!(session.open_note.as_deref(), Some(Path::new("a.md")));
    assert_eq!(
        session.cursor,
        Some(15),
        "the cursor rode along with the text"
    );
    assert_eq!(session.view.as_deref(), Some("live"));
}

/// `:q` is a graceful exit, and a graceful exit leaves the session behind —
/// even though the test platform's `quit()` itself does nothing observable.
#[gpui::test]
fn quitting_saves_where_the_window_stood(cx: &mut TestAppContext) {
    let (dir, window) = workspace(cx, &["a.md"]);
    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("a.md"), cx)
        })
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("i");
    cx.simulate_input("typed, then quit");
    cx.simulate_keystrokes("escape ctrl-q");

    let session = Session::load(dir.path()).unwrap();
    assert_eq!(session.open_note.as_deref(), Some(Path::new("a.md")));
    // The cursor is where the window left it — this engine's `escape` keeps it
    // after the last typed character, and the session saves exactly what the
    // buffer says, not what some other editor's cursor convention would guess.
    assert_eq!(session.cursor, Some(16));
    // The note itself went out through the same exit path.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "typed, then quit"
    );
}
