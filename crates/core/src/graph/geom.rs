//! The graph tab's pure geometry: camera, hit-testing, local-graph BFS,
//! and the constellation spanning tree. No gpui types — `Point`/`Rect`
//! enter at the paint boundary only. Everything here is unit-testable and
//! headless-verifiable, which is how the plan's "verify zoom-anchor math
//! and hit-test headlessly" line gets satisfied before a window exists.

/// The graph tab's camera: world→screen is `screen = world * zoom + pan`
/// (world units are sim coordinates; screen is logical px inside the tab).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub pan: [f32; 2],
    pub zoom: f32,
}

/// Zoom bounds. 0.05 keeps a 2k-node vault's whole layout visible on a
/// small screen; 8x is close enough that node labels are the only thing
/// in view (past that, zooming is pointless — the labels cap out).
pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 8.0;

/// Wheel notch / keypress zoom factor (d3's default is 1.1; 1.15 feels
/// right at label scale).
pub const ZOOM_STEP: f32 = 1.15;

impl Camera {
    pub fn new(pan: [f32; 2], zoom: f32) -> Self {
        Self {
            pan,
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        }
    }

    pub fn to_screen(&self, world: [f32; 2]) -> [f32; 2] {
        [
            world[0] * self.zoom + self.pan[0],
            world[1] * self.zoom + self.pan[1],
        ]
    }

    pub fn to_world(&self, screen: [f32; 2]) -> [f32; 2] {
        [
            (screen[0] - self.pan[0]) / self.zoom,
            (screen[1] - self.pan[1]) / self.zoom,
        ]
    }

    /// Zoom by `factor` keeping the world point under `anchor` (screen px)
    /// fixed — the cursor-anchored wheel zoom. Pan adjusts so
    /// `to_screen(to_world(anchor))` is unchanged at the new zoom.
    pub fn zoom_at(&mut self, anchor: [f32; 2], factor: f32) {
        let world = self.to_world(anchor);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan = [
            anchor[0] - world[0] * self.zoom,
            anchor[1] - world[1] * self.zoom,
        ];
    }

    pub fn pan_by(&mut self, d: [f32; 2]) {
        self.pan[0] += d[0];
        self.pan[1] += d[1];
    }

    /// Fit `positions` into `bounds` ([x, y, w, h] screen px) with `margin`
    /// px of breathing room: the initial camera and the `f`/fit key. A
    /// single point or empty set keeps the current zoom and centers.
    pub fn fit(positions: &[[f32; 2]], bounds: [f32; 4], margin: f32) -> Self {
        if positions.is_empty() {
            return Camera::new(
                [bounds[0] + bounds[2] / 2.0, bounds[1] + bounds[3] / 2.0],
                1.0,
            );
        }
        let (mut lo, mut hi) = ([f32::MAX, f32::MAX], [f32::MIN, f32::MIN]);
        for p in positions {
            lo[0] = lo[0].min(p[0]);
            lo[1] = lo[1].min(p[1]);
            hi[0] = hi[0].max(p[0]);
            hi[1] = hi[1].max(p[1]);
        }
        let w = (hi[0] - lo[0]).max(1.0);
        let h = (hi[1] - lo[1]).max(1.0);
        let zoom = ((bounds[2] - 2.0 * margin) / w)
            .min((bounds[3] - 2.0 * margin) / h)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        let cx = (lo[0] + hi[0]) / 2.0;
        let cy = (lo[1] + hi[1]) / 2.0;
        Camera::new(
            [
                bounds[0] + bounds[2] / 2.0 - cx * zoom,
                bounds[1] + bounds[3] / 2.0 - cy * zoom,
            ],
            zoom,
        )
    }
}

/// Uniform-grid hit-testing over world positions: the hover/click path and
/// the `hjkl` cone search must not scan 2k nodes per mouse move. Cells are
/// `cell` world units (pick ~2x the largest node radius); rebuild is O(n)
/// and cheap enough to redo on every structural change, but not per frame
/// while the sim runs — during sim ticks, hit-test against the grid built
/// at the last settle plus a linear scan fallback is overkill; instead the
/// tab rebuilds the grid every tick it paints (2k nodes ≈ microseconds).
#[derive(Debug, Default, Clone)]
pub struct Grid {
    cell: f32,
    cols: i64,
    rows: i64,
    /// origin of cell (0,0) in world space
    origin: [f32; 2],
    buckets: Vec<Vec<u32>>,
}

