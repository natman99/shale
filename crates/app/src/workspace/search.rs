//! `/` — searching the note that is open.
//!
//! Vault search (`ctrl-shift-f`) finds *notes*; this finds *places in the note you
//! are reading*, which is what a reader does with `/`. It is literal rather than
//! fuzzy, and case follows vim's "smart case" (see [`shale_core::search::find_all`]).
//!
//! The field is the ordinary one-line [`crate::prompt::Prompt`], reused with
//! [`prompt::PromptKind::Search`]: it already knows how to take printable text,
//! `enter` and `escape`. What the search adds is that it answers as the query is
//! typed — the field emits [`prompt::PromptEvent::Changed`] — and that the query
//! and its matches live here in the window rather than in the field, so `n` and
//! `N` keep working after the field has closed.

use gpui::{Context, Entity, SharedString, Window};
use shale_core::search::find_all;

use crate::acts::{SearchBuffer, SearchNext, SearchPrev};
use crate::prompt;

use super::Workspace;

/// Where the `/` search stands: what was typed, what it matched, and which of
/// those matches the cursor is on.
#[derive(Default)]
pub(crate) struct SearchState {
    /// The query. Kept after the field closes, which is what lets `n` and `N`
    /// repeat a search the user finished with `enter`.
    pub(crate) query: String,
    /// Every match in the note, as buffer byte ranges, in the order they appear.
    /// Empty while no search is up.
    pub(crate) hits: Vec<(usize, usize)>,
    /// Which match the cursor is on, as an index into [`Self::hits`]. `None`
    /// while there are no matches.
    pub(crate) index: Option<usize>,
}

impl SearchState {
    /// The match the cursor is on, as a buffer byte range.
    fn current(&self) -> Option<(usize, usize)> {
        self.index.and_then(|index| self.hits.get(index).copied())
    }
}

impl Workspace {
    /// `/` — open the search field at the bottom of the note, with the last query
    /// already in it, so narrowing a search is a few keystrokes and repeating one
    /// is `enter`.
    pub(super) fn search_buffer(
        &mut self,
        _: &SearchBuffer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial = self.search.query.clone();
        self.open_prompt(prompt::PromptKind::Search, "/", &initial, window, cx);
    }

    /// A keystroke in the search field: re-run the search and put the cursor on
    /// the first match, which is what vim does while the query is still being
    /// typed.
    pub(super) fn search_changed(&mut self, query: &str, cx: &mut Context<Self>) {
        self.search.query = query.to_string();
        self.search.index = None;
        self.recount_hits(cx);
        if let Some(hit) = self.search.current() {
            self.editor
                .update(cx, |editor, cx| editor.reveal_offset(hit.0, cx));
        }
        cx.notify();
    }

    /// `enter` in the search field: the field closes and the matches stay, so `n`
    /// and `N` can walk them. Vim's rule — a search outlives its command line.
    pub(super) fn search_submitted(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.query = query.to_string();
        self.search.index = None;
        self.recount_hits(cx);
        if let Some(hit) = self.search.current() {
            self.editor
                .update(cx, |editor, cx| editor.reveal_offset(hit.0, cx));
        }
        self.close_prompt(window, cx);
    }

    /// `escape` in the search field: the search is abandoned outright, matches and
    /// query both, so the note is left as it was found.
    pub(super) fn search_cancelled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_search(cx);
        self.close_prompt(window, cx);
    }

    /// Forget the search: no query, no matches, nothing drawn. Also what opening
    /// a different note does, because the matches were offsets into the old one.
    pub(super) fn clear_search(&mut self, cx: &mut Context<Self>) {
        if self.search.query.is_empty() && self.search.hits.is_empty() {
            return;
        }
        self.search = SearchState::default();
        self.paint_hits(cx);
        cx.notify();
    }

    /// The note was edited while a search was up, so the matches have to be re-run
    /// against the new text: a byte offset that pointed at a match now points at
    /// whatever moved into its place.
    pub(super) fn refresh_search(&mut self, cx: &mut Context<Self>) {
        if self.search.query.is_empty() {
            return;
        }
        self.recount_hits(cx);
        cx.notify();
    }

    /// `n`: the next match, wrapping at the end of the note.
    pub(super) fn search_next(
        &mut self,
        _: &SearchNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_search(1, cx);
    }

    /// `N`: the previous match, wrapping at the top.
    pub(super) fn search_prev(
        &mut self,
        _: &SearchPrev,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_search(-1, cx);
    }

    /// Move the current match by `delta`, wrapping, and take the cursor with it.
    fn step_search(&mut self, delta: i64, cx: &mut Context<Self>) {
        if self.search.hits.is_empty() {
            if self.search.query.is_empty() {
                self.status = "nothing searched for yet".into();
            }
            // Otherwise the status bar already says "no match for …"; there is
            // nothing to move to, and the cursor stays where it is.
            cx.notify();
            return;
        }
        let len = self.search.hits.len() as i64;
        let at = self.search.index.unwrap_or(0) as i64;
        let next = (at + delta).rem_euclid(len) as usize;
        self.search.index = Some(next);
        let hit = self.search.hits[next];
        self.editor
            .update(cx, |editor, cx| editor.reveal_offset(hit.0, cx));
        self.paint_hits(cx);
        cx.notify();
    }

    /// Re-run the query against the note as it is now, keeping the current match
    /// when it still exists and clamping to the last one when the note got shorter
    /// under it.
    fn recount_hits(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text();
        let hits = find_all(&text, &self.search.query);
        self.search.index = if hits.is_empty() {
            None
        } else {
            Some(self.search.index.unwrap_or(0).min(hits.len() - 1))
        };
        self.search.hits = hits;
        self.paint_hits(cx);
    }

    /// Hand the matches to the editor to draw, with the current one marked.
    fn paint_hits(&mut self, cx: &mut Context<Self>) {
        let hits = self.search.hits.clone();
        let active = self.search.current();
        self.editor
            .update(cx, |editor, cx| editor.set_hits(hits, active, cx));
    }

    /// The status bar's readout: `3/17` while a search is up, `no match for x`
    /// when the query found nothing, and nothing at all when there is no search.
    pub(crate) fn search_label(&self) -> SharedString {
        if self.search.query.is_empty() {
            return String::new().into();
        }
        match self.search.hits.len() {
            0 => format!("no match for {}", self.search.query).into(),
            n => format!("{}/{n}", self.search.index.map_or(0, |index| index + 1)).into(),
        }
    }

    /// What the field's text changing means. Only the search answers as it is
    /// typed; the prompts that name something wait for an answer.
    pub(super) fn prompt_changed(
        &mut self,
        prompt: &Entity<prompt::Prompt>,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if prompt.read(cx).kind() == prompt::PromptKind::Search {
            self.search_changed(text, cx);
        }
    }

    /// `escape` in a field: nothing was answered. A search is abandoned outright —
    /// its matches go with the field — while the prompts that name something just
    /// close.
    pub(super) fn cancel_prompt(
        &mut self,
        prompt: &Entity<prompt::Prompt>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if prompt.read(cx).kind() == prompt::PromptKind::Search {
            self.search_cancelled(window, cx);
            return;
        }
        self.close_prompt(window, cx);
    }

    /// The search as the window holds it. Only the tests ask, to check what is
    /// highlighted and where in the matches the cursor stands.
    #[cfg(test)]
    pub(crate) fn search_state(&self) -> &SearchState {
        &self.search
    }
}
