//! The graph tab (roadmap P3.4, `graph-view-plan.md` M1b Tasks 5–8).
//!
//! **A view, not a graph.** The window owns the one [`shale_core::graph::GraphIndex`]
//! and the one force simulation ([`GraphSim`]); this entity is what the user is
//! looking at — the camera, and the counts and canvas painted from the snapshot the
//! window ships over. That split is the integration contract's, and it is what lets
//! M5j's minimap and a local-graph view appear later without a second simulation of
//! the same vault.
//!
//! What is here is the tab made real: a focusable panel with its own key context, the
//! pointer's three jobs (hover, drag, click), and the keyboard's (pan, zoom, pick a
//! node, open it). The filter strip that decides what the picture holds is `panel.rs`;
//! the fields it draws from are here, because the strip and the canvas are two halves
//! of one tab.

mod bench;
mod cache;
mod frame;
mod panel;
mod proj;
pub(crate) mod render;
mod sim;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    canvas, div, prelude::*, App, Bounds, Context, FocusHandle, Focusable, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta,
    ScrollWheelEvent, SharedString, WeakEntity, Window,
};
use shale_core::graph::filter::GraphFilter;
use shale_core::graph::geom::{Camera, MIN_ZOOM, ZOOM_STEP};
use shale_core::graph::proj::Cam3;
use shale_core::theme::Palette;

use crate::acts::{GraphFit, GraphLocal, GraphLocalStep, GraphMode, GraphOpen, GraphPan, GraphSelect, GraphZoom};
use crate::field::Field;
use crate::workspace::Workspace;

pub(crate) use frame::GraphFrame;
// Re-exported for the tests only: the paint path calls `proj::paint_graph3d` where it
// stands, and `proj` is private to this module, so a test that wants the projected
// path's types needs them named here.
#[cfg(test)]
pub(crate) use proj::{paint_graph3d, Projection};
pub(crate) use panel::{FILTER_CONTEXT, DEFAULT_LOCAL_DEPTH};
pub(crate) use sim::{Dim, GraphSim};

/// The key context the graph's bindings live in. It is a context of its own for the
/// same reason the picker and the prompt are: gpui enables a context's bindings
/// whenever it is in the focused element's stack, so the graph's keys cannot collide
/// with vim's normal mode — and every key the graph wants (`hjkl`, `f`, `Enter`) is
/// already spoken for in the note.
pub(crate) const CONTEXT: &str = "graph";

/// How much empty canvas [`Camera::fit`] keeps around the whole graph when it frames
/// it for the first time, or again after `f`.
const FIT_MARGIN: f32 = 24.0;

/// How far an arrow key moves the view, in screen px. Shift triples it, which is what
/// the spec's "`Shift` = 3× speed" asks for.
pub(crate) const PAN_STEP: f32 = 60.0;

/// The fast pan, on `shift-<arrow>`: enough to cross a settled 2k-node layout in a
/// handful of presses without losing the fine control of the plain one.
pub(crate) const PAN_FAST: f32 = 3.0;

/// How much of a node's own radius is added to a hit test, in **screen** px. A hub is
/// 20 px across at 1× and 1 px across at 0.05×; without slop the small ones could not
/// be clicked at all when the whole vault is in view.
const HIT_SLOP_PX: f32 = 3.0;

/// How far the pointer may travel between the button going down and coming back up and
/// still count as a click rather than a drag.
/// The line between a click and a drag, in screen px of pointer travel. Below it a
/// press on a node is a click (it opens the note); past it the press becomes a drag and
/// the node starts following the pointer.
///
/// The threshold exists because a press is not a grab. Before it, a node is not pinned
/// and not moved at all — which matters most in the projected view, where the world point
/// under the cursor at the node's depth can sit a long way from the node itself, so a
/// press that pins and snaps on the first 1 px of jitter throws the layout across the
/// screen. Ruled by the person using it: 3 px was small enough that an unsteady click
/// still shifted the graph noticeably.
const CLICK_SLOP: f32 = 6.0;

/// How many px of a pixel-mode wheel equal one notch, so a trackpad's smooth scrolling
/// zooms smoothly and a wheel's notch is about one step.
const WHEEL_PX_PER_NOTCH: f32 = 50.0;

/// The most one wheel event may zoom, in notches. A flung trackpad can report hundreds
/// of px in a single event, and a graph that jumped ten zoom levels from one flick is
/// unusable.
const WHEEL_NOTCH_CLAMP: f32 = 2.0;

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

/// What a held button on the canvas is doing.
/// A camera that is moving itself to frame the neighbourhood.
///
/// **Frame-paced, never timer-paced.** The drift is a chain of next-frame callbacks, so
/// it takes exactly one step per drawn frame — the camera is moved before the frame is
/// laid out and painted, which means the picture the user sees is the picture the camera
/// was computed against. A wall-clock timer was the first attempt and it was wrong twice
/// over: ticks landed between frames, in frames, or not at all, which read as stutter;
/// and it stepped the camera out of step with the layout's own beat, which read as
/// vibration.
///
/// **A chase, not a tween.** The framing is re-derived from the frame as it currently is
/// each step, because a neighbourhood is still spreading out under the forces when the
/// drift starts. The spring chases a *smoothed* copy of that framing rather than the raw
/// one, so a layout that is still settling shakes the target without shaking the camera.
struct Drift {
    /// The frame the last step ran on. `dt` is measured between drawn frames, so the
    /// camera travels the same distance per second on a 60 Hz laptop and a 144 Hz
    /// monitor instead of the same distance per tick.
    last: Instant,
    /// Frames since the drift started. A safety cap, not the stop condition — the drift
    /// ends when the framing has been inside the dead zone for [`DRIFT_STILL_FRAMES`]
    /// frames, and that is also what lets the window go back to idle: a re-arming frame
    /// callback that never stops is a window that never stops painting.
    frames: u32,
    /// Consecutive frames the framing has been inside the dead zone.
    still: u32,
    space: DriftSpace,
}

/// The drift's state, per space.
///
/// **The projected space springs one quantity and derives the rest.** The eye distance
/// is the only thing sprung there; zoom and pan are re-fitted from the content's extent
/// at whatever distance the eye has reached. Springing zoom and distance independently
/// was the first cut, and it puts the two drivers of a node's drawn size in opposition:
/// measured on a 61-note hub, the picture shrank 42% across a widening while the hub's
/// own dot grew 4.5%, because backing the eye off enlarges the centre-sized things even
/// as the zoom pulls everything down. Deriving the framing from the distance instead
/// pins the ratio of node size to picture size at its true value on every frame.
///
/// **The flat space has one driver already** — `zoom` scales positions and radii alike —
/// so it springs zoom and pan directly.
enum DriftSpace {
    Flat {
        cam: Camera,
        target: Camera,
        vel: [f32; 3],
    },
    Deep {
        cam: Cam3,
        /// The eye distance the spring is chasing (the smoothed fit's) and its velocity.
        dist: f32,
        dist_vel: f32,
        /// The raw extent the framing is fitted to, smoothed from the frame's own. This
        /// is where the framing's ease lives: it lags the content, so a neighbourhood
        /// that has just been widened is framed as it *was* a moment ago and catches up.
        extent: ([f32; 2], [f32; 2]),
    },
}

/// Spring stiffness, rad/s. Critically damped at this value, so the camera reaches the
/// framing without overshooting it; ~0.35 s to settle. Ruled: smoothness over
/// responsiveness — the user would rather the camera glide late than jump.
const DRIFT_OMEGA: f32 = 12.0;
/// How quickly the *target* is allowed to move (s). The raw fit is chased through this,
/// so frame-to-frame jitter in a settling layout averages out before it reaches the
/// camera.
const DRIFT_TAU: f32 = 0.15;
/// The dead zone: a framing this close is the framing already. Sub-pixel pans and half-
/// percent scales are not worth another frame of repaint, and without a dead zone a
/// layout that is still settling keeps the camera micro-moving forever — the vibration
/// this whole design exists to kill.
const DRIFT_DEAD_PAN: f32 = 1.0;
const DRIFT_DEAD_SCALE: f32 = 0.005;
/// Frames inside the dead zone before the drift calls it arrived.
pub(crate) const DRIFT_STILL_FRAMES: u32 = 3;
/// The hard cap (~4 s at 60 Hz), as a safety against a layout that never rests.
const DRIFT_MAX_FRAMES: u32 = 240;
/// The `dt` bounds. Below the floor a hitched or back-to-back step does nothing; above
/// the ceiling a window that was hidden for ten seconds would fling the camera across
/// the screen when it came back.
const DRIFT_DT_MIN: f32 = 1.0 / 240.0;
const DRIFT_DT_MAX: f32 = 1.0 / 20.0;

