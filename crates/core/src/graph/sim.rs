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
    /// Per-node degree, for d3's `1 / min(deg)` link strength.
    degree: Vec<u32>,
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
            degree,
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

        // Link springs: pull both ends toward the rest length, weighted by
        // the *other* end's degree share so a hub moves less than its leaf.
        let dist = self.params.link_distance;
        let strength = self.params.link_strength;
        for &[a, b] in &self.edges {
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
            let da = self.degree[a].max(1) as f32;
            let db = self.degree[b].max(1) as f32;
            let bias = da / (da + db);
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

        // Many-body repulsion: naive below NAIVE_LIMIT, Barnes-Hut above.
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

        // Centering: a gentle pull toward the origin keeps the graph on
        // screen (d3's forceX/forceY at `center` strength).
        let center = self.params.center * alpha;
        for i in 0..self.pos.len() {
            if self.pinned[i] {
                continue;
            }
            for d in 0..D {
                self.vel[i][d] -= self.pos[i][d] * center;
            }
        }

        // Collide: push overlapping spheres apart by their radii.
        if self.params.collide {
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

        // Integrate with d3's 0.6 velocity damping, and sanitize: a NaN from
        // any source must not poison the whole layout.
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

        self.alpha += (0.0 - self.alpha) * self.params.alpha_decay;
        true
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
