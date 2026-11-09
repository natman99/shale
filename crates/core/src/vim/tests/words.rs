//! WORD motions: `W`, `B`, `E`.
//!
//! A WORD is delimited only by blanks, which makes it the motion that fits
//! Markdown: `**bold**`, `[[a link]]` and `code.rs` are each one WORD and several
//! words. Every expectation here was read off real vim (`vim -es file -c 'normal!
//! gg0…' -c 'w! out.txt'`), including where vim's cursor cannot go past the end of
//! a line and what an operator is therefore allowed to eat.

use super::*;

#[test]
fn capital_w_walks_markup_in_one_step_where_w_takes_three() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("**bold** text".to_string());

    type_keys(&mut vim, "w", &mut b);
    assert_eq!(b.cursor(), 2, "`w` stops after the opening `**`");
    b.set_cursor(0);
    type_keys(&mut vim, "W", &mut b);
    assert_eq!(b.cursor(), 9, "`W` takes `**bold**` whole");

    b.set_cursor(0);
    type_keys(&mut vim, "e", &mut b);
    assert_eq!(b.cursor(), 1, "`e` ends the punctuation run");
    b.set_cursor(0);
    type_keys(&mut vim, "E", &mut b);
    assert_eq!(b.cursor(), 7, "`E` ends the WORD, on the last `*`");
}

#[test]
fn capital_w_treats_punctuation_as_part_of_the_word() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    type_keys(&mut vim, "W", &mut b);
    assert_eq!(b.cursor(), 4, "`a,b` is one WORD");
    b.set_cursor(0);
    type_keys(&mut vim, "w", &mut b);
    assert_eq!(b.cursor(), 1, "and three words");
}

#[test]
fn capital_e_lands_on_the_last_character_of_the_word() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    type_keys(&mut vim, "E", &mut b);
    assert_eq!(b.cursor(), 2, "the `b` of `a,b`");
    type_keys(&mut vim, "E", &mut b);
    assert_eq!(
        b.cursor(),
        4,
        "then the `c`, the last character in the buffer"
    );
    type_keys(&mut vim, "E", &mut b);
    assert_eq!(
        b.cursor(),
        4,
        "and no further: `E` cannot step past the end"
    );
}

#[test]
fn capital_b_walks_back_to_the_start_of_the_word() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    b.set_cursor(4);
    type_keys(&mut vim, "B", &mut b);
    assert_eq!(
        b.cursor(),
        0,
        "from the start of a WORD, back over the previous one"
    );

    let mut b = Buffer::new("**bold** text".to_string());
    b.set_cursor(11); // the `x` of `text`
    type_keys(&mut vim, "B", &mut b);
    assert_eq!(b.cursor(), 9, "from the middle of a WORD, to its start");
}

#[test]
fn capital_b_at_the_start_of_the_buffer_goes_nowhere() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    type_keys(&mut vim, "4B", &mut b);
    assert_eq!(b.cursor(), 0);
    // And an operator over a motion that did not move registers nothing.
    type_keys(&mut vim, "yy", &mut b);
    type_keys(&mut vim, "4dB", &mut b);
    assert_eq!(b.text(), "a,b c\n", "nothing was swept");
    assert_eq!(
        vim.register_text(),
        Some("a,b c\n"),
        "so the register is untouched"
    );
}

#[test]
fn d_capital_w_sweeps_the_word_and_the_gap_after_it() {
    // The motion is exclusive, so the character it lands on survives — but the
    // blanks it stepped over go with the word, as vim's `dW` shows.
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    type_keys(&mut vim, "dW", &mut b);
    assert_eq!(b.text(), "c\n");

    let mut vim = Vim::new();
    let mut b = Buffer::new("**bold** text".to_string());
    type_keys(&mut vim, "dW", &mut b);
    assert_eq!(b.text(), "text", "the markup and the space after it");

    let mut vim = Vim::new();
    let mut b = Buffer::new("a  b\n".to_string());
    type_keys(&mut vim, "dW", &mut b);
    assert_eq!(b.text(), "b\n", "however wide the gap is");
}

#[test]
fn d_capital_e_is_inclusive() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    type_keys(&mut vim, "dE", &mut b);
    assert_eq!(b.text(), " c\n", "the WORD, and not the space after it");

    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    type_keys(&mut vim, "d2E", &mut b);
    assert_eq!(b.text(), "\n", "two WORDs, the last one taken whole");
}

#[test]
fn d_capital_b_sweeps_back_to_the_start_of_the_previous_word() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    b.set_cursor(4);
    type_keys(&mut vim, "dB", &mut b);
    assert_eq!(b.text(), "c\n");
    assert_eq!(vim.register_text(), Some("a,b "));
}

#[test]
fn a_word_motion_at_the_end_of_the_buffer_stops_at_the_line() {
    // vim's cursor cannot sit past the last character of a line, so a word
    // motion that has nowhere left to go ends the sweep at the line break
    // instead of eating it: the line is emptied, the line below stays below.
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    type_keys(&mut vim, "d4w", &mut b);
    assert_eq!(b.text(), "\n", "`w` included");

    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c\n".to_string());
    type_keys(&mut vim, "d2W", &mut b);
    assert_eq!(b.text(), "\n");

    let mut vim = Vim::new();
    let mut b = Buffer::new("foo\nbar\n".to_string());
    b.set_cursor(4);
    type_keys(&mut vim, "dW", &mut b);
    assert_eq!(b.text(), "foo\n\n");
}

#[test]
fn word_operators_repeat_with_the_dot() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c d\n".to_string());
    type_keys(&mut vim, "dW", &mut b);
    assert_eq!(b.text(), "c d\n");
    type_keys(&mut vim, ".", &mut b);
    assert_eq!(b.text(), "d\n", "the same sweep again");
}

#[test]
fn capital_w_inside_a_table_cell_stays_in_the_table() {
    // A WORD can be wider than a cell: `dW` at the end of one cell takes the rest
    // of that cell and stops at the pipe, rather than sweeping into the next.
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(6); // inside `bb`
    type_keys(&mut vim, "dW", &mut b);
    // The cell's content goes; its padding and the pipes stay, so the row is
    // still a row.
    assert_eq!(
        b.text(),
        "| a |  | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n"
    );

    // A WORD wider than the cell still steps cell to cell: the motion lands on
    // the next cell's content, never on the pipe between them.
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(6);
    type_keys(&mut vim, "W", &mut b);
    assert_eq!(b.cursor(), 11, "the `c` of `ccc`, one cell along");
}
