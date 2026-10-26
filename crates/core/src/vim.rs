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

/// A partially-typed multi-key normal-mode command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Pending {
    #[default]
    None,
    /// `d` typed, waiting for the motion/operator.
    Delete,
    /// `g` typed, waiting for a second `g`.
    Goto,
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
}

#[derive(Debug, Clone)]
pub struct Vim {
    mode: Mode,
    pending: Pending,
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
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn enter_insert(&mut self) {
        self.mode = Mode::Insert;
        self.pending = Pending::None;
    }

    pub fn enter_normal(&mut self) {
        self.mode = Mode::Normal;
        self.pending = Pending::None;
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
        match self.pending {
            Pending::Delete => {
                self.pending = Pending::None;
                match key {
                    Key::Char('d') => {
                        buf.delete_line();
                        return Outcome::Stay;
                    }
                    _ => return Outcome::Unhandled,
                }
            }
            Pending::Goto => {
                self.pending = Pending::None;
                if let Key::Char('g') = key {
                    buf.buffer_start();
                    return Outcome::Stay;
                }
                return Outcome::Unhandled;
            }
            Pending::None => {}
        }

        match key {
            // Motions
            Key::Char('h') => buf.move_left(),
            Key::Char('j') => buf.move_down(),
            Key::Char('k') => buf.move_up(),
            Key::Char('l') => buf.move_right_stay_on_line(),
            Key::Char('w') => buf.word_forward(),
            Key::Char('b') => buf.word_backward(),
            Key::Char('e') => buf.word_end(),
            Key::Char('0') => buf.line_start_motion(),
            Key::Char('^') => buf.first_non_blank(),
            Key::Char('$') => buf.line_end_motion(),
            Key::Char('G') => buf.buffer_end(),
            Key::Char('g') => {
                self.pending = Pending::Goto;
                return Outcome::Stay;
            }
            // Operators
            Key::Char('d') => {
                self.pending = Pending::Delete;
                return Outcome::Stay;
            }
            Key::Char('x') => buf.delete_char(),
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
                buf.line_end_motion();
                return Outcome::EnterInsert;
            }
            Key::Char('o') => {
                buf.open_line_below();
                return Outcome::EnterInsert;
            }
            Key::Char('O') => {
                buf.open_line_above();
                return Outcome::EnterInsert;
            }
            Key::Escape => buf.clear_selection(),
            _ => return Outcome::Unhandled,
        }
        Outcome::Stay
    }
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

    #[test]
    fn normal_mode_motions_and_operators() {
        let mut vim = Vim::new();
        let mut b = buf();
        assert_eq!(vim.mode(), Mode::Normal);

        vim.handle_normal_key(Key::Char('j'), &mut b);
        assert_eq!(b.cursor(), 12);
        vim.handle_normal_key(Key::Char('$'), &mut b);
        assert_eq!(b.cursor(), 23);
        vim.handle_normal_key(Key::Char('k'), &mut b);
        assert_eq!(b.cursor(), 11);

        // dd deletes the line.
        vim.handle_normal_key(Key::Char('d'), &mut b);
        vim.handle_normal_key(Key::Char('d'), &mut b);
        assert_eq!(b.text(), "second line\nthird");
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
    fn a_and_A_position_cursor() {
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
}
