//! Tabs: one note per tab, and the editor that holds it (roadmap P3.1).
//!
//! **The editor belongs to the tab.** The alternative — one editor whose text is
//! swapped when the tab changes — is less code and worse behaviour: the cursor, the
//! scroll, the folds, the view the note is in and the undo history all live in the
//! editor, so swapping text would undo *another note's* edits when the user pressed
//! `u`. A tab gets its own [`MarkdownEditor`] and keeps everything the user made in
//! it, which is also what makes a per-tab view (this tab raw, that one preview) fall
//! out for free rather than needing a second table of per-note state.
//!
//! **A tab is a note, and opening a note that is already open goes to its tab.**
//! That is Obsidian's rule, and it is what stops following three links from making
//! three tabs. New tabs come from being asked for: `ctrl-t` / `:tabnew` on the note
//! in hand, or a middle click on a tab's `×`… which closes it. Two tabs may hold the
//! same note — that is the "read the top of this note while I edit the bottom of it"
//! case, and it is why the tab's identity is an id rather than its path.
//!
//! **There is always at least one tab.** Closing the last one leaves an empty tab
//! (no path, no text) rather than an empty window, which is what a window on an empty
//! vault starts with too. That invariant is what lets [`Workspace::editor`] be total
//! instead of optional, and it is why closing a tab is "remove it, or clear it if it
//! was the only one".
//!
//! Not here yet, and deliberately: per-tab jump chains and per-tab checker answers
//! (both are window-level today, and P3.2's panes will want the same decision), and
//! a tab bar that scrolls when there are more tabs than fit — they share the width
//! and ellipsis instead.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui::{prelude::*, Context, Entity, Focusable, SharedString, Subscription, Task, Window};
use shale_core::watch::VaultChanges;

use crate::acts::{OpenGraph, TabClose, TabNew, TabNext, TabOnly, TabPrev};
use crate::editor::{EditorEvent, MarkdownEditor, View};

use super::Workspace;

/// How often the graph's beat runs: one frame at 60 fps, which is what makes the
/// layout look like motion rather than a slideshow.
const TICK: Duration = Duration::from_millis(16);

/// Sim steps per beat. Two, so a first graph settles in a couple of seconds while
/// still reading as continuous motion; one would take about five.
const TICKS_PER_BEAT: usize = 2;

/// The most a test may ask the layout to settle in one go. The sim cools
/// geometrically, so ~300 ticks is the real answer and this is only the guard rail.
#[cfg(test)]
const SETTLE_BOUND: usize = 1000;

/// One tab: a note, and the editor holding it — or the vault's graph.
pub(crate) struct Tab {
    /// Stable identity. A tab's *index* moves when another tab closes, and the
    /// editor's subscription outlives that, so the subscription carries the id and
    /// the window looks the tab up by it.
    pub(crate) id: usize,
    /// What the tab holds.
    pub(crate) content: TabContent,
    /// Bumped by every edit in this tab. The debounce below writes only if the number
    /// it was started with is still the current one, so a burst of typing becomes one
    /// write at the end of the pause. Per tab, because a note typed in one tab must
    /// not cancel another tab's pending write. Unused by the graph tab, which has no
    /// note to write.
    pub(crate) autosave_seq: u64,
    /// This tab's autosave countdown. Replacing it drops the old one.
    pub(crate) _autosave: Option<Task<()>>,
}

/// What a tab holds. The graph tab is why [`Workspace::editor`] is optional: with the
/// graph on screen the checker, the `/` search, the dirty dot and the word badge have
/// nothing to act on, and saying so is better than a hidden empty editor pretending
/// there is a note (roadmap P3.4, integration contract).
pub(crate) enum TabContent {
    /// A note, and the editor holding it.
    Note {
        /// The note this tab holds. `None` is the empty tab: what closing the last tab
        /// leaves behind, and what a window over an empty vault starts with.
        path: Option<PathBuf>,
        /// The editor. Owned by the tab, so the cursor, scroll, folds, view and undo
        /// history are the ones the user left in this tab.
        editor: Entity<MarkdownEditor>,
        /// The subscription that carries this editor's events to the window. Alive for
        /// as long as the tab is (see [`Workspace::editor_event`]).
        _events: Subscription,
    },
    /// The vault's link graph. **One per window**: it is a simulation, and a second
    /// view of the same vault wants the same positions, not its own physics.
    Graph(Entity<crate::graph::GraphView>),
}

