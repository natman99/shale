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
    /// A property key from the vault-wide census (`:keys`, P4.3a M5). Choosing it
    /// does not open a new surface: it puts `prop:key` in the search field, which is
    /// the same question the pane is asking and can be refined from there.
    PropKey(String),
    /// The same census row as [`Self::PropKey`], chosen in the pane's rename mood
    /// (`:rename-key`, P4.3a M5): the key is not a question to search, it is the
    /// thing whose spelling is about to change.
    RenameKey(String),
    /// A saved query note (`.shq`) to run against the index (P4.3 M2). The path is the
    /// file, not a note: the answer's rows are the surface, and a row that names a note
    /// becomes a [`Self::Note`] target on its own.
    RunQuery(std::path::PathBuf),
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
                        detail_ranges: Vec::new(),
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
                        detail_ranges: Vec::new(),
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
        self.search_vault_with("", window, cx);
    }

    /// The vault search with the field already holding a query. The properties pane
    /// reaches it this way: a key row is a question about the index, and the search
    /// is where that question can be answered and then widened.
    pub(super) fn search_vault_with(
        &mut self,
        seed: &str,
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
                    // Operators only is a question about notes, so it gets one row
                    // per note rather than one row per line — and each row carries
                    // the line of the note that answers, or a row would name a note
                    // with no way to see why it is there.
                    let ops = q.ops.clone();
                    return selected
                        .unwrap_or_default()
                        .into_iter()
                        .take(MAX_SEARCH_HITS)
                        .map(|path| {
                            let (detail, detail_ranges) = answered(&vault, &path, &ops);
                            let search = rel_text(&path);
                            (
                                PickerItem {
                                    label: clip(
                                        path.file_stem().and_then(|s| s.to_str()).unwrap_or(""),
                                        ROW_TEXT,
                                    )
                                    .into(),
                                    detail: detail.into(),
                                    detail_ranges,
                                    search: search.into(),
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
                            // The line is the label, and the query's own characters
                            // in it are what the picker forwards.
                            detail_ranges: Vec::new(),
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
        self.open_sourced_picker("Search every note", source, seed, window, cx);
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

    /// Open a picker whose rows come from the query, with the field starting at
    /// `seed`.
    pub(super) fn open_sourced_picker(
        &mut self,
        label: &str,
        source: RowSource<PickerTarget>,
        seed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (palette, family, size) = self.picker_style(cx);
        let picker =
            cx.new(|cx| picker::Picker::sourced(label, source, seed, palette, family, size, cx));
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
            PickerTarget::PropKey(key) => {
                // The pane's row asks "which notes write this key?" — the search is
                // the surface that answers that, and the key arrives in its field so
                // the reader can widen the question instead of retyping it. A second
                // list of notes here would be a second surface answering one question.
                let seed = prop_token(&key);
                self.search_vault_with(&seed, window, cx);
            }
            PickerTarget::RunQuery(path) => self.run_query(&path, window, cx),
            PickerTarget::RenameKey(key) => {
                // The pane has said *which* key; the prompt asks for the new
                // spelling and starts with the old one, so the reader edits a name
                // rather than retyping it.
                self.ask_key_rename(&key, window, cx);
            }
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
/// The `prop:` token meaning "this note writes this key at all". A key holding a
/// space or an operator character has to be quoted, because the grammar splits
/// tokens on whitespace and reads `<`, `>`, `=`, `!` as a comparison: `prop:Max HP`
/// would ask for a key called `Max` and a value `HP`, and `prop:cr<5` for a key
/// called `cr<5`... which is to say it would ask for nothing at all.
pub(crate) fn prop_token(key: &str) -> String {
    let quote = key
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, ':' | '<' | '>' | '=' | '!' | '"'));
    if quote {
        format!("prop:\"{key}\"")
    } else {
        format!("prop:{key}")
    }
}

/// A row whose whole job is to be read. See [`PickerTarget::Notice`].
pub(crate) fn notice(reason: impl Into<SharedString>) -> (PickerItem, PickerTarget) {
    let reason: SharedString = reason.into();
    (
        PickerItem {
            label: reason.clone(),
            detail: SharedString::default(),
            detail_ranges: Vec::new(),
            search: reason.clone(),
        },
        // The reason travels with the row so choosing it can say it again where a
        // picker cannot: the status bar.
        PickerTarget::Notice(reason),
    )
}

/// How much room the row's evidence gets in total. Split between the fragments, so
/// two operators both get their key shown rather than the first eating the row.
const EVIDENCE_TEXT: usize = 90;

/// The floor for one fragment: below this a quote stops being a quote.
const MIN_FRAGMENT: usize = 16;

/// A slice of a line to show, cut **around the match** rather than from its start,
/// with the forwarded ranges re-mapped into what is actually shown.
///
/// The head of a long line is not the interesting part: a 78-character line quoted
/// at 60 can cut the match away entirely, which is the opposite of why the row is
/// there. So the window is centred on the first forwarded range, and the ellipses
/// say where text was left out.
///
/// Everything here counts in characters and hands back byte ranges into the string
/// it built, because gpui's text runs are byte lengths and a run that ends inside a
/// multi-byte character is an `invalid text run` panic — which is exactly how the
/// first version of this crashed, its range landing inside the `…` it had appended.
pub(crate) fn evidence_window(
    line: &str,
    ranges: &[(usize, usize)],
    max: usize,
) -> (String, Vec<(usize, usize)>) {
    let chars: Vec<char> = line.chars().collect();
    // Byte offset of each character, so a character window can be stated in bytes.
    let mut byte_of = Vec::with_capacity(chars.len() + 1);
    let mut acc = 0usize;
    for c in chars.iter() {
        byte_of.push(acc);
        acc += c.len_utf8();
    }
    byte_of.push(acc);

    if chars.len() <= max {
        return (line.to_string(), ranges.to_vec());
    }

    // The key: the first range the query named, in character indices.
    let key = ranges
        .first()
        .and_then(|&(start, end)| Some((char_index(line, start)?, char_index(line, end)?)));
    let key_len = key.map_or(0, |(start, end)| end.saturating_sub(start));

    let room = max.saturating_sub(key_len);
    let mut start = key.map_or(0, |(k, _)| k.saturating_sub(room / 2));
    if start + max > chars.len() {
        start = chars.len().saturating_sub(max);
    }
    let end = (start + max).min(chars.len());

    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < chars.len() { "…" } else { "" };
    let shown: String = format!("{prefix}{}{suffix}", &line[byte_of[start]..byte_of[end]]);
    let shift = prefix.len();

    let mapped = ranges
        .iter()
        .filter_map(|&(s, e)| {
            let (cs, ce) = (char_index(line, s)?, char_index(line, e)?);
            // Only a range wholly inside the window is forwarded; a partial one is
            // dropped rather than clipped, so a highlight never claims half a word.
            if cs < start || ce > end || cs >= ce {
                return None;
            }
            Some((
                shift + byte_of[cs] - byte_of[start],
                shift + byte_of[ce] - byte_of[start],
            ))
        })
        .collect();
    (shown, mapped)
}

/// The character index a byte offset sits at, or `None` when the byte is not the
/// start of a character — a range that arrives mid-character is dropped.
fn char_index(line: &str, byte: usize) -> Option<usize> {
    if byte > line.len() || !line.is_char_boundary(byte) {
        return None;
    }
    Some(line[..byte].chars().count())
}

/// What a note says that makes the operators true: one fragment per operator, in
/// the order the query asked, and the folder the note lives in last.
///
/// The fragments come from the note's own text ([`shale_core::evidence`]) rather
/// than from the index's tables — the ruling, and the reason, are in that module.
/// A note whose file has moved on from its index row contributes nothing for that
/// operator, rather than a line it no longer holds.
fn answered(
    vault: &shale_core::vault::Vault,
    path: &std::path::Path,
    ops: &[shale_core::query::Op],
) -> (String, Vec<(usize, usize)>) {
    use shale_core::query::Op;
    // The paths the index answers with are vault-relative, which is what makes a
    // note one string on every platform — and what makes reading it here go through
    // the vault rather than the process's working directory.
    let text = vault.read(path).unwrap_or_default();
    let mut detail = String::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();

    // Two operators, two quotes, both with their key visible: the row's evidence
    // budget is split rather than spent by whichever operator comes first.
    let asked = ops
        .iter()
        .filter(|op| !matches!(op, Op::Path(_) | Op::Modified(_)))
        .count()
        .max(1);
    let budget = (EVIDENCE_TEXT / asked).max(MIN_FRAGMENT);

    for op in ops {
        let fragment = match op {
            // The folder at the end of the row *is* the answer to `path:`.
            Op::Path(_) => None,
            // A date is a fact about the file, not a line inside it. The day shown
            // is the file's own mtime — what `ls` would say — which is what the
            // operator compared when the index was current.
            Op::Modified(_) => Some((
                format!("modified {}", modified_day(vault, path)),
                Vec::new(),
            )),
            _ => shale_core::evidence::evidence(&text, op).map(|ev| (ev.line, ev.ranges)),
        };
        let Some((fragment, inner)) = fragment else {
            continue;
        };
        let (fragment, inner) = evidence_window(fragment.trim(), &inner, budget);
        if fragment.is_empty() {
            continue;
        }
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        let base = detail.len();
        detail.push_str(&fragment);
        ranges.extend(
            inner
                .into_iter()
                .map(|(start, end)| (base + start, base + end)),
        );
    }

    // The folder last, so the note's words lead and a vault with two notes of the
    // same name is still readable.
    let rel = rel_text(path);
    if let Some((folder, _)) = rel.rsplit_once('/') {
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        detail.push_str(folder);
    }
    (detail, ranges)
}

/// The day a file was last written, as `YYYY-MM-DD` — the same form `modified:`
/// takes a date in.
fn modified_day(vault: &shale_core::vault::Vault, rel: &std::path::Path) -> String {
    let absolute = match vault.resolve(rel) {
        Ok(path) => path,
        Err(_) => return String::new(),
    };
    let secs = absolute
        .metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0);
    shale_core::query::civil_date(secs)
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…")
}
