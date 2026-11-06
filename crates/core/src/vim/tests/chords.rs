//! The keys that ask the UI for something rather than edit the note: `gx`, `K`,
//! the `space` leader's chords, and the fold commands.

use super::*;
use crate::buffer::ViewSlot;

#[test]
fn gx_requests_link_follow() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(20);
    vim.handle_normal_key(Key::Char('g'), &mut b, &ViewSlot::new());
    assert_eq!(
        vim.handle_normal_key(Key::Char('x'), &mut b, &ViewSlot::new()),
        Outcome::FollowLink
    );
    // The motion-less `gx` leaves the cursor where it was.
    assert_eq!(b.cursor(), 20);
    assert_eq!(vim.mode(), Mode::Normal);
}

#[test]
fn gd_and_g_capital_requests_the_same_link_and_the_creating_one() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(20);
    vim.handle_normal_key(Key::Char('g'), &mut b, &ViewSlot::new());
    // `gd` — go to definition, the LSP habit — is the same request as `gx`.
    assert_eq!(
        vim.handle_normal_key(Key::Char('d'), &mut b, &ViewSlot::new()),
        Outcome::FollowLink
    );
    vim.handle_normal_key(Key::Char('g'), &mut b, &ViewSlot::new());
    // `gD` asks for the same thing with the note made first if there isn't one.
    assert_eq!(
        vim.handle_normal_key(Key::Char('D'), &mut b, &ViewSlot::new()),
        Outcome::CreateLink
    );
    assert_eq!(b.cursor(), 20, "neither chord moves the cursor");
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
        vim.handle_normal_key(Key::Char('K'), &mut b, &ViewSlot::new()),
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
        vim.handle_normal_key(Key::Char('K'), &mut b, &ViewSlot::new()),
        Outcome::Stay,
        "the table answered it"
    );
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "a",
        "one row up in the same column"
    );
}

/// `g t` / `g T` (roadmap P3.1): the engine owns the chord because `t` and `T` are
/// already the find motions, so the keymap needs no new binding — and the chord has
/// to be resolved here rather than as a two-stroke keymap binding, which would
/// swallow the `g`.
#[test]
fn g_t_and_upper_t_ask_for_the_next_and_previous_tab() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(2);
    vim.handle_normal_key(Key::Char('g'), &mut b, &ViewSlot::new());
    assert_eq!(
        vim.handle_normal_key(Key::Char('t'), &mut b, &ViewSlot::new()),
        Outcome::TabNext
    );
    vim.handle_normal_key(Key::Char('g'), &mut b, &ViewSlot::new());
    assert_eq!(
        vim.handle_normal_key(Key::Char('T'), &mut b, &ViewSlot::new()),
        Outcome::TabPrev
    );
    // Neither moves the cursor: the window decides what tab is on screen.
    assert_eq!(b.cursor(), 2);
    assert_eq!(vim.mode(), Mode::Normal);
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
    // The leader is armed and nothing else happens: a lone `space` waits for its
    // second key rather than moving the cursor under the user.
    assert_eq!(
        vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    assert_eq!(b.cursor(), 0, "space moves nothing");
    assert_eq!(
        vim.handle_normal_key(Key::Char('f'), &mut b, &ViewSlot::new()),
        Outcome::QuickOpen
    );
    // The chord is not a change, so `.` has nothing to replay.
    assert_eq!(
        vim.repeat_last(&mut b, &ViewSlot::new()),
        Outcome::Unhandled
    );
}

