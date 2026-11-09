//! vim's `J` — join the line below onto this one — and the whitespace rules that
//! come with it. Every expectation here was read off real vim; the rules are not
//! the obvious ones.
//!
//! The short version: the break becomes a space, the next line's leading
//! whitespace goes with it, a blank next line contributes no space at all, a line
//! that already ends in whitespace does not get a second separator, and after
//! `.`, `?` or `!` vim's default `joinspaces` inserts two.

use super::*;

fn join(text: &str, keys: &str) -> (String, usize) {
    let mut vim = Vim::new();
    let mut b = Buffer::new(text.to_string());
    type_keys(&mut vim, keys, &mut b);
    (b.text().to_string(), b.cursor())
}

#[test]
fn j_joins_with_a_single_space_and_parks_the_cursor_on_it() {
    let (text, cursor) = join("one\ntwo\nthree\n", "J");
    assert_eq!(text, "one two\nthree\n");
    assert_eq!(cursor, 3, "the cursor sits on the space the join made");

    // A second `J` walks on, because the cursor is on the join, not behind it.
    let (text, _) = join("one\ntwo\nthree\n", "JJ");
    assert_eq!(text, "one two three\n");
}

#[test]
fn a_count_is_the_lines_the_result_spans() {
    let (text, cursor) = join("one\ntwo\nthree\n", "3J");
    assert_eq!(text, "one two three\n");
    assert_eq!(cursor, 3, "and still on the first join");
}

#[test]
fn the_next_line_leading_whitespace_goes_with_the_break() {
    let (text, _) = join("one\n   two\n", "J");
    assert_eq!(text, "one two\n", "not `one    two`");
}

#[test]
fn a_blank_next_line_adds_no_space() {
    let (text, cursor) = join("one\n\ntwo\n", "J");
    assert_eq!(text, "one\ntwo\n", "the blank line is what got joined");
    assert_eq!(
        cursor, 2,
        "nothing was inserted, so the cursor stays on the line"
    );

    // Whitespace-only counts as blank, and its whitespace goes too.
    let (text, _) = join("one\n   \ntwo\n", "J");
    assert_eq!(text, "one\ntwo\n");
}

#[test]
fn a_line_ending_in_whitespace_does_not_get_a_second_separator() {
    let (text, _) = join("one  \ntwo\n", "J");
    assert_eq!(text, "one  two\n");

    let (text, _) = join("one\t\ntwo\n", "J");
    assert_eq!(text, "one\ttwo\n");
}

#[test]
fn joinspaces_puts_two_after_a_sentence_end() {
    // vim's default, and Shale keeps it. Mid-line the second space means nothing
    // to Markdown; only a trailing pair means a break.
    let (text, _) = join("one.\ntwo\n", "J");
    assert_eq!(text, "one.  two\n");

    let (text, _) = join("one?\ntwo\n", "J");
    assert_eq!(text, "one?  two\n");

    let (text, _) = join("one;\ntwo\n", "J");
    assert_eq!(text, "one; two\n", "a semicolon is not a sentence end");
}

#[test]
fn the_last_line_of_a_note_has_nothing_to_join() {
    // The break that ends a note ends the file; it is not an empty line below.
    let (text, cursor) = join("one\n", "J");
    assert_eq!(text, "one\n", "nothing happened");
    assert_eq!(cursor, 0, "and the cursor did not move");

    let (text, _) = join("one\ntwo\n", "2G0J");
    assert_eq!(text, "one\ntwo\n");
}

#[test]
fn a_note_that_really_does_end_in_a_blank_line_can_join_it() {
    let (text, _) = join("one\n\n", "J");
    assert_eq!(text, "one\n");
}

#[test]
fn visual_j_joins_the_selected_lines() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("one\ntwo\nthree\nfour\n".to_string());
    b.set_cursor(0);
    type_keys(&mut vim, "VjjJ", &mut b);
    assert_eq!(b.text(), "one two three\nfour\n");
    assert_eq!(b.cursor(), 3, "on the first join");

    // A charwise selection joins the lines it touches, not just the characters.
    let mut vim = Vim::new();
    let mut b = Buffer::new("one\ntwo\nthree\n".to_string());
    b.set_cursor(1);
    type_keys(&mut vim, "vljjJ", &mut b);
    assert_eq!(b.text(), "one two three\n");
}

#[test]
fn j_repeats_with_the_dot() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("one\ntwo\nthree\nfour\n".to_string());
    b.set_cursor(0);
    type_keys(&mut vim, "J", &mut b);
    b.set_cursor(b.text().find("two").unwrap());
    type_keys(&mut vim, ".", &mut b);
    assert_eq!(b.text(), "one two three\nfour\n");
}
