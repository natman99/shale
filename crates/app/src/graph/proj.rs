//! Projected 3D for the graph tab (roadmap P3.6, `graph-view-plan.md` M2a).
//!
//! The physics already runs in three axes when the tab asks for it
//! ([`super::sim::GraphSim`]); this is the half that turns those positions into a
//! picture. core's [`Cam3`] does the math — rotate, perspective divide, painter
//! order — and this module does the three things a tab needs around it:
//!
//! * **One projection per picture**, built when the frame or the camera changes and
//!   kept in the view, so the paint and the pointer answer to the *same* projected
//!   points. A hit test against world positions would ask where the user clicked and
//!   get an answer from where the graph used to be.
//! * **The hit test in screen space**, over a [`Grid`] built from the projected
//!   points — the binding rule in PROJECT.md ("2D / 3D mode rules"): a grid built
//!   from the *layout's* bounds misses the nodes on the edge of what the screen
//!   shows, which is the thread-38 bug wearing a new costume.
//! * **The paint, far→near.** Nodes sort by view depth and take their radius and
//!   alpha from the perspective factor. Links stay in their own layer *under* the
//!   nodes: gpui orders primitives inside one layer by kind, so interleaving links
//!   and nodes by depth would undo the batching win that took a 5k frame from
//!   14.8 ms to 2.3 (PROJECT.md thread 43). v1 approximates, and says so.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{point, px, size, App, Bounds, ContentMask, Pixels, Window};
use shale_core::graph::geom::Grid;
use shale_core::graph::proj::{Cam3, Projected};
use shale_core::graph::NodeKind;
use shale_core::theme::Palette;

use super::cache::{Cache, Geometry, Key};
use super::frame::GraphFrame;
use super::render::{
    label_runs, ring_px, Labels, LinkPaths, Look, CENTRE_RING_PX, LABEL_SCALE, LINK_MIN_PX,
    LINK_WIDTH,
};
use super::sim::GRID_MAX_SPAN;

/// The smallest projected dot that earns a label, in screen px. Below this the
/// characters are wider than the node they name, which is the same reason the 2D
/// renderer fades its label count out at low zoom — here the perspective factor does
/// the fading per node instead of the zoom doing it per frame.
const LABEL_MIN_DOT_PX: f32 = 1.5;
/// The projected space names **fewer** nodes than the flat one, and that is not a
/// performance choice — it is the clutter. A third axis seen from outside folds depth
/// into the same screen area, so nodes arrive bunched: at the flat view's budget the
/// labels land on top of each other in the middle of the cloud, where the text stops
/// being a name and becomes texture. Highest degree still wins, and nearness breaks
/// the ties, so what survives the cut is the vocabulary of the part being looked at.
const LABEL_CAP_3D: usize = 120;

/// The alpha a node keeps at the far end of the depth range. Nearer nodes come back
/// toward 1.0; nothing goes fully transparent, because a node you cannot see is a
/// note that has stopped existing.
const DEEP_ALPHA: f32 = 0.45;

/// The frame's nodes as the screen sees them, with the index the pointer needs.
///
/// Built once per (frame, camera) pair and shared by the paint and the hit test, so
/// the two can never disagree about where a node is.
pub(crate) struct Projection {
    /// One entry per node, in the frame's slot order: screen px, view depth, and
    /// the perspective factor the radius and the label size are multiplied by.
    projected: Vec<Projected>,
    /// Node indexes far→near — the order the nodes are painted in.
    order: Vec<u32>,
    /// The shallowest and deepest `view_z`, so the alpha ramp is relative to this
    /// graph rather than to a world-unit constant.
    depth_range: (f32, f32),
    /// The spatial index over the **projected** points.
    grid: Grid,
    /// The box those points occupy, for the same reason the 2D frame carries its
    /// layout bounds: [`Grid`] has no answer for a query outside the box it was
    /// built from, and the pointer is outside it constantly.
    bounds: Option<([f32; 2], [f32; 2])>,
    /// The camera this was projected with. Part of the paint cache's key: a rotation
    /// moves every vertex, so a cached batch belongs to one camera and nothing else.
    camera: Cam3,
}

