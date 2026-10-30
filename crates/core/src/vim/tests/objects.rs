//! The markdown text objects, driven through the engine the way the UI drives it:
//! an operator, `i` or `a`, then the scope letter.
//!
//! The ranges themselves are the scanner's business (`mdstyle::object`); what is
//! checked here is the wiring — that the scope reaches the engine, that the
//! operator lands on the right bytes, that a missing object cancels rather than
//! deleting something arbitrary, and that yank, visual and `.` all follow.

use super::*;

/// A note holding every construct an object can name, so one fixture serves the
/// whole subject.
fn md_buf() -> Buffer {
    Buffer::new(
        "# Top\nintro with [[Two]] here\n\n> [!warning] Watch out\n> first\n\n```rust\nlet x = 1;\n```\n\n| a | bb |\n| --- | --- |\n| 1 | 22 |\n"
            .to_string(),
    )
}

/// The offset of `what`, so the tests say which construct they mean instead of
/// doing arithmetic the fixture would silently break.
fn at(b: &Buffer, what: &str) -> usize {
    b.text().find(what).expect("the fixture holds it")
}

/// Two sections, so a section has an end that is not the end of the note.
fn sections_buf() -> Buffer {
    Buffer::new("# One\nintro\n\n# Two\nother\n".to_string())
}

/// The line at `ix`, for assertions about one row of the note.
fn row(b: &Buffer, ix: usize) -> String {
    b.text().lines().nth(ix).unwrap_or_default().to_string()
}

/// Types every key but the last and returns what that one decided, which is how
/// a `c`-style command is checked: the engine *reports* that the user should be
/// typing, and the window is what changes the mode.
fn type_until_last(vim: &mut Vim, keys: &str, b: &mut Buffer) -> Outcome {
    let mut chars = keys.chars();
    let last = chars.next_back().expect("at least one key");
    for c in chars {
        vim.handle_normal_key(Key::Char(c), b);
    }
    vim.handle_normal_key(Key::Char(last), b)
}

#[test]
fn a_link_object_takes_the_name_or_the_whole_link() {
    let mut v = Vim::new();

    let mut b = md_buf();
    b.set_cursor(at(&b, "[[Two]]") + 3);
    type_keys(&mut v, "diL", &mut b);
    assert_eq!(
        row(&b, 1),
        "intro with [[]] here",
        "`diL` removes the name and leaves the brackets"
    );

    let mut b = md_buf();
    b.set_cursor(at(&b, "[[Two]]"));
    type_keys(&mut v, "daL", &mut b);
    assert_eq!(row(&b, 1), "intro with  here", "`daL` removes the link");

    // The cursor on the brackets is still on the link.
    let mut b = md_buf();
    b.set_cursor(at(&b, "[[Two]]") + 1);
    type_keys(&mut v, "yaL", &mut b);
    assert_eq!(
        v.register.as_ref().unwrap().text,
        "[[Two]]",
        "`aL` is the whole construct"
    );
}

#[test]
fn a_section_object_takes_the_heading_or_its_body() {
    let mut v = Vim::new();

    // From a line inside the section, not only from its heading.
    let mut b = sections_buf();
    b.set_cursor(at(&b, "intro"));
    type_keys(&mut v, "da#", &mut b);
    assert_eq!(
        b.text(),
        "# Two\nother\n",
        "`da#` removes the heading and everything it opens, line breaks included"
    );

    let mut b = sections_buf();
    b.set_cursor(at(&b, "# One"));
    assert_eq!(
        type_until_last(&mut v, "ci#", &mut b),
        Outcome::EnterInsert,
        "and the window is told to start typing"
    );
    assert_eq!(
        b.text(),
        "# One\n# Two\nother\n",
        "`ci#` keeps the heading and types over the body"
    );
}

#[test]
fn a_callout_object_takes_the_box_or_its_body() {
    let mut v = Vim::new();

    let mut b = md_buf();
    b.set_cursor(at(&b, "> first"));
    type_keys(&mut v, "da*", &mut b);
    assert!(
        !b.text().contains("[!warning]"),
        "`da*` removes the whole box: {b}",
        b = b.text()
    );

    let mut b = md_buf();
    b.set_cursor(at(&b, "[!warning]"));
    type_keys(&mut v, "ci*", &mut b);
    assert_eq!(
        b.text(),
        "# Top\nintro with [[Two]] here\n\n> [!warning] Watch out\n\n```rust\nlet x = 1;\n```\n\n| a | bb |\n| --- | --- |\n| 1 | 22 |\n",
        "`ci*` leaves the header and the body's line, so the box is still a box to type into"
    );
}

