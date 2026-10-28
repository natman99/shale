//! The pickers: quick-open of notes, the command palette, and vault search.
//!
//! [`crate::picker`] is the widget; this is the part that decides what its rows
//! are and what choosing one means. The same three pieces — build the rows, open
//! the picker, act on the choice — are shared, so a new picker is a new set of
//! rows rather than a new code path.

use std::rc::Rc;

use gpui::{prelude::*, Context, Focusable, SharedString, Window};
use shale_core::search::{search, SearchOptions};

use crate::acts::{CommandPalette, QuickOpen, SearchVault};
use crate::picker::{self, PickerItem, RowSource};

use super::commands::Command;
use super::Workspace;

/// What a picker row stands for. The picker only knows about rows and labels, so
/// the payload travels with the row and comes back through [`Picker::chosen`].
#[derive(Clone)]
pub(crate) enum PickerTarget {
    /// A note to open.
    Note(std::path::PathBuf),
    /// A note to open with the cursor on `line` (1-based, as the search reports
    /// it): a vault-search hit.
    Hit {
        path: std::path::PathBuf,
        line: usize,
    },
    /// A command to dispatch as if its key had been typed.
    Command(Command),
}

/// How many search hits to offer. A keystroke reads the vault, so the walk stops
/// here rather than at the end of a big vault; the planned database is what will
/// let that limit go away.
const MAX_SEARCH_HITS: usize = 200;

/// How much of a matched line a row shows. Long enough to read the sentence the
/// match is in, short enough that the overlay never has to scroll sideways.
const ROW_TEXT: usize = 90;

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
        let rows = self
            .entries
            .iter()
            .map(|entry| {
                (
                    PickerItem {
                        label: entry.stem().to_string().into(),
                        detail: entry
                            .path
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default()
                            .into(),
                        search: entry.path.to_string_lossy().to_string().into(),
                    },
                    PickerTarget::Note(entry.path.clone()),
                )
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            self.status = "the vault holds no notes yet".into();
            cx.notify();
            return;
        }
        self.open_picker("Open a note", rows, window, cx);
    }

    /// The command palette (`ctrl-shift-p`, or `:` in normal mode): one row per
    /// command, each labelled with the keys that reach it.
    pub(super) fn command_palette(
        &mut self,
        _: &CommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = Command::all()
            .iter()
            .map(|command| {
                (
                    PickerItem {
                        label: SharedString::from(command.label()),
                        detail: SharedString::from(command.keys()),
                        // The keys match too, so `ctrl-w` finds the window commands.
                        search: SharedString::from(format!(
                            "{} {}",
                            command.label(),
                            command.keys()
                        )),
                    },
                    PickerTarget::Command(*command),
                )
            })
            .collect::<Vec<_>>();
        self.open_picker("Run a command", rows, window, cx);
    }

    /// Vault search (`ctrl-shift-f`): one row per line that contains the query.
    ///
    /// Unlike the other two, the rows cannot be listed up front — finding them
    /// means reading every note — so the picker is given a [`RowSource`] and asks
    /// it on each keystroke. That is cheap for a vault of this size and will want
    /// the planned database (or at least a cached index) for a big one.
    pub(super) fn search_vault(
        &mut self,
        _: &SearchVault,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let vault = Rc::clone(&self.vault);
        let entries = self.entries.clone();
        let source: RowSource<PickerTarget> = Rc::new(move |query: &str| {
            let query = query.trim();
            if query.is_empty() {
                return Vec::new();
            }
            let opts = SearchOptions {
                max_hits: Some(MAX_SEARCH_HITS),
                ..Default::default()
            };
            let Ok(hits) = search(&vault, &entries, query, &opts) else {
                return Vec::new();
            };
            hits.into_iter()
                .map(|hit| {
                    // The line leads, because that is what the user is looking
                    // for; where it is lives in the dimmer text after it.
                    let line = clip(hit.line_text.trim(), ROW_TEXT);
                    (
                        PickerItem {
                            label: line.clone().into(),
                            detail: SharedString::from(format!(
                                "{}:{}",
                                hit.path.display(),
                                hit.line_number
                            )),
                            search: line.into(),
                        },
                        PickerTarget::Hit {
                            path: hit.path,
                            line: hit.line_number,
                        },
                    )
                })
                .collect()
        });
        self.open_sourced_picker("Search every note", source, window, cx);
    }

    /// Open a picker over rows that already exist.
    pub(super) fn open_picker(
        &mut self,
        label: &str,
        rows: Vec<(PickerItem, PickerTarget)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (palette, family, size) = self.picker_style(cx);
        let picker = cx.new(|cx| picker::Picker::new(label, rows, palette, family, size, cx));
        self.show_picker(picker, window, cx);
    }

    /// Open a picker whose rows come from the query.
    pub(super) fn open_sourced_picker(
        &mut self,
        label: &str,
        source: RowSource<PickerTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (palette, family, size) = self.picker_style(cx);
        let picker = cx.new(|cx| picker::Picker::sourced(label, source, palette, family, size, cx));
        self.show_picker(picker, window, cx);
    }

    /// The panel is drawn in the editor's face and colours, so a row of search
    /// results — or a checker's note — reads like the note it came from.
    pub(super) fn picker_style(
        &self,
        cx: &Context<Self>,
    ) -> (Rc<shale_core::theme::Palette>, SharedString, f32) {
        let editor = self.editor.read(cx);
        (
            Rc::clone(&self.palette),
            editor.font_family(),
            editor.font_size(),
        )
    }

    /// Put an open picker on screen and point the keyboard at it.
    fn show_picker(
        &mut self,
        picker: gpui::Entity<self::picker::Picker<PickerTarget>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A picker owns the keyboard, so a popup cannot stay up beside it: one
        // panel, one focus, no argument about which `escape` answers to.
        self.close_popup(window, cx);
        self._picker_events = Some(cx.subscribe_in(
            &picker,
            window,
            |ws, picker, event, window, cx| match event {
                picker::PickerEvent::Choose => {
                    let chosen = picker.read_with(cx, |picker, _cx| picker.chosen());
                    if let Some(chosen) = chosen {
                        ws.choose_picker_row(chosen, window, cx);
                    }
                }
                picker::PickerEvent::Cancel => ws.close_picker(window, cx),
            },
        ));
        self.picker = Some(picker.clone());
        self.picker_return_focus = window.focused(cx);
        let focus = picker.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// The row `enter` chose. Closing first is what puts the keyboard back before
    /// the choice takes it: a command is dispatched to whatever had the keyboard
    /// a moment ago, which is what the user was working on.
    fn choose_picker_row(
        &mut self,
        chosen: PickerTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_picker(window, cx);
        match chosen {
            PickerTarget::Note(path) => self.open_note(&path, cx),
            PickerTarget::Hit { path, line } => {
                self.open_note_at(&path, line, cx);
            }
            PickerTarget::Command(command) => {
                self.status = command.label().into();
                window.dispatch_action(command.action(), cx);
            }
        }
    }

    /// Close the picker and hand the keyboard back to whatever had it before.
    pub(super) fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_none() {
            return;
        }
        self._picker_events = None;
        let focus = match self.picker_return_focus.take() {
            Some(focus) => focus,
            None => self.editor.read(cx).focus_handle(cx),
        };
        window.focus(&focus, cx);
        cx.notify();
    }
}

/// Shorten to `max` characters, marking that something was left off. Never cuts
/// through the middle of a character.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…")
}
