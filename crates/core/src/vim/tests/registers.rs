//! Named and numbered registers. Every expectation here was read out of vim's own
//! `:reg` output (`vim -es -N -u NONE -i NONE`, `:reg` redirected to a file), which
//! is the only way to see the rules rather than guess them — and two of them are
//! not what you would guess: a `"`-named register also fills the unnamed one, and
//! a small charwise delete goes to `"-` instead of the numbered list.

use super::*;

fn run(text: &str, keys: &str) -> String {
    let mut vim = Vim::new();
    let mut b = Buffer::new(text.to_string());
    type_keys(&mut vim, keys, &mut b);
    b.text().to_string()
}

#[test]
fn a_named_register_also_fills_the_unnamed_one() {
    // vim: `"ayw` then `p` pastes the word. The prefix picks an *extra* register;
    // it does not hide the operation from the unnamed one.
    assert_eq!(run("one two three\n", r#"0"aywp"#), "oone ne two three\n");
}

#[test]
fn a_yank_leaves_the_cursor_where_it_was() {
    // The paste lands where the yank started, not where its motion ended — which
    // is what makes the case above read the way it does.
    assert_eq!(run("one two three\n", "0ywp"), "oone ne two three\n");
    assert_eq!(run("one two three\n", "0ywwp"), "one tone wo three\n");
}

#[test]
fn a_named_yank_does_not_touch_register_zero() {
    // `"0` holds the last *unnamed* yank. `"ayw` then `"0p` pastes nothing.
    assert_eq!(run("one two three\n", r#"0"ayw"0p"#), "one two three\n");
    // An unnamed yank does fill it, and a later delete does not disturb it.
    assert_eq!(run("one two three\n", r#"0ywdw"0p"#), "tone wo three\n");
}

#[test]
fn a_small_delete_goes_to_the_dash_register_not_the_numbers() {
    // `dw` is a small delete: `"-` takes it and `"1` stays empty, so `"1p` does
    // nothing and `"-p` pastes the word back.
    assert_eq!(run("one two three\n", r#"0dw"1p"#), "two three\n");
    assert_eq!(run("one two three\n", r#"0dw"-p"#), "tone wo three\n");
}

#[test]
fn a_line_delete_pushes_the_numbered_registers() {
    assert_eq!(run("one\ntwo\nthree\n", r#"0dd"1p"#), "two\none\nthree\n");
    // Most recent first: the second `dd` took `two`, so `"1` is `two` and `"2` is
    // the earlier `one`.
    assert_eq!(run("one\ntwo\nthree\n", r#"0dddd"1p"#), "three\ntwo\n");
    assert_eq!(run("one\ntwo\nthree\n", r#"0dddd"2p"#), "three\none\n");
}

#[test]
fn two_named_registers_hold_two_different_texts() {
    // Yank `one ` into `a`, then `two ` into `b`, then paste `b` where the cursor
    // sits. Neither paste reaches for the other register.
    assert_eq!(
        run("one two three\n", r#"0"ayww"byw"bp"#),
        "one ttwo wo three\n",
        "the second yank needs its own `y`: a register prefix with no operator yanks nothing"
    );
}

#[test]
fn the_black_hole_leaves_the_unnamed_register_alone() {
    // `"_dw` deletes into nothing, so the earlier yank is still what `p` pastes.
    assert_eq!(run("one two three\n", r#"0yw"_dwp"#), "tone wo three\n");
}

#[test]
fn a_named_linewise_delete_still_pushes_the_numbers() {
    // The register prefix does not exempt the operation from the automatic
    // registers: `"addw` fills `a`, the unnamed register, and `"1`.
    assert_eq!(run("one\ntwo\nthree\n", r#"0"add"1p"#), "two\none\nthree\n");
}

#[test]
fn pasting_from_an_empty_register_does_nothing() {
    assert_eq!(run("one two\n", r#"0"qp"#), "one two\n");
    assert_eq!(run("one two\n", r#"0"9p"#), "one two\n");
}

#[test]
fn the_prefix_applies_to_one_operation_only() {
    // After `"ayw` the prefix is spent: the deletes that follow are ordinary ones.
    assert_eq!(run("one two three\n", r#"0"aywdwdw"#), "three\n");
}