impl Grid {
    pub fn build(positions: &[[f32; 2]], cell: f32) -> Self {
        let cell = cell.max(1.0);
        if positions.is_empty() {
            return Self {
                cell,
                ..Default::default()
            };
        }
        let (mut lo, mut hi) = ([f32::MAX, f32::MAX], [f32::MIN, f32::MIN]);
        for p in positions {
            lo[0] = lo[0].min(p[0]);
            lo[1] = lo[1].min(p[1]);
            hi[0] = hi[0].max(p[0]);
            hi[1] = hi[1].max(p[1]);
        }
        let cols = ((hi[0] - lo[0]) / cell).floor() as i64 + 1;
        let rows = ((hi[1] - lo[1]) / cell).floor() as i64 + 1;
        let mut buckets = vec![Vec::new(); (cols.max(1) * rows.max(1)) as usize];
        for (i, p) in positions.iter().enumerate() {
            let c = ((p[0] - lo[0]) / cell).floor() as i64;
            let r = ((p[1] - lo[1]) / cell).floor() as i64;
            buckets[(c + r * cols) as usize].push(i as u32);
        }
        Self {
            cell,
            cols,
            rows,
            origin: lo,
            buckets,
        }
    }

    fn cell_of(&self, w: [f32; 2]) -> Option<(i64, i64)> {
        let c = ((w[0] - self.origin[0]) / self.cell).floor() as i64;
        let r = ((w[1] - self.origin[1]) / self.cell).floor() as i64;
        if (0..self.cols).contains(&c) && (0..self.rows).contains(&r) {
            Some((c, r))
        } else {
            None
        }
    }

    /// The visible node whose center is nearest to `world` within `radius`
    /// world units (pass the node's own radius + a few px of slop), or
    /// None. Checks the 3x3 cell neighborhood.
    pub fn nearest(
        &self,
        positions: &[[f32; 2]],
        world: [f32; 2],
        radius: f32,
        visible: Option<&[bool]>,
    ) -> Option<u32> {
        let (c0, r0) = self.cell_of(world)?;
        let mut best: Option<(u32, f32)> = None;
        for r in (r0 - 1)..=(r0 + 1) {
            for c in (c0 - 1)..=(c0 + 1) {
                if c < 0 || r < 0 || c >= self.cols || r >= self.rows {
                    continue;
                }
                for &i in &self.buckets[(c + r * self.cols) as usize] {
                    if let Some(vis) = visible {
                        if !vis[i as usize] {
                            continue;
                        }
                    }
                    let p = positions[i as usize];
                    let d2 = (p[0] - world[0]).powi(2) + (p[1] - world[1]).powi(2);
                    if d2 <= radius * radius && best.is_none_or(|(_, bd)| d2 < bd) {
                        best = Some((i, d2));
                    }
                }
            }
        }
        best.map(|(i, _)| i)
    }

    /// `hjkl` selection: from `from`, the visible node nearest to the ray
    /// in direction `dir` (unit-ish) within a ±30° cone, scored by distance.
    /// Returns None if nothing is in the cone.
    pub fn in_cone(
        &self,
        positions: &[[f32; 2]],
        from: u32,
        dir: [f32; 2],
        visible: Option<&[bool]>,
    ) -> Option<u32> {
        let origin = positions[from as usize];
        let dl = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt().max(1e-6);
        let (dx, dy) = (dir[0] / dl, dir[1] / dl);
        // cos(30°) ≈ 0.866; accept 0.85 for float slop.
        const CONE: f32 = 0.85;
        let mut best: Option<(u32, f32)> = None;
        for (i, p) in positions.iter().enumerate() {
            let i = i as u32;
            if i == from {
                continue;
            }
            if let Some(vis) = visible {
                if !vis[i as usize] {
                    continue;
                }
            }
            let vx = p[0] - origin[0];
            let vy = p[1] - origin[1];
            let d = (vx * vx + vy * vy).sqrt();
            if d < 1e-4 {
                continue;
            }
            if (vx * dx + vy * dy) / d < CONE {
                continue;
            }
            // Score by distance with a small angular penalty so a node
            // dead-ahead wins over one at the cone's edge.
            let off = 1.0 - (vx * dx + vy * dy) / d;
            let score = d * (1.0 + 2.0 * off);
            if best.is_none_or(|(_, bs)| score < bs) {
                best = Some((i, score));
            }
        }
        best.map(|(i, _)| i)
    }
}

