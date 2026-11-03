//! The window's one force simulation, and the slots that tie it to the index.
//!
//! The sim is the window's, not the tab's (integration contract,
//! `graph-view-plan.md`): one graph per window means one set of positions, and a
//! second view — M5j's minimap, a local graph — wants to look at *those* rather
//! than run its own physics. What the tab gets is a [`GraphFrame`] snapshot.
//!
//! The awkward part of tying a sim to an index is that [`GraphIndex`] rebuilds
//! its arena on every structural change, so a `NodeIndex` is only valid until the
//! next one. So the sim is indexed by **slot**, and each slot carries the node's
//! identity as a string. That is what lets a layout survive an edit: the indexes
//! are meaningless across a rebuild, but "notes/idea.md" is not, and a graph that
//! jumped to a new arrangement every time a word was typed would be unusable.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::SharedString;
use shale_core::graph::geom::Grid;
use shale_core::graph::index::{GraphIndex, NodeMeta};
use shale_core::graph::sim::{Sim, SimParams};
use shale_core::graph::NodeKind;

use super::frame::{GraphFrame, GraphNode};

/// The layout is reproducible: the same vault and the same seed give the same
/// starting spiral, which is what makes the headless paint tests mean anything.
const SEED: u64 = 0x5f3a_7c19_2b48_0d61;

/// How hot a re-heat is when the graph changed under a layout the user was
/// looking at. Full heat (1.0) would throw the old arrangement away; this is
/// enough for the new or moved nodes to find their place.
const REHEAT: f32 = 0.3;

/// The most cells the pointer's hit-test grid is ever given along one axis, so the
/// grid is at most about this squared buckets (a few MB). `Grid::build` allocates one
/// bucket per cell over the layout's bounding box, so a cell sized only off the node
/// radii turns a layout that has flown apart into a request for hundreds of gigabytes.
/// Widening the cell past the ideal only costs hit-test precision.
const GRID_MAX_SPAN: f32 = 512.0;

/// What a sim slot stands for, copied out of the index at build time.
struct Slot {
    /// The node's identity across an arena rebuild.
    key: String,
    /// Note, attachment, ghost or tag.
    kind: NodeKind,
    /// The note stem, tag name, or ghost target.
    label: String,
    /// Vault-relative path for notes and attachments; `None` for tags and ghosts.
    path: Option<PathBuf>,
    /// Link count, which sizes the dot and decides label priority.
    degree: u32,
}

/// What identifies a node across a rebuild. Notes and attachments are their path;
/// tags and ghosts have none, so their kind-qualified label is the identity (and
/// the kind prefix is what keeps a tag `idea` from matching a ghost `idea`).
fn key(meta: &NodeMeta) -> String {
    match meta.path.as_deref() {
        Some(path) => path.display().to_string(),
        None => match meta.kind {
            NodeKind::Tag => format!("#{}", meta.label),
            NodeKind::Ghost => format!("?{}", meta.label),
            // Unreachable for a node with no path; kept honest rather than assumed.
            NodeKind::Note | NodeKind::Attachment => format!("~{}", meta.label),
        },
    }
}

/// The set of node identities a sim stands for, sorted so it can be compared with
/// another build's (see [`GraphSim::build`]).
fn keys(slots: &[Slot]) -> Vec<&str> {
    let mut out: Vec<&str> = slots.iter().map(|slot| slot.key.as_str()).collect();
    out.sort_unstable();
    out
}

/// The set of links, each one named by the **identities** of its endpoints and sorted,
/// so two builds of the same graph agree however the arena happened to number its
/// nodes (see [`GraphSim::build`]).
fn canonical_links<'a>(slots: &'a [Slot], edges: &[[u32; 2]]) -> Vec<(&'a str, &'a str)> {
    let mut out: Vec<(&str, &str)> = edges
        .iter()
        .map(|[a, b]| {
            let from = slots[*a as usize].key.as_str();
            let to = slots[*b as usize].key.as_str();
            if from <= to {
                (from, to)
            } else {
                (to, from)
            }
        })
        .collect();
    out.sort_unstable();
    out
}

/// The simulation plus the slot map that says which node each body is.
pub(crate) struct GraphSim {
    /// The physics. Its arrays are indexed by slot.
    sim: Sim<2>,
    /// One entry per slot.
    slots: Vec<Slot>,
    /// Edges as slot pairs. Kept here because the frame needs them and because
    /// "did the shape change?" is a question about them.
    edges: Vec<[u32; 2]>,
}

