//! Painting the graph: edges under nodes under labels (roadmap P3.4 Task 6).
//!
//! This is the one place in Shale that draws with the raw paint API rather than
//! laying out divs, and the reason is the shape of the thing: a few thousand dots
//! and lines whose positions come from a simulation, not from a layout pass. The
//! cost of that freedom is doing the three things a layout would have done —
//! culling to the viewport, grouping paint calls into colour runs, and deciding
//! what is too small to draw — by hand, here.
//!
//! Coordinates: the [`Camera`] maps world units to **window-absolute** pixels,
//! because it was fitted against the canvas's absolute bounds. The paint calls
//! take the same space, so there is no offset to remember.

use std::rc::Rc;

use gpui::{
    point, px, size, App, Bounds, ContentMask, Font, Path, PathVertex, Pixels, Point, TextAlign,
    TextRun, Window,
};
use shale_core::graph::geom::Camera;
use shale_core::graph::NodeKind;
use shale_core::theme::Palette;

pub(crate) use super::cache::{Cache, Geometry, Key};
use super::frame::{GraphFrame, GraphNode};

/// The most labels drawn in one frame, highest degree first. A 2k-node vault at
/// a zoom that shows all of it cannot label all of it, and a wall of overlapping
/// text is worse than a quiet graph.
pub(crate) const LABEL_CAP: usize = 300;

/// A label's *budget* ramps over this zoom range and labels stop entirely below it:
/// under ~0.45x the characters are smaller than the gap between two nodes, so the
/// labels cost more than they say.
const LABEL_FADE_FROM: f32 = 0.45;
const LABEL_FADE_FULL: f32 = 0.85;

/// The smallest the budget gets at the zoomed-out end of the fade band, as a fraction
/// of [`LABEL_CAP`]. The ramp is about how many nodes are worth naming at a zoom where
/// they cannot all be read, not about fading them out: a wall of text at full strength
/// and a wall of text at half strength are both unreadable, and the first is cheaper to
/// draw. (The labels used to come out near-black, which looked exactly like an opacity
/// bug and was not one — see [`label_runs`].)
const LABEL_CAP_FLOOR: f32 = 0.25;

/// A link's width in world units, so it thickens with zoom like the nodes do
/// rather than turning into a hairline at 0.2x.
///
/// `pub(crate)` so the paint bench's comparison painters ask for the same stroke the real
/// renderer does.
pub(crate) const LINK_WIDTH: f32 = 1.0;

/// The smallest a link is ever drawn, in screen px. Thinner than this and a link is a
/// fraction of a device pixel wide, which is where it stops being a line at all.
pub(crate) const LINK_MIN_PX: f32 = 0.5;

/// How far a node may hang off the viewport edge and still be drawn. A node half
/// in view is still a node the user can see.
pub(crate) const CULL_MARGIN: f32 = 2.0;

/// The face the node labels are drawn in. Bundled with the palette rather than passed
/// as three arguments because they always travel together, and a seventh argument to
/// a paint function is a call site nobody can read.
pub(crate) struct Labels {
    /// The family, which is the note's: a label should look like the title it names.
    pub(crate) font: Font,
    /// Its size in px, before [`LABEL_SCALE`].
    pub(crate) size: f32,
}

/// How much smaller a label is than the note's text, so the dots stay the subject.
pub(crate) const LABEL_SCALE: f32 = 0.8;

/// How much bigger than a node its hover ring is, in screen px. Two is enough to
/// read as a ring at any zoom and small enough not to swallow a hub.
pub(crate) const RING_PX: f32 = 2.0;
/// How much wider the centre note's halo is than a hover's, in screen px. The two marks
/// share a colour — both mean "the note in question" — so size is what keeps the
/// pointer's answer and the window's answer apart when they land on different dots.
pub(crate) const CENTRE_RING_PX: f32 = 3.0;

/// Everything about *how* a frame is painted, as opposed to what is in it: the
/// colours, the face the labels use, and which node (if any) the pointer or the
/// keyboard has chosen. Bundled because they always travel together, and a seventh
/// argument to a paint function is a call site nobody can read.
pub(crate) struct Look {
    /// The palette the graph is drawn with, from the window, so a theme change is one
    /// assignment rather than a rebuild.
    pub(crate) palette: Rc<Palette>,
    /// The chosen node, as an index into the frame: ringed, with its links drawn in
    /// the active colour. `None` is an ordinary frame.
    pub(crate) hover: Option<usize>,
    /// The note the **window** is showing, when the graph is a neighbourhood around it.
    /// A halo of its own, wider than the hover's, and its name is always drawn: a local
    /// graph that does not say which dot it is local *to* makes the user find the note
    /// they were just reading by its shape.
    pub(crate) centre: Option<usize>,
    /// The labels' face and size.
    pub(crate) labels: Labels,
}

