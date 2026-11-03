//! Timing the graph's paint path — roadmap P3.4's frame-rate exit criteria.
//!
//! Scaffolding, not a suite: every test here is `#[ignore]`d, because a wall-clock number
//! from one machine is not an assertion. Run it with
//!
//! ```text
//! cargo test -p shale-app --release -- --ignored --nocapture graph_paint
//! ```
//!
//! **Release, always.** A debug build of this measures 5 to 10 times the work, and the
//! fixed costs in it (lyon, the font shaping) do not scale the same way, so a debug number
//! is not a bigger version of a real one — it points at the wrong things.
//!
//! **What it measures** is the paint phase: the work Shale does per frame to turn a
//! settled layout into scene primitives. That is the same span the in-app `ms paint`
//! readout measures, and it is the half Shale can optimize — rasterizing the result is the
//! GPU's job and happens on another thread. The **primitive count** is printed beside every
//! number because it is what the scene and the GPU pay per frame, and unlike the timing it
//! does not move with the machine the bench runs on.
//!
//! **What it does not measure:** the rest of the frame (the file tree, the tab bar, the
//! editor). That is deliberate — the graph's cost is what this bench exists to move.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, point, prelude::*, px, Context, Font, IntoElement, PathBuilder, Render,
    TestAppContext, VisualTestContext, Window, WindowHandle,
};
use shale_core::graph::{geom::Camera, GraphIndex, NodeKind};
use shale_core::theme::Palette;
use shale_core::vault::Vault;

use crate::graph::render::{
    paint_hover_links, paint_labels, paint_links, paint_nodes, Labels, Viewport, CULL_MARGIN,
    LINK_MIN_PX, LINK_WIDTH,
};
use crate::graph::{GraphFrame, GraphSim};

/// The margin [`Camera::fit`] is given, matching what the graph tab asks for, so the bench
/// frames the layout the way the tab does.
const FIT_MARGIN: f32 = 24.0;

/// The zooms to measure, as multiples of the fitted whole-vault view: the whole vault, a
/// neighbourhood, and one note read up close. The user lives across this range, and the
/// phases do not cost the same at each end.
const ZOOMS: [f32; 3] = [1.0, 3.0, 10.0];

/// Frames per measurement. Enough to average out scheduler noise, short enough that the
/// whole sweep finishes before the user forgets what they were looking for.
const REPS: usize = 40;

/// Frames drawn before timing, so the allocator and the font caches are warm.
const WARMUP: usize = 5;

// ----- the view under test --------------------------------------------------

/// Which phases the bench view paints.
const LINKS: usize = 1;
/// The chosen-node links, which is a second pass over the edge list.
const HOVER_LINKS: usize = 2;
const NODES: usize = 4;
const LABELS: usize = 8;
/// The same frame with every paint call inside one `Window::paint_layer`, which is gpui's
/// way of saying "these primitives share a draw order": it replaces the per-primitive
/// bounds-tree insert with one insert for the batch. The only difference between this row
/// and `whole` is how the scene is charged for the drawing.
const LAYER: usize = 16;
/// The nodes as one filled path per colour rather than one quad per node.
const NODES_PATH: usize = 32;
/// The links stroked by gpui's lyon-powered `PathBuilder::stroke` rather than tessellated
/// by hand by the renderer, which is the slower one (see [`paint_links_lyon`]).
const LYON_LINKS: usize = 64;
const ALL: usize = LINKS | HOVER_LINKS | NODES | LABELS;

/// A view that paints one graph frame and nothing else, so the numbers are the graph's
/// rather than the sidebar's, the tab bar's and the editor's.
struct PaintBench {
    /// The settled layout. Shared with the canvas's paint closure, which is `'static`.
    frame: Rc<GraphFrame>,
    /// The colours, for the same reason.
    palette: Rc<Palette>,
    /// The labels' face and size.
    labels: Labels,
    /// The framing, worked out on the first frame and reused after it. Shared with the
    /// canvas's prepaint closure, which is `'static`.
    fitted: Rc<RefCell<Option<Camera>>>,
    /// How much of the frame survives culling at this zoom — `(nodes, links)` — counted by
    /// the paint closure. The timings only mean anything next to these.
    counts: Rc<RefCell<(usize, usize)>>,
    /// Which phases to run this round.
    phases: usize,
    /// How much bigger than the fitted whole-vault view to draw at.
    zoom: f32,
}

