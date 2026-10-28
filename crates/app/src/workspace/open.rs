//! Quick-open: jump to a note by typing part of its name or path.
//!
//! [`crate::picker`] is the widget; this is the part that decides what its rows
//! are and what choosing one means. The same three pieces — build the rows, open
//! the picker, act on the choice — are what vault search and the command palette
//! will need, so they live here rather than inside the action handler.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{prelude::*, Context, Focusable, Window};

use crate::acts::QuickOpen;
use crate::picker::{self, PickerItem};

use super::Workspace;

impl Workspace {
    /// Quick-open (`ctrl-o`): one row per note in the vault.
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
            targets.push(entry.path.clone());
        }
        if items.is_empty() {
            self.status = "the vault holds no notes yet".into();
            cx.notify();
            return;
        }
        self.open_picker("Open a note", &items, &targets, window, cx);
    }

    /// Open a picker over `items`, remembering what each row stands for.
    pub(super) fn open_picker(
        &mut self,
        label: &str,
        items: &[PickerItem],
        targets: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let family = self.editor.read(cx).font_family();
        let size = self.editor.read(cx).font_size();
        let palette = Rc::clone(&self.palette);
        let items = items.to_vec();
        let picker = cx.new(|cx| picker::Picker::new(label, items, palette, family, size, cx));
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
    /// the chosen note takes it.
    fn choose_picker_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = self.picker_targets.get(index).cloned();
        self.close_picker(window, cx);
        match chosen {
            Some(path) => self.open_note(&path, cx),
            None => {
                self.status = "nothing to open".into();
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
