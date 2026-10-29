//! Barnes-Hut many-body repulsion: the O(n log n) path the simulation
//! switches to above [`super::sim::NAIVE_LIMIT`] nodes.
//!
//! Same physics as the naive loop in [`super::sim`] — d3's many-body law,
//! force on i = `delta * (strength / d²)` (strength negative, d² clamped) —
//! approximated the classic way: a quadtree over the positions where a cell
//! far enough away is replaced by its center of mass (opened only while
//! `width / distance < THETA`). The validation test measures the
//! approximation against the naive loop.

use super::sim::NAIVE_LIMIT;

/// The classic opening criterion: a cell is approximated when its width over
/// the distance to it is under this. d3-force uses 0.9 (prettier is close
/// enough); we hold the layout to 5% per node against the exact loop, which
/// for the 1/d law needs a tighter 0.5 — still comfortably O(n log n).
const THETA: f32 = 0.5;

/// A cell of the quadtree arena. A leaf holds one point (or a bundle of
/// coincident points); an internal cell holds the center of mass of its
/// subtree and its children, indexed by quadrant, `usize::MAX` for absent.
struct Cell {
    /// Center of mass of every point in this subtree.
    com: [f32; 2],
    /// How many points this subtree holds.
    mass: usize,
    /// Half-width of this cell's square.
    half: f32,
    /// The leaf's point (only meaningful while `children` is `None`).
    point: [f32; 2],
    /// Quadrant index -> child cell, or `usize::MAX`.
    children: Option<[usize; 4]>,
}

struct Tree {
    cells: Vec<Cell>,
    root: usize,
    /// Center of the root square; every cell's center is derived from the
    /// path taken to reach it, so descent carries it as an argument.
    root_center: [f32; 2],
}

/// Quadrant of `p` in a cell centered at `c`: bit 0 = +x, bit 1 = +y.
fn quadrant(c: [f32; 2], p: [f32; 2]) -> usize {
    ((p[0] >= c[0]) as usize) + ((p[1] >= c[1]) as usize) * 2
}

/// Center of quadrant `q` of the cell centered at `c` with half-width `half`.
fn child_center(c: [f32; 2], half: f32, q: usize) -> [f32; 2] {
    let h = half * 0.5;
    [
        c[0] + if q & 1 == 1 { h } else { -h },
        c[1] + if q & 2 == 2 { h } else { -h },
    ]
}

impl Tree {
    fn new(pos: &[[f32; 2]]) -> Self {
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for p in pos {
            min[0] = min[0].min(p[0]);
            min[1] = min[1].min(p[1]);
            max[0] = max[0].max(p[0]);
            max[1] = max[1].max(p[1]);
        }
        let root_center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
        let root_half = ((max[0] - min[0]).max(max[1] - min[1])) * 0.5 + 1.0;

        let mut tree = Tree {
            cells: Vec::with_capacity(pos.len() * 2),
            root: 0,
            root_center,
        };
        tree.cells.push(Cell {
            com: root_center,
            mass: 0,
            half: root_half,
            point: root_center,
            children: None,
        });
        for &p in pos {
            let root = tree.root;
            tree.insert(root, root_center, p);
        }
        tree
    }

