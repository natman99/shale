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
    point, px, size, App, Bounds, ContentMask, Font, PathBuilder, Pixels, Rgba, TextAlign, TextRun,
    Window,
};
use lyon::math::{point as lpoint, Point};
use lyon::tessellation::VertexBuffers;
use shale_core::graph::geom::Camera;
use shale_core::graph::NodeKind;
use shale_core::theme::Palette;

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
const LABEL_SCALE: f32 = 0.8;

/// How much bigger than a node its hover ring is, in screen px. Two is enough to
/// read as a ring at any zoom and small enough not to swallow a hub.
const RING_PX: f32 = 2.0;

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
    /// The labels' face and size.
    pub(crate) labels: Labels,
}

/// Paint one graph frame into `bounds`, using `camera` for this frame and `look` for
/// its colours, labels and chosen node.
///
/// Called from the canvas element's paint phase, so it borrows nothing that outlives
/// the frame: the caller hands over an owned snapshot, a camera, and the look.
///
/// Everything is painted behind a clip to `bounds`. The camera maps world units to
/// window-absolute pixels and the graph is routinely larger than its pane, so a link whose
/// ends are outside the pane used to be painted anyway — across the file tree and the tab
/// bar, which is what it looks like for a graph to bleed out of its pane.
///
/// And everything is painted inside one [`Window::paint_layer`], which is gpui's way of
/// being told that a batch of geometry shares a draw order: it replaces the bounds-tree
/// search each primitive otherwise pays for its draw order with one insert for the pane.
/// On the 5k vault that is 14.8 ms a frame at whole-vault zoom turned into 3.2, because the
/// search is not free and it is not even linear — 4768 discs cost 11.6 ms of it, 2.4 µs
/// each, while 1151 of the same discs cost 1.9 ms. Inside the layer the primitives keep
/// their insertion order (gpui's sort by draw order is stable), so links are still under
/// nodes and nodes are still under labels.
pub(crate) fn paint_graph(
    bounds: Bounds<Pixels>,
    camera: Camera,
    frame: &GraphFrame,
    look: &Look,
    window: &mut Window,
    cx: &mut App,
) {
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        window.paint_layer(bounds, |window| {
            paint_frame(bounds, camera, frame, look, window, cx);
        });
    });
}

/// The body of [`paint_graph`], with the clip in place: work out what the canvas can see,
/// then draw links, nodes, labels, in that order. Split out so the phases can be timed
/// against each other, and so the clip is something the caller does once rather than
/// something every phase has to remember.
pub(crate) fn paint_frame(
    bounds: Bounds<Pixels>,
    camera: Camera,
    frame: &GraphFrame,
    look: &Look,
    window: &mut Window,
    cx: &mut App,
) {
    let palette = look.palette.as_ref();
    let view = Viewport::new(bounds, &camera);

    paint_links(frame, &camera, &view, palette.graph_edge, window);
    if let Some(slot) = look.hover {
        paint_hover_links(
            frame,
            &camera,
            &view,
            slot,
            palette.graph_node_active,
            window,
        );
    }
    paint_nodes(frame, &camera, &view, palette, look.hover, window);
    paint_labels(frame, &camera, &view, &look.labels, palette, window, cx);
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
pub(crate) fn paint_links(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    color: u32,
    window: &mut Window,
) {
    let width = (LINK_WIDTH * camera.zoom).max(LINK_MIN_PX);
    let mut batch = StrokeBatch::new(px(width), gpui::rgb(color), window);
    for &[a, b] in &frame.edges {
        let (Some(from), Some(to)) = (frame.nodes.get(a as usize), frame.nodes.get(b as usize))
        else {
            continue;
        };
        if view.link_misses(from.position, to.position) {
            continue;
        }
        batch.segment(
            camera.to_screen(from.position),
            camera.to_screen(to.position),
        );
    }
}

/// The links that hang off the chosen node, in the active colour: the graph's answer to
/// "what connects to this?" without making the user trace lines by hand. Dimming
/// everything else instead would need a colour per link, and one path per colour is the
/// deal this renderer made.
pub(crate) fn paint_hover_links(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    slot: usize,
    color: u32,
    window: &mut Window,
) {
    let width = (LINK_WIDTH * camera.zoom).max(LINK_MIN_PX);
    let mut batch = StrokeBatch::new(px(width), gpui::rgb(color), window);
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
        batch.segment(
            camera.to_screen(from.position),
            camera.to_screen(to.position),
        );
    }
}

