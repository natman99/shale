//! Motions, counts, operators, text objects and the `.` repeat, in prose.

use super::*;

#[test]
fn normal_mode_motions_and_operators() {
    let mut vim = Vim::new();
    let mut b = buf();
    assert_eq!(vim.mode(), Mode::Normal);

    vim.handle_normal_key(Key::Char('j'), &mut b);
    assert_eq!(b.cursor(), 12);
    vim.handle_normal_key(Key::Char('$'), &mut b);
    assert_eq!(b.cursor(), 22); // last char of "second line"
    vim.handle_normal_key(Key::Char('k'), &mut b);
    assert_eq!(b.cursor(), 10); // column memory lands on the 'd'

    // dd deletes the line.
    vim.handle_normal_key(Key::Char('d'), &mut b);
    vim.handle_normal_key(Key::Char('d'), &mut b);
    assert_eq!(b.text(), "second line\nthird");
}

#[test]
fn counted_motions() {
    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "3j", &mut b);
    assert_eq!(b.cursor(), 24); // clamped onto the last line

    let mut b = buf();
    type_keys(&mut vim, "2w", &mut b);
    assert_eq!(b.cursor(), 12); // start of "second"

    let mut b = buf();
    type_keys(&mut vim, "2l", &mut b);
    assert_eq!(b.cursor(), 2);

    // Multi-digit counts, and `G`/`gg` jumping to a line number.
    let mut b = buf();
    type_keys(&mut vim, "10l", &mut b);
    assert_eq!(b.cursor(), 10);
    let mut b = buf();
    type_keys(&mut vim, "3G", &mut b);
    assert_eq!(b.row_col(b.cursor()), (2, 0));
    let mut b = buf();
    type_keys(&mut vim, "2gg", &mut b);
    assert_eq!(b.row_col(b.cursor()), (1, 0));

    // A bare `0` is still the line-start motion.
    let mut b = buf();
    type_keys(&mut vim, "5l0", &mut b);
    assert_eq!(b.cursor(), 0);
}