impl Render for PaintBench {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Sized from the window itself: a `size_full` div at the root of a test window
        // resolves to a height of 0 (its parent chain has a definite width and an
        // automatic height), and a zero-height canvas culls everything and measures
        // nothing.
        let size = window.viewport_size();
        // The two closures each take what they need: `canvas`'s are `FnOnce`, so an `Rc`
        // cannot be shared between them without cloning it.
        let fit_frame = self.frame.clone();
        let paint_frame = self.frame.clone();
        let counts = self.counts.clone();
        let fitted = self.fitted.clone();
        let palette = self.palette.clone();
        let labels = Labels {
            font: self.labels.font.clone(),
            size: self.labels.size,
        };
        let phases = self.phases;
        let zoom = self.zoom;
        div().w(size.width).h(size.height).child(
            canvas(
                // Framing happens in the prepaint phase, where the element's bounds are known,
                // and is handed to the paint phase — so the camera is not a cost the paint
                // measurement is pretending not to see, it is simply not part of it.
                move |bounds, _window, _cx| {
                    // Framed once and kept, which is what the graph tab does: it fits the
                    // whole graph on the first frames and then drives the camera the user
                    // set. Re-fitting every frame would measure a walk over every node that
                    // the live app does not pay.
                    if let Some(camera) = fitted.borrow().as_ref() {
                        return *camera;
                    }
                    let positions: Vec<[f32; 2]> =
                        fit_frame.nodes.iter().map(|node| node.position).collect();
                    let mut camera = Camera::fit(
                        &positions,
                        [
                            bounds.origin.x.as_f32(),
                            bounds.origin.y.as_f32(),
                            bounds.size.width.as_f32(),
                            bounds.size.height.as_f32(),
                        ],
                        FIT_MARGIN,
                    );
                    if zoom != 1.0 {
                        let center = [
                            bounds.origin.x.as_f32() + bounds.size.width.as_f32() / 2.0,
                            bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0,
                        ];
                        camera.zoom_at(center, zoom);
                    }
                    *fitted.borrow_mut() = Some(camera);
                    camera
                },
                move |bounds, camera, window, cx| {
                    // Counted every frame, whatever the phases asked for: the cost of a
                    // phase is the cost of what it draws, and a zoom that draws 1% of the
                    // vault is not comparable to one that draws all of it.
                    {
                        let view = Viewport::new(bounds, &camera);
                        let nodes = paint_frame
                            .nodes
                            .iter()
                            .filter(|node| view.holds(node, CULL_MARGIN))
                            .count();
                        let links = paint_frame
                            .edges
                            .iter()
                            .filter(|&&[a, b]| {
                                match (
                                    paint_frame.nodes.get(a as usize),
                                    paint_frame.nodes.get(b as usize),
                                ) {
                                    (Some(from), Some(to)) => {
                                        !view.link_misses(from.position, to.position)
                                    }
                                    _ => false,
                                }
                            })
                            .count();
                        *counts.borrow_mut() = (nodes, links);
                    }
                    let mut paint = |window: &mut Window| {
                        let view = Viewport::new(bounds, &camera);
                        if phases & LINKS != 0 {
                            paint_links(&paint_frame, &camera, &view, palette.graph_edge, window);
                        }
                        if phases & LYON_LINKS != 0 {
                            paint_links_lyon(
                                &paint_frame,
                                &camera,
                                &view,
                                palette.graph_edge,
                                window,
                            );
                        }
                        if phases & HOVER_LINKS != 0 {
                            // The middle node stands in for a hub: the pass is over the whole
                            // edge list either way, and that is what costs.
                            let slot = paint_frame.nodes.len() / 2;
                            paint_hover_links(
                                &paint_frame,
                                &camera,
                                &view,
                                slot,
                                palette.graph_node_active,
                                window,
                            );
                        }
                        if phases & NODES != 0 {
                            paint_nodes(
                                &paint_frame,
                                &camera,
                                &view,
                                palette.as_ref(),
                                None,
                                window,
                            );
                        }
                        if phases & NODES_PATH != 0 {
                            paint_nodes_path(
                                &paint_frame,
                                &camera,
                                &view,
                                palette.as_ref(),
                                window,
                            );
                        }
                        if phases & LABELS != 0 {
                            paint_labels(
                                &paint_frame,
                                &camera,
                                &view,
                                &labels,
                                palette.as_ref(),
                                window,
                                cx,
                            );
                        }
                    };
                    if phases & LAYER != 0 {
                        window.paint_layer(bounds, paint);
                    } else {
                        paint(window);
                    }
                },
            )
            // The canvas element has no intrinsic height, so it is told to fill the div
            // that was given the window's size: a canvas of height 0 culls every node and
            // measures nothing at all.
            .size_full(),
        )
    }
}