impl Tab {
    /// The note this tab holds. `None` for an empty note tab, and for the graph.
    pub(crate) fn path(&self) -> Option<&Path> {
        match &self.content {
            TabContent::Note { path, .. } => path.as_deref(),
            TabContent::Graph(_) => None,
        }
    }

    /// The tab's editor, or `None` when the tab is the graph.
    pub(crate) fn editor(&self) -> Option<&Entity<MarkdownEditor>> {
        match &self.content {
            TabContent::Note { editor, .. } => Some(editor),
            TabContent::Graph(_) => None,
        }
    }

    /// Whether this tab is the graph.
    pub(crate) fn is_graph(&self) -> bool {
        matches!(self.content, TabContent::Graph(_))
    }
}

impl Workspace {
    /// The editor of the tab on screen, or `None` when that tab is the graph. Every
    /// part of the window that talks to the note goes through this rather than holding
    /// an editor of its own, which is what keeps "the note" meaning "the note in this
    /// tab" everywhere at once — and what lets a graph tab simply have no answer.
    pub(crate) fn editor(&self) -> Option<&Entity<MarkdownEditor>> {
        self.tabs[self.tab].editor()
    }

    /// The face the note is drawn in, which every panel that types text matches so a
    /// typed name looks like the text it will become. With the graph tab on screen
    /// there is no note to match, and the window's configured font is the nearest
    /// thing to it.
    pub(crate) fn note_font(&self, cx: &gpui::App) -> (SharedString, f32) {
        match self.editor() {
            Some(editor) => (editor.read(cx).font_family(), editor.read(cx).font_size()),
            None => (
                self.config.appearance.font_family.clone().into(),
                self.config.appearance.font_size,
            ),
        }
    }

    /// The note the command acts on, or a message saying there is none.
    ///
    /// The graph tab has no note, and a command that quietly did nothing reads as a
    /// binding that is broken. Saying "no note open" is the integration contract's
    /// answer for every command that needs a buffer (`graph-view-plan.md`).
    pub(crate) fn note_editor(&mut self, cx: &mut Context<Self>) -> Option<Entity<MarkdownEditor>> {
        match self.editor().cloned() {
            Some(editor) => Some(editor),
            None => {
                self.say("no note open", cx);
                cx.notify();
                None
            }
        }
    }

    /// The note the tab on screen holds, or `None` for an empty tab or the graph.
    pub(crate) fn note(&self) -> Option<PathBuf> {
        self.tabs[self.tab].path().map(|path| path.to_path_buf())
    }

