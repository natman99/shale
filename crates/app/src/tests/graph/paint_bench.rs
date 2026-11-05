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
    canvas, div, point, prelude::*, px, Bounds, Context, Font, IntoElement, PathBuilder, Pixels,
    Render, TestAppContext, VisualTestContext, Window, WindowHandle,
};
use lyon::math::point as lpoint;
use lyon::tessellation::VertexBuffers;
use shale_core::graph::{geom::Camera, GraphIndex, NodeKind};
use shale_core::theme::Palette;
use shale_core::vault::Vault;

use crate::graph::render::{
    paint_hover_links, paint_labels, paint_links, paint_nodes, stroke_quad, Cache, Geometry, Key,
    Labels, Viewport, CULL_MARGIN, LINK_MIN_PX, LINK_WIDTH,
};
use crate::graph::{paint_graph3d, Dim, GraphFrame, GraphSim, Projection};

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

/// Measurements per cell, of which the **fastest** is the one printed. The same code
/// measured twice on this machine has moved by 20%, which is more than several of the
/// differences this table is read for; the spread is the scheduler, and the floor is the
/// paint.
const ROUNDS: usize = 3;

/// Frames drawn before timing, so the allocator and the font caches are warm.
const WARMUP: usize = 5;

// ----- the view under test --------------------------------------------------

/// Which phases the bench view paints.
const LINKS: usize = 1;
/// The chosen-node links, which is a second pass over the edge list.
const HOVER_LINKS: usize = 2;
const NODES: usize = 4;
const LABELS: usize = 8;
/// The same frame with each phase inside its own `Window::paint_layer`, which is what the
/// renderer does: it replaces the per-primitive bounds-tree search with one insert per phase.
/// The only difference between this row and `whole` is how the scene is charged for the
/// drawing — and one layer for the whole canvas is **not** this: gpui breaks a shared draw
/// order by primitive kind, so links would cover nodes.
const LAYER: usize = 16;
/// The nodes as one filled path per colour rather than one quad per node.
const NODES_PATH: usize = 32;
/// The links stroked by gpui's lyon-powered `PathBuilder::stroke` rather than tessellated
/// by hand by the renderer, which is the slower one (see [`paint_links_lyon`]).
const LYON_LINKS: usize = 64;
/// The links through the renderer's paint cache, which is kept across the frames of one
/// measurement. Without this flag every frame gets a cache of its own, which is the same
/// as having none: the point of the cache is the second paint of a picture that stopped
/// moving.
const CACHE: usize = 128;
/// The links built through lyon's indexed buffers, which is how the renderer used to build
/// them, before it wrote `gpui::Path` vertices itself.
const BUFFERS: usize = 256;
const ALL: usize = LINKS | HOVER_LINKS | NODES | LABELS;

