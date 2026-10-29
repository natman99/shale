//! The table bindings: cell motions, row and column edits, and the operator
//! ranges that stop at a cell's edge.

use crate::table;

use super::*;

#[test]
fn table_cell_motions_step_between_cells() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    type_keys(&mut vim, "L", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 2],
        "22",
        "one column right"
    );
    type_keys(&mut vim, "H", &mut b);
    assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "1", "back left");
    type_keys(&mut vim, "J", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "4",
        "shift-J steps to the cell below"
    );
    type_keys(&mut vim, "K", &mut b);
    assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "1");
    // A count jumps that many cells; past the bottom of the table the rest of
    // the count becomes an ordinary line motion, which is how the cursor
    // leaves the table (see `j_and_k_walk_out_of_the_table`).
    type_keys(&mut vim, "J", &mut b);
    assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "4", "one cell down");
    b.set_cursor(b.text().find('1').unwrap());
    type_keys(&mut vim, "2L", &mut b);
    assert_eq!(&b.text()[b.cursor()..b.cursor() + 3], "333");
}

#[test]
fn character_motions_step_from_cell_to_cell() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    // `l` never comes to rest on a cell's padding or on a pipe: one step past
    // the content lands on the next cell's first character.
    type_keys(&mut vim, "l", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "2",
        "across the boundary into the next cell"
    );
    let second_cell = b.cursor();
    type_keys(&mut vim, "l", &mut b);
    assert_eq!(
        b.cursor(),
        second_cell + 1,
        "walking the content stays inside the cell"
    );
    type_keys(&mut vim, "l", &mut b);
    assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "3");
    // At the right edge of the row there is nowhere to step, so the cursor
    // stays on the cell's last character instead of escaping into the gutter.
    type_keys(&mut vim, "lll", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 2],
        "3 ",
        "held at the cell's last content character, not the padding beyond it"
    );
    // `h` is the mirror image: it arrives at the previous cell's last
    // character, which is where a leftward motion belongs.
    b.set_cursor(b.text().find('3').unwrap());
    type_keys(&mut vim, "h", &mut b);
    assert_eq!(
        b.cursor(),
        b.text().find("22").unwrap() + 1,
        "one cell left, at its end"
    );
    // `b` arrives at the previous cell's first character.
    type_keys(&mut vim, "b", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 2],
        "22",
        "the first character of the cell to the left"
    );
    // The column edges replace the line's, and both land on content.
    b.set_cursor(b.text().find("333").unwrap());
    type_keys(&mut vim, "0", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "3",
        "0 goes to the cell's first content character"
    );
    type_keys(&mut vim, "$", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 3],
        "3 |",
        "$ stops at the cell's last character, not the line's"
    );
    type_keys(&mut vim, "^", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "3",
        "^ goes to the first content character"
    );
}

#[test]
fn shift_j_and_k_skip_the_delimiter_row() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('a').unwrap());
    type_keys(&mut vim, "J", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "1",
        "the rule line under the header is skipped"
    );
    type_keys(&mut vim, "K", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "a",
        "and on the way back up"
    );
}

/// Plain `j`/`k` are not table commands: they are the ordinary line motions,
/// which follow the display model. Live preview draws the delimiter row while
/// the cursor is on the header or on the row itself, and then they stop on it
/// like any other line, which is how the user reaches `:---:` to edit it.
#[test]
fn plain_j_and_k_stop_on_the_delimiter_row_when_it_is_drawn() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('a').unwrap());
    type_keys(&mut vim, "j", &mut b);
    assert_eq!(
        b.row_col(b.cursor()).0,
        1,
        "the rule row is a line like any other"
    );
    assert!(
        table::locate(&table::tables(b.text()), b.cursor()).is_none(),
        "and it is not a cell, so the table commands do not apply to it"
    );
    type_keys(&mut vim, "j", &mut b);
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 1],
        "1",
        "the next j moves on into the body"
    );
    type_keys(&mut vim, "k", &mut b);
    assert_eq!(b.row_col(b.cursor()).0, 1, "and k comes back onto it");
}

#[test]
fn j_and_k_walk_out_of_the_table() {
    let mut vim = Vim::new();
    let mut b = table_buf_with_context();
    b.set_cursor(b.text().find('a').unwrap());
    type_keys(&mut vim, "K", &mut b);
    assert_eq!(b.row_col(b.cursor()).0, 1, "up out of the table");
    assert!(
        table::locate(&table::tables(b.text()), b.cursor()).is_none(),
        "the cursor is no longer in a cell"
    );
    // Down through the whole table: the steps the table could not use become
    // ordinary line motions, and the rule line is skipped on the way out.
    b.set_cursor(b.text().find('a').unwrap());
    type_keys(&mut vim, "4J", &mut b);
    assert_eq!(
        b.row_col(b.cursor()).0,
        7,
        "past the last row and the blank line, never stopping on the rule"
    );
    type_keys(&mut vim, "j", &mut b);
    assert_eq!(b.row_col(b.cursor()).0, 8, "and on down into the prose");
}