/// Advance a critically damped spring over `dt`, **in closed form**:
/// `x(t) = (d + (v + ωd)t)·e^{-ωt} + target`, `d = x - target`.
///
/// Stepping it (`v += a·dt; x += v·dt`) was the first attempt and it overshoots at this
/// stiffness — the camera passes the framing and comes back, which is exactly the bounce
/// the "no snapping" ruling was reaching past. The closed form cannot: from rest,
/// `d·(1 + ωt)·e^{-ωt}` keeps its sign for every t, so the camera approaches its framing
/// and never crosses it.
fn spring(x: f32, v: &mut f32, target: f32, dt: f32) -> f32 {
    let decay = (-DRIFT_OMEGA * dt).exp();
    let d = x - target;
    let b = *v + DRIFT_OMEGA * d;
    let moved = d + b * dt;
    *v = (b - DRIFT_OMEGA * moved) * decay;
    target + moved * decay
}

/// The same spring in log space, for the two quantities that are multipliers: zoom and
/// eye distance. A linear spring on those makes travelling in feel different from
/// travelling out, which is exactly the asymmetry the wheel ruling killed years ago.
fn spring_log(x: f32, v: &mut f32, target: f32, dt: f32) -> f32 {
    spring(x.ln(), v, target.ln(), dt).exp()
}

/// Move `current` a fraction of the way toward `raw` in `dt`, the fraction set by
/// [`DRIFT_TAU`]. This is the smoothing the spring chases.
fn chase(current: f32, raw: f32, dt: f32) -> f32 {
    current + (raw - current) * (1.0 - (-dt / DRIFT_TAU).exp())
}

struct Drag {
    /// Where the button went down. A press that never travels past [`CLICK_SLOP`] is a
    /// click, and a click on a node opens the note it stands for.
    start: Point<Pixels>,
    /// Whether the press has travelled far enough to be a drag. Until it has, a node is
    /// neither pinned nor moved: the press is still, potentially, a click.
    began: bool,
    /// Where the pointer was last, which is what the next move steps from.
    last: Point<Pixels>,
    /// The node under the button, if there was one. `None` moves the view instead of a
    /// node: that is the left button over the background, and every middle-button press,
    /// which pans wherever it lands.
    node: Option<usize>,
    /// Which button started it. The middle one pans in both spaces; the left one pans
    /// the flat view and **orbits** the projected one, which is the one gesture whose
    /// meaning changes with the mode (roadmap P3.6).
    button: MouseButton,
}

/// The graph tab: a camera over the window's graph.
pub(crate) struct GraphView {
    /// The view's own keyboard, so the `graph` context can be the active one and
    /// `escape` can hand the keyboard back to the note.
    pub(crate) focus_handle: FocusHandle,
    /// The colours the panel is drawn with, from the window. Shared with every other
    /// surface so a theme change is one assignment rather than a rebuild.
    palette: Rc<Palette>,
    /// The face the labels are drawn in — the same one every other panel types in, so
    /// a node's label looks like the note title it names.
    font: SharedString,
    /// Its size, in px. Labels are drawn a little smaller than the note's text.
    font_size: f32,
    /// What the graph holds, once the window has built it. `None` is the moment before
    /// the first build, and the panel says so rather than drawing a blank box.
    summary: Option<GraphSummary>,
    /// Whether the window is scanning the vault for the index right now. The scan runs on
    /// a background thread, so the tab is on screen before there is anything to draw, and
    /// the counts row says so rather than showing an empty graph.
    building: bool,
    /// What to paint: the window's simulation snapshot, shipped whenever it moves.
    frame: Option<Rc<GraphFrame>>,
    /// The radius of the sphere the current layout lives in, in world units — measured
    /// when the frame arrives (see [`Self::store_frame`]) and used by the projected
    /// view's dolly clamps.
    cloud: f32,
    /// Where the **user** put the view — by a session restore, by a drag or a wheel.
    /// `None` means "keep framing the whole graph", and that is the right default
    /// rather than a lazy one: the layout is still moving while it settles, and a
    /// camera fitted to the first frame leaves the settled graph hanging off the edge
    /// of the screen.
    camera: Option<Camera>,
    /// The last camera the canvas fitted, remembered so the session file can record
    /// where the user was looking even when they never moved the view themselves.
    ///
    /// It arrives through a cell because the paint phase cannot reach into the entity
    /// being painted, and the first frame is the only one that does not already know
    /// the canvas's bounds.
    pending_fit: Rc<RefCell<Option<Camera>>>,
    /// Which space this view is showing — the window's graph mode, pushed in when the
    /// tab opens and when `2`/`3` turn it. The camera, the hit test and the paint all
    /// follow it; the physics follows it through the window.
    mode: Dim,
    /// Where the **user** put the projected view — by an orbit, a pan or a wheel.
    /// `None` means "keep framing the whole graph", for the same reason the flat
    /// camera's `None` does: the layout is still moving while it settles.
    cam3: Option<Cam3>,
    /// The angle the last session was looking at, and **only** the angle. A restored
    /// pan and zoom would be a camera pointed at wherever the graph used to be, and in
    /// three axes that is a screenful of nothing rather than a graph slightly off
    /// centre; the framing is re-derived from the layout every launch (PROJECT.md,
    /// "2D / 3D mode rules").
    orientation: Option<(f32, f32)>,
    /// The last framing the canvas fitted in the projected space, for the session file.
    pending_fit3: Rc<RefCell<Option<Cam3>>>,
    /// The picture as the last frame projected it: built while prepainting, and read
    /// back by the pointer's handlers, so what the user clicked and what the user saw
    /// are the same points rather than two answers that can drift apart.
    projection: Rc<RefCell<Option<Rc<proj::Projection>>>>,
    /// The canvas's bounds, recorded while prepainting, for the same reason the fit
    /// is: the keyboard's zoom needs the middle of a space the view only learns about
    /// during a frame.
    canvas: Rc<RefCell<Option<Bounds<Pixels>>>>,
    /// A camera moving itself to frame the neighbourhood (M2b). Cleared by any camera
    /// input from the user.
    reframe: Option<Drift>,
    /// Whether the chain of next-frame callbacks that drives the drift is live. Arming
    /// happens in `render`, which is where the frame's `Window` is within reach.
    drift_armed: bool,
    /// The links kept from the last paint, so a repaint that moved nothing does not
    /// tessellate 18k of them again. In a cell because the paint phase cannot reach into
    /// the entity it is painting; one per view because each view has its own camera.
    paint: Rc<RefCell<render::Cache>>,
    /// Which nodes the projected view named last frame. Label placement is greedy, so
    /// without a memory of the previous frame the same real estate is contested afresh
    /// every tick: two labels a few px apart trade the spot back and forth and the
    /// picture pops. Remembered labels get first claim, which is hysteresis — the set
    /// only changes when something actually moves into a name's space.
    label_memory: Rc<RefCell<Vec<usize>>>,
    /// The temporary frame counter: how often the canvas is painted and what each paint
    /// costs. See [`bench`] for why it exists and why it is meant to be deleted.
    bench: Rc<RefCell<bench::Bench>>,
    /// The node the pointer is over, as an index into the frame's nodes. Its ring and
    /// the brighter colour of its links are the graph's answer to "which one am I
    /// pointing at", and `enter` opens it when the keyboard has not chosen another.
    hover: Option<usize>,
    /// The node the keyboard has chosen with `h`/`j`/`k`/`l`. Separate from the
    /// pointer's because gpui suppresses hover while the keyboard is what the user is
    /// using, and a keyboard user still needs something to open.
    selected: Option<usize>,
    /// What a held left button is doing, if anything.
    drag: Option<Drag>,
    /// Whether the filter strip is showing. `/` opens it, `escape` and `enter` close it,
    /// and it is the only thing in the tab with a text field, so it is also the only
    /// thing that takes the keyboard away from the graph.
    panel: bool,
    /// What the strip's chips show. The window owns the filter that does the work — its
    /// mask decides what the simulation holds — and this is the copy the chips are drawn
    /// from, pushed back on every refresh.
    filter: GraphFilter,
    /// The search line's text and caret.
    filter_field: Field,
    /// The strip's keyboard, so the `graph-filter` context can be the active one and the
    /// field can receive the letters a query is made of.
    panel_focus: FocusHandle,
    /// The window, because the simulation and the notes are its, not the view's: a
    /// node drag and a click both go through it. Weak — the window owns this view, and
    /// a strong handle back would be a cycle.
    workspace: WeakEntity<Workspace>,
}