    /// Insert `p` into the subtree at `cell`, whose square is centered at
    /// `center`. Accumulates center-of-mass on the way down.
    fn insert(&mut self, cell: usize, center: [f32; 2], p: [f32; 2]) {
        let (com, mass) = {
            let c = &self.cells[cell];
            (c.com, c.mass)
        };
        self.cells[cell].com = [
            (com[0] * mass as f32 + p[0]) / (mass + 1) as f32,
            (com[1] * mass as f32 + p[1]) / (mass + 1) as f32,
        ];
        self.cells[cell].mass += 1;

        let (children, half, point) = {
            let c = &self.cells[cell];
            (c.children, c.half, c.point)
        };

        if let Some(children) = children {
            let q = quadrant(center, p);
            if children[q] != usize::MAX {
                let cc = child_center(center, half, q);
                self.insert(children[q], cc, p);
                return;
            }
            let idx = self.cells.len();
            self.cells.push(Cell {
                com: p,
                mass: 1,
                half: half * 0.5,
                point: p,
                children: None,
            });
            let mut kids = children;
            kids[q] = idx;
            self.cells[cell].children = Some(kids);
            return;
        }

        if mass == 0 {
            // The very first point in this cell: it *is* the leaf.
            self.cells[cell].point = p;
            return;
        }

        if mass >= 1 {
            // A leaf holding one point (or a bundle of coincident points,
            // all at `point`): split until the newcomer lands in a different
            // quadrant. The old bundle moves into a child leaf carrying its
            // mass, so this cell's count stays correct — recursing back into
            // this cell would double-count it.
            if point == p {
                // Coincident with the bundle: stay bundled in one leaf; the
                // sim's jitter path handles coincident nodes.
                return;
            }
            let q_old = quadrant(center, point);
            let q_new = quadrant(center, p);
            let child = self.cells.len();
            self.cells.push(Cell {
                com: point,
                mass,
                half: half * 0.5,
                point,
                children: None,
            });
            self.cells[cell].children = Some({
                let mut kids = [usize::MAX; 4];
                kids[q_old] = child;
                kids
            });
            if q_old == q_new {
                let cc = child_center(center, half, q_new);
                self.insert(child, cc, p);
            } else {
                let idx_new = self.cells.len();
                self.cells.push(Cell {
                    com: p,
                    mass: 1,
                    half: half * 0.5,
                    point: p,
                    children: None,
                });
                let kids = self.cells[cell].children.unwrap();
                let mut kids = kids;
                kids[q_new] = idx_new;
                self.cells[cell].children = Some(kids);
            }
            return;
        }

        // mass > 1 with no children only happens for coincident bundles,
        // which stay bundled: nothing else reaches here.
    }

    /// Repulsion force on `p` from every point in the subtree at `cell`
    /// except `p` itself. `strength` is negative and carries alpha.
    fn force_on(&self, cell: usize, center: [f32; 2], p: [f32; 2], strength: f32) -> [f32; 2] {
        let c = &self.cells[cell];
        if c.mass == 0 {
            return [0.0; 2];
        }
        let dx = c.com[0] - p[0];
        let dy = c.com[1] - p[1];
        let d2 = dx * dx + dy * dy;

        let Some(children) = c.children else {
            // Leaf: the exact point — or a bundle of coincident points, all
            // sitting at `point`, whose combined pull scales with mass. The
            // self-force of `p` against its own bundle is zero because the
            // distance is under the floor (the sim's jitter handles
            // coincidences).
            if d2 < 0.01 {
                return [0.0; 2];
            }
            let w = strength * c.mass as f32 / d2.max(1.0);
            return [dx * w, dy * w];
        };

        // Internal cell: approximate by center of mass unless it is near.
        let width = 2.0 * c.half;
        if width * width > THETA * THETA * d2 {
            let mut out = [0.0; 2];
            for q in 0..4 {
                let child = children[q];
                if child == usize::MAX {
                    continue;
                }
                let cc = child_center(center, c.half, q);
                let f = self.force_on(child, cc, p, strength);
                out[0] += f[0];
                out[1] += f[1];
            }
            out
        } else {
            if d2 < 0.01 {
                return [0.0; 2];
            }
            let w = strength * c.mass as f32 / d2.max(1.0);
            [dx * w, dy * w]
        }
    }
}