impl Projection {
    /// Project every node of `frame` with `camera`, and index the result.
    pub(crate) fn build(frame: &GraphFrame, camera: &Cam3) -> Self {
        let (sy, cy, sp, cp) = camera.trig();
        let projected: Vec<Projected> = frame
            .nodes
            .iter()
            .map(|node| {
                camera.project_with(
                    [node.position[0], node.position[1], node.depth],
                    sy,
                    cy,
                    sp,
                    cp,
                )
            })
            .collect();

        // The projected box and the depth ramp, in one pass.
        let mut bounds = None;
        let mut depth_range = (f32::MAX, f32::MIN);
        for p in &projected {
            depth_range.0 = depth_range.0.min(p.view_z);
            depth_range.1 = depth_range.1.max(p.view_z);
            bounds = match bounds {
                None => Some(([p.xy[0], p.xy[1]], [p.xy[0], p.xy[1]])),
                Some((mut lo, mut hi)) => {
                    lo[0] = lo[0].min(p.xy[0]);
                    lo[1] = lo[1].min(p.xy[1]);
                    hi[0] = hi[0].max(p.xy[0]);
                    hi[1] = hi[1].max(p.xy[1]);
                    Some((lo, hi))
                }
            };
        }
        if projected.is_empty() {
            depth_range = (0.0, 0.0);
        }

        // Painter's order: far first, so the near nodes land on top. Same rule as
        // core's `Cam3::painter_order`, applied to the depths we already have rather
        // than re-projecting.
        let mut order: Vec<u32> = (0..projected.len() as u32).collect();
        order.sort_by(|a, b| {
            projected[*a as usize]
                .view_z
                .partial_cmp(&projected[*b as usize].view_z)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // The hit-test grid, over the projected points. Cells about twice the largest
        // projected dot, with the same span floor the 2D frame uses: `Grid::build`
        // allocates a bucket per cell, and a layout that has flown apart would ask for
        // gigabytes (PROJECT.md thread 39).
        let biggest = frame
            .nodes
            .iter()
            .zip(&projected)
            .map(|(node, p)| drawn_radius(node.radius, p.scale))
            .fold(0.0, f32::max);
        let wanted = (2.0 * biggest).max(8.0);
        let cell = match bounds {
            Some(([lo_x, lo_y], [hi_x, hi_y])) => {
                let floor = (hi_x - lo_x).max(hi_y - lo_y) / GRID_MAX_SPAN;
                wanted.max(floor)
            }
            None => wanted,
        };
        let points: Vec<[f32; 2]> = projected.iter().map(|p| p.xy).collect();
        let grid = Grid::build(&points, cell);

        Self {
            projected,
            order,
            depth_range,
            grid,
            bounds,
            camera: *camera,
        }
    }

    /// The camera this picture was made for — the paint cache's key, and the session's
    /// orientation memory.
    pub(crate) fn camera(&self) -> Cam3 {
        self.camera
    }

    /// Where a node landed on screen, in window px.
    pub(crate) fn screen(&self, slot: usize) -> Option<[f32; 2]> {
        self.projected.get(slot).map(|p| p.xy)
    }

    /// A node's view-space depth, which is the invariant a drag holds: the world point
    /// under the cursor moves as the world rotates, but the depth the perspective divide
    /// used does not.
    pub(crate) fn view_z(&self, slot: usize) -> Option<f32> {
        self.projected.get(slot).map(|p| p.view_z)
    }

    /// A node's radius as drawn: its world radius times a **damped** perspective factor,
    /// so a near hub is a bigger dot than the same hub far away without the whole cloud
    /// resizing every time the eye moves. See [`PERSP_SIZE`].
    fn radius(&self, frame: &GraphFrame, slot: usize) -> f32 {
        match (frame.nodes.get(slot), self.projected.get(slot)) {
            (Some(node), Some(p)) => drawn_radius(node.radius, p.scale),
            _ => 0.0,
        }
    }

    /// How opaque a node at `slot` is, from where it sits in this graph's depth range.
    fn alpha(&self, slot: usize) -> f32 {
        let (lo, hi) = self.depth_range;
        if hi - lo < 1.0 {
            return 1.0;
        }
        let t = (self.projected[slot].view_z - lo) / (hi - lo);
        DEEP_ALPHA + (1.0 - DEEP_ALPHA) * t
    }

    /// The node under a screen point, generous by `slop` **screen** px.
    ///
    /// The shape of this is the 2D frame's [`GraphFrame::node_at`] on purpose, including
    /// the clamp-to-box retry: the grid answers "nothing" for a query outside the box it
    /// was built from, and a pointer beside an edge node is a pointer on that node.
    pub(crate) fn node_at(&self, frame: &GraphFrame, screen: [f32; 2], slop: f32) -> Option<usize> {
        let points: Vec<[f32; 2]> = self.projected.iter().map(|p| p.xy).collect();
        // Walked over **this picture's** nodes, not the frame's. A projection is a
        // snapshot of the graph as it was when it was built, and its slots are that
        // graph's slots; the frame the view is holding now can be bigger, smaller, or a
        // different neighbourhood altogether. Reaching for `frame.nodes.len()` here was
        // the crash: "index out of bounds: the len is 12 but the index is 12", from a
        // pointer event landing between a widened frame arriving and the frame being
        // painted. [`Self::radius`] is the guarded door for the same question.
        let biggest = (0..self.projected.len())
            .map(|slot| self.radius(frame, slot))
            .fold(0.0, f32::max);
        let hit = self
            .grid
            .nearest(&points, screen, biggest + slop, None)
            .or_else(|| {
                let (lo, hi) = self.bounds?;
                let clamped = [screen[0].clamp(lo[0], hi[0]), screen[1].clamp(lo[1], hi[1])];
                if clamped == screen {
                    return None;
                }
                self.grid.nearest(&points, clamped, biggest + slop, None)
            });
        hit.map(|slot| slot as usize)
            .filter(|&slot| match self.screen(slot) {
                Some(xy) => {
                    let dx = xy[0] - screen[0];
                    let dy = xy[1] - screen[1];
                    (dx * dx + dy * dy).sqrt() <= self.radius(frame, slot) + slop
                }
                None => false,
            })
    }

    /// The nearest node to a screen point, however far: the keyboard's way in.
    pub(crate) fn nearest(&self, screen: [f32; 2]) -> Option<usize> {
        self.projected
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| distance(a.xy, screen).total_cmp(&distance(b.xy, screen)))
            .map(|(slot, _)| slot)
    }

    /// `h`/`j`/`k`/`l` in 3D: the nearest node in that **screen** direction, within
    /// core's ±30° cone. The directions are screen directions here, not world ones —
    /// in a rotating world, `l` means "the next node to the right of what I have",
    /// which is what a person pressing `l` means.
    pub(crate) fn node_toward(&self, from: usize, dir: [f32; 2]) -> Option<usize> {
        if from >= self.projected.len() {
            return None;
        }
        let points: Vec<[f32; 2]> = self.projected.iter().map(|p| p.xy).collect();
        self.grid
            .in_cone(&points, from as u32, dir, None)
            .map(|slot| slot as usize)
    }
}

// ----- the paint ------------------------------------------------------------

/// The 3D frame: links, the chosen node's links, nodes far→near, labels — each phase
/// in its own [`gpui::Window::paint_layer`], for the same reason the 2D frame is
/// layered (thread 43).
///
/// The caller hands over a [`Projection`] it built with the same camera it will use
/// for the pointer, so what the user clicked and what the user saw are one answer.
#[allow(clippy::too_many_arguments)] // a paint pass's arguments are the frame's context
pub(crate) fn paint_graph3d(
    bounds: Bounds<Pixels>,
    frame: &Rc<GraphFrame>,
    projection: &Projection,
    look: &Look,
    cache: &mut Cache,
    label_memory: &RefCell<Vec<usize>>,
    window: &mut Window,
    cx: &mut App,
) {
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let palette = look.palette.as_ref();
        let key = Key::new(frame, Geometry::Deep(projection.camera()), bounds);

        window.paint_layer(bounds, |window| {
            paint_links3d(
                cache,
                &key,
                frame,
                projection,
                palette.graph_edge,
                bounds,
                window,
            );
        });
        if let Some(slot) = look.hover {
            window.paint_layer(bounds, |window| {
                paint_hover_links3d(
                    frame,
                    projection,
                    slot,
                    palette.graph_node_active,
                    bounds,
                    window,
                );
            });
        }
        window.paint_layer(bounds, |window| {
            paint_nodes3d(
                frame,
                projection,
                palette,
                look.hover,
                look.centre,
                bounds,
                window,
            );
        });
        window.paint_layer(bounds, |window| {
            paint_labels3d(
                frame,
                projection,
                &look.labels,
                palette,
                bounds,
                look.hover,
                look.centre,
                label_memory,
                window,
                cx,
            );
        });
    });
}