#[test]
fn g_f_asks_the_ui_for_the_quick_open_picker() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    assert_eq!(b.cursor(), 0);
    // `g` arms the goto chord, exactly as it does for `gg` and `gx`.
    assert_eq!(
        vim.handle_normal_key(Key::Char('g'), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    assert_eq!(b.cursor(), 0, "`g` alone does not move");
    assert_eq!(
        vim.handle_normal_key(Key::Char('f'), &mut b, &ViewSlot::new()),
        Outcome::QuickOpen
    );
    // The chord is not a change, so `.` has nothing to replay.
    assert_eq!(
        vim.repeat_last(&mut b, &ViewSlot::new()),
        Outcome::Unhandled
    );
}

/// `space` is the leader and not a motion: it arms the chords and leaves the cursor
/// exactly where it was, in prose and inside a table cell alike. vim binds space to
/// `l`, and giving that up is the point — a key that moves first makes the window
/// jump under someone who is reaching for a binding.
#[test]
fn a_lone_space_moves_nothing() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    // `l` is the right motion, and still moves.
    vim.handle_normal_key(Key::Char('l'), &mut b, &ViewSlot::new());
    let moved = b.cursor();
    assert!(moved > 0, "`l` moved right");
    assert_eq!(
        vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    assert_eq!(b.cursor(), moved, "space moved nothing");
    assert!(vim.awaiting_chord(), "and the leader is armed");

    // Inside a table, `space` used to step to the next cell. `l` still does; the
    // leader means the same thing everywhere or it means nothing.
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(2);
    vim.handle_normal_key(Key::Char('l'), &mut b, &ViewSlot::new());
    assert!(b.cursor() > 2, "`l` steps the cell");
    b.set_cursor(2);
    assert_eq!(
        vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    assert_eq!(b.cursor(), 2, "space stepped no cell");
}

#[test]
fn space_s_asks_the_ui_for_vault_search() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    assert_eq!(
        vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    assert_eq!(
        vim.handle_normal_key(Key::Char('s'), &mut b, &ViewSlot::new()),
        Outcome::SearchVault
    );
    // The chord is not a change, so `.` has nothing to replay.
    assert_eq!(
        vim.repeat_last(&mut b, &ViewSlot::new()),
        Outcome::Unhandled
    );
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
        // As with the other chords, the leader arms without moving anything first.
        assert_eq!(
            vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new()),
            Outcome::Stay
        );
        assert_eq!(
            vim.handle_normal_key(Key::Char(key), &mut b, &ViewSlot::new()),
            want,
            "space {key}"
        );
        // A chord is not a change, so `.` has nothing to replay.
        assert_eq!(
            vim.repeat_last(&mut b, &ViewSlot::new()),
            Outcome::Unhandled
        );
    }
}

