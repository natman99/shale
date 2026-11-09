//! vim's own text objects: bracket pairs and WORDs, driven through the engine.
//!
//! These are the objects a vim user types without translating — `ci(`, `da"`,
//! `diW` — as opposed to Shale's markdown objects (`i#`, `iL`, `i*`), which name
//! constructs vim has no letter for. Every expectation was read off real vim.
//!
//! One deliberate difference from vim, and it is the same one the quote object
//! already makes: brackets inside a code span or a fenced block are text, not
//! brackets. vim does not know Markdown; Shale does, and `di(` in prose should not
//! see the parentheses a code block happens to hold.

use super::*;

fn pair_buf() -> Buffer {
    Buffer::new("a (b (c) d) e\n".to_string())
}

#[test]
fn di_paren_takes_the_innermost_pair() {
    let mut vim = Vim::new();
    let mut b = pair_buf();
    b.set_cursor(6); // the `c`
    type_keys(&mut vim, "di(", &mut b);
    assert_eq!(b.text(), "a (b () d) e\n");

    let mut vim = Vim::new();
    let mut b = pair_buf();
    b.set_cursor(2); // the first `(`
    type_keys(&mut vim, "di(", &mut b);
    assert_eq!(b.text(), "a () e\n", "from the outer pair, the outer pair");
}

#[test]
fn da_paren_takes_the_pair_with_it() {
    let mut vim = Vim::new();
    let mut b = pair_buf();
    b.set_cursor(2);
    type_keys(&mut vim, "da(", &mut b);
    assert_eq!(
        b.text(),
        "a  e\n",
        "the brackets and what was between them, and no more"
    );

    let mut vim = Vim::new();
    let mut b = pair_buf();
    b.set_cursor(6);
    type_keys(&mut vim, "da(", &mut b);
    assert_eq!(b.text(), "a (b  d) e\n");
}

#[test]
fn a_pair_answers_to_either_of_its_characters() {
    // vim's `i[` and `i]` are one object, as are `i{`, `i}` and `iB`, and `ib`
    // names parens. The letter the fingers reach for should not matter.
    for (keys, expect) in [
        ("di[", "a [b [] d] e\n"),
        ("di]", "a [b [] d] e\n"),
        ("diB", "a {b {} d} e\n"),
        ("di{", "a {b {} d} e\n"),
        ("di(", "a (b () d) e\n"),
        ("dib", "a (b () d) e\n"),
        ("di<", "a <b <> d> e\n"),
    ] {
        let inner = keys[2..].chars().next().unwrap();
        let src = match inner {
            '[' | ']' => "a [b [c] d] e\n",
            '{' | 'B' => "a {b {c} d} e\n",
            '<' => "a <b <c> d> e\n",
            _ => "a (b (c) d) e\n",
        };
        let mut vim = Vim::new();
        let mut b = Buffer::new(src.to_string());
        b.set_cursor(6);
        type_keys(&mut vim, keys, &mut b);
        assert_eq!(b.text(), expect, "`{keys}` on {src:?}");
    }
}

#[test]
fn a_pair_that_never_closes_matches_nothing() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a (b c\n".to_string());
    b.set_cursor(3);
    type_keys(&mut vim, "di(", &mut b);
    assert_eq!(b.text(), "a (b c\n", "no close, no object");
    assert_eq!(vim.register_text(), None, "and nothing yanked or cut");
}

#[test]
fn a_backslash_hides_the_bracket_after_it() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("x (a\\)b) y\n".to_string());
    b.set_cursor(5);
    type_keys(&mut vim, "di(", &mut b);
    assert_eq!(
        b.text(),
        "x ()b) y\n",
        "the escaped `)` is content, not the close"
    );
}

#[test]
fn a_pair_may_cross_lines() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a (b\nc) d\n".to_string());
    b.set_cursor(3);
    type_keys(&mut vim, "di(", &mut b);
    assert_eq!(b.text(), "a () d\n");
}

#[test]
fn brackets_inside_code_are_not_brackets() {
    // The Shale-specific rule, stated in the open: the pair object follows the
    // quote object's lead and looks through code.
    let mut vim = Vim::new();
    let mut b = Buffer::new("use `f(x)` now\n".to_string());
    b.set_cursor(6);
    type_keys(&mut vim, "di(", &mut b);
    assert_eq!(b.text(), "use `f(x)` now\n", "the span's parens are text");

    let mut vim = Vim::new();
    let mut b = Buffer::new("```\nlet x = f(1);\n```\n".to_string());
    b.set_cursor(12);
    type_keys(&mut vim, "di(", &mut b);
    assert_eq!(
        b.text(),
        "```\nlet x = f(1);\n```\n",
        "and so are a block's"
    );
}