/// How much of the perspective factor a drawn dot carries. Positions are projected in
/// full — the cloud is where it is — but the dot size carries only this fraction of it.
///
/// At the full factor every node in the cloud resizes whenever the eye moves, because
/// `dist/(dist - view_z)` is a function of the eye distance and not only of where the node
/// sits: measured on a 61-note hub, a widening that shrank the picture by 42% grew the
/// hub's own dot by 4.5%, and a layout still settling under the forces swings every node's
/// depth, which at full factor is a ±7% swing in every dot at once — the whole graph
/// pulsing. Damped to 0.4, the depth cue between a near hub and a far one survives at
/// roughly half strength and the pulsing stops being legible as motion.
const PERSP_SIZE: f32 = 0.4;

/// A world radius at a perspective factor, damped by [`PERSP_SIZE`]. The single place
/// this is computed, so the dot that is drawn, the dot that is hit-tested, and the grid
/// cell that indexes them can never disagree about how big a node is.
pub(crate) fn drawn_radius(world: f32, scale: f32) -> f32 {
    world * (1.0 + PERSP_SIZE * (scale - 1.0))
}

/// Whether a projected point with radius `r` is anywhere in the canvas.
fn on_screen(xy: [f32; 2], r: f32, bounds: Bounds<Pixels>) -> bool {
    let lo_x = bounds.origin.x.as_f32();
    let lo_y = bounds.origin.y.as_f32();
    let hi_x = lo_x + bounds.size.width.as_f32();
    let hi_y = lo_y + bounds.size.height.as_f32();
    xy[0] + r >= lo_x && xy[0] - r <= hi_x && xy[1] + r >= lo_y && xy[1] - r <= hi_y
}

