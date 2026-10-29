use super::*;

fn buf() -> Buffer {
    Buffer::new("hello world\nsecond line\nthird".to_string())
}

#[test]
fn line_bounds() {
    let b = buf();
    assert_eq!(b.line_count(), 3);
    assert_eq!(b.line_start(1), 12);
    assert_eq!(b.line_end(1), 23);
    assert_eq!(b.line_end(2), 29);
}

#[test]
fn row_col_roundtrip() {
    let b = buf();
    assert_eq!(b.row_col(12), (1, 0));
    assert_eq!(b.offset_of(1, 6), 18);
    // Column beyond line end clamps to line end.
    assert_eq!(b.offset_of(0, 100), 11);
}

#[test]
fn word_motions() {
    let mut b = Buffer::new("foo bar  baz\nnext".to_string());
    b.word_forward(); // at 'f' -> start of "bar"
    assert_eq!(b.cursor(), 4);
    b.word_forward(); // -> start of "baz"
    assert_eq!(b.cursor(), 9);
    b.word_forward(); // -> start of "next"
    assert_eq!(b.cursor(), 13);
    b.word_backward(); // -> start of "baz"
    assert_eq!(b.cursor(), 9);

    let mut b2 = Buffer::new("hello world".to_string());
    b2.word_end();
    assert_eq!(b2.cursor(), 4); // 'o' of hello
    b2.word_end();
    assert_eq!(b2.cursor(), 10); // 'd' of world
}

/// `w`/`b`/`e` must treat punctuation runs as words instead of stalling.
/// Like vim, a run of non-blank non-keyword characters counts as one word,
/// so `).` is a single word.
#[test]
fn word_motions_punctuation() {
    let mut b = Buffer::new("a, b). c".to_string());
    b.word_forward(); // 'a' -> ','
    assert_eq!(b.cursor(), 1);
    b.word_forward(); // ',' -> 'b'
    assert_eq!(b.cursor(), 3);
    b.word_forward(); // 'b' -> ')'
    assert_eq!(b.cursor(), 4);
    b.word_forward(); // '). ' -> 'c'
    assert_eq!(b.cursor(), 7);
    b.word_forward(); // at end: stays
    assert_eq!(b.cursor(), 8);

    b.word_backward(); // end -> 'c'
    assert_eq!(b.cursor(), 7);
    b.word_backward(); // 'c' -> start of the `).` run
    assert_eq!(b.cursor(), 4);
    b.word_backward(); // '). ' -> 'b'
    assert_eq!(b.cursor(), 3);
    b.word_backward(); // 'b' -> ','
    assert_eq!(b.cursor(), 1);

    // `e` walks run ends, including punctuation runs.
    let mut b = Buffer::new("foo,bar".to_string());
    b.word_end(); // end of "foo"
    assert_eq!(b.cursor(), 2);
    b.word_end(); // end of ","
    assert_eq!(b.cursor(), 3);
    b.word_end(); // end of "bar"
    assert_eq!(b.cursor(), 6);
}

/// `w` crosses a line end to the first word of the next line.
#[test]
fn word_motion_crosses_lines() {
    let mut b = Buffer::new("end\n   next".to_string());
    b.set_cursor(0);
    b.word_forward(); // 'end' -> 'next'
    assert_eq!(b.cursor(), 7);
    b.word_backward(); // 'next' -> 'end'
    assert_eq!(b.cursor(), 0);
}

#[test]
fn word_text_objects() {
    let b = buf(); // "hello world\nsecond line\nthird"
    assert_eq!(
        b.word_object(0, true),
        (0, 5),
        "iw is the run under the cursor"
    );
    assert_eq!(
        b.word_object(3, true),
        (0, 5),
        "from anywhere inside the word"
    );
    assert_eq!(
        b.word_object(0, false),
        (0, 6),
        "aw takes the trailing space"
    );
    assert_eq!(
        b.word_object(10, false),
        (5, 11),
        "and the leading one at the row end"
    );
    assert_eq!(b.word_object(5, true), (5, 6), "the space is its own run");
    assert_eq!(
        b.word_object(5, false),
        (5, 11),
        "aw on whitespace takes the word after"
    );

    let b = Buffer::new("a.b".to_string());
    assert_eq!(
        b.word_object(1, true),
        (1, 2),
        "punctuation is its own word"
    );
    assert_eq!(
        b.word_object(0, false),
        (0, 1),
        "aw does not span a run that is not whitespace"
    );
}