impl GraphSim {
    /// Build the sim for the nodes of `index` that `visible` lets through, carrying
    /// over whatever layout `previous` had.
    ///
    /// `visible` is core's filter mask, indexed by petgraph's node index — the same one
    /// `filter::apply` returns. A filtered-out node is in neither the physics nor the
    /// picture, which is the integration contract's rule: a hidden node that still
    /// pushed on its neighbours would leave their layout moving for no visible reason.
    ///
    /// Three cases, and the difference between them is what the user sees:
    /// - **No previous**: a fresh layout at full heat (the phyllotaxis spiral,
    ///   then the forces sort it out).
    /// - **Same shape** — the same nodes and the same edges, which is what a
    ///   prose edit that changes no links produces: the old positions, velocities
    ///   and heat are copied across, so the graph does not move at all.
    /// - **Changed shape**: surviving nodes keep their positions and the sim
    ///   re-heats gently, so the new or moved nodes find a place without the rest
    ///   jumping. A filter change lands here, which is what the plan's "filters
    ///   re-heat the sim" line asks for.
    pub(crate) fn build(index: &GraphIndex, previous: Option<&GraphSim>, visible: &[bool]) -> Self {
        let graph = index.graph();
        let nodes: Vec<_> = graph
            .node_indices()
            .filter(|node| visible.get(node.index()).copied().unwrap_or(true))
            .collect();
        let count = nodes.len();

        // `slot_of` maps a petgraph index to its slot. The arena is rebuilt fresh
        // on every change, so the two are usually identical — but "usually" is not
        // a thing to build a layout on, and this costs one vector. A filtered-out
        // node keeps `u32::MAX`, which is what drops its edges below.
        let mut slot_of = vec![u32::MAX; graph.node_count()];
        let mut slots = Vec::with_capacity(count);
        for (slot, node) in nodes.iter().enumerate() {
            slot_of[node.index()] = slot as u32;
            let meta = index.meta(*node);
            slots.push(Slot {
                key: key(meta),
                kind: meta.kind,
                label: meta.label.clone(),
                path: meta.path.clone(),
                degree: index.degree(*node),
            });
        }
        // `edge_endpoints` rather than `edge_references`: the latter needs petgraph's
        // `EdgeRef` trait in scope, and the app crate deliberately does not depend on
        // petgraph — the index is the only thing that speaks it.
        let edges: Vec<[u32; 2]> = graph
            .edge_indices()
            .filter_map(|edge| {
                graph
                    .edge_endpoints(edge)
                    .map(|(from, to)| [slot_of[from.index()], slot_of[to.index()]])
            })
            // An edge is drawn, and pulled on, exactly when both ends are: that is how
            // a ghost's links disappear with the ghost.
            .filter(|[from, to]| *from != u32::MAX && *to != u32::MAX)
            .collect();

        let mut sim = Sim::<2>::new(count, &edges, SimParams::default(), SEED);
        if let Some(previous) = previous {
            // "Did the graph change?" is a question about sets, not about order: the
            // arena is rebuilt from a HashMap, so the same vault can come back with its
            // nodes in a different order, and a layout that treated that as a change
            // would re-heat on every save.
            let same_shape = previous.edges.len() == edges.len()
                && keys(&previous.slots) == keys(&slots)
                && canonical_links(&previous.slots, &previous.edges)
                    == canonical_links(&slots, &edges);
            // Positions are carried by identity either way: slot order is not a fact
            // that survives a rebuild.
            let old: HashMap<&str, usize> = previous
                .slots
                .iter()
                .enumerate()
                .map(|(slot, slot_data)| (slot_data.key.as_str(), slot))
                .collect();
            for (slot, slot_data) in slots.iter().enumerate() {
                if let Some(&was) = old.get(slot_data.key.as_str()) {
                    sim.pos[slot] = previous.sim.pos[was];
                    if same_shape {
                        sim.vel[slot] = previous.sim.vel[was];
                    }
                }
            }
            if same_shape {
                sim.alpha = previous.sim.alpha;
            } else {
                sim.reheat(REHEAT);
            }
        }
        Self { sim, slots, edges }
    }

    /// Step the sim up to `ticks` times, stopping when it settles. True if it
    /// moved, which is what tells the window whether a repaint is owed.
    ///
    /// This is the synchronous door the tests drive the layout through: nothing
    /// here waits on a thread, so a headless test can produce an exactly-settled
    /// graph and assert on what it paints.
    pub(crate) fn advance(&mut self, ticks: usize) -> bool {
        let mut moved = false;
        for _ in 0..ticks {
            if !self.sim.step() {
                break;
            }
            moved = true;
        }
        moved
    }

    /// Whether the sim has cooled and the beat can stop (the idling rule: with the
    /// sim settled and drift off, the window must draw zero frames).
    pub(crate) fn settled(&self) -> bool {
        self.sim.is_settled()
    }

