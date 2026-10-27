//! Vim-style mode state machine, decoupled from any UI.
//!
//! The UI feeds printable keys to [`Vim::handle_normal_key`] while in normal
//! mode; in insert mode all text goes to the buffer directly and only
//! [`Vim::handle_insert_key`] is consulted.

use crate::buffer::Buffer;
use crate::table::{self, Cell, Located, Table};

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

    /// The normal-mode state machine: motions, operators and insert transitions.
    fn handle_motion_key(&mut self, key: Key, buf: &mut Buffer) -> Outcome {
        // The table under the cursor, if any. Parsed per key: the scan is cheap
        // (it bails out at once when the text has no pipe) and every table edit
        // below invalidates it anyway.
        let tables = table::tables(buf.text());
        let located = table::locate(&tables, buf.cursor());

        // A digit extends the count. A bare `0` (with no count in progress) is
        // the line-start motion instead. A digit that is the argument of a
        // half-typed command (`f0`) is a literal, not a count.
        if let Key::Char(c @ '0'..='9') = key {
            if !self.waiting_for_literal() {
                if c == '0' && self.count.is_none() && self.pending == Pending::None {
                    self.zero_motion(buf);
                    return Outcome::Stay;
                }
                self.count = Some(self.count.unwrap_or(0) * 10 + (c as usize - '0' as usize));
                return Outcome::Stay;
            }
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
            // Inside a cell, a find that would land outside it counts as "not
            // found": the cursor stays put and an operator is cancelled.
            if located.is_some_and(|loc| !loc.cell.contains(buf.cursor())) {
                buf.set_cursor(start);
                return Outcome::Stay;
            }
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
                // Bare `iw`/`ip`: a motion. In visual mode it *is* the
                // selection; in normal mode it just moves the cursor.
                if self.mode.is_visual() {
                    let head = if hi > lo {
                        buf.prev_char_boundary(hi)
                    } else {
                        lo
                    };
                    buf.set_selection(lo, head);
                } else {
                    buf.set_cursor(lo);
                }
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
        // `alt+hjkl` moves a whole row or column of the table the cursor is in.
        // Outside a table the keys do nothing, so they stay unhandled.
        if let Key::AltChar(c) = key {
            let Some(loc) = located else {
                return Outcome::Unhandled;
            };
            let Some(table) = tables.get(loc.table) else {
                return Outcome::Unhandled;
            };
            let count = count as isize;
            match c {
                'h' => table::move_column(buf, table, loc.row, loc.column, -count),
                'l' => table::move_column(buf, table, loc.row, loc.column, count),
                'k' => table::move_row(buf, table, loc.row, loc.column, -count),
                'j' => table::move_row(buf, table, loc.row, loc.column, count),
                _ => return Outcome::Unhandled,
            }
            return Outcome::Stay;
        }
        // Inside a cell the table commands come first: `H`/`L` jump columns,
        // `j`/`k`/`J`/`K` step through cells, and `o`/`I`/`A` act on the cell
        // rather than the whole row.
        if let Some(loc) = located {
            if let Some(outcome) = self.table_command(key, count, &tables, loc, buf) {
                return outcome;
            }
        }
        // The motions that follow are the ordinary (non-table) ones; a table row
        // adjusts where they come to rest below.
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
            // Visual mode entry
            Key::Char('v') => {
                self.enter_visual(false, buf);
                return Outcome::Stay;
            }
            Key::Char('V') => {
                self.enter_visual(true, buf);
                return Outcome::Stay;
            }
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
        // Inside a cell the ordinary motions stay on the table's content: one
        // that would come to rest on padding, a pipe or outside the row steps
        // into the neighbouring cell instead. The column-edge motions stop at the
        // cell's content edges rather than the line's.
        if let Some(loc) = located {
            let cell = loc.cell;
            match key {
                Key::Char('h' | 'l' | ' ' | 'w' | 'b' | 'e') => {
                    if let Some(table) = tables.get(loc.table) {
                        snap_cell_motion(table, loc, key, buf);
                    }
                }
                Key::Char('0') => buf.set_cursor(cell.cursor()),
                Key::Char('^') => buf.set_cursor(cell.cursor()),
                Key::Char('$') => {
                    buf.set_cursor(if cell.start < cell.end {
                        buf.prev_char_boundary(cell.end)
                    } else {
                        cell.nav_start()
                    });
                }
                _ => {}
            }
        }
        Outcome::Stay
    }

    /// The `0` motion: the start of the screen line, or the first content
    /// character of the cell when the cursor is inside a table (its padding is
    /// not a place a cursor should come to rest).
    fn zero_motion(&self, buf: &mut Buffer) {
        match table::locate(&table::tables(buf.text()), buf.cursor()) {
            Some(loc) => buf.set_cursor(loc.cell.cursor()),
            None => buf.line_start_motion(),
        }
    }

    /// Table commands, consulted before the ordinary motions. Returns `None`
    /// when the key is not a table command, so its normal binding applies.
    ///
    /// `tables` and `loc` describe the buffer as it was when the key arrived; the
    /// edit helpers re-parse it and reposition the cursor themselves.
    fn table_command(
        &mut self,
        key: Key,
        count: usize,
        tables: &[Table],
        loc: Located,
        buf: &mut Buffer,
    ) -> Option<Outcome> {
        let Key::Char(c) = key else {
            return None;
        };
        let table = tables.get(loc.table)?;
        let count = count as isize;
        match c {
            // Cell-to-cell motions. `h`/`l` walk the cell's content and step into
            // the neighbouring cell at its edge; the shifted forms jump a whole
            // column. Plain `j`/`k` step through cells, skipping the delimiter
            // row, and walk out of the table at its top or bottom.
            'H' => step_cell(table, loc, 0, -count, buf),
            'L' => step_cell(table, loc, 0, count, buf),
            'K' | 'k' => match step_cell_rows(table, loc, -count, buf) {
                // Walking off the top of the table leaves it: whatever part of the
                // count the table did not use becomes the ordinary up motion.
                Some(leftover) if leftover > 0 => {
                    repeat(leftover, || buf.move_up());
                    step_past_rule(table, buf, false);
                }
                Some(_) => {}
                None => buf.move_up(),
            },
            'J' | 'j' => match step_cell_rows(table, loc, count, buf) {
                Some(leftover) if leftover > 0 => {
                    repeat(leftover, || buf.move_down());
                    step_past_rule(table, buf, true);
                }
                Some(_) => {}
                None => buf.move_down(),
            },
            'D' => {
                // Each delete re-locates the cursor, which can shift the table's
                // indices, so the table is re-read for every column.
                for _ in 0..count.max(1) {
                    let tables = table::tables(buf.text());
                    let Some(loc) = table::locate(&tables, buf.cursor()) else {
                        break;
                    };
                    table::delete_column(buf, &tables[loc.table], loc.row, loc.column);
                }
            }
            '|' => table::insert_column_after(buf, table, loc.row, loc.column),
            '\\' => table::insert_column_before(buf, table, loc.row, loc.column),
            'I' => {
                // Shift+I empties the cell and puts the cursor in it; outside a
                // table `I` keeps its vim meaning of row-first-non-blank.
                buf.delete_range(loc.cell.start, loc.cell.end);
                buf.set_cursor(loc.cell.start);
                return Some(Outcome::EnterInsert);
            }
            'A' => {
                // Append at the end of the cell's content rather than the row's.
                buf.set_cursor(loc.cell.end);
                return Some(Outcome::EnterInsert);
            }
            'o' => {
                table::insert_row_after(buf, table, loc.row, loc.column);
                return Some(Outcome::EnterInsert);
            }
            'O' => {
                table::insert_row_before(buf, table, loc.row, loc.column);
                return Some(Outcome::EnterInsert);
            }
            _ => return None,
        }
        Some(Outcome::Stay)
    }

    /// Visual mode: motions extend the selection, operators act on it.
    fn handle_visual_key(&mut self, key: Key, buf: &mut Buffer) -> Outcome {
        // A digit extends the count, unless it is the argument of a half-typed
        // command (`f0`).
        if let Key::Char(c @ '0'..='9') = key {
            if !self.waiting_for_literal() {
                if c == '0' && self.count.is_none() {
                    self.zero_motion(buf);
                    return self.reanchor(buf);
                }
                self.count = Some(self.count.unwrap_or(0) * 10 + (c as usize - '0' as usize));
                return Outcome::Stay;
            }
        }

        // A half-typed command takes the next key as its argument, whatever it
        // is: `fv` finds a literal `v` even though `v` is a bound key.
        if self.waiting_for_literal() {
            return self.visual_motion(key, buf);
        }

        match key {
            Key::Escape => {
                self.exit_visual(buf);
                Outcome::Stay
            }
            // `v` switches charwise <-> linewise; typing the form the mode
            // already has ends the selection.
            Key::Char('v') | Key::Char('V') => {
                let line_wise = key == Key::Char('V');
                if self.mode == Mode::Visual && line_wise {
                    self.enter_visual(true, buf);
                } else if self.mode == Mode::VisualLine && !line_wise {
                    self.enter_visual(false, buf);
                } else {
                    self.exit_visual(buf);
                }
                Outcome::Stay
            }
            // `o` jumps to the other end of the selection.
            Key::Char('o') | Key::Char('O') => {
                if let Some((a, h)) = buf.selection() {
                    buf.set_selection(h, a);
                }
                Outcome::Stay
            }
            // `i`/`a` start a text object, which extends the selection.
            Key::Char('i') | Key::Char('a') => {
                let count = self.count.take().unwrap_or(1);
                self.pending = Pending::TextObject {
                    op: None,
                    count,
                    around: key == Key::Char('a'),
                };
                Outcome::Stay
            }
            // `I`/`A` insert at the first non-blank / last character of the
            // selection's near and far lines.
            Key::Char('I') | Key::Char('A') => {
                let Some((lo, hi)) = self.selection_range(buf) else {
                    return Outcome::Unhandled;
                };
                self.mode = Mode::Normal;
                buf.clear_selection();
                if key == Key::Char('I') {
                    buf.set_cursor(lo);
                    buf.row_first_non_blank();
                } else {
                    buf.set_cursor(hi.saturating_sub(1));
                    buf.row_last_char_motion();
                    buf.move_right();
                }
                Outcome::EnterInsert
            }
            // Operators apply to the selection itself.
            Key::Char('d')
            | Key::Char('x')
            | Key::Char('c')
            | Key::Char('s')
            | Key::Char('y')
            | Key::Char('p') => {
                let Some((lo, hi)) = self.selection_range(buf) else {
                    return Outcome::Unhandled;
                };
                let linewise = self.mode == Mode::VisualLine;
                let text = buf.text()[lo..hi].to_string();
                self.exit_visual(buf);
                match key {
                    Key::Char('y') => self.register = Some(Register { text, linewise }),
                    Key::Char('p') => {
                        buf.delete_range(lo, hi);
                        let reg = self.register.clone().map(|r| r.text).unwrap_or_default();
                        buf.insert_at(lo, &reg);
                    }
                    Key::Char('c') | Key::Char('s') => {
                        self.register = Some(Register { text, linewise });
                        buf.delete_range(lo, hi);
                        return Outcome::EnterInsert;
                    }
                    _ => {
                        self.register = Some(Register { text, linewise });
                        buf.delete_range(lo, hi);
                    }
                }
                Outcome::Stay
            }
            // Everything else is a motion.
            _ => self.visual_motion(key, buf),
        }
    }

    /// Runs a motion while in visual mode. Motions measure from the cursor, so
    /// the selection is cleared first and then re-anchored where it was.
    fn visual_motion(&mut self, key: Key, buf: &mut Buffer) -> Outcome {
        let anchor = buf.selection().map(|(a, _)| a);
        buf.clear_selection();
        let outcome = self.handle_motion_key(key, buf);
        // A text object sets its own selection; a plain motion only moves the
        // head.
        if buf.selection().is_none() {
            if let Some(anchor) = anchor {
                buf.set_selection(anchor, buf.cursor());
            }
        }
        outcome
    }

    /// Enter visual mode: `v` starts the selection at the cursor, `V` takes
    /// whole lines.
    fn enter_visual(&mut self, line_wise: bool, buf: &mut Buffer) {
        self.pending = Pending::None;
        self.count = None;
        self.mode = if line_wise {
            Mode::VisualLine
        } else {
            Mode::Visual
        };
        let cursor = buf.cursor();
        if line_wise {
            let row = buf.row_col(cursor).0;
            buf.set_selection(buf.line_start(row), buf.line_end(row));
        } else {
            buf.set_selection(cursor, cursor);
        }
    }

    /// Leave visual mode, leaving the cursor on the selection's far end.
    fn exit_visual(&mut self, buf: &mut Buffer) {
        self.mode = Mode::Normal;
        self.pending = Pending::None;
        self.count = None;
        buf.clear_selection();
    }

    /// The byte range `[lo, hi)` a visual selection covers. Charwise visual is
    /// *inclusive* of the character under the cursor, so the end sits one
    /// character past the head; linewise takes whole lines, line breaks
    /// included.
    pub fn selection_range(&self, buf: &Buffer) -> Option<(usize, usize)> {
        let (anchor, head) = buf.selection()?;
        let (lo, hi) = (anchor.min(head), anchor.max(head));
        if self.mode == Mode::VisualLine {
            let end = buf.line_end(buf.row_col(hi).0);
            let end = if end < buf.text().len() {
                buf.next_char_boundary(end)
            } else {
                end
            };
            Some((buf.line_start(buf.row_col(lo).0), end))
        } else {
            let end = if hi < buf.text().len() {
                buf.next_char_boundary(hi)
            } else {
                buf.text().len()
            };
            Some((lo, end))
        }
    }

    /// After a motion in visual mode the anchor stays put and the head follows
    /// the cursor.
    fn reanchor(&self, buf: &mut Buffer) -> Outcome {
        if let Some((anchor, _)) = buf.selection() {
            buf.set_selection(anchor, buf.cursor());
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
        // Where the cursor sits in a table, if anywhere: a linewise delete
        // removes rows and must leave the cursor in the cell that took their
        // place, and a charwise motion must not sweep out of the cell.
        let located = table::locate(&table::tables(buf.text()), buf.cursor());
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
                Operator::Delete => {
                    buf.delete_line(lines);
                    // `dd` on a table row removes the row; the cell that moved
                    // up into its place becomes the cursor's cell.
                    if let Some(loc) = located {
                        if let Some(offset) =
                            table::cursor_for(buf, loc.first_row, loc.row, loc.column)
                        {
                            buf.set_cursor(offset);
                        }
                    }
                }
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
        // `$` and `e` are inclusive motions: they land *on* a character, which
        // the operator must include. `w` is exclusive, so it stops at the start
        // of the next word and that character is left alone.
        if matches!(key, Key::Char('$') | Key::Char('e')) && end < buf.text().len() {
            end = buf.next_char_boundary(end);
        }
        // Inside a cell the sweep stops at the cell's content edge, so `dw` at
        // the end of a cell deletes the rest of that cell and not the next one.
        if let Some(loc) = located {
            end = end.clamp(loc.cell.nav_start(), loc.cell.end);
        }
        let (lo, hi) = (start.min(end), start.max(end));
        self.apply_range(op, lo, hi, buf)
    }

    /// The range of the text object with the given scope under `offset`.
    fn object_range(buf: &Buffer, offset: usize, scope: char, inner: bool) -> (usize, usize) {
        match scope {
            'p' => buf.paragraph_object(offset, inner),
            _ => buf.word_object(offset, inner),
        }
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

/// Moves the cursor to the cell `d_row` rows and `d_col` columns away, clamped
/// inside the table, landing on its first content character. Clamping is what
/// keeps the cursor from jumping to another table or wandering off the grid when
/// it is already at an edge. Used for the column jumps (`H`/`L`); the vertical
/// motions go through [`step_cell_rows`], which can leave the table.
fn step_cell(table: &Table, loc: Located, d_row: isize, d_col: isize, buf: &mut Buffer) {
    let last_row = table.rows.len().saturating_sub(1) as isize;
    let last_col = table.columns.saturating_sub(1) as isize;
    let row = (loc.row as isize + d_row).clamp(0, last_row) as usize;
    let column = (loc.column as isize + d_col).clamp(0, last_col) as usize;
    if let Some(cell) = table.cell_clamped(row, column) {
        let at = cell.cursor();
        buf.set_cursor(at);
    }
}

/// The vertical cell motion: `delta` rows down (or up, when negative) in the
/// same column, skipping the delimiter row, landing on the cell's first content
/// character. Returns the part of the count the table could not use — the steps
/// left over after walking off its top or bottom, which the caller spends on the
/// ordinary line motion so the cursor escapes the table — and `Some(0)` when the
/// motion stayed inside. `None` means the cursor was not on a navigable row at
/// all, so the caller should just use the ordinary motion.
fn step_cell_rows(table: &Table, loc: Located, delta: isize, buf: &mut Buffer) -> Option<usize> {
    let rows: Vec<usize> = table.navigable_rows().collect();
    let pos = rows.iter().position(|r| *r == loc.row)? as isize;
    let target = pos + delta;
    // Walking off the table comes to rest on its edge first, so the steps that
    // are left over continue from there instead of re-crossing the grid.
    let (index, leftover) = if target < 0 {
        (0, (-target) as usize)
    } else if target as usize >= rows.len() {
        (rows.len() - 1, target as usize - (rows.len() - 1))
    } else {
        (target as usize, 0)
    };
    let cell = table.rows[rows[index]].cells.get(loc.column)?;
    buf.set_cursor(cell.cursor());
    Some(leftover)
}

/// A vertical motion that left a table must not come to rest on its rule line:
/// if the ordinary line motion landed there, step once more in the same
/// direction. `table` describes the buffer as the motion saw it (no edits are
/// involved), so its row offsets are still exact.
fn step_past_rule(table: &Table, buf: &mut Buffer, down: bool) {
    if table
        .row_at(buf.cursor())
        .is_some_and(|row| table.is_delimiter(row))
    {
        if down {
            buf.move_down();
        } else {
            buf.move_up();
        }
    }
}

/// Where a character motion (`h`/`l`/`w`/`b`/`e`) may come to rest inside a
/// table: on a cell's content, never on its padding or pipes. A motion that
/// walks off the end of a cell steps into the next one in the same row (the
/// previous one for a backward motion); at the edge of the row it stays on the
/// cell's own content instead of escaping into the gutter.
///
/// Which end of the neighbour it lands on follows the motion: `l`, `w` and `b`
/// arrive at a cell's first content character, while `h` and the inclusive `e`
/// arrive at its last. An empty cell has no content, so its padding is the only
/// place to sit and `Cell::cursor()` is used.
fn snap_cell_motion(table: &Table, loc: Located, key: Key, buf: &mut Buffer) {
    let cell = loc.cell;
    let cursor = buf.cursor();
    // Already on the cell's content: the motion was ordinary, leave it alone.
    if cell.start <= cursor && cursor < cell.end {
        return;
    }
    let forward = matches!(key, Key::Char('l' | ' ' | 'w' | 'e'));
    let land_on_end = matches!(key, Key::Char('h' | 'e'));
    let Some(row) = table.rows.get(loc.row) else {
        return;
    };
    let neighbour = if forward {
        row.cells.get(loc.column + 1)
    } else if loc.column > 0 {
        row.cells.get(loc.column - 1)
    } else {
        None
    };
    let at = match neighbour {
        Some(cell) => cell_edge(buf, cell, land_on_end),
        // At the edge of the row there is nowhere to step, so the cursor stays on
        // this cell's content at the end the motion was travelling towards.
        None => cell_edge(buf, &cell, forward),
    };
    buf.set_cursor(at);
}

/// The offset of a cell's first or last content character. An empty cell has no
/// content, so its padding is the only place a cursor can sit.
fn cell_edge(buf: &Buffer, cell: &Cell, last: bool) -> usize {
    if last && cell.start < cell.end {
        buf.prev_char_boundary(cell.end)
    } else {
        cell.cursor()
    }
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
    /// Visual mode: motions extend the selection, operators act on it, and a
    /// charwise selection is inclusive of the character under the cursor.
    #[test]
    fn visual_mode() {
        let mut v = Vim::new();
        let mut b = buf(); // "hello world\nsecond line\nthird"
        type_keys(&mut v, "vll", &mut b);
        assert_eq!(v.mode(), Mode::Visual);
        assert_eq!(b.selection(), Some((0, 2)));
        assert_eq!(v.selection_range(&b), Some((0, 3)), "inclusive of the head");
        type_keys(&mut v, "d", &mut b);
        assert_eq!(b.text(), "lo world\nsecond line\nthird");
        assert_eq!(v.mode(), Mode::Normal, "the operator ends the selection");
        assert_eq!(b.selection(), None);

        // A word motion is exclusive, so the selection reaches into the next
        // word, exactly as vim's `vw` does.
        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "vw", &mut b);
        assert_eq!(v.selection_range(&b), Some((0, 7)));
        type_keys(&mut v, "y", &mut b);
        assert_eq!(v.register.as_ref().unwrap().text, "hello w");

        // `o` jumps to the other end of the selection.
        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "vll", &mut b);
        assert_eq!(b.cursor(), 2);
        type_keys(&mut v, "o", &mut b);
        assert_eq!(b.cursor(), 0);
        assert_eq!(b.selection(), Some((2, 0)));

        // `h` past the anchor flips the selection to the other side, as vim's
        // does.
        v = Vim::new();
        let mut b = buf();
        b.set_cursor(6);
        type_keys(&mut v, "vh", &mut b);
        assert_eq!(v.selection_range(&b), Some((5, 7)), "the space and the w");

        // Counts extend visual motions.
        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "v2l", &mut b);
        assert_eq!(b.selection(), Some((0, 2)));

        // A text object selects the object itself.
        v = Vim::new();
        let mut b = buf();
        b.set_cursor(7);
        type_keys(&mut v, "viw", &mut b);
        assert_eq!(
            v.selection_range(&b),
            Some((6, 11)),
            "iw selects the word under the cursor"
        );

        // `I` and `A` insert at the near and far ends of the selection's lines.
        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "vl", &mut b);
        assert_eq!(
            v.handle_normal_key(Key::Char('I'), &mut b),
            Outcome::EnterInsert
        );
        assert_eq!(b.cursor(), 0);
        assert_eq!(b.selection(), None);

        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "vl", &mut b);
        assert_eq!(
            v.handle_normal_key(Key::Char('A'), &mut b),
            Outcome::EnterInsert
        );
        assert_eq!(b.cursor(), 11, "`A` goes to the end of the line");

        // `v` again exits, and `V` switches the selection to linewise.
        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "vlv", &mut b);
        assert_eq!(v.mode(), Mode::Normal);
        assert_eq!(b.selection(), None);

        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "vV", &mut b);
        assert_eq!(v.mode(), Mode::VisualLine);

        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "vl", &mut b);
        v.handle_normal_key(Key::Escape, &mut b);
        assert_eq!(v.mode(), Mode::Normal);
        assert_eq!(b.selection(), None);
    }

    /// Linewise visual (`V`): whole lines, line breaks included.
    #[test]
    fn visual_line_mode() {
        let mut v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "V", &mut b);
        assert_eq!(v.mode(), Mode::VisualLine);
        assert_eq!(v.selection_range(&b), Some((0, 12)), "one whole line");

        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "Vj", &mut b);
        assert_eq!(v.selection_range(&b), Some((0, 24)), "two whole lines");
        type_keys(&mut v, "d", &mut b);
        assert_eq!(b.text(), "third");

        // `V` + yank + paste duplicates the line.
        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "Vyp", &mut b);
        assert_eq!(b.text(), "hello world\nhello world\nsecond line\nthird");
        assert!(v.register.as_ref().unwrap().linewise);

        // `c` on a linewise selection deletes and enters insert.
        v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "Vc", &mut b);
        assert_eq!(b.text(), "second line\nthird");
    }

    /// `.` replays the last change: operator commands, counts, and insert
    /// sessions including the text that was typed.
    #[test]
    fn repeat_last_change() {
        let mut v = Vim::new();
        let mut b = Buffer::new("one two three".to_string());
        type_keys(&mut v, "dw", &mut b);
        assert_eq!(b.text(), "two three");
        type_keys(&mut v, ".", &mut b);
        assert_eq!(b.text(), "three", "`.` replays at the new cursor");
        type_keys(&mut v, ".", &mut b);
        assert_eq!(b.text(), "", "and `.` does not record itself");

        // A count belongs to the recording.
        let mut v = Vim::new();
        let mut b = Buffer::new("a b c d e f".to_string());
        type_keys(&mut v, "2dw", &mut b);
        assert_eq!(b.text(), "c d e f");
        type_keys(&mut v, ".", &mut b);
        assert_eq!(b.text(), "e f");

        // `dd` and `p` replay as well.
        let mut v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "dd.", &mut b);
        assert_eq!(b.text(), "third");

        let mut v = Vim::new();
        let mut b = Buffer::new("ab\ncd\nef\ngh".to_string());
        type_keys(&mut v, "yyjp", &mut b);
        assert_eq!(b.text(), "ab\ncd\nab\nef\ngh");
        type_keys(&mut v, "3G.", &mut b);
        assert_eq!(
            b.text(),
            "ab\ncd\nab\nab\nef\ngh",
            "the paste repeats on the line the cursor is on"
        );

        // An insert session replays with the typed text, and stays in normal
        // mode.
        let mut v = Vim::new();
        let mut b = Buffer::new("one\ntwo".to_string());
        type_keys(&mut v, "o", &mut b);
        b.insert("new");
        v.note_inserted("new");
        v.enter_normal();
        assert_eq!(b.text(), "one\nnew\ntwo");
        assert_eq!(v.mode(), Mode::Normal);
        type_keys(&mut v, ".", &mut b);
        assert_eq!(b.text(), "one\nnew\nnew\ntwo");
        assert_eq!(v.mode(), Mode::Normal, "`.` stays in normal mode");

        // A command that changed nothing has nothing to repeat.
        let mut v = Vim::new();
        let mut b = buf();
        type_keys(&mut v, "2w", &mut b);
        assert_eq!(
            v.handle_normal_key(Key::Char('.'), &mut b),
            Outcome::Unhandled
        );
    }

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

        // A bound key can still be the find argument: `fv` must find a literal
        // `v`, not enter visual mode.
        let mut b = Buffer::new("abc vivid".to_string());
        type_keys(&mut v, "0fv", &mut b);
        assert_eq!(v.mode(), Mode::Normal, "`fv` is not visual mode");
        assert_eq!(b.cursor(), 4, "and should find the v");

        // The same in visual mode, where more keys are intercepted.
        let mut b = Buffer::new("abc. def".to_string());
        type_keys(&mut v, "vf.", &mut b);
        assert_eq!(v.mode(), Mode::Visual, "`f.` is not repeat-last");
        assert_eq!(v.selection_range(&b), Some((0, 4)));
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

    /// A table with distinct content in every cell, so the cell the cursor is in
    /// can be read straight off the text.
    fn table_buf() -> Buffer {
        Buffer::new(
            "| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n"
                .to_string(),
        )
    }

    #[test]
    fn table_cell_motions_step_between_cells() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        type_keys(&mut vim, "L", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 2],
            "22",
            "one column right"
        );
        type_keys(&mut vim, "H", &mut b);
        assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "1", "back left");
        type_keys(&mut vim, "j", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 1],
            "4",
            "plain j steps to the cell below"
        );
        type_keys(&mut vim, "k", &mut b);
        assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "1");
        // A count jumps that many cells; past the bottom of the table the rest of
        // the count becomes an ordinary line motion, which is how the cursor
        // leaves the table (see `j_and_k_walk_out_of_the_table`).
        type_keys(&mut vim, "j", &mut b);
        assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "4", "one cell down");
        b.set_cursor(b.text().find('1').unwrap());
        type_keys(&mut vim, "2L", &mut b);
        assert_eq!(&b.text()[b.cursor()..b.cursor() + 3], "333");
    }

    #[test]
    fn character_motions_step_from_cell_to_cell() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        // `l` never comes to rest on a cell's padding or on a pipe: one step past
        // the content lands on the next cell's first character.
        type_keys(&mut vim, "l", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 1],
            "2",
            "across the boundary into the next cell"
        );
        let second_cell = b.cursor();
        type_keys(&mut vim, "l", &mut b);
        assert_eq!(
            b.cursor(),
            second_cell + 1,
            "walking the content stays inside the cell"
        );
        type_keys(&mut vim, "l", &mut b);
        assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "3");
        // At the right edge of the row there is nowhere to step, so the cursor
        // stays on the cell's last character instead of escaping into the gutter.
        type_keys(&mut vim, "lll", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 2],
            "3 ",
            "held at the cell's last content character, not the padding beyond it"
        );
        // `h` is the mirror image: it arrives at the previous cell's last
        // character, which is where a leftward motion belongs.
        b.set_cursor(b.text().find('3').unwrap());
        type_keys(&mut vim, "h", &mut b);
        assert_eq!(
            b.cursor(),
            b.text().find("22").unwrap() + 1,
            "one cell left, at its end"
        );
        // `b` arrives at the previous cell's first character.
        type_keys(&mut vim, "b", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 2],
            "22",
            "the first character of the cell to the left"
        );
        // The column edges replace the line's, and both land on content.
        b.set_cursor(b.text().find("333").unwrap());
        type_keys(&mut vim, "0", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 1],
            "3",
            "0 goes to the cell's first content character"
        );
        type_keys(&mut vim, "$", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 3],
            "3 |",
            "$ stops at the cell's last character, not the line's"
        );
        type_keys(&mut vim, "^", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 1],
            "3",
            "^ goes to the first content character"
        );
    }

    /// The same table with prose above and below, so a motion can be seen leaving
    /// it.
    fn table_buf_with_context() -> Buffer {
        Buffer::new(
            "before\n\n| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n\nafter\n"
                .to_string(),
        )
    }

    #[test]
    fn j_and_k_skip_the_delimiter_row() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('a').unwrap());
        type_keys(&mut vim, "j", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 1],
            "1",
            "the rule line under the header is skipped"
        );
        type_keys(&mut vim, "k", &mut b);
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 1],
            "a",
            "and on the way back up"
        );
    }

    #[test]
    fn j_and_k_walk_out_of_the_table() {
        let mut vim = Vim::new();
        let mut b = table_buf_with_context();
        b.set_cursor(b.text().find('a').unwrap());
        type_keys(&mut vim, "k", &mut b);
        assert_eq!(b.row_col(b.cursor()).0, 1, "up out of the table");
        assert!(
            table::locate(&table::tables(b.text()), b.cursor()).is_none(),
            "the cursor is no longer in a cell"
        );
        // Down through the whole table: the steps the table could not use become
        // ordinary line motions, and the rule line is skipped on the way out.
        b.set_cursor(b.text().find('a').unwrap());
        type_keys(&mut vim, "4j", &mut b);
        assert_eq!(
            b.row_col(b.cursor()).0,
            7,
            "past the last row and the blank line, never stopping on the rule"
        );
        type_keys(&mut vim, "j", &mut b);
        assert_eq!(b.row_col(b.cursor()).0, 8, "and on down into the prose");
    }

    #[test]
    fn alt_hjkl_moves_a_row_or_column() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        type_alt_keys(&mut vim, 'j', &mut b);
        assert_eq!(
            b.text(),
            "| a | bb | ccc |\n| --- | --- | --- |\n| 4 | 55 | 666 |\n| 1 | 22 | 333 |\n",
            "alt-j moves the row down"
        );
        assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "1");
        type_alt_keys(&mut vim, 'k', &mut b);
        assert_eq!(b.text(), table_buf().text(), "and alt-k puts it back");

        b.set_cursor(b.text().find("22").unwrap());
        type_alt_keys(&mut vim, 'l', &mut b);
        assert_eq!(
            b.text(),
            "| a | ccc | bb |\n| --- | --- | --- |\n| 1 | 333 | 22 |\n| 4 | 666 | 55 |\n",
            "alt-l moves the column right"
        );
        assert_eq!(&b.text()[b.cursor()..b.cursor() + 2], "22");
        type_alt_keys(&mut vim, 'h', &mut b);
        assert_eq!(b.text(), table_buf().text(), "and alt-h puts it back");
    }

    #[test]
    fn alt_motions_outside_a_table_do_nothing() {
        let mut vim = Vim::new();
        let mut b = buf();
        let before = b.text().to_string();
        assert_eq!(
            vim.handle_normal_key(Key::AltChar('j'), &mut b),
            Outcome::Unhandled
        );
        assert_eq!(b.text(), before);
    }

    /// Feed a sequence of alt-held printable keys.
    fn type_alt_keys(vim: &mut Vim, c: char, b: &mut Buffer) {
        vim.handle_normal_key(Key::AltChar(c), b);
    }

    #[test]
    fn find_is_confined_to_the_cell() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        let start = b.text().find('1').unwrap();
        b.set_cursor(start);
        // The pipe that closes the cell belongs to it, so `f|` reaches it.
        type_keys(&mut vim, "f|", &mut b);
        assert_eq!(&b.text()[b.cursor()..b.cursor() + 1], "|");
        // A character that only exists past the cell edge counts as not found.
        b.set_cursor(start);
        type_keys(&mut vim, "f3", &mut b);
        assert_eq!(b.cursor(), start, "the find is cancelled");
    }

    #[test]
    fn operators_stop_at_the_cell_edge() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        type_keys(&mut vim, "dw", &mut b);
        assert!(
            b.text().contains("|  | 22 | 333 |"),
            "only the cell's content went: {}",
            b.text()
        );
    }

    #[test]
    fn dd_removes_a_row_and_selects_the_cell_that_took_its_place() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find("22").unwrap());
        type_keys(&mut vim, "dd", &mut b);
        assert_eq!(
            b.text(),
            "| a | bb | ccc |\n| --- | --- | --- |\n| 4 | 55 | 666 |\n"
        );
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 2],
            "55",
            "the row that moved up is selected"
        );
    }

    #[test]
    fn shift_d_removes_a_column() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find("22").unwrap());
        type_keys(&mut vim, "D", &mut b);
        assert_eq!(
            b.text(),
            "| a | ccc |\n| --- | --- |\n| 1 | 333 |\n| 4 | 666 |\n"
        );
        assert_eq!(
            &b.text()[b.cursor()..b.cursor() + 3],
            "333",
            "the cell that took the column's place"
        );
    }

    #[test]
    fn o_opens_a_table_row_below() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        assert_eq!(
            vim.handle_normal_key(Key::Char('o'), &mut b),
            Outcome::EnterInsert
        );
        assert_eq!(
            b.text(),
            "| a | bb | ccc |\n| --- | --- | --- |\n| 1 | 22 | 333 |\n|  |  |  |\n| 4 | 55 | 666 |\n"
        );
        assert_eq!(
            b.cursor(),
            b.text().find("|  |  |  |").unwrap() + 1,
            "the cursor is in the new row"
        );
    }

    #[test]
    fn capital_o_opens_a_table_row_above() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        assert_eq!(
            vim.handle_normal_key(Key::Char('O'), &mut b),
            Outcome::EnterInsert
        );
        assert_eq!(
            b.text(),
            "| a | bb | ccc |\n| --- | --- | --- |\n|  |  |  |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n"
        );
    }

    #[test]
    fn a_row_is_not_opened_between_the_header_and_its_delimiter() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find(" bb ").unwrap() + 1);
        vim.handle_normal_key(Key::Char('o'), &mut b);
        assert_eq!(
            b.text(),
            "| a | bb | ccc |\n| --- | --- | --- |\n|  |  |  |\n| 1 | 22 | 333 |\n| 4 | 55 | 666 |\n",
            "the table stays a table"
        );
    }

    #[test]
    fn shift_i_clears_the_cell_and_enters_insert() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find("22").unwrap());
        assert_eq!(
            vim.handle_normal_key(Key::Char('I'), &mut b),
            Outcome::EnterInsert
        );
        assert!(
            b.text().contains("| 1 |  | 333 |"),
            "the cell is empty: {}",
            b.text()
        );
        vim.enter_insert();
        b.insert("x");
        assert!(b.text().contains("| 1 | x | 333 |"));
    }

    #[test]
    fn capital_a_appends_at_the_end_of_the_cell() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        assert_eq!(
            vim.handle_normal_key(Key::Char('A'), &mut b),
            Outcome::EnterInsert
        );
        vim.enter_insert();
        b.insert("9");
        assert!(b.text().contains("| 19 | 22 | 333 |"));
    }

    #[test]
    fn pipe_inserts_a_column_after_and_backslash_before() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        type_keys(&mut vim, "|", &mut b);
        assert!(b.text().contains("| 1 |  | 22 | 333 |"), "{}", b.text());
        assert!(
            b.text().contains("| --- | --- | --- | --- |"),
            "the delimiter row stays a valid delimiter"
        );
        b.set_cursor(b.text().find('4').unwrap());
        type_keys(&mut vim, "\\", &mut b);
        assert!(b.text().contains("|  | 4 |  | 55 | 666 |"), "{}", b.text());
    }

    #[test]
    fn visual_selection_follows_cell_motions() {
        let mut vim = Vim::new();
        let mut b = table_buf();
        b.set_cursor(b.text().find('1').unwrap());
        type_keys(&mut vim, "vL", &mut b);
        let (lo, hi) = vim.selection_range(&b).unwrap();
        // Charwise visual is inclusive of the character under the cursor, which
        // landed on the first `2` of the next cell.
        assert_eq!(&b.text()[lo..hi], "1 | 2");
    }
}