#[test]
fn space_g_asks_the_ui_for_the_graph() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    assert_eq!(
        vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    assert_eq!(
        vim.handle_normal_key(Key::Char('g'), &mut b, &ViewSlot::new()),
        Outcome::OpenGraph,
        "the leader's `g` is the graph, not the start of a `g` command"
    );
    // A chord, not a change, and it leaves nothing pending: the graph is the window's
    // answer, and the next key starts a fresh command.
    assert_eq!(
        vim.repeat_last(&mut b, &ViewSlot::new()),
        Outcome::Unhandled
    );
    let before = b.text().to_string();
    assert_eq!(
        vim.handle_normal_key(Key::Char('d'), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    vim.handle_normal_key(Key::Char('w'), &mut b, &ViewSlot::new());
    assert_ne!(b.text(), before, "`dw` still works after the chord");
}

#[test]
fn space_y_asks_the_ui_for_the_system_clipboard() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    assert_eq!(
        vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    assert_eq!(
        vim.handle_normal_key(Key::Char('y'), &mut b, &ViewSlot::new()),
        Outcome::YankClipboard
    );
    // A chord, not a change — and not a yank either. The engine's own register is
    // still empty, because `space y` asks the window for the *system* clipboard
    // rather than remembering anything itself.
    assert_eq!(
        vim.repeat_last(&mut b, &ViewSlot::new()),
        Outcome::Unhandled
    );
    assert_eq!(vim.register_text(), None);

    // Plain `y` is still vim's yank operator, and its text is what the window then
    // has to offer the clipboard.
    vim.handle_normal_key(Key::Char('y'), &mut b, &ViewSlot::new());
    vim.handle_normal_key(Key::Char('y'), &mut b, &ViewSlot::new());
    assert_eq!(vim.register_text(), Some("# Top\n"));

    // The other direction: the clipboard fills the register, and `p` pastes it.
    // Text that arrives with its own line break is linewise, so it lands on a line
    // of its own rather than in the middle of one.
    vim.set_register("from outside\n");
    vim.handle_normal_key(Key::Char('p'), &mut b, &ViewSlot::new());
    assert_eq!(
        b.text(),
        "# Top\nfrom outside\nintro\n## Sub\nbody\ndeep\n# Other\ntail\n",
        "`p` pastes what the clipboard gave, on its own line"
    );
}

#[test]
fn a_change_is_a_cut_too() {
    // vim puts what a change replaces into the register, so `cw` then `p` can put
    // the old word back. The app mirrors the register, so this is a cut other
    // programs see as well.
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(1);
    type_keys(&mut vim, "ciw", &mut b);
    vim.enter_normal();
    assert_eq!(vim.register_text(), Some("intro"));

    // Linewise: `cc` then `p` moves the line it replaced.
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(1);
    type_keys(&mut vim, "cc", &mut b);
    assert_eq!(
        b.text(),
        "# Top\n\n## Sub\nbody\ndeep\n# Other\ntail\n",
        "the line is emptied, not removed: `cc` keeps the line it changes"
    );
    vim.enter_normal();
    type_keys(&mut vim, "p", &mut b);
    assert_eq!(
        b.text(),
        // Pasted below the emptied line, which is still a line.
        "# Top\n\nintro\n## Sub\nbody\ndeep\n# Other\ntail\n",
        "and `p` puts it back below the cursor"
    );
}

#[test]
fn a_linewise_delete_is_a_cut() {
    // `dd` then `p` moves the line, as it does in vim. Until the register was
    // filled here, `p` after `dd` pasted whatever an earlier yank had left.
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(2);
    type_keys(&mut vim, "dd", &mut b);
    assert_eq!(
        b.text(),
        "# Top\nintro\nbody\ndeep\n# Other\ntail\n",
        "the row is gone"
    );
    type_keys(&mut vim, "p", &mut b);
    assert_eq!(
        b.text(),
        "# Top\nintro\nbody\n## Sub\ndeep\n# Other\ntail\n",
        "and `p` puts it back below the line the cursor landed on"
    );
}

#[test]
fn the_engine_says_when_a_key_arrives_mid_command() {
    // The UI asks this before treating a `p` as a paste, because only a `p` that
    // will really paste may pull the system clipboard into the register.
    let mut vim = Vim::new();
    let mut b = note_buf();
    assert!(!vim.awaiting_chord());

    vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new());
    assert!(vim.awaiting_chord(), "the leader waits for its second key");
    vim.handle_normal_key(Key::Char('p'), &mut b, &ViewSlot::new());
    assert!(
        !vim.awaiting_chord(),
        "and a chord ends when it is answered"
    );

    vim.handle_normal_key(Key::Char('d'), &mut b, &ViewSlot::new());
    assert!(vim.awaiting_chord(), "an operator waits for its motion");
    vim.handle_normal_key(Key::Char('i'), &mut b, &ViewSlot::new());
    assert!(vim.awaiting_chord(), "a text object waits for its scope");
    vim.handle_normal_key(Key::Char('w'), &mut b, &ViewSlot::new());
    assert!(
        !vim.awaiting_chord(),
        "and the command is over once it lands"
    );
}

#[test]
fn only_a_plain_space_arms_the_quick_open_chord() {
    let mut vim = Vim::new();
    let mut b = note_buf();
    b.goto_line(0);
    // `f` on its own still starts a find.
    vim.handle_normal_key(Key::Char('f'), &mut b, &ViewSlot::new());
    assert_eq!(vim.pending_motion(), Some('f'));
    vim.enter_normal();
    // `d space` deletes to the right and does not arm the chord, so the
    // next `f` is a find.
    vim.handle_normal_key(Key::Char('d'), &mut b, &ViewSlot::new());
    vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new());
    assert_eq!(vim.pending_motion(), None);
    vim.handle_normal_key(Key::Char('f'), &mut b, &ViewSlot::new());
    assert_eq!(vim.pending_motion(), Some('f'));
    vim.enter_normal();
    // Escape after a space drops the chord too.
    vim.handle_normal_key(Key::Char(' '), &mut b, &ViewSlot::new());
    vim.enter_normal();
    vim.handle_normal_key(Key::Char('f'), &mut b, &ViewSlot::new());
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
        vim.handle_normal_key(Key::Char('q'), &mut b, &ViewSlot::new()),
        Outcome::Unhandled
    );
    assert_eq!(vim.pending_motion(), None);
    assert!(!b.has_closed_folds());
    type_keys(&mut vim, "z", &mut b);
    vim.enter_normal();
    assert_eq!(vim.pending_motion(), None);
}