/// Every link, projected, in as few paths as gpui allows — the same batching the 2D
/// renderer uses, and the same cache, now keyed on the 3D camera: **a rotation moves
/// every vertex**, so a batch belongs to one orientation and nothing else. A still
/// camera replays it; an orbiting one pays the rebuild, which is the honest cost of
/// turning the graph.
fn paint_links3d(
    cache: &mut Cache,
    key: &Key,
    frame: &GraphFrame,
    projection: &Projection,
    color: u32,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let color = gpui::rgb(color);
    if let Some(paths) = cache.links(key) {
        for path in paths {
            window.paint_path(path.clone(), color);
        }
        return;
    }
    let keep = cache.worth_keeping(key);
    let mut links = LinkPaths::new(px(LINK_WIDTH.max(LINK_MIN_PX)), frame.edges.len());
    for &[from, to] in &frame.edges {
        let (Some(a), Some(b)) = (
            projection.screen(from as usize),
            projection.screen(to as usize),
        ) else {
            continue;
        };
        // Cull on the pair's box, exactly as the 2D pass does: a hub's links are long
        // and cross the pane from off-screen, so the box is the whole test (thread 42).
        if boxes_miss(a, b, bounds) {
            continue;
        }
        links.segment(a, b);
    }
    let paths = links.into_paths();
    if keep {
        for path in &paths {
            window.paint_path(path.clone(), color);
        }
        cache.store_links(key.clone(), paths);
    } else {
        for path in paths {
            window.paint_path(path, color);
        }
    }
    cache.used(key.clone());
}

/// Whether a segment's bounding box misses the canvas. The 2D renderer's note on this
/// applies verbatim in 3D: the exact clip was measured and lost, because a graph is
/// not a scene of short local segments.
fn boxes_miss(a: [f32; 2], b: [f32; 2], bounds: Bounds<Pixels>) -> bool {
    let lo_x = bounds.origin.x.as_f32();
    let lo_y = bounds.origin.y.as_f32();
    let hi_x = lo_x + bounds.size.width.as_f32();
    let hi_y = lo_y + bounds.size.height.as_f32();
    a[0].max(b[0]) < lo_x || a[0].min(b[0]) > hi_x || a[1].max(b[1]) < lo_y || a[1].min(b[1]) > hi_y
}

