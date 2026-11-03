//! The graph tab (roadmap P3.4, `graph-view-plan.md` M1b Tasks 5–6).
//!
//! **A view, not a graph.** The window owns the one [`shale_core::graph::GraphIndex`]
//! and the one force simulation ([`GraphSim`]); this entity is what the user is
//! looking at — the camera, and the counts and canvas painted from the snapshot the
//! window ships over. That split is the integration contract's, and it is what lets
//! M5j's minimap and a local-graph view appear later without a second simulation of
//! the same vault.
//!
//! What is here is the *tab kind* made real: a focusable panel with its own key
//! context, so the graph can take the keyboard without borrowing vim's normal-mode
//! keys, and hand it back on `escape`. The interactions (Task 7) and the filter
//! panel (Task 8) come next.

mod frame;
mod render;
mod sim;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    canvas, div, prelude::*, App, Context, FocusHandle, Focusable, IntoElement, Render,
    SharedString, Window,
};
use shale_core::graph::geom::Camera;
use shale_core::theme::Palette;

pub(crate) use frame::GraphFrame;
pub(crate) use sim::GraphSim;

/// The key context the graph's bindings live in. It is a context of its own for the
/// same reason the picker and the prompt are: gpui enables a context's bindings
/// whenever it is in the focused element's stack, so the graph's keys cannot collide
/// with vim normal mode — and every key the graph wants (`hjkl`, `f`, `Enter`) is
/// already spoken for in the note.
pub(crate) const CONTEXT: &str = "graph";

/// How much empty canvas [`Camera::fit`] keeps around the whole graph when it
/// frames it for the first time.
const FIT_MARGIN: f32 = 24.0;

/// What the window tells the view about the graph it is a view over.
///
/// A snapshot rather than a borrow, because the graph lives on the window and the view
/// is an entity that renders on its own: the window pushes this in whenever the graph
/// is built or a note changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GraphSummary {
    /// Notes in the graph, including the ones that link nothing.
    pub(crate) notes: usize,
    /// Link targets that resolve to no file — visible structure rather than vanished.
    pub(crate) ghosts: usize,
    /// `#tag` nodes.
    pub(crate) tags: usize,
    /// Links between them, with A→B and B→A already one edge.
    pub(crate) links: usize,
}

/// The graph tab: a camera over the window's graph.
pub(crate) struct GraphView {
    /// The view's own keyboard, so the `graph` context can be the active one and
    /// `escape` can hand the keyboard back to the note.
    pub(crate) focus_handle: FocusHandle,
    /// The colours the panel is drawn with, from the window. Shared with every other
    /// surface so a theme change is one assignment rather than a rebuild.
    palette: Rc<Palette>,
    /// The face the labels are drawn in — the same one every other panel types in,
    /// so a node's label looks like the note title it names.
    font: SharedString,
    /// Its size, in px. Labels are drawn a little smaller than the note's text.
    font_size: f32,
    /// What the graph holds, once the window has built it. `None` is the moment before
    /// the first build, and the panel says so rather than drawing a blank box.
    summary: Option<GraphSummary>,
    /// What to paint: the window's simulation snapshot, shipped whenever it moves.
    frame: Option<Rc<GraphFrame>>,
    /// Where the **user** put the view — by a session restore now, by a drag or a
    /// wheel later. `None` means "keep framing the whole graph", and that is the right
    /// default rather than a lazy one: the layout is still moving while it settles, and
    /// a camera fitted to the first frame leaves the settled graph hanging off the edge
    /// of the screen.
    camera: Option<Camera>,
    /// The last camera the canvas fitted, remembered so the session file can record
    /// where the user was looking even when they never moved the view themselves.
    ///
    /// It arrives through a cell because the paint phase cannot reach into the entity
    /// being painted, and the first frame is the only one that does not already know
    /// the canvas's bounds.
    pending_fit: Rc<RefCell<Option<Camera>>>,
}