// ----- the measurement ------------------------------------------------------

/// A settled layout over the vault at `root`, built the way the tab builds it: index, sim,
/// driven to rest, then the frame the tab would be handed.
fn settled_frame(root: &std::path::Path) -> GraphFrame {
    let vault = Vault::open(root).expect("the fixture is a vault");
    let notes = vault.list_markdown().expect("listing the vault");
    let attachments = vault.list_attachments().expect("listing the vault");
    let index = GraphIndex::build(&vault, &notes, &attachments).expect("building the index");
    let visible = vec![true; index.graph().node_count()];
    let mut sim = GraphSim::build(&index, None, &visible);
    // The layout is measured at rest: a settling graph is a different workload (it also
    // pays for the sim), and the exit criterion is about holding 60 fps while it moves,
    // which the beat's own tick budget covers.
    sim.advance(4000);
    sim.frame()
}

/// One (zoom, phase-set) cell of the sweep: how long a frame's paint phase takes, and how
/// many primitives it makes.
fn measure(
    cx: &mut TestAppContext,
    frame: &Rc<GraphFrame>,
    zoom: f32,
    phases: usize,
) -> (Duration, usize, (usize, usize)) {
    let view = PaintBench {
        frame: frame.clone(),
        palette: Rc::new(Palette::built_in("mocha").expect("mocha is built in")),
        labels: Labels {
            font: Font::default(),
            size: 14.0,
        },
        fitted: Rc::new(RefCell::new(None)),
        counts: Rc::new(RefCell::new((0, 0))),
        phases,
        zoom,
    };
    let view_counts = view.counts.clone();
    let window: WindowHandle<PaintBench> = cx.add_window(move |_window, _cx| view);
    // The window is drawn through a `VisualTestContext`, not through the window handle:
    // drawing renders the view, and rendering it from inside an update of it is a second
    // borrow of the entity being drawn.
    let mut visual = VisualTestContext::from_window(window.into(), cx);

    for _ in 0..WARMUP {
        repaint(&window, &mut visual);
    }
    let started = Instant::now();
    for _ in 0..REPS {
        repaint(&window, &mut visual);
    }
    let per_frame = started.elapsed() / REPS as u32;
    let quads = visual.update(|window, _cx| window.painted_quads().len());
    let drawn = *view_counts.borrow();
    (per_frame, quads, drawn)
}