#[test]
fn alt_hjkl_moves_a_row_or_column() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    type_alt_keys(&mut vim, 'j', &mut b);
    assert_eq!(
        b.text(),
        "| a | bb | ccc |\n| --- | --- | --- |\n| 4 | 55 | 666 |\n| 1 | 22 | 333 |\n",
        "alt-j moves the row down"
    );
    assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "1");
    type_alt_keys(&mut vim, 'k', &mut b);
    assert_eq!(b.text(), table_buf().text(), "and alt-k puts it back");

    b.set_cursor(b.text().find("22").unwrap());
    type_alt_keys(&mut vim, 'l', &mut b);
    assert_eq!(
        b.text(),
        "| a | ccc | bb |\n| --- | --- | --- |\n| 1 | 333 | 22 |\n| 4 | 666 | 55 |\n",
        "alt-l moves the column right"
    );
    assert_eq!(&b.text()[b.cursor()..b.cursor() + 2], "22");
    type_alt_keys(&mut vim, 'h', &mut b);
    assert_eq!(b.text(), table_buf().text(), "and alt-h puts it back");
}

#[test]
fn alt_motions_outside_a_table_do_nothing() {
    let mut vim = Vim::new();
    let mut b = buf();
    let before = b.text().to_string();
    assert_eq!(
        vim.handle_normal_key(Key::AltChar('j'), &mut b),
        Outcome::Unhandled
    );
    assert_eq!(b.text(), before);
}

#[test]
fn find_is_confined_to_the_cell() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    let start = b.text().find('1').unwrap();
    b.set_cursor(start);
    // The pipe that closes the cell belongs to it, so `f|` reaches it.
    type_keys(&mut vim, "f|", &mut b);
    assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "|");
    // A character that only exists past the cell edge counts as not found.
    b.set_cursor(start);
    type_keys(&mut vim, "f3", &mut b);
    assert_eq!(b.cursor(), start, "the find is cancelled");
}

#[test]
fn operators_stop_at_the_cell_edge() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    type_keys(&mut vim, "dw", &mut b);
    assert!(
        b.text().contains("|  | 22 | 333 |"),
        "only the cell's content went: {}",
        b.text()
    );
}

#[test]
fn dd_removes_a_row_and_selects_the_cell_that_took_its_place() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find("22").unwrap());
    type_keys(&mut vim, "dd", &mut b);
    assert_eq!(
        b.text(),
        "| a | bb | ccc |\n| --- | --- | --- |\n| 4 | 55 | 666 |\n"
    );
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 2],
        "55",
        "the row that moved up is selected"
    );
}

#[test]
fn shift_d_removes_a_column() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find("22").unwrap());
    type_keys(&mut vim, "D", &mut b);
    assert_eq!(
        b.text(),
        "| a | ccc |\n| --- | --- |\n| 1 | 333 |\n| 4 | 666 |\n"
    );
    assert_eq!(
        &b.text()[b.cursor()..b.cursor() + 3],
        "333",
        "the cell that took the column's place"
    );
}

#[test]
fn o_opens_a_table_row_below() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    assert_eq!(
        vim.handle_normal_key(Key::Char('o'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(
        b.text(),
        "| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n|  |  |  |\n| 4 | 55 | 666 |\n"
    );
    assert_eq!(
        b.cursor(),
        b.text().find("|  |  |  |").unwrap() + 1,
        "the cursor is in the new row"
    );
}

#[test]
fn capital_o_opens_a_table_row_above() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    assert_eq!(
        vim.handle_normal_key(Key::Char('O'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(
        b.text(),
        "| a | bb | ccc |\n| --- | --- | --- |\n|  |  |  |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n"
    );
}

#[test]
fn a_row_is_not_opened_between_the_header_and_its_delimiter() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find(" bb ").unwrap() + 1);
    vim.handle_normal_key(Key::Char('o'), &mut b);
    assert_eq!(
        b.text(),
        "| a | bb | ccc |\n| --- | --- | --- |\n|  |  |  |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n",
        "the table stays a table"
    );
}

#[test]
fn shift_i_clears_the_cell_and_enters_insert() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find("22").unwrap());
    assert_eq!(
        vim.handle_normal_key(Key::Char('I'), &mut b),
        Outcome::EnterInsert
    );
    assert!(
        b.text().contains("| 1 |  | 333 |"),
        "the cell is empty: {}",
        b.text()
    );
    vim.enter_insert();
    b.insert("x");
    assert!(b.text().contains("| 1 | x | 333 |"));
}

#[test]
fn capital_a_appends_at_the_end_of_the_cell() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    assert_eq!(
        vim.handle_normal_key(Key::Char('A'), &mut b),
        Outcome::EnterInsert
    );
    vim.enter_insert();
    b.insert("9");
    assert!(b.text().contains("| 19 | 22 | 333 |"));
}

#[test]
fn pipe_inserts_a_column_after_and_backslash_before() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    type_keys(&mut vim, "|", &mut b);
    assert!(b.text().contains("| 1 |  | 22 | 333 |"), "{}", b.text());
    assert!(
        b.text().contains("| --- | --- | --- | --- |"),
        "the delimiter row stays a valid delimiter"
    );
    b.set_cursor(b.text().find('4').unwrap());
    type_keys(&mut vim, "\\", &mut b);
    assert!(b.text().contains("|  | 4 |  | 55 | 666 |"), "{}", b.text());
}

#[test]
fn visual_selection_follows_cell_motions() {
    let mut vim = Vim::new();
    let mut b = table_buf();
    b.set_cursor(b.text().find('1').unwrap());
    type_keys(&mut vim, "vL", &mut b);
    let (lo, hi) = vim.selection_range(&b).unwrap();
    // Charwise visual is inclusive of the character under the cursor, which
    // landed on the first `2` of the next cell.
    assert_eq!(&b.text()[lo..hi], "1 | 2");
}

// ----- folding ------------------------------------------------------
