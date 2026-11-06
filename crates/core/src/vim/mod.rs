//! Vim-style mode state machine, decoupled from any UI.
//!
//! The UI feeds printable keys to [`Vim::handle_normal_key`] while in normal
//! mode; in insert mode all text goes to the buffer directly and only
//! [`Vim::handle_insert_key`] is consulted.

use std::collections::HashMap;

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

/// The selection that was last active, so `gv` can put it back: the anchor and
/// head as they were — which end the cursor was on included — and whether it took
/// whole lines.
#[derive(Debug, Clone, Copy)]
pub struct VisualRecord {
    anchor: usize,
    head: usize,
    linewise: bool,
}

/// What the last `f`/`t`/`F`/`T` searched for, so `;` and `,` can do it again.
#[derive(Debug, Clone, Copy)]
pub struct FindRecord {
    c: char,
    forward: bool,
    inclusive: bool,
}

impl FindRecord {
    /// The repeat of a find: `;` keeps the direction, `,` reverses it. The
    /// inclusivity belongs to the original command, so a `;` after `t` is still
    /// exclusive and lands where `t` would have.
    fn replay(self, backward: bool) -> (char, bool, bool) {
        let forward = self.forward != backward;
        (self.c, forward, self.inclusive)
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
    /// `"` typed, waiting for the register name that follows it.
    Register { count: usize },
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
    /// `r` typed, waiting for the character to replace with. The count is the
    /// number of characters to replace (`3rx` is `rrr`), which is why it is carried
    /// here rather than left in `Vim::count`.
    Replace { count: usize },
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
    /// `gx` or `gd` was typed: the UI should follow the link under the cursor.
    FollowLink,
    /// `gD` was typed: follow the link under the cursor, making the note first when
    /// the target resolves to nothing. A request rather than an edit, like
    /// `space f` — the engine has no idea what the vault holds.
    CreateLink,
    /// `space f` was typed: the UI should open its quick-open picker. The engine
    /// has no idea what notes exist, let alone where they live, so this is a
    /// request rather than an edit.
    QuickOpen,
    /// `space s` was typed: the UI should search every note for what the user
    /// types next. A request rather than an edit, like `space f`: the engine has
    /// no idea what the vault holds.
    SearchVault,
    /// `space r` was typed: the UI should show the markdown source. The view is
    /// the window's answer, not the engine's — it changes what the frame draws and
    /// whether the buffer takes edits, neither of which the engine is told about.
    ViewRaw,
    /// `space l` was typed: back to Shale's live view. It is a chord rather than
    /// only `:live` because of preview, which stops the note taking typing: the
    /// way out has to be shorter than the way in.
    ViewLive,
    /// `space p` was typed: fully rendered, and read-only.
    ViewPreview,
    /// `space d` was typed: turn focus mode around. Like the view chords this is
    /// the window's answer, not the engine's — it changes what the frame draws and
    /// the engine is not told about it — and it is a toggle rather than an on and
    /// an off because the way out has to be the same two keys as the way in.
    ToggleFocus,
    /// `K` was typed outside a table: vim's "look this up". Shale has no man
    /// pages, but the window's spell and grammar checker has something to say
    /// about the word under the cursor, and the engine has no idea that a checker
    /// is even running — so this is a request, like `space f`. Inside a table `K`
    /// stays the row motion.
    Hover,
    /// `space y` was typed: the UI should put what the user has to give — the
    /// selection, or the last yank — on the **system** clipboard. The engine's own
    /// register stays inside the engine; the clipboard is the door to other
    /// programs, and only the window can open it.
    YankClipboard,
    /// `]a` was typed: the UI should hop the agent-review marker to the next pending
    /// hunk. Like the view and clipboard chords this is a request, not an edit — the
    /// engine has no idea that a queue of proposals exists, let alone where it is.
    ReviewNext,
    /// `[a` was typed: the marker to the previous pending hunk.
    ReviewPrev,
    /// `g t` was typed: the UI should go to the next tab. The engine has no idea the
    /// window has tabs — it has a buffer, and a buffer does not know what else is
    /// open beside it — so this is a request, like `space f` and `]a`. vim spells the
    /// same thing `:tabnext`.
    TabNext,
    /// `g T` was typed: the previous tab.
    TabPrev,
    /// `space g` was typed: the UI should show the vault's link graph (roadmap P3.4).
    /// A request rather than an edit, like the other leader chords: the engine has a
    /// buffer, and a buffer does not know that the notes link to each other.
    OpenGraph,
    /// `space e` was typed: put the file tree away, or bring it back. Like focus mode
    /// this is the window's answer rather than the engine's — it changes what the frame
    /// draws, and the note takes the width the tree had — and it is a toggle rather than
    /// an on and an off because the way out has to be the same two keys as the way in.
    /// `e` is what the expanded vim file browsers answer with `<leader>e`, which is the
    /// shape a user reaching for this is reaching for.
    ToggleTree,
}

/// What an operation did to the text it took, which decides which of vim's
/// automatic registers it lands in: a yank also fills `0`, a delete also pushes
/// the recent-delete list, a change neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Yank,
    Delete,
    Change,
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
    /// The last `f`/`t`/`F`/`T` that was typed, which is what `;` repeats and `,`
    /// repeats backwards. vim remembers the command rather than a search pattern,
    /// so a `;` after `tw` re-runs an exclusive find from where the cursor already
    /// sits — which is why it looks like it does nothing, and why the quirk is
    /// worth keeping rather than "fixing".
    last_find: Option<FindRecord>,
    /// The selection `gv` would put back.
    last_visual: Option<VisualRecord>,
    /// Named and numbered registers, keyed by vim's name: `a`-`z` are the named
    /// ones, `0` holds the last yank, `1`-`9` the recent deletes (newest first),
    /// `_` is the black hole, which swallows what it is given. The unnamed
    /// register is `self.register` above.
    registers: HashMap<char, Register>,
    /// The register a `"` prefix chose for the next operator or paste.
    pending_register: Option<char>,
    /// The key just typed was a plain `space` motion, so the next key may be one
    /// of Shale's leader chords (`f` quick-open, `s` vault search, and the three
    /// views `r` / `l` / `p`) rather than a find. The space motion itself is
    /// applied when `space` is typed — deferring it would make a lone `space` look
    /// dead — so a chord is recognised one key late.
    after_space: bool,
    /// The key just typed was a plain `]` or `[`, so the next key may be the `a` of
    /// Shale's review-marker chords (`]a` next pending hunk, `[a` previous). Held as
    /// `Some(true)` for `]` and `Some(false)` for `[`.
    ///
    /// These two keys are bound as actions rather than as a two-stroke keymap chord
    /// on purpose: gpui holds the first key of a chord as pending input, which would
    /// keep a literal `[` from reaching the engine as the argument of `f`, `t` or an
    /// operator. As an action the engine sees every `]` and `[` and decides, which is
    /// what keeps `f[` and `di]` literal and `]a` a command.
    after_bracket: Option<bool>,
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
            last_find: None,
            last_visual: None,
            registers: HashMap::new(),
            pending_register: None,
            after_space: false,
            after_bracket: None,
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
            Pending::Replace { .. } => Some('r'),
            _ => None,
        }
    }

    /// Whether the engine is waiting for a literal character argument (`f`/`t`
    /// and friends). Those keys are not bound in the keymap, so the UI feeds
    /// them in through the input handler instead.
    pub fn waiting_for_literal(&self) -> bool {
        matches!(
            self.pending,
            Pending::Find { .. }
            | Pending::TextObject { .. }
            | Pending::Replace { .. }
            | Pending::Register { .. }
        )
    }

    /// Whether the engine is in the middle of a half-typed command: a leader chord
    /// (`space …`), `g …`, an operator waiting for its motion, a find or text object
    /// waiting for its argument. The UI asks this before treating a `p` as a paste,
    /// because `space p` is the preview chord and `dip` is the paragraph object.
    pub fn awaiting_chord(&self) -> bool {
        self.after_space || self.after_bracket.is_some() || !matches!(self.pending, Pending::None)
    }

    /// Whether `c` is a name vim accepts after `"`.
    fn register_name(c: char) -> bool {
        matches!(c, 'a'..='z' | '0'..='9' | '_' | '-' | '"')
    }

    /// Snapshot the unnamed register before an operation. A `"`-chosen register
    /// takes the text without the unnamed register being clobbered, which is what
    /// vim does, and this is what puts it back.
    fn begin_register_op(&self) -> Option<Register> {
        self.register.clone()
    }

    /// File what the operation just left in the unnamed register. The rules here
    /// were read out of vim's own `:reg` output, and two of them are not what you
    /// would guess:
    ///
    /// - A `"`-named register takes the text *and the unnamed register does too*
    ///   (`"ayw` then `p` pastes it), but it does not touch the automatic ones.
    /// - Only a yank with no target fills `0`.
    /// - A delete fills the recent-delete list `1`-`9` only when it took a whole
    ///   line or crossed one. A small charwise delete — `dw`, `d$` — goes to the
    ///   small-delete register `"-` instead, and the numbered ones stay put.
    fn commit_register_op(&mut self, kind: OpKind, before: Option<Register>) {
        let target = self.pending_register.take();
        let fresh = match self.register.clone() {
            Some(fresh) => fresh,
            None => return,
        };
        // An operation that was cancelled before it took anything leaves the
        // unnamed register holding what it already held; filing that would
        // overwrite the target with stale text. The check is against the target,
        // not just the unnamed register: `"ayw` then `"bw` on the same word is a
        // real yank into a register that has never held it.
        let unchanged = Some(&fresh) == before.as_ref()
            && target.is_none_or(|name| self.registers.get(&name) == Some(&fresh));
        if unchanged {
            return;
        }
        // The black hole swallows the text and leaves the unnamed register as it
        // was. `""` names the unnamed register itself, which is what happens with
        // no prefix at all.
        if target == Some('_') {
            self.register = before;
            return;
        }
        let target = target.filter(|name| *name != '"');
        if let Some(name) = target {
            self.registers.insert(name, fresh.clone());
        }
        if kind == OpKind::Yank && target.is_none() {
            self.registers.insert('0', fresh.clone());
        }
        if kind == OpKind::Delete {
            let big = fresh.linewise || fresh.text.contains('\n');
            if big {
                for slot in (2..=9).rev() {
                    let from = char::from_digit(slot - 1, 10).unwrap();
                    let to = char::from_digit(slot, 10).unwrap();
                    if let Some(prev) = self.registers.get(&from) {
                        let prev = prev.clone();
                        self.registers.insert(to, prev);
                    }
                }
                self.registers.insert('1', fresh);
            } else {
                self.registers.insert('-', fresh);
            }
        }
    }

    /// The register a paste would take: the one a `"` prefix chose, or the unnamed
    /// one.
    fn paste_register(&mut self) -> Option<Register> {
        match self.pending_register.take() {
            Some(name) => self.registers.get(&name).cloned(),
            None => self.register.clone(),
        }
    }

    /// Every register the engine holds, for tests and for a `:reg`-style command.
    pub fn register_dump(&self) -> Vec<(char, String)> {
        let mut rows: Vec<(char, String)> = self
            .registers
            .iter()
            .map(|(name, reg)| (*name, reg.text.clone()))
            .collect();
        rows.sort();
        rows
    }

    /// The unnamed register's text, if anything has been yanked or deleted. The UI
    /// reads this to answer `space y` / `:yank`: the engine has no clipboard, so
    /// the window is what carries the text out to other programs.
    pub fn register_text(&self) -> Option<&str> {
        self.register.as_ref().map(|reg| reg.text.as_str())
    }

    /// Loads text into the unnamed register, which is how the system clipboard
    /// becomes something `p`, `P` and `.` can use. Text that ends with a newline
    /// is linewise — the shape a whole-line copy from another program arrives as —
    /// so it lands on its own line rather than in the middle of one.
    pub fn set_register(&mut self, text: &str) {
        self.register = Some(Register {
            text: text.to_string(),
            linewise: text.ends_with('\n'),
        });
    }

    pub fn enter_insert(&mut self) {
        self.mode = Mode::Insert;
        self.pending = Pending::None;
        self.count = None;
        self.after_space = false;
        self.after_bracket = None;
    }

    pub fn enter_normal(&mut self) {
        self.mode = Mode::Normal;
        self.pending = Pending::None;
        self.count = None;
        self.after_space = false;
        self.after_bracket = None;
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
            // The note's three views. `space l` is the one that matters most: a
            // note in preview will not type, so the way out cannot need the command
            // line.
            (true, Key::Char('r')) => Some(Outcome::ViewRaw),
            (true, Key::Char('l')) => Some(Outcome::ViewLive),
            (true, Key::Char('p')) => Some(Outcome::ViewPreview),
            // The clipboard, not the register: `y` alone is vim's yank and stays
            // the engine's business, so the leader is what makes it the outward
            // copy.
            (true, Key::Char('y')) => Some(Outcome::YankClipboard),
            // Focus mode, which dims everything but the block the cursor is in.
            // `d` because the mode is the note going quiet, and because every
            // letter the view chords did not take is worth spending once.
            (true, Key::Char('d')) => Some(Outcome::ToggleFocus),
            // The vault's graph. `g` is the letter the word "graph" starts with, and
            // the only leader letter left that no view or clipboard chord has taken.
            (true, Key::Char('g')) => Some(Outcome::OpenGraph),
            // The file tree. nvim-tree and NERDTree both answer `<leader>e` with this,
            // and `e` is the last leader letter that no view, clipboard or graph chord
            // has taken.
            (true, Key::Char('e')) => Some(Outcome::ToggleTree),
            _ => None,
        };
        if let Some(outcome) = chord.filter(|_| !self.mode.is_visual()) {
            self.pending = Pending::None;
            self.count = None;
            self.command.clear();
            return outcome;
        }
        // `]a` / `[a`: the agent-review marker's next and previous. The bracket was a
        // plain one, so this is the only key that can finish the chord; anything else
        // cancels it and does nothing, which is what vim does with a `]` command it
        // has no use for.
        if let Some(forward) = self.after_bracket.take() {
            if self.mode.is_visual() {
                return Outcome::Stay;
            }
            self.command.clear();
            return match (forward, key) {
                (true, Key::Char('a')) => Outcome::ReviewNext,
                (false, Key::Char('a')) => Outcome::ReviewPrev,
                _ => Outcome::Stay,
            };
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
