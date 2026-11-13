//! What the chip says about a roll: the numbers, and **which reading of them is in force**
//! (roadmap P7.9, ruling C2 in `surfaces-plan.md`).
//!
//! The arithmetic answer and the table answer are different questions, and the chip has to
//! say which one it is giving. `d20+7 vs AC8` is 100% to arithmetic and 95% at a table,
//! because a natural 1 always fails; `d20-5 vs AC16` is 0% to arithmetic and 5% at a table,
//! because a natural 20 always succeeds. Both numbers are true of different things, so this
//! module returns both and lets the chip name the one it painted.
//!
//! The exception rules apply only to the shape that has them: a twenty-sided die whose
//! result is one die. Damage dice, ability scores rolled `4d6kh1`, and a save DC check on
//! `2d8+4` are arithmetic, and claiming a crit for them would be a made-up rule.

use crate::dice::dist::{DistError, Distribution};
use crate::dice::parse::{Dice, Expression, Keep, Target, TargetWord};

/// How the d20 got its number, which is what `kh1` and `kl1` mean at a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advantage {
    /// One die, rolled once.
    Plain,
    /// Two dice, the higher showing.
    Advantage,
    /// Two dice, the lower showing.
    Disadvantage,
}

impl Advantage {
    /// The word a surface says about it, if it says anything: a plain roll has nothing
    /// worth adding.
    #[must_use]
    pub fn label(&self) -> Option<&'static str> {
        match self {
            Self::Plain => None,
            Self::Advantage => Some("advantage"),
            Self::Disadvantage => Some("disadvantage"),
        }
    }
}

/// The d20 reading: what the roll needs, and the chance of getting it with the natural-20
/// and natural-1 rules in force.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Check {
    /// The face the showing die must reach, once the modifier is taken off the target.
    /// Above 20 means arithmetic says it cannot; at or below 1 means arithmetic says it
    /// always can — and neither is the last word, because of the two exceptions.
    pub needed: i32,
    /// The chance of succeeding **with** the natural-20 and natural-1 rules.
    pub success: f64,
    /// The chance the showing die is a natural 20. Crit range is 20 only: the grammar has
    /// no improved-critical syntax and none is invented here (ruled 2026-11-12).
    pub crit: f64,
    /// Whether the roll is advantage, disadvantage, or a plain roll.
    pub advantage: Advantage,
}

/// Everything the chip may show about one expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    /// The average total, modifier included.
    pub mean: f64,
    /// The lowest total.
    pub min: i64,
    /// The highest total.
    pub max: i64,
    /// The chance of meeting or beating the target, **without** the d20 exceptions — the
    /// plain arithmetic answer, and the only chance a non-d20 roll has.
    pub meets: Option<f64>,
    /// Which word the target used, so the chip can say "hits", "succeeds", or "meets or
    /// beats" without guessing.
    pub word: Option<TargetWord>,
    /// The d20 reading, when the roll is one twenty-sided die showing one die.
    pub check: Option<Check>,
    /// The totals named out loud, when there are few enough to read (ruling D).
    pub shape: Option<String>,
}

impl Stats {
    /// Work out what to say about an expression.
    pub fn of(expression: &Expression) -> Result<Stats, DistError> {
        let dist = Distribution::roll(&expression.dice)?;
        let meets = expression
            .target
            .map(|target| dist.chance_at_least(i64::from(target.value)));
        Ok(Stats {
            mean: dist.mean(),
            min: dist.min,
            max: dist.max(),
            meets,
            word: expression.target.map(|target| target.word),
            check: expression
                .target
                .and_then(|target| check(&expression.dice, target)),
            shape: dist.named_shape(),
        })
    }
}

