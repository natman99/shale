//! Vim-style mode state machine, decoupled from any UI.
//!
//! The UI feeds printable keys to [`Vim::handle_normal_key`] while in normal
//! mode; in insert mode all text goes to the buffer directly and only
//! [`Vim::handle_insert_key`] is consulted.

use crate::buffer::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
}

/// A normal-mode operator waiting for a motion (`d`, `c`, `y`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Delete,
    Change,
    Yank,
}

impl Operator {
    fn char(self) -> char {
        match self {
            Operator::Delete => 'd',
            Operator::Change => 'c',
            Operator::Yank => 'y',
        }
    }
}

/// A partially-typed multi-key normal-mode command. The count typed before the
/// prefix is carried along, since `gg`/`G` take their line number from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Pending {
    #[default]
    None,
    /// An operator typed, waiting for its motion.
    Operator { op: Operator, count: usize },
    /// `g` typed, waiting for a second `g` or `x`.
    Goto { count: usize },
}

/// What the UI should do after a normal-mode key was handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing further; stay in normal mode.
    Stay,
    /// Switch to insert mode (the buffer is already positioned).
    EnterInsert,
    /// Key not applicable (e.g. unbound); UI may fall back to default handling.
    Unhandled,
    /// `gx` was typed: the UI should follow the link under the cursor.
    FollowLink,
}

/// The unnamed register, holding the last yank or delete.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Register {
    text: String,
    linewise: bool,
}

#[derive(Debug, Clone)]
pub struct Vim {
    mode: Mode,
    pending: Pending,
    /// Count typed so far, applied to the next motion or operator.
    count: Option<usize>,
    register: Option<Register>,
}

impl Default for Vim {
    fn default() -> Self {
        Self::new()
    }
}

impl Vim {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            pending: Pending::None,
            count: None,
            register: None,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Count typed so far, for the status bar.
    pub fn count(&self) -> Option<usize> {
        self.count
    }

    /// Operator awaiting its motion, for the status bar.
    pub fn pending_operator(&self) -> Option<char> {
        match self.pending {
            Pending::Operator { op, .. } => Some(op.char()),
            _ => None,
        }
    }

    pub fn enter_insert(&mut self) {
        self.mode = Mode::Insert;
        self.pending = Pending::None;
        self.count = None;
    }

    pub fn enter_normal(&mut self) {
        self.mode = Mode::Normal;
        self.pending = Pending::None;
        self.count = None;
        // Vim leaves the cursor on the last char of the line when past EOL.
    }

    /// Handle a printable key press while in insert mode. Returns true if the
    /// key was consumed (Esc) and the mode switched to normal.
    pub fn handle_insert_key(&mut self, key: Key) -> bool {
        if key == Key::Escape {
            self.enter_normal();
            return true;
        }
        false
    }

    /// Handle a key press while in normal mode.
    pub fn handle_normal_key(&mut self, key: Key, buf: &mut Buffer) -> Outcome {
        // A digit extends the count. A bare `0` (with no count in progress) is
        // the line-start motion instead.
        if let Key::Char(c @ '0'..='9') = key {
            if c == '0' && self.count.is_none() && self.pending == Pending::None {
                buf.line_start_motion();
                return Outcome::Stay;
            }
            self.count = Some(self.count.unwrap_or(0) * 10 + (c as usize - '0' as usize));
            return Outcome::Stay;
        }

        // An operator in progress consumes the next key as its motion.
        if let Pending::Operator { op, count } = self.pending {
            self.pending = Pending::None;
            let motion_count = self.count.take().unwrap_or(1);
            // Vim multiplies a count given on both sides of the operator.
            return self.apply_operator(op, count * motion_count, key, buf);
        }

        if let Pending::Goto { count } = self.pending {
            self.pending = Pending::None;
            let count = self.count.take().unwrap_or(count);
            return match key {
                Key::Char('g') => {
                    if count > 1 {
                        buf.goto_line(count - 1);
                    } else {
                        buf.buffer_start();
                    }
                    Outcome::Stay
                }
                Key::Char('x') => Outcome::FollowLink,
                _ => Outcome::Unhandled,
            };
        }

        let count = self.count.take().unwrap_or(1);
        match key {
            // Motions
            Key::Char('h') => repeat(count, || buf.move_left()),
            Key::Char('j') => repeat(count, || buf.move_down()),
            Key::Char('k') => repeat(count, || buf.move_up()),
            Key::Char('l') => repeat(count, || buf.move_right_stay_on_line()),
            Key::Char('w') => repeat(count, || buf.word_forward()),
            Key::Char('b') => repeat(count, || buf.word_backward()),
            Key::Char('e') => repeat(count, || buf.word_end()),
            Key::Char('^') => buf.first_non_blank(),
            Key::Char('$') => buf.line_last_char_motion(),
            Key::Char('G') => {
                if count > 1 {
                    buf.goto_line(count - 1);
                } else {
                    buf.buffer_end();
                }
            }
            Key::Char('g') => {
                self.pending = Pending::Goto { count };
                return Outcome::Stay;
            }
            // Operators
            Key::Char('d') | Key::Char('c') | Key::Char('y') => {
                let op = match key {
                    Key::Char('d') => Operator::Delete,
                    Key::Char('c') => Operator::Change,
                    _ => Operator::Yank,
                };
                self.pending = Pending::Operator { op, count };
                return Outcome::Stay;
            }
            Key::Char('x') => {
                let (start, end) = char_range(buf, buf.cursor(), count);
                self.register = Some(Register {
                    text: buf.text()[start..end].to_string(),
                    linewise: false,
                });
                buf.delete_range(start, end);
            }
            Key::Char('p') => self.paste(true, buf),
            Key::Char('P') => self.paste(false, buf),
            // Insert transitions
            Key::Char('i') => return Outcome::EnterInsert,
            Key::Char('a') => {
                buf.move_right();
                return Outcome::EnterInsert;
            }
            Key::Char('I') => {
                buf.first_non_blank();
                return Outcome::EnterInsert;
            }
            Key::Char('A') => {
                buf.line_last_char_motion();
                buf.move_right();
                return Outcome::EnterInsert;
            }
            Key::Char('o') => {
                buf.open_line_below(count);
                return Outcome::EnterInsert;
            }
            Key::Char('O') => {
                buf.open_line_above(count);
                return Outcome::EnterInsert;
            }
            // Undo (redo is bound to ctrl-r by the UI).
            Key::Char('u') => buf.undo(),
            Key::Escape => buf.clear_selection(),
            _ => return Outcome::Unhandled,
        }
        Outcome::Stay
    }

