//! The one-keystroke edits: `r`, `~`, `s`, `S`, `C`, `D`, `X`, `Y`.
//!
//! These are the commands a vim user reaches for without thinking, and each is
//! vim's shorthand for an operator and a motion — the tests say so in the open,
//! because the equivalence (`s` is `cl`, `C` is `c$`, `Y` is `yy`) is what the
//! engine actually implements.

use super::*;
use crate::buffer::ViewSlot;

#[test]
fn r_replaces_the_character_under_the_cursor() {
    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "rx", &mut b);
    assert_eq!(b.text(), "xello world\nsecond line\nthird");
    // The cursor stays on what was written.
    assert_eq!(b.cursor(), 0);

    // A replace is not a change: the register is untouched, because vim's `r`
    // destroys one character and does not cut it.
    let mut b = buf();
    type_keys(&mut vim, "yy", &mut b);
    type_keys(&mut vim, "rx", &mut b);
    assert_eq!(vim.register_text(), Some("hello world\n"));
}

#[test]
fn r_takes_a_count_and_lands_on_the_last_character() {
    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "3rx", &mut b);
    assert_eq!(b.text(), "xxxlo world\nsecond line\nthird");
    assert_eq!(b.cursor(), 2);
}

#[test]
fn r_takes_a_digit_as_its_character() {
    // The count is over once `r` is typed: `r5` replaces with `5`, it does not
    // become "five of nothing".
    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "r5", &mut b);
    assert_eq!(b.text(), "5ello world\nsecond line\nthird");
    assert!(!vim.waiting_for_literal());
}

#[test]
fn r_replaces_one_character_of_a_multi_byte_one() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("héllo".to_string());
    b.set_cursor(1); // the é, two bytes
    type_keys(&mut vim, "rx", &mut b);
    assert_eq!(b.text(), "hxllo");
    assert_eq!(b.cursor(), 1);
}

#[test]
fn r_with_a_line_break_joins_the_line_it_touched() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(10); // the 'd' of "world"
    type_keys(&mut vim, "r\n", &mut b);
    // vim replaces the character with a single line break; the one that was
    // already there stays, so the line ends early rather than merging.
    assert_eq!(b.text(), "hello worl\n\nsecond line\nthird");
}

#[test]
fn r_does_not_take_apart_a_table() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(2); // the `a` of the first cell
                     // Replacing across the cell's edge stops at the content: the pipe stays.
    type_keys(&mut vim, "4rx", &mut b);
    assert_eq!(
        b.text(),
        "| x | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n"
    );
    // And a pipe is not a character to replace a cell's content with: that would
    // split the row.
    let mut b = table_buf();
    b.set_cursor(2);
    type_keys(&mut vim, "r|", &mut b);
    assert_eq!(
        b.text(),
        "| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n"
    );
}

#[test]
fn tilde_switches_case_and_moves_onto_the_next_character() {
    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "~", &mut b);
    assert_eq!(b.text(), "Hello world\nsecond line\nthird");
    // vim moves the cursor right after switching, which is what makes `~` usable
    // over a word one keystroke at a time.
    assert_eq!(b.cursor(), 1);

    let mut b = buf();
    type_keys(&mut vim, "3~", &mut b);
    assert_eq!(b.text(), "HELlo world\nsecond line\nthird");
    // Checked against vim: `3~` ends one past the last character it switched, not
    // on it — `~` is a command that moves, not one that stays.
    assert_eq!(b.cursor(), 3);
}

#[test]
fn tilde_repeats_over_the_word() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("abc def".to_string());
    type_keys(&mut vim, "~", &mut b);
    assert_eq!(b.text(), "Abc def");
    type_keys(&mut vim, ".", &mut b);
    assert_eq!(b.text(), "ABc def");
    type_keys(&mut vim, ".", &mut b);
    assert_eq!(b.text(), "ABC def");
    assert_eq!(b.cursor(), 3); // the space
                               // The next `.` switches the space — a no-op — and steps again.
    type_keys(&mut vim, ".", &mut b);
    assert_eq!(b.text(), "ABC def");
    assert_eq!(b.cursor(), 4);
}

#[test]
fn s_is_cl() {
    let mut vim = Vim::new();
    let mut b = buf();
    let outcome = vim.handle_normal_key(Key::Char('s'), &mut b, &mut ViewSlot::new());
    assert_eq!(outcome, Outcome::EnterInsert);
    assert_eq!(b.text(), "ello world\nsecond line\nthird");
    assert_eq!(vim.register_text(), Some("h"));

    let mut vim = Vim::new();
    let mut b = buf();
    vim.handle_normal_key(Key::Char('2'), &mut b, &mut ViewSlot::new());
    vim.handle_normal_key(Key::Char('s'), &mut b, &mut ViewSlot::new());
    assert_eq!(b.text(), "llo world\nsecond line\nthird");
    assert_eq!(vim.register_text(), Some("he"));
}

#[test]
fn c_is_c_dollar_and_d_is_d_dollar() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(6); // the `w` of "world"
    let outcome = vim.handle_normal_key(Key::Char('C'), &mut b, &mut ViewSlot::new());
    assert_eq!(outcome, Outcome::EnterInsert);
    assert_eq!(b.text(), "hello \nsecond line\nthird");
    assert_eq!(vim.register_text(), Some("world"));

    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(6);
    type_keys(&mut vim, "D", &mut b);
    assert_eq!(b.text(), "hello \nsecond line\nthird");
    assert_eq!(vim.register_text(), Some("world"));
    // Both stop at the line break: neither eats it.
}