impl GraphView {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        mode: Dim,
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
            building: false,
            frame: None,
            cloud: 0.0,
            camera: None,
            mode,
            cam3: None,
            orientation: None,
            pending_fit: Rc::default(),
            pending_fit3: Rc::default(),
            projection: Rc::default(),
            reframe: None,
            drift_armed: false,
            canvas: Rc::default(),
            paint: Rc::new(RefCell::new(render::Cache::new())),
            bench: Rc::new(RefCell::new(bench::Bench::new())),
            label_memory: Rc::new(RefCell::new(Vec::new())),
            hover: None,
            selected: None,
            drag: None,
            panel: false,
            // Everything on, which is what the window's default filter is and what an
            // empty strip with no query says.
            filter: GraphFilter::default(),
            filter_field: Field::new(""),
            panel_focus: cx.focus_handle(),
            workspace,
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

    /// Whether the window is scanning the vault right now. The tab is on screen before
    /// the scan's answer is, so this is the difference between an empty graph and a
    /// graph on its way.
    pub(crate) fn set_building(&mut self, building: bool, cx: &mut Context<Self>) {
        self.building = building;
        cx.notify();
    }

    /// The picture to paint. Shipped whenever the sim ticks or the graph changes
    /// underneath it; the view never reaches into the sim.
    ///
    /// A frame with a different number of nodes is a different graph — the index
    /// rebuilds its arena on a structural change, and the slot order that comes back
    /// is not the one that went in — so the pointer's and the keyboard's node are
    /// dropped rather than left pointing at whatever now occupies their index.
    pub(crate) fn set_frame(&mut self, frame: Rc<GraphFrame>, cx: &mut Context<Self>) {
        self.store_frame(frame);
        cx.notify();
    }

    /// The part of [`Self::set_frame`] that needs no context, so a frame that came back
    /// from the window inside one of the view's own handlers can be stored the same way.
    fn store_frame(&mut self, frame: Rc<GraphFrame>) {
        // The cloud's radius travels with the frame: the projected camera needs it for
        // the dolly's clamps, and it is cheaper to measure once per frame than per
        // wheel notch.
        self.cloud = frame.bounding_radius();
        let same_size = self
            .frame
            .as_ref()
            .is_some_and(|previous| previous.nodes.len() == frame.nodes.len());
        if !same_size {
            self.hover = None;
            self.selected = None;
            self.drag = None;
            // And the picture on the shelf goes with them. It was built from the graph
            // that was last painted, so its slots are that graph's slots: slot 3 of a
            // two-hop neighbourhood is not slot 3 of a four-hop one. Nothing may ask it
            // about this frame — not even which of the nodes it happens to know is
            // nearest — and the next paint builds a true one.
            *self.projection.borrow_mut() = None;
        }
        self.frame = Some(frame);
    }

    /// Where the view is looking, for the session file: the user's camera if they set
    /// one, and otherwise the framing the canvas last chose for itself.
    pub(crate) fn camera(&self) -> Option<Camera> {
        self.camera.or(self.pending_fit.borrow().and_then(|fit| {
            // A fit made against a canvas that had no size yet is not a view worth
            // remembering; the next frame fits again.
            (fit.zoom > MIN_ZOOM).then_some(fit)
        }))
    }

    /// Put the view back where the last session left it. Called once, while the tab is
    /// being restored, before anything has been painted.
    pub(crate) fn set_camera(&mut self, camera: Option<Camera>) {
        self.camera = camera;
    }

    /// Which space this view is showing.
    pub(crate) fn mode(&self) -> Dim {
        self.mode
    }

    /// `2` and `3`: turn the graph into the other space.
    ///
    /// The ask goes to the window first, because it is the window's **simulation** that
    /// changes space — one graph per window means one set of positions — and the picture
    /// that comes back is the switched one.
    fn turn(&mut self, action: &GraphMode, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == action.0 {
            return;
        }
        self.set_mode(action.0);
        if let Some((summary, frame)) = self
            .workspace
            .update(cx, |ws, cx| ws.graph_set_mode(action.0, cx))
            .ok()
            .flatten()
        {
            self.set_summary(summary, cx);
            self.store_frame(frame);
        }
        cx.notify();
    }

    /// `[` and `]`: the local graph's depth (M2b). The window owns the centre — which
    /// note the neighbourhood is around — and the view only asks for a step, because the
    /// centre is a fact about the window's tabs rather than about the picture.
    fn local_step(
        &mut self,
        action: &GraphLocalStep,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((summary, frame)) = self
            .workspace
            .update(cx, |ws, cx| ws.graph_local_step(action.0, cx))
            .ok()
            .flatten()
        {
            self.set_summary(summary, cx);
            self.store_frame(frame);
            // The chips say what the filter is, and the filter changed.
            if let Ok(filter) = self.workspace.update(cx, |ws, _cx| ws.graph_filter_state()) {
                self.adopt_filter(filter);
            }
        }
        cx.notify();
    }

    /// The palette's row and any config binding: set the local depth, or toggle it.
    fn local(&mut self, action: &GraphLocal, _window: &mut Window, cx: &mut Context<Self>) {
        match action.0 {
            None => self.toggle_local(cx),
            // `0` is the whole vault — the same spelling `:graph-global` and the chip
            // reaching off both use.
            Some(0) => {
                if let Some((summary, frame)) = self
                    .workspace
                    .update(cx, |ws, cx| ws.graph_local(None, cx))
                    .ok()
                    .flatten()
                {
                    self.set_summary(summary, cx);
                    self.store_frame(frame);
                }
                if let Ok(filter) = self.workspace.update(cx, |ws, _cx| ws.graph_filter_state()) {
                    self.adopt_filter(filter);
                }
                cx.notify();
            }
            Some(depth) => {
                if let Some((summary, frame)) = self
                    .workspace
                    .update(cx, |ws, cx| ws.graph_local(Some(depth), cx))
                    .ok()
                    .flatten()
                {
                    self.set_summary(summary, cx);
                    self.store_frame(frame);
                }
                if let Ok(filter) = self.workspace.update(cx, |ws, _cx| ws.graph_filter_state()) {
                    self.adopt_filter(filter);
                }
                cx.notify();
            }
        }
    }

    /// The neighbourhood moved under the user — a depth step, a re-centre, a toggle —
    /// so the camera drifts to frame it. The lean is toward the **note**, not the whole
    /// vault: the framing is of the neighbourhood the frame now holds, and the centre
    /// note sits at the middle of that by construction. Ruled by the person using it:
    /// expanding the hops in the projected view otherwise grows the cloud off the screen
    /// and there is no way to compare one depth against the next.
    fn followed_local(&mut self, before: &Option<shale_core::graph::filter::LocalScope>) {
        if before == &self.filter.local {
            return;
        }
        self.reframe_to_neighbourhood();
    }

    /// Take a filter the window or the strip answered with, and re-frame if the **local
    /// scope** was the part that moved. Every place the view's filter changes goes
    /// through here, so the drift can never be forgotten at a new entry point — and a
    /// search or kind change, which is not a move through the neighbourhood, leaves the
    /// camera exactly where the user left it.
    fn adopt_filter(&mut self, filter: GraphFilter) {
        let before = self.filter.local.clone();
        self.filter = filter;
        self.followed_local(&before);
    }

