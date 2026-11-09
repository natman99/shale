//! The pickers: quick-open of notes, the command palette, and vault search.
//!
//! [`crate::picker`] is the widget; this is the part that decides what its rows
//! are and what choosing one means. The same three pieces — build the rows, open
//! the picker, act on the choice — are shared, so a new picker is a new set of
//! rows rather than a new code path.

use std::rc::Rc;

use gpui::{prelude::*, Context, Focusable, SharedString, Window};
use shale_core::search::{search, SearchOptions};
use shale_core::vault::{rel_text, VaultEntry};

use crate::acts::{CommandPalette, QuickOpen, SearchVault};
use crate::picker::{self, PickerItem, RowSource};

use super::Workspace;
use crate::command::Command;

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
    /// A palette to draw the window with, named the way `appearance.theme` names
    /// one: a built-in or the stem of a `<vault>/.shale/themes/<name>.toml`.
    Theme(String),
    /// A vault to open in this process — the window's vault closes when it does.
    Vault(std::path::PathBuf),
    /// The file dialog that adds a vault the app has never seen.
    VaultPrompt,
    /// A row that exists to be read rather than chosen: the query could not be
    /// answered, and the reason *is* the answer. Choosing it repeats the reason in
    /// the status bar. A picker has no other way to say anything, and returning no
    /// rows at all would read as an empty vault rather than a refused question.
    Notice(SharedString),
}

/// How many search hits to offer. A keystroke reads the vault, so the walk stops
/// here rather than at the end of a big vault; the planned database is what will
/// let that limit go away.
const MAX_SEARCH_HITS: usize = 200;

/// How much of a matched line a row shows. Long enough to read the sentence the
/// match is in, short enough that the overlay never has to scroll sideways.
const ROW_TEXT: usize = 90;

