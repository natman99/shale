//! `gv` — put back the selection that was last active. vim remembers the last
//! visual area even after an operator consumed it, which is what makes `viwd`
//! followed by `gvd` delete the same text twice. Every expectation here was read
//! off vim.

use super::*;

fn run(text: &str, keys: &str) -> (String, Option<(usize, usize)>) {
    let mut vim = Vim::new();
    let mut b = Buffer::new(text.to_string());
    type_keys(&mut vim, keys, &mut b);
    (b.text().to_string(), b.selection())
}

#[test]
fn gv_reselects_what_was_just_selected_and_operators_take_it() {
    let (text, _) = run("one\ntwo\nthree\nfour\n", "Vjjygv");
    assert_eq!(text, "one\ntwo\nthree\nfour\n", "nothing was deleted yet");
    let (text, _) = run("one\ntwo\nthree\nfour\n", "Vjjygvd");
    assert_eq!(text, "four\n", "the same three lines, again");
}

#[test]
fn the_mode_comes_back_with_the_selection() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("one\ntwo\nthree\n".to_string());
    type_keys(&mut vim, "Vjy", &mut b);
    assert_eq!(vim.mode(), Mode::Normal, "the yank ended visual");
    type_keys(&mut vim, "gv", &mut b);
    assert_eq!(vim.mode(), Mode::VisualLine, "and `gv` is back in it");

    let mut vim = Vim::new();
    let mut b = Buffer::new("one\ntwo\nthree\n".to_string());
    type_keys(&mut vim, "viwy", &mut b);
    type_keys(&mut vim, "gv", &mut b);
    assert_eq!(vim.mode(), Mode::Visual, "charwise stays charwise");
}

#[test]
fn a_charwise_selection_comes_back_exactly_as_it_was() {
    // `vljj` walks the selection down three lines, one column wide at each stop;
    // vim's `gvd` deletes exactly that, `one\ntwo\nt` plus the `h` the last `j`
    // landed on.
    let (text, _) = run("one\ntwo\nthree\nfour\n", "vljjy");
    assert_eq!(text, "one\ntwo\nthree\nfour\n");
    let (text, _) = run("one\ntwo\nthree\nfour\n", "vljjygvd");
    assert_eq!(text, "ree\nfour\n");

    // The same selection without the round trip through `gv`, to show the two
    // agree.
    let (direct, _) = run("one\ntwo\nthree\nfour\n", "vljjd");
    let (via_gv, _) = run("one\ntwo\nthree\nfour\n", "vljjygvd");
    assert_eq!(direct, via_gv, "`gv` restores the same range");
}

#[test]
fn the_cursor_is_back_on_the_end_it_was_on() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("one\ntwo\nthree\n".to_string());
    type_keys(&mut vim, "0vljy", &mut b);
    let before = b.cursor();
    type_keys(&mut vim, "gv", &mut b);
    assert_eq!(
        b.cursor(),
        before,
        "the head of the selection, not its start"
    );
    let (anchor, head) = b.selection().unwrap();
    assert_eq!(
        (anchor, head),
        (0, before),
        "and the same anchor and head, inclusive as a charwise selection is"
    );
}

#[test]
fn gv_without_a_previous_selection_does_nothing() {
    let (text, selection) = run("one\ntwo\n", "0gv");
    assert_eq!(text, "one\ntwo\n");
    assert!(
        selection.is_none(),
        "and no selection appears out of nowhere"
    );
}

#[test]
fn a_reselected_area_can_be_changed_not_only_deleted() {
    let (text, _) = run("one\ntwo\nthree\n", "viwygvc");
    // `c` on a reselected word empties it and drops into insert mode.
    assert_eq!(text, "\ntwo\nthree\n");
}