    /// Start the drift: snapshot where the camera is and where the neighbourhood wants
    /// it, and let `render` arm the frame loop. Nothing here moves the camera — the first
    /// move happens on a real frame, against a real `dt`.
    fn reframe_to_neighbourhood(&mut self) {
        // A drift needs a canvas to frame against and a frame to frame; both are checked
        // again on every step, because either can go away mid-chase.
        let (Some(_bounds), Some(frame)) = (*self.canvas.borrow(), self.frame.as_ref()) else {
            return;
        };
        let space = match self.mode {
            Dim::Two => {
                let Some(current) = self.camera() else {
                    return;
                };
                DriftSpace::Flat {
                    cam: current,
                    // The smoothed target starts at the camera, not at the framing.
                    // Seeding it with the fit would put the target a whole move ahead of
                    // the camera on frame one, and the spring would then travel to where
                    // the target *was* while the target kept moving — the camera would
                    // pass the framing and come back, which is the bounce the whole shape
                    // of this is meant to avoid. Starting at the camera keeps the target
                    // between the camera and its framing at every step, so the approach
                    // is one-directional.
                    target: current,
                    vel: [0.0; 3],
                }
            }
            Dim::Three => {
                let Some(current) = self.camera3() else {
                    return;
                };
                let positions: Vec<[f32; 3]> = frame
                    .nodes
                    .iter()
                    .map(|node| [node.position[0], node.position[1], node.depth])
                    .collect();
                // Seeded from where the camera already is, in both of its degrees of
                // freedom, so the first step of a drift moves nothing: the framing eases
                // away from the picture the user is looking at rather than snapping to
                // somewhere near it and easing back.
                let extent = Cam3 {
                    yaw: current.yaw,
                    pitch: current.pitch,
                    dist: current.dist,
                    ..Cam3::new()
                }
                .raw_extent(&positions);
                DriftSpace::Deep {
                    cam: current,
                    dist: current.dist,
                    dist_vel: 0.0,
                    extent,
                }
            }
        };
        self.reframe = Some(Drift {
            last: Instant::now(),
            frames: 0,
            still: 0,
            space,
        });
    }

    /// The step the frame loop runs: `dt` is the gap between drawn frames, clamped so a
    /// window that was hidden for a minute cannot fling the camera when it comes back.
    fn step_drift_timed(&mut self) -> bool {
        let dt = match self.reframe.as_ref() {
            Some(drift) => (Instant::now() - drift.last).as_secs_f32(),
            None => return false,
        };
        self.step_drift(dt.clamp(DRIFT_DT_MIN, DRIFT_DT_MAX))
    }

    /// One step of the drift over `dt` seconds: re-derive the framing from the frame as
    /// it is now, smooth the target toward it, and let the spring take up some of the
    /// gap. Returns whether the camera still has ground to cover.
    fn step_drift(&mut self, dt: f32) -> bool {
        let Some(mut drift) = self.reframe.take() else {
            return false;
        };
        let Some(bounds) = *self.canvas.borrow() else {
            return false;
        };
        drift.last = Instant::now();
        drift.frames += 1;
        let box_px = [
            bounds.origin.x.as_f32(),
            bounds.origin.y.as_f32(),
            bounds.size.width.as_f32(),
            bounds.size.height.as_f32(),
        ];
        let close = match &mut drift.space {
            DriftSpace::Flat { cam, target, vel } => {
                let Some(frame) = self.frame.as_ref() else {
                    return false;
                };
                let positions: Vec<[f32; 2]> =
                    frame.nodes.iter().map(|node| node.position).collect();
                let raw = Camera::fit(&positions, box_px, FIT_MARGIN);
                target.pan[0] = chase(target.pan[0], raw.pan[0], dt);
                target.pan[1] = chase(target.pan[1], raw.pan[1], dt);
                // The smoothing is for jitter, and a filter change is not jitter. A
                // framing may lag a shrink — pulling in late is invisible — but never a
                // growth, because a picture that outgrows its frame is exactly what the
                // eye reads as the camera jolting the wrong way.
                target.zoom = chase(target.zoom, raw.zoom, dt).min(raw.zoom);
                cam.pan[0] = spring(cam.pan[0], &mut vel[0], target.pan[0], dt);
                cam.pan[1] = spring(cam.pan[1], &mut vel[1], target.pan[1], dt);
                cam.zoom = spring_log(cam.zoom, &mut vel[2], target.zoom, dt);
                self.camera = Some(*cam);
                (raw.pan[0] - cam.pan[0]).abs() < DRIFT_DEAD_PAN
                    && (raw.pan[1] - cam.pan[1]).abs() < DRIFT_DEAD_PAN
                    && (raw.zoom - cam.zoom).abs() <= raw.zoom * DRIFT_DEAD_SCALE
            }
            DriftSpace::Deep {
                cam,
                dist,
                dist_vel,
                extent,
            } => {
                let Some(frame) = self.frame.as_ref() else {
                    return false;
                };
                let positions: Vec<[f32; 3]> = frame
                    .nodes
                    .iter()
                    .map(|node| [node.position[0], node.position[1], node.depth])
                    .collect();
                // Where the fit would put the eye for the content as it stands. The angle
                // is the user's and the camera it starts from is canonical, so this is
                // the same answer `f` gives.
                let mut raw = Cam3 {
                    yaw: cam.yaw,
                    pitch: cam.pitch,
                    ..Cam3::new()
                };
                raw.fit(&positions, box_px, FIT_MARGIN);

                // The framing input is the content's extent at the eye distance actually
                // reached this frame — the same distance the positions are drawn at — and
                // it is smoothed, because that is where the framing's ease lives.
                let here = Cam3 {
                    yaw: cam.yaw,
                    pitch: cam.pitch,
                    dist: cam.dist,
                    ..Cam3::new()
                };
                let (alo, ahi) = here.raw_extent(&positions);
                let eased = (
                    [
                        chase(extent.0[0], alo[0], dt),
                        chase(extent.0[1], alo[1], dt),
                    ],
                    [
                        chase(extent.1[0], ahi[0], dt),
                        chase(extent.1[1], ahi[1], dt),
                    ],
                );
                // The smoothing is for jitter, and a widened neighbourhood is not jitter.
                // The framing may lag a shrink — pulling in late is invisible — but it
                // must cover a growth the frame it happens, because a picture that
                // outgrows its frame is what the eye reads as the camera jolting the
                // wrong way: the report was "the camera zooms in before it zooms out",
                // and what was actually happening was the picture ballooning 36% past
                // its size while the camera stalled.
                *extent = (
                    [eased.0[0].min(alo[0]), eased.0[1].min(alo[1])],
                    [eased.1[0].max(ahi[0]), eased.1[1].max(ahi[1])],
                );

                // One spring, and the framing follows from it. Zoom and pan are not
                // sprung at all here: they are re-fitted from the extent at the distance
                // the eye has reached, which is what stops a node's dot and the picture
                // it sits in from being moved by two different springs at once.
                *dist = chase(*dist, raw.dist, dt);
                cam.dist = spring_log(cam.dist, dist_vel, *dist, dt);
                // MUTATION: framing answered at the distance the eye is heading for,
                // picture drawn at the distance it reached.
                let mut probe = Cam3 {
                    yaw: cam.yaw,
                    pitch: cam.pitch,
                    dist: *dist,
                    ..Cam3::new()
                };
                probe.fit_extent(extent.0, extent.1, box_px, FIT_MARGIN);
                cam.zoom = probe.zoom;
                cam.pan = probe.pan;
                self.cam3 = Some(*cam);
                (raw.pan[0] - cam.pan[0]).abs() < DRIFT_DEAD_PAN
                    && (raw.pan[1] - cam.pan[1]).abs() < DRIFT_DEAD_PAN
                    && (raw.zoom - cam.zoom).abs() <= raw.zoom * DRIFT_DEAD_SCALE
                    && (raw.dist - cam.dist).abs() <= raw.dist * DRIFT_DEAD_SCALE
            }
        };
        drift.still = if close { drift.still + 1 } else { 0 };
        let done = drift.still >= DRIFT_STILL_FRAMES || drift.frames >= DRIFT_MAX_FRAMES;
        if !done {
            self.reframe = Some(drift);
        }
        !done
    }

