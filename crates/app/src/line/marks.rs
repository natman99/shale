//! The callout marks: the drawn glyph that says **what kind** a callout box is.
//!
//! A callout's type is carried entirely by colour today. Live preview hides the `>`
//! markers and the `[!type]` header (`design-language.md` §8's "second channel" case),
//! leaving a 2px bar — and the palette maps 14 kinds onto 10 colours, with four pairs
//! sharing one outright (`info`/`todo`, `abstract`/`example`, `tip`/`success`,
//! `danger`/`bug`) and `other` landing on the same blue as `info`. So a reader who is
//! colourblind, or on a theme where two of these sit close, cannot tell a `warning` from
//! a `failure` or a `todo` from an `info`. The mark is the second channel, and the
//! invariant the tests hold is that **kinds sharing a colour never share a mark**.
//!
//! **The mark lives in the gutter**, in the note column's left padding, outside the box
//! rather than inside it. The alternative was widening the 2px bar to carry the glyph,
//! which costs the callout's text 12px and reaches into the wrap model; the gutter is
//! already there, and a mark floating just outside a box reads as a label *for* the box.
//! It is drawn once, on the box's header row, because a mark on every row would be a
//! stripe of glyphs down the margin.
//!
//! The shapes are Lucide's vocabulary (`triangle-alert`, `circle-check`, `octagon`,
//! `quote`) reduced to what survives at [`MARK_SIZE`]. Lucide draws 24px line art with a
//! 2px stroke; at 8px that stroke is under a pixel and gpui's stroker has no round caps
//! to fall back on, so the marks here are filled where Lucide would outline, and outlined
//! only where the outline *is* the distinction (a `todo` box against a `note` dot). They
//! are hand-built from polygons rather than parsed from Lucide's SVG for the same reason:
//! a dozen 8px glyphs are cheaper and more predictable to state as points than to
//! transcribe path data and rescale.

use gpui::{point, px, rgb, Bounds, PathBuilder, Pixels, Point, Size, Window};
use shale_core::mdstyle::CalloutKind;

/// The side of the square a mark is drawn in.
///
/// Sized for the gutter it has to fit: the note column pads itself by 16 px, and
/// [`MARK_SIZE`] plus [`MARK_GAP`] is 12 px of that, leaving 4 px before the text
/// column's edge. Going much larger eats the padding and the mark starts crowding the
/// box it is labelling.
pub(crate) const MARK_SIZE: Pixels = px(8.);
/// How far the mark sits from the callout box's left edge.
///
/// Enough that the mark reads as belonging to the box without touching it, and small
/// enough that the pair still fits in the gutter (see [`MARK_SIZE`]).
pub(crate) const MARK_GAP: Pixels = px(4.);
/// The weight of an outlined mark's rule.
///
/// Heavier than the window's hairlines for the same reason the key chips are
/// (`crate::style::CHIP_RULE`): at this size a 1 px outline disappears into the fill it
/// is supposed to contrast with.
const MARK_RULE: Pixels = px(1.5);

/// The shape a callout kind is drawn with.
///
/// Thirteen shapes for fourteen kinds: `note` and `other` deliberately share one, because
/// an unrecognised type is drawn as a note and its visible title is the reader's only
/// clue (`CalloutKind::Other`). Every other pair that shares a palette colour is told
/// apart here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    /// A filled disc: the neutral one, and the one an unknown type falls back to.
    Dot,
    /// A ring. `info`, against `todo`'s [`Mark::Box`] in the same colour.
    Ring,
    /// An outlined square. `todo`, which is a checkbox in every callout syntax.
    Box,
    /// Three horizontal rules. `abstract`: a summary is a run of lines.
    Bars,
    /// A filled diamond. `example`, against `abstract` in the same colour.
    Diamond,
    /// An outlined diamond. `question`.
    DiamondOutline,
    /// A filled triangle. `tip`, against `success`'s [`Mark::Check`] in the same colour.
    Triangle,
    /// A tick. `success`.
    Check,
    /// An outlined triangle: the universal warning shape, and the one Lucide names
    /// `triangle-alert`. `warning`.
    Alert,
    /// Two crossing strokes. `failure`.
    Cross,
    /// A filled octagon — a stop sign. `danger`, against `bug`'s [`Mark::Square`] in the
    /// same colour.
    Octagon,
    /// A filled square. `bug`.
    Square,
    /// Two vertical rules. `quote`, the blockquote mark.
    DoubleBar,
}