/// The local graph: BFS out to `depth` hops from `root` over visible
/// nodes, returning the visibility bitmask restricted to that neighborhood
/// (same shape as [`super::filter`]'s, so the renderer's path is shared).
/// `neighbors` is the index's adjacency; `visible` is the current filter
/// mask (a node outside the filter never joins the neighborhood).
pub fn local_mask(
    node_count: usize,
    root: u32,
    depth: usize,
    neighbors: &impl Fn(u32) -> Vec<u32>,
    visible: &[bool],
) -> Vec<bool> {
    let mut mask = vec![false; node_count];
    if root as usize >= node_count || !visible[root as usize] {
        return mask;
    }
    // depth 0 = just the root; each hop expands the frontier once.
    let mut frontier = vec![root];
    mask[root as usize] = true;
    for _ in 0..depth {
        let mut next = Vec::new();
        for &n in &frontier {
            for m in neighbors(n) {
                if !mask[m as usize] && visible[m as usize] {
                    mask[m as usize] = true;
                    next.push(m);
                }
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
    mask
}

/// Prim's minimum spanning tree over `nodes` (indices into `positions`),
/// edge weight = squared distance. Returns edge pairs (positions of the
/// input nodes, not world indices) — the constellation "star lines" of
/// M3's constellations view. O(n²) is right for cluster-sized sets (tens
/// to low hundreds of nodes); recomputed only on settle or filter change.
pub fn mst_edges(nodes: &[u32], positions: &[[f32; 2]]) -> Vec<[u32; 2]> {
    if nodes.len() < 2 {
        return Vec::new();
    }
    let n = nodes.len();
    let dist2 = |a: u32, b: u32| {
        let p = positions[a as usize];
        let q = positions[b as usize];
        (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)
    };
    // Prim over the complete graph, implicit: `low` = cheapest edge from
    // the tree to each outside node.
    let mut in_tree = vec![false; n];
    let mut low = vec![(f32::MAX, 0u32); n]; // (weight, tree endpoint — a world index)
    let mut edges = Vec::with_capacity(n - 1);
    in_tree[0] = true;
    for j in 1..n {
        low[j] = (dist2(nodes[0], nodes[j]), nodes[0]);
    }
    for _ in 1..n {
        let mut pick = usize::MAX;
        for j in 0..n {
            if !in_tree[j] && (pick == usize::MAX || low[j].0 < low[pick].0) {
                pick = j;
            }
        }
        let (w, from) = low[pick];
        edges.push([from, nodes[pick]]);
        in_tree[pick] = true;
        for j in 0..n {
            if !in_tree[j] {
                let d = dist2(nodes[pick], nodes[j]);
                if d < low[j].0 {
                    low[j] = (d, nodes[pick]);
                }
            }
        }
        let _ = w;
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_at_keeps_the_anchor_point_fixed() {
        let mut cam = Camera::new([100.0, -50.0], 1.0);
        let anchor = [37.0, 211.0];
        let world_before = cam.to_world(anchor);
        cam.zoom_at(anchor, ZOOM_STEP);
        cam.zoom_at(anchor, 1.0 / ZOOM_STEP);
        // Round trip returns to the original camera exactly enough.
        assert!((cam.zoom - 1.0).abs() < 1e-5);
        let world_after = cam.to_world(anchor);
        assert!((world_after[0] - world_before[0]).abs() < 1e-3);
        assert!((world_after[1] - world_before[1]).abs() < 1e-3);
    }

    #[test]
    fn zoom_at_pins_the_world_point_under_the_cursor() {
        let mut cam = Camera::new([0.0, 0.0], 1.0);
        let anchor = [500.0, 300.0];
        let w = cam.to_world(anchor);
        cam.zoom_at(anchor, 2.0);
        let s = cam.to_screen(w);
        assert!((s[0] - anchor[0]).abs() < 1e-3 && (s[1] - anchor[1]).abs() < 1e-3);
    }

    #[test]
    fn zoom_clamps_at_both_ends() {
        let mut cam = Camera::new([0.0, 0.0], MIN_ZOOM);
        cam.zoom_at([0.0, 0.0], 0.001);
        assert_eq!(cam.zoom, MIN_ZOOM);
        cam.zoom_at([0.0, 0.0], 1e6);
        assert_eq!(cam.zoom, MAX_ZOOM);
    }

    #[test]
    fn fit_frames_the_spread_with_margin() {
        let pos = [[0.0, 0.0], [100.0, 0.0], [0.0, 50.0], [100.0, 50.0]];
        let cam = Camera::fit(&pos, [0.0, 0.0, 500.0, 300.0], 20.0);
        // The spread must land inside the bounds with margin, and the
        // center of the spread at the center of the bounds.
        for p in &pos {
            let s = cam.to_screen(*p);
            assert!(s[0] >= 19.9 && s[0] <= 480.1, "x {s:?}");
            assert!(s[1] >= 19.9 && s[1] <= 280.1, "y {s:?}");
        }
        let c = cam.to_screen([50.0, 25.0]);
        assert!((c[0] - 250.0).abs() < 0.1 && (c[1] - 150.0).abs() < 0.1);
    }

    #[test]
    fn grid_finds_the_nearest_node_and_skips_hidden_ones() {
        let pos = [[0.0, 0.0], [10.0, 0.0], [7.0, 3.0], [200.0, 200.0]];
        let grid = Grid::build(&pos, 16.0);
        assert_eq!(grid.nearest(&pos, [9.0, 0.5], 4.0, None), Some(1));
        assert_eq!(
            grid.nearest(&pos, [9.0, 0.5], 4.0, Some(&[true, false, true, true])),
            Some(2)
        );
        // Outside the radius: nothing.
        assert_eq!(grid.nearest(&pos, [5.0, 5.0], 2.0, None), None);
        // Far outside the grid bounds entirely.
        assert_eq!(grid.nearest(&pos, [9999.0, 9999.0], 4.0, None), None);
    }

    #[test]
    fn cone_search_picks_dead_ahead_not_cone_edge() {
        let pos = [[0.0, 0.0], [10.0, 0.0], [9.0, 4.0]]; // #2 is ~24° off
        let grid = Grid::build(&pos, 16.0);
        assert_eq!(grid.in_cone(&pos, 0, [1.0, 0.0], None), Some(1));
        // Nothing behind.
        assert_eq!(grid.in_cone(&pos, 0, [-1.0, 0.0], None), None);
        // With #1 hidden, the cone-edge node wins.
        assert_eq!(
            grid.in_cone(&pos, 0, [1.0, 0.0], Some(&[true, false, true])),
            Some(2)
        );
    }

    #[test]
    fn local_mask_is_bfs_depth_and_respects_the_filter() {
        // chain 0-1-2-3, plus 4 linked only to 0
        let adj: Vec<Vec<u32>> = vec![vec![1, 4], vec![0, 2], vec![1, 3], vec![2], vec![0]];
        let neighbors = |n: u32| adj[n as usize].clone();
        let all = vec![true; 5];
        let m = local_mask(5, 0, 1, &neighbors, &all);
        assert_eq!(m, vec![true, true, false, false, true]);
        let m2 = local_mask(5, 0, 2, &neighbors, &all);
        assert_eq!(m2, vec![true, true, true, false, true]);
        // depth 0 = the root alone.
        assert_eq!(
            local_mask(5, 0, 0, &neighbors, &all),
            vec![true, false, false, false, false]
        );
        let filtered = vec![true, false, true, true, true];
        let m3 = local_mask(5, 0, 3, &neighbors, &filtered);
        assert_eq!(m3, vec![true, false, false, false, true]);
    }

    #[test]
    fn mst_is_a_tree_and_uses_short_edges() {
        // square with a long diagonal available: MST = 3 sides, never 2 sides + diagonal
        let pos = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let nodes = [0u32, 1, 2, 3];
        let edges = mst_edges(&nodes, &pos);
        assert_eq!(edges.len(), 3);
        let total: f32 = edges
            .iter()
            .map(|[a, b]| {
                let p = pos[*a as usize];
                let q = pos[*b as usize];
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
            })
            .sum();
        assert!((total - 30.0).abs() < 1e-3, "MST total {total}");
        // Connected: union-find over the edges.
        let mut parent = vec![0, 1, 2, 3];
        fn find(p: &mut Vec<u32>, x: u32) -> u32 {
            if p[x as usize] != x {
                let r = find(p, p[x as usize]);
                p[x as usize] = r;
            }
            p[x as usize]
        }
        for [a, b] in &edges {
            let (ra, rb) = (find(&mut parent, *a), find(&mut parent, *b));
            parent[ra as usize] = rb;
        }
        let roots: Vec<u32> = (0..4).map(|x| find(&mut parent, x)).collect();
        assert_eq!(roots, vec![3; 4]);
    }

    #[test]
    fn mst_handles_nodes_that_are_not_0_based() {
        // The same square, but the node list starts at world index 5 and is
        // unsorted — the tree endpoints must be world indices throughout.
        let pos = [
            [0.0, 0.0],
            [1.0, 1.0], // decoys
            [1.0, 1.0],
            [10.0, 0.0],
            [1.0, 1.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [1.0, 1.0],
            [10.0, 10.0],
            [1.0, 1.0],
            [0.0, 10.0],
        ];
        let nodes = [6u32, 3, 9, 11]; // a 10x10 square in world indices
        let edges = mst_edges(&nodes, &pos);
        assert_eq!(edges.len(), 3);
        let total: f32 = edges
            .iter()
            .map(|[a, b]| {
                let p = pos[*a as usize];
                let q = pos[*b as usize];
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
            })
            .sum();
        assert!((total - 30.0).abs() < 1e-3, "MST total {total}");
    }
}