    /// Move a body to `world` and hold it there: the node drag.
    ///
    /// Pinning is what makes a drag behave — an unpinned node is fought by every force
    /// in the layout, so the pointer and the springs would both move it and the graph
    /// would squirm under the cursor. The heat is gentle on purpose: the dragged node
    /// goes where it is told, and its neighbours have time to follow rather than the
    /// whole graph throwing itself apart.
    pub(crate) fn drag_node(&mut self, slot: usize, world: [f32; 2]) {
        if slot >= self.slots.len() {
            return;
        }
        self.sim.pos[slot] = world;
        self.sim.vel[slot] = [0.0; 2];
        self.sim.pinned[slot] = true;
        self.reheat(REHEAT);
    }

    /// Let go of a dragged body. `pinned` — a shift-drag — leaves it where the user put
    /// it, which is how a graph gets arranged by hand; otherwise the forces take it
    /// back, and the layout re-heats so it can.
    pub(crate) fn release_node(&mut self, slot: usize, pinned: bool) {
        if slot >= self.slots.len() {
            return;
        }
        self.sim.pinned[slot] = pinned;
        if !pinned {
            self.reheat(REHEAT);
        }
    }

    /// Which bodies are held against the forces, for the tests that check a drag
    /// pinned a node and that letting go un-pinned it.
    #[cfg(test)]
    pub(crate) fn pinned(&self) -> &[bool] {
        &self.sim.pinned
    }

    /// The bodies' world positions, for the tests that ask the simulation directly
    /// rather than through the frame the tab is painting — a drag moves the sim, and
    /// the frame follows on the next tick.
    #[cfg(test)]
    pub(crate) fn positions(&self) -> Vec<[f32; 2]> {
        self.sim.positions().to_vec()
    }

    /// Warm the sim again — a filter change (Task 8) or a node drag will want this.
    pub(crate) fn reheat(&mut self, alpha: f32) {
        self.sim.reheat(alpha);
    }

    /// The picture for the tab: positions, radii, kinds, labels, edges, and the grid
    /// built over them for the pointer's questions.
    pub(crate) fn frame(&self) -> GraphFrame {
        let positions = self.sim.positions();
        let nodes = self
            .slots
            .iter()
            .enumerate()
            .map(|(slot, slot_data)| GraphNode {
                position: positions[slot],
                radius: self.sim.radius_of(slot),
                kind: slot_data.kind,
                degree: slot_data.degree,
                label: SharedString::from(slot_data.label.clone()),
                path: slot_data
                    .path
                    .as_ref()
                    .map(|path| SharedString::from(path.display().to_string())),
            })
            .collect();
        // The box the layout occupies, in one pass over the positions the sim is
        // already holding. Two things need it: the pointer's hit test, because core's
        // grid has no answer for a query outside the box it was built from, and the
        // cell size below.
        let bounds = positions
            .iter()
            .fold(None::<([f32; 2], [f32; 2])>, |acc, p| match acc {
                None => Some(([p[0], p[1]], [p[0], p[1]])),
                Some((mut lo, mut hi)) => {
                    lo[0] = lo[0].min(p[0]);
                    lo[1] = lo[1].min(p[1]);
                    hi[0] = hi[0].max(p[0]);
                    hi[1] = hi[1].max(p[1]);
                    Some((lo, hi))
                }
            });
        // Cells about twice the largest node, which is what core's own note on `Grid`
        // asks for: few enough cells that a hit test looks at a handful of nodes, large
        // enough that no node can be missed by the 3×3 neighbourhood. But never smaller
        // than one [`GRID_MAX_SPAN`]th of the layout, because `Grid::build` allocates a
        // bucket per cell: a layout that has flown apart spans a billion world units, and
        // a cell sized off the node radii alone then asks for a hundred gigabytes (the
        // 1k fixture did exactly this — PROJECT.md thread 39). A wide cell costs hit-test
        // precision, which is the cheaper thing to lose.
        let biggest = self.sim.radius.iter().copied().fold(0.0, f32::max);
        let wanted = (2.0 * biggest).max(8.0);
        let cell = match bounds {
            Some(([lo_x, lo_y], [hi_x, hi_y])) => {
                let floor = (hi_x - lo_x).max(hi_y - lo_y) / GRID_MAX_SPAN;
                if floor > wanted {
                    // Debug, not warn: a layout that is diverging re-frames every tick,
                    // and a log line per frame is not evidence, it is noise.
                    tracing::debug!(
                        span = (hi_x - lo_x).max(hi_y - lo_y),
                        cell = floor,
                        "the graph's layout is spread thin; widening the hit-test grid"
                    );
                }
                wanted.max(floor)
            }
            None => wanted,
        };
        let grid = Grid::build(positions, cell);
        GraphFrame {
            nodes,
            edges: self.edges.clone(),
            grid,
            bounds,
        }
    }
}