    /// Arm the chain of next-frame callbacks that drives the drift. Called from `render`,
    /// which is where the frame's `Window` is within reach.
    fn arm_drift(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drift_armed = true;
        Self::drift_frame(window, cx.entity().downgrade());
    }

    /// One link in the chain. Re-arming is the loop; **not** re-arming is how the drift
    /// ends — and stopping the re-arm is what lets the window go back to idle, so a
    /// settled camera never leaves a graph painting itself for no one.
    fn drift_frame(window: &mut Window, weak: gpui::WeakEntity<Self>) {
        window.on_next_frame(move |window, cx| {
            let more = weak
                .update(cx, |view, cx| {
                    let more = view.step_drift_timed();
                    if !more {
                        view.drift_armed = false;
                    }
                    cx.notify();
                    more
                })
                .unwrap_or(false);
            if more {
                Self::drift_frame(window, weak);
            }
        });
    }

    /// Any camera input from the user ends the drift: the view stops moving on its own
    /// the moment they touch it.
    fn cancel_reframe(&mut self) {
        // The pending callback runs, finds no drift, and stops re-arming. Clearing
        // `drift_armed` here would arm a second chain alongside it.
        self.reframe = None;
    }

    /// The view's own half of a mode change: which space to draw and hit-test in. The
    /// other half — running the physics in that space — is the window's, and it is the
    /// window that calls this when a session is being restored, where the sim is already
    /// being built in the right space and the view only has to agree.
    ///
    /// The projection is dropped rather than re-projected: it belongs to the old space,
    /// and the next frame builds the new one from the frame the window ships.
    pub(crate) fn set_mode(&mut self, dim: Dim) {
        if self.mode == dim {
            return;
        }
        self.mode = dim;
        *self.projection.borrow_mut() = None;
    }