/// The nodes, grouped by fill so the paint calls come in runs. Notes and attachments
/// share a colour; a ghost is deliberately dimmer, because it is a hole in the vault
/// rather than a note. A tag takes the colour a `#tag` has in prose, which is what makes
/// the graph and the note agree about what a tag is. (Colouring *notes* by their tags is
/// the colour-by-tag option, and it comes with the filter panel; the hues for it are
/// already in core.)
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
    window: &mut Window,
) {
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
            NodeKind::Note | NodeKind::Attachment => gpui::rgb(palette.graph_node),
            NodeKind::Tag => gpui::rgb(palette.tag),
            NodeKind::Ghost => gpui::rgb(palette.ui_dim),
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

/// The most vertices gpui will put in one path.
///
/// gpui hands a path's vertices to lyon's `VertexBuffers<Point, u16>`, and past 65535 of
/// them the answer is `TooManyVertices`, the build fails, and gpui draws *nothing at all*
/// for that path. This is not a performance detail: the 5k vault's 18843 links are more
/// than twice the ceiling, so asking one path to hold them is not a big drawing, it is an
/// empty pane with a warning in the log.
const PATH_VERTEX_LIMIT: usize = 65_535;

/// The vertices a hand-built link takes: a quad, as two triangles that share an edge.
const LINK_VERTICES: usize = 4;

/// How many links go into one path before it is painted and the next started.
///
/// Far below what [`PATH_VERTEX_LIMIT`] allows, and measured rather than derived: the
/// batch's vertex buffer is the allocation that costs. A 15000-link batch asks for 665 KB,
/// which the allocator answers with fresh pages every frame; a 1000-link batch is 88 KB,
/// which it keeps and hands back. On the 5k vault at whole-vault zoom that is 7.6 ms of a
/// frame turned into 4.6, and it is why the ceiling is a limit and not a target.
const LINKS_PER_PATH: usize = 1_000;

/// A batch stays inside gpui's ceiling whatever the tuning above settles on.
const _: () = assert!(LINKS_PER_PATH * LINK_VERTICES < PATH_VERTEX_LIMIT);

/// One colour's worth of link geometry, tessellated by hand and flushed a path at a time.
///
/// Two reasons this exists instead of asking gpui to stroke the edges:
///
/// * A butt-capped straight segment *is* a quad. lyon's stroke tessellator runs a sweep
///   line over the whole path to find that out — measured 0.45 µs a segment, and the 5k
///   vault has 18843 of them — where the four corners here are a normal and two
///   multiplications. On the 5k vault that is 7.9 ms of a frame at whole-vault zoom turned
///   into 5.6, and 2.6 into 1.2 at a neighbourhood's.
/// * [`PathBuilder::build_path`] is public and is the very function gpui's tessellators
///   hand their results to, so the path that reaches the scene is shaped exactly as gpui
///   would have shaped it. This is gpui's format, used from its own front door.
///
/// It batches because [`PATH_VERTEX_LIMIT`] says a whole vault does not fit in one path.
struct StrokeBatch<'a> {
    /// Half the stroke width in screen px: how far each corner sits from the line between
    /// the two nodes.
    half: f32,
    /// The corners and triangles of the links waiting to be painted.
    buf: VertexBuffers<Point, u16>,
    /// How many links are in [`Self::buf`].
    drawn: usize,
    /// The colour every flushed path is painted with.
    color: Rgba,
    /// Where the flushed paths go.
    window: &'a mut Window,
}

impl<'a> StrokeBatch<'a> {
    /// A batch of links `width` wide, painted in `color`.
    fn new(width: Pixels, color: Rgba, window: &'a mut Window) -> Self {
        Self {
            half: width.as_f32() / 2.0,
            buf: empty_buffers(),
            drawn: 0,
            color,
            window,
        }
    }