/// One full frame: the view is marked dirty, then the window is drawn and its scene thrown
/// away, which is what the other paint tests do.
fn repaint(window: &WindowHandle<PaintBench>, cx: &mut VisualTestContext) {
    window
        .update(cx, |_view, _window, cx| cx.notify())
        .expect("the bench window is open");
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// The sweep, printed as a table a human can read.
fn sweep(cx: &mut TestAppContext, root: &std::path::Path) {
    let frame = Rc::new(settled_frame(root));
    eprintln!(
        "\n{} — {} nodes, {} links",
        root.display(),
        frame.nodes.len(),
        frame.edges.len()
    );
    eprintln!(
        "  {:>7}  {:>12}  {:>10}  {:>13}",
        "zoom", "per frame", "quads", "drawn"
    );
    for zoom in ZOOMS {
        for (name, phases) in [
            ("none", 0),
            ("links", LINKS),
            ("lyon lnk", LYON_LINKS),
            ("hover", HOVER_LINKS),
            ("nodes", NODES),
            ("nodes+path", NODES_PATH),
            ("labels", LABELS),
            ("whole", ALL),
            ("whole+lay", ALL | LAYER),
            (
                "path+lay",
                LINKS | HOVER_LINKS | NODES_PATH | LABELS | LAYER,
            ),
        ] {
            let (per_frame, quads, (nodes, links)) = measure(cx, &frame, zoom, phases);
            eprintln!(
                "{zoom:>5.1}x {name:>7}  {:>10.3} ms  {:>10}  {:>6}n {:>5}l",
                per_frame.as_secs_f64() * 1000.0,
                quads,
                nodes,
                links
            );
        }
    }
}

/// The 5k fixture: 5261 nodes and ~19k links, which is the size the user's vault is at.
#[gpui::test]
#[ignore]
fn graph_paint_bench_5k(cx: &mut TestAppContext) {
    sweep(cx, &fixture("5k"));
}

/// The 1k fixture, for telling "cost per node" from "cost per link".
#[gpui::test]
#[ignore]
fn graph_paint_bench_1k(cx: &mut TestAppContext) {
    sweep(cx, &fixture("1k"));
}

/// The fixtures live beside the vaults, not under the crate.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/graph")
        .join(name)
}

// ----- the alternatives the bench keeps honest -------------------------------

/// The links stroked the way gpui asks for it: `PathBuilder::stroke`, and lyon's sweep line
/// over the whole edge list.
///
/// The renderer hand-builds the same quads instead (`graph::render::StrokeBatch`), and this
/// row is what that trade looks like: on the 5k vault, 7.9 ms of a frame against 5.6 at
/// whole-vault zoom and 2.6 against 1.2 at a neighbourhood's. Kept because a trade you
/// cannot re-measure is a guess.
fn paint_links_lyon(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    color: u32,
    window: &mut Window,
) {
    let width = (LINK_WIDTH * camera.zoom).max(LINK_MIN_PX);
    let mut path = PathBuilder::stroke(px(width));
    let mut spent = 0usize;
    let mut drew = false;
    for &[a, b] in &frame.edges {
        let (Some(from), Some(to)) = (frame.nodes.get(a as usize), frame.nodes.get(b as usize))
        else {
            continue;
        };
        if view.link_misses(from.position, to.position) {
            continue;
        }
        // Six vertices a segment, which is what the ceiling probe measures for a stroked
        // two-point subpath, and why this has to be flushed in batches at all.
        if spent + 6 > PATH_VERTEX_BUDGET {
            if drew {
                if let Ok(built) = path.build() {
                    window.paint_path(built, gpui::rgb(color));
                }
            }
            path = PathBuilder::stroke(px(width));
            spent = 0;
        }
        let a = camera.to_screen(from.position);
        let b = camera.to_screen(to.position);
        path.move_to(point(px(a[0]), px(a[1])));
        path.line_to(point(px(b[0]), px(b[1])));
        spent += 6;
        drew = true;
    }
    if drew {
        if let Ok(built) = path.build() {
            window.paint_path(built, gpui::rgb(color));
        }
    }
}

/// The headroom under gpui's 65535-vertex path ceiling that the two alternatives below fill
/// to. The renderer's own version of this is in [`crate::graph::render`].
const PATH_VERTEX_BUDGET: usize = 60_000;

/// What one filled disc costs in vertices, as a conservative upper bound (measured per
/// disc: 21 at r=0.25, 33 at r=1, 73 at r=4, 184 at r=12 — lyon subdivides arcs to a fixed
/// tolerance, so the real count grows far more slowly than this line).
fn disc_vertices(r: f32) -> usize {
    (40.0 + 13.0 * r).max(1.0) as usize
}

/// Appends one filled disc to `path` as its own contour: two half-arcs from the east point
/// back to it.
fn disc(path: &mut PathBuilder, center: [f32; 2], r: f32) {
    let y = px(center[1]);
    let radii = point(px(r), px(r));
    path.move_to(point(px(center[0] + r), y));
    path.arc_to(radii, px(0.), false, true, point(px(center[0] - r), y));
    path.arc_to(radii, px(0.), false, true, point(px(center[0] + r), y));
    path.close();
}