    /// Which tab holds `path`, if one does. This is what makes opening a note that is
    /// already open a tab change rather than a second copy of the note.
    pub(super) fn tab_for_path(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.path() == Some(path))
    }

    /// Which tab has this id, for the subscriptions that captured one.
    pub(super) fn tab_for_id(&self, id: usize) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// Add a tab after the one on screen and make it the active one. The editor is
    /// empty and pathless when it arrives: what it holds is decided by the caller,
    /// which is also the point at which the note's text is read.
    pub(super) fn add_tab(&mut self, cx: &mut Context<Self>) -> usize {
        let id = self.tab_seq;
        self.tab_seq += 1;
        // The view the user is working in, which is what a tab opened from a note in
        // raw mode is opened in. There is no tab to ask yet only at startup, where
        // the editor's own default is the right answer.
        let view = self
            .tabs
            .get(self.tab)
            .and_then(|tab| tab.editor())
            .map(|editor| editor.read(cx).view())
            .unwrap_or(View::Live);
        let content = self.note_content(id, view, cx);
        let index = if self.tabs.is_empty() {
            0
        } else {
            self.tab + 1
        };
        self.tabs.insert(
            index,
            Tab {
                id,
                content,
                autosave_seq: 0,
                _autosave: None,
            },
        );
        self.tab = index;
        self.mark_note_mru();
        // The new tab is the drawn one, so both the pointer's subscription and the
        // keyboard have to come here; neither path has a window, so the next frame is
        // asked for both.
        self.hover_stale = true;
        self.focus_stale = true;
        index
    }

    /// An empty note tab's content: a fresh editor, wired to the window exactly as
    /// [`add_tab`] wires one, and subscribed under `id`.
    ///
    /// Split out because a tab can *become* this without changing its id, which is what
    /// closing the last tab does to a graph tab: the subscriptions that captured the id
    /// keep pointing at the same tab.
    fn note_content(&self, id: usize, view: View, cx: &mut Context<Self>) -> TabContent {
        let editor = cx.new(|cx| MarkdownEditor::new(cx));
        // Everything the window knows that an editor cannot discover for itself —
        // the vault, the attachments, the link names, the font, the palette — is
        // applied here, so a tab opened at any point in the session starts with the
        // settings the session is already using.
        editor.update(cx, |editor, cx| {
            editor.set_vault(Rc::clone(&self.vault), self.attachments.clone(), cx);
            editor.set_known_links(self.known.clone(), cx);
            editor.set_palette(Rc::clone(&self.palette), cx);
            editor.set_font_size(self.config.appearance.font_size, cx);
            editor.set_font_family(&self.config.appearance.font_family, cx);
            editor.set_view(view, cx);
        });
        let events = cx.subscribe(&editor, move |ws, _editor, event, cx| {
            ws.editor_event(id, event, cx)
        });
        TabContent::Note {
            path: None,
            editor,
            _events: events,
        }
    }

    /// The window's graph view, if any tab holds one. There is at most one graph tab
    /// per window (see [`TabContent::Graph`]), so this is a search rather than a table.
    pub(super) fn graph_view(&self) -> Option<&Entity<crate::graph::GraphView>> {
        self.tabs.iter().find_map(|tab| match &tab.content {
            TabContent::Graph(view) => Some(view),
            TabContent::Note { .. } => None,
        })
    }

    /// Add the window's graph tab after the one on screen, and make it the active one.
    ///
    /// Unlike [`add_tab`] there is no editor event to subscribe to: the graph changes
    /// because the vault changed, and the window is the one that noticed.
    fn add_graph_tab(&mut self, cx: &mut Context<Self>) -> usize {
        let id = self.tab_seq;
        self.tab_seq += 1;
        // The labels are drawn in the face the rest of the window types in, so a
        // node's label looks like the note title it names.
        let (family, size) = self.note_font(cx);
        let view =
            cx.new(|cx| crate::graph::GraphView::new(Rc::clone(&self.palette), family, size, cx));
        let index = if self.tabs.is_empty() {
            0
        } else {
            self.tab + 1
        };
        self.tabs.insert(
            index,
            Tab {
                id,
                content: TabContent::Graph(view),
                autosave_seq: 0,
                _autosave: None,
            },
        );
        self.tab = index;
        // The graph takes the pointer's subscription away: there is no note under it
        // to hover, and the keyboard has to land on the graph's own key context.
        self.hover_stale = true;
        self.focus_stale = true;
        index
    }

    /// Records which note tab the frame is on, which is what the graph tab's "where
    /// does a note open instead?" rule needs (see [`Workspace::note_tab`]). Called
    /// wherever `self.tab` is assigned.
    pub(super) fn mark_note_mru(&mut self) {
        if !self.tabs[self.tab].is_graph() {
            self.note_mru = Some(self.tabs[self.tab].id);
        }
    }

    /// The tab a note should open into, making it the one on screen.
    ///
    /// The graph tab is never the answer: it is a view over the whole vault, and a
    /// click on a link that turned it into a note would be the graph vanishing. So a
    /// note opened while the graph is on screen goes to the note tab the frame was
    /// most recently on, and to a new tab when the window holds none (integration
    /// contract, `graph-view-plan.md`).
    pub(super) fn note_tab(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.tabs[self.tab].is_graph() {
            return self.tab;
        }
        let index = self
            .note_mru
            .and_then(|id| self.tab_for_id(id))
            .unwrap_or_else(|| self.add_tab(cx));
        self.show_tab(index, cx);
        index
    }

    /// The counts the graph panel shows, read off the window's index. `None` when the
    /// index has never been built.
    fn graph_summary(&self) -> Option<crate::graph::GraphSummary> {
        use shale_core::graph::NodeKind;
        let index = self.graph.as_ref()?;
        let (mut notes, mut ghosts, mut tags) = (0usize, 0usize, 0usize);
        for node in index.graph().node_indices() {
            match index.meta(node).kind {
                NodeKind::Note | NodeKind::Attachment => notes += 1,
                NodeKind::Ghost => ghosts += 1,
                NodeKind::Tag => tags += 1,
            }
        }
        Some(crate::graph::GraphSummary {
            notes,
            ghosts,
            tags,
            // The graph is undirected and never holds parallel edges, so this is
            // already "A→B and B→A counted once".
            links: index.graph().edge_count(),
        })
    }

    /// Re-send the graph to the view: the counts, and the picture.
    ///
    /// Called when the index is built and whenever a note moves under it. The sim is
    /// re-slotted here rather than at every index call site, because "the graph
    /// changed" and "show the graph" are the same moment, and [`GraphSim::build`] is
    /// what decides whether that means a re-heat or nothing at all (a prose edit that
    /// changes no links moves nothing).
    pub(crate) fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        let Some(summary) = self.graph_summary() else {
            return;
        };
        let Some(view) = self.graph_view().cloned() else {
            return;
        };
        let previous = self.graph_sim.take();
        if let Some(index) = self.graph.as_ref() {
            self.graph_sim = Some(crate::graph::GraphSim::build(index, previous.as_ref()));
        }
        let frame = self
            .graph_sim
            .as_ref()
            .map(|sim| Rc::new(sim.frame()))
            .unwrap_or_else(|| Rc::new(crate::graph::GraphFrame::empty()));
        view.update(cx, |view, cx| {
            view.set_summary(summary, cx);
            view.set_frame(frame, cx);
        });
        self.pump_graph(cx);
    }

    /// Start the sim's beat if it has work to do.
    ///
    /// One task, ticking on the main executor: the sim is pure arithmetic over a few
    /// thousand numbers, so a thread of its own would buy nothing and cost a
    /// synchronization point per frame. The beat ends by itself when the layout settles
    /// or the tab closes, which is the idling rule — with the sim settled and drift
    /// off (M5g, not built), the window must draw zero frames.
    fn pump_graph(&mut self, cx: &mut Context<Self>) {
        if self.graph_beat.is_some() {
            return;
        }
        if self.graph_sim.as_ref().is_none_or(|sim| sim.settled()) {
            return;
        }
        self.graph_beat = Some(cx.spawn(async move |ws, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                // The weak handle: a window that is gone has no graph to lay out, and
                // `graph_tick` says so when the sim has settled or the tab has closed.
                let moved = ws.update(cx, |ws, cx| ws.graph_tick(cx)).unwrap_or(false);
                if !moved {
                    break;
                }
            }
        }));
    }

    /// One beat of the layout: tick the sim, ship the new picture, and say whether
    /// anything moved (which is whether another beat is owed).
    fn graph_tick(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.tabs.iter().any(|tab| tab.is_graph()) {
            self.graph_beat = None;
            return false;
        }
        let Some(sim) = self.graph_sim.as_mut() else {
            self.graph_beat = None;
            return false;
        };
        let moved = sim.advance(TICKS_PER_BEAT);
        if moved {
            let frame = Rc::new(sim.frame());
            if let Some(view) = self.graph_view().cloned() {
                view.update(cx, |view, cx| view.set_frame(frame, cx));
            }
            cx.notify();
        } else {
            self.graph_beat = None;
        }
        moved
    }

    /// Drive the layout to rest, synchronously. The tests' door: nothing here waits on
    /// a timer or a thread, so a headless test can hold an exactly-settled graph and
    /// assert on what it paints.
    #[cfg(test)]
    pub(crate) fn settle_graph(&mut self, cx: &mut Context<Self>) {
        self.graph_beat = None;
        let Some(sim) = self.graph_sim.as_mut() else {
            return;
        };
        // `advance` stops at settle on its own; the bound is only so a test cannot
        // hang on a sim that refuses to cool.
        sim.advance(SETTLE_BOUND);
        let frame = Rc::new(sim.frame());
        if let Some(view) = self.graph_view().cloned() {
            view.update(cx, |view, cx| view.set_frame(frame, cx));
        }
    }

    /// Records which note tab the frame is on, which is what the graph tab's "where
    /// Fold one set of disk changes into the graph, if it has been built.
    ///
    /// `set_note` and `remove_note` each rebuild the arena, so a burst of changes costs
    /// one rebuild per changed note. That is cheap enough at the vault sizes the graph
    /// is useful at, and it is what the integration contract's "incremental now,
    /// DB-backed extraction deferred" ruling covers.
    pub(super) fn graph_changes(&mut self, changes: &VaultChanges) {
        let Some(index) = self.graph.as_mut() else {
            return;
        };
        for removed in &changes.removed {
            index.remove_note(removed);
        }
        for path in changes.added.iter().chain(changes.modified.iter()) {
            match self.vault.read(path) {
                Ok(text) => {
                    index.set_note(path, &text, &self.vault, &self.entries, &self.attachments)
                }
                Err(err) => tracing::warn!(path = %path.display(), "graph re-scan failed: {err:#}"),
            }
        }
    }

    /// The graph follows a note Shale wrote itself. The watcher is stamped over our own
    /// saves, so this is the only way a link typed in the editor reaches the graph.
    pub(crate) fn graph_note_saved(&mut self, path: &Path, text: &str) {
        if let Some(index) = self.graph.as_mut() {
            index.set_note(path, text, &self.vault, &self.entries, &self.attachments);
        }
    }

    /// A note that is gone leaves the graph — and the ghosts and tags only it linked to
    /// leave with it.
    pub(crate) fn graph_note_removed(&mut self, path: &Path) {
        if let Some(index) = self.graph.as_mut() {
            index.remove_note(path);
        }
    }

    /// `space g` / `:graph`: put the vault's graph on screen.
    ///
    /// A second call moves the frame to the tab that already holds the graph rather
    /// than opening another: the graph is one simulation per window, and a second tab
    /// would be a second view of the same physics with its own camera to keep in step
    /// (integration contract, `graph-view-plan.md`).
    ///
    /// The index is built on the first call and kept current after that, because
    /// building it reads every note in the vault and a window that never opens the
    /// graph should never pay for it.
    pub(crate) fn open_graph(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.is_graph()) {
            self.show_tab(index, cx);
            return;
        }
        if self.graph.is_none() {
            match shale_core::graph::GraphIndex::build(
                &self.vault,
                &self.entries,
                &self.attachments,
            ) {
                Ok(index) => self.graph = Some(index),
                Err(err) => {
                    // The same posture as a checker that will not start: the graph is
                    // a view of the notes, not another copy of them, so a failed build
                    // is a message rather than a broken window.
                    self.say(format!("graph failed: {err:#}"), cx);
                    cx.notify();
                    return;
                }
            }
        }
        self.add_graph_tab(cx);
        self.refresh_graph(cx);
        // The same report a tab change makes: the frame moved to something that is not
        // a note, and the status bar is where that is said.
        self.say("graph", cx);
    }

    /// The action's door: `space g`, `:graph`, the palette row and the MCP surface all
    /// reach [`Workspace::open_graph`] this way.
    ///
    /// The keyboard comes here now rather than waiting for the next frame, as it does
    /// for `ctrl-t`: the graph has a key context of its own, and until the frame is on
    /// it its bindings — `escape` among them — are not the active ones.
    pub(super) fn show_graph(
        &mut self,
        _: &OpenGraph,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_graph(cx);
        self.focus_active_tab(window, cx);
    }

    /// What the window does with an editor's news, which it can only decide by asking
    /// *which* tab that editor belongs to: an edit in the tab on screen re-checks the
    /// note and restarts its autosave, and an edit in a tab nobody is looking at only
    /// has to reach the disk.
    pub(super) fn editor_event(&mut self, id: usize, event: &EditorEvent, cx: &mut Context<Self>) {
        let Some(index) = self.tab_for_id(id) else {
            // The tab closed between the event and this call. Nothing to do: the
            // editor is on its way out with the tab.
            return;
        };
        let active = index == self.tab;
        match event {
            EditorEvent::FollowLink { target } => {
                // Only the tab on screen can be clicked or `gx`-ed, but a link
                // followed in a tab that is not on screen would otherwise edit the
                // wrong note, so it is brought on screen first.
                if !active {
                    self.show_tab(index, cx);
                }
                self.follow_link(target, cx);
            }
            EditorEvent::Changed => {
                self.queue_autosave(index, cx);
                if active {
                    // The checker's copy of the note, and the search's byte offsets,
                    // both belong to the note on screen.
                    self.send_note_to_checker(cx);
                    self.refresh_search(cx);
                }
            }
            // Hover is answered by the subscription `attach_window` keeps on the tab
            // that is on screen, which is the one that has a window to act on.
            EditorEvent::Hovered { .. } => {}
        }
        cx.notify();
    }

    /// Make tab `index` the one on screen. The note itself is already loaded — that is
    /// what a per-tab editor means — so what a tab change owes is the same cleanup
    /// opening a note does: a popup and a search belong to the note they were made on.
    pub(super) fn show_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.tab || index >= self.tabs.len() {
            return;
        }
        self.drop_popup(cx);
        self.clear_search(cx);
        self.tab = index;
        self.mark_note_mru();
        // The pointer can only hover over the note that is drawn, so the hover
        // subscription has to move with the frame. It needs a window, and this path
        // does not always have one, so the next frame is asked to do it.
        self.hover_stale = true;
        // For the same reason, the keyboard: the node that holds it is the drawn tab's
        // editor, and the entity it is sitting on is no longer in the tree.
        self.focus_stale = true;
        // The checker follows the note on screen, so its answers are the ones the
        // user can act on. `checker_opened` sends the new note and clears the old
        // note's diagnostics.
        if let Some(path) = self.note() {
            let text = self
                .editor()
                .map(|editor| editor.read_with(cx, |editor, _cx| editor.text()))
                .unwrap_or_default();
            self.checker_opened(&path, &text, cx);
            self.say(path.display().to_string(), cx);
        } else {
            self.checker_closed(cx);
            // The graph tab has no note to name, so the bar says what is on screen.
            if self.tabs[index].is_graph() {
                self.say("graph", cx);
            }
        }
        cx.notify();
    }

    /// Hand the keyboard to the tab on screen.
    ///
    /// A tab owns what it shows — a note tab its editor, the graph tab its view — so
    /// the node the keystrokes reach is the drawn tab's. After a tab change the focus
    /// tracker still points at the entity that is no longer drawn, and every keystroke
    /// after it is lost — which is why every path that moves the frame marks this owed,
    /// and the next frame pays it.
    pub(crate) fn focus_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_stale = false;
        let focus = self.active_focus(cx);
        window.focus(&focus, cx);
    }

    /// The focus handle of the tab on screen: a note tab's editor, the graph tab's
    /// view. Every path that owes the keyboard to "wherever the user was working" asks
    /// for this rather than assuming it was the note.
    pub(super) fn active_focus(&self, cx: &mut Context<Self>) -> gpui::FocusHandle {
        match &self.tabs[self.tab].content {
            TabContent::Note { editor, .. } => editor.read(cx).focus_handle(cx),
            TabContent::Graph(view) => view.read(cx).focus_handle(cx),
        }
    }

    /// Move the tab on screen by `by`, wrapping at the ends: a tab bar is usually a
    /// handful of tabs, and wrapping beats counting them. `gt`/`gT` and
    /// `:tabnext`/`:tabprev` all come here.
    pub(super) fn select_tab(&mut self, by: isize, cx: &mut Context<Self>) {
        if self.tabs.len() < 2 {
            self.say("only one tab", cx);
            cx.notify();
            return;
        }
        let next = (self.tab as isize + by).rem_euclid(self.tabs.len() as isize) as usize;
        self.show_tab(next, cx);
    }

    /// Close a tab: its note reaches the disk first if it is ahead of it, and the tab
    /// goes. Closing the only tab clears it instead, which is the empty tab a window
    /// starts with — the window does not close because a tab did.
    ///
    /// **A write that fails keeps the tab open.** The user can see the reason in the
    /// status bar, and a tab that closes on a failed write is work that vanished
    /// because of it.
    pub(super) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        // A tab holding a note writes it before it goes; an empty tab has nothing to
        // write, and nothing to fail at writing. The graph tab has no note either.
        if self.tabs[index].path().is_some()
            && self.tabs[index]
                .editor()
                .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty()))
        {
            self.write_tab(index, cx);
            if self.tabs[index]
                .editor()
                .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty()))
            {
                cx.notify();
                return false;
            }
        }
        if self.tabs.len() == 1 {
            // The last tab: cleared rather than removed, so the window keeps its
            // invariant and the user keeps a place to type. Closing the graph tab
            // leaves an empty *note* tab, because the thing to fall back to is a place
            // to write.
            self.clear_tab(0, cx);
            self.checker_closed(cx);
            self.say("no note open", cx);
        } else {
            self.tabs.remove(index);
            // The tab to the left, which is where the eye already is.
            self.tab = index.min(self.tabs.len() - 1);
            self.mark_note_mru();
            self.hover_stale = true;
            self.focus_stale = true;
            self.drop_popup(cx);
            self.clear_search(cx);
            if let Some(path) = self.note() {
                let text = self
                    .editor()
                    .map(|editor| editor.read_with(cx, |editor, _cx| editor.text()))
                    .unwrap_or_default();
                self.checker_opened(&path, &text, cx);
            } else if !self.tabs[self.tab].is_graph() {
                self.checker_closed(cx);
            }
        }
        cx.notify();
        true
    }

    /// Empty a tab without closing it: what `:tabonly` leaves, and what closing the
    /// last tab leaves. A graph tab cannot be emptied — an empty graph tab is a window
    /// with nothing in it — so it becomes a fresh empty *note* tab under the same id,
    /// which is the place to write the invariant asks for.
    pub(super) fn clear_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.tabs[index].is_graph() {
            let id = self.tabs[index].id;
            let content = self.note_content(id, View::Live, cx);
            let tab = &mut self.tabs[index];
            tab.content = content;
            tab.autosave_seq = 0;
            tab._autosave = None;
            return;
        }
        // The editor is taken out of the borrow before it is written to: the autosave
        // subscription lives on the same tab, and `self.tabs` cannot be borrowed twice
        // to clear both.
        let editor = match &mut self.tabs[index].content {
            TabContent::Note { path, editor, .. } => {
                *path = None;
                editor.clone()
            }
            TabContent::Graph(_) => return,
        };
        self.tabs[index]._autosave = None;
        editor.update(cx, |editor, cx| editor.set_text(String::new(), cx));
    }

    /// `ctrl-t` / `:tabnew`: a new tab holding the note in hand. Two tabs on one note
    /// is the point — the same note read twice, each with its own cursor, scroll,
    /// folds and undo history.
    pub(super) fn tab_new(&mut self, _: &TabNew, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.note();
        let index = self.add_tab(cx);
        if let Some(path) = path.as_ref() {
            match self.vault.read(path) {
                Ok(text) => {
                    if let TabContent::Note {
                        path: slot, editor, ..
                    } = &mut self.tabs[index].content
                    {
                        *slot = Some(path.clone());
                        editor.update(cx, |editor, cx| editor.set_text(text, cx));
                    }
                    // The view came from the tab it was opened from, and `set_view`
                    // is what makes a preview tab read-only, so nothing else is owed
                    // here.
                }
                Err(err) => {
                    // The tab exists and is empty rather than the keystroke doing
                    // nothing: the status bar says why, and the user is one `:e` away
                    // from a note.
                    tracing::warn!(path = %path.display(), "failed to read note: {err:#}");
                    self.say(format!("{err:#}"), cx);
                }
            }
        }
        self.say(format!("{} tabs", self.tabs.len()), cx);
        // The keyboard comes here now rather than waiting for the next frame: this is
        // a keystroke the editor made, and the node it belongs to is the new tab's.
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// `g t` / `:tabnext`. The keyboard is handed to the tab that arrived on screen:
    /// the keystroke that got here came from the tab that is leaving it.
    pub(super) fn tab_next(&mut self, _: &TabNext, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab(1, cx);
        self.focus_active_tab(window, cx);
    }

    /// `g T` / `:tabprev`: the other direction, and the same debt to the keyboard.
    pub(super) fn tab_prev(&mut self, _: &TabPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab(-1, cx);
        self.focus_active_tab(window, cx);
    }

    /// `:tabclose`: close the tab on screen. Its note is written first, which is the
    /// same promise a quit makes with autosave on — a tab the user closes is not a
    /// wish to lose the note.
    pub(super) fn tab_close(&mut self, _: &TabClose, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab(self.tab, cx);
        self.focus_active_tab(window, cx);
    }

    /// `:tabonly`: close every tab but this one. The others' notes are written by the
    /// same rule `:tabclose` follows, and a tab whose write fails stays open.
    pub(super) fn tab_only(&mut self, _: &TabOnly, window: &mut Window, cx: &mut Context<Self>) {
        let keep = self.tab;
        let others: Vec<usize> = (0..self.tabs.len()).filter(|n| *n != keep).collect();
        let mut closed = 0;
        for index in others.into_iter().rev() {
            if self.close_tab(index, cx) {
                closed += 1;
            }
        }
        if closed == 0 {
            self.say("nothing else to close", cx);
        }
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// `:tabmove <n>`: the tab on screen goes to position `n`, which is how a tab bar
    /// gets ordered without dragging anything. The command line is the only door with
    /// a number in it, so it calls this directly rather than through an action — the
    /// same posture `:theme <name>` keeps.
    pub(super) fn move_tab(&mut self, to: usize, cx: &mut Context<Self>) {
        let from = self.tab;
        let to = to.min(self.tabs.len() - 1);
        if from == to {
            self.say(format!("tab {} of {}", from + 1, self.tabs.len()), cx);
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.tab = to;
        self.say(format!("tab {} of {}", to + 1, self.tabs.len()), cx);
        cx.notify();
    }

    /// The tab bar's own click: point at a tab and it is the note on screen, and the
    /// keyboard goes with it.
    pub(super) fn click_tab(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.tab_for_id(id) else {
            return;
        };
        self.show_tab(index, cx);
        let handle = self.active_focus(cx);
        window.focus(&handle, cx);
    }

    /// The tab bar's middle click, and the `×` on a tab: close it. The keyboard has
    /// to be moved off a closed tab's editor for the same reason a tab change moves it
    /// on.
    pub(super) fn click_tab_close(
        &mut self,
        id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.tab_for_id(id) else {
            return;
        };
        self.close_tab(index, cx);
        self.focus_active_tab(window, cx);
    }

    /// Every tab's note to the disk, for the exits that owe more than the note on
    /// screen: `:q`, `:wq`, a termination signal, and the window manager's close.
    /// Whether every tab came out clean, which is what lets a quit decide whether it
    /// may happen.
    pub(super) fn write_all_tabs(&mut self, cx: &mut Context<Self>) -> bool {
        let mut clean = true;
        for index in 0..self.tabs.len() {
            if !self.tabs[index]
                .editor()
                .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty()))
            {
                continue;
            }
            self.write_tab(index, cx);
            if self.tabs[index]
                .editor()
                .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty()))
            {
                clean = false;
            }
        }
        clean
    }

    /// Whether any tab is ahead of its note on disk, which is what `:q` asks with
    /// autosave off, and what the tab bar's dots show. The graph tab is never dirty:
    /// it has no note to be ahead of.
    pub(super) fn any_dirty(&self, cx: &gpui::App) -> bool {
        self.tabs.iter().any(|tab| {
            tab.editor()
                .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty()))
        })
    }

    /// The tabs as the tab bar and the session file want them: whether the tab is the
    /// graph, its path, whether the note is ahead of the disk, and the view the tab is
    /// in. A graph tab has no path, no dirty state and no view, which is what the tab
    /// bar and the session both read as "this one is the graph".
    pub(crate) fn tab_state(&self, cx: &gpui::App) -> Vec<TabState> {
        self.tabs
            .iter()
            .map(|tab| TabState {
                id: tab.id,
                graph: tab.is_graph(),
                path: tab.path().map(|path| path.to_path_buf()),
                dirty: tab
                    .editor()
                    .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.is_dirty())),
                view: tab
                    .editor()
                    .map(|editor| editor.read(cx).view().word().to_string())
                    .unwrap_or_default(),
                // Only the graph tab has a camera, and only once it has been painted
                // at least once: before that there is nothing to remember.
                camera: match &tab.content {
                    TabContent::Graph(view) => {
                        view.read(cx)
                            .camera()
                            .map(|camera| shale_core::session::GraphSeat {
                                pan: camera.pan,
                                zoom: camera.zoom,
                            })
                    }
                    TabContent::Note { .. } => None,
                },
            })
            .collect()
    }
}

/// What a tab looks like from outside it: enough for the tab bar to draw a row and
/// for the session file to remember one.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TabState {
    /// The tab's stable id, which is what a click is answered with.
    pub(crate) id: usize,
    /// The note it holds, or `None` for an empty tab and for the graph.
    pub(crate) path: Option<PathBuf>,
    /// Whether this tab is the vault's graph rather than a note.
    pub(crate) graph: bool,
    /// Where the graph tab was looking, for the session file. `None` for a note tab.
    pub(crate) camera: Option<shale_core::session::GraphSeat>,
    /// Whether its note is ahead of the disk: the dot on the tab.
    pub(crate) dirty: bool,
    /// Which of the three views the tab is in, since a tab keeps its own.
    pub(crate) view: String,
}