#[test]
fn a_quoted_object_takes_the_words_or_the_marks() {
    let mut v = Vim::new();

    let mut b = Buffer::new("say \"hello there\" now\n".to_string());
    b.set_cursor(at(&b, "hello"));
    type_keys(&mut v, "diq", &mut b);
    assert_eq!(b.text(), "say \"\" now\n", "`diq` leaves the marks");

    let mut b = Buffer::new("say \"hello there\" now\n".to_string());
    b.set_cursor(at(&b, "hello"));
    type_keys(&mut v, "daq", &mut b);
    assert_eq!(
        b.text(),
        "say  now\n",
        "`daq` takes the marks with the words"
    );

    // An apostrophe belongs to a word, so there is no quotation here and the
    // operator is cancelled rather than let loose on something else.
    let mut b = Buffer::new("don't do this\n".to_string());
    b.set_cursor(at(&b, "don") + 1);
    type_keys(&mut v, "diq", &mut b);
    assert_eq!(b.text(), "don't do this\n", "nothing was deleted");

    // A quote mark inside code is a string delimiter, not a quotation.
    let mut b = Buffer::new("use `let s = \"x\";` here\n".to_string());
    b.set_cursor(at(&b, "x"));
    type_keys(&mut v, "diq", &mut b);
    assert_eq!(
        b.text(),
        "use `let s = \"x\";` here\n",
        "code keeps its quotes"
    );
}

#[test]
fn a_one_line_callout_loses_its_text_and_keeps_its_marker() {
    // The shape most callouts actually are: one line long. Its title text is its
    // body, so `ci*` retypes the sentence and `da*` takes the line — and neither
    // may eat the `[!note]` marker, which is what makes the quote a callout.
    let mut v = Vim::new();

    let mut b = Buffer::new("before\n> [!note] just this\nafter\n".to_string());
    b.set_cursor(at(&b, "[!note]") + 1);
    type_keys(&mut v, "ci*", &mut b);
    assert_eq!(
        b.text(),
        "before\n> [!note] \nafter\n",
        "`ci*` leaves the marker, with the caret after it"
    );

    let mut b = Buffer::new("before\n> [!note] just this\nafter\n".to_string());
    b.set_cursor(at(&b, "just"));
    type_keys(&mut v, "da*", &mut b);
    assert_eq!(b.text(), "before\nafter\n", "`da*` removes the line");

    // And the yank that used to land on an empty range.
    let mut b = Buffer::new("before\n> [!note] just this\nafter\n".to_string());
    b.set_cursor(at(&b, "just"));
    type_keys(&mut v, "yi*", &mut b);
    assert_eq!(
        b.text(),
        "before\n> [!note] just this\nafter\n",
        "`yi*` changes nothing"
    );
    let mut b = Buffer::new("before\n> [!note] just this\nafter\n".to_string());
    b.set_cursor(at(&b, "just"));
    type_keys(&mut v, "yi*p", &mut b);
    assert_eq!(
        row(&b, 1),
        "> [!note] jjust thisust this",
        "`p` puts the title it yanked just after the caret"
    );
}

#[test]
fn a_code_object_takes_the_block_or_the_span() {
    let mut v = Vim::new();

    let mut b = md_buf();
    b.set_cursor(at(&b, "let x"));
    type_keys(&mut v, "da`", &mut b);
    assert!(
        !b.text().contains("let x"),
        "`` da` `` removes the block: {b}",
        b = b.text()
    );

    let mut b = md_buf();
    b.set_cursor(at(&b, "let x"));
    assert_eq!(
        type_until_last(&mut v, "ci`", &mut b),
        Outcome::EnterInsert,
        "ready for the new code"
    );
    assert_eq!(
        b.text(),
        "# Top\nintro with [[Two]] here\n\n> [!warning] Watch out\n> first\n\n```rust\n```\n\n| a | bb |\n| --- | --- |\n| 1 | 22 |\n",
        "`` ci` `` leaves the fences standing"
    );
}

#[test]
fn a_cell_object_takes_the_content_or_the_cell_and_its_pipe() {
    let mut v = Vim::new();

    let mut b = table_buf();
    b.set_cursor(at(&b, "22"));
    assert_eq!(
        type_until_last(&mut v, "cit", &mut b),
        Outcome::EnterInsert,
        "and the cell is ready to type in"
    );
    assert_eq!(row(&b, 2), "| 1 |  | 333 |", "`cit` empties the cell");

    let mut b = table_buf();
    b.set_cursor(at(&b, "22"));
    type_keys(&mut v, "dat", &mut b);
    assert_eq!(
        row(&b, 2),
        "| 1 | 333 |",
        "`dat` takes the closing pipe too, so the row loses the cell rather than gaining an empty one"
    );

    // The last cell of a row ends at the row's own pipe.
    let mut b = table_buf();
    b.set_cursor(at(&b, "333"));
    type_keys(&mut v, "dat", &mut b);
    assert_eq!(row(&b, 2), "| 1 | 22 |");
}

