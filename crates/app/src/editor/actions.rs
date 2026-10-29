//! The editor's editing handlers: the vim normal-mode entry point and the
//! handlers its key context wires to actions. `render` binds these; the keys
//! themselves are mapped in `crate::acts`.

use gpui::{Action, Context, Window};
use shale_core::continuation::{enter_action, EnterAction};
use shale_core::mdstyle::Style;
use shale_core::vim::{Key, Mode, Outcome};

use crate::acts::*;

use super::MarkdownEditor;

impl MarkdownEditor {
    /// Status-bar hint for a half-typed command, e.g. `3d`, `2` or `df`.
    pub fn pending_hint(&self) -> String {
        let count = self.vim.count().map(|c| c.to_string()).unwrap_or_default();
        let op = self
            .vim
            .pending_operator()
            .map(|c| c.to_string())
            .unwrap_or_default();
        let motion = self
            .vim
            .pending_motion()
            .map(|c| c.to_string())
            .unwrap_or_default();
        if count.is_empty() && op.is_empty() && motion.is_empty() {
            String::new()
        } else {
            format!(" {count}{op}{motion}")
        }
    }
    pub(super) fn normal(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.buffer.text().to_string();
        let outcome = self.vim.handle_normal_key(key, &mut self.buffer);
        let changed = self.buffer.text() != before;
        if changed {
            self.mark_dirty(cx);
        }
        match outcome {
            Outcome::EnterInsert => {
                self.vim.enter_insert();
                // Everything typed from here belongs to one undo step. When an
                // operator (`cw`, `o`) already changed the text it also already
                // snapshotted, so join that step instead of starting a new one.
                if changed {
                    self.buffer.continue_undo_group();
                } else {
                    self.buffer.begin_undo_group();
                }
            }
            Outcome::FollowLink => {
                let cursor = self.buffer.cursor();
                self.refresh_links();
                self.follow_link_at(cursor, cx);
            }
            // The engine decided a chord that asks for something the window owns
            // (a picker, not an edit). Dispatching the action rather than calling
            // into the workspace is what keeps the editor free of it: the action
            // travels up from here to the window that handles it, exactly as the
            // key would have.
            Outcome::QuickOpen => window.dispatch_action(QuickOpen.boxed_clone(), cx),
            // `space s` is the other leader chord: the window owns the search.
            Outcome::SearchVault => window.dispatch_action(SearchVault.boxed_clone(), cx),
            // vim's `K`: the window owns the checker, so the lookup is asked for by
            // name and travels up to it. Inside a table `K` is still the row
            // motion, which the engine decides before it ever gets here.
            Outcome::Hover => window.dispatch_action(ShowNote.boxed_clone(), cx),
            Outcome::Stay | Outcome::Unhandled => {}
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn insert_edit(&mut self, inserted: &str, cx: &mut Context<Self>) {
        // So `.` can replay an insert session, the engine collects the text.
        self.vim.note_inserted(inserted);
        self.mark_dirty(cx);
        self.ensure_cursor_visible();
        cx.notify();
    }

    // ----- action handlers ----------------------------------------------

    pub(super) fn on_escape(
        &mut self,
        _: &VimEscape,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.vim.enter_normal();
        self.buffer.clear_selection();
        self.buffer.end_undo_group();
        cx.notify();
    }

    /// `:` — vim's command line. The window reads the word the user types next
    /// (see [`crate::ex`]); it is *not* the command palette, which is the fuzzy
    /// list on `ctrl-shift-p` and would turn `:w` into a guess.
    pub(super) fn on_colon(&mut self, _: &VimColon, window: &mut Window, cx: &mut Context<Self>) {
        window.dispatch_action(RunCommandLine.boxed_clone(), cx);
    }

    /// Enter in insert mode. Ordinary rows get a newline; a row that opens a
    /// list, task or quote carries its marker onto the new row (Obsidian), and a
    /// row that holds *nothing but* its marker loses it, which is how a list ends
    /// without typing a second empty item. The marker text is inserted as ordinary
    /// insert text, so it joins the session's undo group and `.` replays it.
    pub(super) fn on_newline(&mut self, _: &Newline, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() != Mode::Insert {
            return;
        }
        let cursor = self.buffer.cursor();
        let in_code = self.row_is_code(cursor);
        match enter_action(self.buffer.text(), cursor, in_code) {
            EnterAction::Continue { text } => {
                self.buffer.insert(&text);
                self.insert_edit(&text, cx);
            }
            EnterAction::Clear { start, end } => {
                // The marker (indent included) goes away and the row is left
                // blank, with the cursor back at its start.
                self.buffer.delete_range(start, end);
                self.insert_edit("", cx);
            }
            EnterAction::Newline => {
                self.buffer.insert("\n");
                self.insert_edit("\n", cx);
            }
        }
    }

    /// Whether the row holding `offset` is inside a fenced code block, where
    /// `-`, `>` and `1.` are literal text and must not be continued.
    fn row_is_code(&mut self, offset: usize) -> bool {
        self.refresh_styles();
        let (row, _) = self.buffer.row_col(offset);
        let start = self.buffer.line_start(row);
        let end = self.buffer.line_end(row);
        self.spans_in(start, end)
            .into_iter()
            .any(|(_, _, style)| matches!(style, Style::CodeBlock | Style::CodeFence))
    }

    pub(super) fn on_next_line(
        &mut self,
        _: &NextLine,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.vim.mode() == Mode::Insert {
            return;
        }
        self.buffer.next_line_first_non_blank();
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_backspace(
        &mut self,
        _: &Backspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.backspace();
        self.insert_edit("", cx);
    }

    pub(super) fn on_delete_forward(
        &mut self,
        _: &DeleteForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.delete_forward();
        self.insert_edit("", cx);
    }

    /// `ctrl-backspace` in insert mode: delete the word before the cursor, which
    /// is vim's `i_CTRL-W` and the same pair normal-mode `db` makes. It joins the
    /// insert session's undo group, exactly as the backspace key does.
    pub(super) fn on_delete_word_back(
        &mut self,
        _: &DeleteWordBack,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.delete_word_back();
        self.insert_edit("", cx);
    }

    pub(super) fn on_move_left(
        &mut self,
        _: &MoveLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.move_left();
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_move_right(
        &mut self,
        _: &MoveRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.move_right();
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_up();
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_move_down(
        &mut self,
        _: &MoveDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.move_down();
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(undo_depth = self.buffer.undo_depth(), "undo action");
        self.buffer.undo();
        self.mark_dirty(cx);
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(redo_depth = self.buffer.redo_depth(), "redo action");
        self.buffer.redo();
        self.mark_dirty(cx);
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_copy(&mut self, _: &CopyText, _window: &mut Window, cx: &mut Context<Self>) {
        let range = match self.vim.mode() {
            Mode::Visual | Mode::VisualLine => self.vim.selection_range(&self.buffer),
            _ => self.buffer.selection().map(|(a, h)| (a.min(h), a.max(h))),
        };
        if let Some((a, h)) = range {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                self.buffer.text()[a..h].to_string(),
            ));
        }
    }

    pub(super) fn on_paste(&mut self, _: &PasteText, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.buffer.insert(&text);
            self.insert_edit(&text, cx);
        }
    }
}
