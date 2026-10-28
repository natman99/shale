//! The pickers: quick-open of notes and the command palette.
//!
//! [`crate::picker`] is the widget; this is the part that decides what its rows
//! are and what choosing one means. The same three pieces — build the rows, open
//! the picker, act on the choice — are what vault search will need too, so they
//! live here rather than inside an action handler.

use std::rc::Rc;

use gpui::{prelude::*, Context, Focusable, SharedString, Window};

use crate::acts::{CommandPalette, QuickOpen};
use crate::picker::{self, PickerItem};

use super::commands::Command;
use super::Workspace;

/// What a picker row stands for. The picker itself only knows about rows and
/// labels, so the workspace keeps the payload alongside them.
#[derive(Clone)]
pub(crate) enum PickerTarget {
    /// A note to open.
    Note(std::path::PathBuf),
    /// A command to dispatch as if its key had been typed.
    Command(Command),
}

impl Workspace {
    /// Quick-open (`ctrl-o`, `space f`): one row per note in the vault.
    pub(super) fn quick_open(
        &mut self,
        _: &QuickOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The name leads, the folder it lives in is shown dimmer after it, and the
        // whole path is what the query matches, so `no/an` reaches
        // `notes/Another Note.md` without the row being hard to read.
        let mut items = Vec::with_capacity(self.entries.len());
        let mut targets = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            items.push(PickerItem {
                label: entry.stem().to_string().into(),
                detail: entry
                    .path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default()
                    .into(),
                search: entry.path.to_string_lossy().to_string().into(),
            });
            targets.push(PickerTarget::Note(entry.path.clone()));
        }
        if items.is_empty() {
            self.status = "the vault holds no notes yet".into();
            cx.notify();
            return;
        }
        self.open_picker("Open a note", &items, &targets, window, cx);
    }

    /// The command palette (`ctrl-shift-p`, or `:` in normal mode): one row per
    /// command, each labelled with the keys that reach it.
    pub(super) fn command_palette(
        &mut self,
        _: &CommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = Command::all()
            .iter()
            .map(|command| PickerItem {
                label: SharedString::from(command.label()),
                detail: SharedString::from(command.keys()),
                // The keys match too, so `ctrl-w` finds the window commands.
                search: SharedString::from(format!("{} {}", command.label(), command.keys())),
            })
            .collect::<Vec<_>>();
        let targets = Command::all()
            .iter()
            .map(|command| PickerTarget::Command(*command))
            .collect::<Vec<_>>();
        self.open_picker("Run a command", &items, &targets, window, cx);
    }

    /// Open a picker over `items`, remembering what each row stands for.
    pub(super) fn open_picker(
        &mut self,
        label: &str,
        items: &[PickerItem],
        targets: &[PickerTarget],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let family = self.editor.read(cx).font_family();
        let size = self.editor.read(cx).font_size();
        let palette = Rc::clone(&self.palette);
        let picker =
            cx.new(|cx| picker::Picker::new(label, items.to_vec(), palette, family, size, cx));
        self._picker_events = Some(cx.subscribe_in(
            &picker,
            window,
            |ws, _picker, event, window, cx| match event {
                picker::PickerEvent::Choose(index) => ws.choose_picker_row(*index, window, cx),
                picker::PickerEvent::Cancel => ws.close_picker(window, cx),
            },
        ));
        self.picker = Some(picker.clone());
        self.picker_targets = targets.to_vec();
        self.picker_return_focus = window.focused(cx);
        let focus = picker.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// The row `enter` chose. Closing first is what puts the keyboard back before
    /// the choice takes it: a command is dispatched to whatever had the keyboard
    /// a moment ago, which is what the user was working on.
    fn choose_picker_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = self.picker_targets.get(index).cloned();
        self.close_picker(window, cx);
        match chosen {
            Some(PickerTarget::Note(path)) => self.open_note(&path, cx),
            Some(PickerTarget::Command(command)) => {
                self.status = command.label().into();
                window.dispatch_action(command.action(), cx);
            }
            None => {
                self.status = "nothing to choose".into();
                cx.notify();
            }
        }
    }

    /// Close the picker and hand the keyboard back to whatever had it before.
    pub(super) fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_none() {
            return;
        }
        self.picker_targets.clear();
        self._picker_events = None;
        let focus = match self.picker_return_focus.take() {
            Some(focus) => focus,
            None => self.editor.read(cx).focus_handle(cx),
        };
        window.focus(&focus, cx);
        cx.notify();
    }
}
