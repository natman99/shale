//! The case table the text objects and counted objects were checked against vim
//! with, and the harness that keeps the check cheap to repeat.
//!
//! `cargo test -p shale-core vim_table -- --ignored` prints one line per case as
//! `text \t cursor \t keys \t result`. The driver in
//! `scripts/vimcrosscheck.py` feeds the same cases to `vim -es` and diffs the two
//! outputs, so a behaviour claim here can be re-verified against the real editor
//! in one command instead of by hand.
//!
//! The cases are deliberately small: one line where possible, so a difference
//! shows up as a character position rather than a paragraph of prose.

use crate::buffer::Buffer;
use crate::vim::Vim;

use super::type_keys;

/// `text, cursor, keys` — the cursor is a byte offset into `text`.
const CASES: &[(&str, usize, &str)] = &[
    // Bracket pairs, from inside and from the characters themselves.
    ("a (b c) d\n", 4, "di("),
    ("a (b c) d\n", 2, "da("),
    ("a (b c) d\n", 6, "da("),
    ("a [b [c] d] e\n", 6, "di["),
    ("a [b [c] d] e\n", 6, "di]"),
    ("a [b [c] d] e\n", 6, "da["),
    ("a {b {c} d} e\n", 6, "di{"),
    ("a {b {c} d} e\n", 6, "diB"),
    ("a <b <c> d> e\n", 6, "di<"),
    ("a (b (c) d) e\n", 4, "di("),
    ("a (b (c) d) e\n", 4, "da("),
    ("a (b\n  c) d\n", 4, "di("),
    ("a (b\n  c) d\n", 4, "da("),
    ("no brackets here\n", 3, "di("),
    ("a (b c\n", 3, "di("),
    ("a )b c\n", 3, "di("),
    // Quotes, which were already here and stay the shape they were.
    ("say \"hi\" now\n", 5, "di\""),
    ("say \"hi\" now\n", 5, "da\""),
    // WORDs: markup is one WORD.
    ("see [[Target|label]] here\n", 6, "diW"),
    ("see [[Target|label]] here\n", 6, "daW"),
    ("see [[Target|label]] here\n", 6, "diw"),
    ("see [[Target|label]] here\n", 6, "daw"),
    ("a,b c\n", 2, "diW"),
    ("a,b c\n", 2, "diw"),
    // Counts: units, not objects.
    ("hello world foo bar\n", 0, "diw"),
    ("hello world foo bar\n", 0, "d2iw"),
    ("hello world foo bar\n", 0, "d3iw"),
    ("hello world foo bar\n", 0, "d4iw"),
    ("hello world foo bar\n", 0, "daw"),
    ("hello world foo bar\n", 0, "d2aw"),
    ("hello world foo bar\n", 0, "d3aw"),
    ("hello world foo bar\n", 6, "diw"),
    ("hello world foo bar\n", 6, "d2iw"),
    ("hello world foo bar\n", 6, "d3iw"),
    ("hello world foo bar\n", 6, "d2aw"),
    ("hello world\nsecond line\nthird", 0, "d2aw"),
    ("hello world\nsecond line\nthird", 0, "daw"),
    ("a,b c d\n", 0, "d2aW"),
    ("one\n\ntwo\n\nthree\n", 0, "d2ip"),
    ("one\n\ntwo\n\nthree\n", 0, "d2ap"),
    ("one\n\ntwo\n\nthree\n", 0, "dip"),
    // Changing and yanking an object, not only deleting it.
    ("a (b c) d\n", 4, "ci("),
    ("a (b c) d\n", 4, "yi("),
    ("hello world foo bar\n", 0, "y2iw"),
];

#[test]
#[ignore = "prints the table; scripts/vimcrosscheck.py drives vim with it"]
fn vim_table() {
    for (text, cursor, keys) in CASES {
        let mut vim = Vim::new();
        let mut buf = Buffer::new((*text).to_string());
        buf.set_cursor(*cursor);
        type_keys(&mut vim, keys, &mut buf);
        println!(
            "{}\t{}\t{}\t{}",
            text.escape_debug(),
            cursor,
            keys,
            buf.text().escape_debug()
        );
    }
}