#[test]
fn paragraph_text_objects() {
    let b = Buffer::new("one\ntwo\n\nthree\n\n\nfour".to_string());
    // rows: one(0) two(4) blank(8) three(9) blank(15) blank(16) four(17)
    assert_eq!(
        b.paragraph_object(0, true),
        (0, 7),
        "a run of non-blank lines"
    );
    assert_eq!(b.paragraph_object(5, true), (0, 7), "from any line in it");
    assert_eq!(
        b.paragraph_object(9, true),
        (9, 14),
        "a lone line is a paragraph"
    );
    assert_eq!(
        b.paragraph_object(8, true),
        (8, 9),
        "a blank line is its own object"
    );
    assert_eq!(
        b.paragraph_object(0, false),
        (0, 9),
        "ap swallows the blank line after"
    );
    assert_eq!(
        b.paragraph_object(9, false),
        (9, 17),
        "ap swallows every blank line after"
    );
    assert_eq!(
        b.paragraph_object(17, false),
        (15, 21),
        "ap takes the blank lines before when nothing follows"
    );
}

/// `f`/`t`/`F`/`T` search within the row only, and leave the cursor alone
/// when the character is not there.
#[test]
fn find_char_motions() {
    let mut b = buf(); // "hello world\nsecond line\nthird"
    b.set_cursor(0);
    b.find_char('o', true, true, 1);
    assert_eq!(b.cursor(), 4, "f lands on the character");
    b.find_char('o', true, true, 1);
    assert_eq!(b.cursor(), 7, "a second f finds the next one");
    b.find_char('z', true, true, 1);
    assert_eq!(b.cursor(), 7, "a missing character leaves the cursor alone");

    b.set_cursor(0);
    b.find_char('o', true, false, 1);
    assert_eq!(b.cursor(), 3, "t stops before the character");

    b.set_cursor(7);
    b.find_char('w', false, true, 1);
    assert_eq!(b.cursor(), 6, "F searches backwards");
    b.set_cursor(7);
    b.find_char('w', false, false, 1);
    assert_eq!(
        b.cursor(),
        7,
        "T stops just after it, which is where we were"
    );
    b.set_cursor(10);
    b.find_char('e', false, false, 1);
    assert_eq!(b.cursor(), 2, "T lands after the match");

    b.set_cursor(0);
    b.find_char('o', true, true, 2);
    assert_eq!(b.cursor(), 7, "2f finds the second occurrence");

    b.set_cursor(8);
    b.find_char('s', true, true, 1);
    assert_eq!(b.cursor(), 8, "the search stops at the end of the row");
}

/// `$` is the last character of the line, not past it.
#[test]
fn dollar_is_last_char() {
    let mut b = Buffer::new("abcd\nxy".to_string());
    b.set_cursor(0);
    b.line_last_char_motion();
    assert_eq!(b.cursor(), 3);
    b.move_down();
    b.line_last_char_motion();
    assert_eq!(b.cursor(), 6);
    // An empty line stays at the line start.
    let mut b = Buffer::new("a\n\nb".to_string());
    b.set_cursor(2);
    b.line_last_char_motion();
    assert_eq!(b.cursor(), 2);
}

/// Vertical motions remember the column they were aiming for.
#[test]
fn vertical_motions_keep_desired_column() {
    let mut b = Buffer::new("aaaaaaaa\nb\ncccccccc".to_string());
    b.set_cursor(7); // col 7 of the first line
    b.move_down(); // clamped onto the short line
    assert_eq!(b.row_col(b.cursor()), (1, 1));
    b.move_down(); // restores col 7 on the third line
    assert_eq!(b.row_col(b.cursor()), (2, 7));
    // A horizontal motion resets the target column.
    b.line_start_motion();
    b.move_up();
    assert_eq!(b.row_col(b.cursor()), (1, 0));
}

#[test]
fn vertical_motions_cross_wrapped_display_lines() {
    // One buffer row wrapped into three display lines, then a normal row.
    let mut b = Buffer::new("aaa bbb ccc\nd".to_string());
    b.set_display_segments(vec![vec![0, 4, 8], vec![12]]);
    assert_eq!(b.display_line_count(), 4);
    assert_eq!(b.display_range(1), Some((4, 8)));
    assert_eq!(b.display_index_for(9), 2);

    b.set_cursor(0);
    b.move_down();
    assert_eq!(b.cursor(), 4, "j steps onto the wrapped segment");
    b.move_down();
    assert_eq!(b.cursor(), 8);
    b.move_down();
    assert_eq!(b.cursor(), 12, "and then onto the next buffer row");
    b.move_up();
    assert_eq!(b.cursor(), 8);

    // The column being aimed for is relative to the display line.
    b.set_cursor(9); // one byte into the "ccc" segment
    assert_eq!(b.desired_col, Some(1));
    b.move_down();
    assert_eq!(b.cursor(), 13, "clamped to the short row");
    b.move_up();
    assert_eq!(b.cursor(), 9, "and restored when coming back");
}