/// What the bench canvas is drawing, handed from its prepaint to its paint: the flat
/// graph through the 2D camera, or the projected graph through the tab's 3D path.
enum BenchSpace {
    Flat(Camera),
    Deep(Rc<Projection>),
}

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
    /// The same, for the projected view (roadmap P3.6): fitted once and kept, so the
    /// measurement is the frame rather than the fit.
    fitted3: Rc<RefCell<Option<shale_core::graph::proj::Cam3>>>,
    /// Which space this window paints: flat, or projected with the tab's own 3D path.
    deep: bool,
    /// How much of the frame survives culling at this zoom — `(nodes, links)` — counted by
    /// the paint closure. The timings only mean anything next to these.
    counts: Rc<RefCell<(usize, usize)>>,
    /// How many link paints found a cached batch, over the frames since the last write.
    /// Printed beside the cached rows: a cache that is not hitting is not slow, it is
    /// absent, and the timing alone cannot tell the two apart.
    hits: Rc<RefCell<usize>>,
    /// Which phases to run this round.
    phases: usize,
    /// How much bigger than the fitted whole-vault view to draw at.
    zoom: f32,
    /// The renderer's link cache, kept for the whole measurement so the cached rows get
    /// their second and third paints of the same picture.
    paint: Rc<RefCell<Cache>>,
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
        let hits = self.hits.clone();
        let fitted = self.fitted.clone();
        let fitted3 = self.fitted3.clone();
        let deep = self.deep;
        let palette = self.palette.clone();
        let labels = Labels {
            font: self.labels.font.clone(),
            size: self.labels.size,
        };
        let phases = self.phases;
        let zoom = self.zoom;
        let paint_cache = self.paint.clone();
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
                    if deep {
                        let box_px = [
                            bounds.origin.x.as_f32(),
                            bounds.origin.y.as_f32(),
                            bounds.size.width.as_f32(),
                            bounds.size.height.as_f32(),
                        ];
                        // Copied out before the match: `Cam3` is `Copy`, and holding a
                        // `borrow()` across the arm that writes the cell is the oldest
                        // `RefCell` trap there is.
                        let fitted = *fitted3.borrow();
                        let camera = match fitted {
                            Some(camera) => camera,
                            None => {
                                let positions: Vec<[f32; 3]> = fit_frame
                                    .nodes
                                    .iter()
                                    .map(|node| {
                                        [node.position[0], node.position[1], node.depth]
                                    })
                                    .collect();
                                let mut camera = shale_core::graph::proj::Cam3::new();
                                camera.fit(&positions, box_px, FIT_MARGIN);
                                if zoom != 1.0 {
                                    camera.zoom_at(
                                        [box_px[0] + box_px[2] / 2.0, box_px[1] + box_px[3] / 2.0],
                                        zoom,
                                    );
                                }
                                *fitted3.borrow_mut() = Some(camera);
                                camera
                            }
                        };
                        // One projection per frame, exactly as the tab builds it — so the
                        // projection's own walk over the nodes is in the measurement
                        // rather than pretending not to be.
                        return BenchSpace::Deep(Rc::new(Projection::build(&fit_frame, &camera)));
                    }
                    if let Some(camera) = fitted.borrow().as_ref() {
                        return BenchSpace::Flat(*camera);
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
                    BenchSpace::Flat(camera)
                },
                move |bounds, space, window, cx| {
                    // The projected frame, painted by the tab's own path: the same four
                    // layers, the same cache, the same cull. The phase flags do not apply
                    // to it — the tab draws all four every frame — but `CACHE` does, which
                    // is the row that says what the link cache is worth when the camera is
                    // still (roadmap P3.6).
                    let camera = match space {
                        BenchSpace::Deep(projection) => {
                            {
                                let lo_x = bounds.origin.x.as_f32();
                                let lo_y = bounds.origin.y.as_f32();
                                let hi_x = lo_x + bounds.size.width.as_f32();
                                let hi_y = lo_y + bounds.size.height.as_f32();
                                let on = |at: [f32; 2]| {
                                    at[0] >= lo_x && at[0] <= hi_x && at[1] >= lo_y && at[1] <= hi_y
                                };
                                let nodes = (0..paint_frame.nodes.len())
                                    .filter(|slot| projection.screen(*slot).is_some_and(on))
                                    .count();
                                let links = paint_frame
                                    .edges
                                    .iter()
                                    .filter(|&&[a, b]| {
                                        projection.screen(a as usize).is_some_and(on)
                                            && projection.screen(b as usize).is_some_and(on)
                                    })
                                    .count();
                                *counts.borrow_mut() = (nodes, links);
                            }
                            let look = crate::graph::render::Look {
                                palette: palette.clone(),
                                hover: Some(paint_frame.nodes.len() / 2),
                                labels: Labels {
                                    font: labels.font.clone(),
                                    size: labels.size,
                                },
                            };
                            if phases & CACHE != 0 {
                                let key = Key::new(
                                    &paint_frame,
                                    Geometry::Deep(projection.camera()),
                                    bounds,
                                );
                                if paint_cache.borrow().links(&key).is_some() {
                                    *hits.borrow_mut() += 1;
                                }
                                paint_graph3d(
                                    bounds,
                                    &paint_frame,
                                    &projection,
                                    &look,
                                    &mut paint_cache.borrow_mut(),
                                    window,
                                    cx,
                                );
                            } else {
                                paint_graph3d(
                                    bounds,
                                    &paint_frame,
                                    &projection,
                                    &look,
                                    &mut Cache::new(),
                                    window,
                                    cx,
                                );
                            }
                            return;
                        }
                        BenchSpace::Flat(camera) => camera,
                    };
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
                    let layered = phases & LAYER != 0;
                    let view = Viewport::new(bounds, &camera);
                    if phases & LINKS != 0 {
                        phase(window, bounds, layered, |window| {
                            if phases & BUFFERS != 0 {
                                paint_links_buffers(
                                    &paint_frame,
                                    &camera,
                                    &view,
                                    palette.graph_edge,
                                    window,
                                );
                                return;
                            }
                            let key = Key::new(
                                &paint_frame,
                                crate::graph::render::Geometry::Flat(camera),
                                bounds,
                            );
                            // A cache per frame is the same as no cache, and says so: the
                            // cached rows are the ones that keep one across the frames.
                            let cached = paint_cache.borrow().links(&key).is_some();
                            if cached {
                                *hits.borrow_mut() += 1;
                            }
                            if phases & CACHE != 0 {
                                paint_links(
                                    &mut paint_cache.borrow_mut(),
                                    &key,
                                    &paint_frame,
                                    &camera,
                                    &view,
                                    palette.graph_edge,
                                    window,
                                );
                            } else {
                                paint_links(
                                    &mut Cache::new(),
                                    &key,
                                    &paint_frame,
                                    &camera,
                                    &view,
                                    palette.graph_edge,
                                    window,
                                );
                            }
                        });
                    }
                    if phases & LYON_LINKS != 0 {
                        phase(window, bounds, layered, |window| {
                            paint_links_lyon(
                                &paint_frame,
                                &camera,
                                &view,
                                palette.graph_edge,
                                window,
                            );
                        });
                    }
                    if phases & HOVER_LINKS != 0 {
                        // The middle node stands in for a hub: the pass is over the whole
                        // edge list either way, and that is what costs.
                        let slot = paint_frame.nodes.len() / 2;
                        phase(window, bounds, layered, |window| {
                            paint_hover_links(
                                &paint_frame,
                                &camera,
                                &view,
                                slot,
                                palette.graph_node_active,
                                window,
                            );
                        });
                    }
                    if phases & NODES != 0 {
                        phase(window, bounds, layered, |window| {
                            paint_nodes(
                                &paint_frame,
                                &camera,
                                &view,
                                palette.as_ref(),
                                None,
                                window,
                            );
                        });
                    }
                    if phases & NODES_PATH != 0 {
                        phase(window, bounds, layered, |window| {
                            paint_nodes_path(
                                &paint_frame,
                                &camera,
                                &view,
                                palette.as_ref(),
                                window,
                            );
                        });
                    }
                    if phases & LABELS != 0 {
                        phase(window, bounds, layered, |window| {
                            paint_labels(
                                &paint_frame,
                                &camera,
                                &view,
                                &labels,
                                palette.as_ref(),
                                // No hover: this row measures the frame's own budget, and
                                // the hovered label is one extra line of text either way.
                                None,
                                window,
                                cx,
                            );
                        });
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

/// Runs one phase, in a [`gpui::Window::paint_layer`] of its own when the row asked for one.
///
/// Per phase, never one layer over the canvas: gpui orders the scene by `(order, kind)`, so
/// primitives sharing one draw order are sequenced by *kind* — every quad before every path —
/// and a single layer over the graph put every link on top of every node. The renderer layers
/// every frame now; the unlayered row stays so the cost of the layers is still measurable.
fn phase(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    layered: bool,
    paint: impl FnOnce(&mut Window),
) {
    if layered {
        window.paint_layer(bounds, |window| paint(window));
    } else {
        paint(window);
    }
}

/// A settled layout over the vault at `root`, built the way the tab builds it: index, sim,
/// driven to rest, then the frame the tab would be handed.
fn settled_frame(root: &std::path::Path, dim: Dim) -> GraphFrame {
    let vault = Vault::open(root).expect("the fixture is a vault");
    let notes = vault.list_markdown().expect("listing the vault");
    let attachments = vault.list_attachments().expect("listing the vault");
    let index = GraphIndex::build(&vault, &notes, &attachments).expect("building the index");
    let visible = vec![true; index.graph().node_count()];
    let mut sim = GraphSim::build(&index, None, &visible, dim);
    // The layout is measured at rest: a settling graph is a different workload (it also
    // pays for the sim), and the exit criterion is about holding 60 fps while it moves,
    // which the beat's own tick budget covers.
    sim.advance(4000);
    sim.frame()
}

/// A window that paints one graph frame at `zoom` with `phases`, and the handles on what
/// it counts. Split out of [`measure`] so a comparison can hold one window open and change
/// its mind between rounds.
struct BenchWindow {
    /// The view, for the repaints and for changing its phases.
    window: WindowHandle<PaintBench>,
    /// The context that draws it.
    visual: VisualTestContext,
    /// What survived culling, as `(nodes, links)`.
    counts: Rc<RefCell<(usize, usize)>>,
    /// How many link paints found a cached batch.
    hits: Rc<RefCell<usize>>,
}

fn bench_window(
    cx: &mut TestAppContext,
    frame: &Rc<GraphFrame>,
    zoom: f32,
    phases: usize,
    deep: bool,
) -> BenchWindow {
    let view = PaintBench {
        frame: frame.clone(),
        palette: Rc::new(Palette::built_in("mocha").expect("mocha is built in")),
        labels: Labels {
            font: Font::default(),
            size: 14.0,
        },
        fitted: Rc::new(RefCell::new(None)),
        fitted3: Rc::default(),
        deep,
        counts: Rc::new(RefCell::new((0, 0))),
        hits: Rc::new(RefCell::new(0)),
        phases,
        zoom,
        paint: Rc::new(RefCell::new(Cache::new())),
    };
    let counts = view.counts.clone();
    let hits = view.hits.clone();
    let window: WindowHandle<PaintBench> = cx.add_window(move |_window, _cx| view);
    // The window is drawn through a `VisualTestContext`, not through the window handle:
    // drawing renders the view, and rendering it from inside an update of it is a second
    // borrow of the entity being drawn.
    let visual = VisualTestContext::from_window(window.into(), cx);
    BenchWindow {
        window,
        visual,
        counts,
        hits,
    }
}

/// One (zoom, phase-set) cell of the sweep: how long a frame's paint phase takes, how many
/// primitives it makes, and how often the link cache answered.
fn measure(
    cx: &mut TestAppContext,
    frame: &Rc<GraphFrame>,
    zoom: f32,
    phases: usize,
    deep: bool,
) -> (Duration, usize, (usize, usize), usize) {
    let mut bench = bench_window(cx, frame, zoom, phases, deep);

    for _ in 0..WARMUP {
        repaint(&bench.window, &mut bench.visual);
    }
    // The fastest round, not the average. This machine moves the same code by 20% between
    // runs, which is more than several of the results in this table, and the spread is the
    // scheduler: the floor is what it reports when it is not fighting for a core.
    let mut best = Duration::MAX;
    let mut hits = 0;
    for _ in 0..ROUNDS {
        *bench.hits.borrow_mut() = 0;
        let started = Instant::now();
        for _ in 0..REPS {
            repaint(&bench.window, &mut bench.visual);
        }
        let per_frame = started.elapsed() / REPS as u32;
        if per_frame < best {
            best = per_frame;
            hits = *bench.hits.borrow();
        }
    }
    let quads = bench
        .visual
        .update(|window, _cx| window.painted_quads().len());
    let drawn = *bench.counts.borrow();
    (best, quads, drawn, hits)
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
///
/// **Read a row, not a difference between rows.** The same code measured in two rows of one
/// sweep has come out 20% apart on this machine, which is more than most of the differences
/// the table looks like it is reporting; [`graph_paint_cache_pays_for_itself`] is the one
/// that alternates the variants in one window and can be trusted to compare them.
fn sweep(cx: &mut TestAppContext, root: &std::path::Path) {
    let frame = Rc::new(settled_frame(root, Dim::Two));
    eprintln!(
        "\n{} — {} nodes, {} links",
        root.display(),
        frame.nodes.len(),
        frame.edges.len()
    );
    eprintln!(
        "  {:>7}  {:>12}  {:>10}  {:>13}  {:>7}",
        "zoom", "per frame", "quads", "drawn", "cache"
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
            // The same phases as the rows above, in a layer of their own, which is what the
            // renderer does. A phase's cost is not the same in both: the bounds-tree search an
            // unlayered primitive pays for its draw order is charged to the phase that pays
            // it, and a full-canvas path pays a great deal more for it than a disc does.
            ("lnk+lay", LINKS | LAYER),
            ("hov+lay", HOVER_LINKS | LAYER),
            ("nds+lay", NODES | LAYER),
            ("lbl+lay", LABELS | LAYER),
            ("whole", ALL),
            ("whole+lay", ALL | LAYER),
            // The frame the tab draws when nothing in it moved: the same picture painted
            // again, which is what a hover change or another panel's `notify` asks for.
            ("whole+cache", ALL | LAYER | CACHE),
            ("lnk+cache", LINKS | LAYER | CACHE),
            (
                "path+lay",
                LINKS | HOVER_LINKS | NODES_PATH | LABELS | LAYER,
            ),
        ] {
            let (per_frame, quads, (nodes, links), hits) = measure(cx, &frame, zoom, phases, false);
            eprintln!(
                "{zoom:>5.1}x {name:>10}  {:>10.3} ms  {:>10}  {:>6}n {:>5}l  {:>4}",
                per_frame.as_secs_f64() * 1000.0,
                quads,
                nodes,
                links,
                hits
            );
        }
    }
}

/// The same sweep for the projected view (roadmap P3.6): the frame the 3D tab draws, at
/// the same zooms, so the two tables can be read against each other.
///
/// Two rows rather than the flat table's seventeen. The phase-by-phase breakdown exists to
/// find a slow phase, and the projected frame has the same four phases as the flat one;
/// what is new is the projection and the depth sort, which are inside the whole-frame
/// number, and the link cache keyed on a **camera that rotates**, which is what the
/// `cache` column reports: it answers every frame while the camera is still, and never
/// while it moves, which is the honest cost of turning the graph.
fn sweep3d(cx: &mut TestAppContext, root: &std::path::Path) {
    let frame = Rc::new(settled_frame(root, Dim::Three));
    eprintln!(
        "\n{} — {} nodes, {} links, projected 3D",
        root.display(),
        frame.nodes.len(),
        frame.edges.len()
    );
    eprintln!(
        "  {:>7}  {:>12}  {:>10}  {:>13}  {:>7}",
        "zoom", "per frame", "quads", "drawn", "cache"
    );
    for zoom in ZOOMS {
        for (name, phases) in [("whole", ALL), ("whole+cache", ALL | CACHE)] {
            let (per_frame, quads, (nodes, links), hits) = measure(cx, &frame, zoom, phases, true);
            eprintln!(
                "{zoom:>5.1}x {name:>10}  {:>10.3} ms  {:>10}  {:>6}n {:>5}l  {:>4}",
                per_frame.as_secs_f64() * 1000.0,
                quads,
                nodes,
                links,
                hits
            );
        }
    }
}

/// The 5k fixture: 5261 nodes and ~19k links, which is the size the user's vault is at.
#[gpui::test]
#[ignore]
fn graph_paint_bench_5k(cx: &mut TestAppContext) {
    sweep(cx, &fixture("5k"));
    sweep3d(cx, &fixture("5k"));
}

/// The 1k fixture, for telling "cost per node" from "cost per link".
#[gpui::test]
#[ignore]
fn graph_paint_bench_1k(cx: &mut TestAppContext) {
    sweep(cx, &fixture("1k"));
    sweep3d(cx, &fixture("1k"));
}

/// The cache question, measured the only way this machine allows: one window, one picture,
/// and the variants **alternating round by round**, so whatever the machine is doing to its
/// own speed lands on every side of the comparison. Rows of a sweep are not comparable with
/// each other here — the same code measured in two rows of one sweep has come out 20% apart,
/// which is more than most of what this table is read for.
///
/// Three variants of the same links: the paths the renderer writes today, the lyon-indexed
/// buffers it wrote before, and the cache replaying the paths it already wrote.
#[gpui::test]
#[ignore]
fn graph_paint_cache_pays_for_itself(cx: &mut TestAppContext) {
    let frame = Rc::new(settled_frame(&fixture("5k"), Dim::Two));
    eprintln!(
        "\ncache A/B — {} nodes, {} links — {} rounds of {} frames, fastest kept",
        frame.nodes.len(),
        frame.edges.len(),
        ROUNDS,
        REPS
    );
    for zoom in ZOOMS {
        for (name, phases) in [("links", LINKS | LAYER), ("whole", ALL | LAYER)] {
            let mut bench = bench_window(cx, &frame, zoom, phases, false);
            for _ in 0..WARMUP {
                repaint(&bench.window, &mut bench.visual);
            }
            let mut plain = Duration::MAX;
            let mut buffers = Duration::MAX;
            let mut cached = Duration::MAX;
            let mut hits = 0;
            for _ in 0..ROUNDS {
                for (extra, best) in [
                    (0usize, &mut plain),
                    (BUFFERS, &mut buffers),
                    (CACHE, &mut cached),
                ] {
                    bench
                        .window
                        .update(&mut bench.visual, |view, _window, _cx| {
                            view.phases = phases | extra;
                        })
                        .expect("the bench window is open");
                    *bench.hits.borrow_mut() = 0;
                    let started = Instant::now();
                    for _ in 0..REPS {
                        repaint(&bench.window, &mut bench.visual);
                    }
                    let per_frame = started.elapsed() / REPS as u32;
                    let beat = per_frame < *best;
                    if beat {
                        *best = per_frame;
                    }
                    // Only the cached variant can hit, and the number worth printing is
                    // the one from the round that produced the timing being printed.
                    if beat && extra & CACHE != 0 {
                        hits = *bench.hits.borrow();
                    }
                }
            }
            eprintln!(
                "{zoom:>5.1}x {name:>6}  path {:>7.3} ms  buffers {:>7.3} ms  cached {:>7.3} ms  \
                 {hits:>4} hits",
                plain.as_secs_f64() * 1000.0,
                buffers.as_secs_f64() * 1000.0,
                cached.as_secs_f64() * 1000.0,
            );
        }
    }
}

/// The fixtures live beside the vaults, not under the crate.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/graph")
        .join(name)
}

// ----- the mechanism the layering rests on ----------------------------------

/// A view that paints four quads: two in one layer, one in a layer opened afterwards, and one
/// with no layer at all.
struct LayerProbe;

impl Render for LayerProbe {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            canvas(
                move |bounds, _window, _cx| bounds,
                move |bounds, _prepaint, window, _cx| {
                    let quad = |color: u32| {
                        gpui::fill(
                            Bounds {
                                origin: bounds.origin,
                                size: bounds.size,
                            },
                            gpui::rgb(color),
                        )
                    };
                    window.paint_layer(bounds, |window| {
                        window.paint_quad(quad(0x00_00_11));
                        window.paint_quad(quad(0x00_00_12));
                    });
                    window.paint_layer(bounds, |window| window.paint_quad(quad(0x00_00_21)));
                    window.paint_quad(quad(0x00_00_31));
                },
            )
            .size_full(),
        )
    }
}

