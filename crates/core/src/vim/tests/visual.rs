//! Visual mode: `v`, `V`, and what an operator does to a selection.

use super::*;

/// `f`/`t`/`F`/`T` as motions and as operator arguments.
/// `iw`/`aw` and `ip`/`ap`: the operator waits for the object's scope, then
/// acts on the range under the cursor.
/// Visual mode: motions extend the selection, operators act on it, and a
/// charwise selection is inclusive of the character under the cursor.
#[test]
fn visual_mode() {
    let mut v = Vim::new();
    let mut b = buf(); // "hello world\nsecond line\nthird"
    type_keys(&mut v, "vll", &mut b);
    assert_eq!(v.mode(), Mode::Visual);
    assert_eq!(b.selection(), Some((0, 2)));
    assert_eq!(v.selection_range(&b), Some((0, 3)), "inclusive of the head");
    type_keys(&mut v, "d", &mut b);
    assert_eq!(b.text(), "lo world\nsecond line\nthird");
    assert_eq!(v.mode(), Mode::Normal, "the operator ends the selection");
    assert_eq!(b.selection(), None);

    // A word motion is exclusive, so the selection reaches into the next
    // word, exactly as vim's `vw` does.
    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "vw", &mut b);
    assert_eq!(v.selection_range(&b), Some((0, 7)));
    type_keys(&mut v, "y", &mut b);
    assert_eq!(v.register.as_ref().unwrap().text, "hello w");

    // `o` jumps to the other end of the selection.
    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "vll", &mut b);
    assert_eq!(b.cursor(), 2);
    type_keys(&mut v, "o", &mut b);
    assert_eq!(b.cursor(), 0);
    assert_eq!(b.selection(), Some((2, 0)));

    // `h` past the anchor flips the selection to the other side, as vim's
    // does.
    v = Vim::new();
    let mut b = buf();
    b.set_cursor(6);
    type_keys(&mut v, "vh", &mut b);
    assert_eq!(v.selection_range(&b), Some((5, 7)), "the space and the w");

    // Counts extend visual motions.
    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "v2l", &mut b);
    assert_eq!(b.selection(), Some((0, 2)));

    // A text object selects the object itself.
    v = Vim::new();
    let mut b = buf();
    b.set_cursor(7);
    type_keys(&mut v, "viw", &mut b);
    assert_eq!(
        v.selection_range(&b),
        Some((6, 11)),
        "iw selects the word under the cursor"
    );

    // `I` and `A` insert at the near and far ends of the selection's lines.
    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "vl", &mut b);
    assert_eq!(
        v.handle_normal_key(Key::Char('I'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(b.cursor(), 0);
    assert_eq!(b.selection(), None);

    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "vl", &mut b);
    assert_eq!(
        v.handle_normal_key(Key::Char('A'), &mut b),
        Outcome::EnterInsert
    );
    assert_eq!(b.cursor(), 11, "`A` goes to the end of the line");

    // `v` again exits, and `V` switches the selection to linewise.
    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "vlv", &mut b);
    assert_eq!(v.mode(), Mode::Normal);
    assert_eq!(b.selection(), None);

    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "vV", &mut b);
    assert_eq!(v.mode(), Mode::VisualLine);

    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "vl", &mut b);
    v.handle_normal_key(Key::Escape, &mut b);
    assert_eq!(v.mode(), Mode::Normal);
    assert_eq!(b.selection(), None);
}

/// Linewise visual (`V`): whole lines, line breaks included.
#[test]
fn visual_line_mode() {
    let mut v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "V", &mut b);
    assert_eq!(v.mode(), Mode::VisualLine);
    assert_eq!(v.selection_range(&b), Some((0, 12)), "one whole line");

    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "Vj", &mut b);
    assert_eq!(v.selection_range(&b), Some((0, 24)), "two whole lines");
    type_keys(&mut v, "d", &mut b);
    assert_eq!(b.text(), "third");

    // `V` + yank + paste duplicates the line.
    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "Vyp", &mut b);
    assert_eq!(b.text(), "hello world\nhello world\nsecond line\nthird");
    assert!(v.register.as_ref().unwrap().linewise);

    // `c` on a linewise selection deletes and enters insert.
    v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "Vc", &mut b);
    assert_eq!(b.text(), "second line\nthird");
}

/// `.` replays the last change: operator commands, counts, and insert
/// sessions including the text that was typed.
#[test]
fn repeat_last_change() {
    let mut v = Vim::new();
    let mut b = Buffer::new("one two three".to_string());
    type_keys(&mut v, "dw", &mut b);
    assert_eq!(b.text(), "two three");
    type_keys(&mut v, ".", &mut b);
    assert_eq!(b.text(), "three", "`.` replays at the new cursor");
    type_keys(&mut v, ".", &mut b);
    assert_eq!(b.text(), "", "and `.` does not record itself");

    // A count belongs to the recording.
    let mut v = Vim::new();
    let mut b = Buffer::new("a b c d e f".to_string());
    type_keys(&mut v, "2dw", &mut b);
    assert_eq!(b.text(), "c d e f");
    type_keys(&mut v, ".", &mut b);
    assert_eq!(b.text(), "e f");

    // `dd` and `p` replay as well.
    let mut v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "dd.", &mut b);
    assert_eq!(b.text(), "third");

    let mut v = Vim::new();
    let mut b = Buffer::new("ab\ncd\nef\ngh".to_string());
    type_keys(&mut v, "yyjp", &mut b);
    assert_eq!(b.text(), "ab\ncd\nab\nef\ngh");
    type_keys(&mut v, "3G.", &mut b);
    assert_eq!(
        b.text(),
        "ab\ncd\nab\nab\nef\ngh",
        "the paste repeats on the line the cursor is on"
    );

    // An insert session replays with the typed text, and stays in normal
    // mode.
    let mut v = Vim::new();
    let mut b = Buffer::new("one\ntwo".to_string());
    type_keys(&mut v, "o", &mut b);
    b.insert("new");
    v.note_inserted("new");
    v.enter_normal();
    assert_eq!(b.text(), "one\nnew\ntwo");
    assert_eq!(v.mode(), Mode::Normal);
    type_keys(&mut v, ".", &mut b);
    assert_eq!(b.text(), "one\nnew\nnew\ntwo");
    assert_eq!(v.mode(), Mode::Normal, "`.` stays in normal mode");

    // A command that changed nothing has nothing to repeat.
    let mut v = Vim::new();
    let mut b = buf();
    type_keys(&mut v, "2w", &mut b);
    assert_eq!(
        v.handle_normal_key(Key::Char('.'), &mut b),
        Outcome::Unhandled
    );
}
