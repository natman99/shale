//! The window's half of the graph: the index, the layout, and the beat that runs it
//! (roadmap P3.4).
//!
//! Three things live here and none of them is a view:
//!
//! - **The index** (`Workspace.graph`), built the first time the graph tab is opened
//!   and kept current from the watcher, the saves, and the vault edits Shale makes
//!   itself. It is the window's rather than the tab's because it is one scan of the
//!   vault, and a second view of the graph (M5j's minimap) wants the same answers.
//! - **The simulation** (`Workspace.graph_sim`), one set of positions per window, for
//!   the same reason. [`GraphSim::build`] re-slots it whenever the index changes and
//!   decides there whether the layout is worth moving at all.
//! - **The beat** (`Workspace.graph_beat`), one task that ticks the layout while a
//!   graph tab is on screen and ends by itself when the sim settles or the tab closes.
//!
//! What the user sees — the camera, the counts, the canvas — is `crate::graph`, which
//! only ever receives a [`crate::graph::GraphFrame`] snapshot. The doors the view
//! calls back through are here too, and they deliberately never touch the view: a
//! call from inside the view's own mouse handler would be a second borrow of the
//! entity that is calling.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{prelude::*, Context, Entity, Window};
use shale_core::vault::VaultEntry;
use shale_core::watch::VaultChanges;

use crate::acts::OpenGraph;

use super::tabs::{Tab, TabContent};
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

/// What the vault did while the index scan was in flight.
///
/// The scan is a photograph of the disk taken some seconds before its answer arrives, so
/// anything that moved in between has to be folded into the index by hand or it stays
/// invisible in the graph until something else forces a rebuild.
pub(super) enum GraphChange {
    /// A note and the text it now holds.
    Saved(PathBuf, String),
    /// A note that is gone.
    Removed(PathBuf),
}

/// Fold one deferred change into a fresh index. The same calls the present-tense paths
/// make, factored out so the queue and those paths cannot drift apart.
fn apply_graph_change(
    index: &mut shale_core::graph::GraphIndex,
    change: GraphChange,
    entries: &[VaultEntry],
    attachments: &[VaultEntry],
) {
    match change {
        GraphChange::Saved(path, text) => index.set_note(&path, &text, entries, attachments),
        GraphChange::Removed(path) => index.remove_note(&path),
    }
}

