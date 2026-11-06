//! `;` and `,` — repeat the last `f`/`t`/`F`/`T`. What they repeat is the
//! *command*, inclusivity and all, which is why a `;` after `tw` looks inert: the
//! cursor already sits right before the character it is looking for, and an
//! exclusive find cannot move. vim behaves that way; the quirk is kept on purpose.
//!
//! The expectations were read off vim with a replace-the-character probe, which is
//! how a motion's landing spot shows up in text.

use super::*;

fn cursor_after(text: &str, keys: &str) -> usize {
    let mut vim = Vim::new();
    let mut b = Buffer::new(text.to_string());
    type_keys(&mut vim, keys, &mut b);
    b.cursor()
}

#[test]
fn semicolon_repeats_the_find_in_the_same_direction() {
    // "a w b w c w d": the `w`s are at 2, 6, 10.
    assert_eq!(cursor_after("a w b w c w d\n", "0fw"), 2);
    assert_eq!(cursor_after("a w b w c w d\n", "0fw;"), 6);
    assert_eq!(cursor_after("a w b w c w d\n", "0fw;;"), 10);
    assert_eq!(cursor_after("a w b w c w d\n", "0fw2;"), 10, "a count reaches the second one");
}

#[test]
fn a_failed_repeat_moves_nothing() {
    // Past the last `w` there is nothing to find, and vim leaves the cursor where
    // it was rather than parking it at the end of the line.
    assert_eq!(cursor_after("a w b w c w d\n", "0fw3;"), 2);
    assert_eq!(cursor_after("a w b w c w d\n", "0fz;"), 0, "a find that never found is nothing to repeat");
    assert_eq!(cursor_after("a w b w c w d\n", "0;"), 0, "and `;` with no find before it does nothing");
}

#[test]
fn comma_repeats_in_the_other_direction() {
    assert_eq!(cursor_after("a w b w c w d\n", "0fw2;,"), 6, "one step back");
    assert_eq!(cursor_after("a w b w c w d\n", "0fw2;,,,"), 0 + 2, "back as far as the first one");
    assert_eq!(cursor_after("a w b w c w d\n", "0fW;"), 0, "a backward find repeats backward");
}

#[test]
fn the_inclusivity_belongs_to_the_original_command() {
    // `t` stops *before* its character, so a `;` after `tw` is looking at the `w`
    // that sits one past the cursor and cannot move. That is vim's behaviour.
    assert_eq!(cursor_after("a w b w c w d\n", "0tw"), 1);
    assert_eq!(cursor_after("a w b w c w d\n", "0tw;"), 1);
    // From a place where there is another `w` further along, it does move.
    assert_eq!(cursor_after("a w b w c w d\n", "04ltw;"), 5);
}

#[test]
fn an_operator_can_take_the_repeat() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a w b w c w d\n".to_string());
    type_keys(&mut vim, "0fw", &mut b);
    type_keys(&mut vim, "d;", &mut b);
    // `f` is inclusive, so `d;` takes through the `w` it lands on.
    assert_eq!(b.text(), "a  c w d\n");

    let mut vim = Vim::new();
    let mut b = Buffer::new("a w b w c w d\n".to_string());
    type_keys(&mut vim, "0tw", &mut b);
    type_keys(&mut vim, "d;", &mut b);
    // The cursor already sits just before a `w`, so an exclusive repeat has
    // nowhere to go and the operator is cancelled rather than deleting nothing.
    assert_eq!(b.text(), "a w b w c w d\n");

    let mut vim = Vim::new();
    let mut b = Buffer::new("a w b w c w d\n".to_string());
    type_keys(&mut vim, "0fwy;", &mut b);
    type_keys(&mut vim, "0P", &mut b);
    assert_eq!(b.text(), "w b wa w b w c w d\n", "and a repeat yanks like a find would");
}

#[test]
fn the_repeat_survives_other_commands_in_between() {
    // The memory is of the last find, not of the last thing typed: two `w` steps
    // in between, and `;` still looks for `w`.
    assert_eq!(cursor_after("a w b w c w d\n", "0fw w w;"), 10);
    assert_eq!(cursor_after("a w b w c w d\n", "0fT;"), 0, "a find of an absent character has nothing to give");
}