/// The nodes as one filled path per colour instead of one quad each.
///
/// The slower one, and by a distance: 25.8 ms against 11.8 on the 5k vault at whole-vault
/// zoom, because lyon tessellates every disc and gpui then copies the vertices into the
/// scene, which costs more than the bounds-tree search a path avoids. It is worth re-running
/// if the GPU ever becomes the limit, since a path is one primitive and four thousand discs
/// are four thousand.
fn paint_nodes_path(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    palette: &Palette,
    window: &mut Window,
) {
    for kind in [
        NodeKind::Note,
        NodeKind::Attachment,
        NodeKind::Tag,
        NodeKind::Ghost,
    ] {
        let fill = match kind {
            NodeKind::Note | NodeKind::Attachment => gpui::rgb(palette.graph_node),
            NodeKind::Tag => gpui::rgb(palette.tag),
            NodeKind::Ghost => gpui::rgb(palette.ui_dim),
        };
        let mut path = PathBuilder::fill();
        let mut spent = 0usize;
        let mut drew = false;
        for node in frame.nodes.iter().filter(|node| node.kind == kind) {
            if !view.holds(node, CULL_MARGIN) {
                continue;
            }
            let r = node.radius * camera.zoom;
            let cost = disc_vertices(r);
            if spent + cost > PATH_VERTEX_BUDGET {
                if drew {
                    if let Ok(built) = path.build() {
                        window.paint_path(built, fill);
                    }
                }
                path = PathBuilder::fill();
                spent = 0;
            }
            disc(&mut path, camera.to_screen(node.position), r);
            spent += cost;
            drew = true;
        }
        if drew {
            if let Ok(built) = path.build() {
                window.paint_path(built, fill);
            }
        }
    }
}

// ----- the path vertex ceiling ----------------------------------------------

/// gpui tessellates every path into lyon's `VertexBuffers<Point, u16>`, so a path that
/// needs more than 65535 vertices does not merely get slow: lyon returns
/// `TooManyVertices`, gpui's `build()` returns `Err`, and the whole path is not drawn.
/// On a 5k vault that is the difference between a graph and an empty pane, so the
/// renderer has to know what a path costs before it puts a whole vault in one.
///
/// This probe prints the exchange rate for both shapes the graph draws — a stroked
/// segment and a filled disc — and the point where the ceiling is hit.
#[test]
#[ignore]
fn the_path_vertex_ceiling() {
    eprintln!("\nstroke (one move_to + line_to each)");
    for width in [0.5, 2.0, 40.0] {
        for n in [1_000, 8_000, 10_922, 10_923, 18_843] {
            eprintln!(
                "  w={width:>5} {:>6} segments: {}",
                n,
                stroke_path(n, width)
                    .map(|v| format!("{v} vertices"))
                    .unwrap_or_else(|e| format!("ERR {e}"))
            );
        }
    }
    eprintln!("fill (one disc each)");
    for r in [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 12.0, 20.0, 32.0] {
        eprintln!(
            "  r={r:>5} {:>6} discs: {}",
            1_000,
            fill_path(1_000, r)
                .map(|v| format!("{:.1} per disc", v as f64 / 1000.0))
                .unwrap_or_else(|e| format!("ERR {e}"))
        );
    }
}

/// The tessellated vertex count for `n` separate stroked segments, or the error lyon gave.
fn stroke_path(n: usize, width: f32) -> Result<usize, String> {
    let mut path = PathBuilder::stroke(px(width));
    for i in 0..n {
        let x = ((i % 900) as f32) * 1.7;
        let y = ((i / 900) as f32) * 2.3;
        path.move_to(point(px(x), px(y)));
        path.line_to(point(px(x + 40.0), px(y + 17.0)));
    }
    path.build()
        .map(|path| path.vertices.len())
        .map_err(|e| format!("{e:#}"))
}

/// The tessellated vertex count for `n` separate filled discs, or the error lyon gave.
fn fill_path(n: usize, r: f32) -> Result<usize, String> {
    let mut path = PathBuilder::fill();
    for i in 0..n {
        let x = ((i % 900) as f32) * 1.7;
        let y = ((i / 900) as f32) * 2.3;
        disc(&mut path, [x, y], r);
    }
    path.build()
        .map(|path| path.vertices.len())
        .map_err(|e| format!("{e:#}"))
}