#[test]
fn x_is_dh_and_leaves_the_register_when_there_is_nothing_to_delete() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(6);
    type_keys(&mut vim, "X", &mut b);
    // The space goes: `X` is `dh`, the character *before* the cursor.
    assert_eq!(b.text(), "helloworld\nsecond line\nthird");
    assert_eq!(vim.register_text(), Some(" "));

    // At the start of a line there is nothing before the cursor. vim refuses, and
    // the important part of refusing is *not* overwriting the register with an
    // empty delete.
    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "yw", &mut b);
    let yanked = vim.register_text().map(str::to_string);
    b.set_cursor(0);
    type_keys(&mut vim, "X", &mut b);
    assert_eq!(b.text(), "hello world\nsecond line\nthird");
    assert_eq!(vim.register_text().map(str::to_string), yanked);
}

#[test]
fn s_capital_is_cc_and_y_capital_is_yy() {
    let mut vim = Vim::new();
    let mut b = buf();
    let outcome = vim.handle_normal_key(Key::Char('S'), &mut b, &mut ViewSlot::new());
    assert_eq!(outcome, Outcome::EnterInsert);
    assert_eq!(
        b.text(),
        "\nsecond line\nthird",
        "the line is emptied, not deleted"
    );
    assert_eq!(vim.register_text(), Some("hello world\n"));

    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "Y", &mut b);
    assert_eq!(b.text(), "hello world\nsecond line\nthird");
    assert_eq!(vim.register_text(), Some("hello world\n"));
    // A linewise register pastes as its own line, which is the point of `Y` being
    // `yy` rather than `y$`.
    type_keys(&mut vim, "p", &mut b);
    assert_eq!(b.text(), "hello world\nhello world\nsecond line\nthird");
}

#[test]
fn a_linewise_change_leaves_the_line_behind() {
    // `dd` takes the line break with the line; `cc` and `S` do not, because the
    // line they empty is the line insert mode starts on.
    let mut vim = Vim::new();
    let mut b = Buffer::new("one\ntwo\nthree".to_string());
    type_keys(&mut vim, "cc", &mut b);
    assert_eq!(b.text(), "\ntwo\nthree");
    assert_eq!(b.cursor(), 0, "the cursor is on the emptied line");
    b.insert("X");
    assert_eq!(b.text(), "X\ntwo\nthree");

    let mut vim = Vim::new();
    let mut b = Buffer::new("one\ntwo".to_string());
    b.set_cursor(4); // the last line
    type_keys(&mut vim, "S", &mut b);
    assert_eq!(b.text(), "one\n");
    b.insert("Y");
    assert_eq!(b.text(), "one\nY");
    // The register still holds the line, so `p` can put it back somewhere else.
    // (A final line with no trailing break is yanked without one; that is how
    // `yy` has always treated the last line.)
    assert_eq!(vim.register_text(), Some("two"));
}

#[test]
fn an_operator_over_nothing_registers_nothing() {
    // `c0` at column zero has no text to change. vim refuses, and does not open
    // insert mode over the empty range.
    let mut vim = Vim::new();
    let mut b = buf();
    let outcome = vim.handle_normal_key(Key::Char('0'), &mut b, &mut ViewSlot::new());
    assert_eq!(outcome, Outcome::Stay);
    vim.handle_normal_key(Key::Char('c'), &mut b, &mut ViewSlot::new());
    let outcome = vim.handle_normal_key(Key::Char('0'), &mut b, &mut ViewSlot::new());
    assert_ne!(outcome, Outcome::EnterInsert);
    assert_eq!(b.text(), "hello world\nsecond line\nthird");
    assert_eq!(vim.register_text(), None);
}

#[test]
fn the_new_edits_repeat_with_dot() {
    // `~` is the clearest case: the cursor walks right, so `.` keeps switching.
    let mut vim = Vim::new();
    let mut b = Buffer::new("cat".to_string());
    type_keys(&mut vim, "~", &mut b);
    type_keys(&mut vim, ".", &mut b);
    assert_eq!(b.text(), "CAt"); // two switched so far, the cursor is on the `t`
    type_keys(&mut vim, ".", &mut b);
    assert_eq!(b.text(), "CAT");

    // `C` records the typed text, as every change does, and `.` replays both
    // halves on the next line.
    let mut vim = Vim::new();
    let mut b = Buffer::new("abc\ndef\n".to_string());
    type_keys(&mut vim, "C", &mut b);
    assert_eq!(b.text(), "\ndef\n");
    // The UI is what types; the engine only records what was typed, so the test
    // has to do both, the way the app does.
    vim.note_inserted("X");
    b.insert_at(b.cursor(), "X");
    b.move_right();
    vim.enter_normal();
    assert_eq!(b.text(), "X\ndef\n");
    // The next line, and the same change again.
    b.set_cursor(2); // the `d`
    type_keys(&mut vim, ".", &mut b);
    assert_eq!(b.text(), "X\nX\n");
}

#[test]
fn a_read_only_note_refuses_the_new_edits() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_read_only(true);
    type_keys(&mut vim, "rx", &mut b);
    type_keys(&mut vim, "~", &mut b);
    type_keys(&mut vim, "s", &mut b);
    type_keys(&mut vim, "D", &mut b);
    assert_eq!(b.text(), "hello world\nsecond line\nthird");
    // Yank asks nothing of the buffer, so it still works in a note being read.
    type_keys(&mut vim, "Y", &mut b);
    assert_eq!(vim.register_text(), Some("hello world\n"));
}