#[test]
fn no_object_under_the_cursor_cancels_the_operator() {
    let mut v = Vim::new();

    // `da#` above the first heading: there is no section, so nothing moves.
    let mut b = md_buf();
    b.set_cursor(0);
    type_keys(&mut v, "d", &mut b);
    type_keys(&mut v, "a", &mut b);
    type_keys(&mut v, "*", &mut b);
    assert_eq!(
        b.text(),
        md_buf().text(),
        "a callout that is not there deletes nothing"
    );
    assert_eq!(
        v.mode(),
        Mode::Normal,
        "and the operator is dropped, not left pending"
    );

    // A table object outside a table, and a link object outside a link.
    let mut b = buf();
    type_keys(&mut v, "dat", &mut b);
    type_keys(&mut v, "diL", &mut b);
    assert_eq!(b.text(), buf().text(), "neither deletes anything");

    // An unknown scope is the same: the operator is gone, the text is intact.
    let mut b = buf();
    type_keys(&mut v, "dix", &mut b);
    assert_eq!(b.text(), buf().text());
}

#[test]
fn a_read_only_buffer_yanks_but_never_edits() {
    // Preview mode: the note can be read and copied, not changed. The operator
    // refuses at the range, so `d` does not fill the register with text it never
    // removed and `c` does not promise an insert mode the view will not give.
    let mut b = md_buf();
    b.set_read_only(true);
    let mut v = Vim::new();
    b.set_cursor(at(&b, "[[Two]]") + 3);
    type_keys(&mut v, "daL", &mut b);
    assert_eq!(
        b.text(),
        md_buf().text(),
        "the link survives an attempted delete"
    );
    assert!(
        v.register.is_none(),
        "and the register is untouched by an edit that did not happen"
    );
    assert_eq!(
        type_until_last(&mut v, "ciL", &mut b),
        Outcome::Stay,
        "and change does not report an insert mode"
    );

    type_keys(&mut v, "yiL", &mut b);
    assert_eq!(
        v.register.as_ref().unwrap().text,
        "Two",
        "but yank asks nothing of the buffer, so it still works"
    );
}

#[test]
fn a_visual_selection_takes_the_object() {
    let mut v = Vim::new();

    let mut b = md_buf();
    b.set_cursor(at(&b, "[[Two]]") + 3);
    type_keys(&mut v, "viL", &mut b);
    let (anchor, head) = b.selection().expect("visual mode holds a selection");
    assert_eq!(
        &b.text()[anchor..=head],
        "Two",
        "`iL` selects the name, the same inner half `diL` deletes"
    );

    // A fresh engine: `v` in visual mode ends the selection rather than starting
    // a second one, so the second command needs its own session.
    let mut v = Vim::new();
    let mut b = md_buf();
    b.set_cursor(at(&b, "[[Two]]") + 3);
    type_keys(&mut v, "vaL", &mut b);
    let (anchor, head) = b.selection().expect("visual mode holds a selection");
    assert_eq!(
        &b.text()[anchor..=head],
        "[[Two]]",
        "`aL` takes the brackets with it"
    );
}

#[test]
fn an_object_yank_lands_in_the_register_and_pastes() {
    let mut v = Vim::new();
    let mut b = md_buf();
    b.set_cursor(at(&b, "[[Two]]") + 3);
    type_keys(&mut v, "yiL", &mut b);
    assert_eq!(v.register.as_ref().unwrap().text, "Two");
    assert!(!v.register.as_ref().unwrap().linewise, "charwise");

    b.set_cursor(at(&b, "here") + 3);
    type_keys(&mut v, "p", &mut b);
    assert_eq!(
        row(&b, 1),
        "intro with [[Two]] hereTwo",
        "and `p` puts it after the cursor, as a charwise register does"
    );
}

#[test]
fn a_count_does_not_extend_a_markdown_object() {
    let mut v = Vim::new();
    let mut b = sections_buf();
    b.set_cursor(at(&b, "intro"));
    type_keys(&mut v, "d2a#", &mut b);
    // One section, not two: what `2` means across two callouts or two links is
    // not a question worth guessing at, so the count is ignored.
    assert_eq!(b.text(), "# Two\nother\n");
    assert_eq!(v.count(), None, "and the count is spent, not left pending");
}

#[test]
fn dot_replays_a_markdown_object_on_the_next_one() {
    let mut v = Vim::new();
    let mut b = Buffer::new("one [[A]] two [[B]]\n".to_string());
    b.set_cursor(at(&b, "[[A]]") + 3);
    type_keys(&mut v, "diL", &mut b);
    assert_eq!(b.text(), "one [[]] two [[B]]\n");

    b.set_cursor(at(&b, "[[B]]") + 3);
    type_keys(&mut v, ".", &mut b);
    assert_eq!(
        b.text(),
        "one [[]] two [[]]\n",
        "`.` replays the same command on the next link"
    );
}