/// Paint one graph frame into `bounds`, using `camera` for this frame and `look` for
/// its colours, labels and chosen node, reusing what [`Cache`] still holds from the last
/// paint of the same picture.
///
/// Called from the canvas element's paint phase, so it borrows nothing that outlives
/// the frame: the caller hands over an owned snapshot, a camera, the look, and the view's
/// cache.
///
/// Everything is painted behind a clip to `bounds`. The camera maps world units to
/// window-absolute pixels and the graph is routinely larger than its pane, so a link whose
/// ends are outside the pane used to be painted anyway — across the file tree and the tab
/// bar, which is what it looks like for a graph to bleed out of its pane.
pub(crate) fn paint_graph(
    bounds: Bounds<Pixels>,
    camera: Camera,
    frame: &Rc<GraphFrame>,
    look: &Look,
    cache: &mut Cache,
    window: &mut Window,
    cx: &mut App,
) {
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        paint_frame(bounds, camera, frame, look, cache, window, cx);
    });
}

/// The body of [`paint_graph`], behind the clip: work out what the canvas can see, then draw
/// links, the chosen node's links, nodes, labels — **each phase in its own
/// [`Window::paint_layer`]**.
///
/// The layer is what makes a big vault affordable. gpui gives every primitive a draw order by
/// searching the scene's bounds tree, that search is not linear in the primitive count, and a
/// layer replaces a whole phase's worth of searching with one insert: on the 5k vault, 14.8 ms
/// a frame at whole-vault zoom turned into 3.2, with 4768 discs costing 11.6 ms of searching
/// (2.4 µs each) where 1151 of the same discs cost 1.9 ms.
///
/// **One layer per phase, never one for the whole canvas**, because of how the scene reaches
/// the GPU: `BatchIterator` orders primitives by `(order, kind)`, so when everything shares a
/// single order the *kind* breaks the tie — and `PrimitiveKind` puts every quad before every
/// path. One layer over the graph drew every link on top of every node, links covering the
/// nodes they hang off. Separate layers get increasing orders (each overlaps the last, and
/// `BoundsTree::insert` answers max-intersecting-order + 1), so links stay under nodes stay
/// under labels, for four inserts a frame instead of one.
pub(crate) fn paint_frame(
    bounds: Bounds<Pixels>,
    camera: Camera,
    frame: &Rc<GraphFrame>,
    look: &Look,
    cache: &mut Cache,
    window: &mut Window,
    cx: &mut App,
) {
    let palette = look.palette.as_ref();
    let view = Viewport::new(bounds, &camera);
    let key = Key::new(frame, Geometry::Flat(camera), bounds);

    window.paint_layer(bounds, |window| {
        paint_links(
            cache,
            &key,
            frame,
            &camera,
            &view,
            palette.graph_edge,
            window,
        );
    });
    if let Some(slot) = look.hover {
        window.paint_layer(bounds, |window| {
            paint_hover_links(
                frame,
                &camera,
                &view,
                slot,
                palette.graph_node_active,
                window,
            );
        });
    }
    window.paint_layer(bounds, |window| {
        paint_nodes(
            frame,
            &camera,
            &view,
            palette,
            look.hover,
            look.centre,
            window,
        );
    });
    window.paint_layer(bounds, |window| {
        paint_labels(
            frame,
            &camera,
            &view,
            &look.labels,
            palette,
            look.hover,
            look.centre,
            window,
            cx,
        );
    });
}

/// The world rectangle the canvas can see. Everything outside it is skipped, and at a few
/// thousand nodes that test is what makes a frame cheap.
pub(crate) struct Viewport {
    lo_x: f32,
    hi_x: f32,
    lo_y: f32,
    hi_y: f32,
}

impl Viewport {
    /// The rectangle `bounds` (window px) maps to through `camera`. Min/max rather than
    /// assuming an order: nothing here promises the camera is not flipped.
    ///
    /// `pub(crate)` rather than private so the paint bench can time one phase on its own,
    /// which needs the same viewport the real frame paints into.
    pub(crate) fn new(bounds: Bounds<Pixels>, camera: &Camera) -> Self {
        let lo = camera.to_world([bounds.origin.x.as_f32(), bounds.origin.y.as_f32()]);
        let hi = camera.to_world([
            bounds.origin.x.as_f32() + bounds.size.width.as_f32(),
            bounds.origin.y.as_f32() + bounds.size.height.as_f32(),
        ]);
        Self {
            lo_x: lo[0].min(hi[0]),
            hi_x: lo[0].max(hi[0]),
            lo_y: lo[1].min(hi[1]),
            hi_y: lo[1].max(hi[1]),
        }
    }

    /// Whether any part of `node` (its world disc, grown by `margin`) falls inside. A node
    /// half in view is still a node the user can see.
    ///
    /// `pub(crate)` for the same reason as [`Self::new`]: the paint bench culls its own
    /// candidate primitives exactly as the real frame does.
    pub(crate) fn holds(&self, node: &GraphNode, margin: f32) -> bool {
        let [x, y] = node.position;
        let r = node.radius + margin;
        x + r >= self.lo_x && x - r <= self.hi_x && y + r >= self.lo_y && y - r <= self.hi_y
    }

