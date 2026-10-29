//! Projected pseudo-3D for the graph (graph-view-plan.md M2): the sim runs
//! in 3D ([`super::sim::Sim<3>`]), the camera rotates the world and
//! perspective-divides onto the screen, and the painter's algorithm draws
//! far→near — no depth buffer, no GPU 3D, everything still lands in
//! `paint_quad`/`paint_path`.
//!
//! The camera is d3/orbit-style rather than a full view matrix: yaw around
//! the vertical axis, pitch around the horizontal, a fixed camera distance
//! that sets perspective strength, and the same pan/zoom screen transform
//! as the 2D [`super::geom::Camera`]. Hit-testing needs no new machinery:
//! project, then hit-test the projected 2D points with the existing
//! [`super::geom::Grid`] (radii scale with the perspective factor).
//!
//! Native scalar math on purpose (see PROGRESS.md for the glam-vs-native
//! bench): the per-point cost is ~10 flops; the projection of a 5k-node
//! frame is microseconds either way, and the painter sort dominates.

/// Camera distance from the origin, in world units. Bigger = flatter
/// perspective; the default keeps a ~1000-unit layout comfortably
/// three-dimensional without extreme foreshortening.
pub const DEFAULT_DIST: f32 = 2200.0;
/// Near guard: points closer to the camera than this clamp to it (a node
/// cannot project to infinity).
pub const NEAR: f32 = 50.0;

/// The 3D camera.
#[derive(Debug, Clone, Copy)]
pub struct Cam3 {
    /// Rotation about the vertical (y) axis, radians.
    pub yaw: f32,
    /// Rotation about the horizontal (x) axis, radians. Clamped to ±85°
    /// so the view never flips over the poles (gimbal confusion, not a
    /// gimbal lock — yaw/pitch has one at the poles, which the clamp
    /// avoids).
    pub pitch: f32,
    /// Screen translation, logical px.
    pub pan: [f32; 2],
    /// Screen scale (shares the 2D camera's bounds).
    pub zoom: f32,
    /// Camera distance (perspective strength).
    pub dist: f32,
}

/// One projected point: where it lands on screen, how far it is, and the
/// perspective scale factor (multiply node radii and label sizes by it;
/// sort by `view_z` for the painter's algorithm).
#[derive(Debug, Clone, Copy)]
pub struct Projected {
    pub xy: [f32; 2],
    /// Rotated z in view space (camera looks down -z; bigger = nearer).
    pub view_z: f32,
    /// Perspective foreshortening: 1.0 at the origin, > 1 nearer, < 1
    /// farther. Never negative (the near clamp guarantees it).
    pub scale: f32,
}

const PITCH_LIMIT: f32 = 1.484; // 85°

impl Cam3 {
    pub fn new() -> Self {
        Self {
            yaw: 0.0,
            pitch: -0.35, // a slight downward tilt reads as "3D" immediately
            pan: [0.0, 0.0],
            zoom: 1.0,
            dist: DEFAULT_DIST,
        }
    }