/// The chosen node's links, in the active colour: the same answer as the 2D hover
/// pass, projected. Not cached, for the same reason (the hover slot decides it, and
/// the phase is a fraction of the frame).
fn paint_hover_links3d(
    frame: &GraphFrame,
    projection: &Projection,
    slot: usize,
    color: u32,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let Some(far) = projection.screen(slot) else {
        return;
    };
    let mut links = LinkPaths::new(px(LINK_WIDTH.max(LINK_MIN_PX)), 64);
    for &[from, to] in &frame.edges {
        let other = match (from as usize, to as usize) {
            (a, b) if a == slot => b,
            (a, b) if b == slot => a,
            _ => continue,
        };
        let Some(near) = projection.screen(other) else {
            continue;
        };
        if boxes_miss(far, near, bounds) {
            continue;
        }
        links.segment(far, near);
    }
    let color = gpui::rgb(color);
    for path in links.into_paths() {
        window.paint_path(path, color);
    }
}

/// The nodes, far→near, each one a disc sized and dimmed by its depth.
///
/// **The depth order is real; the interleaving with links is not.** Nodes paint over
/// all links, near or far, because putting links between nodes by depth would mean
/// one layer for both, and gpui orders primitives in one layer by *kind* — which is
/// the bug that put every link on top of every node (thread 43). The depth still
/// reads, from the size ramp and the alpha ramp; what it cannot do is hide a far node
/// behind a near one's link.
fn paint_nodes3d(
    frame: &GraphFrame,
    projection: &Projection,
    palette: &Palette,
    hover: Option<usize>,
    centre: Option<usize>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    // The neighbourhood's centre, haloed wider than any hover: in a cloud of two hundred
    // dots the one the window is showing has to be findable without a search.
    if let Some(slot) = centre {
        if let Some(center) = projection.screen(slot) {
            let r = projection.radius(frame, slot);
            let r = r + ring_px(r) + CENTRE_RING_PX;
            if on_screen(center, r, bounds) {
                window.paint_quad(
                    gpui::fill(
                        Bounds {
                            origin: point(px(center[0] - r), px(center[1] - r)),
                            size: size(px(2.0 * r), px(2.0 * r)),
                        },
                        gpui::rgb(palette.graph_node_active),
                    )
                    .corner_radii(px(r)),
                );
            }
        }
    }
    if let Some(slot) = hover {
        if let Some(center) = projection.screen(slot) {
            let r = projection.radius(frame, slot);
            let r = r + ring_px(r);
            if on_screen(center, r, bounds) {
                window.paint_quad(
                    gpui::fill(
                        Bounds {
                            origin: point(px(center[0] - r), px(center[1] - r)),
                            size: size(px(2.0 * r), px(2.0 * r)),
                        },
                        gpui::rgb(palette.graph_node_active),
                    )
                    .corner_radii(px(r)),
                );
            }
        }
    }
    for &slot in &projection.order {
        let slot = slot as usize;
        let Some(node) = frame.nodes.get(slot) else {
            continue;
        };
        let fill = match node.kind {
            NodeKind::Note => palette.graph_node,
            NodeKind::Attachment => palette.graph_attachment,
            NodeKind::Query => palette.graph_query,
            NodeKind::Tag => palette.graph_tag,
            NodeKind::Ghost => palette.graph_ghost,
        };
        let Some(center) = projection.screen(slot) else {
            continue;
        };
        let r = projection.radius(frame, slot);
        if !on_screen(center, r, bounds) {
            continue;
        }
        let mut color = gpui::rgb(fill);
        color.a = projection.alpha(slot);
        window.paint_quad(
            gpui::fill(
                Bounds {
                    origin: point(px(center[0] - r), px(center[1] - r)),
                    size: size(px(2.0 * r), px(2.0 * r)),
                },
                color,
            )
            .corner_radii(px(r)),
        );
    }
}

