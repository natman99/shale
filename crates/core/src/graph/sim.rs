//! The force simulation the dynamic graph view runs: link springs, pairwise
//! repulsion, and a pull toward the center — the same force vocabulary
//! Obsidian's Forces panel exposes (link force, link distance, repel force,
//! center force), modelled after d3-force because that is what Obsidian
//! itself runs.
//!
//! The sim is pure data: [`Sim::step`] touches no gpui type and no IO, so it
//! runs on a background thread and ships positions to the UI as one
//! `Vec<[f32; D]>` per tick (PROJECT.md's idling rules: the sim sleeps at
//! `is_settled`, and the UI thread stops drawing when it does).
//!
//! Dimension-generic: `Sim<2>` is the flat graph, `Sim<3>` is the projected
//! 3D view (M2) — same forces, same tuning, one code path monomorphized per
//! dimension (d3-force-3d is the same library with one extra axis, so this
//! mirrors Obsidian's own structure). The Barnes-Hut tree generalizes the
//! same way: quadtree for D=2, octree for D=3 ([`super::quadtree`]).
//!
//! Determinism: initial placement is seeded (phyllotaxis spiral in 2D,
//! Fibonacci sphere in 3D — the same construction d3-force uses), so the
//! same vault and the same seed produce byte-identical layouts — which is
//! what makes the static-mode cache (graph-view-plan.md M3) and the
//! headless tests meaningful.

// The dimension loops (`for d in 0..D`) are inherently index-based: D is a
// const generic and the loops index two or three arrays at once, which is
// exactly what needless_range_loop wants to rewrite away. It can't here.
#![allow(clippy::needless_range_loop)]

/// Below this node count the sim uses the O(n²) pair loop; above it, the
/// Barnes-Hut quadtree in [`super::quadtree`]. Measured crossover on this
/// machine is ~250 (probe_crossover): the tree's build cost dominates
/// until then, and BH wins 1.5x-6x beyond it.
pub const NAIVE_LIMIT: usize = 256;

/// Tunables, in d3-force's vocabulary so the settings panel maps 1:1.
#[derive(Debug, Clone)]
pub struct SimParams {
    /// Many-body repulsion strength; more negative pushes harder (d3's
    /// `forceManyBody().strength()`, default -30, scaled for our units).
    pub repel: f32,
    /// Spring constant per link, d3-style: `link_strength / min(deg_a, deg_b)`
    /// so a hub's links stay loose while a leaf's snap tight.
    pub link_strength: f32,
    /// Rest length of every link, in world units (d3's `distance`, default 30).
    pub link_distance: f32,
    /// Pull toward the origin (d3's `forceCenter`/`forceX+forceY` blend).
    pub center: f32,
    /// Radius-based overlap push, so big nodes stay readable (d3's
    /// `forceCollide`).
    pub collide: bool,
    /// Per-tick decay toward zero (d3's `alphaDecay`, default 0.0228).
    pub alpha_decay: f32,
    /// Below this alpha the sim is settled and the caller may stop ticking.
    pub alpha_min: f32,
}

impl Default for SimParams {
    fn default() -> Self {
        Self {
            repel: -90.0,
            link_strength: 0.7,
            link_distance: 34.0,
            center: 0.02,
            collide: true,
            alpha_decay: 0.0228,
            alpha_min: 0.001,
        }
    }
}

