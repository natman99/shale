//! Barnes-Hut many-body repulsion: the O(n log n) path the simulation
//! switches to above [`super::sim::NAIVE_LIMIT`] nodes.
//!
//! Same physics as the naive loop in [`super::sim`] — d3's many-body law,
//! force on i = `delta * (strength / d²)` (strength negative, d² clamped) —
//! approximated the classic way: a spatial tree over the positions where a
//! cell far enough away is replaced by its center of mass (opened only
//! while `width / distance < THETA`). The validation test measures the
//! approximation against the naive loop.
//!
//! Dimension-generic like the sim: `D=2` is a quadtree (4 children),
//! `D=3` an octree (8) — the same code monomorphized, quadrant bits being
//! one per axis.
//!
//! Performance notes (measured, see PROGRESS.md): the algorithm itself is
//! right — ~230 cell visits per point at n=5000 vs 5000 naive pairs — but
//! a first recursive implementation cost ~23 ns per visit and barely beat
//! the naive loop. This version is built for the constant factor: cells are
//! flat records with the square center stored (no `Option`, no
//! `child_center` recomputation during traversal), and the force traversal
//! is an explicit stack, not recursion.

use super::sim::NAIVE_LIMIT;

/// The classic opening criterion: a cell is approximated when its width over
/// the distance to it is under this. d3-force ships 0.9; we hold the layout
/// to 5% per node against the exact loop. Measured worst-case at 0.8
/// (probe_theta_margin[_clustered]): 0.6%/2.8-3.1% in 2D/3D, on both
/// uniform scatter and settled force-directed (clustered) layouts. 0.9
/// would save ~10% more visits but leaves 3D at 3.6% — thin against the
/// 5% bound. The accuracy tests pin this down.
const THETA: f32 = 0.8;

/// Absent child marker.
const NONE: u32 = u32::MAX;

/// A cell of the tree arena. A leaf holds one point (or a bundle of
/// coincident points) with `children` all `NONE` — its `com` is the point.
/// An internal cell holds the center of mass of its subtree. The square's
/// center is stored at creation so the force traversal never recomputes it.
struct Cell<const D: usize> {
    /// Center of mass of every point in this subtree (== the point for a
    /// leaf).
    com: [f32; D],
    /// Squared width of this cell's square — the opening test compares it
    /// against THETA² * d², so it is stored pre-squared. (The square's
    /// *center* is deliberately not stored: only insertion needs it, and
    /// insertion computes it on the way down. Keeping it out of Cell
    /// shrinks the struct the hot traversal streams through.)
    width2: f32,
    /// How many points this subtree holds (>= 1 once built).
    mass: u32,
    /// Bit q set iff child slot q is present. The hot traversal tests
    /// this instead of scanning the 8 slots for NONE (leaf == mask 0),
    /// and walks only the set bits when opening a cell.
    child_mask: u8,
    /// Quadrant index -> child cell, `NONE` for absent. Quadrant bits:
    /// bit k = the point is on the + side of axis k. Fixed 8 slots (the
    /// octree count): `1 << D` is not a stable array length, so the
    /// quadtree simply leaves the top four absent forever.
    children: [u32; 8],
}

struct Tree<const D: usize> {
    cells: Vec<Cell<D>>,
    root: u32,
}

/// Quadrant of `p` in a cell centered at `c`: bit k = + side of axis k.
fn quadrant<const D: usize>(c: [f32; D], p: [f32; D]) -> usize {
    let mut q = 0;
    for k in 0..D {
        q |= ((p[k] >= c[k]) as usize) << k;
    }
    q
}

/// Center of quadrant `q` of the cell centered at `c` with half-width `half`.
fn child_center<const D: usize>(c: [f32; D], half: f32, q: usize) -> [f32; D] {
    let h = half * 0.5;
    let mut out = [0.0f32; D];
    for k in 0..D {
        out[k] = c[k] + if q & (1 << k) != 0 { h } else { -h };
    }
    out
}

