//! The engine's tests, driven by typing keys.
//!
//! `type_keys` is the whole harness: a string of keystrokes through the same
//! entry point the UI uses, then assertions about the buffer and the cursor. The
//! helpers here build the buffers every subject needs; the subjects are the
//! submodules.

use crate::buffer::Buffer;
use crate::vim::*;

mod chords;
mod motions;
mod tables;
mod visual;

fn buf() -> Buffer {
    Buffer::new("hello world\nsecond line\nthird".to_string())
}

fn note_buf() -> Buffer {
    Buffer::new("# Top\nintro\n## Sub\nbody\ndeep\n# Other\ntail\n".to_string())
}

/// A table with distinct content in every cell, so the cell the cursor is in
/// can be read straight off the text.
fn table_buf() -> Buffer {
    Buffer::new(
        "| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n".to_string(),
    )
}

/// The same table with prose above and below, so a motion can be seen leaving
/// it.
fn table_buf_with_context() -> Buffer {
    Buffer::new(
            "before\n\n| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n\nafter\n"
                .to_string(),
        )
}

/// Feed a sequence of alt-held printable keys.
fn type_alt_keys(vim: &mut Vim, c: char, b: &mut Buffer) {
    vim.handle_normal_key(Key::AltChar(c), b);
}

/// Feed a sequence of normal-mode keys.
fn type_keys(vim: &mut Vim, keys: &str, b: &mut Buffer) {
    for c in keys.chars() {
        vim.handle_normal_key(Key::Char(c), b);
    }
}