/// The simulation state: SoA arrays indexed by `NodeIndex::index()`.
/// `D` is the number of spatial dimensions (2 or 3).
#[derive(Debug, Clone)]
pub struct Sim<const D: usize> {
    /// World positions.
    pub pos: Vec<[f32; D]>,
    /// Velocities; d3's velocity-Verlet keeps them between ticks.
    pub vel: Vec<[f32; D]>,
    /// Node radii (from degree), used by collide and by the renderer.
    pub radius: Vec<f32>,
    /// Pinned nodes ignore forces and hold their position (drag, orbit
    /// focus's center, Shift-pinned nodes).
    pub pinned: Vec<bool>,
    /// The cooling schedule: 1.0 at re-heat, decays to `alpha_min`.
    pub alpha: f32,
    /// The tunables.
    pub params: SimParams,
    /// Edge endpoints as index pairs, flattened for cache-friendly stepping.
    edges: Vec<[u32; 2]>,
    /// Per-edge spring bias (`deg_a / (deg_a + deg_b)`), precomputed: the
    /// degrees never change after construction, so recomputing it per tick
    /// is two loads, a max, and a division of pure waste.
    bias: Vec<f32>,
    /// Collide grid scratch (rebuilt per tick; kept here so the hot path
    /// never allocates). `grid_heads[cell] -> first point`, `grid_next[
    /// point] -> next point in the same cell`, u32::MAX ends both.
    grid_heads: Vec<u32>,
    grid_next: Vec<u32>,
    /// Largest node radius (the collide cell-size bound), static after
    /// construction.
    max_radius: f32,
    /// Cached cell index per point (bucketing computes it; the neighbor
    /// walk reuses it instead of re-deriving from the position).
    grid_cell: Vec<u32>,
    /// The 3^D neighbor-cell offsets, decoded once at construction
    /// (decoding per point per offset costs divisions in the hot loop).
    grid_offsets: Vec<[i32; D]>,
    /// The PRNG state (splitmix64), kept so a re-heat does not reseed.
    rng: u64,
}

impl<const D: usize> Sim<D> {
    /// Build a sim for `n` nodes with `edges` given as index pairs.
    /// Positions start on a seeded phyllotaxis spiral (2D) or Fibonacci
    /// sphere (3D).
    pub fn new(n: usize, edges: &[[u32; 2]], params: SimParams, seed: u64) -> Self {
        assert!(
            (2..=3).contains(&D),
            "the graph sim runs in 2 or 3 dimensions"
        );
        let mut degree = vec![0u32; n];
        for &[a, b] in edges {
            if (a as usize) < n && (b as usize) < n {
                degree[a as usize] += 1;
                degree[b as usize] += 1;
            }
        }
        let radius: Vec<f32> = degree.iter().map(|&d| node_radius(d)).collect();
        let max_radius = radius.iter().copied().fold(0.0f32, f32::max);
        let bias: Vec<f32> = edges
            .iter()
            .map(|&[a, b]| {
                let da = degree[a as usize].max(1) as f32;
                let db = degree[b as usize].max(1) as f32;
                da / (da + db)
            })
            .collect();

        // d3-force's initial placement: a phyllotaxis spiral, golden angle
        // (π·(3−√5); spelled as a literal because const math has no sqrt).
        // In 3D the same construction becomes a Fibonacci sphere at the
        // spiral's outer radius, keeping node density comparable.
        const GOLDEN: f32 = 2.399_963_2;
        let mut pos = Vec::with_capacity(n);
        let mut rng = seed | 1; // splitmix64 tolerates any state but never 0
        for i in 0..n {
            let mut p = [0.0f32; D];
            let a = i as f32 * GOLDEN;
            if D == 2 {
                let r = 10.0 * (i as f32 + 0.5).sqrt();
                p[0] = r * a.cos();
                p[1] = r * a.sin();
            } else {
                let z = 1.0 - 2.0 * (i as f32 + 0.5) / n.max(1) as f32;
                let rr = (1.0 - z * z).max(0.0).sqrt();
                let s = 10.0 * (n.max(1) as f32).sqrt();
                p[0] = rr * a.cos() * s;
                p[1] = rr * a.sin() * s;
                p[2] = z * s;
            }
            // A touch of jitter breaks the perfect symmetry that would
            // otherwise leave the placement's forces balanced forever.
            for d in 0..D {
                p[d] += (next_f32(&mut rng) - 0.5) * 2.0;
            }
            pos.push(p);
        }

        Self {
            pos,
            vel: vec![[0.0; D]; n],
            radius,
            pinned: vec![false; n],
            alpha: 1.0,
            params,
            edges: edges.to_vec(),
            bias,
            max_radius,
            grid_heads: Vec::new(),
            grid_next: Vec::new(),
            grid_cell: Vec::new(),
            grid_offsets: {
                // All base-3 vectors of length D with digits in {-1,0,+1}:
                // the 3^D neighbor-cell offsets, decoded once here.
                let mut offs = Vec::with_capacity(3usize.pow(D as u32));
                for code in 0..3usize.pow(D as u32) {
                    let mut v = [0i32; D];
                    let mut c = code;
                    for k in 0..D {
                        v[k] = (c % 3) as i32 - 1;
                        c /= 3;
                    }
                    offs.push(v);
                }
                offs
            },
            rng,
        }
    }

