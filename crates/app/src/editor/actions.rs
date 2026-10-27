//! The editor's editing handlers: the vim normal-mode entry point and the
//! handlers its key context wires to actions. `render` binds these; the keys
//! themselves are mapped in `crate::acts`.

use gpui::{Context, Window};
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
    pub(super) fn normal(&mut self, key: Key, cx: &mut Context<Self>) {
        let before = self.buffer.text().to_string();
        let outcome = self.vim.handle_normal_key(key, &mut self.buffer);
        let changed = self.buffer.text() != before;
        if changed {
            self.mark_dirty();
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
            Outcome::Stay | Outcome::Unhandled => {}
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn insert_edit(&mut self, inserted: &str, cx: &mut Context<Self>) {
        // So `.` can replay an insert session, the engine collects the text.
        self.vim.note_inserted(inserted);
        self.mark_dirty();
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

    pub(super) fn on_newline(&mut self, _: &Newline, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() != Mode::Insert {
            return;
        }
        self.buffer.insert("\n");
        self.insert_edit("\n", cx);
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
        self.mark_dirty();
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(redo_depth = self.buffer.redo_depth(), "redo action");
        self.buffer.redo();
        self.mark_dirty();
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