/// The mark a callout kind is drawn with.
pub(crate) fn mark_for(kind: CalloutKind) -> Mark {
    match kind {
        CalloutKind::Note => Mark::Dot,
        // "Drawn like a note": Obsidian does not know this type, so it takes the note's
        // neutral mark and colour, and the title text is what tells the reader what it is.
        CalloutKind::Other => Mark::Dot,
        CalloutKind::Info => Mark::Ring,
        CalloutKind::Todo => Mark::Box,
        CalloutKind::Abstract => Mark::Bars,
        CalloutKind::Example => Mark::Diamond,
        CalloutKind::Tip => Mark::Triangle,
        CalloutKind::Success => Mark::Check,
        CalloutKind::Question => Mark::DiamondOutline,
        CalloutKind::Warning => Mark::Alert,
        CalloutKind::Failure => Mark::Cross,
        CalloutKind::Danger => Mark::Octagon,
        CalloutKind::Bug => Mark::Square,
        CalloutKind::Quote => Mark::DoubleBar,
    }
}

/// Where the mark goes, given the box it labels.
///
/// Centred vertically in the header row and hung in the gutter to the left of
/// `box_left`. The gutter is the note column's own padding, so this reaches outside the
/// line element's bounds on purpose — the padding is inside the editor's scroll clip, so
/// nothing crops it.
pub(crate) fn mark_box(box_left: Pixels, row_top: Pixels, row_height: Pixels) -> Bounds<Pixels> {
    Bounds {
        origin: Point {
            x: box_left - MARK_GAP - MARK_SIZE,
            y: row_top + (row_height - MARK_SIZE) / 2.,
        },
        size: Size {
            width: MARK_SIZE,
            height: MARK_SIZE,
        },
    }
}

/// The box a mark is drawn in, as a corner-to-corner pair for the polygon helpers.
#[derive(Debug, Clone, Copy)]
struct Frame {
    x: f32,
    y: f32,
    s: f32,
}

impl Frame {
    fn new(b: Bounds<Pixels>) -> Self {
        Self {
            x: f32::from(b.origin.x),
            y: f32::from(b.origin.y),
            s: f32::from(b.size.width),
        }
    }

    /// A point at a fraction along the frame, so each shape is stated once and scales
    /// with [`MARK_SIZE`] instead of being re-measured if the size changes.
    fn p(&self, fx: f32, fy: f32) -> Point<Pixels> {
        point(px(self.x + self.s * fx), px(self.y + self.s * fy))
    }
}