/// Who gets named in the projected space: at most [`LABEL_CAP_3D`], on the nodes big
/// enough on screen to name, highest degree first with nearness breaking a tie — the
/// 2D rule, with the perspective factor deciding who is too small to name instead of
/// the zoom deciding it for the whole frame at once.
///
/// Split out from the paint because it is a *choice*, and a choice this visible is
/// worth pinning: [`label_slots_respects_the_cap`] checks the budget and who survives
/// it without a window in sight.
pub(crate) fn label_slots(
    frame: &GraphFrame,
    projection: &Projection,
    bounds: Bounds<Pixels>,
    hover: Option<usize>,
    centre: Option<usize>,
) -> Vec<usize> {
    // Ranked, then cut. The tuple *is* the ranking — degree first, and nearness (which
    // is what `alpha` reports) breaking a tie so a hub far away does not out-shout the
    // note the user is actually looking at. It used to be built, mapped back to slots,
    // and thrown away without a sort, which made the cut below "the first 120 nodes by
    // index" — the doc comment promised hubs and the code delivered whatever the index
    // scan happened to produce. `label_slots_respects_the_cap` is what keeps that from
    // coming back.
    let mut ranked: Vec<(std::cmp::Reverse<u32>, std::cmp::Reverse<[u8; 4]>, usize)> = frame
        .nodes
        .iter()
        .enumerate()
        .filter(|(slot, _)| {
            projection
                .screen(*slot)
                .is_some_and(|xy| on_screen(xy, projection.radius(frame, *slot), bounds))
                && projection.radius(frame, *slot) >= LABEL_MIN_DOT_PX
        })
        .map(|(slot, node)| {
            (
                std::cmp::Reverse(node.degree),
                std::cmp::Reverse(projection.alpha(slot).to_be_bytes()),
                slot,
            )
        })
        .collect();
    ranked.sort();
    let mut chosen: Vec<usize> = ranked.into_iter().map(|(_, _, slot)| slot).collect();
    chosen.truncate(LABEL_CAP_3D);
    // The hovered node is named whatever the budget and the size gate decided, in this
    // space especially: the node under the pointer may be a small, far one, and a ring
    // around an unnamed dot is a question with no answer.
    for slot in hover.into_iter().chain(centre) {
        if !chosen.contains(&slot) {
            chosen.push(slot);
        }
    }
    chosen
}

/// How much breathing room two labels must have between them, in screen px.
const LABEL_PAD: f32 = 3.0;

/// The labels already placed this frame, as screen boxes.
///
/// The cap is a ceiling, not the mechanism: in a bunched cloud every eligible node sits
/// in the same band, so a budget of 120 names 120 overlapping words and the picture
/// reads as texture. What actually keeps a graph readable is **naming what fits** — a
/// candidate is drawn only if its box clears every box already taken. Highest degree
/// still goes first (and the hover before all of them), so the names that survive are
/// the ones a person would have chosen anyway, and the ones that go were going to be
/// unreadable regardless.
#[derive(Default)]
struct Placed {
    boxes: Vec<[f32; 4]>,
}

impl Placed {
    /// Whether `candidate` (x0, y0, x1, y1) clears everything taken, by [`LABEL_PAD`].
    fn fits(&self, candidate: [f32; 4]) -> bool {
        self.boxes.iter().all(|taken| {
            candidate[2] + LABEL_PAD <= taken[0]
                || taken[2] + LABEL_PAD <= candidate[0]
                || candidate[3] + LABEL_PAD <= taken[1]
                || taken[3] + LABEL_PAD <= candidate[1]
        })
    }

    fn add(&mut self, candidate: [f32; 4]) {
        self.boxes.push(candidate);
    }
}

/// Put the nodes named last frame in front of the ones that were not, keeping the order
/// they were named in. This is the whole of the label hysteresis: greedy placement
/// otherwise contests the same real estate afresh every tick, so two labels a few px
/// apart trade the spot back and forth and the picture pops. A remembered label still
/// has to fit, and the ranking still decides among newcomers, so the set changes when
/// something actually moves into a name's space — not every tick.
fn remembered_first(mut chosen: Vec<usize>, remembered: &[usize]) -> Vec<usize> {
    if remembered.is_empty() {
        return chosen;
    }
    let rank: HashMap<usize, usize> = remembered
        .iter()
        .enumerate()
        .map(|(order, slot)| (*slot, order))
        .collect();
    chosen.sort_by_key(|slot| rank.get(slot).copied().unwrap_or(usize::MAX));
    chosen
}

