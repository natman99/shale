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
    /// Build the sim for `index`, carrying over whatever layout `previous` had.
    ///
    /// Three cases, and the difference between them is what the user sees:
    /// - **No previous**: a fresh layout at full heat (the phyllotaxis spiral,
    ///   then the forces sort it out).
    /// - **Same shape** — the same nodes and the same edges, which is what a
    ///   prose edit that changes no links produces: the old positions, velocities
    ///   and heat are copied across, so the graph does not move at all.
    /// - **Changed shape**: surviving nodes keep their positions and the sim
    ///   re-heats gently, so the new ones find a place without the rest jumping.
    pub(crate) fn build(index: &GraphIndex, previous: Option<&GraphSim>) -> Self {
        let graph = index.graph();
        let nodes: Vec<_> = graph.node_indices().collect();
        let count = nodes.len();

        // `slot_of` maps a petgraph index to its slot. The arena is rebuilt fresh
        // on every change, so the two are usually identical — but "usually" is not
        // a thing to build a layout on, and this costs one vector.
        let mut slot_of = vec![u32::MAX; count];
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

    /// Warm the sim again — a filter change or a node drag (Task 7) will want this.
    #[allow(dead_code)]
    pub(crate) fn reheat(&mut self, alpha: f32) {
        self.sim.reheat(alpha);
    }

    /// The picture for the tab: positions, radii, kinds, labels, edges.
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
        GraphFrame {
            nodes,
            edges: self.edges.clone(),
        }
    }
}