impl Workspace {
    /// Quick-open (`space f`, `g f`, `:e`): one row per note in the vault.
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
                            .map(rel_text)
                            .unwrap_or_default()
                            .into(),
                        search: rel_text(&entry.path).into(),
                    },
                    PickerTarget::Note(entry.path.clone()),
                )
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            self.say("the vault holds no notes yet", cx);
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
            .map(|command| {
                (
                    PickerItem {
                        label: SharedString::from(command.label()),
                        detail: SharedString::from(command.hint()),
                        // The keys match too, so `ctrl-w` finds the window commands.
                        // The command-line spellings are shown but *not* searched:
                        // their letters are what make `new note` match the word
                        // count, and a palette that offers the wrong command is
                        // worse than one that does not know the alias.
                        search: SharedString::from(match command.keys() {
                            "" => command.label().to_string(),
                            keys => format!("{} {}", command.label(), keys),
                        }),
                    },
                    PickerTarget::Command(command),
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
        let db = self.db.clone();
        let source: RowSource<PickerTarget> = Rc::new(move |query: &str| {
            let query = query.trim();
            if query.is_empty() {
                return Vec::new();
            }
            let q = shale_core::query::parse(query);

            // A token shaped like an operator and not one is said, not searched.
            // As prose it would return "0 of 0 matching" for a literal string,
            // which reads as an empty vault rather than a typo.
            if !q.unknown.is_empty() {
                return q
                    .unknown
                    .iter()
                    .map(|token| {
                        notice(format!(
                            "not an operator: {token}  (tag: path: link: prop: modified:)"
                        ))
                    })
                    .collect();
            }

            // Operators are questions about the index. With no index there is no
            // answer, and no rows would lie the same way an empty vault does.
            let selected = if q.needs_index() {
                match db.as_ref().map(|db| db.notes_matching(&q)) {
                    Some(Ok(paths)) => Some(paths),
                    Some(Err(err)) => {
                        return vec![notice(format!("the vault index could not answer: {err:#}"))]
                    }
                    None => {
                        return vec![notice(
                            "tag:, path:, link:, prop: and modified: ask the vault index, \
                             and this vault has none (index.enabled in .shale/config.toml)",
                        )]
                    }
                }
            } else {
                None
            };

            // The operators choose *which notes*; the text still chooses the line.
            let scoped: Vec<VaultEntry> = match &selected {
                Some(paths) => entries
                    .iter()
                    .filter(|entry| paths.contains(&entry.path))
                    .cloned()
                    .collect(),
                None => entries.clone(),
            };

            let text = match q.text.as_deref() {
                // Operators only is a question about notes, so it gets one row per
                // note rather than one row per line.
                None => {
                    return selected
                        .unwrap_or_default()
                        .into_iter()
                        .take(MAX_SEARCH_HITS)
                        .map(|path| {
                            let detail = rel_text(&path);
                            (
                                PickerItem {
                                    label: clip(
                                        path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
                                        ROW_TEXT,
                                    )
                                    .into(),
                                    detail: detail.clone().into(),
                                    search: detail.into(),
                                },
                                PickerTarget::Note(path),
                            )
                        })
                        .collect();
                }
                Some(text) => text,
            };

            let opts = SearchOptions {
                max_hits: Some(MAX_SEARCH_HITS),
                ..Default::default()
            };
            let Ok(hits) = search(&vault, &scoped, text, &opts) else {
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
                                rel_text(&hit.path),
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
        let (family, size) = self.note_font(cx);
        (Rc::clone(&self.palette), family, size)
    }

    /// Put an open picker on screen and point the keyboard at it.
    fn show_picker(
        &mut self,
        picker: gpui::Entity<self::picker::Picker<PickerTarget>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A picker owns the keyboard, so a popup cannot stay up beside it: one
        // panel, one focus, no argument about which `escape` answers to. The card
        // floats over the note too, and a picker is asking for a note by name — a
        // preview of the one the pointer last touched is noise by then.
        self.close_popup(window, cx);
        self.close_card(cx);
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
                // The row under the pointer or the selection. Only a theme row is
                // worth acting on: previewing a palette is the point of listing
                // them, and every other picker's rows would only be disturbed.
                picker::PickerEvent::Highlight => {
                    let target = picker.read_with(cx, |picker, _cx| picker.chosen());
                    if let Some(PickerTarget::Theme(name)) = target {
                        ws.preview_theme(&name, cx);
                    }
                }
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
        // A chosen theme is already on screen as a preview, so accepting it is
        // only forgetting the palette the picker started from. Doing that before
        // the close is what stops `close_picker` putting the old one back.
        if let PickerTarget::Theme(name) = &chosen {
            self.accept_theme(name, cx);
        }
        self.close_picker(window, cx);
        match chosen {
            PickerTarget::Notice(reason) => {
                // The row already says it; this puts the same words where they
                // outlive the picker.
                if !reason.is_empty() {
                    self.say(reason.to_string(), cx);
                }
            }
            PickerTarget::Note(path) => self.open_note(&path, cx),
            PickerTarget::Hit { path, line } => {
                self.open_note_at(&path, line, cx);
            }
            PickerTarget::Theme(name) => {
                // Chosen, so it is saved: the theme the picker was opened with is
                // the one the next launch should open in.
                let note = self.readability_suffix();
                let saved = self.persist_theme(&name);
                self.say(
                    match saved {
                        Ok(()) => format!("theme: {name}{note}"),
                        Err(err) => {
                            tracing::warn!("could not save the theme: {err:#}");
                            format!("theme: {name}, not saved: {err:#}")
                        }
                    },
                    cx,
                );
                cx.notify();
            }
            PickerTarget::Command(command) => {
                self.say(command.label(), cx);
                window.dispatch_action(command.action(), cx);
            }
            PickerTarget::Vault(path) => self.switch_vault(&path, window, cx),
            PickerTarget::VaultPrompt => self.prompt_for_vault(window, cx),
        }
    }

    /// Close the picker and hand the keyboard back to whatever had it before.
    ///
    /// A theme the picker was only previewing goes back with the picker: escape
    /// means "not that one", and the window is drawn as it was before the picker
    /// opened.
    pub(super) fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_none() {
            return;
        }
        self._picker_events = None;
        self.revert_theme(cx);
        let focus = match self.picker_return_focus.take() {
            Some(focus) => focus,
            None => self.active_focus(cx),
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Close the overlays that float over the note, because the pointer clicked
    /// somewhere that was not one of them.
    ///
    /// The picker, vim's command line and the checker's panel all float over the
    /// note, and a click under one used to belong to nothing: the panel stayed up
    /// until `escape`. The panels stop their own clicks on the way to this handler,
    /// so a click on a row, or in the field, is still only a click inside the panel.
    ///
    /// The sidebar's prompts are deliberately *not* dismissed this way. They are
    /// part of the sidebar's layout rather than a panel over the note, and a name
    /// half typed into a rename is work the user has not finished.
    pub(crate) fn dismiss_overlays(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.is_some() {
            self.close_picker(window, cx);
        }
        if self.prompt.as_ref().is_some_and(|(kind, _)| {
            matches!(
                kind,
                crate::prompt::PromptKind::Command | crate::prompt::PromptKind::Search
            )
        }) {
            // Both float over the note. A command line is simply finished; a
            // search keeps its matches, because the click that closed the field
            // was the user putting the cursor somewhere else, not abandoning the
            // query — `n` still works.
            self.close_prompt(window, cx);
        }
        // The checker's panel has a second reason to go here that the others do
        // not: the click takes the keyboard off it whether the window agrees or
        // not, because gpui focuses what the pointer clicked. A panel left up and
        // unfocused is the worst of both — `escape` goes to vim instead of closing
        // it, and a hover will not replace a panel that `K` asked for — so the
        // panel goes with the keyboard, and `close_popup` hands the keyboard back.
        if self.popup.is_some() {
            self.close_popup(window, cx);
        }
        // The card goes with it: a click somewhere else in the window is the pointer
        // saying it is done with the note it was resting on.
        self.close_card(cx);
    }
}

/// Shorten to `max` characters, marking that something was left off. Never cuts
/// through the middle of a character.
/// A row whose whole job is to be read. See [`PickerTarget::Notice`].
fn notice(reason: impl Into<SharedString>) -> (PickerItem, PickerTarget) {
    let reason: SharedString = reason.into();
    (
        PickerItem {
            label: reason.clone(),
            detail: SharedString::default(),
            search: reason.clone(),
        },
        // The reason travels with the row so choosing it can say it again where a
        // picker cannot: the status bar.
        PickerTarget::Notice(reason),
    )
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…")
}
