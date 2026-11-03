//! The picture the window ships to the graph tab.
//!
//! The tab never reaches into the simulation: the window owns one sim per window
//! (the integration contract's rule — a second view of the same vault wants the
//! same positions, not its own physics), and what a view gets is a snapshot it can
//! paint at its leisure. That is also what makes the headless tests honest: they
//! build a frame by stepping the sim synchronously and then draw it.

use gpui::SharedString;
use shale_core::graph::NodeKind;

/// One node as the tab sees it.
pub(crate) struct GraphNode {
    /// World position, straight out of the simulation.
    pub(crate) position: [f32; 2],
    /// Radius in world units (`4 + 2·√degree`), so a hub is a bigger dot.
    pub(crate) radius: f32,
    /// Note, attachment, ghost or tag — what decides the fill.
    pub(crate) kind: NodeKind,
    /// How many links hang off it. Decides who gets a label when they cannot
    /// all have one, and will decide what a hover panel says.
    pub(crate) degree: u32,
    /// The note stem, the tag name without `#`, or a ghost's raw target.
    pub(crate) label: SharedString,
    /// What the node stands for, for the click path (Task 7): a note's
    /// vault-relative path, and `None` for a tag or a ghost.
    ///
    /// Not read yet: painting does not need it, and Task 7's click-to-open will.
    #[allow(dead_code)]
    pub(crate) path: Option<SharedString>,
}

/// A snapshot of the window's simulation and the graph it is a simulation of.
///
/// Slot order is the sim's, so an edge is a pair of indexes into `nodes`. The
/// window rebuilds this whenever the sim moves or the graph changes underneath it.
pub(crate) struct GraphFrame {
    /// The nodes, in sim slot order.
    pub(crate) nodes: Vec<GraphNode>,
    /// Edges as slot pairs into `nodes`. A→B and B→A is one edge already,
    /// because the index never holds parallel edges.
    pub(crate) edges: Vec<[u32; 2]>,
}

impl GraphFrame {
    /// An empty frame: a vault with nothing in it, or a graph that failed to
    /// build. Painting it draws the canvas with nothing in it, which is a
    /// different fact from "no canvas at all".
    pub(crate) fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}
