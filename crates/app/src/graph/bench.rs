//! A **temporary** frame counter for benchmarking the graph tab.
//!
//! P3.4's exit criteria are numbers, not vibes: about 60 fps while the layout is
//! moving, and zero frames once it is at rest. Checking them on a real vault should not
//! need a profiler attached to a headless harness, so the graph tab reports its own
//! rate: how often the canvas was drawn, and how long each draw took. That is the
//! difference between "the graph is slow" and "the graph is drawing 60 times a second
//! and each paint costs 0.4 ms" — two different problems with two different fixes.
//!
//! It is a module of its own so that deleting it later is one file and a handful of
//! lines rather than an excavation. Nothing here is a feature.

use std::time::{Duration, Instant};

/// How long a measurement window is. A second is a rate a person can recognise, and it
/// is short enough that the numbers appear as soon as the graph starts moving.
const WINDOW: Duration = Duration::from_millis(1000);

/// The canvas's paint calls, counted over a rolling [`WINDOW`].
///
/// It lives in a cell inside [`super::GraphView`] for the same reason the camera fit
/// and the canvas bounds do: the paint phase cannot reach into the entity it is
/// painting, and it must never `cx.notify()` — that would ask for the frame that is
/// already being drawn.
pub(crate) struct Bench {
    /// Paints counted since the current window opened.
    frames: u32,
    /// Paint time accumulated since the current window opened.
    painted: Duration,
    /// When the current window opened.
    since: Instant,
    /// Frames per second over the last complete window. `None` until one has closed.
    fps: Option<f32>,
    /// Mean paint time over the last complete window, in milliseconds.
    paint_ms: Option<f32>,
}

impl Bench {
    /// A counter whose first window starts now.
    pub(crate) fn new() -> Self {
        Self {
            frames: 0,
            painted: Duration::ZERO,
            since: Instant::now(),
            fps: None,
            paint_ms: None,
        }
    }

    /// Close out one paint that began at `started`: count it, then roll the window if a
    /// second has gone by.
    pub(crate) fn time(&mut self, started: Instant) {
        self.frames += 1;
        self.painted += started.elapsed();
        self.roll();
    }

    /// The rate over the last complete window, in frames per second.
    ///
    /// A window also closes here, which is what lets an idle graph report `0 fps`
    /// instead of the rate it held when it last moved. What it cannot do is *redraw*
    /// the number while nothing is drawing — there is no timer behind this, on purpose.
    pub(crate) fn fps(&mut self) -> Option<f32> {
        self.roll();
        self.fps
    }

    /// The mean paint time over the last complete window, in milliseconds.
    pub(crate) fn paint_ms(&mut self) -> Option<f32> {
        self.roll();
        self.paint_ms
    }

    /// End the current window if it is at least [`WINDOW`] old, publishing its rate and
    /// mean cost, and start the next one.
    fn roll(&mut self) {
        let elapsed = self.since.elapsed();
        if elapsed < WINDOW {
            return;
        }
        // A window with no paints says "0 fps", which is the idle contract being
        // observed rather than a missing number.
        let frames = self.frames.max(1) as f32;
        self.fps = Some(self.frames as f32 / elapsed.as_secs_f32());
        self.paint_ms = Some(self.painted.as_secs_f32() * 1000.0 / frames);
        self.frames = 0;
        self.painted = Duration::ZERO;
        self.since = Instant::now();
    }
}
