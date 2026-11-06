//! The overlay's layer of the keys (roadmap P2.5b). The engine is told which lines a
//! proposal is sitting on and answers the one question the two-layer scheme turns on:
//! does this bare `y`/`x`/`za` belong to the proposal or to vim. Everything the queue
//! itself does — the diff, the accept, the note it belongs to — stays with the window.

use super::*;

const NOTE: &str = "one\ntwo\nthree\nfour\n";

/// A buffer with the cursor on line `row`.
fn on(row: usize) -> (Vim, Buffer) {
    let mut b = Buffer::new(NOTE.to_string());
    b.set_cursor(b.line_start(row));
    (Vim::new(), b)
}

/// The proposal covers lines 1 and 2 — the lines a hunk would take away, not its
/// context, which the overlay leaves alone.
fn under_review(vim: &mut Vim) {
    vim.set_review_lines(vec![(1, 2)]);
}

#[test]
fn y_on_a_line_under_review_asks_for_the_accept_and_yanks_nothing() {
    let (mut vim, mut b) = on(1);
    under_review(&mut vim);
    assert_eq!(
        vim.handle_normal_key(Key::Char('y'), &mut b),
        Outcome::ReviewAccept
    );
    assert_eq!(b.text(), NOTE, "the note is untouched");
    assert!(
        vim.register_dump().is_empty(),
        "nothing went into a register: {:?}",
        vim.register_dump()
    );
}

/// The whole point of the two-layer scheme: one line away, `y` is vim's again. A
/// proposal that turned `y` into "accept" wherever the cursor happens to rest would
/// break the key the user is reaching for in prose.
#[test]
fn y_one_line_away_is_still_the_yank() {
    let (mut vim, mut b) = on(3);
    under_review(&mut vim);
    assert_eq!(
        vim.handle_normal_key(Key::Char('y'), &mut b),
        Outcome::Stay,
        "line 3 is not under review"
    );
    vim.handle_normal_key(Key::Char('y'), &mut b);
    assert!(
        vim.register_text().is_some_and(|t| t.contains("four")),
        "the yank landed in a register: {:?}",
        vim.register_dump()
    );
}

#[test]
fn x_on_a_line_under_review_asks_for_the_reject_and_deletes_nothing() {
    let (mut vim, mut b) = on(2);
    under_review(&mut vim);
    assert_eq!(
        vim.handle_normal_key(Key::Char('x'), &mut b),
        Outcome::ReviewReject
    );
    assert_eq!(b.text(), NOTE, "`x` did not eat a character");
}

#[test]
fn x_off_the_review_lines_still_deletes_the_character() {
    let (mut vim, mut b) = on(3);
    under_review(&mut vim);
    assert_eq!(vim.handle_normal_key(Key::Char('x'), &mut b), Outcome::Stay);
    assert_eq!(b.text(), "one\ntwo\nthree\nour\n", "the `f` went");
}

/// `za` is the one command the overlay takes from vim on those lines: it collapses the
/// proposal rather than folding the section around it.
#[test]
fn za_on_a_line_under_review_collapses_the_proposal_not_the_section() {
    let mut b = Buffer::new("# Top\nintro\n## Sub\nbody\n".to_string());
    b.set_cursor(b.line_start(1));
    let mut vim = Vim::new();
    under_review(&mut vim);
    vim.handle_normal_key(Key::Char('z'), &mut b);
    assert_eq!(
        vim.handle_normal_key(Key::Char('a'), &mut b),
        Outcome::ReviewCollapse
    );
    assert!(
        b.fold_view().marker.iter().all(|m| m.is_none()),
        "and no fold was opened anywhere: the key went to the proposal"
    );
}

#[test]
fn za_elsewhere_is_the_fold_it_always_was() {
    let mut b = Buffer::new("# Top\nintro\n## Sub\nbody\n".to_string());
    // The cursor is on the heading's own line, outside the proposal's lines.
    b.set_cursor(b.line_start(0));
    let mut vim = Vim::new();
    under_review(&mut vim);
    vim.handle_normal_key(Key::Char('z'), &mut b);
    assert_eq!(vim.handle_normal_key(Key::Char('a'), &mut b), Outcome::Stay);
    let view = b.fold_view();
    assert!(
        view.marker[0].is_some(),
        "the heading opened a fold, which is what `za` has always done"
    );
    assert!(
        view.collapsed.get(1).is_some_and(|c| *c),
        "and the line under it is hidden"
    );
}

/// A count means the user is still spelling a vim command: `2y` is a yank of two lines,
/// not an accept with a number in front of it.
#[test]
fn a_count_keeps_y_vim_s() {
    let (mut vim, mut b) = on(1);
    under_review(&mut vim);
    vim.handle_normal_key(Key::Char('2'), &mut b);
    assert_eq!(vim.handle_normal_key(Key::Char('y'), &mut b), Outcome::Stay);
    assert_eq!(
        vim.pending_operator(),
        Some('y'),
        "the operator is waiting for its motion"
    );
}

/// An operator waiting for its motion is the same story: the `y` of `dy` is part of a
/// delete, not a request to accept.
#[test]
fn a_pending_operator_keeps_y_vim_s() {
    let (mut vim, mut b) = on(1);
    under_review(&mut vim);
    vim.handle_normal_key(Key::Char('d'), &mut b);
    let outcome = vim.handle_normal_key(Key::Char('y'), &mut b);
    assert!(
        !matches!(outcome, Outcome::ReviewAccept),
        "the `y` of `dy` is part of a delete: {outcome:?}"
    );
    assert_eq!(b.text(), NOTE, "nothing was accepted, and nothing was deleted");
}

/// In visual mode the selection is what the user is holding, and `x` there is vim's
/// delete of the selection — the overlay answers to the cursor, not to a range the
/// cursor is inside.
#[test]
fn visual_mode_keeps_its_own_keys() {
    let (mut vim, mut b) = on(1);
    under_review(&mut vim);
    vim.handle_normal_key(Key::Char('v'), &mut b);
    vim.handle_normal_key(Key::Char('l'), &mut b);
    assert_eq!(vim.mode(), Mode::Visual);
    assert_eq!(vim.handle_normal_key(Key::Char('x'), &mut b), Outcome::Stay);
    assert_eq!(b.text(), "one\no\nthree\nfour\n", "the selection went, not the proposal");
}

/// With nothing proposed, the engine has no lines to answer about and every key is
/// ordinary — which is the state the note is in almost all the time.
#[test]
fn with_nothing_pending_everything_is_ordinary() {
    let (mut vim, mut b) = on(1);
    assert_eq!(vim.handle_normal_key(Key::Char('x'), &mut b), Outcome::Stay);
    assert_eq!(b.text(), "one\nwo\nthree\nfour\n", "the character under the cursor went");
}
