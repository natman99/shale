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

use gpui::{Context, Entity, Window, prelude::*};
use shale_core::vault::{VaultEntry, rel_text};
use shale_core::watch::VaultChanges;

use crate::acts::{GraphLens, OpenGraph};
use crate::picker::PickerItem;
use crate::workspace::open::{PickerTarget, notice};

use super::Workspace;
use super::tabs::{Tab, TabContent};

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
    queries: &[VaultEntry],
    aliases: &[(String, String)],
) {
    match change {
        GraphChange::Saved(path, text) => {
            index.set_note(&path, &text, entries, attachments, queries, aliases);
        }
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
                pinned: false,
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
        let (mut notes, mut queries, mut ghosts, mut tags) = (0usize, 0usize, 0usize, 0usize);
        for node in index.graph().node_indices() {
            if !visible.get(node.index()).copied().unwrap_or(true) {
                continue;
            }
            match index.meta(node).kind {
                NodeKind::Note | NodeKind::Attachment => notes += 1,
                NodeKind::Query => queries += 1,
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
            queries,
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
        if let Some(previous) = previous.as_ref()
            && previous.dim() != self.graph_dim
        {
            // Worth one line: a switch re-heats the layout at its own heat, and a
            // graph that moved for no visible reason is the kind of thing a log is
            // for (roadmap P3.6).
            tracing::debug!(dim = ?self.graph_dim, "the graph changes space");
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
        // EDGE-L7: every index move ends at this door, so this is where a worn lens is
        // asked again. It happens before the picture is re-made, so an answer that moved
        // rides the same rebuild the mask change would need anyway — and one that did not
        // move disturbs the layout not at all, which is the half the measurement was for.
        self.lens_follow();
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
            } => Some(rel_text(path)),
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
        use shale_core::graph::filter::{LOCAL_DEPTH_MAX, LocalScope};
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

    // The lens (roadmap P4.16): a saved query the graph wears. The dimension itself is
    // core's (`graph::filter::LensScope`); what lives here is the door — the verb, the
    // picker, and the moment the answer becomes a mask and the picture is re-made.

    /// `:lens [name]`. An empty name opens the picker (ruling 5: `:lens` alone mirrors
    /// `:query`); a name puts that lens on the graph.
    pub(crate) fn graph_lens_command(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = name.trim();
        if name.is_empty() {
            self.open_lens_picker(window, cx);
            return;
        }
        // `save` is the verb's other reserved word, like `none`: `:lens save goblins`
        // keeps the filter line's question under that name and puts it on. A lens actually
        // named `save` is unreachable through this verb — the picker still lists it, and
        // the picker is where it stays reachable.
        if let Some(rest) = name.strip_prefix("save") {
            let rest = rest.trim();
            if rest.is_empty() {
                self.say("name it: `:lens save <name>`", cx);
                return;
            }
            let Some(search) = self.graph_filter.search.clone() else {
                self.say(
                    "there is no question in the graph's filter line to keep",
                    cx,
                );
                return;
            };
            if let Err(err) = self.write_lens(&search, rest, cx) {
                self.say(format!("{err:#}"), cx);
            }
            return;
        }
        self.wear_lens(name, "", cx);
    }

    /// `alt-s` in the filter strip: ask for a name for the question the line holds
    /// (M5). The name is asked rather than invented, because a picture nobody named is a
    /// picture that cannot be found again.
    pub(crate) fn ask_save_lens(
        &mut self,
        search: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search = search.trim();
        if search.is_empty() {
            self.say(
                "the filter line is empty: type the question first, or name one you have with `:lens save <name>`",
                cx,
            );
            return;
        }
        // Asked before the name is typed, exactly as `:save-query` does it: a question
        // that cannot be kept should say which part of it cannot before the reader has
        // spent a name on it.
        if let Err(reason) = shale_core::db::compile_sql(&shale_core::query::parse(search)) {
            self.say(reason, cx);
            return;
        }
        self.open_prompt(
            crate::prompt::PromptKind::SaveLens(search.to_string()),
            "name this lens",
            &Self::query_name(search),
            window,
            cx,
        );
    }

    /// Write the lens and put it on (ruled 2026-11-18: a saved lens goes on right away).
    ///
    /// The file holds the SQL the filter line's operators mean — the same translation
    /// keep-a-search uses, so the lens answers what the line answered. The strip's chips
    /// are **not** encoded (ruling 1): they stay filter dimensions, so a lens never
    /// freezes a chip choice into a file its author will later edit by hand.
    pub(crate) fn write_lens(
        &mut self,
        search: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let sql = shale_core::db::compile_sql(&shale_core::query::parse(search))
            .map_err(anyhow::Error::msg)?;
        // The question is kept above the statement as a comment, so the file says what it
        // was asked in the words that were typed. A lens is not a vault note, so it is
        // written through `lens::write` rather than the vault's doors (EDGE-L1).
        let text = format!(
            "+++\nname = \"{}\"\n+++\n-- Saved from the graph's filter line: {search}\n{sql}\n",
            name.trim().replace('"', "\\\"")
        );
        shale_core::lens::write(self.vault.root(), name, &text)?;
        // The lens folder is not in the vault's signature, so the stamps beside it are
        // re-taken: a lens Shale wrote itself must not come back as somebody else's edit.
        self.resign();
        self.wear_lens(name, "saved ", cx);
        Ok(())
    }

    /// The palette row: `:lens` with nothing typed, which asks rather than acts.
    pub(crate) fn graph_lens_action(
        &mut self,
        _: &GraphLens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_lens_picker(window, cx);
    }

    /// A lens chosen in the picker.
    pub(crate) fn graph_lens_chosen(&mut self, name: &str, cx: &mut Context<Self>) {
        self.wear_lens(name, "", cx);
    }

    /// Put a lens on the graph, or take it off.
    ///
    /// The file is the authority: it is read here, on use, so an edit made in an editor
    /// outside Shale takes effect the next time the lens is put on rather than needing an
    /// import. A lens that will not answer leaves the picture standing (EDGE-L6) and says
    /// why on the status line, because the command line has already closed and the status
    /// line is where a reader looks for why nothing changed.
    fn wear_lens(&mut self, name: &str, said: &str, cx: &mut Context<Self>) {
        // EDGE-L9: `none` is a name, not a mode — the same dimension set back to None.
        if name == "none" {
            if self.graph_filter.lens.is_none() {
                self.say("no lens is on the graph", cx);
                return;
            }
            self.graph_filter.lens = None;
            self.remake_graph(cx);
            self.say("lens off", cx);
            return;
        }
        let text = match shale_core::lens::read(self.vault.root(), name) {
            Ok(text) => text,
            Err(err) => {
                self.say(format!("{err:#}"), cx);
                return;
            }
        };
        // EDGE-L12: with the index off there is nothing to answer from, and "no note
        // answers this" would be a lie about the vault rather than about the setting.
        let Some(db) = self.db.as_ref() else {
            self.say(
                "there is no vault index to read (index.enabled = false)",
                cx,
            );
            return;
        };
        let conn = match db.read() {
            Ok(conn) => conn,
            Err(err) => {
                self.say(format!("the vault index could not be read: {err}"), cx);
                return;
            }
        };
        let stem = name
            .strip_suffix(&format!(".{}", shale_core::query_note::EXTENSION))
            .unwrap_or(name);
        match shale_core::lens::answer(&text, stem, &conn) {
            shale_core::lens::LensOutcome::Refused { why } => {
                // The picture stands; the sentence is core's, the same one the picker and
                // an embedded table give for the same file.
                self.say(format!("{said}lens `{name}`: {why}"), cx);
            }
            shale_core::lens::LensOutcome::Empty { name } => {
                self.graph_filter.lens = Some(shale_core::graph::filter::LensScope {
                    name: name.clone(),
                    paths: Vec::new(),
                    unmatched: 0,
                    stale: None,
                });
                self.remake_graph(cx);
                self.say(format!("{said}lens `{name}`: no note answers this"), cx);
            }
            shale_core::lens::LensOutcome::Answered { name, paths } => {
                // EDGE-L5: an answer can name paths the graph has no node for. They are
                // counted out loud rather than dropped.
                let missing = self
                    .graph
                    .as_ref()
                    .map(|index| shale_core::lens::unmatched(index, &paths).len())
                    .unwrap_or(0);
                let kept = paths.len() - missing;
                self.graph_filter.lens = Some(shale_core::graph::filter::LensScope {
                    name: name.clone(),
                    paths,
                    unmatched: missing,
                    stale: None,
                });
                self.remake_graph(cx);
                let extra = if missing > 0 {
                    format!(", {missing} with no node")
                } else {
                    String::new()
                };
                self.say(
                    format!(
                        "{said}lens `{name}`: {kept} {}{extra}",
                        shale_core::lens::notes_word(kept)
                    ),
                    cx,
                );
            }
        }
    }

    /// Re-make the picture for a filter the view did not ask for, and tell the view what
    /// the filter now says.
    ///
    /// [`Workspace::refresh_graph`] rather than `rebuild_graph` + `pump_graph`: a lens can
    /// shrink a picture without heating the layout at all, and a settled sim means
    /// `pump_graph` has nothing to tick — the view would keep the old frame and the old
    /// counts. `refresh_graph` ships both, settled or not.
    fn remake_graph(&mut self, cx: &mut Context<Self>) {
        self.refresh_graph(cx);
    }

    /// EDGE-L7: a worn lens **re-runs when the index moves**.
    ///
    /// Re-asking is one SQL call, which is cheap. The layout is the expensive half —
    /// ~0.9 s of sim work for a 2000-node answer while the reader is typing
    /// (`PROGRESS.md`, 2026-11-17) — so the scope is replaced, and the picture re-made,
    /// only when the answer's **node set** actually changed. An answer that came back in
    /// a different order is the same picture.
    ///
    /// A lens that stops answering keeps its last good picture (EDGE-L6) and carries the
    /// sentence for why, which the strip says out loud. The status line stays quiet: a
    /// broken lens re-fails on every index move, and a shout each time is noise, not news.
    fn lens_follow(&mut self) {
        let Some(worn) = self.graph_filter.lens.clone() else {
            return;
        };
        // EDGE-L12: with the index off there is nothing to answer from, and that is a fact
        // about the setting rather than a failure of the lens. The picture keeps what it
        // last answered.
        let Some(db) = self.db.as_ref() else {
            return;
        };
        let Ok(conn) = db.read() else {
            return;
        };
        // The file is the authority, so it is read again here. A lens whose file is gone
        // is not a lens that failed: [`Workspace::check_lenses`] is the door that says so,
        // and it says it once.
        let Ok(text) = shale_core::lens::read(self.vault.root(), &worn.name) else {
            return;
        };
        let stem = worn
            .name
            .strip_suffix(&format!(".{}", shale_core::query_note::EXTENSION))
            .unwrap_or(&worn.name);
        let paths = match shale_core::lens::answer(&text, stem, &conn) {
            shale_core::lens::LensOutcome::Answered { paths, .. } => paths,
            shale_core::lens::LensOutcome::Empty { .. } => Vec::new(),
            shale_core::lens::LensOutcome::Refused { why } => {
                if worn.stale.as_deref() != Some(why.as_str())
                    && let Some(scope) = self.graph_filter.lens.as_mut()
                {
                    scope.stale = Some(why);
                }
                return;
            }
        };
        // The set, not the order: a query without an `ORDER BY` may answer in any order,
        // and the mask is the same mask either way.
        let mut before: Vec<&str> = worn.paths.iter().map(String::as_str).collect();
        let mut after: Vec<&str> = paths.iter().map(String::as_str).collect();
        before.sort_unstable();
        after.sort_unstable();
        if before == after && worn.stale.is_none() {
            return;
        }
        let unmatched = self
            .graph
            .as_ref()
            .map(|index| shale_core::lens::unmatched(index, &paths).len())
            .unwrap_or(0);
        self.graph_filter.lens = Some(shale_core::graph::filter::LensScope {
            name: worn.name,
            paths,
            unmatched,
            stale: None,
        });
        #[cfg(test)]
        self.lens_replaced.set(self.lens_replaced.get() + 1);
    }

    /// EDGE-L7's other half, and EDGE-L11: the lens **files**. The vault's signature walk
    /// skips `.shale/` on purpose (EDGE-L1), so the lens folder is stamped beside it on
    /// the same tick — one small directory, no recursive walk.
    pub(super) fn check_lenses(&mut self, cx: &mut Context<Self>) {
        let now = shale_core::lens::stamps(self.vault.root());
        let before = std::mem::replace(&mut self.lens_stamps, now.clone());
        let Some(worn) = self
            .graph_filter
            .lens
            .as_ref()
            .map(|scope| scope.name.clone())
        else {
            return;
        };
        if !now.contains_key(&worn) {
            // EDGE-L11: the file went away under a graph that is wearing it. The picture
            // stands — the mask still holds the paths it last answered — and says the lens
            // is gone. The next `:lens`, or a restart, is what clears it.
            if let Some(scope) = self.graph_filter.lens.as_mut() {
                scope.stale = Some(format!("`{worn}` is gone"));
            }
            self.say(
                format!("lens `{worn}` is gone; the picture stays as it was"),
                cx,
            );
            // The strip draws its row from the copy the window pushes in, so the sentence
            // has to be shipped — `refresh_graph` re-asks the lens, finds no file, and
            // leaves the standing picture and the new sentence exactly as they are.
            self.refresh_graph(cx);
            return;
        }
        if before.get(&worn) != now.get(&worn) {
            // The program changed under the picture. `refresh_graph` re-asks it, and the
            // layout moves only if the answer's set moved with it.
            self.refresh_graph(cx);
        }
    }

    /// The lens picker: one row per lens the vault holds, each carrying how many nodes it
    /// answers **now** (EDGE-L10: measured when the picker opens, one query per lens, on
    /// one read-only connection so two lenses cannot answer against two different
    /// snapshots). A lens that will not answer shows `—` and still opens, because
    /// EDGE-L6 keeps the picture standing.
    fn open_lens_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lenses = shale_core::lens::list(self.vault.root());
        let current = self
            .graph_filter
            .lens
            .as_ref()
            .map(|lens| lens.name.clone())
            .unwrap_or_default();
        let conn = self.db.as_ref().and_then(|db| db.read().ok());
        let mut rows: Vec<(PickerItem, PickerTarget)> = Vec::new();
        if conn.is_none() {
            rows.push(notice(
                "there is no vault index to read (index.enabled = false), so no lens can answer",
            ));
        }
        if !current.is_empty() {
            rows.push((
                PickerItem {
                    label: "no lens".to_string().into(),
                    detail: format!("currently wearing `{current}`").into(),
                    detail_ranges: Vec::new(),
                    search: "no lens none".to_string().into(),
                },
                PickerTarget::Lens("none".to_string()),
            ));
        }
        for name in &lenses {
            let detail = match (
                conn.as_ref(),
                shale_core::lens::read(self.vault.root(), name),
            ) {
                (Some(conn), Ok(text)) => match shale_core::lens::answer(&text, name, conn) {
                    shale_core::lens::LensOutcome::Answered { paths, .. } => {
                        let missing = self
                            .graph
                            .as_ref()
                            .map(|index| shale_core::lens::unmatched(index, &paths).len())
                            .unwrap_or(0);
                        let kept = paths.len() - missing;
                        format!("{} {}", kept, shale_core::lens::notes_word(kept))
                    }
                    shale_core::lens::LensOutcome::Empty { .. } => {
                        format!("0 {}", shale_core::lens::notes_word(0))
                    }
                    shale_core::lens::LensOutcome::Refused { .. } => {
                        "\u{2014} not answering".to_string()
                    }
                },
                // A lens whose file cannot be read still gets a row: the row is how the
                // reader finds out, and a picker that hides it hides the problem too.
                _ => "\u{2014}".to_string(),
            };
            rows.push((
                PickerItem {
                    label: name.clone().into(),
                    detail: detail.into(),
                    detail_ranges: Vec::new(),
                    search: name.clone().into(),
                },
                PickerTarget::Lens(name.clone()),
            ));
        }
        if lenses.is_empty() {
            rows.push(notice(
                "no lenses yet: a lens is a .shq file under .shale/lenses",
            ));
        }
        self.open_picker("Lens", rows, window, cx);
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

    /// The pointer moved over the graph: which node it is resting on, or `None` when it is
    /// resting on nothing — including the moment it presses, which is the end of resting.
    ///
    /// The graph reports every move; the window decides, and the decision is a **wait**.
    /// A graph is dots packed together, and sweeping the pointer across one to reach
    /// another is ordinary, so a card that answered the moment the pointer arrived would
    /// flash at every dot on the way past. The dwell is the whole answer, and it is why
    /// the card is not simply drawn from the graph's own hover state.
    pub(crate) fn graph_pointer_rest(&mut self, slot: Option<usize>, cx: &mut Context<Self>) {
        // Only a node that is a picture of a note has anything to preview: a ghost names a
        // link nobody resolved, and a tag names a word.
        let resting = slot.filter(|slot| {
            self.graph_view().is_some_and(|view| {
                view.read_with(cx, |view, _cx| view.node_path(*slot))
                    .is_some()
            })
        });
        let Some(slot) = resting else {
            self.close_card(cx);
            return;
        };
        // Already showing exactly this node: leave it alone, so the card does not reopen
        // on every mouse-move event over the same dot.
        if self.card_anchor == Some(crate::workspace::popups::CardAnchor::Node(slot)) {
            return;
        }
        self.node_card_seq += 1;
        let seq = self.node_card_seq;
        self._node_card = Some(cx.spawn(async move |ws, cx| {
            cx.background_executor()
                .timer(crate::card::NODE_DWELL)
                .await;
            let _ = ws.update(cx, |ws, cx| {
                // The belt to the brace: a countdown that woke after the pointer moved on
                // writes nothing.
                if ws.node_card_seq != seq {
                    return;
                }
                ws.open_node_card(slot, cx);
            });
        }));
    }

    /// Put up the card for the node the pointer has been resting on.
    ///
    /// The path is asked for again here rather than carried through the wait: a frame can
    /// arrive while the pointer is resting, and its slots are that frame's slots. Asking
    /// the current view is what keeps a card from naming a note the picture no longer has.
    fn open_node_card(&mut self, slot: usize, cx: &mut Context<Self>) {
        let Some(path) = self
            .graph_view()
            .and_then(|view| view.read_with(cx, |view, _cx| view.node_path(slot)))
        else {
            return;
        };
        let path = PathBuf::from(path.as_str());
        let Some((title, lines, hidden)) = self.node_preview(&path, cx) else {
            return;
        };
        let (palette, family, size) = self.picker_style(cx);
        let card =
            cx.new(|_cx| crate::card::Preview::new(title, lines, hidden, palette, family, size));
        self.card = Some(card);
        self.card_anchor = Some(crate::workspace::popups::CardAnchor::Node(slot));
        self.card_link_start = None;
        cx.notify();
    }

    /// What a node card can say about a note: its name, the first lines of it with the
    /// markers hidden, how many it left out, and — ahead of them — the names the note
    /// declares for itself.
    ///
    /// The lines go through the same paint an embed's rows and the note's own rows use, so
    /// a heading in a card is a heading and not a `#`. The text comes from the buffer while
    /// a tab holds the note and from the disk otherwise — the rule the review strip already
    /// follows, for the reason it states: a preview that lagged an unsaved edit would
    /// disagree with the note the user is looking at.
    fn node_preview(
        &self,
        path: &Path,
        cx: &Context<Self>,
    ) -> Option<(String, Vec<String>, usize)> {
        let text = self.note_body(path, cx);
        let mut written: Vec<String> = text.lines().map(|line| line.to_string()).collect();
        // The frontmatter block is the note's bookkeeping, not its prose: the same fence
        // the fold, the checker and the embed door already skip.
        if let Some(block) = shale_core::frontmatter::block(&text) {
            let (open, close) = block.lines;
            written.drain(open..(close + 1).min(written.len()));
        }
        let painted = crate::editor::embeds::paint_body(
            &written,
            &[],
            crate::editor::embeds::Reveal::default(),
        );
        let shown = crate::card::VISIBLE_LINES;
        let mut lines: Vec<String> = painted
            .into_iter()
            .map(|row| row.text)
            .take(shown)
            .collect();
        // A note that opens with a blank row — the gap under the frontmatter fence — would
        // start the card with a hole. The card begins at the first row with something on
        // it; the count of what is still below is untouched, because those rows are still
        // there in the note.
        while lines.first().is_some_and(|line| line.trim().is_empty()) {
            lines.remove(0);
        }
        let hidden = written.len().saturating_sub(shown);
        // The alias line goes first: it is a fact about *which note this is*, and what
        // follows is what the note says. A note that declares nothing gets no line, so the
        // card never spends a row saying nothing.
        let aliases = self.declared_aliases(path);
        if !aliases.is_empty() {
            lines.insert(0, format!("also known as: {}", aliases.join(", ")));
        }
        let title = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        Some((title, lines, hidden))
    }

    /// The note's text: the buffer while a tab holds it, the disk otherwise.
    fn note_body(&self, path: &Path, cx: &Context<Self>) -> String {
        if let Some(index) = self.tab_for_path(path) {
            self.pane().tabs[index]
                .editor()
                .map(|editor| editor.read_with(cx, |editor, _cx| editor.text()))
                .unwrap_or_default()
        } else {
            self.vault.read(path).unwrap_or_default()
        }
    }

    /// The names a note declares for itself, in the order the vault lists them.
    fn declared_aliases(&self, path: &Path) -> Vec<String> {
        self.aliases
            .iter()
            .filter(|(_, owner)| Path::new(owner) == path)
            .map(|(alias, _)| alias.clone())
            .collect()
    }

    /// Fold one set of disk changes into the graph, if it has been built.
    ///
    /// `set_note` and `remove_note` each rebuild the arena, so a burst of changes costs
    /// one rebuild per changed note. That is cheap enough at the vault sizes the graph
    /// is useful at, and it is what the integration contract's "incremental now,
    /// DB-backed extraction deferred" ruling covers.
    pub(super) fn graph_changes(&mut self, changes: &VaultChanges) {
        // A scan in flight is the case that matters: it read the disk before these changes
        // and will not have seen them, so they go on the queue and are folded in when it
        // lands — **even when an index is already live**. A live index is not enough on its
        // own since the alias rebuild (below) can leave an old index standing while a new
        // scan runs; a change applied only to the old one would be lost when the new one
        // arrives. With no index and no scan there is no graph to keep current, which is
        // the same answer either way.
        let building = self.graph_build.is_some();
        let live = if building { None } else { self.graph.as_mut() };
        let Some(index) = live else {
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
                Ok(text) => index.set_note(
                    path,
                    &text,
                    &self.entries,
                    &self.attachments,
                    &self.queries,
                    &self.aliases,
                ),
                Err(err) => tracing::warn!(path = %path.display(), "graph re-scan failed: {err:#}"),
            }
        }
    }

    /// The graph follows a note Shale wrote itself. The watcher is stamped over our own
    /// saves, so this is the only way a link typed in the editor reaches the graph.
    pub(crate) fn graph_note_saved(&mut self, path: &Path, text: &str) {
        // The same rule as [`Workspace::graph_changes`]: a scan in flight has not seen this
        // save, so it belongs on the queue whether or not an index is standing.
        let building = self.graph_build.is_some();
        let live = if building { None } else { self.graph.as_mut() };
        if let Some(index) = live {
            index.set_note(
                path,
                text,
                &self.entries,
                &self.attachments,
                &self.queries,
                &self.aliases,
            );
        } else {
            self.queue_graph_change(GraphChange::Saved(path.to_path_buf(), text.to_string()));
        }
    }

    /// The aliases the vault declares changed, so the graph's edges may have changed
    /// **everywhere** — and the graph re-scans from the files.
    ///
    /// An alias is declared in one note's frontmatter and written in another note's body,
    /// so the note that was saved is not the note whose link it is: re-scanning only the
    /// saved note would leave `[[the deep one]]` a dead node in `b.md` while the editor is
    /// already painting it live, which is the one disagreement the alias feature's own
    /// comment says it cannot have. A full re-scan is what opening the tab already does —
    /// on the background executor, and only when the alias set actually moved.
    pub(crate) fn graph_aliases_changed(&mut self, cx: &mut Context<Self>) {
        if self.graph.is_none() || self.graph_build.is_some() {
            return;
        }
        self.start_graph_build(cx);
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
        let queries = self.queries.clone();
        let aliases = self.aliases.clone();
        self.graph_pending.clear();
        self.graph_build = Some(cx.spawn(async move |ws, cx| {
            let started = Instant::now();
            let built = cx
                .background_executor()
                .spawn(async move {
                    shale_core::graph::GraphIndex::build(
                        &vault,
                        &entries,
                        &attachments,
                        &queries,
                        &aliases,
                    )
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
                    apply_graph_change(
                        &mut index,
                        change,
                        &self.entries,
                        &self.attachments,
                        &self.queries,
                        &self.aliases,
                    );
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