    /// Apply `op` over `count` repetitions of the motion `key`.
    fn apply_operator(
        &mut self,
        op: Operator,
        count: usize,
        key: Key,
        buf: &mut Buffer,
    ) -> Outcome {
        // Linewise forms: the doubled operator, and line-crossing motions.
        let linewise = match key {
            Key::Char(c) if c == op.char() => Some(count),
            Key::Char('j') | Key::Char('k') => Some(count + 1),
            _ => None,
        };

        if let Some(lines) = linewise {
            let row = buf.row_col(buf.cursor()).0;
            match op {
                Operator::Yank => {
                    self.register = Some(Register {
                        text: buf.yank_lines(row, lines),
                        linewise: true,
                    });
                    return Outcome::Stay;
                }
                Operator::Delete => buf.delete_line(lines),
                Operator::Change => {
                    buf.delete_line(lines);
                    return Outcome::EnterInsert;
                }
            }
            return Outcome::Stay;
        }

        // Charwise: run the motion, then act on the swept range.
        let start = buf.cursor();
        match key {
            Key::Char('w') => repeat(count, || buf.word_forward()),
            Key::Char('e') => repeat(count, || buf.word_end()),
            Key::Char('b') => repeat(count, || buf.word_backward()),
            Key::Char('h') => repeat(count, || buf.move_left()),
            Key::Char('l') => repeat(count, || buf.move_right_stay_on_line()),
            Key::Char('0') => buf.line_start_motion(),
            Key::Char('^') => buf.first_non_blank(),
            Key::Char('$') => buf.line_last_char_motion(),
            Key::Char('G') => buf.goto_line(count.saturating_sub(1)),
            _ => {
                // Unknown motion: cancel the operator rather than guessing.
                return Outcome::Unhandled;
            }
        }
        let mut end = buf.cursor();
        // `$` lands on the last character, which the operator must include.
        if key == Key::Char('$') {
            end = buf.next_char_boundary(end);
        }
        let (lo, hi) = (start.min(end), start.max(end));
        match op {
            Operator::Yank => {
                self.register = Some(Register {
                    text: buf.text()[lo..hi].to_string(),
                    linewise: false,
                });
            }
            Operator::Delete => {
                self.register = Some(Register {
                    text: buf.text()[lo..hi].to_string(),
                    linewise: false,
                });
                buf.delete_range(lo, hi);
            }
            Operator::Change => {
                buf.delete_range(lo, hi);
                return Outcome::EnterInsert;
            }
        }
        Outcome::Stay
    }

    /// Paste the register after (`p`) or before (`P`) the cursor.
    fn paste(&mut self, after: bool, buf: &mut Buffer) {
        let Some(reg) = self.register.clone() else {
            return;
        };
        let len = buf.text().len();
        if reg.linewise {
            let row = buf.row_col(buf.cursor()).0;
            let text = if reg.text.ends_with('\n') {
                reg.text
            } else {
                format!("{}\n", reg.text)
            };
            let offset = if after {
                (buf.line_end(row) + 1).min(len)
            } else {
                buf.line_start(row)
            };
            buf.insert_at(offset, &text);
        } else {
            let offset = if after {
                buf.next_char_boundary(buf.cursor()).min(len)
            } else {
                buf.cursor()
            };
            buf.insert_at(offset, &reg.text);
        }
    }
}

fn repeat(count: usize, mut f: impl FnMut()) {
    for _ in 0..count.max(1) {
        f();
    }
}

/// The byte range of `count` characters starting at `offset`.
fn char_range(buf: &Buffer, offset: usize, count: usize) -> (usize, usize) {
    let mut end = offset;
    for _ in 0..count.max(1) {
        end = buf.next_char_boundary(end);
    }
    (offset, end)
}

/// UI-independent key identity for the vim engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Escape,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;

    fn buf() -> Buffer {
        Buffer::new("hello world\nsecond line\nthird".to_string())
    }

    /// Feed a sequence of normal-mode keys.
    fn type_keys(vim: &mut Vim, keys: &str, b: &mut Buffer) {
        for c in keys.chars() {
            vim.handle_normal_key(Key::Char(c), b);
        }
    }

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
    fn gx_requests_link_follow() {
        let mut vim = Vim::new();
        let mut b = buf();
        b.set_cursor(20);
        vim.handle_normal_key(Key::Char('g'), &mut b);
        assert_eq!(
            vim.handle_normal_key(Key::Char('x'), &mut b),
            Outcome::FollowLink
        );
        // The motion-less `gx` leaves the cursor where it was.
        assert_eq!(b.cursor(), 20);
        assert_eq!(vim.mode(), Mode::Normal);
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
}