impl GraphView {
    pub(crate) fn new(
        palette: Rc<Palette>,
        font: SharedString,
        font_size: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            palette,
            font,
            font_size,
            summary: None,
            frame: None,
            camera: None,
            pending_fit: Rc::default(),
        }
    }

    /// The colours and the face the panel is drawn with. Called at startup and
    /// whenever the theme changes, from the same place that repaints everything else.
    pub(crate) fn set_appearance(
        &mut self,
        palette: Rc<Palette>,
        font: SharedString,
        font_size: f32,
        cx: &mut Context<Self>,
    ) {
        self.palette = palette;
        self.font = font;
        self.font_size = font_size;
        cx.notify();
    }

    /// What the graph looks like, as counts. Called by the window whenever the index
    /// is built or a note moves under it.
    pub(crate) fn set_summary(&mut self, summary: GraphSummary, cx: &mut Context<Self>) {
        self.summary = Some(summary);
        cx.notify();
    }

    /// The picture to paint. Shipped whenever the sim ticks or the graph changes
    /// underneath it; the view never reaches into the sim.
    pub(crate) fn set_frame(&mut self, frame: Rc<GraphFrame>, cx: &mut Context<Self>) {
        self.frame = Some(frame);
        cx.notify();
    }

    /// Where the view is looking, for the session file: the user's camera if they set
    /// one, and otherwise the framing the canvas last chose for itself.
    pub(crate) fn camera(&self) -> Option<Camera> {
        self.camera.or(self.pending_fit.borrow().and_then(|fit| {
            // A fit made against a canvas that had no size yet is not a view worth
            // remembering; the next frame fits again.
            (fit.zoom > shale_core::graph::geom::MIN_ZOOM).then_some(fit)
        }))
    }

    /// Put the view back where the last session left it. Called once, while the tab is
    /// being restored, before anything has been painted.
    pub(crate) fn set_camera(&mut self, camera: Option<Camera>) {
        self.camera = camera;
    }

    /// What the panel is showing. Only the tests ask: the window is the one that
    /// pushes the counts in.
    #[cfg(test)]
    pub(crate) fn summary(&self) -> Option<GraphSummary> {
        self.summary
    }

    /// The frame the view is painting, for the tests that count what a frame drew.
    #[cfg(test)]
    pub(crate) fn frame(&self) -> Option<&GraphFrame> {
        self.frame.as_deref()
    }
}

impl Focusable for GraphView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// One count in the summary row: the number in the node colour, its label in the
/// chrome's quiet colour, so the row reads as data with captions rather than prose.
fn stat(palette: &Palette, value: usize, label: &'static str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .text_color(gpui::rgb(palette.graph_node))
                .child(value.to_string()),
        )
        .child(div().text_color(gpui::rgb(palette.ui_dim)).child(label))
}

impl Render for GraphView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Rc::clone(&self.palette);
        let frame = self.frame.clone();
        // The two closures each need their own handle on the snapshot: prepaint only
        // wants the positions, paint wants the whole picture.
        let painted = frame.clone();
        let camera = self.camera;
        let pending_fit = Rc::clone(&self.pending_fit);
        let labels = render::Labels {
            font: gpui::font(self.font.clone()),
            size: self.font_size,
        };

        // The summary, or the reason there is not one. A graph that has been built but
        // is empty says "0 notes", which is a different fact from "nothing has been
        // built", and the user should be able to tell them apart.
        let body = match self.summary {
            None => div()
                .text_color(gpui::rgb(palette.ui_dim))
                .child("The graph has not been built yet."),
            Some(counts) => div()
                .flex()
                .flex_wrap()
                .gap_4()
                .child(stat(&palette, counts.notes, "notes"))
                .child(stat(&palette, counts.links, "links"))
                .child(stat(&palette, counts.tags, "tags"))
                .child(stat(&palette, counts.ghosts, "missing")),
        };
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .text_color(gpui::rgb(palette.ui_fg))
            .child(
                div()
                    .flex_none()
                    .font_weight(gpui::FontWeight::BOLD)
                    // The label is drawn in the colour the graph's own text will use,
                    // so the one thing on screen that is already the graph is painted
                    // with the graph's palette.
                    .text_color(gpui::rgb(palette.graph_text))
                    .child("Graph"),
            )
            .child(body)
            // The canvas: the graph itself, drawn with the paint API rather than laid
            // out as divs. The wrapper occludes, so a picker or popup over the graph
            // cannot be hovered through, and the canvas carries the border that makes
            // the frame visible even in an empty vault.
            //
            // Read before the closures below, which take the palette with them.
            .child({
                let border = gpui::rgb(palette.ui_border);
                div().flex_1().min_h_0().occlude().child(
                    canvas(
                        move |bounds, _window, _cx| {
                            // The camera for this frame: the user's, or a first
                            // look that fits the whole graph into what there is to
                            // draw it in.
                            let fitted = match camera {
                                Some(camera) => camera,
                                None => {
                                    let positions: Vec<[f32; 2]> = frame
                                        .as_ref()
                                        .map(|frame| {
                                            frame.nodes.iter().map(|node| node.position).collect()
                                        })
                                        .unwrap_or_default();
                                    let camera = Camera::fit(
                                        &positions,
                                        [
                                            bounds.origin.x.as_f32(),
                                            bounds.origin.y.as_f32(),
                                            bounds.size.width.as_f32(),
                                            bounds.size.height.as_f32(),
                                        ],
                                        FIT_MARGIN,
                                    );
                                    *pending_fit.borrow_mut() = Some(camera);
                                    camera
                                }
                            };
                            fitted
                        },
                        move |bounds, camera, window, cx| {
                            if let Some(frame) = &painted {
                                render::paint_graph(
                                    bounds, camera, frame, &palette, &labels, window, cx,
                                );
                            }
                        },
                    )
                    .size_full()
                    .border_1()
                    .border_color(border)
                    .rounded(crate::style::CHIP_RADIUS),
                )
            })
    }
}