#[test]
fn the_quote_object_answers_to_the_quote_marks() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("say \"hi\" now\n".to_string());
    b.set_cursor(5);
    type_keys(&mut vim, "di\"", &mut b);
    assert_eq!(b.text(), "say \"\" now\n");

    // `da"` leaves the two spaces behind. vim swallows one of them here; Shale's
    // quote object is the pair and nothing else, which is what `aq` has always
    // been and what its own tests hold it to.
    let mut vim = Vim::new();
    let mut b = Buffer::new("say \"hi\" now\n".to_string());
    b.set_cursor(5);
    type_keys(&mut vim, "da\"", &mut b);
    assert_eq!(b.text(), "say  now\n");

    // An apostrophe inside a word is not a quotation, which is the rule the quote
    // object already had and `i'` now inherits.
    let mut vim = Vim::new();
    let mut b = Buffer::new("it's here\n".to_string());
    b.set_cursor(3);
    type_keys(&mut vim, "di'", &mut b);
    assert_eq!(b.text(), "it's here\n");
}

#[test]
fn the_word_object_takes_the_whole_markup_word() {
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c d\n".to_string());
    type_keys(&mut vim, "diW", &mut b);
    assert_eq!(b.text(), " c d\n", "`a,b` is one WORD");

    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c d\n".to_string());
    type_keys(&mut vim, "daW", &mut b);
    assert_eq!(
        b.text(),
        "c d\n",
        "and `aW` takes its trailing space, as `aw` does"
    );

    let mut vim = Vim::new();
    let mut b = Buffer::new("**bold** text\n".to_string());
    type_keys(&mut vim, "diW", &mut b);
    assert_eq!(b.text(), " text\n", "markup and all");
}

/// vim counts a counted object as alternating units — object, gap, object — and
/// the sweep ends at the end of the Nth unit for `i` and the 2Nth for `a`. Every
/// row below was read off vim on `hello world foo bar` (W1 S1 W2 S2 W3 S3 W4),
/// with the cursor on the first word unless stated, because the rule is not the
/// intuitive "N objects" and is easy to get wrong twice.
#[test]
fn a_count_counts_units_not_objects() {
    let cases: &[(&str, &str)] = &[
        // `iw`: one word, then its space, then the next word, then its space.
        ("diw", " world foo bar\n"),
        ("d2iw", "world foo bar\n"),
        ("d3iw", " foo bar\n"),
        ("d4iw", "foo bar\n"),
        // `aw`: each count takes a word *and* its space.
        ("daw", "world foo bar\n"),
        ("d2aw", "foo bar\n"),
        ("d3aw", "bar\n"),
    ];
    for (keys, expect) in cases {
        let mut vim = Vim::new();
        let mut b = Buffer::new("hello world foo bar\n".to_string());
        type_keys(&mut vim, keys, &mut b);
        assert_eq!(b.text(), *expect, "`{keys}`");
    }

    // The same from the second word, so the rule cannot be an artifact of being
    // at the start of the line.
    for (keys, expect) in [
        ("diw", "hello  foo bar\n"),
        ("d2iw", "hello foo bar\n"),
        ("d3iw", "hello  bar\n"),
    ] {
        let mut vim = Vim::new();
        let mut b = Buffer::new("hello world foo bar\n".to_string());
        b.set_cursor(6);
        type_keys(&mut vim, keys, &mut b);
        assert_eq!(b.text(), expect, "`{keys}` from the second word");
    }

    // WORDs and paragraphs count the same way.
    let mut vim = Vim::new();
    let mut b = Buffer::new("a,b c d\n".to_string());
    type_keys(&mut vim, "d2aW", &mut b);
    assert_eq!(b.text(), "d\n");

    let mut vim = Vim::new();
    let mut b = Buffer::new("one\n\ntwo\n\nthree\n".to_string());
    type_keys(&mut vim, "d2ip", &mut b);
    assert_eq!(b.text(), "two\n\nthree\n");

    let mut vim = Vim::new();
    let mut b = Buffer::new("one\n\ntwo\n\nthree\n".to_string());
    type_keys(&mut vim, "d2ap", &mut b);
    assert_eq!(b.text(), "three\n");
}