impl<const D: usize> Tree<D> {
    fn new(pos: &[[f32; D]]) -> Self {
        let mut min = [f32::MAX; D];
        let mut max = [f32::MIN; D];
        for p in pos {
            for k in 0..D {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        let mut root_center = [0.0f32; D];
        let mut extent = 0.0f32;
        for k in 0..D {
            root_center[k] = (min[k] + max[k]) * 0.5;
            extent = extent.max(max[k] - min[k]);
        }
        let root_half = extent * 0.5 + 1.0;

        let mut tree = Tree {
            cells: Vec::with_capacity(pos.len() * 2),
            root: 0,
        };
        tree.cells.push(Cell {
            com: root_center,
            width2: (2.0 * root_half) * (2.0 * root_half),
            mass: 0,
            child_mask: 0,
            children: [NONE; 8],
        });
        for &p in pos {
            tree.insert(0, p, root_center, root_half);
        }
        tree
    }

    /// Insert `p` into the subtree at `cell`. Accumulates center of mass on
    /// the way down (running average — insertion is O(n log n) total and
    /// never the hot path, so the divisions are fine). The cell's square
    /// (`center`, `half`) is threaded through the recursion rather than
    /// stored in the cell: only insertion ever needs it.
    fn insert(&mut self, cell: u32, p: [f32; D], center: [f32; D], half: f32) {
        let ci = cell as usize;
        let (com, mass) = (self.cells[ci].com, self.cells[ci].mass);
        for k in 0..D {
            self.cells[ci].com[k] = (com[k] * mass as f32 + p[k]) / (mass + 1) as f32;
        }
        self.cells[ci].mass = mass + 1;

        let children = self.cells[ci].children;

        if children != [NONE; 8] {
            let q = quadrant(center, p);
            if children[q] != NONE {
                let cc = child_center(center, half, q);
                self.insert(children[q], p, cc, half * 0.5);
                return;
            }
            let idx = self.cells.len() as u32;
            self.cells.push(Cell {
                com: p,
                // child square: half -> half/2, so width2 -> half^2
                width2: half * half,
                mass: 1,
                child_mask: 0,
                children: [NONE; 8],
            });
            let mut kids = children;
            kids[q] = idx;
            self.cells[ci].children = kids;
            self.cells[ci].child_mask |= 1 << q;
            return;
        }

        // Leaf. mass was 0 (empty root) or >= 1.
        if mass == 0 || com == p {
            // First point in the cell, or coincident with the leaf's point
            // (or bundle): stay in this leaf; the com update above already
            // absorbed p (a no-op when p == com). The sim's jitter path
            // handles coincident nodes.
            return;
        }

        // Split: the old point — or a coincident bundle, all at `com`,
        // mass > 1 — moves into a child leaf carrying its mass, so this
        // cell's count stays correct; recursing back into this cell would
        // double-count it.
        let old = com;
        let q_old = quadrant(center, old);
        let child = self.cells.len() as u32;
        let cc_old = child_center(center, half, q_old);
        self.cells.push(Cell {
            com: old,
            width2: half * half,
            mass,
            child_mask: 0,
            children: [NONE; 8],
        });
        self.cells[ci].children = {
            let mut kids = [NONE; 8];
            kids[q_old] = child;
            kids
        };
        self.cells[ci].child_mask = 1 << q_old;
        let q_new = quadrant(center, p);
        if q_old == q_new {
            self.insert(child, p, cc_old, half * 0.5);
        } else {
            let idx = self.cells.len() as u32;
            self.cells.push(Cell {
                com: p,
                width2: half * half,
                mass: 1,
                child_mask: 0,
                children: [NONE; 8],
            });
            self.cells[ci].children[q_new] = idx;
            self.cells[ci].child_mask |= 1 << q_new;
        }
    }

    /// Repulsion force on `p` from every point in the tree except `p`
    /// itself (its own leaf is under the distance floor and contributes
    /// nothing). `strength` is negative and carries alpha. Iterative with
    /// an explicit stack — the hot loop, ~230 visits per point at n=5000.
    fn force_on(&self, p: [f32; D], strength: f32, stack: &mut Vec<u32>) -> [f32; D] {
        stack.clear();
        stack.push(self.root);
        let mut f = [0.0f32; D];
        let t2 = THETA * THETA;
        while let Some(ci) = stack.pop() {
            let c = &self.cells[ci as usize];
            let mut delta = [0.0f32; D];
            let mut d2 = 0.0f32;
            for k in 0..D {
                delta[k] = c.com[k] - p[k];
                d2 += delta[k] * delta[k];
            }
            let mask = c.child_mask;
            if mask != 0 && c.width2 > t2 * d2 {
                // Near internal cell: open it — walk only the present children.
                let kids = c.children;
                let mut m = mask;
                while m != 0 {
                    let q = m.trailing_zeros() as usize;
                    m &= m - 1;
                    stack.push(kids[q]);
                }
                continue;
            }
            // Leaf, or a far internal cell: treat as one mass at `com`.
            // The self-force of `p` against its own leaf is zero because
            // the distance is under the floor.
            if d2 < 0.01 {
                continue;
            }
            let w = strength * c.mass as f32 / d2.max(1.0);
            for k in 0..D {
                f[k] += delta[k] * w;
            }
        }
        f
    }
}

/// Apply Barnes-Hut repulsion to `vel` for every unpinned point. `strength`
/// is negative (repulsion) and already multiplied by alpha.
pub fn apply_repulsion<const D: usize>(
    pos: &[[f32; D]],
    vel: &mut [[f32; D]],
    pinned: &[bool],
    strength: f32,
) {
    debug_assert!(
        pos.len() > NAIVE_LIMIT,
        "apply_repulsion is the large-graph path; the sim picks the naive loop below the limit"
    );
    if pos.is_empty() {
        return;
    }
    let tree = Tree::<D>::new(pos);
    let mut stack: Vec<u32> = Vec::with_capacity(256);
    for i in 0..pos.len() {
        if pinned[i] {
            continue;
        }
        let f = tree.force_on(pos[i], strength, &mut stack);
        for k in 0..D {
            vel[i][k] += f[k];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The naive loop's force on every point, for comparison: the same law
    /// as the sim — delta * (strength / d²), d² clamped.
    fn naive_forces<const D: usize>(pos: &[[f32; D]], strength: f32) -> Vec<[f32; D]> {
        let mut out = vec![[0.0; D]; pos.len()];
        for i in 0..pos.len() {
            for j in 0..pos.len() {
                if i == j {
                    continue;
                }
                let mut delta = [0.0f32; D];
                let mut d2 = 0.0f32;
                for k in 0..D {
                    delta[k] = pos[j][k] - pos[i][k];
                    d2 += delta[k] * delta[k];
                }
                if d2 < 0.01 {
                    continue;
                }
                let w = strength / d2.max(1.0);
                for k in 0..D {
                    out[i][k] += delta[k] * w;
                }
            }
        }
        out
    }

    /// A deterministic scatter of `n` points in a 1000^D box.
    fn scatter<const D: usize>(n: usize, seed: u64) -> Vec<[f32; D]> {
        let mut state = seed | 1;
        (0..n)
            .map(|_| {
                let mut p = [0.0f32; D];
                for v in p.iter_mut() {
                    state = state.wrapping_add(0x9E3779B97F4A7C15);
                    let mut z = state;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
                    z ^= z >> 31;
                    *v = (z >> 40) as f32 / (1u64 << 24) as f32 * 1000.0;
                }
                p
            })
            .collect()
    }

    /// Worst per-point approximation error, as a fraction of the total
    /// absolute force that point feels. (Relative-to-*net* error is
    /// meaningless: an interior point's net force is near zero through
    /// cancellation, and the ratio then explodes on a value nobody
    /// perceives. The meaningful statement is that the approximation error
    /// stays a small fraction of the force the point actually feels.)
    fn worst_error<const D: usize>(pos: &[[f32; D]], strength: f32) -> f32 {
        let exact = naive_forces(pos, strength);
        let total_abs: Vec<f32> = pos
            .iter()
            .map(|p| {
                pos.iter()
                    .filter(|q| *q != p)
                    .map(|q| {
                        let mut d2 = 0.0f32;
                        for k in 0..D {
                            d2 += (q[k] - p[k]).powi(2);
                        }
                        let d2 = d2.max(0.01);
                        strength.abs() / d2.max(1.0) * d2.sqrt()
                    })
                    .sum()
            })
            .collect();

        let mut vel = vec![[0.0; D]; pos.len()];
        let pinned = vec![false; pos.len()];
        apply_repulsion(pos, &mut vel, &pinned, strength);

        let mut worst = 0.0f32;
        for i in 0..pos.len() {
            let e = exact[i];
            let a = vel[i];
            let mut err2 = 0.0f32;
            for k in 0..D {
                err2 += (a[k] - e[k]).powi(2);
            }
            worst = worst.max(err2.sqrt() / total_abs[i]);
        }
        worst
    }

    /// What the approximation error actually is at the current THETA (the
    /// accuracy tests only assert < 5%; this prints the margin).
    #[test]
    #[ignore]
    fn probe_theta_margin() {
        let pos = scatter::<2>(4000, 42);
        println!(
            "THETA={THETA}  2D worst error: {:.4}",
            worst_error(&pos, -90.0)
        );
        let pos = scatter::<3>(1500, 7);
        println!(
            "THETA={THETA}  3D worst error: {:.4}",
            worst_error(&pos, -90.0)
        );
    }

    /// The scatter-based margin above is uniform; a real vault graph is
    /// clustered (hubs + leaves), the harder case for the multipole
    /// approximation. This measures the margin on the layout the sim
    /// actually produces. (n modest: the naive comparison is O(n²).)
    #[test]
    #[ignore]
    fn probe_theta_margin_clustered() {
        use crate::graph::quadtree::bench::settled;
        let pos = settled::<2>(1200);
        println!(
            "THETA={THETA}  clustered 2D worst: {:.4}",
            worst_error(&pos, -90.0)
        );
        let pos = settled::<3>(1200);
        println!(
            "THETA={THETA}  clustered 3D worst: {:.4}",
            worst_error(&pos, -90.0)
        );
    }

    #[test]
    fn barnes_hut_matches_naive_within_five_percent() {
        // 4000 points: past the naive limit, the tree's reason to exist.
        let pos = scatter::<2>(4000, 42);
        let worst = worst_error(&pos, -90.0);
        assert!(
            worst < 0.05,
            "Barnes-Hut deviated {worst:.3} from the naive loop (> 0.05)"
        );
    }

    #[test]
    fn octree_matches_naive_within_five_percent_3d() {
        // The 3D path: same law, one more axis. Smaller n because the
        // naive comparison loop is O(n²) in a debug test.
        let pos = scatter::<3>(1500, 42);
        let worst = worst_error(&pos, -90.0);
        assert!(
            worst < 0.05,
            "octree deviated {worst:.3} from the naive loop (> 0.05)"
        );
    }

    #[test]
    fn pinned_points_are_skipped_and_coincident_points_survive() {
        let mut pos = scatter::<2>(3000, 7);
        pos[0] = [500.0, 500.0];
        pos[1] = [500.0, 500.0]; // coincident pair: no divide-by-zero
        let mut vel = vec![[0.0; 2]; pos.len()];
        let mut pinned = vec![false; pos.len()];
        pinned[2] = true;
        apply_repulsion(&pos, &mut vel, &pinned, -90.0);
        assert_eq!(vel[2], [0.0; 2], "a pinned point gets no force");
        assert!(
            vel.iter().all(|f| f[0].is_finite() && f[1].is_finite()),
            "no NaN from the coincident pair"
        );
    }

    #[test]
    fn coincident_points_survive_3d() {
        let mut pos = scatter::<3>(3000, 7);
        pos[0] = [500.0; 3];
        pos[1] = [500.0; 3];
        let mut vel = vec![[0.0; 3]; pos.len()];
        let pinned = vec![false; pos.len()];
        apply_repulsion(&pos, &mut vel, &pinned, -90.0);
        assert!(vel.iter().all(|f| f.iter().all(|v| v.is_finite())));
    }

    #[test]
    fn insert_splits_and_counts_everything() {
        let pos = vec![
            [0.0, 0.0],
            [10.0, 10.0],
            [-10.0, -10.0],
            [10.0, -10.0],
            [-10.0, 10.0],
            [0.1, 0.1], // forces a deep split against [0,0]
        ];
        let tree = Tree::<2>::new(&pos);
        assert_eq!(tree.cells[tree.root as usize].mass, pos.len() as u32);
        // The root's center of mass must be the true mean.
        let mean = [
            pos.iter().map(|p| p[0]).sum::<f32>() / pos.len() as f32,
            pos.iter().map(|p| p[1]).sum::<f32>() / pos.len() as f32,
        ];
        let com = tree.cells[tree.root as usize].com;
        assert!(
            (com[0] - mean[0]).abs() < 1e-3 && (com[1] - mean[1]).abs() < 1e-3,
            "center of mass {com:?} vs mean {mean:?}"
        );
    }

    #[test]
    fn every_point_lands_in_exactly_one_leaf() {
        // Walk the tree; the multiset of leaf points must equal the input
        // (coincident points bundled together, mass counting them).
        let pos = scatter::<2>(500, 11);
        let tree = Tree::<2>::new(&pos);
        let mut counted = 0u32;
        let mut stack = vec![tree.root];
        while let Some(ci) = stack.pop() {
            let c = &tree.cells[ci as usize];
            if c.children == [NONE; 8] {
                counted += c.mass;
            } else {
                for &k in &c.children {
                    if k != NONE {
                        stack.push(k);
                    }
                }
            }
        }
        assert_eq!(counted as usize, pos.len());

        let pos = scatter::<3>(500, 11);
        let tree = Tree::<3>::new(&pos);
        let mut counted = 0u32;
        let mut stack = vec![tree.root];
        while let Some(ci) = stack.pop() {
            let c = &tree.cells[ci as usize];
            if c.children == [NONE; 8] {
                counted += c.mass;
            } else {
                for &k in &c.children {
                    if k != NONE {
                        stack.push(k);
                    }
                }
            }
        }
        assert_eq!(counted as usize, pos.len());
    }
}

#[cfg(test)]
mod bench {
    use super::*;

    /// Shared fixture: a preferential-attachment graph settled to a real
    /// (clustered) layout — the shape real vaults roughly have, and the
    /// input the probes below use.
    pub(super) fn settled<const D: usize>(n: usize) -> Vec<[f32; D]> {
        use crate::graph::sim::{Sim, SimParams};
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
        let mut sim = Sim::<D>::new(n, &edges, SimParams::default(), 7);
        while sim.step() {}
        sim.positions().to_vec()
    }

    /// Split of apply_repulsion's cost: tree build vs force traversal.
    /// The answer decides what to optimize next.
    #[test]
    #[ignore]
    fn probe_build_vs_traverse() {
        use std::time::Instant;
        for n in [2000usize, 5000] {
            let pos2 = settled::<2>(n);
            let t0 = Instant::now();
            for _ in 0..20 {
                let t = Tree::<2>::new(&pos2);
                std::hint::black_box(&t);
            }
            println!("n={n:5}  2D build     {:>8.2?}", t0.elapsed() / 20);
            let tree = Tree::<2>::new(&pos2);
            let mut vel = vec![[0.0; 2]; n];
            let pinned = vec![false; n];
            let t0 = Instant::now();
            for _ in 0..20 {
                let mut stack = Vec::with_capacity(256);
                for (i, &p) in pos2.iter().enumerate() {
                    let f = tree.force_on(p, -90.0, &mut stack);
                    vel[i] = f;
                }
            }
            println!("n={n:5}  2D traverse  {:>8.2?}", t0.elapsed() / 20);

            let pos3 = settled::<3>(n);
            let t0 = Instant::now();
            for _ in 0..20 {
                let t = Tree::<3>::new(&pos3);
                std::hint::black_box(&t);
            }
            println!("n={n:5}  3D build     {:>8.2?}", t0.elapsed() / 20);
            let tree = Tree::<3>::new(&pos3);
            let mut vel = vec![[0.0; 3]; n];
            let t0 = Instant::now();
            for _ in 0..20 {
                let mut stack = Vec::with_capacity(256);
                for (i, &p) in pos3.iter().enumerate() {
                    let f = tree.force_on(p, -90.0, &mut stack);
                    vel[i] = f;
                }
            }
            println!("n={n:5}  3D traverse  {:>8.2?}", t0.elapsed() / 20);
        }
    }

    /// Cell count and per-point visit count on a settled layout.
    #[test]
    #[ignore]
    fn probe_visits() {
        let n = 5000usize;
        let pos = settled::<2>(n);
        let tree = Tree::<2>::new(&pos);
        println!("2D cells: {} (points: {n})", tree.cells.len());
        let mut total = 0usize;
        let mut stack: Vec<u32> = Vec::with_capacity(256);
        for p in &pos {
            stack.clear();
            stack.push(tree.root);
            while let Some(ci) = stack.pop() {
                total += 1;
                let c = &tree.cells[ci as usize];
                let d2 = (c.com[0] - p[0]).powi(2) + (c.com[1] - p[1]).powi(2);
                if c.children != [NONE; 8] {
                    if c.width2 > THETA * THETA * d2 {
                        for &k in &c.children {
                            if k != NONE {
                                stack.push(k);
                            }
                        }
                    }
                }
            }
        }
        println!(
            "2D visits: {total} total, {:#.1} per point",
            total as f64 / n as f64
        );
    }

    /// Per-tick repulsion cost alone (tree build + force), BH vs naive,
    /// 2D and 3D.
    #[test]
    #[ignore]
    fn probe_repulsion_cost() {
        use std::time::Instant;
        for n in [500usize, 2000, 5000] {
            let pos = settled::<2>(n);
            let mut vel = vec![[0.0; 2]; n];
            let pinned = vec![false; n];
            let t0 = Instant::now();
            for _ in 0..20 {
                apply_repulsion(&pos, &mut vel, &pinned, -90.0);
            }
            let dt = t0.elapsed();
            println!("BH 2D   n={n:5}  {:>7.2?}/tick", dt / 20);
            let t0 = Instant::now();
            for _ in 0..20 {
                let _ = naive_pub(&pos, -90.0);
            }
            let dt = t0.elapsed();
            println!("naive 2D n={n:5}  {:>7.2?}/tick", dt / 20);
        }
        for n in [2000usize, 5000] {
            let pos = settled::<3>(n);
            let mut vel = vec![[0.0; 3]; n];
            let pinned = vec![false; n];
            let t0 = Instant::now();
            for _ in 0..20 {
                apply_repulsion(&pos, &mut vel, &pinned, -90.0);
            }
            let dt = t0.elapsed();
            println!("BH 3D   n={n:5}  {:>7.2?}/tick", dt / 20);
        }
    }

    fn naive_pub<const D: usize>(pos: &[[f32; D]], strength: f32) -> Vec<[f32; D]> {
        let mut out = vec![[0.0; D]; pos.len()];
        for i in 0..pos.len() {
            for j in 0..pos.len() {
                if i == j {
                    continue;
                }
                let mut delta = [0.0f32; D];
                let mut d2 = 0.0f32;
                for k in 0..D {
                    delta[k] = pos[j][k] - pos[i][k];
                    d2 += delta[k] * delta[k];
                }
                if d2 < 0.01 {
                    continue;
                }
                let w = strength / d2.max(1.0);
                for k in 0..D {
                    out[i][k] += delta[k] * w;
                }
            }
        }
        out
    }

    /// Crossover probe: where does BH start beating naive?
    #[test]
    #[ignore]
    fn probe_crossover() {
        use std::time::Instant;
        for n in [50usize, 100, 200, 300, 400, 600, 800, 1200] {
            let pos = settled::<2>(n);
            let mut vel = vec![[0.0; 2]; n];
            let pinned = vec![false; n];
            let t0 = Instant::now();
            for _ in 0..200 {
                apply_repulsion(&pos, &mut vel, &pinned, -90.0);
            }
            let bh = t0.elapsed();
            let t0 = Instant::now();
            for _ in 0..200 {
                let _ = naive_pub(&pos, -90.0);
            }
            let nv = t0.elapsed();
            println!(
                "n={n:5}  BH {:>8.3?}/tick   naive {:>8.3?}/tick",
                bh / 200,
                nv / 200
            );
        }
    }
}
