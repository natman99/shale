//! The force simulation the dynamic graph view runs: link springs, pairwise
//! repulsion, and a pull toward the center — the same force vocabulary
//! Obsidian's Forces panel exposes (link force, link distance, repel force,
//! center force), modelled after d3-force because that is what Obsidian
//! itself runs.
//!
//! The sim is pure data: [`Sim::step`] touches no gpui type and no IO, so it
//! runs on a background thread and ships positions to the UI as one
//! `Vec<[f32; 2]>` per tick (PROJECT.md's idling rules: the sim sleeps at
//! `is_settled`, and the UI thread stops drawing when it does).
//!
//! Determinism: initial placement is a seeded phyllotaxis spiral (the same
//! construction d3-force uses), so the same vault and the same seed produce
//! byte-identical layouts — which is what makes the static-mode cache
//! (graph-view-plan.md M3) and the headless tests meaningful.

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
#[derive(Debug, Clone)]
pub struct Sim {
    /// World positions.
    pub pos: Vec<[f32; 2]>,
    /// Velocities; d3's velocity-Verlet keeps them between ticks.
    pub vel: Vec<[f32; 2]>,
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

impl Sim {
    /// Build a sim for `n` nodes with `edges` given as index pairs. Positions
    /// start on a seeded phyllotaxis spiral.
    pub fn new(n: usize, edges: &[[u32; 2]], params: SimParams, seed: u64) -> Self {
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
        const GOLDEN: f32 = 2.399_963_2;
        let mut pos = Vec::with_capacity(n);
        let mut rng = seed | 1; // splitmix64 tolerates any state but never 0
        for i in 0..n {
            let r = 10.0 * (i as f32 + 0.5).sqrt();
            let a = i as f32 * GOLDEN;
            // A touch of jitter breaks the perfect symmetry that would
            // otherwise leave the spiral's forces balanced forever.
            let j = (next_f32(&mut rng) - 0.5) * 2.0;
            pos.push([r * a.cos() + j, r * a.sin() + j]);
        }

        Self {
            pos,
            vel: vec![[0.0; 2]; n],
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
    pub fn positions(&self) -> &[[f32; 2]] {
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
            let [ax, ay] = self.pos[a];
            let [bx, by] = self.pos[b];
            let dx = bx - ax;
            let dy = by - ay;
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let da = self.degree[a].max(1) as f32;
            let db = self.degree[b].max(1) as f32;
            let bias = da / (da + db);
            let k = (len - dist) / len * alpha * strength;
            let (fx, fy) = (dx * k, dy * k);
            if !self.pinned[b] {
                self.vel[b][0] -= fx * bias;
                self.vel[b][1] -= fy * bias;
            }
            if !self.pinned[a] {
                self.vel[a][0] += fx * (1.0 - bias);
                self.vel[a][1] += fy * (1.0 - bias);
            }
        }

        // Many-body repulsion: naive below NAIVE_LIMIT, Barnes-Hut above.
        let repel = self.params.repel;
        if self.pos.len() <= NAIVE_LIMIT {
            for i in 0..self.pos.len() {
                if self.pinned[i] {
                    continue;
                }
                let [px, py] = self.pos[i];
                let mut fx = 0.0f32;
                let mut fy = 0.0f32;
                for j in 0..self.pos.len() {
                    if i == j {
                        continue;
                    }
                    let [qx, qy] = self.pos[j];
                    // d3-force's many-body law, exactly: force on i =
                    // delta * (strength * alpha / d²), delta being (j - i).
                    // The magnitude therefore falls off as 1/d (d3's own
                    // falloff); strength is negative, so the force points
                    // away from j. d² is clamped so near-touching nodes do
                    // not explode.
                    let dx = qx - px;
                    let dy = qy - py;
                    let d2 = dx * dx + dy * dy;
                    if d2 < 0.01 {
                        // Coincident nodes: nudge apart deterministically so
                        // the force never divides by zero.
                        let jx = next_f32(&mut self.rng) - 0.5;
                        let jy = next_f32(&mut self.rng) - 0.5;
                        fx += jx * 10.0;
                        fy += jy * 10.0;
                        continue;
                    }
                    let w = repel * alpha / d2.max(1.0);
                    fx += dx * w;
                    fy += dy * w;
                }
                self.vel[i][0] += fx;
                self.vel[i][1] += fy;
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
            self.vel[i][0] -= self.pos[i][0] * center;
            self.vel[i][1] -= self.pos[i][1] * center;
        }

        // Collide: push overlapping circles apart by their radii.
        if self.params.collide {
            for i in 0..self.pos.len() {
                for j in (i + 1)..self.pos.len() {
                    let [px, py] = self.pos[i];
                    let [qx, qy] = self.pos[j];
                    let dx = qx - px;
                    let dy = qy - py;
                    let want = self.radius[i] + self.radius[j] + 2.0;
                    let d2 = dx * dx + dy * dy;
                    if d2 >= want * want || d2 < 0.0001 {
                        continue;
                    }
                    let d = d2.sqrt();
                    let push = (want - d) / d * 0.5 * alpha;
                    let (ox, oy) = (dx * push, dy * push);
                    if !self.pinned[i] {
                        self.vel[i][0] -= ox;
                        self.vel[i][1] -= oy;
                    }
                    if !self.pinned[j] {
                        self.vel[j][0] += ox;
                        self.vel[j][1] += oy;
                    }
                }
            }
        }

        // Integrate with d3's 0.6 velocity damping, and sanitize: a NaN from
        // any source must not poison the whole layout.
        for i in 0..self.pos.len() {
            if self.pinned[i] {
                self.vel[i] = [0.0; 2];
                continue;
            }
            let v = &mut self.vel[i];
            v[0] *= 0.6;
            v[1] *= 0.6;
            let p = &mut self.pos[i];
            p[0] += v[0];
            p[1] += v[1];
            if !p[0].is_finite() || !p[1].is_finite() {
                *p = [0.0, 0.0];
                *v = [0.0, 0.0];
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
        let mut a = Sim::new(30, &edges, SimParams::default(), 7);
        let mut b = Sim::new(30, &edges, SimParams::default(), 7);
        for _ in 0..200 {
            a.step();
            b.step();
        }
        assert_eq!(a.pos, b.pos, "determinism is what the static cache needs");

        let mut c = Sim::new(30, &edges, SimParams::default(), 8);
        for _ in 0..200 {
            c.step();
        }
        assert_ne!(a.pos, c.pos, "the seed actually seeds");
    }

    #[test]
    fn it_settles() {
        let edges = chain(50);
        let mut sim = Sim::new(50, &edges, SimParams::default(), 1);
        let mut ticks = 0;
        while sim.step() {
            ticks += 1;
            assert!(ticks < 5000, "alpha must decay to the floor");
        }
        assert!(sim.is_settled());
    }

    #[test]
    fn two_components_separate() {
        // Two triangles with no edge between them: repulsion must push the
        // clusters apart beyond a link distance.
        let edges = vec![[0u32, 1], [1, 2], [2, 0], [3, 4], [4, 5], [5, 3]];
        let mut sim = Sim::new(6, &edges, SimParams::default(), 3);
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
        let mut sim = Sim::new(2, &[[0u32, 1]], SimParams::default(), 1);
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
        let mut sim = Sim::new(3, &[[0u32, 1], [1, 2]], SimParams::default(), 1);
        sim.pinned[1] = true;
        sim.pos[1] = [123.0, -45.0];
        for _ in 0..300 {
            sim.step();
        }
        assert_eq!(sim.pos[1], [123.0, -45.0], "a pinned node does not move");
    }

    #[test]
    fn degenerate_graphs_never_nan() {
        let mut empty = Sim::new(0, &[], SimParams::default(), 1);
        assert!(!empty.step(), "nothing to simulate");

        let mut single = Sim::new(1, &[], SimParams::default(), 1);
        for _ in 0..100 {
            single.step();
        }
        assert!(single.pos[0][0].is_finite());

        // Coincident nodes: the jitter path, not a divide-by-zero.
        let mut twin = Sim::new(2, &[[0u32, 1]], SimParams::default(), 1);
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
    fn reheat_wakes_a_settled_sim() {
        let mut sim = Sim::new(4, &chain(4), SimParams::default(), 1);
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
        let mut sim = Sim::new(n, &edges, SimParams::default(), 7);
        let t0 = Instant::now();
        let mut ticks = 0;
        while sim.step() && ticks < 400 {
            ticks += 1;
        }
        let dt = t0.elapsed();
        println!(
            "n={n:5}  settled in {ticks} ticks  {:>7.2?} total  {:>6.2?}/tick",
            dt,
            dt / ticks as u32
        );
    }
}