    /// The node that stands for the note the window is showing, when the graph has been
    /// narrowed to a neighbourhood around it. A lookup by path rather than a remembered
    /// slot, because an index rebuild hands back a different slot order under the same
    /// path — and the halo has to follow the note, not the number.
    fn centre_slot(&self) -> Option<usize> {
        let center = self.filter.local.as_ref()?.center.as_str();
        self.frame
            .as_ref()?
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| node.path.as_deref() == Some(center))
            .map(|(slot, _)| slot)
    }

    /// Whether the view is currently moving its own camera.
    #[cfg(test)]
    pub(crate) fn reframing(&self) -> bool {
        self.reframe.is_some()
    }

    /// What the next paint would draw for a node: its screen centre and its drawn
    /// radius, in whichever space is on screen. The test asks what the user would see,
    /// not what the camera holds, because the complaint was about dots resizing.
    #[cfg(test)]
    pub(crate) fn drawn_node(&self, path: &str) -> Option<([f32; 2], f32)> {
        let frame = self.frame.as_ref()?;
        let slot = frame.nodes.iter().position(|n| n.path.as_deref() == Some(path))?;
        let node = &frame.nodes[slot];
        match self.mode {
            Dim::Two => {
                let camera = self.camera()?;
                Some((camera.to_screen(node.position), node.radius * camera.zoom))
            }
            Dim::Three => {
                let camera = self.camera3()?;
                let p = camera.project([node.position[0], node.position[1], node.depth]);
                Some((p.xy, proj::drawn_radius(node.radius, p.scale)))
            }
        }
    }

    /// How wide the whole picture is on screen, in px — the other half of the ratio a
    /// test wants. In the projected view this is the zoom and the perspective factor
    /// together, which is exactly what the eye reads as "the zoom".
    #[cfg(test)]
    pub(crate) fn drawn_spread(&self) -> Option<f32> {
        let frame = self.frame.as_ref()?;
        let xs: Vec<f32> = match self.mode {
            Dim::Two => {
                let camera = self.camera()?;
                frame
                    .nodes
                    .iter()
                    .map(|n| camera.to_screen(n.position)[0])
                    .collect()
            }
            Dim::Three => {
                let camera = self.camera3()?;
                frame
                    .nodes
                    .iter()
                    .map(|n| camera.project([n.position[0], n.position[1], n.depth]).xy[0])
                    .collect()
            }
        };
        Some(
            xs.iter()
                .fold(f32::MIN, |a, &b| a.max(b))
                - xs.iter().fold(f32::MAX, |a, &b| a.min(b)),
        )
    }

    /// Where the running drift is heading, for the tests that watch it arrive.
    #[cfg(test)]
    pub(crate) fn drift_target(&self) -> Option<([f32; 2], f32, f32)> {
        self.reframe.as_ref().map(|d| match &d.space {
            DriftSpace::Flat { target, .. } => (target.pan, target.zoom, 0.0),
            DriftSpace::Deep { cam, dist, .. } => (cam.pan, cam.zoom, *dist),
        })
    }

    /// One step of the drift, driven by hand at a fixed 60 Hz: the test cares where the
    /// camera arrives, not what the frame clock happened to give the real loop.
    #[cfg(test)]
    pub(crate) fn drift_step(&mut self) -> bool {
        self.step_drift(1.0 / 60.0)
    }

    #[cfg(test)]
    pub(crate) fn centre_mark(&self) -> Option<usize> {
        self.centre_slot()
    }

    /// Where a node was painted in the last projected frame. The honest door for the
    /// tests that aim the pointer: it reports the very points the paint used, not a
    /// re-projection that could have drifted from them.
    #[cfg(test)]
    pub(crate) fn projection_slot(&self, slot: usize) -> Option<[f32; 2]> {
        (*self.projection.borrow()).as_ref()?.screen(slot)
    }

    /// The projected view's answer to "which node is under here", asked of a frame the
    /// view is not holding. The test that pins the stale-projection crash needs to put a
    /// bigger graph in front of the picture on the shelf, which is exactly what a pointer
    /// event does when it lands between a frame arriving and that frame being painted.
    #[cfg(test)]
    pub(crate) fn projection_hit(&self, frame: &GraphFrame, screen: [f32; 2]) -> Option<usize> {
        let projection = (*self.projection.borrow()).clone()?;
        projection.node_at(frame, screen, HIT_SLOP_PX)
    }

    /// Where the projected view is looking: the user's camera if they turned or panned
    /// it, and otherwise the framing the canvas last chose for itself.
    pub(crate) fn camera3(&self) -> Option<Cam3> {
        self.cam3.or(*self.pending_fit3.borrow())
    }

    /// Restore the projected view's **angle** from the last session, leaving the framing
    /// to be re-derived. `None` is the default orientation.
    pub(crate) fn set_orientation(&mut self, yaw_pitch: Option<(f32, f32)>) {
        self.orientation = yaw_pitch;
    }

    /// The middle of the canvas in window-absolute px, which is the space the camera
    /// was fitted against. `None` before the first frame has been laid out.
    fn canvas_center(&self) -> Option<[f32; 2]> {
        let bounds = (*self.canvas.borrow())?;
        Some([
            (bounds.origin.x + bounds.size.width / 2.0).as_f32(),
            (bounds.origin.y + bounds.size.height / 2.0).as_f32(),
        ])
    }

    /// The camera this frame is being painted with, and the world point a window px
    /// lands on with it. `None` until there is a camera to ask.
    fn world_at(&self, screen: Point<Pixels>) -> Option<([f32; 2], Camera)> {
        let camera = self.camera()?;
        Some((
            camera.to_world([screen.x.as_f32(), screen.y.as_f32()]),
            camera,
        ))
    }

    /// The node under the pointer, generous by [`HIT_SLOP_PX`] screen px so a small dot
    /// is still hittable when the whole vault is in view.
    fn node_at(&self, screen: Point<Pixels>) -> Option<usize> {
        let frame = self.frame.as_ref()?;
        match self.mode {
            Dim::Two => {
                let (world, camera) = self.world_at(screen)?;
                frame.node_at(world, HIT_SLOP_PX / camera.zoom)
            }
            Dim::Three => {
                // The projected view answers in **screen** px, against the very points
                // the last frame painted — so the slop needs no division by the zoom,
                // and the answer cannot disagree with what is on screen.
                let projection = (*self.projection.borrow()).clone()?;
                projection.node_at(frame, [screen.x.as_f32(), screen.y.as_f32()], HIT_SLOP_PX)
            }
        }
    }

    /// The world point a screen px lands on at a node's own view depth — the projected
    /// drag's inverse. The depth is the invariant while the world turns under the
    /// cursor, which is what keeps a dragged node under the pointer instead of sliding
    /// off it. `None` while there is no projection to ask.
    fn world_at_depth(&self, screen: Point<Pixels>, slot: usize) -> Option<[f32; 3]> {
        let camera = self.camera3()?;
        let view_z = (*self.projection.borrow()).as_ref()?.view_z(slot)?;
        Some(camera.screen_to_world_at_depth(
            [screen.x.as_f32(), screen.y.as_f32()],
            view_z,
        ))
    }

    /// The node the pointer's press should act on, and the keyboard's answer to the
    /// same question: the keyboard's choice wins, because a user who has moved a
    /// selection with `hjkl` means it.
    fn chosen(&self) -> Option<usize> {
        self.selected.or(self.hover)
    }

    /// The button went down on the canvas: over a node the left button starts a drag of
    /// that node, over the background a drag of the view. The middle button drags the
    /// view wherever it lands, node or not — it is the pan button, and a hand reaching
    /// for it wants the picture to move, not a hub pinned and shoved.
    fn canvas_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A hand on the canvas is the end of the view moving itself.
        self.cancel_reframe();
        let over = self.node_at(event.position);
        self.hover = over;
        let node = if matches!(event.button, MouseButton::Middle) {
            None
        } else {
            over
        };
        self.drag = Some(Drag {
            start: event.position,
            last: event.position,
            began: false,
            node,
            button: event.button,
        });
        cx.notify();
    }

    /// The pointer moved. With the button down that is a drag — a node follows the
    /// pointer, the background moves the view under it — and without it, hover.
    fn canvas_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = self.drag.as_mut() {
            let step = [
                (event.position.x - drag.last.x).as_f32(),
                (event.position.y - drag.last.y).as_f32(),
            ];
            drag.last = event.position;
            match drag.node {
                Some(slot) => {
                    // A press on a node becomes a drag only past the slop, and it begins
                    // by pinning the node **where it already is** — not where the pointer
                    // is. Pinning at the pointer is what made an unsteady click throw the
                    // layout around, worst of all in the projected view, where the world
                    // point under the cursor at the node's depth has nothing to do with
                    // the node.
                    if !drag.began {
                        let travelled = (event.position.x - drag.start.x).as_f32().abs()
                            + (event.position.y - drag.start.y).as_f32().abs();
                        if travelled <= CLICK_SLOP {
                            return;
                        }
                        drag.began = true;
                        if let Some(node) = self.frame.as_ref().and_then(|f| f.nodes.get(slot)) {
                            let _ = self.workspace.update(cx, |ws, cx| {
                                ws.graph_drag_node(slot, [node.position[0], node.position[1], node.depth], cx);
                            });
                        }
                    }
                    let world = match self.mode {
                        Dim::Two => self.world_at(event.position).map(|(world, _)| [world[0], world[1], 0.0]),
                        Dim::Three => self.world_at_depth(event.position, slot),
                    };
                    if let Some(world) = world {
                        let _ = self.workspace.update(cx, |ws, cx| {
                            ws.graph_drag_node(slot, world, cx);
                        });
                    }
                }
                None => {
                    // The middle button pans in both spaces — it is the pan button, and
                    // its meaning does not move with the mode. Over the background the
                    // left button pans the flat view and **orbits** the projected one:
                    // turning the world is what makes the third axis legible, and a
                    // left drag over empty space has nothing else it could mean.
                    let middle = matches!(drag.button, MouseButton::Middle);
                    if self.mode == Dim::Three && !middle {
                        if let Some(mut camera) = self.camera3() {
                            camera.orbit_by(step[0], step[1]);
                            self.cam3 = Some(camera);
                        }
                    } else if self.mode == Dim::Three {
                        if let Some(mut camera) = self.camera3() {
                            camera.pan_by(step);
                            self.cam3 = Some(camera);
                        }
                    } else if let Some(mut camera) = self.camera() {
                        camera.pan_by(step);
                        self.camera = Some(camera);
                    }
                }
            }
            cx.notify();
            return;
        }
        let over = self.node_at(event.position);
        if over != self.hover {
            self.hover = over;
            cx.notify();
        }
    }

    /// The button came back up, inside the canvas or outside it. A press that never
    /// moved and landed on a node is a click, and a click opens that note.
    fn canvas_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let Some(node) = drag.node else {
            cx.notify();
            return;
        };
        // A press that never became a drag is a click, and a click on a node opens the
        // note it stands for. One threshold decides both halves, so the two can never
        // disagree about where the line is.
        if !drag.began {
            self.open_node(node, cx);
            cx.notify();
            return;
        }
        // Shift means "leave it where I put it": the node stays pinned against the
        // forces, which is how the user arranges a graph by hand.
        let _ = self.workspace.update(cx, |ws, cx| {
            ws.graph_release_node(node, event.modifiers.shift, cx);
        });
        cx.notify();
    }

    /// The wheel: zoom anchored at the cursor, which is the one zoom that does not make
    /// the thing being looked for leave the screen — in **both** spaces, because a
    /// gesture that means one thing in the flat view and another thing three axes over
    /// is a gesture the user has to relearn every time they press `3`.
    ///
    /// `ctrl` + wheel is the projected space's **dolly**: it moves the eye along the
    /// view axis instead of magnifying the picture, which deepens or flattens the
    /// perspective. Kept off the bare wheel on purpose — the user's word for what the
    /// dolly felt like was "fisheye", because perspective changing under a zoom is a
    /// movement the eye reads as distortion rather than as travel.
    fn canvas_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_reframe();
        let notches = match event.delta {
            // The delta is passed through with its own sign, which on the real backends
            // means a **downward** wheel zooms **out** and an upward one zooms in — the
            // direction the user asked for, and the one gpui's own scrollable div reads
            // the same way. A pixel delta (a trackpad) is divided into notches so it
            // stays smooth rather than jumping.
            ScrollDelta::Lines(delta) => delta.y,
            ScrollDelta::Pixels(delta) => delta.y.as_f32() / WHEEL_PX_PER_NOTCH,
        };
        if notches == 0.0 {
            return;
        }
        let factor = ZOOM_STEP.powf(notches.clamp(-WHEEL_NOTCH_CLAMP, WHEEL_NOTCH_CLAMP));
        let anchor = [event.position.x.as_f32(), event.position.y.as_f32()];
        match self.mode {
            Dim::Two => {
                let Some(mut camera) = self.camera() else {
                    return;
                };
                camera.zoom_at(anchor, factor);
                self.camera = Some(camera);
            }
            Dim::Three => {
                let Some(mut camera) = self.camera3() else {
                    return;
                };
                if event.modifiers.control {
                    // Travel, bounded by the cloud's own radius, so the eye can never
                    // cross into it. Inverted because moving the eye *closer* is what
                    // scrolling up means.
                    camera.dolly_by(self.cloud, 1.0 / factor);
                } else {
                    // core's 3D zoom anchors on the **projected** point, so the node
                    // under the cursor is the one that stays under it, at its own depth.
                    camera.zoom_at(anchor, factor);
                }
                self.cam3 = Some(camera);
            }
        }
        cx.notify();
    }

    /// `+` and `-`: the same zoom as the wheel, anchored at the middle of the canvas
    /// because the keyboard has no cursor to anchor it at.
    fn zoom(&mut self, action: &GraphZoom, _window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_reframe();
        let Some(center) = self.canvas_center() else {
            return;
        };
        match self.mode {
            Dim::Two => {
                let Some(mut camera) = self.camera() else {
                    return;
                };
                camera.zoom_at(center, action.0);
                self.camera = Some(camera);
            }
            Dim::Three => {
                // Magnify, like the flat view: the keyboard has no cursor to anchor at,
                // so the canvas center stands in for it.
                let Some(mut camera) = self.camera3() else {
                    return;
                };
                camera.zoom_at(center, action.0);
                self.cam3 = Some(camera);
            }
        }
        cx.notify();
    }

    /// The arrows: move the **camera**, so the picture slides the other way — pressing
    /// `right` takes the view right, which moves the notes left under it, exactly as a
    /// scrollbar behaves when you move the thumb. The action's vector is a direction in
    /// world space (`right` is +x), which is why the sign flips here rather than in the
    /// binding table.
    ///
    /// This is deliberately the opposite of dragging the background, which grabs the
    /// picture itself and moves it with the pointer.
    ///
    /// Once the user has moved the camera, the automatic framing is over — that is what
    /// makes `camera` mean "the user's".
    fn pan(&mut self, action: &GraphPan, _window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_reframe();
        let [dx, dy] = action.0;
        match self.mode {
            Dim::Two => {
                let Some(mut camera) = self.camera() else {
                    return;
                };
                camera.pan_by([-dx, -dy]);
                self.camera = Some(camera);
            }
            Dim::Three => {
                let Some(mut camera) = self.camera3() else {
                    return;
                };
                camera.pan_by([-dx, -dy]);
                self.cam3 = Some(camera);
            }
        }
        cx.notify();
    }

    /// `h`/`j`/`k`/`l`: move the keyboard's node selection. With nothing chosen yet,
    /// the node nearest the middle of the screen is the one to start from, which is
    /// what a user looking at the graph would call "the one in front of me".
    fn select(&mut self, action: &GraphSelect, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(frame) = self.frame.as_ref().cloned() else {
            return;
        };
        let from = match self.chosen() {
            Some(slot) => slot,
            None => {
                let Some(center) = self.canvas_center() else {
                    return;
                };
                let found = match self.mode {
                    Dim::Two => self
                        .camera()
                        .and_then(|camera| frame.nearest(camera.to_world(center))),
                    // The projected view asks the same question in screen px: the node
                    // nearest the middle of what is on screen, not of the world.
                    Dim::Three => (*self.projection.borrow())
                        .as_ref()
                        .and_then(|projection| projection.nearest(center)),
                };
                match found {
                    Some(slot) => slot,
                    None => return,
                }
            }
        };
        // Nothing in that direction is not a reason to lose the selection: the user
        // keeps the node they had and can try the other way. This is also what makes
        // the first press of any direction choose something.
        // The directions are screen directions in the projected view: `l` means "the
        // next node to the right of what I have", which is what a hand on `l` means in
        // a world that rotates.
        let next = match self.mode {
            Dim::Two => frame.node_toward(from, action.0),
            Dim::Three => (*self.projection.borrow())
                .as_ref()
                .and_then(|projection| projection.node_toward(from, action.0)),
        }
        .unwrap_or(from);
        if Some(next) != self.selected {
            self.selected = Some(next);
            cx.notify();
        }
    }

    /// `f`: frame the whole graph again, which is what clearing the user's camera
    /// means — while there is none, every frame re-fits.
    ///
    /// Both spaces, because `f` means the same thing whichever is on screen. In the
    /// projected one the **angle survives** and only the framing is redone: `f` is the
    /// recovery key for a view that has drifted off the graph, and losing the angle the
    /// user was looking at would make it a way to get lost rather than found.
    fn fit(&mut self, _: &GraphFit, _window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_reframe();
        self.camera = None;
        // In the projected view the angle survives and only the framing is redone.
        // Setting the orientation and clearing the camera is exactly what a restored
        // session does, so "this angle, framed fresh" is one path rather than two.
        if let Some(camera) = self.camera3() {
            self.orientation = Some((camera.yaw, camera.pitch));
        }
        self.cam3 = None;
        *self.projection.borrow_mut() = None;
        cx.notify();
    }

    /// `enter`: open the chosen node's note. A ghost or a tag has no note to open, and
    /// quietly doing nothing is better than a bar message on every press; making a note
    /// from a ghost is the same open thread as making one from an unresolved link in
    /// prose.
    fn open(&mut self, _: &GraphOpen, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(node) = self.chosen() else {
            return;
        };
        self.open_node(node, cx);
    }

    /// The one door from a node to a note.
    fn open_node(&mut self, slot: usize, cx: &mut Context<Self>) {
        let Some(path) = self.frame.as_ref().and_then(|frame| frame.node_path(slot)) else {
            return;
        };
        let _ = self.workspace.update(cx, |ws, cx| {
            ws.graph_open_note(&path, cx);
        });
    }

    /// Where the keyboard should go when the window hands this tab its focus: the
    /// filter strip's field while the strip is open, the graph itself otherwise.
    ///
    /// Every "give the keyboard back to wherever the user was working" path asks the
    /// window for this. A graph tab with its strip open has two keyboards, and the one
    /// the user is looking at is the field — so a tab change made with the mouse, or a
    /// popup closing, has to land on the field or the strip is left looking ready and
    /// deaf.
    pub(crate) fn focus_target(&self) -> FocusHandle {
        if self.panel {
            self.panel_focus.clone()
        } else {
            self.focus_handle.clone()
        }
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

    /// The node the pointer is over, for the tests that aim it.
    #[cfg(test)]
    pub(crate) fn hover(&self) -> Option<usize> {
        self.hover
    }

    /// The node the keyboard has chosen, for the tests that move it.
    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The canvas's bounds as the last frame laid them out, for the tests that put the
    /// pointer in the middle of the graph.
    #[cfg(test)]
    pub(crate) fn canvas(&self) -> Option<Bounds<Pixels>> {
        *self.canvas.borrow()
    }
}