    /// Re-heat: the caller (vault edit, filter change, drag) restarts the
    /// cooling schedule.
    pub fn reheat(&mut self, alpha: f32) {
        self.alpha = alpha.max(self.params.alpha_min * 2.0);
    }

    /// True when the sim has cooled and the caller may stop ticking and stop
    /// marking the frame dirty (the idling contract).
    pub fn is_settled(&self) -> bool {
        self.pos.is_empty() || self.alpha < self.params.alpha_min
    }

    /// The node radius convention shared with the renderer:
    /// `4 + 2*sqrt(degree)`.
    pub fn radius_of(&self, node: usize) -> f32 {
        self.radius[node]
    }

    /// World positions, for the renderer and hit-testing (the tab ships a
    /// copy per painted frame; the sim keeps ownership).
    pub fn positions(&self) -> &[[f32; D]] {
        &self.pos
    }

    /// One tick: apply forces, integrate with damping, cool. Returns false
    /// when settled (and does nothing), so the caller's loop can stop.
    pub fn step(&mut self) -> bool {
        if self.is_settled() {
            return false;
        }
        let alpha = self.alpha;
        self.apply_links(alpha);
        self.apply_repulsion(alpha);
        self.apply_center(alpha);
        self.apply_collide(alpha);
        self.integrate();
        self.alpha += (0.0 - self.alpha) * self.params.alpha_decay;
        true
    }

    /// Link springs: pull both ends toward the rest length, weighted by
    /// the *other* end's degree share so a hub moves less than its leaf.
    fn apply_links(&mut self, alpha: f32) {
        let dist = self.params.link_distance;
        let strength = self.params.link_strength;
        for e in 0..self.edges.len() {
            let [a, b] = self.edges[e];
            let (a, b) = (a as usize, b as usize);
            // Destructure once: indexing self.pos[a][d] per axis re-checks
            // the outer bounds every iteration (the hot loops of the whole
            // sim; measured 2x cost if left un-hoisted).
            let pa = self.pos[a];
            let pb = self.pos[b];
            let mut delta = [0.0f32; D];
            let mut d2 = 0.0f32;
            for d in 0..D {
                delta[d] = pb[d] - pa[d];
                d2 += delta[d] * delta[d];
            }
            let len = d2.sqrt().max(0.001);
            // bias is static per edge (degrees never change after
            // construction) — precomputed in `bias`, not per tick.
            let bias = self.bias[e];
            let k = (len - dist) / len * alpha * strength;
            let pin_a = self.pinned[a];
            let pin_b = self.pinned[b];
            if !pin_b {
                let vb = &mut self.vel[b];
                for d in 0..D {
                    vb[d] -= delta[d] * k * bias;
                }
            }
            if !pin_a {
                let va = &mut self.vel[a];
                for d in 0..D {
                    va[d] += delta[d] * k * (1.0 - bias);
                }
            }
        }
    }

