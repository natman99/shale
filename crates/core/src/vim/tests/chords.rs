//! The keys that ask the UI for something rather than edit the note: `gx`, `K`,
//! the `space` leader's chords, and the fold commands.

use super::*;

#[test]
fn gx_requests_link_follow() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(20);
    vim.handle_normal_key(Key::Char('g'), &mut b);
    assert_eq!(
        vim.handle_normal_key(Key::Char('x'), &mut b),
        Outcome::FollowLink
    );
    // The motion-less `gx` leaves the cursor where it was.
    assert_eq!(b.cursor(), 20);
    assert_eq!(vim.mode(), Mode::Normal);
}

#[test]
fn upper_k_requests_the_checkers_note() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(2);
    // The engine has nothing to look up and no idea what the word means: it
    // hands the request to the window and leaves the cursor alone.
    assert_eq!(
        vim.handle_normal_key(Key::Char('K'), &mut b),
        Outcome::Hover
    );
    assert_eq!(b.cursor(), 2);
    assert_eq!(vim.mode(), Mode::Normal);
    assert_eq!(b.text(), "hello world\nsecond line\nthird");
}

/// Inside a table, `K` is the grid's row motion, which the table path answers
/// before the lookup can claim it.
#[test]
fn upper_k_in_a_table_moves_a_row_instead() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    assert_eq!(
        vim.handle_normal_key(Key::Char('K'), &mut b),
        Outcome::Stay,
        "the table answered it"
    );
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "a",
        "one row up in the same column"
    );
}

#[test]
fn za_folds_and_unfolds_the_section_the_cursor_is_in() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(3); // `body`, inside `## Sub`
    type_keys(&mut vim, "za", &mut b);
    let view = b.fold_view();
    assert_eq!(
        view.marker[2],
        Some(2),
        "the innermost section around the cursor closed"
    );
    assert_eq!(b.row_col(b.cursor()).0, 2);
    assert_eq!(b.text(), note_buf().text(), "a fold never touches the text");
    type_keys(&mut vim, "za", &mut b);
    assert!(!b.has_closed_folds());
}

#[test]
fn zc_zo_and_zd_close_and_open_a_fold() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    type_keys(&mut vim, "zc", &mut b);
    assert_eq!(b.fold_view().marker[0], Some(4));
    // The cursor sits on the fold's heading row, which is where `zo` and `zd`
    // apply.
    type_keys(&mut vim, "zo", &mut b);
    assert!(!b.has_closed_folds());
    type_keys(&mut vim, "zc", &mut b);
    assert!(b.has_closed_folds());
    type_keys(&mut vim, "zd", &mut b);
    assert!(!b.has_closed_folds());
}

#[test]
fn space_f_asks_the_ui_for_the_quick_open_picker() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    assert_eq!(b.cursor(), 0);
    // The space motion is applied immediately, so a lone space is never
    // left waiting for a second key.
    assert_eq!(vim.handle_normal_key(Key::Char(' '), &mut b), Outcome::Stay);
    assert_eq!(b.cursor(), 1, "space still moves right");
    assert_eq!(
        vim.handle_normal_key(Key::Char('f'), &mut b),
        Outcome::QuickOpen
    );
    // The chord is not a change, so `.` has nothing to replay.
    assert_eq!(vim.repeat_last(&mut b), Outcome::Unhandled);
}

#[test]
fn g_f_asks_the_ui_for_the_quick_open_picker() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    assert_eq!(b.cursor(), 0);
    // `g` arms the goto chord, exactly as it does for `gg` and `gx`.
    assert_eq!(vim.handle_normal_key(Key::Char('g'), &mut b), Outcome::Stay);
    assert_eq!(b.cursor(), 0, "`g` alone does not move");
    assert_eq!(
        vim.handle_normal_key(Key::Char('f'), &mut b),
        Outcome::QuickOpen
    );
    // The chord is not a change, so `.` has nothing to replay.
    assert_eq!(vim.repeat_last(&mut b), Outcome::Unhandled);
}

#[test]
fn space_s_asks_the_ui_for_vault_search() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    assert_eq!(vim.handle_normal_key(Key::Char(' '), &mut b), Outcome::Stay);
    assert_eq!(
        vim.handle_normal_key(Key::Char('s'), &mut b),
        Outcome::SearchVault
    );
    // The chord is not a change, so `.` has nothing to replay.
    assert_eq!(vim.repeat_last(&mut b), Outcome::Unhandled);
}

#[test]
fn the_space_leader_asks_the_ui_for_each_view() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    for (key, want) in [
        ('r', Outcome::ViewRaw),
        ('l', Outcome::ViewLive),
        ('p', Outcome::ViewPreview),
    ] {
        // As with the other chords, the space motion is applied first, so a lone
        // space is never left waiting for a second key.
        assert_eq!(vim.handle_normal_key(Key::Char(' '), &mut b), Outcome::Stay);
        assert_eq!(
            vim.handle_normal_key(Key::Char(key), &mut b),
            want,
            "space {key}"
        );
        // A chord is not a change, so `.` has nothing to replay.
        assert_eq!(vim.repeat_last(&mut b), Outcome::Unhandled);
    }
}

#[test]
fn only_a_plain_space_arms_the_quick_open_chord() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    // `f` on its own still starts a find.
    vim.handle_normal_key(Key::Char('f'), &mut b);
    assert_eq!(vim.pending_motion(), Some('f'));
    vim.enter_normal();
    // `d space` deletes to the right and does not arm the chord, so the
    // next `f` is a find.
    vim.handle_normal_key(Key::Char('d'), &mut b);
    vim.handle_normal_key(Key::Char(' '), &mut b);
    assert_eq!(vim.pending_motion(), None);
    vim.handle_normal_key(Key::Char('f'), &mut b);
    assert_eq!(vim.pending_motion(), Some('f'));
    vim.enter_normal();
    // Escape after a space drops the chord too.
    vim.handle_normal_key(Key::Char(' '), &mut b);
    vim.enter_normal();
    vim.handle_normal_key(Key::Char('f'), &mut b);
    assert_eq!(
        vim.pending_motion(),
        Some('f'),
        "f starts a find rather than opening the picker"
    );
}

#[test]
fn zm_and_zr_fold_and_unfold_every_section() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(4);
    type_keys(&mut vim, "zM", &mut b);
    let view = b.fold_view();
    assert_eq!(view.marker[0], Some(4));
    assert_eq!(
        view.marker[2], None,
        "nested folds hide under the outer one"
    );
    assert_eq!(view.marker[5], Some(2));
    assert_eq!(b.row_col(b.cursor()).0, 0);
    type_keys(&mut vim, "zR", &mut b);
    assert!(!b.has_closed_folds());
}

#[test]
fn z_waits_for_its_fold_command() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    type_keys(&mut vim, "z", &mut b);
    assert_eq!(vim.pending_motion(), Some('z'));
    // An unbound command letter cancels it without touching anything.
    assert_eq!(
        vim.handle_normal_key(Key::Char('q'), &mut b),
        Outcome::Unhandled
    );
    assert_eq!(vim.pending_motion(), None);
    assert!(!b.has_closed_folds());
    type_keys(&mut vim, "z", &mut b);
    vim.enter_normal();
    assert_eq!(vim.pending_motion(), None);
}
