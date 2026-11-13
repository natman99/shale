//! Dice: the grammar a `!p` line spells, and the exact arithmetic behind it
//! (roadmap P7.9, `surfaces-plan.md`).
//!
//! The whole module is pure — no window, no buffer, no database — because the thing it
//! answers is a property of an expression, not an event: `2d8+4 vs AC16` has a hit chance
//! whether or not anyone rolls it. That is also why nothing here writes anything back. The
//! roll log (`!r`, the `rolls` table) is deferred; a probability has no result to record.
//!
//! **Exact, not sampled.** Every count is an integer until the last division, so the chip
//! that says 40% means exactly 8 outcomes in 20 and a test can say so. The brainstorm ruled
//! pure Rust and no crate, and the two pieces that could have come from a crate — the
//! convolution and the keep/drop order statistics — are the two the tests pin.
//!
//! ## What is deliberately not a goal
//!
//! This is not a probability calculator with a menu. Improved-crit ranges, exploding dice,
//! rerolls and dice pools the grammar does not spell are refused rather than invented
//! (ruled 2026-11-12: "there are websites for that"). A roll too wide to compute exactly
//! refuses out loud instead of hanging, because this runs at render time.

pub mod dist;
pub mod parse;
pub mod stats;

pub use dist::{DistError, Distribution};
pub use parse::{parse, request, Dice, Expression, Keep, ParseError, Target, TargetWord, REQUEST};
pub use stats::{Advantage, Check, Stats};

/// The largest number of dice one term may roll. Past this the distribution is either
/// meaningless to a reader or too wide to compute at render time, and the honest answer is
/// a refusal that names the number.
pub const MAX_DICE: u32 = 100;

/// The largest die any term may have. `d1000` is a real thing a table can use; `d100000`
/// is a way to make the app think for a long time.
pub const MAX_SIDES: u32 = 1000;
