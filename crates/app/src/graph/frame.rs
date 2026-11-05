//! The picture the window ships to the graph tab.
//!
//! The tab never reaches into the simulation: the window owns one sim per window
//! (the integration contract's rule — a second view of the same vault wants the
//! same positions, not its own physics), and what a view gets is a snapshot it can
//! paint at its leisure. That is also what makes the headless tests honest: they
//! build a frame by stepping the sim synchronously and then draw it.
//!
//! The snapshot carries its own [`Grid`] because the pointer's questions — "which
//! node is this click on?", "which node is north-west of this one?" — are asked on
//! every mouse move, and scanning a few thousand nodes to answer one is the kind of
//! cost that only shows up on a big vault.

use gpui::SharedString;
use shale_core::graph::geom::Grid;
use shale_core::graph::NodeKind;

/// One node as the tab sees it.
pub(crate) struct GraphNode {
    /// World position, straight out of the simulation.
    pub(crate) position: [f32; 2],
    /// The third axis: 0 for every node of a flat layout, and the axis the 3D mode
    /// projects (roadmap P3.6). The 2D path never reads it, which is what lets one
    /// frame serve both views.
    pub(crate) depth: f32,
    /// Radius in world units (`4 + 2·√degree`), so a hub is a bigger dot.
    pub(crate) radius: f32,
    /// Note, attachment, ghost or tag — what decides the fill.
    pub(crate) kind: NodeKind,
    /// How many links hang off it. Decides who gets a label when they cannot all
    /// have one, and what a hover ring is drawn around.
    pub(crate) degree: u32,
    /// The note stem, the tag name without `#`, or a ghost's raw target.
    pub(crate) label: SharedString,
    /// What the node stands for: a note's or attachment's vault-relative path, and
    /// `None` for a tag or a ghost — which is what a click uses to know there is
    /// nothing to open.
    pub(crate) path: Option<SharedString>,
}

/// A snapshot of the window's simulation and the graph it is a simulation of.
///
/// Slot order is the sim's, so an edge is a pair of indexes into `nodes`. The
/// window rebuilds this whenever the sim moves or the graph changes underneath it.
pub(crate) struct GraphFrame {
    /// The nodes, in sim slot order.
    pub(crate) nodes: Vec<GraphNode>,
    /// Edges as slot pairs into `nodes`. A→B and B→A is one edge already, because
    /// the index never holds parallel edges.
    pub(crate) edges: Vec<[u32; 2]>,
    /// The spatial index over `nodes`, built with them. Only the pointer's questions
    /// use it, and it is rebuilt per frame rather than per question. Set by the sim,
    /// which is the only thing that can build a frame's nodes.
    pub(super) grid: Grid,
    /// The box the layout occupies in world space, as `(lo, hi)`, or `None` when there
    /// is no layout.
    ///
    /// The hit test keeps it because [`Grid`] only knows the box it was built from: a
    /// query outside it is answered "nothing", and the pointer is outside it constantly
    /// — the nodes on the layout's own edge are on the box's edge, and the screen→world
    /// round trip lands a hair past it. See [`GraphFrame::node_at`].
    pub(super) bounds: Option<([f32; 2], [f32; 2])>,
}

impl GraphFrame {
    /// The radius of the sphere the layout lives in, in world units. This is the one
    /// measurement the projected camera needs about the layout as a whole: it is what
    /// keeps the eye outside the cloud when fitting, and what bounds the dolly
    /// (see `shale_core::graph::proj::Cam3`).
    pub(crate) fn bounding_radius(&self) -> f32 {
        self.nodes
            .iter()
            .map(|n| {
                n.position[0] * n.position[0]
                    + n.position[1] * n.position[1]
                    + n.depth * n.depth
            })
            .fold(0.0_f32, f32::max)
            .sqrt()
    }

    /// An empty frame: a vault with nothing in it, or a graph that failed to build.
    /// Painting it draws the canvas with nothing in it, which is a different fact from
    /// "no canvas at all".
    pub(crate) fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            grid: Grid::default(),
            bounds: None,
        }
    }

    /// The world positions in slot order, which is the shape both the camera and
    /// [`Grid`] want.
    pub(crate) fn positions(&self) -> Vec<[f32; 2]> {
        self.nodes.iter().map(|node| node.position).collect()
    }

    /// The node under `world`, or `None`. `slop` world units are added to the hit
    /// bound so a dot that is a few px across on screen is still clickable; the
    /// candidate the grid returns is then checked against its own radius, so the slop
    /// never makes a click land on a node the pointer is not near.
    pub(crate) fn node_at(&self, world: [f32; 2], slop: f32) -> Option<usize> {
        let biggest = self
            .nodes
            .iter()
            .map(|node| node.radius)
            .fold(0.0, f32::max);
        let positions = self.positions();
        let hit = self
            .grid
            .nearest(&positions, world, biggest + slop, None)
            .or_else(|| {
                // Nothing in the grid's neighbourhood. If the query is outside the box
                // the grid was built from, ask again from the nearest point inside it:
                // a pointer beside an edge node is a pointer on that node, and the
                // round trip from screen px puts it just past the box every time the
                // graph is framed to fit. The distance check below is still against the
                // pointer's real position, so this cannot make a far click land.
                let (lo, hi) = self.bounds?;
                let clamped = [world[0].clamp(lo[0], hi[0]), world[1].clamp(lo[1], hi[1])];
                if clamped == world {
                    return None;
                }
                self.grid.nearest(&positions, clamped, biggest + slop, None)
            });
        hit.map(|slot| slot as usize)
            .filter(|&slot| match self.nodes.get(slot) {
                Some(node) => distance(node.position, world) <= node.radius + slop,
                None => false,
            })
    }

    /// The node nearest `world`, however far it is. The keyboard's way to get a first
    /// selection when the pointer has never been over the canvas.
    pub(crate) fn nearest(&self, world: [f32; 2]) -> Option<usize> {
        self.nodes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                distance(a.position, world).total_cmp(&distance(b.position, world))
            })
            .map(|(slot, _)| slot)
    }

    /// `h`/`j`/`k`/`l`: the node nearest the ray from `from` in direction `dir`, within
    /// the core's ±30° cone. `None` when there is nothing that way, which leaves the
    /// selection where it is rather than throwing it across the graph. This is a scan
    /// rather than a grid lookup — core's `in_cone` is one — and it only runs on a
    /// keypress, so the scan is the honest choice.
    pub(crate) fn node_toward(&self, from: usize, dir: [f32; 2]) -> Option<usize> {
        if from >= self.nodes.len() {
            return None;
        }
        self.grid
            .in_cone(&self.positions(), from as u32, dir, None)
            .map(|slot| slot as usize)
    }

    /// Where a node is, by slot. `None` once the slot is gone, which is what a frame
    /// that arrived under a held pointer means. Test-only today: the pointer's own path
    /// reads the node out of `nodes` directly, and this is the door `graph_node_screen`
    /// aims the pointer through.
    #[cfg(test)]
    pub(crate) fn node_position(&self, slot: usize) -> Option<[f32; 2]> {
        self.nodes.get(slot).map(|node| node.position)
    }

    /// What a node stands for, by slot: `None` for a tag or a ghost, which have no
    /// note to open.
    pub(crate) fn node_path(&self, slot: usize) -> Option<SharedString> {
        self.nodes.get(slot)?.path.clone()
    }
}

/// Straight-line distance in world units.
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