    /// Orbit: drag rotates the world. `dx` is screen px (yaw), `dy` screen
    /// px (pitch); ~0.006 rad/px feels right at typical window sizes.
    pub fn orbit_by(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.006;
        self.pitch = (self.pitch + dy * 0.006).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    pub fn pan_by(&mut self, d: [f32; 2]) {
        self.pan[0] += d[0];
        self.pan[1] += d[1];
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(super::geom::MIN_ZOOM, super::geom::MAX_ZOOM);
    }

    /// Wheel zoom anchored at a screen point — same contract as the 2D
    /// camera: the world point under the cursor stays under it. Uses the
    /// projected position, so it is exact for what the user sees.
    pub fn zoom_at(&mut self, anchor: [f32; 2], factor: f32) {
        let old = self.zoom;
        self.set_zoom(old * factor);
        let f = self.zoom / old;
        self.pan = [
            anchor[0] - (anchor[0] - self.pan[0]) * f,
            anchor[1] - (anchor[1] - self.pan[1]) * f,
        ];
    }

    /// Rotate a world point into view space (yaw then pitch).
    fn rotate(&self, p: [f32; 3], sy: f32, cy: f32, sp: f32, cp: f32) -> [f32; 3] {
        // Yaw about y.
        let x = p[0] * cy - p[2] * sy;
        let z = p[0] * sy + p[2] * cy;
        // Pitch about x.
        let y = p[1] * cp - z * sp;
        let z = p[1] * sp + z * cp;
        [x, y, z]
    }

    /// Project one point. Precompute the trig with [`Self::trig`] when
    /// projecting a batch.
    pub fn project(&self, p: [f32; 3]) -> Projected {
        let (sy, cy, sp, cp) = self.trig();
        self.project_with(p, sy, cy, sp, cp)
    }

    pub fn trig(&self) -> (f32, f32, f32, f32) {
        (
            self.yaw.sin(),
            self.yaw.cos(),
            self.pitch.sin(),
            self.pitch.cos(),
        )
    }

    pub fn project_with(&self, p: [f32; 3], sy: f32, cy: f32, sp: f32, cp: f32) -> Projected {
        let [x, y, z] = self.rotate(p, sy, cy, sp, cp);
        // Camera at +dist looking toward the origin: view depth is how far
        // in front of the camera plane the point sits.
        let depth = (self.dist - z).max(NEAR);
        let scale = self.dist / depth;
        Projected {
            xy: [
                x * scale * self.zoom + self.pan[0],
                // Screen y grows downward; world y grows up.
                -y * scale * self.zoom + self.pan[1],
            ],
            view_z: z,
            scale,
        }
    }

    /// Project every point into `out` (reused across frames; the tab owns
    /// one buffer and ships it to the painter).
    pub fn project_all(&self, pos: &[[f32; 3]], out: &mut Vec<Projected>) {
        out.clear();
        out.reserve(pos.len());
        let t = self.trig();
        for p in pos {
            out.push(self.project_with(*p, t.0, t.1, t.2, t.3));
        }
    }

    /// Painter's-algorithm order: indices sorted far→near (ascending
    /// view_z), so painting in this order draws nearer things last, over
    /// the top. Stable enough: ties by index keep it deterministic.
    pub fn painter_order(&self, projected: &[Projected]) -> Vec<u32> {
        let mut order: Vec<u32> = (0..projected.len() as u32).collect();
        order.sort_by(|a, b| {
            projected[*a as usize]
                .view_z
                .partial_cmp(&projected[*b as usize].view_z)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        order
    }

    /// Fit a 3D spread into `bounds` ([x, y, w, h] px): project at the
    /// current rotation, frame the projected extent, keep the current
    /// rotation. The 3D analogue of [`super::geom::Camera::fit`].
    pub fn fit(&mut self, pos: &[[f32; 3]], bounds: [f32; 4], margin: f32) {
        if pos.is_empty() {
            self.pan = [bounds[0] + bounds[2] / 2.0, bounds[1] + bounds[3] / 2.0];
            return;
        }
        let t = self.trig();
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in pos {
            let pr = self.project_with(*p, t.0, t.1, t.2, t.3);
            lo[0] = lo[0].min(pr.xy[0]);
            lo[1] = lo[1].min(pr.xy[1]);
            hi[0] = hi[0].max(pr.xy[0]);
            hi[1] = hi[1].max(pr.xy[1]);
        }
        // Undo the current pan/zoom to get the raw extent, then re-fit.
        let raw = |v: f32, axis: usize| (v - self.pan[axis]) / self.zoom;
        let (rlo, rhi) = (
            [raw(lo[0], 0), raw(lo[1], 1)],
            [raw(hi[0], 0), raw(hi[1], 1)],
        );
        let w = (rhi[0] - rlo[0]).max(1.0);
        let h = (rhi[1] - rlo[1]).max(1.0);
        self.zoom = ((bounds[2] - 2.0 * margin) / w)
            .min((bounds[3] - 2.0 * margin) / h)
            .clamp(super::geom::MIN_ZOOM, super::geom::MAX_ZOOM);
        let cx = (rlo[0] + rhi[0]) / 2.0;
        let cy = (rlo[1] + rhi[1]) / 2.0;
        self.pan = [
            bounds[0] + bounds[2] / 2.0 - cx * self.zoom,
            bounds[1] + bounds[3] / 2.0 - cy * self.zoom,
        ];
    }

    /// Inverse for node dragging: given a screen point and the node's
    /// *view-space* depth (`Projected.view_z`, the invariant while the
    /// world rotates under the cursor), find the world position whose
    /// projection sits at that screen point at that depth. The tab pins
    /// the dragged node here each frame, so dragging feels like grabbing
    /// the node even while the world rotates behind it.
    pub fn screen_to_world_at_depth(&self, screen: [f32; 2], view_z: f32) -> [f32; 3] {
        let (sy, cy, sp, cp) = self.trig();
        // Undo pan/zoom and the perspective divide at the fixed depth.
        let depth = (self.dist - view_z).max(NEAR);
        let scale = self.dist / depth;
        let vx = (screen[0] - self.pan[0]) / (self.zoom * scale);
        let vy = -(screen[1] - self.pan[1]) / (self.zoom * scale);
        // (vx, vy, view_z) is the view-space point; undo pitch then yaw.
        let y0 = vy * cp + view_z * sp;
        let z0 = -vy * sp + view_z * cp;
        let x0 = vx * cy + z0 * sy;
        let z1 = -vx * sy + z0 * cy;
        [x0, y0, z1]
    }
}

impl Default for Cam3 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn identity_projection_is_orthographic_like_at_the_origin() {
        let cam = Cam3 {
            yaw: 0.0,
            pitch: 0.0,
            pan: [0.0, 0.0],
            zoom: 1.0,
            dist: DEFAULT_DIST,
        };
        let p = cam.project([0.0, 0.0, 0.0]);
        assert!(approx(p.xy[0], 0.0) && approx(p.xy[1], 0.0));
        assert!(approx(p.scale, 1.0));
        // Nearer the camera (bigger z) => bigger scale.
        let near = cam.project([0.0, 0.0, 500.0]);
        let far = cam.project([0.0, 0.0, -500.0]);
        assert!(near.scale > 1.0 && far.scale < 1.0);
        // World +y maps to screen -y (up is up).
        let up = cam.project([0.0, 100.0, 0.0]);
        assert!(up.xy[1] < 0.0);
    }

    #[test]
    fn yaw_moves_x_and_z_together() {
        let cam = Cam3 {
            yaw: std::f32::consts::FRAC_PI_2,
            ..Cam3::new()
        };
        let p = cam.project([100.0, 0.0, 0.0]);
        // A quarter turn puts +x on the +z axis: dead center, nearer.
        assert!(approx(p.xy[0], 0.0));
        assert!(p.scale > 1.0);
    }

    #[test]
    fn pitch_clamps_at_the_poles() {
        let mut cam = Cam3::new();
        for _ in 0..1000 {
            cam.orbit_by(0.0, 1000.0);
        }
        assert!(cam.pitch <= PITCH_LIMIT + 1e-6);
        for _ in 0..1000 {
            cam.orbit_by(0.0, -1000.0);
        }
        assert!(cam.pitch >= -PITCH_LIMIT - 1e-6);
    }

    #[test]
    fn painter_order_draws_far_first() {
        let cam = Cam3::new();
        let pts = [[0.0, 0.0, -300.0], [0.0, 0.0, 300.0], [0.0, 0.0, 0.0]];
        let mut proj = Vec::new();
        cam.project_all(&pts, &mut proj);
        let order = cam.painter_order(&proj);
        assert_eq!(order, vec![0, 2, 1], "far (-z) first, near (+z) last");
    }

    #[test]
    fn near_clamp_prevents_infinity() {
        let cam = Cam3::new();
        let p = cam.project([0.0, 0.0, cam.dist]); // at the camera plane
        assert!(p.xy[0].is_finite() && p.xy[1].is_finite() && p.scale.is_finite());
        assert!(p.scale > 0.0);
    }

    #[test]
    fn zoom_at_keeps_the_projected_point_under_the_cursor() {
        let mut cam = Cam3 {
            yaw: 0.7,
            pitch: -0.4,
            pan: [300.0, 200.0],
            zoom: 1.3,
            dist: DEFAULT_DIST,
        };
        let world = [120.0, -40.0, 80.0];
        let anchor = cam.project(world).xy;
        cam.zoom_at(anchor, 1.15);
        let after = cam.project(world).xy;
        assert!(
            approx(anchor[0], after[0]) && approx(anchor[1], after[1]),
            "{anchor:?} vs {after:?}"
        );
    }

    #[test]
    fn fit_frames_the_spread() {
        let mut cam = Cam3 {
            yaw: 0.5,
            ..Cam3::new()
        };
        let pos = [
            [-400.0, -400.0, -400.0],
            [400.0, 400.0, 400.0],
            [400.0, -400.0, -400.0],
            [-400.0, 400.0, 400.0],
        ];
        cam.fit(&pos, [0.0, 0.0, 800.0, 600.0], 30.0);
        let mut proj = Vec::new();
        cam.project_all(&pos, &mut proj);
        for p in &proj {
            assert!(p.xy[0] >= 29.0 && p.xy[0] <= 771.0, "x {:?}", p.xy);
            assert!(p.xy[1] >= 29.0 && p.xy[1] <= 571.0, "y {:?}", p.xy);
        }
    }

    #[test]
    fn drag_inverse_round_trips() {
        let cam = Cam3 {
            yaw: 0.9,
            pitch: -0.6,
            pan: [410.0, 260.0],
            zoom: 1.7,
            dist: 1800.0,
        };
        let world = [230.0, -120.0, 90.0];
        let pr = cam.project(world);
        // The invariant during a drag is the *view-space* depth (what the
        // perspective divide used), not the world z — the world point
        // moves as the world rotates under the cursor.
        let back = cam.screen_to_world_at_depth(pr.xy, pr.view_z);
        let pr2 = cam.project(back);
        assert!(approx(pr.xy[0], pr2.xy[0]) && approx(pr.xy[1], pr2.xy[1]));
        assert!(approx(pr.view_z, pr2.view_z));
        assert!(approx(pr.scale, pr2.scale));
    }

    #[test]
    fn projection_is_deterministic_and_finite_for_a_settled_layout() {
        use crate::graph::sim::{Sim, SimParams};
        let edges: Vec<[u32; 2]> = (1..200).map(|i| [i - 1, i]).collect();
        let mut sim = Sim::<3>::new(200, &edges, SimParams::default(), 5);
        while sim.step() {}
        let cam = Cam3 {
            yaw: 1.2,
            pitch: 0.8,
            ..Cam3::new()
        };
        let mut proj = Vec::new();
        cam.project_all(sim.positions(), &mut proj);
        assert!(proj.iter().all(|p| p.xy[0].is_finite()
            && p.xy[1].is_finite()
            && p.scale.is_finite()
            && p.scale > 0.0));
    }
}

/// glam-vs-native projection bench (#[ignore]; run with:
/// `cargo test --release -p shale-core bench_proj -- --ignored --nocapture`).
///
/// The question this answers: does pulling glam into the *real* dependency
/// graph buy anything for the M2 projection layer? Both paths do the same
/// work per point — rotate (yaw+pitch), perspective divide, screen
/// transform — the glam path with Vec3A + a composed Mat4, the native
/// path with the scalar code above. If glam wins meaningfully, it becomes
/// a real dependency; otherwise the native code stays.
#[cfg(test)]
mod bench {
    use super::*;
    use glam::{Affine3A, Vec3, Vec3A};
    use std::time::Instant;

    fn layout(n: usize) -> Vec<[f32; 3]> {
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
        let mut sim = Sim::<3>::new(n, &edges, SimParams::default(), 7);
        while sim.step() {}
        sim.positions().to_vec()
    }

    /// The glam path: one composed Mat4 (yaw*pitch*perspective*screen),
    /// applied per point. This is the shape a glam-based renderer would
    /// actually use.
    fn project_glam(cam: &Cam3, pos: &[[f32; 3]], out: &mut Vec<[f32; 3]>) {
        // Rotation is affine, so the fastest glam form applies: Affine3A
        // (SSE-friendly, no bottom row) for the rotate; the perspective
        // divide and screen transform stay scalar either way.
        // Match the native order and handedness: native yaw is the negative
        // of glam's right-handed Y rotation (it only decides which way a
        // drag rotates the world), and pitch composes after yaw.
        let m = Affine3A::from_axis_angle(Vec3::X, cam.pitch)
            * Affine3A::from_axis_angle(Vec3::Y, -cam.yaw);
        out.clear();
        out.reserve(pos.len());
        for p in pos {
            let v = m.transform_vector3a(Vec3A::from(*p)).to_array();
            let depth = (cam.dist - v[2]).max(NEAR);
            let scale = cam.dist / depth;
            out.push([
                v[0] * scale * cam.zoom + cam.pan[0],
                -v[1] * scale * cam.zoom + cam.pan[1],
                v[2],
            ]);
        }
    }

    #[test]
    #[ignore]
    fn bench_proj() {
        for n in [2000usize, 5000, 10000] {
            let pos = layout(n);
            let cam = Cam3 {
                yaw: 0.9,
                pitch: -0.5,
                pan: [600.0, 400.0],
                zoom: 1.4,
                ..Cam3::new()
            };

            let mut out_native = Vec::with_capacity(n);
            let t0 = Instant::now();
            for _ in 0..200 {
                cam.project_all(&pos, &mut out_native);
            }
            let native = t0.elapsed();

            let mut out_glam = Vec::with_capacity(n);
            let t0 = Instant::now();
            for _ in 0..200 {
                project_glam(&cam, &pos, &mut out_glam);
            }
            let glam_t = t0.elapsed();

            // Sanity: the two paths agree to f32 noise. Tolerance is
            // relative: near the near-plane the perspective factor is
            // large and absolute error grows with it.
            for i in 0..n {
                let a = out_native[i];
                let b = out_glam[i];
                let tol = (a.xy[0].abs().max(a.xy[1].abs())).max(100.0) * 1e-5;
                assert!(
                    (a.xy[0] - b[0]).abs() < tol && (a.xy[1] - b[1]).abs() < tol,
                    "mismatch at {i}: {a:?} vs {b:?}"
                );
            }

            println!(
                "n={n:6}  native {:>8.2?}/frame   glam {:>8.2?}/frame",
                native / 200,
                glam_t / 200
            );
        }
    }
}