#[test]
fn counted_operators() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("one two three four\nnext line".to_string());
    type_keys(&mut vim, "d2w", &mut b);
    assert_eq!(b.text(), "three four\nnext line");

    // A count before the operator is equivalent.
    let mut b = Buffer::new("one two three four".to_string());
    type_keys(&mut vim, "2dw", &mut b);
    assert_eq!(b.text(), "three four");

    // `2dd` deletes two lines; `dj` deletes the current and next.
    let mut b = buf();
    type_keys(&mut vim, "2dd", &mut b);
    assert_eq!(b.text(), "third");
    let mut b = buf();
    type_keys(&mut vim, "dj", &mut b);
    assert_eq!(b.text(), "third");

    // `d$` includes the last character of the line.
    let mut b = buf();
    type_keys(&mut vim, "5ld$", &mut b);
    assert_eq!(b.text(), "hello\nsecond line\nthird");

    // `2x` and `2o`.
    let mut b = buf();
    type_keys(&mut vim, "2x", &mut b);
    assert_eq!(b.text(), "llo world\nsecond line\nthird");
    let mut b = Buffer::new("a\nb".to_string());
    vim.handle_normal_key(Key::Char('2'), &mut b);
    assert_eq!(
        vim.handle_normal_key(Key::Char('o'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(b.text(), "a\n\n\nb");
}

#[test]
fn change_operator_enters_insert() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("one two three".to_string());
    assert_eq!(vim.handle_normal_key(Key::Char('c'), &mut b), Outcome::Stay);
    assert_eq!(
        vim.handle_normal_key(Key::Char('w'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(b.text(), "two three");
    vim.enter_insert();
    b.insert("X");
    vim.handle_insert_key(Key::Escape);
    assert_eq!(b.text(), "Xtwo three");
}

#[test]
fn yank_and_paste() {
    let mut vim = Vim::new();
    // Charwise yank spans through the whitespace, like vim's `w`.
    let mut b = Buffer::new("one two three".to_string());
    type_keys(&mut vim, "yw", &mut b);
    assert_eq!(vim.register.as_ref().unwrap().text, "one ");
    type_keys(&mut vim, "$p", &mut b);
    assert_eq!(b.text(), "one two threeone ");

    // Linewise yank and paste above the next line.
    let mut vim = Vim::new();
    let mut b = buf();
    type_keys(&mut vim, "yy", &mut b);
    assert_eq!(vim.register.as_ref().unwrap().text, "hello world\n");
    type_keys(&mut vim, "jP", &mut b);
    assert_eq!(b.text(), "hello world\nhello world\nsecond line\nthird");
}

/// With soft wrap, `$`, `0` and `^` are screen-line motions (vim's `g$`/
/// `g0`/`g^` are the line-wise forms), while `A` and `I` stay line-wise.
#[test]
fn dollar_and_zero_follow_the_display_line() {
    let mut b = Buffer::new("aaaa   bbbb cccc".to_string());
    b.set_display_segments(vec![vec![0, 11]]);
    let mut v = Vim::new();

    type_keys(&mut v, "$", &mut b);
    assert_eq!(b.cursor(), 10, "$ stops at the display line end");
    type_keys(&mut v, "0", &mut b);
    assert_eq!(b.cursor(), 0, "0 goes to the display line start");

    b.set_cursor(12);
    assert_eq!(
        v.handle_normal_key(Key::Char('I'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(b.cursor(), 0, "I is line-wise: the row's first non-blank");

    b.set_cursor(0);
    assert_eq!(
        v.handle_normal_key(Key::Char('A'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(b.cursor(), 16, "A is line-wise: the end of the whole row");

    b.set_cursor(12);
    type_keys(&mut v, "d$", &mut b);
    assert_eq!(b.text(), "aaaa   bbbb ", "d$ stops at the display line end");
}

#[test]
fn text_objects() {
    let mut v = Vim::new();

    let mut b = buf(); // "hello world\nsecond line\nthird"
    type_keys(&mut v, "diw", &mut b);
    assert_eq!(b.text(), " world\nsecond line\nthird", "diw");

    let mut b = buf();
    type_keys(&mut v, "daw", &mut b);
    assert_eq!(b.text(), "world\nsecond line\nthird", "daw");

    let mut b = buf();
    b.set_cursor(6);
    type_keys(&mut v, "daw", &mut b);
    assert_eq!(
        b.text(),
        "hello\nsecond line\nthird",
        "daw falls back to the leading space at a line end"
    );

    let mut b = buf();
    b.set_cursor(12);
    type_keys(&mut v, "ci", &mut b);
    assert_eq!(
        v.handle_normal_key(Key::Char('w'), &mut b),
        Outcome::EnterInsert,
        "ciw leaves the cursor ready for typing"
    );
    assert_eq!(b.text(), "hello world\n line\nthird");

    let mut b = buf();
    type_keys(&mut v, "yiw", &mut b);
    assert_eq!(
        v.register.as_ref().unwrap().text,
        "hello",
        "a charwise yank of a word"
    );
    assert!(!v.register.as_ref().unwrap().linewise, "and not linewise");

    // A count before the object extends it over the next one.
    let mut b = buf();
    type_keys(&mut v, "d2aw", &mut b);
    assert_eq!(b.text(), "\nsecond line\nthird", "d2aw takes both words");

    let mut b = Buffer::new("one\ntwo\n\nthree\n\n\nfour".to_string());
    type_keys(&mut v, "cip", &mut b);
    assert_eq!(b.text(), "\n\nthree\n\n\nfour", "cip keeps the break");

    let mut b = Buffer::new("one\ntwo\n\nthree\n\n\nfour".to_string());
    type_keys(&mut v, "cap", &mut b);
    assert_eq!(b.text(), "three\n\n\nfour", "cap swallows the blanks");

    let mut b = Buffer::new("one\ntwo\n\nthree\n\n\nfour".to_string());
    b.set_cursor(9);
    type_keys(&mut v, "yip", &mut b);
    assert_eq!(v.register.as_ref().unwrap().text, "three");

    let mut b = buf();
    type_keys(&mut v, "dij", &mut b);
    assert_eq!(
        b.text(),
        "hello world\nsecond line\nthird",
        "an unknown scope cancels the operator"
    );
}

#[test]
fn find_motions_and_operators() {
    let mut v = Vim::new();

    let mut b = buf(); // "hello world\nsecond line\nthird"
    type_keys(&mut v, "fo", &mut b);
    assert_eq!(b.cursor(), 4, "f lands on the character");
    type_keys(&mut v, "fo", &mut b);
    assert_eq!(b.cursor(), 7, "and the next one on the second press");
    type_keys(&mut v, "2fo", &mut b);
    assert_eq!(b.cursor(), 7, "no third o on the row, so nothing moves");

    let mut b = buf();
    type_keys(&mut v, "dfo", &mut b);
    assert_eq!(
        b.text(),
        " world\nsecond line\nthird",
        "dfo includes the found character"
    );

    let mut b = buf();
    type_keys(&mut v, "dto", &mut b);
    assert_eq!(
        b.text(),
        "lo world\nsecond line\nthird",
        "dto stops before the found character"
    );

    let mut b = buf();
    b.set_cursor(10);
    type_keys(&mut v, "dFw", &mut b);
    assert_eq!(
        b.text(),
        "hello d\nsecond line\nthird",
        "dFw deletes back to the match"
    );

    let mut b = buf();
    type_keys(&mut v, "dzq", &mut b);
    assert_eq!(
        b.text(),
        "hello world\nsecond line\nthird",
        "a find that fails cancels the operator"
    );

    // A bound key can still be the find argument: `fv` must find a literal
    // `v`, not enter visual mode.
    let mut b = Buffer::new("abc vivid".to_string());
    type_keys(&mut v, "0fv", &mut b);
    assert_eq!(v.mode(), Mode::Normal, "`fv` is not visual mode");
    assert_eq!(b.cursor(), 4, "and should find the v");

    // The same in visual mode, where more keys are intercepted.
    let mut b = Buffer::new("abc. def".to_string());
    type_keys(&mut v, "vf.", &mut b);
    assert_eq!(v.mode(), Mode::Visual, "`f.` is not repeat-last");
    assert_eq!(v.selection_range(&b), Some((0, 4)));
}

#[test]
fn insert_transitions() {
    let mut vim = Vim::new();
    let mut b = buf();
    assert_eq!(
        vim.handle_normal_key(Key::Char('o'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(b.text(), "hello world\n\nsecond line\nthird");
    vim.enter_insert();
    assert_eq!(vim.mode(), Mode::Insert);

    b.insert("typed");
    assert!(vim.handle_insert_key(Key::Escape));
    assert_eq!(vim.mode(), Mode::Normal);
    assert_eq!(b.text(), "hello world\ntyped\nsecond line\nthird");
}

#[test]
fn gg_and_pending_reset() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(20);
    vim.handle_normal_key(Key::Char('g'), &mut b);
    vim.handle_normal_key(Key::Char('g'), &mut b);
    assert_eq!(b.cursor(), 0);

    // Unbound key after `d` cancels the pending operator.
    vim.handle_normal_key(Key::Char('d'), &mut b);
    assert_eq!(
        vim.handle_normal_key(Key::Char('q'), &mut b),
        Outcome::Unhandled
    );
    vim.handle_normal_key(Key::Char('d'), &mut b);
    vim.handle_normal_key(Key::Char('d'), &mut b);
    assert_eq!(b.line_count(), 2);
}

#[test]
fn a_and_uppercase_a_position_cursor() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(0);
    vim.handle_normal_key(Key::Char('a'), &mut b);
    assert_eq!(b.cursor(), 1);
    vim.enter_insert();
    vim.handle_insert_key(Key::Escape);
    vim.handle_normal_key(Key::Char('A'), &mut b);
    assert_eq!(b.cursor(), 11);
}

/// `A` on an empty row appends *there*. Such a row has no last character to
/// step off, so a step that may cross the newline lands on the row below.
#[test]
fn a_on_an_empty_row_stays_on_that_row() {
    let mut v = Vim::new();
    let mut b = Buffer::new("one\n\nthree\n".to_string());
    type_keys(&mut v, "j", &mut b);
    assert_eq!(
        b.row_col(b.cursor()).0,
        1,
        "the cursor starts on the empty row"
    );
    assert_eq!(
        v.handle_normal_key(Key::Char('A'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(b.cursor(), 4, "the append belongs to the empty row");
    assert_eq!(b.row_col(b.cursor()).0, 1);
}

/// The same trap for visual `A` when the selection's far row is empty.
#[test]
fn visual_a_on_a_selection_ending_on_an_empty_row() {
    let mut v = Vim::new();
    let mut b = Buffer::new("one\n\n".to_string());
    type_keys(&mut v, "Vj", &mut b);
    assert_eq!(
        v.handle_normal_key(Key::Char('A'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(
        b.row_col(b.cursor()).0,
        1,
        "the append belongs to the empty row, not the one after it"
    );
}

#[test]
fn undo_and_count_reset() {
    let mut vim = Vim::new();
    let mut b = buf();
    b.set_cursor(0);
    b.insert("X");
    assert_eq!(b.text(), "Xhello world\nsecond line\nthird");
    type_keys(&mut vim, "u", &mut b);
    assert_eq!(b.text(), "hello world\nsecond line\nthird");

    // An unhandled key must not leave a stale count behind.
    type_keys(&mut vim, "3q", &mut b);
    assert_eq!(vim.count(), None);
}