    /// Many-body repulsion: naive below NAIVE_LIMIT, Barnes-Hut above.
    fn apply_repulsion(&mut self, alpha: f32) {
        let repel = self.params.repel;
        if self.pos.len() <= NAIVE_LIMIT {
            for i in 0..self.pos.len() {
                if self.pinned[i] {
                    continue;
                }
                let pi = self.pos[i];
                let mut f = [0.0f32; D];
                for j in 0..self.pos.len() {
                    if i == j {
                        continue;
                    }
                    let pj = self.pos[j];
                    let mut delta = [0.0f32; D];
                    let mut d2 = 0.0f32;
                    for d in 0..D {
                        delta[d] = pj[d] - pi[d];
                        d2 += delta[d] * delta[d];
                    }
                    // d3-force's many-body law, exactly: force on i =
                    // delta * (strength * alpha / d²), delta being (j - i).
                    // The magnitude therefore falls off as 1/d (d3's own
                    // falloff); strength is negative, so the force points
                    // away from j. d² is clamped so near-touching nodes do
                    // not explode.
                    if d2 < 0.01 {
                        // Coincident nodes: nudge apart deterministically so
                        // the force never divides by zero.
                        for d in 0..D {
                            f[d] += (next_f32(&mut self.rng) - 0.5) * 10.0;
                        }
                        continue;
                    }
                    let w = repel * alpha / d2.max(1.0);
                    for d in 0..D {
                        f[d] += delta[d] * w;
                    }
                }
                for d in 0..D {
                    self.vel[i][d] += f[d];
                }
            }
        } else {
            crate::graph::quadtree::apply_repulsion(
                &self.pos,
                &mut self.vel,
                &self.pinned,
                repel * alpha,
            );
        }
    }

    /// Centering: a gentle pull toward the origin keeps the graph on
    /// screen (d3's forceX/forceY at `center` strength).
    fn apply_center(&mut self, alpha: f32) {
        let center = self.params.center * alpha;
        for i in 0..self.pos.len() {
            if self.pinned[i] {
                continue;
            }
            for d in 0..D {
                self.vel[i][d] -= self.pos[i][d] * center;
            }
        }
    }