/// The d20 reading, when the roll has one: the face that shows, what it needs, and the two
/// exceptions that outrank the arithmetic.
fn check(dice: &Dice, target: Target) -> Option<Check> {
    if !dice.is_d20_check() {
        return None;
    }
    // The distribution of the *showing* face: the same roll with the modifier taken off, so
    // the numbers are faces and not totals.
    let showing = Distribution::roll(&dice.showing()).ok()?;
    let needed = target.value - dice.modifier;
    let crit = showing.chance_of(20);
    // A natural 20 always succeeds and a natural 1 always fails (ruled: the general
    // assumption; how a table shifts those is not Shale's problem). So the face succeeds
    // when it is a 20, or when it clears the bar **and** is not a 1.
    let floor = needed.max(2);
    let mut success = showing.chance_at_least(i64::from(floor));
    if floor > 20 {
        success += crit;
    }
    Some(Check {
        needed,
        success,
        crit,
        advantage: match (dice.count, dice.keep) {
            (1, None) => Advantage::Plain,
            (_, Some(Keep::Highest(1))) => Advantage::Advantage,
            (_, Some(Keep::Lowest(1))) => Advantage::Disadvantage,
            // More dice kept than one is not an attack roll even on d20s, and more dice
            // with no keep clause is a pool. `is_d20_check` already ruled those out.
            _ => Advantage::Plain,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::parse::parse;

    fn stats(line: &str) -> Stats {
        Stats::of(&parse(line).unwrap()).unwrap()
    }

    fn close(got: f64, want: f64) -> bool {
        (got - want).abs() < 1e-9
    }

    /// The example from the ruling: a plain d20 against AC 16 hits on 16 and up.
    #[test]
    fn a_d20_against_ac16_hits_on_16_and_up() {
        let got = stats("d20 vs AC16");
        let check = got.check.expect("a d20 has a check reading");
        assert_eq!(check.needed, 16);
        assert!(close(check.success, 0.25), "success {}", check.success);
        assert!(close(got.meets.unwrap(), 0.25));
        assert!(close(check.crit, 0.05));
        assert_eq!(check.advantage, Advantage::Plain);
        assert_eq!(got.word, Some(TargetWord::Ac));
    }

    /// The modifier is part of the total, so the face needs less: `+3 vs AC16` needs a 13.
    #[test]
    fn a_modifier_lowers_what_the_face_must_show() {
        let got = stats("d20+3 vs AC16");
        let check = got.check.unwrap();
        assert_eq!(check.needed, 13);
        assert!(close(check.success, 0.40), "success {}", check.success);
    }

    /// `DC` is an alias, and the arithmetic does not move: `+3 vs DC12` needs a 9.
    #[test]
    fn a_check_uses_the_same_rule_as_an_attack() {
        let got = stats("d20+3 vs DC12");
        let check = got.check.unwrap();
        assert_eq!(check.needed, 9);
        assert!(close(check.success, 0.60));
        assert_eq!(got.word, Some(TargetWord::Dc));
    }

    /// Advantage, the case the plan says the tests must pin: 1 − (12/20)², and a crit on
    /// either die.
    #[test]
    fn advantage_raises_both_numbers() {
        let got = stats("2d20kh1+3 vs AC16");
        let check = got.check.unwrap();
        assert_eq!(check.needed, 13);
        assert!(close(check.success, 0.64), "success {}", check.success);
        assert!(close(check.crit, 0.0975), "crit {}", check.crit);
        assert_eq!(check.advantage, Advantage::Advantage);
    }

    /// Disadvantage: both dice must clear the bar, so the chance is the square of the plain
    /// one's complement.
    #[test]
    fn disadvantage_needs_both_dice_to_clear_the_bar() {
        let got = stats("2d20kl1+3 vs AC16");
        let check = got.check.unwrap();
        assert!(close(check.success, 0.16), "success {}", check.success);
        assert!(close(check.crit, 0.0025));
        assert_eq!(check.advantage, Advantage::Disadvantage);
    }

    /// Where arithmetic and the table disagree, the chip's reading is the table's — and the
    /// arithmetic number is still there to be shown beside it.
    #[test]
    fn a_natural_one_always_fails_even_when_the_math_says_otherwise() {
        let got = stats("d20+7 vs AC8");
        let check = got.check.unwrap();
        assert_eq!(check.needed, 1);
        assert!(close(got.meets.unwrap(), 1.0), "arithmetic says always");
        assert!(close(check.success, 0.95), "the table says not on a 1");
    }

    /// …and the other edge: arithmetic says impossible, the natural 20 says otherwise.
    #[test]
    fn a_natural_twenty_always_succeeds_even_when_the_math_says_otherwise() {
        let got = stats("d20-5 vs AC16");
        let check = got.check.unwrap();
        assert_eq!(check.needed, 21);
        assert!(close(got.meets.unwrap(), 0.0));
        assert!(close(check.success, 0.05), "only the 20");
    }

    /// Damage dice are arithmetic, and claiming a crit for them would be a made-up rule.
    #[test]
    fn damage_dice_have_no_crit_and_no_exceptions() {
        let got = stats("2d8+4 vs 16");
        assert!(got.check.is_none(), "not a d20, so no check reading");
        // 2d8 must reach 12: sums 12..16 in 15 of 64 outcomes.
        assert!(
            close(got.meets.unwrap(), 15.0 / 64.0),
            "meets {:?}",
            got.meets
        );
        assert_eq!(got.word, Some(TargetWord::Bare));
    }

    /// With nothing to meet, the answer is the shape of the roll: mean, min, max (ruling D).
    #[test]
    fn with_no_target_the_answer_is_the_shape() {
        let got = stats("2d8+4");
        assert!(got.meets.is_none());
        assert!(got.check.is_none());
        assert!(close(got.mean, 13.0), "2d8 averages 9, and the +4 rides");
        assert_eq!(got.min, 6);
        assert_eq!(got.max, 20);
    }

    /// A d20 pool is not an attack roll: `2d20` with no keep clause shows no crit.
    #[test]
    fn a_pool_of_d20s_is_not_one_die_showing() {
        let got = stats("2d20 vs 25");
        assert!(got.check.is_none(), "two dice showing, so no single face");
        // 2d20 reaching 25 or more is 136 of 400 outcomes.
        assert!(close(got.meets.unwrap(), 0.34));
    }
}