    /// Whether a link's own bounding box misses the viewport entirely. Cheap reject: at a
    /// few thousand links this is the test that pays for the frame.
    ///
    /// The box is deliberately the whole test. The exact alternative — clip the segment
    /// itself against the pane (Liang-Barsky) — was measured on the 5k vault and lost: at
    /// 10x zoom it rejected 8% of the links (3262 boxes in view, 3003 segments actually
    /// crossing), because a hub's links are long and cross the pane from off-screen, and the
    /// four divisions it costs per edge were more than the drawing it saved. A graph is not a
    /// scene of short local segments, and the numbers say so.
    ///
    /// `pub(crate)` for the same reason as [`Self::new`]: the paint bench counts what
    /// survives culling, which is what its timings are the cost of.
    pub(crate) fn link_misses(&self, a: [f32; 2], b: [f32; 2]) -> bool {
        a[0].max(b[0]) < self.lo_x
            || a[0].min(b[0]) > self.hi_x
            || a[1].max(b[1]) < self.lo_y
            || a[1].min(b[1]) > self.hi_y
    }
}

/// Every visible link, in as few paths as gpui will allow. They share a colour, and one
/// path is one scene primitive rather than thousands — but a whole vault's links do not fit
/// in one path (see [`PATH_VERTEX_LIMIT`]), so this hands them over in batches.
///
/// The batches are kept in `cache` under `key`, and a paint whose key matches the held
/// batch replays it instead of tessellating: see [`Cache`] for what that buys and what it
/// does not.
pub(crate) fn paint_links(
    cache: &mut Cache,
    key: &Key,
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    color: u32,
    window: &mut Window,
) {
    let color = gpui::rgb(color);
    if let Some(paths) = cache.links(key) {
        for path in paths {
            window.paint_path(path.clone(), color);
        }
        return;
    }

    // A second paint for the same key means the picture stopped moving, and only then is
    // holding the batch worth the clone it costs.
    let keep = cache.worth_keeping(key);
    let width = px((LINK_WIDTH * camera.zoom).max(LINK_MIN_PX));
    let paths = tessellate_links(frame, camera, view, width);
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

/// The links that hang off the chosen node, in the active colour: the graph's answer to
/// "what connects to this?" without making the user trace lines by hand. Dimming
/// everything else instead would need a colour per link, and one path per colour is the
/// deal this renderer made.
///
/// Not cached: the hover slot is part of what decides this phase's geometry, so the cache
/// would be thrown away on every hover change — and the phase is 0.14 ms of a 3.4 ms frame.
pub(crate) fn paint_hover_links(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    slot: usize,
    color: u32,
    window: &mut Window,
) {
    let width = px((LINK_WIDTH * camera.zoom).max(LINK_MIN_PX));
    for path in tessellate_hover_links(frame, camera, view, slot, width) {
        window.paint_path(path, gpui::rgb(color));
    }
}

/// The nodes, grouped by fill so the paint calls come in runs. Each kind takes its own
/// palette role: a ghost is quieter than a note, because it is a hole in the vault rather
/// than a note, and an attachment is quieter in a different way, because it is a file you
/// cannot open as a note. A tag takes `graph_tag`, which is a theme's own answer rather
/// than the colour a `#tag` has in prose -- in several built-in families those are the same
/// colour every note node already is, which would make a tag invisible. (Colouring *notes*
/// by their tags is the colour-by-tag option, and it comes with the filter panel; the hues
/// for it are already in core.)
///
/// The chosen node gets a ring: a disc a little larger than its own, painted first so the
/// node's fill covers the middle of it.
///
/// One quad per node, which is more primitives than the links' one-path-per-colour and still
/// cheaper: the same nodes as filled paths cost 25.8 ms against 11.8 at whole-vault zoom,
/// because lyon tessellates every disc and gpui copies the vertices into the scene. The
/// comparison is a row in the paint bench, not dead code in here.
pub(crate) fn paint_nodes(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    palette: &Palette,
    hover: Option<usize>,
    centre: Option<usize>,
    window: &mut Window,
) {
    // The centre first, so the hover's narrower ring sits inside it when both land on
    // the same dot rather than fighting it for the same pixels.
    if let Some(slot) = centre {
        if let Some(node) = frame.nodes.get(slot) {
            if view.holds(node, CULL_MARGIN) {
                let center = camera.to_screen(node.position);
                let r = node.radius * camera.zoom + RING_PX + CENTRE_RING_PX;
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
        if let Some(node) = frame.nodes.get(slot) {
            if view.holds(node, CULL_MARGIN) {
                let center = camera.to_screen(node.position);
                let r = node.radius * camera.zoom + RING_PX;
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
    for kind in [
        NodeKind::Note,
        NodeKind::Attachment,
        NodeKind::Tag,
        NodeKind::Ghost,
    ] {
        let fill = match kind {
            NodeKind::Note => gpui::rgb(palette.graph_node),
            NodeKind::Attachment => gpui::rgb(palette.graph_attachment),
            NodeKind::Tag => gpui::rgb(palette.graph_tag),
            NodeKind::Ghost => gpui::rgb(palette.graph_ghost),
        };
        for node in frame.nodes.iter().filter(|node| node.kind == kind) {
            if !view.holds(node, CULL_MARGIN) {
                continue;
            }
            let center = camera.to_screen(node.position);
            let r = node.radius * camera.zoom;
            // A square whose corners are rounded to the half-width is a circle:
            // gpui rasterises corner radii with an SDF, so this is a real disc.
            window.paint_quad(
                gpui::fill(
                    Bounds {
                        origin: point(px(center[0] - r), px(center[1] - r)),
                        size: size(px(2.0 * r), px(2.0 * r)),
                    },
                    fill,
                )
                .corner_radii(px(r)),
            );
        }
    }
}

// ----- link geometry --------------------------------------------------------

/// The most vertices one link path is asked to carry.
///
/// This is gpui's ceiling for a *stroked* path: `PathBuilder::stroke` hands its triangles
/// to lyon's `VertexBuffers<Point, u16>`, and past 65535 of them the answer is
/// `TooManyVertices`, the build fails, and gpui draws *nothing at all* for that path. The
/// renderer no longer goes through lyon at all — it writes a path's `PathVertex` array
/// itself, and `gpui_wgpu` draws a path's vertices unindexed — so this is no longer a wall
/// the graph can hit. It is kept as the bound on a batch anyway, because what settled
/// [`LINKS_PER_PATH`] is the allocation a batch makes, not the ceiling.
const PATH_VERTEX_LIMIT: usize = 65_535;

/// The vertices a hand-built link takes: a quad, as two triangles.
///
/// Six rather than four, because gpui's `Path` has no index buffer: `push_triangle` writes
/// its three corners into the vertex array and shares nothing with the next triangle. That
/// is also what gpui's own stroker ends up with, which is the shape this has to match.
const LINK_VERTICES: usize = 6;

/// How many links go into one path before it is closed and the next started.
///
/// Far below what [`PATH_VERTEX_LIMIT`] allows, and measured rather than derived: the
/// batch's vertex array is the allocation that costs. A 15000-link batch asks for 665 KB,
/// which the allocator answers with fresh pages every frame; a 1000-link batch is 88 KB,
/// which it keeps and hands back. On the 5k vault at whole-vault zoom that is 7.6 ms of a
/// frame turned into 4.6, and it is why the ceiling is a limit and not a target.
///
/// Re-checked after the batch became a `gpui::Path` vertex array instead of lyon's indexed
/// buffers (a different allocation, six 32-byte vertices a link rather than four 8-byte
/// points and six 2-byte indices): 512 and 2000 both came out no better than 1000, so 1000
/// stayed. The paired A/B in the paint bench is the way to re-run that.
///
/// `pub(crate)` so the paint bench's comparison of the two ways to build a batch sizes its
/// batches the same way the renderer does.
pub(crate) const LINKS_PER_PATH: usize = 1_000;

/// A batch stays inside gpui's ceiling whatever the tuning above settles on.
const _: () = assert!(LINKS_PER_PATH * LINK_VERTICES < PATH_VERTEX_LIMIT);

/// The links of one frame, as the paths they are painted with.
///
/// Three reasons this exists instead of asking gpui to stroke the edges:
///
/// * A butt-capped straight segment *is* a quad. lyon's stroke tessellator runs a sweep
///   line over the whole path to find that out — measured 0.45 µs a segment, and the 5k
///   vault has 18843 of them — where the four corners here are a normal and two
///   multiplications. Paired A/B on the 5k vault at whole-vault zoom: gpui's stroker is
///   9.4 ms of the frame, this is 1.9; at a neighbourhood's zoom, 2.6 against 0.86.
/// * gpui's path format is a flat array of `PathVertex`s: `Path::push_triangle` writes
///   three of them and grows `bounds`, and `gpui::PathBuilder::build_path` is a loop over
///   it. Writing that array directly is using gpui's own format, not inventing a private
///   one, and `a_hand_built_link_is_the_path_gpui_would_stroke` pins it against gpui's
///   stroked path vertex for vertex.
/// * Writing it directly is also about 30% cheaper than calling `push_triangle` from here,
///   because that call inlines inside gpui — where `build_path` makes it — and not across
///   a crate boundary, where it is six calls a link and 111k calls a frame.
///
/// It batches because [`PATH_VERTEX_LIMIT`] says a whole vault does not fit in one path.
pub(crate) struct LinkPaths {
    /// Half the stroke width in screen px: how far each corner sits from the line between
    /// the two nodes.
    half: f32,
    /// The path the links are going into. `None` before the first link and after every
    /// closed batch: a path needs a starting point, and the first corner of the first link
    /// is what `gpui::PathBuilder::build_path` would have used as its own.
    path: Option<Path<Pixels>>,
    /// How many links are in [`Self::path`].
    drawn: usize,
    /// The closed batches, in the order they are painted.
    closed: Vec<Path<Pixels>>,
    /// What each batch's vertex array is reserved for, in vertices.
    capacity: usize,
}

impl LinkPaths {
    /// A builder for links `width` wide, whose batches are sized for `links` of them.
    ///
    /// `pub(crate)` rather than private so the 3D pass (roadmap P3.6) batches its
    /// projected links through the same hand-built vertex shape, the same ceiling and
    /// the same batching — one link format in the crate, not two that can drift.
    ///
    /// The hint is not a cap — a batch that outgrows it is closed and the next started —
    /// it is what the vertex array is reserved for. A whole-vault pass wants
    /// [`LINKS_PER_PATH`]; a hub's links want its degree, and reserving 6000 vertices for
    /// a batch that draws forty of them is an allocation the hover would pay for every
    /// time the pointer moves.
    pub(crate) fn new(width: Pixels, links: usize) -> Self {
        Self {
            half: width.as_f32() / 2.0,
            path: None,
            drawn: 0,
            closed: Vec::new(),
            capacity: links.min(LINKS_PER_PATH) * LINK_VERTICES,
        }
    }

    /// The quad for the segment from `a` to `b`, both in window px.
    pub(crate) fn segment(&mut self, a: [f32; 2], b: [f32; 2]) {
        let Some(quad) = stroke_quad(a, b, self.half) else {
            // Two nodes sitting on each other. Stroking that would need a direction, and
            // there is not one.
            return;
        };
        if self.drawn == LINKS_PER_PATH {
            self.finish();
        }
        let corners = [
            point(px(quad[0][0]), px(quad[0][1])),
            point(px(quad[1][0]), px(quad[1][1])),
            point(px(quad[2][0]), px(quad[2][1])),
            point(px(quad[3][0]), px(quad[3][1])),
        ];
        let path = self.path.get_or_insert_with(|| {
            let mut path = Path::new(corners[0]);
            // Sized up front: the vertex array is only ever pushed to, and growing a
            // several-thousand-vertex array by doubling is an allocation this code can
            // actually avoid.
            path.vertices = Vec::with_capacity(self.capacity);
            path
        });
        // Two triangles a link, along the same diagonal `gpui::PathBuilder::build_path`
        // takes from lyon's indices, with the normal it gives every vertex.
        //
        // The six vertices are written into the array directly rather than pushed through
        // `Path::push_triangle`, which is the call gpui's own stroker makes — but makes
        // *inside* gpui, where it inlines. From here it does not, and six calls a link
        // across 18k links is a measurable part of a frame that is otherwise nothing but
        // copying 32-byte vertices. `a_hand_built_link_is_the_path_gpui_would_stroke` is
        // what keeps the two shapes identical, vertex for vertex.
        let st = link_normal();
        path.bounds = cover(path.bounds, &corners);
        path.vertices.extend_from_slice(&[
            vertex(corners[0], st),
            vertex(corners[1], st),
            vertex(corners[2], st),
            vertex(corners[1], st),
            vertex(corners[2], st),
            vertex(corners[3], st),
        ]);
        self.drawn += 1;
    }

    /// Closes the open batch, if there is one. Idempotent.
    fn finish(&mut self) {
        if self.drawn == 0 {
            return;
        }
        if let Some(path) = self.path.take() {
            self.closed.push(path);
        }
        self.drawn = 0;
    }

    /// The closed batches. It finishes first rather than trusting the caller to: a frame
    /// that stopped mid-scan would otherwise lose the last links it drew, which is what
    /// the `Drop` impl this replaces used to guarantee.
    pub(crate) fn into_paths(mut self) -> Vec<Path<Pixels>> {
        self.finish();
        self.closed
    }
}

/// The normal gpui gives every vertex of a stroked path: `build_path` passes `(0, 1)` for
/// all of them, and matching it is what makes a hand-built path shade like a stroked one.
fn link_normal() -> Point<f32> {
    point(0.0, 1.0)
}

/// One vertex of a hand-built path, in the shape gpui's own stroker writes.
fn vertex(xy_position: Point<Pixels>, st_position: Point<f32>) -> PathVertex<Pixels> {
    PathVertex {
        xy_position,
        st_position,
        content_mask: ContentMask::default(),
    }
}

/// Growing `bounds` to cover `corners`, which is what `Path::push_triangle` does one vertex
/// at a time: the same box, in one go.
fn cover(bounds: Bounds<Pixels>, corners: &[Point<Pixels>; 4]) -> Bounds<Pixels> {
    let (mut lo_x, mut lo_y) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
    let (mut hi_x, mut hi_y) = (
        (bounds.origin.x + bounds.size.width).as_f32(),
        (bounds.origin.y + bounds.size.height).as_f32(),
    );
    for corner in corners {
        lo_x = lo_x.min(corner.x.as_f32());
        lo_y = lo_y.min(corner.y.as_f32());
        hi_x = hi_x.max(corner.x.as_f32());
        hi_y = hi_y.max(corner.y.as_f32());
    }
    Bounds {
        origin: point(px(lo_x), px(lo_y)),
        size: size(px(hi_x - lo_x), px(hi_y - lo_y)),
    }
}

/// Every link the canvas can see, as paths.
fn tessellate_links(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    width: Pixels,
) -> Vec<Path<Pixels>> {
    let mut paths = LinkPaths::new(width, LINKS_PER_PATH);
    for &[a, b] in &frame.edges {
        let (Some(from), Some(to)) = (frame.nodes.get(a as usize), frame.nodes.get(b as usize))
        else {
            continue;
        };
        if view.link_misses(from.position, to.position) {
            continue;
        }
        paths.segment(
            camera.to_screen(from.position),
            camera.to_screen(to.position),
        );
    }
    paths.into_paths()
}

/// The links that hang off `slot`, as paths. The scan is over every edge either way, which
/// is what makes a hub's links cost the same as a leaf's here.
fn tessellate_hover_links(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    slot: usize,
    width: Pixels,
) -> Vec<Path<Pixels>> {
    let mut paths = LinkPaths::new(
        width,
        frame.nodes.get(slot).map_or(0, |node| node.degree as usize),
    );
    for &[a, b] in &frame.edges {
        if a as usize != slot && b as usize != slot {
            continue;
        }
        let (Some(from), Some(to)) = (frame.nodes.get(a as usize), frame.nodes.get(b as usize))
        else {
            continue;
        };
        if view.link_misses(from.position, to.position) {
            continue;
        }
        paths.segment(
            camera.to_screen(from.position),
            camera.to_screen(to.position),
        );
    }
    paths.into_paths()
}

/// The four corners of the quad a segment strokes to: its endpoints moved perpendicular to
/// the line by `half`, in the endpoints' own space.
///
/// `None` when the segment has no direction to be perpendicular to. Kept apart from
/// [`LinkPaths`] because it is the part that can be wrong without anything visibly
/// breaking, and so the part worth pinning with a test. `pub(crate)` so the paint bench's
/// older-batch comparison strokes the same quads the renderer does.
pub(crate) fn stroke_quad(a: [f32; 2], b: [f32; 2], half: f32) -> Option<[[f32; 2]; 4]> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        return None;
    }
    // The unit normal times the half-width: the offset from the line to each corner.
    let nx = -dy / len * half;
    let ny = dx / len * half;
    Some([
        [a[0] + nx, a[1] + ny],
        [a[0] - nx, a[1] - ny],
        [b[0] - nx, b[1] - ny],
        [b[0] + nx, b[1] + ny],
    ])
}

/// The runs one label is shaped with.
///
/// `len` is not decoration: gpui walks the shaped glyphs against these runs, and a run
/// covering zero bytes is stepped over, which leaves every glyph at the painter's initial
/// colour — `black()`. Leaving `len` at its default is how the labels came out near-black
/// on a dark canvas for as long as they have been drawn, whatever colour was asked for.
pub(crate) fn label_runs(palette: &Palette, labels: &Labels, text: &str) -> [TextRun; 1] {
    [TextRun {
        len: text.len(),
        color: gpui::rgb(palette.graph_text).into(),
        font: labels.font.clone(),
        ..Default::default()
    }]
}

/// Who gets named, and why the hovered node is never left out.
///
/// The budget is the zoom's: [`LABEL_CAP`] at most, highest degree first, because if
/// only *n* names fit they should be the hubs a person is most likely looking for.
/// The hovered node then joins whatever survived — it is the one name the user is
/// asking for, and a graph that answers a hover with a ring and no name makes them
/// read the ring. That holds even below [`LABEL_FADE_FROM`], where the frame itself
/// carries no labels: the hover is not texture, it is an answer.
fn chosen_labels(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    hover: Option<usize>,
    centre: Option<usize>,
) -> Vec<usize> {
    // `fade` is how far into the ramp this zoom is: 0 below [`LABEL_FADE_FROM`], where the
    // characters are smaller than the gap between two nodes and the labels are not worth
    // their cost, 1 at [`LABEL_FADE_FULL`]. It buys **fewer labels**, never fainter ones.
    let fade =
        ((camera.zoom - LABEL_FADE_FROM) / (LABEL_FADE_FULL - LABEL_FADE_FROM)).clamp(0.0, 1.0);
    let cap = if fade <= 0.02 {
        0
    } else {
        (LABEL_CAP as f32 * fade.max(LABEL_CAP_FLOOR)) as usize
    };
    let mut chosen: Vec<usize> = frame
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| view.holds(node, CULL_MARGIN))
        .map(|(slot, _)| slot)
        .collect();
    chosen.sort_by_key(|slot| std::cmp::Reverse(frame.nodes[*slot].degree));
    chosen.truncate(cap);
    for slot in hover.into_iter().chain(centre) {
        if !chosen.contains(&slot) {
            chosen.push(slot);
        }
    }
    chosen
}

/// The labels: [`LABEL_CAP`] of them at most, highest degree first, at full strength,
/// plus the hovered node always.
pub(crate) fn paint_labels(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    labels: &Labels,
    palette: &Palette,
    hover: Option<usize>,
    centre: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) {
    let chosen = chosen_labels(frame, camera, view, hover, centre);
    if chosen.is_empty() {
        return;
    }

    let label_size = px(labels.size * LABEL_SCALE);
    let text_system = window.text_system().clone();
    for slot in chosen {
        let Some(node) = frame.nodes.get(slot) else {
            continue;
        };
        let center = camera.to_screen(node.position);
        let r = node.radius * camera.zoom;
        let label = node.label.clone();
        let runs = label_runs(palette, labels, &label);
        let shaped = text_system.shape_line(label, label_size, &runs, None);
        // Centred under the dot, which means measuring the label rather than
        // guessing its width — long titles drift otherwise.
        let origin = point(
            px(center[0] - shaped.width.as_f32() / 2.0),
            px(center[1] + r + labels.size),
        );
        if let Err(err) = shaped.paint(origin, px(labels.size), TextAlign::Left, None, window, cx) {
            tracing::warn!("a graph label could not be painted: {err:#}");
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::PathBuilder;

    /// A label's run has to cover the label's bytes.
    ///
    /// gpui colours glyphs by walking the decoration runs it was handed, and it steps over
    /// a run that covers none of them, which leaves every glyph at the painter's initial
    /// colour — `black()`. Building the run with `TextRun::default()`'s `len` (zero) is
    /// exactly that, and it is how the node labels came out near-black on a dark canvas
    /// for as long as they have been drawn, whatever colour the palette asked for. This
    /// gpui has no `painted_text()`, so the run itself is what can be pinned.
    #[test]
    fn a_label_run_covers_the_whole_label() {
        let palette = shale_core::theme::Palette::built_in("mocha").unwrap();
        let labels = Labels {
            font: Font::default(),
            size: 14.0,
        };
        let text = "Ashfall 0003.md";
        let runs = label_runs(&palette, &labels, text);
        assert_eq!(runs[0].len, text.len(), "the run covers its label");
        assert_ne!(
            runs[0].color,
            gpui::black(),
            "and the colour is the palette's, not the painter's starting colour"
        );
    }

    /// The quad a link strokes to, pinned.
    ///
    /// The renderer builds this geometry itself instead of asking lyon for it, so nothing
    /// checks that it is the right shape: a wrong normal, a half-width applied twice, a
    /// winding that a future renderer decides to cull, and the graph still draws *something*
    /// — links that are too thick, too thin, or missing on one backend only.
    #[test]
    fn a_link_is_a_quad_the_width_of_its_stroke() {
        // Horizontal: the offset is straight up and down, and the quad is the segment grown
        // by the half-width on each side.
        let quad = stroke_quad([0., 0.], [10., 0.], 1.0).expect("a segment with a direction");
        assert_eq!(quad[0], [0., 1.]);
        assert_eq!(quad[1], [0., -1.]);
        assert_eq!(quad[2], [10., -1.]);
        assert_eq!(quad[3], [10., 1.]);

        // Diagonal: the corners are perpendicular to the line, not along the axes, and they
        // are exactly `half` away from it.
        let quad = stroke_quad([0., 0.], [3., 4.], 2.0).expect("a segment with a direction");
        for corner in quad {
            let along = corner[0] * 3.0 + corner[1] * 4.0; // dot with the direction
            let across = corner[0] * -4.0 + corner[1] * 3.0; // dot with the normal
            assert!(
                (across.abs() - 2.0 * 5.0).abs() < 1e-4,
                "{corner:?} is half a stroke off the line, got {across}"
            );
            assert!(
                (0.0..=5.0 * 5.0).contains(&along),
                "{corner:?} is not between the ends"
            );
        }

        // The area is the link's length times its width, which is what a stroke means. It
        // catches the doubled-half-width and the wrong-normal mistakes at once.
        let area = quad_area(&quad);
        assert!(
            (area - 5.0 * 4.0).abs() < 1e-4,
            "a 5-long link 4 wide covers 20, got {area}"
        );

        assert!(
            stroke_quad([2., 2.], [2., 2.], 1.0).is_none(),
            "a link between a node and itself has no direction to be perpendicular to"
        );
    }

    /// The renderer builds link geometry itself instead of asking lyon to stroke it, which
    /// is only safe if what it hands gpui is the same shape. This compares the two paths:
    /// same vertex count, same bounds, same corners, same normals.
    ///
    /// If gpui ever changes what a stroke means (round caps, joins between subpaths, a
    /// different vertex format), this fails here rather than as links with the wrong ends in
    /// a window.
    #[test]
    fn a_hand_built_link_is_the_path_gpui_would_stroke() {
        let segments: [[f32; 4]; 3] = [[0., 0., 10., 0.], [0., 0., 0., 8.], [-3., -4., 6., 12.]];
        let width = px(2.0);

        let mut stroked = PathBuilder::stroke(width);
        for [ax, ay, bx, by] in segments {
            stroked.move_to(point(px(ax), px(ay)));
            stroked.line_to(point(px(bx), px(by)));
        }
        let stroked = stroked.build().expect("gpui can stroke three segments");

        // The renderer's own builder, which is what a frame actually hands gpui.
        let mut paths = LinkPaths::new(width, LINKS_PER_PATH);
        for [ax, ay, bx, by] in segments {
            paths.segment([ax, ay], [bx, by]);
        }
        let paths = paths.into_paths();
        assert_eq!(paths.len(), 1, "three links fit in one batch");
        let hand = &paths[0];

        assert_eq!(
            hand.vertices.len(),
            stroked.vertices.len(),
            "two triangles a link either way"
        );
        assert_eq!(
            hand.vertices.len(),
            segments.len() * LINK_VERTICES,
            "and six vertices a link, because a gpui path has no index buffer"
        );
        for (a, b) in [
            (hand.bounds.origin.x, stroked.bounds.origin.x),
            (hand.bounds.origin.y, stroked.bounds.origin.y),
            (hand.bounds.size.width, stroked.bounds.size.width),
            (hand.bounds.size.height, stroked.bounds.size.height),
        ] {
            assert!(
                (a.as_f32() - b.as_f32()).abs() < 1e-3,
                "the same bounds: {a} vs {b}"
            );
        }
        let normals = |path: &gpui::Path<Pixels>| {
            let mut seen: Vec<(f32, f32)> = path
                .vertices
                .iter()
                .map(|v| (v.st_position.x, v.st_position.y))
                .collect();
            seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
            seen.dedup();
            seen
        };
        assert_eq!(
            normals(hand),
            normals(&stroked),
            "every vertex carries the normal gpui's own stroker gives it, which is what makes \n             the two shade alike"
        );
        let corners = |path: &gpui::Path<Pixels>| {
            let mut points: Vec<(f32, f32)> = path
                .vertices
                .iter()
                .map(|v| (v.xy_position.x.as_f32(), v.xy_position.y.as_f32()))
                .collect();
            points.sort_by(|a, b| a.partial_cmp(b).unwrap());
            // Deduplicated: both paths are two triangles a link, but they split the quad
            // along different diagonals, so which corner is repeated is not the same.
            points.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4);
            points
        };
        let (hand, stroked) = (corners(hand), corners(&stroked));
        assert_eq!(
            hand.len(),
            stroked.len(),
            "the same twelve corners: four a link, whatever diagonal each splits its quad along"
        );
        for (h, s) in hand.iter().zip(stroked.iter()) {
            assert!(
                (h.0 - s.0).abs() < 1e-3 && (h.1 - s.1).abs() < 1e-3,
                "the same corners, got {h:?} vs {s:?}"
            );
        }
    }

    /// A batch closes at [`LINKS_PER_PATH`], and the links left over after the last full
    /// batch are still drawn.
    ///
    /// The builder used to flush on `Drop`, which made that automatic; this one finishes in
    /// [`LinkPaths::into_paths`], and a mistake there shows up as the last few links of
    /// every frame quietly missing — which survives a glance at a window.
    #[test]
    fn the_last_links_of_a_frame_are_not_left_in_the_batch() {
        let width = px(2.0);
        let mut paths = LinkPaths::new(width, LINKS_PER_PATH);
        for i in 0..(LINKS_PER_PATH + 7) {
            let x = i as f32;
            paths.segment([x, 0.], [x, 10.]);
        }
        let paths = paths.into_paths();
        assert_eq!(paths.len(), 2, "one full batch and the remainder");
        assert_eq!(
            paths[0].vertices.len(),
            LINKS_PER_PATH * LINK_VERTICES,
            "the full one is full"
        );
        assert_eq!(
            paths[1].vertices.len(),
            7 * LINK_VERTICES,
            "and nothing is lost"
        );
    }

    /// The area of a quad given as corners in order, by the shoelace formula.
    fn quad_area(quad: &[[f32; 2]; 4]) -> f32 {
        let mut sum = 0.0;
        for i in 0..4 {
            let [x1, y1] = quad[i];
            let [x2, y2] = quad[(i + 1) % 4];
            sum += x1 * y2 - x2 * y1;
        }
        sum.abs() / 2.0
    }
}
