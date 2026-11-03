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

use gpui::{point, px, size, App, Bounds, Font, PathBuilder, Pixels, TextAlign, TextRun, Window};
use shale_core::graph::geom::Camera;
use shale_core::graph::NodeKind;
use shale_core::theme::Palette;

use super::frame::{GraphFrame, GraphNode};

/// The most labels drawn in one frame, highest degree first. A 2k-node vault at
/// a zoom that shows all of it cannot label all of it, and a wall of overlapping
/// text is worse than a quiet graph.
pub(crate) const LABEL_CAP: usize = 300;

/// A label's opacity ramps in over this zoom range and is invisible below it:
/// under ~0.45x the characters are smaller than the gap between two nodes, so
/// the labels cost more than they say.
const LABEL_FADE_FROM: f32 = 0.45;
const LABEL_FADE_FULL: f32 = 0.85;

/// A link's width in world units, so it thickens with zoom like the nodes do
/// rather than turning into a hairline at 0.2x.
const LINK_WIDTH: f32 = 1.0;

/// The smallest a link is ever drawn, in screen px. Thinner than this and the
/// path's own antialiasing makes it flicker as the sim moves.
const LINK_MIN_PX: f32 = 0.5;

/// How far a node may hang off the viewport edge and still be drawn. A node half
/// in view is still a node the user can see.
const CULL_MARGIN: f32 = 2.0;

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

/// Paint one graph frame into `bounds`, using `camera` for this frame.
///
/// Called from the canvas element's paint closure, so it borrows nothing that
/// outlives the frame: the caller hands over an owned snapshot and a camera.
pub(crate) fn paint_graph(
    bounds: Bounds<Pixels>,
    camera: Camera,
    frame: &GraphFrame,
    palette: &Palette,
    labels: &Labels,
    window: &mut Window,
    cx: &mut App,
) {
    // The world rectangle the canvas can see. Everything else is skipped.
    let lo = camera.to_world([bounds.origin.x.as_f32(), bounds.origin.y.as_f32()]);
    let hi = camera.to_world([
        bounds.origin.x.as_f32() + bounds.size.width.as_f32(),
        bounds.origin.y.as_f32() + bounds.size.height.as_f32(),
    ]);
    let (lo_x, hi_x) = (lo[0].min(hi[0]), lo[0].max(hi[0]));
    let (lo_y, hi_y) = (lo[1].min(hi[1]), lo[1].max(hi[1]));

    // ----- links ---------------------------------------------------------
    // One path for every visible link: they are all the same colour today, and a
    // single path is one scene primitive instead of thousands.
    let mut path = PathBuilder::stroke(px((LINK_WIDTH * camera.zoom).max(LINK_MIN_PX)));
    let mut drew = false;
    for &[a, b] in &frame.edges {
        let (Some(from), Some(to)) = (frame.nodes.get(a as usize), frame.nodes.get(b as usize))
        else {
            continue;
        };
        // Cheap reject: a link whose own bounding box misses the viewport cannot
        // be seen, and at 2k nodes this is the test that pays for the frame.
        let [ax, ay] = from.position;
        let [bx, by] = to.position;
        if ax.max(bx) < lo_x || ax.min(bx) > hi_x || ay.max(by) < lo_y || ay.min(by) > hi_y {
            continue;
        }
        let a = camera.to_screen([ax, ay]);
        let b = camera.to_screen([bx, by]);
        path.move_to(point(px(a[0]), px(a[1])));
        path.line_to(point(px(b[0]), px(b[1])));
        drew = true;
    }
    if drew {
        match path.build() {
            Ok(built) => window.paint_path(built, gpui::rgb(palette.graph_edge)),
            Err(err) => tracing::warn!("the graph's links could not be built: {err:#}"),
        }
    }

    // ----- nodes ---------------------------------------------------------
    // Grouped by fill so the paint calls come in runs. Notes and attachments
    // share a colour; a ghost is deliberately dimmer, because it is a hole in the
    // vault rather than a note. A tag takes the colour a `#tag` has in prose,
    // which is what makes the graph and the note agree about what a tag is.
    // (Colouring *notes* by their tags is the colour-by-tag option, and it comes
    // with the filter panel; the hues for it are already in core.)
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
            if !visible(node, lo_x, hi_x, lo_y, hi_y, CULL_MARGIN) {
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

    // ----- labels --------------------------------------------------------
    let alpha =
        ((camera.zoom - LABEL_FADE_FROM) / (LABEL_FADE_FULL - LABEL_FADE_FROM)).clamp(0.0, 1.0);
    if alpha <= 0.02 {
        return;
    }
    // Highest degree first: if only `LABEL_CAP` labels fit, they should be the
    // hubs, because those are the notes a person is most likely looking for.
    let mut visible: Vec<&GraphNode> = frame
        .nodes
        .iter()
        .filter(|node| visible(node, lo_x, hi_x, lo_y, hi_y, CULL_MARGIN))
        .collect();
    visible.sort_by_key(|node| std::cmp::Reverse(node.degree));
    visible.truncate(LABEL_CAP);

    let label_size = px(labels.size * LABEL_SCALE);
    let mut color: gpui::Hsla = gpui::rgb(palette.graph_text).into();
    color.a *= alpha;
    let runs = [TextRun {
        color,
        font: labels.font.clone(),
        ..Default::default()
    }];
    let text_system = window.text_system().clone();
    for node in visible {
        let center = camera.to_screen(node.position);
        let r = node.radius * camera.zoom;
        let shaped = text_system.shape_line(node.label.clone(), label_size, &runs, None);
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

/// Whether any part of `node` (its world disc, grown by `margin`) falls inside the
/// visible world rectangle.
fn visible(node: &GraphNode, lo_x: f32, hi_x: f32, lo_y: f32, hi_y: f32, margin: f32) -> bool {
    let [x, y] = node.position;
    let r = node.radius + margin;
    x + r >= lo_x && x - r <= hi_x && y + r >= lo_y && y - r <= hi_y
}