#[allow(clippy::too_many_arguments)] // likewise: the label pass reads the whole frame
fn paint_labels3d(
    frame: &GraphFrame,
    projection: &Projection,
    labels: &Labels,
    palette: &Palette,
    bounds: Bounds<Pixels>,
    hover: Option<usize>,
    centre: Option<usize>,
    label_memory: &RefCell<Vec<usize>>,
    window: &mut Window,
    cx: &mut App,
) {
    let mut chosen = label_slots(frame, projection, bounds, hover, centre);
    // Hysteresis, before anything else: the nodes this view named last frame get first
    // claim on the space, in the order they were named. Greedy placement without a
    // memory of the previous frame contests the same real estate afresh every tick, and
    // two labels a few px apart trade the spot back and forth — the picture pops. The
    // ranking still decides among *newcomers*, and a remembered label still has to fit,
    // so this is stability at the margin rather than a name pinned to a stale spot.
    let remembered = label_memory.borrow();
    chosen = remembered_first(chosen, &remembered);
    drop(remembered);
    // The hovered label is placed **first**, so it keeps its space rather than being
    // skipped because a hub happened to be measured before it — and the centre's right
    // behind it, for the same reason.
    let mut marks: Vec<usize> = Vec::new();
    for slot in hover.into_iter().chain(centre) {
        if !marks.contains(&slot) {
            marks.push(slot);
        }
    }
    for (rank, slot) in marks.iter().enumerate().rev() {
        chosen.retain(|candidate| *candidate != *slot);
        chosen.insert(rank.min(chosen.len()), *slot);
    }
    let label_size = px(labels.size * LABEL_SCALE);
    let text_system = window.text_system().clone();
    let mut placed = Placed::default();
    let mut painted: Vec<usize> = Vec::new();
    for slot in chosen {
        let Some(node) = frame.nodes.get(slot) else {
            continue;
        };
        let Some(center) = projection.screen(slot) else {
            continue;
        };
        let r = projection.radius(frame, slot);
        let label = node.label.clone();
        let runs = label_runs(palette, labels, &label);
        let shaped = text_system.shape_line(label, label_size, &runs, None);
        let origin = point(
            px(center[0] - shaped.width.as_f32() / 2.0),
            px(center[1] + r + labels.size),
        );
        let box_ = [
            origin.x.as_f32(),
            origin.y.as_f32(),
            origin.x.as_f32() + shaped.width.as_f32(),
            origin.y.as_f32() + labels.size,
        ];
        if !placed.fits(box_) {
            continue;
        }
        placed.add(box_);
        painted.push(slot);
        if let Err(err) = shaped.paint(
            origin,
            px(labels.size),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        ) {
            tracing::warn!("a graph label could not be painted: {err:#}");
            break;
        }
    }
    *label_memory.borrow_mut() = painted;
}