/// Paint one callout mark in `color`.
pub(crate) fn paint_mark(mark: Mark, at: Bounds<Pixels>, color: u32, window: &mut Window) {
    let f = Frame::new(at);
    let color = rgb(color);
    match mark {
        Mark::Dot => polygon(&circle(f, 0.38), true, color, window),
        Mark::Ring => polygon(&circle(f, 0.36), false, color, window),
        Mark::Box => polygon(
            &[
                f.p(0.12, 0.12),
                f.p(0.88, 0.12),
                f.p(0.88, 0.88),
                f.p(0.12, 0.88),
            ],
            false,
            color,
            window,
        ),
        Mark::Square => polygon(
            &[
                f.p(0.18, 0.18),
                f.p(0.82, 0.18),
                f.p(0.82, 0.82),
                f.p(0.18, 0.82),
            ],
            true,
            color,
            window,
        ),
        Mark::Diamond => diamond(f, true, color, window),
        Mark::DiamondOutline => diamond(f, false, color, window),
        Mark::Triangle => polygon(
            &[f.p(0.5, 0.1), f.p(0.92, 0.86), f.p(0.08, 0.86)],
            true,
            color,
            window,
        ),
        Mark::Alert => polygon(
            &[f.p(0.5, 0.08), f.p(0.94, 0.88), f.p(0.06, 0.88)],
            false,
            color,
            window,
        ),
        Mark::Check => stroke(
            &[f.p(0.14, 0.55), f.p(0.4, 0.8), f.p(0.88, 0.2)],
            color,
            window,
        ),
        Mark::Cross => {
            stroke(&[f.p(0.18, 0.18), f.p(0.82, 0.82)], color, window);
            stroke(&[f.p(0.82, 0.18), f.p(0.18, 0.82)], color, window);
        }
        Mark::Octagon => {
            // A regular octagon, flat-side up: the stop-sign silhouette survives at 8 px
            // where a twelve-sided "almost circle" would not read as anything.
            let cut = 0.31;
            polygon(
                &[
                    f.p(cut, 0.06),
                    f.p(1. - cut, 0.06),
                    f.p(0.94, cut),
                    f.p(0.94, 1. - cut),
                    f.p(1. - cut, 0.94),
                    f.p(cut, 0.94),
                    f.p(0.06, 1. - cut),
                    f.p(0.06, cut),
                ],
                true,
                color,
                window,
            );
        }
        Mark::Bars => {
            for fy in [0.24, 0.5, 0.76] {
                stroke(&[f.p(0.1, fy), f.p(0.9, fy)], color, window);
            }
        }
        Mark::DoubleBar => {
            for fx in [0.3, 0.7] {
                stroke(&[f.p(fx, 0.14), f.p(fx, 0.86)], color, window);
            }
        }
    }
}

fn diamond(f: Frame, filled: bool, color: gpui::Rgba, window: &mut Window) {
    polygon(
        &[
            f.p(0.5, 0.08),
            f.p(0.92, 0.5),
            f.p(0.5, 0.92),
            f.p(0.08, 0.5),
        ],
        filled,
        color,
        window,
    );
}

/// A round mark, as a twelve-point polygon.
///
/// `PathBuilder` has `arc_to`, but a dodecagon tessellates identically and is smooth
/// enough at [`MARK_SIZE`] that the faceting is invisible; the fewer moving parts in the
/// one place a build can fail, the better.
fn circle(f: Frame, radius: f32) -> Vec<Point<Pixels>> {
    (0..12)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 12.;
            point(
                px(f.x + f.s * (0.5 + radius * a.cos())),
                px(f.y + f.s * (0.5 + radius * a.sin())),
            )
        })
        .collect()
}

/// A closed polygon, filled or outlined.
fn polygon(points: &[Point<Pixels>], filled: bool, color: gpui::Rgba, window: &mut Window) {
    let mut builder = if filled {
        PathBuilder::fill()
    } else {
        PathBuilder::stroke(MARK_RULE)
    };
    for (i, point) in points.iter().enumerate() {
        if i == 0 {
            builder.move_to(*point);
        } else {
            builder.line_to(*point);
        }
    }
    builder.close();
    paint(builder, color, window);
}

/// An open polyline, always outlined: the marks that are strokes (a tick, a cross, a
/// set of bars) have no fill to speak of.
fn stroke(points: &[Point<Pixels>], color: gpui::Rgba, window: &mut Window) {
    let mut builder = PathBuilder::stroke(MARK_RULE);
    for (i, point) in points.iter().enumerate() {
        if i == 0 {
            builder.move_to(*point);
        } else {
            builder.line_to(*point);
        }
    }
    paint(builder, color, window)
}

/// Hand a built path to gpui.
///
/// A path that fails to tessellate draws **nothing at all** — gpui has no fallback box —
/// so a malformed mark would look like a missing glyph in the app and like nothing at all
/// in a test. These shapes are a dozen points each, nowhere near lyon's vertex ceiling, so
/// a failure here is a mistake in this file: said loudly where the tests will trip on it,
/// without taking the user's window down with it.
fn paint(builder: PathBuilder, color: gpui::Rgba, window: &mut Window) {
    match builder.build() {
        Ok(path) => window.paint_path(path, color),
        Err(error) => debug_assert!(false, "the callout mark failed to build: {error:?}"),
    }
}