impl Focusable for GraphView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// What the canvas decided this frame is drawn in, handed from prepaint to paint.
///
/// One enum rather than two canvases, because the tab is one element and the mode is
/// per-window state: switching spaces is a re-render of the same canvas, not a swap of
/// elements that would throw away the link cache and the canvas's layout.
enum Space {
    /// The flat graph, through the 2D camera.
    Flat(Camera),
    /// The projected graph — or nothing, while there is no frame to project.
    Deep(Option<Rc<proj::Projection>>),
}

/// One count in the summary row: the number in the node colour, its label in the
/// chrome's quiet colour, so the row reads as data with captions rather than prose.
fn stat(palette: &Palette, value: impl std::fmt::Display, label: &'static str) -> gpui::Div {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A drift is a chain of next-frame callbacks: one step per drawn frame, with the
        // camera moved before the frame is laid out and painted, so the picture the user
        // sees is the picture the camera was computed against. It is armed here because
        // this is where the frame's `Window` is within reach; the chain re-arms itself
        // while the camera has ground to cover and stops when it doesn't, which is what
        // returns the window to idle.
        if self.reframe.is_some() && !self.drift_armed {
            self.arm_drift(window, cx);
        }
        let palette = Rc::clone(&self.palette);
        let frame = self.frame.clone();
        // The two closures each need their own handle on the snapshot: prepaint only
        // wants the positions, paint wants the whole picture.
        let painted = frame.clone();
        let camera = self.camera;
        let pending_fit = Rc::clone(&self.pending_fit);
        let mode = self.mode;
        let cam3 = self.cam3;
        let orientation = self.orientation;
        let pending_fit3 = Rc::clone(&self.pending_fit3);
        let projection_cell = Rc::clone(&self.projection);
        let canvas_bounds = Rc::clone(&self.canvas);
        let bench = Rc::clone(&self.bench);
        let paint = Rc::clone(&self.paint);
        let label_memory = Rc::clone(&self.label_memory);
        let look = render::Look {
            palette: Rc::clone(&self.palette),
            hover: self.chosen(),
            centre: self.centre_slot(),
            labels: render::Labels {
                font: gpui::font(self.font.clone()),
                size: self.font_size,
            },
        };

        // The benchmark readout: the rate and cost of the last full second the canvas
        // was drawn. Temporary, and only in the way a tape measure is temporary — the
        // exit criteria for this milestone are a frame rate, and there is no profiler
        // out here. See `bench.rs`.
        let fps = self.bench.borrow_mut().fps();
        let paint_ms = self.bench.borrow_mut().paint_ms();

        // The summary, or the reason there is not one. A graph that has been built but
        // is empty says "0 notes", which is a different fact from "nothing has been
        // built", and a third from "the vault is being read right now" — the scan runs on
        // a background thread, so the tab is on screen before there is a picture.
        let body = match self.summary {
            None => div()
                .text_color(gpui::rgb(palette.ui_dim))
                .child(if self.building {
                    "Building the graph…"
                } else {
                    "The graph has not been built yet."
                }),
            Some(counts) => {
                let mut row = div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(stat(&palette, counts.notes, "notes"))
                    .child(stat(&palette, counts.links, "links"))
                    .child(stat(&palette, counts.tags, "tags"))
                    .child(stat(&palette, counts.ghosts, "missing"));
                if let Some(fps) = fps {
                    row = row.child(stat(&palette, format!("{fps:.0}"), "fps"));
                }
                if let Some(paint_ms) = paint_ms {
                    row = row.child(stat(&palette, format!("{paint_ms:.2}"), "ms paint"));
                }
                row
            }
        };
        // The strip is built before the row below, because the field it holds needs this
        // entity to be registered with the input handler during this frame's paint.
        let panel = self.filter_panel(cx);
        div()
            // The graph's own keys are *handled* here, but their context lives on the
            // canvas below. gpui's context stack is the focused element's ancestors, so a
            // `graph` context on this div would sit under the search field too, and a
            // query could not contain a single one of `hjkl`, `f`, `+` or `-`.
            .on_action(cx.listener(Self::zoom))
            .on_action(cx.listener(Self::pan))
            .on_action(cx.listener(Self::select))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::fit))
            .on_action(cx.listener(Self::turn))
            .on_action(cx.listener(Self::local_step))
            .on_action(cx.listener(Self::local))
            // `/` is handled on the window rather than on the canvas because the strip it
            // opens is a sibling of the canvas, and the handler is the same either way.
            .on_action(cx.listener(Self::open_panel))
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .text_color(gpui::rgb(palette.ui_fg))
            .child(
                div()
                    .flex_none()
                    .text_size(crate::style::TEXT_LG)
                    .font_weight(gpui::FontWeight::BOLD)
                    // The label is drawn in the colour the graph's own text will use,
                    // so the one thing on screen that is already the graph is painted
                    // with the graph's palette.
                    .text_color(gpui::rgb(palette.graph_text))
                    .child("Graph"),
            )
            .child(body)
            .when(self.panel, |d| d.child(panel))
            // The canvas: the graph itself, drawn with the paint API rather than laid
            // out as divs. The wrapper occludes, so a picker or popup over the graph
            // cannot be hovered through, and it is the wrapper that holds the pointer:
            // the canvas element paints and takes no events. The canvas carries the
            // border that makes the frame visible even in an empty vault.
            //
            // It is also what carries the `graph` key context and the tab's focus, for
            // the reason above: the filter strip is a sibling, and its field has to be
            // able to type the letters this context would otherwise swallow.
            .child({
                // The canvas is what holds the graph tab's keyboard, so it is what
                // carries the focus rule: `ui_active_border` while it has the
                // keyboard, `ui_border` while the filter strip's field does
                // (`design-language.md` §4).
                let border = gpui::rgb(if self.focus_handle.is_focused(window) {
                    palette.ui_active_border
                } else {
                    palette.ui_border
                });
                div()
                    .key_context(CONTEXT)
                    .track_focus(&self.focus_handle)
                    .flex_1()
                    .min_h_0()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::canvas_down))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::canvas_up))
                    // A button released outside the canvas still has to let go of the
                    // node it was holding, or the graph is left with a body pinned to
                    // nothing.
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::canvas_up))
                    // The middle button pans (roadmap P3.4). It shares the canvas
                    // handlers with the left one rather than having its own, because the
                    // drag it starts is the background drag the left button already
                    // makes: grab the picture, move it with the pointer.
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::canvas_down))
                    .on_mouse_up(MouseButton::Middle, cx.listener(Self::canvas_up))
                    .on_mouse_up_out(MouseButton::Middle, cx.listener(Self::canvas_up))
                    .on_mouse_move(cx.listener(Self::canvas_move))
                    .on_scroll_wheel(cx.listener(Self::canvas_wheel))
                    .child(
                        canvas(
                            move |bounds, _window, _cx| {
                                // Where the canvas is, for the keyboard's zoom and for
                                // the tests that aim the pointer at a node.
                                *canvas_bounds.borrow_mut() = Some(bounds);
                                let box_px = [
                                    bounds.origin.x.as_f32(),
                                    bounds.origin.y.as_f32(),
                                    bounds.size.width.as_f32(),
                                    bounds.size.height.as_f32(),
                                ];
                                match mode {
                                    Dim::Two => {
                                        // The camera for this frame: the user's, or a
                                        // first look that fits the whole graph into what
                                        // there is to draw it in.
                                        // The camera for this frame: the user's, or a
                                        // first look that fits the whole graph into what
                                        // there is to draw it in.
                                        let fitted = match camera {
                                            Some(camera) => camera,
                                            None => {
                                                let positions = frame
                                                    .as_ref()
                                                    .map(|frame| frame.positions())
                                                    .unwrap_or_default();
                                                let camera =
                                                    Camera::fit(&positions, box_px, FIT_MARGIN);
                                                *pending_fit.borrow_mut() = Some(camera);
                                                camera
                                            }
                                        };
                                        Space::Flat(fitted)
                                    }
                                    Dim::Three => {
                                        // The same rule in three axes: the user's camera
                                        // if they turned, panned or zoomed it, and
                                        // otherwise a framing re-derived from the layout
                                        // at the angle the last session left. No
                                        // "middle of nowhere" guard is needed here,
                                        // because the session remembers only the angle
                                        // and the framing is always re-fitted — which is
                                        // the whole point of that rule.
                                        let Some(frame) = frame.as_ref() else {
                                            *projection_cell.borrow_mut() = None;
                                            return Space::Deep(None);
                                        };
                                        let positions: Vec<[f32; 3]> = frame
                                            .nodes
                                            .iter()
                                            .map(|node| {
                                                [node.position[0], node.position[1], node.depth]
                                            })
                                            .collect();
                                        let camera = match cam3 {
                                            Some(camera) => camera,
                                            None => {
                                                let mut camera = match orientation {
                                                    Some((yaw, pitch)) => Cam3 {
                                                        yaw,
                                                        pitch,
                                                        ..Cam3::new()
                                                    },
                                                    None => Cam3::new(),
                                                };
                                                camera.fit(&positions, box_px, FIT_MARGIN);
                                                *pending_fit3.borrow_mut() = Some(camera);
                                                camera
                                            }
                                        };
                                        // One projection per frame, shared with the
                                        // pointer's handlers: the paint and the hit test
                                        // answer from the same projected points.
                                        let projection =
                                            Rc::new(proj::Projection::build(frame, &camera));
                                        *projection_cell.borrow_mut() =
                                            Some(Rc::clone(&projection));
                                        Space::Deep(Some(projection))
                                    }
                                }
                            },
                            move |bounds, space, window, cx| {
                                // Timed, not sampled: the number the strip shows is the
                                // cost of the paints themselves, not of the frame around
                                // them.
                                let started = Instant::now();
                                match space {
                                    Space::Flat(camera) => {
                                        if let Some(frame) = &painted {
                                            render::paint_graph(
                                                bounds,
                                                camera,
                                                frame,
                                                &look,
                                                &mut paint.borrow_mut(),
                                                window,
                                                cx,
                                            );
                                        }
                                    }
                                    Space::Deep(Some(projection)) => {
                                        if let Some(frame) = &painted {
                                            proj::paint_graph3d(
                                                bounds,
                                                frame,
                                                &projection,
                                                &look,
                                                &mut paint.borrow_mut(),
                                                &label_memory,
                                                window,
                                                cx,
                                            );
                                        }
                                    }
                                    Space::Deep(None) => {}
                                }
                                bench.borrow_mut().time(started);
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