    /// Collide: push overlapping spheres apart by their radii.
    ///
    /// A uniform spatial grid, rebuilt per tick: cell size is the largest
    /// possible contact distance (`2*max_radius + 2`), so two points can
    /// only touch if their cells are within one step on every axis — each
    /// point checks its 3^D neighborhood instead of every other point.
    /// (The old O(n²) pair loop was the single most expensive force:
    /// 13.3 ms/tick at n=5000, more than Barnes-Hut repulsion — see
    /// PROGRESS.md.) Positions change every tick, so the grid is rebuilt
    /// each call; the scratch buffers live in the sim to avoid per-tick
    /// allocation. Falls back to the naive loop when the grid would be
    /// sparse (a pathological layout with a tiny radius and a huge extent).
    fn apply_collide(&mut self, alpha: f32) {
        if !self.params.collide || self.pos.len() < 2 {
            return;
        }
        let n = self.pos.len();

        // Contact distance bound: static after construction (radii never
        // change), precomputed in `max_radius`.
        let cell = 2.0 * self.max_radius + 2.0;

        let mut min = [f32::MAX; D];
        let mut max = [f32::MIN; D];
        for p in &self.pos {
            for k in 0..D {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        let mut dims = [1usize; D];
        let mut total = 1usize;
        for k in 0..D {
            dims[k] = ((max[k] - min[k]) / cell).floor() as usize + 1;
            total = total.saturating_mul(dims[k]);
        }
        if total > 16 * n {
            // Sparse layout: the grid would cost more to clear than the
            // pair loop it saves. Rare enough to keep the fallback simple.
            self.collide_naive(alpha);
            return;
        }

        // Bucket points into the grid (intrusive linked lists: heads per
        // cell, next per point). The cell index is cached per point — the
        // neighbor walk reuses it instead of re-deriving from the position.
        self.grid_heads.clear();
        self.grid_heads.resize(total, u32::MAX);
        self.grid_next.clear();
        self.grid_next.resize(n, u32::MAX);
        self.grid_cell.clear();
        self.grid_cell.resize(n, 0);
        // Stride per axis for the flat cell index (same order as `dims`).
        let mut stride = [1usize; D];
        for k in 1..D {
            stride[k] = stride[k - 1] * dims[k - 1];
        }
        let cell_of = |p: &[f32; D]| -> usize {
            let mut idx = 0usize;
            for k in 0..D {
                // floor can underflow for p[k] < min[k] by float noise;
                // saturating cast + clamp keeps it in range. Clamping is
                // monotone, so it never separates a touching pair beyond
                // one cell step.
                let c = ((p[k] - min[k]) / cell).floor();
                let c = if c <= 0.0 { 0usize } else { c as usize }.min(dims[k] - 1);
                idx += c * stride[k];
            }
            idx
        };
        for i in 0..n {
            let c = cell_of(&self.pos[i]);
            self.grid_cell[i] = c as u32;
            self.grid_next[i] = self.grid_heads[c];
            self.grid_heads[c] = i as u32;
        }

        // For each point, walk the 3^D neighboring cells; process each
        // pair once via j > i. The neighbor cell index is ci + the
        // offset's stride-weighted steps; bounds are checked per axis
        // against the decoded coordinates of ci (decoded once per point).
        for i in 0..n {
            let pi = self.pos[i];
            let ri = self.radius[i];
            let ci = self.grid_cell[i] as usize;
            let mut coord = [0usize; D];
            {
                let mut rem = ci;
                for k in 0..D {
                    coord[k] = rem % dims[k];
                    rem /= dims[k];
                }
            }
            let noffs = self.grid_offsets.len();
            for oi in 0..noffs {
                let off = self.grid_offsets[oi]; // copy; ends the borrow
                let mut cidx = ci;
                let mut ok = true;
                for k in 0..D {
                    let c = coord[k] as i64 + off[k] as i64;
                    if c < 0 || c as usize >= dims[k] {
                        ok = false;
                        break;
                    }
                    cidx = (cidx as i64 + off[k] as i64 * stride[k] as i64) as usize;
                }
                if !ok {
                    continue;
                }
                let mut jw = self.grid_heads[cidx];
                while jw != u32::MAX {
                    let j = jw as usize;
                    if j > i {
                        let pj = self.pos[j];
                        let mut delta = [0.0f32; D];
                        let mut d2 = 0.0f32;
                        for d in 0..D {
                            delta[d] = pj[d] - pi[d];
                            d2 += delta[d] * delta[d];
                        }
                        let want = ri + self.radius[j] + 2.0;
                        if d2 < want * want && d2 >= 0.0001 {
                            let d = d2.sqrt();
                            let push = (want - d) / d * 0.5 * alpha;
                            let pin_i = self.pinned[i];
                            let pin_j = self.pinned[j];
                            for k in 0..D {
                                let o = delta[k] * push;
                                if !pin_i {
                                    self.vel[i][k] -= o;
                                }
                                if !pin_j {
                                    self.vel[j][k] += o;
                                }
                            }
                        }
                    }
                    jw = self.grid_next[j];
                }
            }
        }
    }

    /// The old O(n²) collide, kept as the sparse-grid fallback.
    fn collide_naive(&mut self, alpha: f32) {
        for i in 0..self.pos.len() {
            let pi = self.pos[i];
            for j in (i + 1)..self.pos.len() {
                let pj = self.pos[j];
                let mut delta = [0.0f32; D];
                let mut d2 = 0.0f32;
                for d in 0..D {
                    delta[d] = pj[d] - pi[d];
                    d2 += delta[d] * delta[d];
                }
                let want = self.radius[i] + self.radius[j] + 2.0;
                if d2 >= want * want || d2 < 0.0001 {
                    continue;
                }
                let d = d2.sqrt();
                let push = (want - d) / d * 0.5 * alpha;
                for k in 0..D {
                    let o = delta[k] * push;
                    if !self.pinned[i] {
                        self.vel[i][k] -= o;
                    }
                    if !self.pinned[j] {
                        self.vel[j][k] += o;
                    }
                }
            }
        }
    }

    /// Integrate with d3's 0.6 velocity damping, and sanitize: a NaN from
    /// any source must not poison the whole layout.
    fn integrate(&mut self) {
        for i in 0..self.pos.len() {
            if self.pinned[i] {
                self.vel[i] = [0.0; D];
                continue;
            }
            for d in 0..D {
                self.vel[i][d] *= 0.6;
                self.pos[i][d] += self.vel[i][d];
            }
            if !self.pos[i].iter().all(|v| v.is_finite()) {
                self.pos[i] = [0.0; D];
                self.vel[i] = [0.0; D];
            }
        }
    }
}

/// The shared node-radius convention (graph-view-plan.md): `4 + 2*sqrt(degree)`.
pub fn node_radius(degree: u32) -> f32 {
    4.0 + 2.0 * (degree as f32).sqrt()
}

/// splitmix64: one step of a tiny, fast, well-mixed PRNG. Deterministic
/// across platforms, unlike std's HashMap hasher or getrandom.
fn next_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// A uniform f32 in [0, 1).
fn next_f32(state: &mut u64) -> f32 {
    (next_u64(state) >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(n: u32) -> Vec<[u32; 2]> {
        (1..n).map(|i| [i - 1, i]).collect()
    }

    /// The grid collide must find exactly the pairs the naive loop finds
    /// (and only those) — a missed pair is two nodes overlapping silently;
    /// an extra pair is a force that shouldn't exist.
    #[test]
    fn grid_collide_matches_naive() {
        // Deliberately tight layout: many overlaps, a hub with a big
        // radius next to small leaves, and a coincident pair.
        let edges: Vec<[u32; 2]> = (1..40u32).map(|i| [0, i]).collect();
        let mut sim = Sim::<2>::new(40, &edges, SimParams::default(), 3);
        // Scatter into a small box so plenty of pairs overlap.
        for (i, p) in sim.pos.iter_mut().enumerate() {
            p[0] = (i % 8) as f32 * 3.0;
            p[1] = (i / 8) as f32 * 3.0;
        }
        // A coincident pair (the d2 < 0.0001 skip path).
        sim.pos[5] = sim.pos[6];

        // Snapshot velocities, run grid collide, capture the delta.
        let v0 = sim.vel.clone();
        sim.apply_collide(1.0);
        let grid_delta: Vec<[f32; 2]> = (0..40)
            .map(|i| [sim.vel[i][0] - v0[i][0], sim.vel[i][1] - v0[i][1]])
            .collect();

        // Same starting state through the naive loop.
        let mut sim2 = sim.clone();
        sim2.vel = v0.clone();
        sim2.collide_naive(1.0);
        let naive_delta: Vec<[f32; 2]> = (0..40)
            .map(|i| [sim2.vel[i][0] - v0[i][0], sim2.vel[i][1] - v0[i][1]])
            .collect();

        for i in 0..40 {
            assert!(
                (grid_delta[i][0] - naive_delta[i][0]).abs() < 1e-4
                    && (grid_delta[i][1] - naive_delta[i][1]).abs() < 1e-4,
                "node {i}: grid {:?} vs naive {:?}",
                grid_delta[i],
                naive_delta[i]
            );
        }
    }

    #[test]
    fn same_seed_gives_the_same_layout() {
        let edges = chain(30);
        let mut a = Sim::<2>::new(30, &edges, SimParams::default(), 7);
        let mut b = Sim::<2>::new(30, &edges, SimParams::default(), 7);
        for _ in 0..200 {
            a.step();
            b.step();
        }
        assert_eq!(a.pos, b.pos, "determinism is what the static cache needs");

        let mut c = Sim::<2>::new(30, &edges, SimParams::default(), 8);
        for _ in 0..200 {
            c.step();
        }
        assert_ne!(a.pos, c.pos, "the seed actually seeds");
    }

    #[test]
    fn same_seed_gives_the_same_layout_3d() {
        let edges = chain(30);
        let mut a = Sim::<3>::new(30, &edges, SimParams::default(), 7);
        let mut b = Sim::<3>::new(30, &edges, SimParams::default(), 7);
        for _ in 0..200 {
            a.step();
            b.step();
        }
        assert_eq!(a.pos, b.pos);
    }

    #[test]
    fn it_settles() {
        let edges = chain(50);
        let mut sim = Sim::<2>::new(50, &edges, SimParams::default(), 1);
        let mut ticks = 0;
        while sim.step() {
            ticks += 1;
            assert!(ticks < 5000, "alpha must decay to the floor");
        }
        assert!(sim.is_settled());
    }

    #[test]
    fn it_settles_3d() {
        let edges = chain(50);
        let mut sim = Sim::<3>::new(50, &edges, SimParams::default(), 1);
        let mut ticks = 0;
        while sim.step() {
            ticks += 1;
            assert!(ticks < 5000);
        }
        assert!(sim.is_settled());
    }

    #[test]
    fn two_components_separate() {
        // Two triangles with no edge between them: repulsion must push the
        // clusters apart beyond a link distance.
        let edges = vec![[0u32, 1], [1, 2], [2, 0], [3, 4], [4, 5], [5, 3]];
        let mut sim = Sim::<2>::new(6, &edges, SimParams::default(), 3);
        for _ in 0..600 {
            if !sim.step() {
                break;
            }
        }
        let centroid = |group: &[u32]| -> [f32; 2] {
            let mut c = [0.0; 2];
            for &g in group {
                c[0] += sim.pos[g as usize][0];
                c[1] += sim.pos[g as usize][1];
            }
            [c[0] / group.len() as f32, c[1] / group.len() as f32]
        };
        let a = centroid(&[0, 1, 2]);
        let b = centroid(&[3, 4, 5]);
        let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        assert!(
            d > sim.params.link_distance * 2.0,
            "unlinked clusters drift apart, got {d}"
        );
    }

    #[test]
    fn a_link_pulls_its_endpoints_together() {
        let mut sim = Sim::<2>::new(2, &[[0u32, 1]], SimParams::default(), 1);
        sim.pos[0] = [-400.0, 0.0];
        sim.pos[1] = [400.0, 0.0];
        for _ in 0..600 {
            if !sim.step() {
                break;
            }
        }
        let d = (sim.pos[0][0] - sim.pos[1][0]).abs();
        assert!(d < 200.0, "the spring dragged them from 800 apart to {d}");
    }

    #[test]
    fn pinned_nodes_hold() {
        let mut sim = Sim::<2>::new(3, &[[0u32, 1], [1, 2]], SimParams::default(), 1);
        sim.pinned[1] = true;
        sim.pos[1] = [123.0, -45.0];
        for _ in 0..300 {
            sim.step();
        }
        assert_eq!(sim.pos[1], [123.0, -45.0], "a pinned node does not move");
    }

    #[test]
    fn degenerate_graphs_never_nan() {
        let mut empty = Sim::<2>::new(0, &[], SimParams::default(), 1);
        assert!(!empty.step(), "nothing to simulate");

        let mut single = Sim::<2>::new(1, &[], SimParams::default(), 1);
        for _ in 0..100 {
            single.step();
        }
        assert!(single.pos[0][0].is_finite());

        // Coincident nodes: the jitter path, not a divide-by-zero.
        let mut twin = Sim::<2>::new(2, &[[0u32, 1]], SimParams::default(), 1);
        twin.pos = vec![[0.0, 0.0], [0.0, 0.0]];
        for _ in 0..300 {
            twin.step();
        }
        assert!(twin
            .pos
            .iter()
            .all(|p| p[0].is_finite() && p[1].is_finite()));
    }

    #[test]
    fn degenerate_graphs_never_nan_3d() {
        let mut twin = Sim::<3>::new(2, &[[0u32, 1]], SimParams::default(), 1);
        twin.pos = vec![[0.0; 3], [0.0; 3]];
        for _ in 0..300 {
            twin.step();
        }
        assert!(twin.pos.iter().all(|p| p.iter().all(|v| v.is_finite())));
    }

    #[test]
    fn reheat_wakes_a_settled_sim() {
        let mut sim = Sim::<2>::new(4, &chain(4), SimParams::default(), 1);
        while sim.step() {}
        assert!(sim.is_settled());
        sim.reheat(1.0);
        assert!(!sim.is_settled());
        assert!(sim.step());
    }
}

/// Manual perf check (run with: cargo test --release -p shale-core \
/// bench_sim -- --ignored --nocapture). Not part of the normal suite.
#[test]
#[ignore]
fn bench_sim() {
    use std::time::Instant;
    for n in [500usize, 2000, 5000] {
        // A power-law-ish random graph: preferential attachment, the
        // shape real vaults roughly have.
        let mut edges = Vec::with_capacity(n * 2);
        let mut r = 0x1234_5678_9abc_def0u64;
        for i in 1..n {
            for _ in 0..2 {
                r = r
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let j = (r >> 33) as usize % i;
                edges.push([i as u32, j as u32]);
            }
        }
        let mut sim = Sim::<2>::new(n, &edges, SimParams::default(), 7);
        let t0 = Instant::now();
        let mut ticks = 0;
        while sim.step() && ticks < 400 {
            ticks += 1;
        }
        let dt = t0.elapsed();
        println!(
            "2D n={n:5}  settled in {ticks} ticks  {:>7.2?} total  {:>6.2?}/tick",
            dt,
            dt / ticks as u32
        );
        let mut sim = Sim::<3>::new(n, &edges, SimParams::default(), 7);
        let t0 = Instant::now();
        let mut ticks = 0;
        while sim.step() && ticks < 400 {
            ticks += 1;
        }
        let dt = t0.elapsed();
        println!(
            "3D n={n:5}  settled in {ticks} ticks  {:>7.2?} total  {:>6.2?}/tick",
            dt,
            dt / ticks as u32
        );
    }
}

/// What one tick costs mid-drag (alpha re-heated, not cold): the
/// number that decides whether physics can run at 60 fps.
#[test]
#[ignore]
fn bench_tick() {
    use std::time::Instant;
    for n in [500usize, 2000, 5000] {
        let edges = pa_edges(n);
        for dim in 0..2 {
            if dim == 0 {
                let mut sim = Sim::<2>::new(n, &edges, SimParams::default(), 7);
                while sim.step() {}
                let t0 = Instant::now();
                for _ in 0..100 {
                    sim.reheat(1.0);
                    sim.step();
                }
                let dt = t0.elapsed();
                println!("n={n:5}  {dim}D mid-drag tick {:>7.2?}", dt / 100);
            } else {
                let mut sim = Sim::<3>::new(n, &edges, SimParams::default(), 7);
                while sim.step() {}
                let t0 = Instant::now();
                for _ in 0..100 {
                    sim.reheat(1.0);
                    sim.step();
                }
                let dt = t0.elapsed();
                println!("n={n:5}  {dim}D mid-drag tick {:>7.2?}", dt / 100);
            }
        }
    }
}

/// Preferential-attachment edges (the shape a link graph actually has:
/// hubs and leaves, not a random graph). Bench-only.
#[cfg(test)]
fn pa_edges(n: usize) -> Vec<[u32; 2]> {
    let mut edges = Vec::with_capacity(n * 2);
    let mut r = 0x1234_5678_9abc_def0u64;
    for i in 1..n {
        for _ in 0..2 {
            r = r
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let j = (r >> 33) as usize % i;
            edges.push([i as u32, j as u32]);
        }
    }
    edges
}

/// Per-force breakdown of one hot tick: where the tick time actually
/// goes, measured instead of guessed. The methods are the same ones
/// `step()` calls, timed in isolation on a settled layout.
#[test]
#[ignore]
fn bench_breakdown() {
    use std::time::Instant;
    for n in [2000usize, 5000] {
        let edges = pa_edges(n);
        let mut sim = Sim::<2>::new(n, &edges, SimParams::default(), 7);
        while sim.step() {}
        let reps = 50;

        let t0 = Instant::now();
        for _ in 0..reps {
            sim.apply_links(1.0);
        }
        println!("n={n:5}  links      {:>8.2?}/tick", t0.elapsed() / reps);

        let t0 = Instant::now();
        for _ in 0..reps {
            sim.apply_repulsion(1.0);
        }
        println!("n={n:5}  repulsion  {:>8.2?}/tick", t0.elapsed() / reps);

        let t0 = Instant::now();
        for _ in 0..reps {
            sim.apply_center(1.0);
        }
        println!("n={n:5}  center     {:>8.2?}/tick", t0.elapsed() / reps);

        let t0 = Instant::now();
        for _ in 0..reps {
            sim.apply_collide(1.0);
        }
        println!("n={n:5}  collide    {:>8.2?}/tick", t0.elapsed() / reps);

        let t0 = Instant::now();
        for _ in 0..reps {
            sim.integrate();
        }
        println!("n={n:5}  integrate  {:>8.2?}/tick", t0.elapsed() / reps);
    }
}