/// **Why the renderer opens a layer per phase instead of one over the canvas.**
///
/// gpui sequences the scene by `(order, kind)` (`Scene::batches` → `BatchIterator`), so
/// primitives that share a draw order are drawn grouped by kind — every quad before every
/// path. One layer over the whole graph therefore drew every link (a path) on top of every
/// node (a quad): links covering the nodes they hang off. Layers opened in sequence get
/// increasing orders, because each one's bounds overlap the last and `BoundsTree::insert`
/// answers max-intersecting-order + 1 — which is the contract the per-phase layering depends
/// on, and gpui's own kind tiebreak is the reason it matters.
#[gpui::test]
fn layers_are_painted_in_the_order_they_were_opened(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| LayerProbe);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));

    let painted = visual.update(|window, _cx| window.painted_quads());
    let order_of = |color: u32| {
        let wanted = gpui::Background::from(gpui::rgb(color));
        painted
            .iter()
            .find(|quad| quad.background == wanted)
            .unwrap_or_else(|| panic!("the {color:#06x} quad was not painted"))
            .order
    };

    assert_eq!(
        order_of(0x00_00_11),
        order_of(0x00_00_12),
        "quads inside one layer share its order, which is why the kind tiebreak decides what \
         lands on top of what when they do"
    );
    assert!(
        order_of(0x00_00_11) < order_of(0x00_00_21),
        "a layer opened later is drawn later"
    );
    assert!(
        order_of(0x00_00_21) < order_of(0x00_00_31),
        "and a primitive painted after both layers, in no layer of its own, is above them"
    );
}