/// Apply Barnes-Hut repulsion to `vel` for every unpinned point. `strength`
/// is negative (repulsion) and already multiplied by alpha.
pub fn apply_repulsion(pos: &[[f32; 2]], vel: &mut [[f32; 2]], pinned: &[bool], strength: f32) {
    debug_assert!(
        pos.len() > NAIVE_LIMIT,
        "apply_repulsion is the large-graph path; the sim picks the naive loop below the limit"
    );
    if pos.is_empty() {
        return;
    }
    let tree = Tree::new(pos);
    let root = tree.root;
    let root_center = tree.root_center;
    for i in 0..pos.len() {
        if pinned[i] {
            continue;
        }
        let f = tree.force_on(root, root_center, pos[i], strength);
        vel[i][0] += f[0];
        vel[i][1] += f[1];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The naive loop's force on every point, for comparison: the same law
    /// as the sim — delta * (strength / d²), d² clamped.
    fn naive_forces(pos: &[[f32; 2]], strength: f32) -> Vec<[f32; 2]> {
        let mut out = vec![[0.0; 2]; pos.len()];
        for i in 0..pos.len() {
            let [px, py] = pos[i];
            let mut fx = 0.0f32;
            let mut fy = 0.0f32;
            for j in 0..pos.len() {
                if i == j {
                    continue;
                }
                let [qx, qy] = pos[j];
                let dx = qx - px;
                let dy = qy - py;
                let d2 = dx * dx + dy * dy;
                if d2 < 0.01 {
                    continue;
                }
                let w = strength / d2.max(1.0);
                fx += dx * w;
                fy += dy * w;
            }
            out[i] = [fx, fy];
        }
        out
    }

    /// A deterministic scatter of `n` points in a 1000x1000 box.
    fn scatter(n: usize, seed: u64) -> Vec<[f32; 2]> {
        let mut state = seed | 1;
        (0..n)
            .map(|_| {
                let mut x = 0.0f32;
                let mut y = 0.0f32;
                for _ in 0..2 {
                    state = state.wrapping_add(0x9E3779B97F4A7C15);
                    let mut z = state;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
                    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
                    z ^= z >> 31;
                    let v = (z >> 40) as f32 / (1u64 << 24) as f32;
                    if x == 0.0 {
                        x = v * 1000.0;
                    } else {
                        y = v * 1000.0;
                    }
                }
                [x, y]
            })
            .collect()
    }

    #[test]
    fn barnes_hut_matches_naive_within_five_percent() {
        // 4000 points: past the naive limit, the quadtree's reason to exist.
        let pos = scatter(4000, 42);
        let strength = -90.0;
        let exact = naive_forces(&pos, strength);
        // The denominator is the sum of the *absolute* pair forces, not the
        // net magnitude: an interior point's net force is near zero through
        // cancellation, and relative-to-net error there is meaningless. The
        // meaningful statement is that the approximation error stays a small
        // fraction of the total force the point actually feels.
        let total_abs: Vec<f32> = pos
            .iter()
            .map(|p| {
                pos.iter()
                    .filter(|q| *q != p)
                    .map(|q| {
                        let dx = q[0] - p[0];
                        let dy = q[1] - p[1];
                        let d2 = (dx * dx + dy * dy).max(0.01);
                        strength.abs() / d2.max(1.0) * d2.sqrt()
                    })
                    .sum()
            })
            .collect();

        let mut vel = vec![[0.0; 2]; pos.len()];
        let pinned = vec![false; pos.len()];
        apply_repulsion(&pos, &mut vel, &pinned, strength);

        let mut worst = 0.0f32;
        for i in 0..pos.len() {
            let e = exact[i];
            let a = vel[i];
            let err = ((a[0] - e[0]).powi(2) + (a[1] - e[1]).powi(2)).sqrt() / total_abs[i];
            worst = worst.max(err);
        }
        assert!(
            worst < 0.05,
            "Barnes-Hut deviated {worst:.3} from the naive loop (> 0.05)"
        );
    }

    #[test]
    fn pinned_points_are_skipped_and_coincident_points_survive() {
        let mut pos = scatter(3000, 7);
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
    fn insert_splits_and_counts_everything() {
        let pos = vec![
            [0.0, 0.0],
            [10.0, 10.0],
            [-10.0, -10.0],
            [10.0, -10.0],
            [-10.0, 10.0],
            [0.1, 0.1], // forces a deep split against [0,0]
        ];
        let tree = Tree::new(&pos);
        assert_eq!(tree.cells[tree.root].mass, pos.len());
        // The root's center of mass must be the true mean.
        let mean = [
            pos.iter().map(|p| p[0]).sum::<f32>() / pos.len() as f32,
            pos.iter().map(|p| p[1]).sum::<f32>() / pos.len() as f32,
        ];
        let com = tree.cells[tree.root].com;
        assert!(
            (com[0] - mean[0]).abs() < 1e-3 && (com[1] - mean[1]).abs() < 1e-3,
            "center of mass {com:?} vs mean {mean:?}"
        );
    }
}
