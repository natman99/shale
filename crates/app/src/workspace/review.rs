//! The window's half of agent edit review (roadmap P2.5a): noticing the queue, and
//! running the commands the strip's keys name.
//!
//! The queue lives in `.shale/pending-edits/` as one TOML file per MCP write call, and
//! the **MCP server writes them** (see `crates/mcp/src/pending_tools.rs`). That is what
//! buys two ruled behaviours for free: proposals arrive while Shale is closed, and they
//! survive `kill -TERM` because they were never in the window's memory. The files are
//! the source of truth; the index may mirror them but a corrupt DB loses nothing.
//!
//! **Why a poll rather than the vault watcher:** the watcher deliberately does not watch
//! dot directories ([`shale_core::watch`]), and `.shale` is not vault content
//! ([`shale_core::vault`]) — which is correct for the vault and wrong for the queue.
//! Rather than teach the watcher about one exempt dot directory, the window reads the
//! queue folder on a beat. It is a small folder, the read is one `read_dir` plus a parse
//! of a few hundred bytes, and a second of latency before a proposal appears is under
//! the time it takes an agent's tool call to come back to it anyway.
//!
//! Accepting is the apply-or-stale policy in [`shale_core::diff`] being *run*: the hunk
//! goes into the buffer when the note is open (one undo step, cursor and unsaved edits
//! left alone) and onto the disk when it is not. What is left of the proposal goes back
//! into its file, re-anchored by the exact line shift the accepted hunk caused, and the
//! file goes away when nothing is left in it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use gpui::{Context, Focusable, Window};
use shale_core::diff::{self, Hunk};
use shale_core::pending::{self, PendingEdit};
use shale_core::vault::rel_text;

use crate::acts::*;
use crate::editor::overlay::InlineEdit;
use crate::editor::View;
use super::tabs::TabContent;

use super::Workspace;

impl Workspace {
    /// Read the queue off the disk and put it in front of the user.
    ///
    /// The immediate paths — startup, and whatever a hunk was just accepted or retracted
    /// — run this. The beat that keeps noticing is the vault's, not the window's
    /// ([`VaultServices::start_queue_watch`](super::services)): one `read_dir` a second
    /// per vault however many windows are looking at it, fanned out to
    /// [`apply_pending_queue`](Self::apply_pending_queue).
    ///
    /// Each note is read once per scan rather than once per hunk, and the note that is
    /// open is read from the **buffer**: what the user can see is what a hunk has to
    /// fit, so a hunk against a note they have since edited is stale, not applicable.
    pub(crate) fn scan_pending(&mut self, cx: &mut Context<Self>) {
        self.apply_pending_queue(pending::list(self.vault.root()), cx);
    }

    /// Judge a queue that has already been read and put it in front of the user. The
    /// judgement is per window: a hunk is stale against *this* window's buffers, which
    /// may be holding a note another window has already written to disk.
    pub(crate) fn apply_pending_queue(
        &mut self,
        queue: pending::Queue,
        cx: &mut Context<Self>,
    ) {
        // A file that does not parse is said once, not once a second: it is a real
        // problem (a proposal the user cannot see) and a log line a person will act on.
        if queue.unreadable != self.pending_unreadable {
            for path in &queue.unreadable {
                tracing::warn!(path = %path.display(), "could not read a pending edit");
            }
            self.pending_unreadable = queue.unreadable.clone();
        }

        let mut stale = HashSet::new();
        let mut texts: HashMap<PathBuf, String> = HashMap::new();
        for (index, edit) in queue.edits.iter().enumerate() {
            let text = texts
                .entry(edit.note.clone())
                .or_insert_with(|| self.note_text(&edit.note, cx));
            for hunk in pending::stale_hunks(edit, text) {
                stale.insert((index, hunk));
            }
        }

        // Two proposals over the same lines. Each may still apply cleanly, and the
        // second goes stale the moment the first lands — which is a decision about
        // order the user gets to make, so it is flagged rather than hidden.
        let mut clash = HashSet::new();
        for (a, first) in queue.edits.iter().enumerate() {
            for (b, second) in queue.edits.iter().enumerate().skip(a + 1) {
                if first.note != second.note {
                    continue;
                }
                for (ha, hunk_a) in first.hunks.iter().enumerate() {
                    for (hb, hunk_b) in second.hunks.iter().enumerate() {
                        if overlaps(hunk_a, hunk_b) {
                            clash.insert((a, ha));
                            clash.insert((b, hb));
                        }
                    }
                }
            }
        }

        let changed = self.review.update(cx, |review, cx| {
            review.set_queue(queue.edits, stale, clash, cx)
        });
        // The note's copy of the same list (roadmap P2.5b). Run unconditionally rather
        // than on `changed`: a tab that has just opened a note needs its proposals even
        // when the queue itself did not move, and handing an editor the list it already
        // has costs it nothing.
        self.sync_overlay(cx);
        if changed {
            cx.notify();
        }
    }