#[test]
fn a_collapsed_row_takes_no_display_line() {
    // A fenced block whose ``` lines live preview does not draw at all: five
    // buffer rows, three display lines.
    let mut b = Buffer::new("one\n```\ncode\n```\ntwo".to_string());
    b.set_display_segments(vec![vec![0], vec![], vec![8], vec![], vec![17]]);
    assert_eq!(b.display_line_count(), 3);
    assert_eq!(
        b.display_range(1),
        Some((8, 12)),
        "the code line follows `one`"
    );
    assert_eq!(b.display_index_for(17), 2);
    assert_eq!(
        b.display_index_for(4),
        1,
        "an offset inside a collapsed row resolves to the line after it"
    );

    b.set_cursor(0);
    b.move_down();
    assert_eq!(b.cursor(), 8, "j steps over the collapsed fence");
    b.move_down();
    assert_eq!(b.cursor(), 17, "and over the closing one");
    b.move_up();
    assert_eq!(b.cursor(), 8);
}

#[test]
fn desired_column_counts_characters_not_bytes() {
    // 'é' is two bytes and one cell, so a byte column and a character column
    // disagree from the second character onwards.
    let mut b = Buffer::new("a\u{e9}\u{e9}\nxy".to_string());
    // row 0: a(0) \u{e9}(1..3) \u{e9}(3..5), newline at 5; row 1 starts at 6.
    b.set_cursor(3);
    assert_eq!(b.display_col(3), 2, "the second \u{e9} is the third cell");
    assert_eq!(b.desired_col, Some(2));

    b.move_down();
    assert_eq!(b.cursor(), 8, "two cells into the short row");
    b.move_up();
    assert_eq!(
        b.cursor(),
        3,
        "the cell column is restored, not a byte offset"
    );
}

#[test]
fn screen_line_motions_respect_soft_wrap() {
    // "aaaa   bbbb" wrapped after the first space run, so the second display
    // line starts with blanks, plus a tab-indented second row.
    let mut b = Buffer::new("aaaa   bbbb\n\tzz".to_string());
    b.set_display_segments(vec![vec![0, 5], vec![12]]);
    assert_eq!(b.display_range(1), Some((5, 11)));

    b.set_cursor(9);
    b.line_start_motion();
    assert_eq!(b.cursor(), 5, "0 goes to the display line start");

    b.set_cursor(9);
    b.line_last_char_motion();
    assert_eq!(b.cursor(), 10, "$ goes to the display line's last char");

    b.set_cursor(9);
    b.first_non_blank();
    assert_eq!(b.cursor(), 7, "^ skips blanks inside the display line");

    b.set_cursor(9);
    b.row_first_non_blank();
    assert_eq!(b.cursor(), 0, "the line-wise `I` target ignores the wrap");

    b.set_cursor(9);
    b.row_last_char_motion();
    assert_eq!(b.cursor(), 10, "the line-wise `A` target is the row end");

    // On a row's last display line the screen motions are the row bounds.
    b.set_cursor(13);
    b.line_last_char_motion();
    assert_eq!(b.cursor(), 14);
    b.line_start_motion();
    assert_eq!(b.cursor(), 12);
    b.first_non_blank();
    assert_eq!(b.cursor(), 13, "^ skips the leading tab");
}

#[test]
fn line_motions() {
    let mut b = buf();
    b.set_cursor(15);
    b.line_start_motion();
    assert_eq!(b.cursor(), 12);
    b.line_end_motion();
    assert_eq!(b.cursor(), 23);
    b.move_up();
    assert_eq!(b.row_col(b.cursor()), (0, 11));
    b.first_non_blank();
    assert_eq!(b.cursor(), 0);
}

#[test]
fn enter_motion() {
    let mut b = Buffer::new("aaa\n  indented\nzzz".to_string());
    b.set_cursor(0);
    b.next_line_first_non_blank();
    assert_eq!(b.cursor(), 6); // 'i' of "indented"
    b.next_line_first_non_blank();
    assert_eq!(b.cursor(), 15); // 'z' of "zzz"
    b.next_line_first_non_blank(); // stays at last line
    assert_eq!(b.cursor(), 15);
}

#[test]
fn insert_and_undo_redo() {
    let mut b = Buffer::new("ab".to_string());
    b.set_cursor(1);
    b.insert("X");
    assert_eq!(b.text(), "aXb");
    assert_eq!(b.cursor(), 2);
    b.undo();
    assert_eq!(b.text(), "ab");
    assert_eq!(b.cursor(), 1);
    b.redo();
    assert_eq!(b.text(), "aXb");
}