impl Workspace {
    /// The window's graph view, if any tab holds one. There is at most one graph tab
    /// per window (see [`TabContent::Graph`]), so this is a search rather than a table.
    pub(super) fn graph_view(&self) -> Option<&Entity<crate::graph::GraphView>> {
        self.pane().tabs.iter().find_map(|tab| match &tab.content {
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
        // The view gets a weak handle back to the window, because the pointer's work
        // lives there: a node drag moves the window's simulation, and a click opens a
        // note through the window. Weak because the window owns the view, and a strong
        // handle back would be a cycle neither half could break.
        let window = cx.entity().downgrade();
        let view = cx.new(|cx| {
            crate::graph::GraphView::new(
                window,
                self.graph_dim,
                Rc::clone(&self.palette),
                family,
                size,
                cx,
            )
        });
        let index = if self.pane().tabs.is_empty() {
            0
        } else {
            self.pane().active + 1
        };
        self.pane_mut().tabs.insert(
            index,
            Tab {
                id,
                content: TabContent::Graph(view),
                autosave_seq: 0,
                _autosave: None,
            },
        );
        self.pane_mut().active = index;
        // The graph takes the pointer's subscription away: there is no note under it
        // to hover, and the keyboard has to land on the graph's own key context.
        self.hover_stale = true;
        self.focus_stale = true;
        index
    }

    /// The nodes the filter lets through, as core's mask indexed by petgraph's node
    /// index. Everything the graph shows — the counts, the physics, the picture — is
    /// computed from this, so a hidden node is absent from all three rather than drawn
    /// in a faint colour.
    fn graph_visibility(&self) -> Vec<bool> {
        self.graph
            .as_ref()
            .map(|index| shale_core::graph::filter::apply(index, &self.graph_filter))
            .unwrap_or_default()
    }

    /// The counts the graph panel shows, read off the window's index and the filter's
    /// mask: they describe what is on screen, not what the vault holds, because a row
    /// that says "12 notes" above a picture of four is a row nobody believes.
    /// `None` when the index has never been built.
    fn graph_summary(&self, visible: &[bool]) -> Option<crate::graph::GraphSummary> {
        use shale_core::graph::NodeKind;
        let index = self.graph.as_ref()?;
        let (mut notes, mut ghosts, mut tags) = (0usize, 0usize, 0usize);
        for node in index.graph().node_indices() {
            if !visible.get(node.index()).copied().unwrap_or(true) {
                continue;
            }
            match index.meta(node).kind {
                NodeKind::Note | NodeKind::Attachment => notes += 1,
                NodeKind::Ghost => ghosts += 1,
                NodeKind::Tag => tags += 1,
            }
        }
        // The graph is undirected and never holds parallel edges, so this is already
        // "A→B and B→A counted once"; an edge counts when both ends are visible, which
        // is the same rule the renderer and the simulation use. `edge_endpoints` rather
        // than `edge_references`, because the latter needs petgraph's `EdgeRef` trait
        // and this crate does not speak petgraph.
        let links = index
            .graph()
            .edge_indices()
            .filter_map(|edge| index.graph().edge_endpoints(edge))
            .filter(|(from, to)| {
                visible.get(from.index()).copied().unwrap_or(true)
                    && visible.get(to.index()).copied().unwrap_or(true)
            })
            .count();
        Some(crate::graph::GraphSummary {
            notes,
            ghosts,
            tags,
            links,
        })
    }

    /// The window's half of a refresh: re-slot the sim from the index and the filter's
    /// mask, and recount what is on screen. `None` while the index has never been built.
    ///
    /// Split from [`Workspace::refresh_graph`] because the filter strip asks from inside a
    /// handler that already holds the view's borrow, so the result cannot be pushed into
    /// the view the usual way: that caller takes the counts and the picture back instead.
    fn rebuild_graph(
        &mut self,
    ) -> Option<(crate::graph::GraphSummary, Rc<crate::graph::GraphFrame>)> {
        let visible = self.graph_visibility();
        let summary = self.graph_summary(&visible)?;
        let previous = self.graph_sim.take();
        if let Some(previous) = previous.as_ref() {
            if previous.dim() != self.graph_dim {
                // Worth one line: a switch re-heats the layout at its own heat, and a
                // graph that moved for no visible reason is the kind of thing a log is
                // for (roadmap P3.6).
                tracing::debug!(dim = ?self.graph_dim, "the graph changes space");
            }
        }
        if let Some(index) = self.graph.as_ref() {
            self.graph_sim = Some(crate::graph::GraphSim::build(
                index,
                previous.as_ref(),
                &visible,
                self.graph_dim,
            ));
        }
        let frame = self
            .graph_sim
            .as_ref()
            .map(|sim| Rc::new(sim.frame()))
            .unwrap_or_else(|| Rc::new(crate::graph::GraphFrame::empty()));
        Some((summary, frame))
    }

    /// Re-send the graph to the view: the counts, the picture, and the filter its chips
    /// are drawn from.
    ///
    /// Called when the index is built and whenever a note moves under it. The sim is
    /// re-slotted here rather than at every index call site, because "the graph
    /// changed" and "show the graph" are the same moment, and [`GraphSim::build`] is
    /// what decides whether that means a re-heat or nothing at all (a prose edit that
    /// changes no links moves nothing).
    pub(crate) fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        // With no graph tab on screen there is nothing to send to, and re-slotting a
        // simulation nobody is looking at is work for no one: the next `:graph` builds it.
        let Some(view) = self.graph_view().cloned() else {
            return;
        };
        let Some((summary, frame)) = self.rebuild_graph() else {
            return;
        };
        let filter = self.graph_filter.clone();
        view.update(cx, |view, cx| {
            view.set_summary(summary, cx);
            view.set_frame(frame, cx);
            view.set_filter(filter, cx);
        });
        self.pump_graph(cx);
    }

    /// The note the local graph is centred on (M2b): the most recently shown note in the
    /// active pane. The graph tab is never the centre — the centre is the note the user
    /// was reading when they turned the vault into a neighbourhood, and `note_mru` is
    /// that fact already, kept for the session.
    pub(crate) fn graph_center(&self) -> Option<String> {
        let pane = self.pane();
        let id = pane.note_mru?;
        let tab = pane.tabs.iter().find(|tab| tab.id == id)?;
        match &tab.content {
            TabContent::Note {
                path: Some(path), ..
            } => Some(path.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    /// The window's filter, for the view's chips: the window is the one that owns it, so
    /// a chip that changed by key has to be read back rather than guessed at.
    pub(crate) fn graph_filter_state(&self) -> shale_core::graph::filter::GraphFilter {
        self.graph_filter.clone()
    }

    /// Turn the graph into a neighbourhood around the note on screen, or back into the
    /// vault (`:graph-local [n]`, `:graph-global`, the strip's chip, `[` and `]`).
    ///
    /// The window owns the centre rather than the strip: the centre is a fact about which
    /// note the window is looking at, and the strip only says how far out to reach. A
    /// depth of `None` is the whole vault. With no note shown there is nothing to be a
    /// neighbourhood around, and the answer is to do nothing rather than to show an empty
    /// canvas or silently widen to the whole vault.
    pub(crate) fn graph_local(
        &mut self,
        depth: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Option<(crate::graph::GraphSummary, Rc<crate::graph::GraphFrame>)> {
        use shale_core::graph::filter::{LocalScope, LOCAL_DEPTH_MAX};
        match depth {
            None => self.graph_filter.local = None,
            Some(depth) => {
                let center = self.graph_center()?;
                self.graph_filter.local = Some(LocalScope {
                    center,
                    depth: depth.clamp(1, LOCAL_DEPTH_MAX),
                });
            }
        }
        let built = self.rebuild_graph();
        self.pump_graph(cx);
        built
    }

    /// The same door for a request that did not come from the view (a `:` command): the
    /// view has to be told what the chips now say, because it did not make the change.
    pub(crate) fn graph_local_synced(&mut self, depth: Option<usize>, cx: &mut Context<Self>) {
        self.graph_local(depth, cx);
        if let Some(view) = self.graph_view().cloned() {
            let filter = self.graph_filter.clone();
            view.update(cx, |view, cx| view.set_filter(filter, cx));
        }
    }

    /// Step the local depth, turning the local view on if it was not on (the graph
    /// context's `[` and `]`).
    pub(crate) fn graph_local_step(
        &mut self,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> Option<(crate::graph::GraphSummary, Rc<crate::graph::GraphFrame>)> {
        use shale_core::graph::filter::LOCAL_DEPTH_MAX;
        let current = self.graph_filter.local.as_ref().map(|scope| scope.depth);
        let next = match current {
            // Off, and stepping: start at the default rather than at one hop, because
            // one hop is the note and its direct links, which is a list, not a picture.
            None => 2,
            Some(depth) => (depth as isize + delta).clamp(1, LOCAL_DEPTH_MAX as isize) as usize,
        };
        self.graph_local(Some(next), cx)
    }

    /// The note under the local view has changed, so the neighbourhood has to follow it.
    ///
    /// Called wherever a note tab becomes the one on screen. A local graph that stayed
    /// centred on the note it *was* looking at would be a picture of a note the user has
    /// left, which is the confusion the feature exists to remove.
    ///
    /// The work is deferred one frame rather than done here. This runs from tab
    /// activation, and a tab is very often activated *by the graph view itself* — a node
    /// click opens its note — so the view is borrowed by the time we get here, and
    /// `refresh_graph` writes to it. One frame later nobody is borrowing anything.
    pub(crate) fn graph_local_follow(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.graph_filter.local.as_ref() else {
            return;
        };
        let Some(center) = self.graph_center() else {
            return;
        };
        if center == scope.center {
            return;
        }
        self.graph_filter.local = Some(shale_core::graph::filter::LocalScope {
            center,
            depth: scope.depth,
        });
        // Re-made on the **next frame**, not here. A tab is very often activated by the
        // graph view itself — a node click opens its note — and the view is borrowed when
        // that happens, so `refresh_graph` writing to it now would be a double borrow.
        // The frame flag is how this codebase defers anything that needs to be one step
        // outside the current borrow (`hover_stale`, `focus_stale`); a background timer
        // looked like the same thing and is not: it never runs inside the test harness,
        // which is how a follow that did nothing passed its own test.
        self.graph_follow_pending = true;
        cx.notify();
    }

    /// A new filter, from the strip: remember it on the window and re-make the picture.
    ///
    /// The window owns the filter rather than the view, because the mask it produces
    /// decides what the **simulation** contains: a hidden node takes part in neither the
    /// physics nor the paint, and the window is the only thing holding the sim. The strip
    /// asks from inside the view's own handler, so — like the other doors the view calls —
    /// this one never touches the view: the counts and the picture go back to the caller.
    pub(crate) fn graph_set_filter(
        &mut self,
        filter: shale_core::graph::filter::GraphFilter,
        cx: &mut Context<Self>,
    ) -> Option<(crate::graph::GraphSummary, Rc<crate::graph::GraphFrame>)> {
        self.graph_filter = filter;
        let built = self.rebuild_graph();
        self.pump_graph(cx);
        built
    }

    /// A new mode, from the graph tab: run the window's physics in the other space.
    ///
    /// Like [`Workspace::graph_set_filter`], the window owns this rather than the view,
    /// because it is the **simulation** that changes space — one graph per window means
    /// one set of positions, and a second view asking for 3D is asking these bodies to
    /// run in 3D. And like the filter, the ask arrives from inside the view's own
    /// handler, so this door never touches the view: the counts and the picture go back
    /// to the caller.
    ///
    /// `None` when the mode did not change, which is what makes a repeated ask cheap.
    pub(crate) fn graph_set_mode(
        &mut self,
        dim: crate::graph::Dim,
        cx: &mut Context<Self>,
    ) -> Option<(crate::graph::GraphSummary, Rc<crate::graph::GraphFrame>)> {
        if self.graph_dim == dim {
            return None;
        }
        self.graph_dim = dim;
        // `rebuild_graph` is the whole switch: `GraphSim::build` sees the space change,
        // carries the arrangement and the pins, seeds the new axis, and re-heats at
        // [`crate::graph::GraphSim`'s switch heat].
        let built = self.rebuild_graph();
        self.pump_graph(cx);
        built
    }

    /// Which space this window's graph runs in. The session capture reads the mode off
    /// the **view** instead (it walks tabs, and a tab owns its view), so this accessor
    /// exists for the tests that ask the window what space it is in.
    #[cfg(test)]
    pub(crate) fn graph_mode(&self) -> crate::graph::Dim {
        self.graph_dim
    }

    /// Restore the graph's space from the session file.
    ///
    /// Unlike [`Workspace::graph_set_mode`] — the door the tab's `2`/`3` keys come
    /// through, which never touches the view because the ask starts inside it — this one
    /// runs while the window is restoring tabs, so it pushes the mode into the view as
    /// well as setting it on the sim. Order matters: the sim is built in the restored
    /// space **before** the tab's first frame, so the first thing painted is already
    /// three-dimensional rather than flat for one frame and turned on the next.
    pub(crate) fn graph_restore_mode(&mut self, dim: crate::graph::Dim, cx: &mut Context<Self>) {
        if self.graph_dim == dim {
            return;
        }
        self.graph_dim = dim;
        let built = self.rebuild_graph();
        self.pump_graph(cx);
        let Some(view) = self.graph_view().cloned() else {
            return;
        };
        view.update(cx, |view, cx| {
            view.set_mode(dim);
            if let Some((summary, frame)) = built {
                view.set_summary(summary, cx);
                view.set_frame(frame, cx);
            }
        });
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
        if !self
            .panes
            .iter()
            .any(|pane| pane.tabs.iter().any(|tab| tab.is_graph()))
        {
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

    /// A node drag, from the graph view's own pointer handler: move the body, hold it
    /// against the forces, and make sure the layout is running again.
    ///
    /// **This must not touch the view.** The call arrives while the view entity is
    /// borrowed by its own event handler, so a `view.update(…)` from here would be a
    /// second borrow of the entity that is calling, and gpui would stop. The beat does
    /// the shipping instead: the next tick sends a new frame, and a drag keeps the sim
    /// hot, so the next tick is never more than a tick away.
    pub(crate) fn graph_drag_node(&mut self, slot: usize, world: [f32; 3], cx: &mut Context<Self>) {
        let Some(sim) = self.graph_sim.as_mut() else {
            return;
        };
        sim.drag_node(slot, world);
        self.pump_graph(cx);
    }

    /// The button came up after a node drag. `pinned` (shift held) leaves the node
    /// where the user put it; otherwise the forces take it back.
    pub(crate) fn graph_release_node(&mut self, slot: usize, pinned: bool, cx: &mut Context<Self>) {
        let Some(sim) = self.graph_sim.as_mut() else {
            return;
        };
        sim.release_node(slot, pinned);
        self.pump_graph(cx);
    }

    /// A click on a node, or `enter` on the keyboard's choice: open the note it stands
    /// for. Safe to call from inside the view because opening a note never touches the
    /// graph view — the frame moves to a note tab, and the graph tab keeps its canvas.
    pub(crate) fn graph_open_note(&mut self, path: &str, cx: &mut Context<Self>) {
        self.open_note(&PathBuf::from(path), cx);
    }

    /// Fold one set of disk changes into the graph, if it has been built.
    ///
    /// `set_note` and `remove_note` each rebuild the arena, so a burst of changes costs
    /// one rebuild per changed note. That is cheap enough at the vault sizes the graph
    /// is useful at, and it is what the integration contract's "incremental now,
    /// DB-backed extraction deferred" ruling covers.
    pub(super) fn graph_changes(&mut self, changes: &VaultChanges) {
        let Some(index) = self.graph.as_mut() else {
            // No index yet. If a scan is in flight it read the disk before these changes
            // and will not have seen them, so they go on the queue and are folded in when
            // it lands; with no scan running there is no graph to keep current, which is
            // the same answer either way.
            for removed in &changes.removed {
                self.queue_graph_change(GraphChange::Removed(removed.clone()));
            }
            for path in changes.added.iter().chain(changes.modified.iter()) {
                match self.vault.read(path) {
                    Ok(text) => {
                        self.queue_graph_change(GraphChange::Saved(path.clone(), text));
                    }
                    Err(err) => {
                        tracing::warn!(path = %path.display(), "graph re-scan failed: {err:#}")
                    }
                }
            }
            return;
        };
        for removed in &changes.removed {
            index.remove_note(removed);
        }
        for path in changes.added.iter().chain(changes.modified.iter()) {
            match self.vault.read(path) {
                Ok(text) => index.set_note(path, &text, &self.entries, &self.attachments),
                Err(err) => tracing::warn!(path = %path.display(), "graph re-scan failed: {err:#}"),
            }
        }
    }

    /// The graph follows a note Shale wrote itself. The watcher is stamped over our own
    /// saves, so this is the only way a link typed in the editor reaches the graph.
    pub(crate) fn graph_note_saved(&mut self, path: &Path, text: &str) {
        if let Some(index) = self.graph.as_mut() {
            index.set_note(path, text, &self.entries, &self.attachments);
        } else {
            self.queue_graph_change(GraphChange::Saved(path.to_path_buf(), text.to_string()));
        }
    }

    /// A note that is gone leaves the graph — and the ghosts and tags only it linked to
    /// leave with it.
    pub(crate) fn graph_note_removed(&mut self, path: &Path) {
        if let Some(index) = self.graph.as_mut() {
            index.remove_note(path);
        } else {
            self.queue_graph_change(GraphChange::Removed(path.to_path_buf()));
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
        // One graph per window, across panes too (roadmap P3.2): a second tab
        // would be a second view of the same physics with its own camera, so the
        // answer is to focus the pane already showing it, not to open another.
        if let Some((pix, tix)) = self.panes.iter().enumerate().find_map(|(pix, pane)| {
            pane.tabs
                .iter()
                .position(|tab| tab.is_graph())
                .map(|tix| (pix, tix))
        }) {
            if pix != self.active_pane {
                self.active_pane = pix;
                self.hover_stale = true;
            }
            self.show_tab(tix, cx);
            return;
        }
        // The tab opens first, index or no index. On a large vault the scan takes
        // seconds, and a window that looks dead for seconds is the bug, not a detail: the
        // tab says what it is waiting for, and the picture arrives when the scan lands.
        self.add_graph_tab(cx);
        if self.graph.is_none() && self.graph_build.is_none() {
            self.start_graph_build(cx);
        }
        self.refresh_graph(cx);
        // The same report a tab change makes: the frame moved to something that is not a
        // note, and the status bar is where that is said. While the scan runs, that is
        // what the status bar says as well, because it is where a user looks when a tab
        // seems slow.
        if self.graph_build.is_some() {
            self.say("graph: building…", cx);
        } else {
            self.say("graph", cx);
        }
    }

    /// Put the index scan on the background executor.
    ///
    /// This is the whole reason the tab can open before the graph exists:
    /// [`shale_core::graph::GraphIndex::build`] reads every note and resolves every link
    /// in it, which is seconds on a large vault and used to happen on the window's
    /// thread, freezing the window. The scan takes its own copies of the vault and of the
    /// two file lists — neither an `Rc` nor a borrow crosses a thread — and those lists
    /// are the file tree's in-memory ones, so handing them over reads nothing.
    fn start_graph_build(&mut self, cx: &mut Context<Self>) {
        let vault = (*self.vault).clone();
        let entries = self.entries.clone();
        let attachments = self.attachments.clone();
        self.graph_pending.clear();
        self.graph_build = Some(cx.spawn(async move |ws, cx| {
            let started = Instant::now();
            let built = cx
                .background_executor()
                .spawn(async move {
                    shale_core::graph::GraphIndex::build(&vault, &entries, &attachments)
                })
                .await;
            ws.update(cx, |ws, cx| ws.graph_index_arrived(built, started, cx))
                .ok();
        }));
        self.set_graph_building(cx, true);
    }

    /// The scan's answer: install the index, catch it up on whatever the vault did while
    /// it ran, and send the picture to the tab.
    fn graph_index_arrived(
        &mut self,
        built: anyhow::Result<shale_core::graph::GraphIndex>,
        started: Instant,
        cx: &mut Context<Self>,
    ) {
        self.graph_build = None;
        self.set_graph_building(cx, false);
        match built {
            Ok(mut index) => {
                for change in std::mem::take(&mut self.graph_pending) {
                    apply_graph_change(&mut index, change, &self.entries, &self.attachments);
                }
                // Info, not debug: this is the line that answers "how long does the graph
                // take on my vault", and it only appears when a scan actually ran.
                tracing::info!(
                    nodes = index.graph().node_count(),
                    took_ms = started.elapsed().as_millis(),
                    "the graph index was built"
                );
                self.graph = Some(index);
                self.refresh_graph(cx);
                self.say("graph", cx);
            }
            Err(err) => {
                // The same posture as a checker that will not start: the graph is a view
                // of the notes, not another copy of them, so a failed build is a message
                // rather than a broken window.
                self.say(format!("graph failed: {err:#}"), cx);
            }
        }
        cx.notify();
    }

    /// Tell the tab whether a scan is running, which is what its "building" line reads.
    fn set_graph_building(&mut self, cx: &mut Context<Self>, building: bool) {
        if let Some(view) = self.graph_view().cloned() {
            view.update(cx, |view, cx| view.set_building(building, cx));
        }
    }

    /// Note a vault change that happened during a scan. A no-op when no scan is running,
    /// which is what lets every change path call it without asking.
    fn queue_graph_change(&mut self, change: GraphChange) {
        if self.graph_build.is_some() {
            self.graph_pending.push(change);
        }
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
}