    /// Hand every note tab in this window the proposals that concern its note, for the
    /// inline overlay painted in the note itself (roadmap P2.5b).
    ///
    /// Per tab rather than per window, because a window can show three notes at once and
    /// a proposal belongs to exactly one of them. The editor is handed the **hunks**
    /// rather than the answer to "where do they go": the buffer moves with every
    /// keystroke, so the editor re-places them against its own text each time it paints,
    /// and a hunk that stops fitting stops being painted without anyone having to be
    /// told — which is the same rule that keeps the overlay from showing a change the
    /// accept would refuse.
    pub(crate) fn sync_overlay(&self, cx: &mut Context<Self>) {
        // Read the whole fan-out out first: handing an editor its list is an update, and
        // the strip cannot be held open across one.
        let mut per_tab: Vec<(gpui::Entity<crate::editor::MarkdownEditor>, Vec<InlineEdit>)> =
            Vec::new();
        {
            let review = self.review.read(cx);
            let marked = review.marked();
            for pane in self.panes.iter() {
                for tab in &pane.tabs {
                    let TabContent::Note {
                        path: Some(path),
                        editor,
                        ..
                    } = &tab.content
                    else {
                        continue;
                    };
                    let edits: Vec<InlineEdit> = review
                        .edits()
                        .iter()
                        .filter(|edit| &edit.note == path)
                        .map(|edit| InlineEdit {
                            id: edit.id.clone().into(),
                            hunks: edit.hunks.clone(),
                            current: marked
                                .as_ref()
                                .filter(|(id, _)| *id == edit.id)
                                .and_then(|(_, hunk)| *hunk),
                        })
                        .collect();
                    per_tab.push((editor.clone(), edits));
                }
            }
        }
        for (editor, edits) in per_tab {
            editor.update(cx, |editor, cx| editor.set_pending_edits(edits, cx));
        }
    }

    /// The hunk the cursor is standing on in the note, and only while the note holds the
    /// keyboard (roadmap P2.5b).
    ///
    /// The focus test is the whole point. The strip's `y` means the marked hunk; the
    /// note's `y` means the hunk under the caret. Reading the caret while the strip has
    /// the keyboard would make one key mean two things depending on nothing the user can
    /// see.
    fn cursor_hunk(&self, window: &mut Window, cx: &gpui::App) -> Option<(String, usize)> {
        let editor = self.pane().active_tab().editor()?;
        editor.read_with(cx, |editor, cx| {
            if !editor.focus_handle(cx).is_focused(window) {
                return None;
            }
            editor
                .hunk_at_cursor()
                .map(|placed| (placed.id.to_string(), placed.hunk))
        })
    }