/// A run of insert-mode edits is one undo step, like vim's insert session.
#[test]
fn insert_group_is_one_undo_step() {
    let mut b = Buffer::new("x".to_string());
    b.begin_undo_group();
    b.insert("a");
    b.insert("b");
    b.backspace();
    b.insert("c");
    b.end_undo_group();
    assert_eq!(b.text(), "acx");
    assert_eq!(b.undo_depth(), 1, "the whole session is one step");
    b.undo();
    assert_eq!(b.text(), "x");
    b.redo();
    assert_eq!(b.text(), "acx");
}

/// Entering insert mode without editing must not leave a dead undo step.
#[test]
fn empty_insert_group_is_dropped() {
    let mut b = Buffer::new("x".to_string());
    b.begin_undo_group();
    b.end_undo_group();
    assert_eq!(b.undo_depth(), 0);
}

/// Edits outside a group (normal-mode operators) still snapshot per edit.
#[test]
fn ungrouped_edits_snapshot_each() {
    let mut b = Buffer::new("x".to_string());
    b.insert("a");
    b.insert("b");
    assert_eq!(b.undo_depth(), 2);
    b.undo();
    assert_eq!(b.text(), "ax");
}

#[test]
fn delete_line_middle_and_last() {
    let mut b = buf();
    b.set_cursor(13);
    b.delete_line(1);
    assert_eq!(b.text(), "hello world\nthird");

    let mut b2 = buf();
    b2.set_cursor(25);
    b2.delete_line(1);
    assert_eq!(b2.text(), "hello world\nsecond line");
}

#[test]
fn open_lines() {
    let mut b = buf();
    b.set_cursor(5);
    b.open_line_below(1);
    assert_eq!(b.text(), "hello world\n\nsecond line\nthird");
    assert_eq!(b.cursor(), 12);

    let mut b2 = buf();
    b2.set_cursor(13);
    b2.open_line_above(1);
    assert_eq!(&b2.text()[11..13], "\n\n");
    assert_eq!(b2.cursor(), 12);
}

#[test]
fn delete_char_and_selection() {
    let mut b = Buffer::new("héllo".to_string());
    b.set_cursor(1); // on the é (2 bytes)
    b.delete_char(1);
    assert_eq!(b.text(), "hllo");

    let mut b2 = Buffer::new("hello".to_string());
    b2.set_selection(0, 3);
    b2.delete_selected();
    assert_eq!(b2.text(), "lo");
    assert_eq!(b2.cursor(), 0);
}

#[test]
fn cursor_clamps_to_char_boundaries() {
    let mut b = Buffer::new("héllo".to_string());
    b.set_cursor(2); // mid-char: é starts at 1, ends at 3
    assert_eq!(b.cursor(), 1);
    b.set_cursor(999);
    assert_eq!(b.cursor(), 6);
}

#[test]
fn counted_deletes() {
    let mut b = Buffer::new("abcdef\nsecond\nthird".to_string());
    b.set_cursor(0);
    b.delete_char(3);
    assert_eq!(b.text(), "def\nsecond\nthird");

    b.set_cursor(4); // on 'e' of "second"
    b.delete_line(2);
    assert_eq!(b.text(), "def");

    let mut b2 = Buffer::new("one two three four".to_string());
    b2.set_cursor(3);
    b2.delete_range(0, 7);
    assert_eq!(b2.text(), " three four");
    assert_eq!(b2.cursor(), 0);
}

/// `ctrl-backspace` in insert mode: the word before the cursor, and the blanks
/// between the cursor and that word go with it — the same boundary `b` moves to,
/// which is vim's `i_CTRL-W`.
#[test]
fn delete_word_back_takes_the_word_and_the_blanks() {
    let mut b = Buffer::new("one two three".to_string());
    b.set_cursor(13);
    b.delete_word_back();
    assert_eq!(b.text(), "one two ");
    assert_eq!(b.cursor(), 8, "the cursor lands where the deletion started");

    b.delete_word_back();
    assert_eq!(
        b.text(),
        "one ",
        "the space that was before the cursor went too"
    );

    // Mid-word: only the part before the cursor goes.
    let mut b = Buffer::new("hello world".to_string());
    b.set_cursor(8); // inside "world"
    b.delete_word_back();
    assert_eq!(b.text(), "hello rld");
    assert_eq!(b.cursor(), 6);

    // A punctuation run is a word (vim's word classes), so `db` on `a, b). c`
    // takes the `c` and the space in front of it.
    let mut b = Buffer::new("a, b). c".to_string());
    b.set_cursor(8);
    b.delete_word_back();
    assert_eq!(b.text(), "a, b). ");

    // At the start of the text there is nothing to delete, and nothing moves.
    let mut b = Buffer::new("x".to_string());
    b.delete_word_back();
    assert_eq!(b.text(), "x");
    assert_eq!(b.cursor(), 0);

    // Multi-byte text: the boundary is a character boundary, never a byte step
    // into the middle of a character.
    let mut b = Buffer::new("naïve café".to_string());
    let end = b.text().len();
    b.set_cursor(end);
    b.delete_word_back();
    assert_eq!(b.text(), "naïve ");

    // A delete is an edit, so it is undoable and lands in the insert session.
    b.undo();
    assert_eq!(b.text(), "naïve café");
}