// ----- the alternatives the bench keeps honest -------------------------------

/// The links the way the renderer built them before it wrote `gpui::Path`s itself: the same
/// hand-computed quads, but into lyon's indexed buffers, which `PathBuilder::build_path`
/// then un-indexes one triangle at a time.
///
/// This is the previous version of the renderer's own code, kept so the change is a
/// measurement rather than a claim: it costs one allocation and one copy a batch more, and
/// the paired A/B below is what says whether that is what it costs.
fn paint_links_buffers(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    color: u32,
    window: &mut Window,
) {
    let half = (LINK_WIDTH * camera.zoom).max(LINK_MIN_PX) / 2.0;
    // Its own batch size, which is the one the renderer used with this route: the two
    // routes' batches are sized by different allocations, so sharing a constant would
    // retune the older one by accident.
    const BATCH: usize = 1_000;
    let mut buf = VertexBuffers::with_capacity(BATCH * 4, BATCH * 6);
    let mut drawn = 0usize;
    for &[a, b] in &frame.edges {
        let (Some(from), Some(to)) = (frame.nodes.get(a as usize), frame.nodes.get(b as usize))
        else {
            continue;
        };
        if view.link_misses(from.position, to.position) {
            continue;
        }
        let Some(quad) = stroke_quad(
            camera.to_screen(from.position),
            camera.to_screen(to.position),
            half,
        ) else {
            continue;
        };
        if drawn == BATCH {
            window.paint_path(
                PathBuilder::build_path(std::mem::take(&mut buf)),
                gpui::rgb(color),
            );
            buf = VertexBuffers::with_capacity(BATCH * 4, BATCH * 6);
            drawn = 0;
        }
        let base = buf.vertices.len() as u16;
        buf.vertices.extend_from_slice(&[
            lpoint(quad[0][0], quad[0][1]),
            lpoint(quad[1][0], quad[1][1]),
            lpoint(quad[2][0], quad[2][1]),
            lpoint(quad[3][0], quad[3][1]),
        ]);
        buf.indices
            .extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 2, base + 3]);
        drawn += 1;
    }
    if drawn > 0 {
        window.paint_path(PathBuilder::build_path(buf), gpui::rgb(color));
    }
}

/// The links stroked the way gpui asks for it: `PathBuilder::stroke`, and lyon's sweep line
/// over the whole edge list.
///
/// The renderer hand-builds the same quads instead (`graph::render::LinkPaths`), and this
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