/// Straight-line distance in screen px.
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::super::frame::GraphNode;
    use super::*;
    use gpui::SharedString;

    /// A crowded frame: `n` nodes packed 12 px apart on a shallow depth, every one of
    /// them big enough to name — the bunched middle of a projected cloud, which is the
    /// case the cap exists for. Each node carries a different degree, so *who* survives
    /// the cut is checkable, not just how many.
    fn crowded_frame(n: usize) -> GraphFrame {
        let nodes = (0..n)
            .map(|i| GraphNode {
                position: [(i % 40) as f32 * 12.0, (i / 40) as f32 * 12.0],
                depth: (i % 7) as f32 * 9.0,
                radius: 4.0,
                kind: NodeKind::Note,
                degree: i as u32,
                label: SharedString::from(format!("n{i}")),
                path: None,
            })
            .collect();
        GraphFrame {
            nodes,
            edges: Vec::new(),
            grid: Grid::default(),
            bounds: None,
        }
    }

    /// A canvas big enough to hold that frame's projection. The camera looks down -z
    /// with screen y growing down, so a layout spread over positive world y projects to
    /// negative screen y — the box has to cover both.
    fn canvas() -> Bounds<Pixels> {
        Bounds::new(point(px(-800.0), px(-800.0)), size(px(1600.0), px(1600.0)))
    }

    /// The projected space names fewer nodes than the flat one, because a third axis
    /// seen from outside folds depth into the same screen area and the labels pile up.
    /// The cut is a vocabulary, not a sample: the hubs survive, the leaves go.
    #[test]
    fn label_slots_respects_the_cap() {
        let extra = 80;
        let frame = crowded_frame(LABEL_CAP_3D + extra);
        let projection = Projection::build(&frame, &Cam3::new());

        let chosen = label_slots(&frame, &projection, canvas(), None, None);
        assert_eq!(
            chosen.len(),
            LABEL_CAP_3D,
            "every node was eligible, so the cap is what answered"
        );
        let kept_lowest = chosen
            .iter()
            .map(|slot| frame.nodes[*slot].degree)
            .min()
            .expect("the cap is not zero");
        assert_eq!(
            kept_lowest as usize, extra,
            "the cut kept the highest-degree nodes rather than the first ones"
        );
    }

    /// The node under the pointer is named whatever the cap and the size gate decided —
    /// in the projected space especially, where the hovered node can be a small far one
    /// and a ring around an unnamed dot is a question with no answer.
    #[test]
    fn the_hovered_node_is_always_named() {
        let frame = crowded_frame(LABEL_CAP_3D + 80);
        let projection = Projection::build(&frame, &Cam3::new());
        // The worst case for the gate: the lowest-degree node, drawn too small to name.
        let hovered = 0usize;
        assert!(
            projection.radius(&frame, hovered) >= LABEL_MIN_DOT_PX,
            "the fixture is the crowded one; the gate is what excludes it below"
        );

        let chosen = label_slots(&frame, &projection, canvas(), Some(hovered), None);
        assert!(
            chosen.contains(&hovered),
            "the hovered node was left out of the frame's {} names",
            chosen.len()
        );
        assert_eq!(
            chosen.len(),
            LABEL_CAP_3D + 1,
            "the hover joins the budget rather than replacing someone in it"
        );
    }

    /// The cap is a ceiling, not the mechanism: in a bunched cloud every eligible node
    /// sits in the same band, so the budget names a pile of overlapping words. What
    /// keeps the picture readable is naming what *fits*.
    #[test]
    fn a_label_that_would_land_on_another_is_skipped() {
        let mut placed = Placed::default();
        assert!(
            placed.fits([0.0, 0.0, 60.0, 12.0]),
            "an empty canvas takes anything"
        );
        placed.add([0.0, 0.0, 60.0, 12.0]);
        assert!(
            !placed.fits([10.0, 2.0, 70.0, 14.0]),
            "a label landing on one already drawn is skipped"
        );
        assert!(
            placed.fits([70.0, 0.0, 130.0, 12.0]),
            "the same box moved clear of it is drawn"
        );
        assert!(
            !placed.fits([62.0, 0.0, 122.0, 12.0]),
            "and the gap has to clear the padding, not just touch"
        );
        assert!(
            placed.fits([0.0, 40.0, 60.0, 52.0]),
            "below it, clear vertically, is a different line of text"
        );
    }

    /// Greedy placement contests the same real estate afresh every tick unless it
    /// remembers what it named last frame. The remembered labels take first claim, in
    /// the order they were named; everyone else keeps the ranking behind them.
    #[test]
    fn the_labels_named_last_frame_keep_first_claim() {
        let ordered = remembered_first(vec![3usize, 1, 7, 9], &[9, 3]);
        assert_eq!(
            ordered,
            vec![9, 3, 1, 7],
            "remembered first and in remembered order, the rest in rank order"
        );
        assert_eq!(
            remembered_first(vec![1, 2], &[]),
            vec![1, 2],
            "with no memory the ranking stands as built"
        );
    }

    /// And when the node is too small for the gate at all, the hover still answers.
    #[test]
    fn a_hovered_node_below_the_size_gate_is_still_named() {
        let mut frame = crowded_frame(40);
        frame.nodes[7].radius = 0.01; // a dot too small to name by the gate
        let projection = Projection::build(&frame, &Cam3::new());
        assert!(
            projection.radius(&frame, 7) < LABEL_MIN_DOT_PX,
            "the fixture must actually be below the gate"
        );

        let plain = label_slots(&frame, &projection, canvas(), None, None);
        assert!(
            !plain.contains(&7),
            "the gate excludes it for an ordinary frame"
        );
        let hovered = label_slots(&frame, &projection, canvas(), Some(7), None);
        assert!(hovered.contains(&7), "but the hover names it anyway");
    }
}