#[test]
fn linewise_helpers() {
    let b = Buffer::new("aa\nb\nccc".to_string());
    assert_eq!(b.yank_lines(0, 2), "aa\nb\n");
    assert_eq!(b.yank_lines(2, 5), "ccc");
    // Deleting the final line takes the preceding newline instead.
    assert_eq!(b.line_range(1, 1), (3, 5));
    assert_eq!(b.line_range(2, 1), (4, 8));

    let mut b = Buffer::new("aa\nb\nccc".to_string());
    b.goto_line(1);
    assert_eq!(b.cursor(), 3);
    b.goto_line(99);
    assert_eq!(b.cursor(), 5);
    b.insert_at(3, "d\n");
    assert_eq!(b.text(), "aa\nd\nb\nccc");
    assert_eq!(b.cursor(), 5);
}

#[test]
fn counted_open_lines() {
    let mut b = Buffer::new("a\nb".to_string());
    b.set_cursor(0);
    b.open_line_below(2);
    assert_eq!(b.text(), "a\n\n\nb");
    assert_eq!(b.cursor(), 3);

    let mut b2 = Buffer::new("a\nb".to_string());
    b2.set_cursor(2);
    b2.open_line_above(2);
    assert_eq!(b2.text(), "a\n\n\nb");
    assert_eq!(b2.cursor(), 2);
}

// ----- folding ------------------------------------------------------

#[test]
fn folding_a_section_hides_its_rows_from_the_display_model() {
    let mut b = Buffer::new("# Top\nintro\n## Sub\nbody\ndeep\n".to_string());
    b.goto_line(3); // `body`, inside `## Sub`
    assert!(!b.has_closed_folds());
    b.fold_toggle();
    assert!(b.has_closed_folds());
    let view = b.fold_view();
    assert_eq!(
        view.collapsed,
        vec![false, false, false, true, true, true],
        "the rows under `## Sub` go, and the heading row stays"
    );
    assert_eq!(view.marker[2], Some(3));
    assert_eq!(
        b.row_col(b.cursor()).0,
        2,
        "the cursor rides up to the heading that opens the fold"
    );
    b.fold_toggle();
    assert!(!b.has_closed_folds());
    assert!(b.fold_view().collapsed.iter().all(|c| !c));
}

#[test]
fn a_fold_follows_its_heading_through_an_edit() {
    let mut b = Buffer::new("# Top\nintro\n## Sub\nbody\n".to_string());
    b.goto_line(2);
    b.fold_close();
    // A fold is keyed on its heading's text, so an edit anywhere else (here,
    // one that pushes every row down) leaves it closed on the same heading.
    b.set_cursor(0);
    b.insert("More title.\n");
    let view = b.fold_view();
    assert_eq!(view.marker[3], Some(2), "`## Sub` moved down a row");
    assert!(view.collapsed[4], "the body row is still hidden");
}

#[test]
fn closing_every_fold_lands_on_a_visible_heading() {
    let mut b = Buffer::new("# Top\nintro\n## Sub\nbody\n".to_string());
    b.goto_line(4); // `body`, inside both sections
    b.fold_close_all();
    assert_eq!(
        b.row_col(b.cursor()).0,
        0,
        "the cursor walks out to the outer heading"
    );
    let view = b.fold_view();
    assert_eq!(view.marker[0], Some(4));
    assert_eq!(
        view.marker[2], None,
        "the nested fold has no visible row to carry a marker"
    );
    b.fold_open_all();
    assert!(!b.has_closed_folds());
}

#[test]
fn loading_another_note_forgets_folds() {
    let mut b = Buffer::new("# Top\nintro\n".to_string());
    b.goto_line(0);
    b.fold_close();
    assert!(b.has_closed_folds());
    b.set_text("# Top\nintro\n".to_string());
    assert!(!b.has_closed_folds());
}