    /// The quad for the segment from `a` to `b`, both in window px.
    fn segment(&mut self, a: [f32; 2], b: [f32; 2]) {
        let Some(quad) = stroke_quad(a, b, self.half) else {
            // Two nodes sitting on each other. Stroking that would need a direction, and
            // there is not one.
            return;
        };
        if self.drawn == LINKS_PER_PATH {
            self.finish();
        }
        let base = self.buf.vertices.len() as u16;
        self.buf.vertices.extend_from_slice(&[
            lpoint(quad[0][0], quad[0][1]),
            lpoint(quad[1][0], quad[1][1]),
            lpoint(quad[2][0], quad[2][1]),
            lpoint(quad[3][0], quad[3][1]),
        ]);
        self.buf.indices.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base + 1,
            base + 2,
            base + 3,
        ]);
        self.drawn += 1;
    }

    /// Paints the links waiting in the buffer and starts the next batch. Idempotent.
    fn finish(&mut self) {
        if self.drawn == 0 {
            return;
        }
        let buf = std::mem::replace(&mut self.buf, empty_buffers());
        self.window
            .paint_path(PathBuilder::build_path(buf), self.color);
        self.drawn = 0;
    }
}

impl Drop for StrokeBatch<'_> {
    /// Finishing is not optional: a batch that goes out of scope mid-way would leave its
    /// last links undrawn.
    fn drop(&mut self) {
        self.finish();
    }
}

/// A buffer sized for a full batch, so a batch never reallocates. [`PathBuilder::build_path`]
/// takes the buffers by value, so each path needs a fresh one.
fn empty_buffers() -> VertexBuffers<Point, u16> {
    VertexBuffers::with_capacity(LINKS_PER_PATH * LINK_VERTICES, LINKS_PER_PATH * 6)
}

/// The four corners of the quad a segment strokes to: its endpoints moved perpendicular to
/// the line by `half`, in the endpoints' own space.
///
/// `None` when the segment has no direction to be perpendicular to. Kept apart from
/// [`StrokeBatch`] because it is the part that can be wrong without anything visibly
/// breaking, and so the part worth pinning with a test.
fn stroke_quad(a: [f32; 2], b: [f32; 2], half: f32) -> Option<[[f32; 2]; 4]> {
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
fn label_runs(palette: &Palette, labels: &Labels, text: &str) -> [TextRun; 1] {
    [TextRun {
        len: text.len(),
        color: gpui::rgb(palette.graph_text).into(),
        font: labels.font.clone(),
        ..Default::default()
    }]
}

/// The labels: [`LABEL_CAP`] of them at most, highest degree first, at full strength.
pub(crate) fn paint_labels(
    frame: &GraphFrame,
    camera: &Camera,
    view: &Viewport,
    labels: &Labels,
    palette: &Palette,
    window: &mut Window,
    cx: &mut App,
) {
    // `fade` is how far into the ramp this zoom is: 0 below [`LABEL_FADE_FROM`], where the
    // characters are smaller than the gap between two nodes and the labels are not worth
    // their cost, 1 at [`LABEL_FADE_FULL`]. It buys **fewer labels**, never fainter ones.
    let fade =
        ((camera.zoom - LABEL_FADE_FROM) / (LABEL_FADE_FULL - LABEL_FADE_FROM)).clamp(0.0, 1.0);
    if fade <= 0.02 {
        return;
    }
    let cap = (LABEL_CAP as f32 * fade.max(LABEL_CAP_FLOOR)) as usize;
    // Highest degree first: if only `cap` labels fit, they should be the
    // hubs, because those are the notes a person is most likely looking for.
    let mut chosen: Vec<&GraphNode> = frame
        .nodes
        .iter()
        .filter(|node| view.holds(node, CULL_MARGIN))
        .collect();
    chosen.sort_by_key(|node| std::cmp::Reverse(node.degree));
    chosen.truncate(cap);

    let label_size = px(labels.size * LABEL_SCALE);
    let text_system = window.text_system().clone();
    for node in chosen {
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
    /// same vertex count, same bounds, same set of corners.
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

        let mut buf = VertexBuffers::new();
        for [ax, ay, bx, by] in segments {
            let quad = stroke_quad([ax, ay], [bx, by], width.as_f32() / 2.0)
                .expect("these segments have a direction");
            let base = buf.vertices.len() as u16;
            buf.vertices.extend_from_slice(&[
                lpoint(quad[0][0], quad[0][1]),
                lpoint(quad[1][0], quad[1][1]),
                lpoint(quad[2][0], quad[2][1]),
                lpoint(quad[3][0], quad[3][1]),
            ]);
            buf.indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base + 1,
                base + 2,
                base + 3,
            ]);
        }
        let hand = PathBuilder::build_path(buf);

        assert_eq!(
            hand.vertices.len(),
            stroked.vertices.len(),
            "two triangles a link either way"
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
        let (hand, stroked) = (corners(&hand), corners(&stroked));
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
