//! Vim-style mode state machine, decoupled from any UI.
//!
//! The UI feeds printable keys to [`Vim::handle_normal_key`] while in normal
//! mode; in insert mode all text goes to the buffer directly and only
//! [`Vim::handle_insert_key`] is consulted.

use crate::buffer::Buffer;

mod motions;
mod operators;
mod tables;
mod visual;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    /// Characterwise visual selection (`v`).
    Visual,
    /// Linewise visual selection (`V`).
    VisualLine,
}

impl Mode {
    pub fn is_visual(self) -> bool {
        matches!(self, Mode::Visual | Mode::VisualLine)
    }
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
    /// `z` typed, waiting for a fold command (`za`, `zc`, `zo`, `zd`, `zM`,
    /// `zR`). A fold changes what is drawn rather than the text, so no count is
    /// carried along.
    Fold,
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
    /// `space f` was typed: the UI should open its quick-open picker. The engine
    /// has no idea what notes exist, let alone where they live, so this is a
    /// request rather than an edit.
    QuickOpen,
    /// `space s` was typed: the UI should search every note for what the user
    /// types next. A request rather than an edit, like `space f`: the engine has
    /// no idea what the vault holds.
    SearchVault,
    /// `K` was typed outside a table: vim's "look this up". Shale has no man
    /// pages, but the window's spell and grammar checker has something to say
    /// about the word under the cursor, and the engine has no idea that a checker
    /// is even running — so this is a request, like `space f`. Inside a table `K`
    /// stays the row motion.
    Hover,
}

/// The unnamed register, holding the last yank or delete.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Register {
    text: String,
    linewise: bool,
}

/// One step of a recorded change, replayed by `.`: a normal-mode key, or a run
/// of text typed in insert mode.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Cmd {
    Key(Key),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct Vim {
    mode: Mode,
    pending: Pending,
    /// Count typed so far, applied to the next motion or operator.
    count: Option<usize>,
    register: Option<Register>,
    /// The command being typed, recorded so `.` can replay it.
    command: Vec<Cmd>,
    /// The last change, as a replayable command.
    last_command: Option<Vec<Cmd>>,
    /// A command that dropped into insert mode is still being recorded.
    insert_session: bool,
    /// The key just typed was a plain `space` motion, so the next key may be one
    /// of Shale's leader chords (`f` quick-open, `s` vault search) rather than a
    /// find. The space motion itself is applied when `space` is typed — deferring
    /// it would make a lone `space` look dead — so a chord is recognised one key
    /// late.
    after_space: bool,
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
            command: Vec::new(),
            last_command: None,
            insert_session: false,
            after_space: false,
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
            Pending::Fold => Some('z'),
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
        self.after_space = false;
    }

    pub fn enter_normal(&mut self) {
        self.mode = Mode::Normal;
        self.pending = Pending::None;
        self.count = None;
        self.after_space = false;
        // Leaving insert mode ends the command that started it.
        // Leaving insert mode ends the insert session that was being recorded.
        if self.insert_session && !self.command.is_empty() {
            self.last_command = Some(std::mem::take(&mut self.command));
        }
        self.command.clear();
        self.insert_session = false;
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

    /// Handle a key press while in normal or visual mode.
    pub fn handle_normal_key(&mut self, key: Key, buf: &mut Buffer) -> Outcome {
        // The `space` leader's chords, one key after the motion that started
        // them. Only a plain space motion arms them: `d space` or `f ` never do.
        let after_space = std::mem::take(&mut self.after_space);
        let chord = match (after_space, key) {
            (true, Key::Char('f')) => Some(Outcome::QuickOpen),
            (true, Key::Char('s')) => Some(Outcome::SearchVault),
            _ => None,
        };
        if let Some(outcome) = chord.filter(|_| !self.mode.is_visual()) {
            self.pending = Pending::None;
            self.count = None;
            self.command.clear();
            return outcome;
        }
        if self.mode.is_visual() {
            return self.handle_visual_key(key, buf);
        }
        if key == Key::Char('.') && !self.waiting_for_literal() {
            return self.repeat_last(buf);
        }
        let before = buf.text().to_string();
        self.command.push(Cmd::Key(key));
        let outcome = self.handle_motion_key(key, buf);
        // The command is over once nothing is pending: keep it for `.` if it
        // changed the buffer. One that drops into insert mode stays open until
        // the insert session ends, so the typed text becomes part of it.
        if self.pending == Pending::None && self.count.is_none() && outcome != Outcome::EnterInsert
        {
            if buf.text() != before {
                self.last_command = Some(std::mem::take(&mut self.command));
            } else {
                self.command.clear();
            }
        } else if outcome == Outcome::EnterInsert {
            self.insert_session = true;
        }
        outcome
    }

    /// Replay the last change (vim's `.`). The recorded keys never leave the
    /// engine in insert mode, because the typed text is part of the recording.
    pub fn repeat_last(&mut self, buf: &mut Buffer) -> Outcome {
        let Some(cmd) = self.last_command.clone() else {
            return Outcome::Unhandled;
        };
        self.count = None;
        self.pending = Pending::None;
        for step in &cmd {
            match step {
                Cmd::Key(key) => {
                    self.handle_motion_key(*key, buf);
                }
                Cmd::Text(text) => buf.insert(text),
            }
        }
        self.count = None;
        self.pending = Pending::None;
        Outcome::Stay
    }

    /// The UI reports text typed in insert mode so `.` can replay it.
    pub fn note_inserted(&mut self, text: &str) {
        if self.command.is_empty() || text.is_empty() {
            return;
        }
        match self.command.last_mut() {
            Some(Cmd::Text(last)) => last.push_str(text),
            _ => self.command.push(Cmd::Text(text.to_string())),
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
    /// A printable key typed with alt held, which the keymap delivers as its own
    /// action. Used for the table row and column moves.
    AltChar(char),
    Escape,
}

#[cfg(test)]
mod tests;