    /// Point the strip's marker at the hunk under the cursor, when the note has the
    /// keyboard.
    ///
    /// Aiming the marker rather than acting on the hunk directly is what lets the note's
    /// `y` and the strip's `y` share one answer — and leaves the strip looking at what
    /// the user just accepted, which is the honest thing for it to show afterwards.
    fn aim_marker_at_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, hunk)) = self.cursor_hunk(window, cx) else {
            return;
        };
        let Some(row) = self.review.read(cx).row_for(&id, hunk) else {
            return;
        };
        self.review.update(cx, |review, cx| review.select_row(row, cx));
    }

    /// Whether the strip has anything to show, which is when it is drawn at all.
    pub(crate) fn review_active(&self, cx: &gpui::App) -> bool {
        !self.review.read(cx).is_empty()
    }

    /// The note a hunk has to fit: the buffer while the note is open, the disk
    /// otherwise. A note that is not on disk counts as empty, which is what a proposal
    /// to create one expects.
    fn note_text(&self, note: &Path, cx: &Context<Self>) -> String {
        // The tab that holds it, if one does — which need not be the tab on screen.
        // A proposal against a note in the background is judged against what that tab
        // is holding, not against the file, or it would be called stale for a change
        // the user can see.
        if let Some(index) = self.tab_for_path(note) {
            self.pane().tabs[index]
                .editor()
                .map(|editor| editor.read_with(cx, |editor, _cx| editor.text()))
                .unwrap_or_default()
        } else {
            self.vault.read(note).unwrap_or_default()
        }
    }

    /// Give the strip the keyboard (`:review`, and the palette's row).
    pub(super) fn review_focus(
        &mut self,
        _: &ReviewFocus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.review.read(cx).is_empty() {
            self.say("nothing is waiting for review", cx);
            cx.notify();
            return;
        }
        // The scan runs first so the strip shows what is on the disk now rather than
        // what was on it a second ago.
        self.scan_pending(cx);
        if self.review.read(cx).is_empty() {
            self.say("nothing is waiting for review", cx);
            cx.notify();
            return;
        }
        window.focus(&self.review.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    /// Hop the marker down (`j` in the strip, `]a` in the note).
    pub(super) fn review_next(
        &mut self,
        _: &ReviewNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review
            .update(cx, |review, cx| review.move_marker(1, cx));
    }

    /// …and up (`k` in the strip, `[a` in the note).
    pub(super) fn review_prev(
        &mut self,
        _: &ReviewPrev,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.review
            .update(cx, |review, cx| review.move_marker(-1, cx));
    }

    /// Collapse or expand the proposal the marker is on (`za`).
    pub(super) fn review_collapse(
        &mut self,
        _: &ReviewCollapse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // One key, two widgets (roadmap P2.5b). Over a line under review in the note it
        // folds that hunk's additions to a `−N +M` count inside the note; over the
        // strip it folds the whole proposal to one row of the list. The strip never
        // rearranges text in the note, and the note never rearranges the list — which
        // is why the two collapse states are kept apart rather than shared.
        if let Some((id, hunk)) = self.cursor_hunk(window, cx) {
            if let Some(editor) = self.pane().active_tab().editor() {
                let collapsed = editor.update(cx, |editor, cx| {
                    let collapsed = editor.toggle_inline_collapse(&id, hunk);
                    cx.notify();
                    collapsed
                });
                self.say(
                    format!(
                        "{} the additions {}",
                        if collapsed { "collapsed" } else { "expanded" },
                        if collapsed { "to a count" } else { "in full" }
                    ),
                    cx,
                );
                cx.notify();
                return;
            }
        }
        self.review
            .update(cx, |review, cx| review.toggle_collapse(cx));
        cx.notify();
    }

    /// Open the note the marker is about (`o`), so the user can read the change in the
    /// note it belongs to.
    ///
    /// The strip keeps the keyboard: `o` is "show me this", and the next key the user
    /// reaches for is usually `y` or `x`. `escape` is the way out of the strip, and
    /// `]a`/`[a` move the marker from the note.
    pub(super) fn review_open_note(
        &mut self,
        _: &ReviewOpenNote,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.review.read(cx).current_row() else {
            return;
        };
        let note = self.review.read(cx).edit(&row).note.clone();
        if self.note().as_ref() == Some(&note) {
            self.say(format!("already showing {}", rel_text(&note)), cx);
            cx.notify();
            return;
        }
        self.open_note(&note, cx);
        // Opening a note re-anchors nothing, but it does change what a hunk has to fit
        // — the buffer is now the answer rather than the disk.
        self.scan_pending(cx);
    }

    /// Accept the hunk the marker is on (`y`).
    ///
    /// On a hunk row that is that one hunk. On a collapsed proposal's row it is
    /// everything of that proposal that still fits, which is what makes "yes, do what
    /// you suggested" one key rather than six.
    pub(super) fn review_accept(
        &mut self,
        _: &ReviewAccept,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The note's `y` means the hunk under its cursor; the strip's means the marked
        // one (roadmap P2.5b). Aiming first is what lets both share the answer below.
        self.aim_marker_at_cursor(window, cx);
        let Some(row) = self.review.read(cx).current_row() else {
            return;
        };
        let edit = self.review.read(cx).edit(&row).clone();
        // A note in the view that refuses edits cannot take an accept, whichever tab
        // it is on.
        if self.note_is_preview(&edit.note, cx) {
            // Preview mode refuses edits, and an accept that quietly did nothing would
            // be worse than saying the note has to be editable first.
            self.say(
                "the note is in preview: `space l` to edit it, then accept",
                cx,
            );
            cx.notify();
            return;
        }
        let (applied, stale) = self.apply_pending(&edit, row.hunk, cx);
        if applied == 0 && stale == 0 {
            self.say("nothing to accept", cx);
        } else if applied == 0 {
            self.say(
                "that hunk no longer fits the note: reject it, or ask the agent to re-propose",
                cx,
            );
        } else if stale == 0 {
            self.say(
                format!(
                    "accepted {} {} in {}",
                    applied,
                    if applied == 1 { "hunk" } else { "hunks" },
                    rel_text(&edit.note)
                ),
                cx,
            );
        } else {
            self.say(
                format!(
                    "accepted {applied} of {} hunks in {}: {stale} no longer fits",
                    edit.hunks.len(),
                    rel_text(&edit.note)
                ),
                cx,
            );
        }
        self.after_queue_change(window, cx);
    }

    /// Reject the hunk the marker is on (`x`): the proposal is taken out of the queue
    /// and the note is left exactly as it is. Nothing is written, in either direction.
    pub(super) fn review_reject(
        &mut self,
        _: &ReviewReject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Same rule as accept: from the note, the hunk under the cursor (roadmap P2.5b).
        self.aim_marker_at_cursor(window, cx);
        let Some(row) = self.review.read(cx).current_row() else {
            return;
        };
        let edit = self.review.read(cx).edit(&row).clone();
        let left = match row.hunk {
            // A collapsed row is the whole proposal: the file goes.
            None => None,
            Some(index) => pending::without_hunk(&edit, index),
        };
        let outcome = self.write_queue_file(&edit, left.as_ref());
        match outcome {
            Ok(()) => self.say(
                format!(
                    "rejected {} in {}",
                    match row.hunk {
                        Some(_) => "a hunk",
                        None => "a proposal",
                    },
                    rel_text(&edit.note)
                ),
                cx,
            ),
            Err(err) => {
                tracing::warn!(path = %edit.note.display(), "could not update the queue: {err}");
                self.say(format!("could not update the queue: {err}"), cx);
            }
        }
        self.after_queue_change(window, cx);
    }

    /// What has to happen after the queue changed under the user's hands: the strip
    /// re-read, the window repainted, and the keyboard handed back to the note if the
    /// strip has nothing left to show.
    ///
    /// That last part is not politeness. A strip that disappears while it has the
    /// keyboard leaves the keyboard nowhere, and the next keystroke goes to nothing.
    fn after_queue_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.scan_pending(cx);
        if self.review.read(cx).is_empty() {
            let focus = self.active_focus(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Put the chosen hunks of one proposal into the note, then put what is left of the
    /// proposal back into its file.
    ///
    /// `only` is `Some(hunk)` for a hunk row and `None` for a collapsed proposal's row.
    /// Returns how many hunks landed and how many were stale.
    ///
    /// The note is the **buffer** when it is open, because what the user can see is what
    /// a hunk has to fit and an accept should be undoable with one `u`; it is the
    /// **disk** when the note is not open, because there is no buffer to show it in.
    fn apply_pending(
        &mut self,
        edit: &PendingEdit,
        only: Option<usize>,
        cx: &mut Context<Self>,
    ) -> (usize, usize) {
        let open = self.tab_for_path(&edit.note);
        let mut current = self.note_text(&edit.note, cx);
        let mut left = edit.clone();
        let mut applied = 0;
        let mut stale = 0;
        // Whether the last hunk was the one just accepted, which is when the file
        // itself goes rather than being rewritten with what is left.
        let mut emptied = false;

        // Always the hunk at `at`, because taking one out leaves the next where it was.
        let mut at = only.unwrap_or(0);
        while at < left.hunks.len() {
            let hunk = left.hunks[at].clone();
            let next = match diff::apply(&current, &hunk) {
                diff::Outcome::Stale => {
                    stale += 1;
                    at += 1;
                    continue;
                }
                diff::Outcome::Applied(next) => next,
            };
            if let Some(index) = open {
                if !self.patch_buffer(index, &current, &hunk, cx) {
                    // `apply` said it fits, so `region` cannot disagree; if it ever did,
                    // stopping here is the honest answer rather than a half-applied edit.
                    stale += 1;
                    break;
                }
            }
            current = next;
            applied += 1;
            left = match pending::without_hunk(&left, at) {
                Some(rest) => rest,
                None => {
                    emptied = true;
                    break;
                }
            };
            if only.is_some() {
                break;
            }
        }

        if open.is_none() && applied > 0 {
            if let Err(err) = self.write_unopened_note(&edit.note, &current) {
                tracing::warn!(path = %edit.note.display(), "could not write the accepted note: {err:#}");
                self.say(format!("could not write the note: {err:#}"), cx);
                // The proposal stays queued: the user can try again, and the note on
                // disk is unchanged.
                return (0, stale);
            }
            self.refresh_graph(cx);
        }

        // Three answers, and the difference between the last two matters: a proposal
        // whose hunks are all gone leaves no file, and one that nothing was accepted
        // from keeps its file exactly as it was.
        let left = if applied == 0 {
            Some(edit)
        } else if emptied {
            None
        } else {
            Some(&left)
        };
        match self.write_queue_file(edit, left) {
            Ok(()) => {}
            Err(err) => {
                // The note has the change; the queue file did not move. Said rather
                // than swallowed, because the alternative is the same hunk offered
                // again after an accept that worked.
                tracing::warn!(path = %edit.note.display(), "could not update the queue: {err}");
                self.say(format!("accepted, but the queue did not update: {err}"), cx);
            }
        }
        (applied, stale)
    }

    /// One accepted hunk into the open buffer, as one undo step.
    ///
    /// The byte range comes from [`diff::region`] rather than a whole-note rewrite, so
    /// the cursor, the undo history and everything the user has not saved survive.
    fn patch_buffer(
        &mut self,
        index: usize,
        current: &str,
        hunk: &Hunk,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((start, end, text)) = diff::region(current, hunk) else {
            return false;
        };
        let Some(editor) = self.pane().tabs[index].editor().cloned() else {
            // A graph tab never holds a note, so this is only the compiler's company.
            return false;
        };
        editor.update(cx, |editor, cx| {
            editor.apply_edits(&[(start, end, text)], cx)
        })
    }

    /// Write a note the window is not showing, making the folder a new note needs, and
    /// tell the index which of the two happened.
    fn write_unopened_note(&mut self, note: &Path, text: &str) -> anyhow::Result<()> {
        let existed = self.vault.exists(note);
        if let Some(parent) = self.vault.resolve(note)?.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| anyhow::anyhow!("creating {}: {err}", parent.display()))?;
        }
        self.vault.write(note, text)?;
        if existed {
            self.index_saved(note);
        } else {
            self.index_created(note);
        }
        // The graph is kept current from the window, and an accepted proposal is one
        // of the ways a note changes without the watcher noticing.
        self.graph_note_saved(note, text);
        Ok(())
    }

    /// Put what is left of a proposal back, or take it out of the queue when nothing is
    /// left. `left` is `None` for "the whole proposal is gone".
    fn write_queue_file(
        &mut self,
        edit: &PendingEdit,
        left: Option<&PendingEdit>,
    ) -> Result<(), String> {
        match left {
            None => pending::remove(self.vault.root(), &edit.id).map_err(|err| format!("{err:#}")),
            Some(left) if left.hunks.len() < edit.hunks.len() => {
                pending::write(self.vault.root(), left)
                    .map(|_| ())
                    .map_err(|err| format!("{err:#}"))
            }
            // Nothing was taken out, so the file is already what it should be.
            Some(_) => Ok(()),
        }
    }

    /// Whether the note is in the view that refuses edits — in whichever tab holds
    /// it, since a background tab is still a tab the user will bring forward.
    fn note_is_preview(&self, note: &Path, cx: &gpui::App) -> bool {
        self.tab_for_path(note).is_some_and(|index| {
            self.pane().tabs[index].editor().is_some_and(|editor| {
                editor.read_with(cx, |editor, _cx| editor.view()) == View::Preview
            })
        })
    }

    /// The strip's rows, for the tests: what the user can see, in the order they can
    /// see it, with the badge each row carries.
    #[cfg(test)]
    pub(crate) fn review_rows(&self, cx: &gpui::App) -> Vec<String> {
        let review = self.review.read(cx);
        review
            .drawn()
            .map(|n| {
                let rows = review.rows();
                let row = rows[n];
                let edit = review.edit(&row);
                let hunk = row.hunk.unwrap_or(0);
                let badge = match row.hunk {
                    None => " collapsed",
                    Some(_) if review.is_stale(row.edit, hunk) => " stale",
                    Some(_) if review.is_clashing(row.edit, hunk) => " clash",
                    Some(_) => "",
                };
                format!(
                    "{}{}{badge}",
                    rel_text(&edit.note),
                    match row.hunk {
                        Some(h) => format!(" hunk {h}"),
                        None => String::new(),
                    }
                )
            })
            .collect()
    }

    /// How many hunks are waiting, for the tests that build a queue long enough to
    /// need the strip's cap.
    #[cfg(test)]
    pub(crate) fn review_hunks(&self, cx: &gpui::App) -> usize {
        self.review.read(cx).hunk_count()
    }

    /// The marker, as `(note, hunk)`, for the tests that check where `j`/`k` and
    /// `]a`/`[a` moved it.
    #[cfg(test)]
    pub(crate) fn review_marker(&self, cx: &gpui::App) -> Option<(String, Option<usize>)> {
        let review = self.review.read(cx);
        let row = review.current_row()?;
        Some((rel_text(&review.edit(&row).note), row.hunk))
    }
}

/// Whether two hunks are proposed over the same lines of the same note.
fn overlaps(a: &Hunk, b: &Hunk) -> bool {
    a.old_start < b.old_start + b.old_len && b.old_start < a.old_start + a.old_len
}