/// `]a` and `[a` hop the agent-review marker. The engine has no queue and no idea
/// one exists, so it asks the window — the same shape as the view chords.
#[test]
fn bracket_a_requests_the_review_marker() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(2);
    assert_eq!(
        vim.handle_normal_key(Key::Char(']'), &mut b, &ViewSlot::new()),
        Outcome::Stay,
        "the bracket waits for the letter that makes it a command"
    );
    assert!(vim.awaiting_chord(), "and it is a half-typed command");
    assert_eq!(
        vim.handle_normal_key(Key::Char('a'), &mut b, &ViewSlot::new()),
        Outcome::ReviewNext
    );
    assert_eq!(
        vim.handle_normal_key(Key::Char('['), &mut b, &ViewSlot::new()),
        Outcome::Stay
    );
    assert_eq!(
        vim.handle_normal_key(Key::Char('a'), &mut b, &ViewSlot::new()),
        Outcome::ReviewPrev
    );
    assert_eq!(b.cursor(), 2, "and the note was never touched");
    assert_eq!(b.text(), "hello world\nsecond line\nthird");
    assert!(!vim.awaiting_chord(), "the chord is over");
}

/// A `]` that does not get its `a` is a command Shale has no use for: it is
/// cancelled, and the key that cancelled it is not run either — so `]x` does not
/// delete a character.
#[test]
fn a_bracket_without_its_a_does_nothing() {
    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "]", &mut b);
    vim.handle_normal_key(Key::Char('x'), &mut b, &ViewSlot::new());
    assert_eq!(b.text(), "hello world\nsecond line\nthird");
    assert!(!vim.awaiting_chord());
}

/// The reason `]` and `[` are bound as single keys rather than as a two-stroke
/// chord: a literal bracket is the argument of a find, and it has to reach the
/// engine rather than be held as pending keymap input.
#[test]
fn a_bracket_is_still_a_literal_find_target() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("see [the link] here".to_string());
    type_keys(&mut vim, "f[", &mut b);
    assert_eq!(
        b.cursor(),
        b.text().find('[').unwrap(),
        "`f[` landed on the bracket"
    );
    type_keys(&mut vim, "f]", &mut b);
    assert_eq!(b.cursor(), b.text().find(']').unwrap());
    // An operator can be given a bracket as its motion's argument too — `d[` is not a
    // Shale motion, so the operator is cancelled rather than let loose on the note.
    let mut b = Buffer::new("see [the link] here".to_string());
    b.set_cursor(b.text().find(']').unwrap());
    type_keys(&mut vim, "d[", &mut b);
    assert_eq!(b.text(), "see [the link] here");
}

/// `]` inside a half-typed command is a literal, not the start of a chord.
#[test]
fn a_bracket_after_an_operator_is_not_a_chord() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("see [the link] here".to_string());
    type_keys(&mut vim, "y", &mut b);
    assert!(
        vim.awaiting_chord(),
        "the operator is waiting for its motion"
    );
    // `y[` is not a Shale motion, so nothing is yanked and nothing is queued.
    vim.handle_normal_key(Key::Char('['), &mut b, &ViewSlot::new());
    assert!(!vim.awaiting_chord());
    vim.handle_normal_key(Key::Char('a'), &mut b, &ViewSlot::new());
    assert_eq!(b.text(), "see [the link] here");
}
