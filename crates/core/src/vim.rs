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
    /// `f`/`t`/`F`/`T` typed, waiting for the character to find. `op` is set for
    /// `dfw`-style commands, so the operator resumes once the search resolves.
    Find {
        op: Option<Operator>,
        count: usize,
        forward: bool,
        inclusive: bool,
    },
    /// `i` or `a` typed after an operator, waiting for the object's scope (`w`
    /// or `p`). `op` is `None` for the bare `iw` motion, used from visual mode.
    TextObject {
        op: Option<Operator>,
        count: usize,
        around: bool,
    },
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
            Pending::Find { op: Some(op), .. } => Some(op.char()),
            Pending::TextObject { op: Some(op), .. } => Some(op.char()),
            _ => None,
        }
    }

    /// The half-typed motion that is still waiting for its argument (`f`, `t`,
    /// `F`, `T`), for the status bar.
    pub fn pending_motion(&self) -> Option<char> {
        match self.pending {
            Pending::Find {
                forward, inclusive, ..
            } => Some(match (forward, inclusive) {
                (true, true) => 'f',
                (true, false) => 't',
                (false, true) => 'F',
                (false, false) => 'T',
            }),
            Pending::TextObject { around, .. } => Some(if around { 'a' } else { 'i' }),
            _ => None,
        }
    }

    /// Whether the engine is waiting for a literal character argument (`f`/`t`
    /// and friends). Those keys are not bound in the keymap, so the UI feeds
    /// them in through the input handler instead.
    pub fn waiting_for_literal(&self) -> bool {
        matches!(
            self.pending,
            Pending::Find { .. } | Pending::TextObject { .. }
        )
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

        // A find is waiting for its character.
        if let Pending::Find {
            op,
            count,
            forward,
            inclusive,
        } = self.pending
        {
            self.pending = Pending::None;
            let Key::Char(c) = key else {
                return Outcome::Unhandled;
            };
            let start = buf.cursor();
            buf.find_char(c, forward, inclusive, count);
            let Some(op) = op else {
                return Outcome::Stay;
            };
            let end = buf.cursor();
            if start == end {
                // Nothing found: the operator is cancelled, as in vim.
                return Outcome::Stay;
            }
            let (lo, hi) = if end > start {
                // A forward `f` lands *on* the character, which the operator
                // must include.
                (
                    start,
                    if inclusive {
                        buf.next_char_boundary(end)
                    } else {
                        end
                    },
                )
            } else {
                (end, start)
            };
            return self.apply_range(op, lo, hi, buf);
        }

        // A text object is waiting for its scope: `diw`, `dap`, `2aw`.
        if let Pending::TextObject { op, count, around } = self.pending {
            self.pending = Pending::None;
            let Key::Char(scope @ ('w' | 'p')) = key else {
                return Outcome::Unhandled;
            };
            let inner = !around;
            let (lo, mut hi) = Self::object_range(buf, buf.cursor(), scope, inner);
            // A count typed before the object extends it over the following
            // objects, as `d2aw` does; the gap between them is absorbed so the
            // sweep stays contiguous.
            for _ in 1..count {
                let mut at = hi;
                while at < buf.text().len()
                    && buf.text()[at..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_whitespace())
                {
                    at = buf.next_char_boundary(at);
                }
                let (_, next_hi) = Self::object_range(buf, at, scope, inner);
                if next_hi <= hi {
                    break;
                }
                hi = next_hi;
            }
            let Some(op) = op else {
                // Bare `iw`/`ip`: a motion, not an operator.
                buf.set_cursor(lo);
                return Outcome::Stay;
            };
            return self.apply_range(op, lo, hi, buf);
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
            // Vim binds space to the same motion as `l`; as the argument of
            // `f `/`t ` it arrives through the pending find instead.
            Key::Char(' ') => repeat(count, || buf.move_right_stay_on_line()),
            Key::Char('w') => repeat(count, || buf.word_forward()),
            Key::Char('b') => repeat(count, || buf.word_backward()),
            Key::Char('e') => repeat(count, || buf.word_end()),
            Key::Char('f') | Key::Char('F') | Key::Char('t') | Key::Char('T') => {
                self.pending = Pending::Find {
                    op: None,
                    count,
                    forward: matches!(key, Key::Char('f') | Key::Char('t')),
                    inclusive: matches!(key, Key::Char('f') | Key::Char('F')),
                };
                return Outcome::Stay;
            }
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
                // `I` is line-wise, unlike `^` (which is screen-line).
                buf.row_first_non_blank();
                return Outcome::EnterInsert;
            }
            Key::Char('A') => {
                // Likewise `A` appends at the end of the whole row.
                buf.row_last_char_motion();
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
    /// The range of the text object with the given scope under `offset`.
    fn object_range(buf: &Buffer, offset: usize, scope: char, inner: bool) -> (usize, usize) {
        match scope {
            'p' => buf.paragraph_object(offset, inner),
            _ => buf.word_object(offset, inner),
        }
    }

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
            Key::Char(' ') => repeat(count, || buf.move_right_stay_on_line()),
            Key::Char('0') => buf.line_start_motion(),
            Key::Char('^') => buf.first_non_blank(),
            Key::Char('$') => buf.line_last_char_motion(),
            Key::Char('G') => buf.goto_line(count.saturating_sub(1)),
            Key::Char('f') | Key::Char('F') | Key::Char('t') | Key::Char('T') => {
                // The operator waits for the search character, e.g. `dfw`.
                self.pending = Pending::Find {
                    op: Some(op),
                    count,
                    forward: matches!(key, Key::Char('f') | Key::Char('t')),
                    inclusive: matches!(key, Key::Char('f') | Key::Char('F')),
                };
                return Outcome::Stay;
            }
            Key::Char('i') | Key::Char('a') => {
                // The operator waits for the object's scope, e.g. `diw`.
                self.pending = Pending::TextObject {
                    op: Some(op),
                    count,
                    around: key == Key::Char('a'),
                };
                return Outcome::Stay;
            }
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
        self.apply_range(op, lo, hi, buf)
    }

    /// Apply `op` to the byte range `[lo, hi)`.
    fn apply_range(&mut self, op: Operator, lo: usize, hi: usize, buf: &mut Buffer) -> Outcome {
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

    /// With soft wrap, `$`, `0` and `^` are screen-line motions (vim's `g$`/
    /// `g0`/`g^` are the line-wise forms), while `A` and `I` stay line-wise.
    #[test]
    fn dollar_and_zero_follow_the_display_line() {
        let mut b = Buffer::new("aaaa   bbbb cccc".to_string());
        b.set_display_segments(vec![vec![0, 11]]);
        let mut v = Vim::new();

        type_keys(&mut v, "$", &mut b);
        assert_eq!(b.cursor(), 10, "$ stops at the display line end");
        type_keys(&mut v, "0", &mut b);
        assert_eq!(b.cursor(), 0, "0 goes to the display line start");

        b.set_cursor(12);
        assert_eq!(
            v.handle_normal_key(Key::Char('I'), &mut b),
            Outcome::EnterInsert
        );
        assert_eq!(b.cursor(), 0, "I is line-wise: the row's first non-blank");

        b.set_cursor(0);
        assert_eq!(
            v.handle_normal_key(Key::Char('A'), &mut b),
            Outcome::EnterInsert
        );
        assert_eq!(b.cursor(), 16, "A is line-wise: the end of the whole row");

        b.set_cursor(12);
        type_keys(&mut v, "d$", &mut b);
        assert_eq!(b.text(), "aaaa   bbbb ", "d$ stops at the display line end");
    }

    /// `f`/`t`/`F`/`T` as motions and as operator arguments.
    /// `iw`/`aw` and `ip`/`ap`: the operator waits for the object's scope, then
    /// acts on the range under the cursor.
    #[test]
    fn text_objects() {
        let mut v = Vim::new();

        let mut b = buf(); // "hello world\nsecond line\nthird"
        type_keys(&mut v, "diw", &mut b);
        assert_eq!(b.text(), " world\nsecond line\nthird", "diw");

        let mut b = buf();
        type_keys(&mut v, "daw", &mut b);
        assert_eq!(b.text(), "world\nsecond line\nthird", "daw");

        let mut b = buf();
        b.set_cursor(6);
        type_keys(&mut v, "daw", &mut b);
        assert_eq!(
            b.text(),
            "hello\nsecond line\nthird",
            "daw falls back to the leading space at a line end"
        );

        let mut b = buf();
        b.set_cursor(12);
        type_keys(&mut v, "ci", &mut b);
        assert_eq!(
            v.handle_normal_key(Key::Char('w'), &mut b),
            Outcome::EnterInsert,
            "ciw leaves the cursor ready for typing"
        );
        assert_eq!(b.text(), "hello world\n line\nthird");

        let mut b = buf();
        type_keys(&mut v, "yiw", &mut b);
        assert_eq!(
            v.register.as_ref().unwrap().text,
            "hello",
            "a charwise yank of a word"
        );
        assert!(!v.register.as_ref().unwrap().linewise, "and not linewise");

        // A count before the object extends it over the next one.
        let mut b = buf();
        type_keys(&mut v, "d2aw", &mut b);
        assert_eq!(b.text(), "\nsecond line\nthird", "d2aw takes both words");

        let mut b = Buffer::new("one\ntwo\n\nthree\n\n\nfour".to_string());
        type_keys(&mut v, "cip", &mut b);
        assert_eq!(b.text(), "\n\nthree\n\n\nfour", "cip keeps the break");

        let mut b = Buffer::new("one\ntwo\n\nthree\n\n\nfour".to_string());
        type_keys(&mut v, "cap", &mut b);
        assert_eq!(b.text(), "three\n\n\nfour", "cap swallows the blanks");

        let mut b = Buffer::new("one\ntwo\n\nthree\n\n\nfour".to_string());
        b.set_cursor(9);
        type_keys(&mut v, "yip", &mut b);
        assert_eq!(v.register.as_ref().unwrap().text, "three");

        let mut b = buf();
        type_keys(&mut v, "dij", &mut b);
        assert_eq!(
            b.text(),
            "hello world\nsecond line\nthird",
            "an unknown scope cancels the operator"
        );
    }

    #[test]
    fn find_motions_and_operators() {
        let mut v = Vim::new();

        let mut b = buf(); // "hello world\nsecond line\nthird"
        type_keys(&mut v, "fo", &mut b);
        assert_eq!(b.cursor(), 4, "f lands on the character");
        type_keys(&mut v, "fo", &mut b);
        assert_eq!(b.cursor(), 7, "and the next one on the second press");
        type_keys(&mut v, "2fo", &mut b);
        assert_eq!(b.cursor(), 7, "no third o on the row, so nothing moves");

        let mut b = buf();
        type_keys(&mut v, "dfo", &mut b);
        assert_eq!(
            b.text(),
            " world\nsecond line\nthird",
            "dfo includes the found character"
        );

        let mut b = buf();
        type_keys(&mut v, "dto", &mut b);
        assert_eq!(
            b.text(),
            "lo world\nsecond line\nthird",
            "dto stops before the found character"
        );

        let mut b = buf();
        b.set_cursor(10);
        type_keys(&mut v, "dFw", &mut b);
        assert_eq!(
            b.text(),
            "hello d\nsecond line\nthird",
            "dFw deletes back to the match"
        );

        let mut b = buf();
        type_keys(&mut v, "dzq", &mut b);
        assert_eq!(
            b.text(),
            "hello world\nsecond line\nthird",
            "a find that fails cancels the operator"
        );
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
