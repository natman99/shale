//! The exact distribution of a roll: how many equally likely outcomes reach each total
//! (roadmap P7.9).
//!
//! Integer counts throughout — no floating point until a caller asks for a chance — because
//! the chip's whole claim is that 40% means eight outcomes in twenty. Two algorithms, chosen
//! by what the roll does:
//!
//! - **No keep/drop:** convolution. Each die slides a window of ones across the running
//!   distribution. Exact, and cheap enough for a hundred dice.
//! - **Keep/drop:** a pass over the face values from the kept end inward, deciding at each
//!   value how many dice show it and how many of those survive. That is the order statistic
//!   the brainstorm named as the part the tests must pin — advantage is the common case, and
//!   a naive convolution cannot express it.
//!
//! A roll too wide to enumerate **refuses** rather than hanging: this runs at render time,
//! and the module is not a probability calculator with a menu (ruled 2026-11-12).

use crate::dice::parse::{Dice, Keep};

/// Why a roll cannot be worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistError {
    /// Wider than the module will enumerate at render time. The refusal is honest about what
    /// it declined, rather than a spinner.
    TooWide,
    /// More outcomes than the counter holds.
    TooManyOutcomes,
}

/// The exact distribution of a roll's total, **after** the modifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distribution {
    /// The lowest total the roll can produce.
    pub min: i64,
    /// How many equally likely outcomes reach each total: index 0 is [`Self::min`], index 1
    /// is `min + 1`, and so on.
    pub ways: Vec<u128>,
    /// How many equally likely outcomes the roll has — the sum of [`Self::ways`].
    pub outcomes: u128,
}

impl Distribution {
    /// Work out a roll exactly.
    pub fn roll(dice: &Dice) -> Result<Distribution, DistError> {
        // The shape's width is checked before the outcome count, so a roll that is both too
        // wide to lay out and too large to count gets the specific refusal rather than the
        // general one.
        // Three gates before any counting, cheapest first, each naming what it declined:
        // the shape's width, the keep/drop table's size, and the outcome count. The last one
        // has to come before the counting, not after: the counts themselves would overflow
        // the counter on a roll this wide.
        if dice.count as usize * (dice.sides as usize - 1) + 1 > MAX_TOTALS {
            return Err(DistError::TooWide);
        }
        let kept = dice.kept() as usize;
        if dice.keep.is_some()
            && (dice.count as usize + 1)
                * (kept + 1)
                * (kept * dice.sides as usize + 1)
                * dice.sides as usize
                > KEEP_TABLE
        {
            return Err(DistError::TooWide);
        }
        let outcomes = outcomes(dice.count, dice.sides)?;
        let ways = match dice.keep {
            None => convolved(dice.count, dice.sides),
            Some(keep) => kept_ways(dice.count, dice.sides, keep),
        };
        // The kept dice each show at least 1, so the lowest total is the kept count, and the
        // modifier moves the whole shape.
        let min = kept as i64 + i64::from(dice.modifier);
        debug_assert_eq!(ways.iter().sum::<u128>(), outcomes, "the counts are exact");
        Ok(Distribution {
            min,
            ways,
            outcomes,
        })
    }

    /// The highest total the roll can produce.
    pub fn max(&self) -> i64 {
        self.min + self.ways.len() as i64 - 1
    }

    /// How many outcomes reach `value` or better. Meeting it counts, so this is `≥` — the
    /// ruling that "an AC means you hit it".
    pub fn at_least(&self, value: i64) -> u128 {
        if value > self.max() {
            return 0;
        }
        let from = (value - self.min).max(0) as usize;
        self.ways[from..].iter().sum()
    }

    /// The chance of reaching `value` or better, as a fraction of 1.
    pub fn chance_at_least(&self, value: i64) -> f64 {
        self.at_least(value) as f64 / self.outcomes as f64
    }

    /// The chance of exactly one face showing — the crit question, which is only ever asked
    /// of a roll whose result is a single die.
    pub fn chance_of(&self, value: i64) -> f64 {
        if value < self.min || value > self.max() {
            return 0.0;
        }
        self.ways[(value - self.min) as usize] as f64 / self.outcomes as f64
    }

    /// The average total. Exact while the numerator fits; past that the sum is accumulated
    /// in floating point, which is a display concern rather than a wrong answer.
    pub fn mean(&self) -> f64 {
        let mut numerator: i128 = 0;
        for (index, ways) in self.ways.iter().enumerate() {
            if *ways > i128::MAX as u128 {
                return self.mean_in_float();
            }
            let term = (*ways as i128) * i128::from(self.min + index as i64);
            let Some(next) = numerator.checked_add(term) else {
                return self.mean_in_float();
            };
            numerator = next;
        }
        numerator as f64 / self.outcomes as f64
    }

    /// The average, summed in floating point: the fallback for a roll whose counts are too
    /// wide for exact integer accumulation.
    fn mean_in_float(&self) -> f64 {
        self.ways
            .iter()
            .enumerate()
            .map(|(index, ways)| *ways as f64 * (self.min + index as i64) as f64)
            .sum::<f64>()
            / self.outcomes as f64
    }

    /// The shape named out loud when it is small enough to name: `1–6` for a die, and the
    /// whole list of totals for a roll a reader can hold in their head (two dice). `None`
    /// when the shape is a curve rather than a list (ruling D: mean, min and max are the
    /// default; the distribution only when it is worth reading).
    pub fn named_shape(&self) -> Option<String> {
        if self.ways.len() > 12 {
            return None;
        }
        let parts: Vec<String> = (0..self.ways.len())
            .map(|index| format!("{}", self.min + index as i64))
            .collect();
        Some(parts.join(" "))
    }
}

/// How many distinct totals one roll may lay out. Past this the shape is a curve no reader
/// would read at a table, and the frame would pay for painting it.
pub const MAX_TOTALS: usize = 200_000;

/// How much work the keep/drop table may cost, in cells times the face values walked. The
/// order-statistic pass is polynomial rather than exponential, but it still runs while a
/// frame is being painted.
pub const KEEP_TABLE: usize = 60_000_000;

/// How many outcomes `count` dice of `sides` faces have: sides^count, or a refusal when the
/// counter cannot hold it.
fn outcomes(count: u32, sides: u32) -> Result<u128, DistError> {
    let mut total: u128 = 1;
    for _ in 0..count {
        total = total
            .checked_mul(u128::from(sides))
            .ok_or(DistError::TooManyOutcomes)?;
    }
    Ok(total)
}

/// The distribution of a sum of identical dice, by convolution with a sliding window of
/// ones — one die at a time, each contributing every face once.
fn convolved(count: u32, sides: u32) -> Vec<u128> {
    let sides = sides as usize;
    let mut ways = vec![1u128];
    for _ in 0..count {
        let mut next = vec![0u128; ways.len() + sides - 1];
        let mut window: u128 = 0;
        for index in 0..next.len() {
            if index < ways.len() {
                window += ways[index];
            }
            if index >= sides {
                window -= ways[index - sides];
            }
            next[index] = window;
        }
        ways = next;
    }
    ways
}

/// The distribution of the sum of the best (or worst) `keep` of `count` dice: the order
/// statistic, computed exactly by walking the faces from the kept end inward.
///
/// At each face value, some number of the still-unassigned dice show it. Because the values
/// are visited from the end that survives, the first `keep` dice met are the kept ones, and
/// the rest are discarded — which is the whole of what "keep highest" means, with no sorting
/// and no enumeration of individual rolls. The weight of each choice is a binomial
/// coefficient: which dice show this face, out of those still free.
fn kept_ways(count: u32, sides: u32, keep: Keep) -> Vec<u128> {
    let (n, s, k) = (count as usize, sides as usize, keep.count() as usize);
    let highest = match keep {
        Keep::Highest(_) => true,
        Keep::Lowest(_) => false,
    };
    // The table is (dice assigned) × (dice kept) × (kept sum). Bounded before it is built:
    // this runs while a frame is being painted, so a roll too wide to fit is refused.
    // The size gate is [`roll`]'s, before any counting: by the time this runs the roll is
    // already known to fit.
    let choose = binomial(n);
    let mut dp = vec![vec![vec![0u128; k * s + 1]; k + 1]; n + 1];
    dp[0][0][0] = 1;
    let order: Vec<usize> = if highest {
        (1..=s).rev().collect()
    } else {
        (1..=s).collect()
    };
    for value in order {
        // Descending over assigned dice, so a state created by this face value is never
        // visited again in the same pass.
        for assigned in (0..=n).rev() {
            for kept in 0..=k {
                for sum in 0..=k * s {
                    let here = dp[assigned][kept][sum];
                    if here == 0 {
                        continue;
                    }
                    let free = n - assigned;
                    for showing in 1..=free {
                        let take = showing.min(k - kept);
                        let next_sum = sum + take * value;
                        if next_sum > k * s {
                            break;
                        }
                        dp[assigned + showing][kept + take][next_sum] +=
                            here * choose[free][showing];
                    }
                }
            }
        }
    }
    // Every die assigned, exactly `k` kept: read the kept sums, whose minimum is `k`.
    dp[n][k][k..=k * s].to_vec()
}

/// Pascal's triangle to `n`, for "which of the free dice show this face".
fn binomial(n: usize) -> Vec<Vec<u128>> {
    let mut rows = vec![vec![1u128]];
    for row in 1..=n {
        let previous = &rows[row - 1];
        let mut next = vec![1u128; row + 1];
        for column in 1..row {
            next[column] = previous[column - 1] + previous[column];
        }
        rows.push(next);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::parse::parse;

    /// The roll of one die is flat, and its counts are the die.
    #[test]
    fn one_die_is_flat() {
        let dice = parse("1d6").unwrap().dice;
        let dist = Distribution::roll(&dice).unwrap();
        assert_eq!(dist.min, 1);
        assert_eq!(dist.max(), 6);
        assert_eq!(dist.outcomes, 6);
        assert_eq!(dist.ways, vec![1, 1, 1, 1, 1, 1]);
    }

    /// Two dice are the triangle every GM draws.
    #[test]
    fn two_dice_are_the_triangle() {
        let dist = Distribution::roll(&parse("2d6").unwrap().dice).unwrap();
        assert_eq!(dist.min, 2);
        assert_eq!(dist.max(), 12);
        assert_eq!(dist.outcomes, 36);
        assert_eq!(dist.ways, vec![1, 2, 3, 4, 5, 6, 5, 4, 3, 2, 1]);
    }

    /// The modifier moves the shape and nothing else.
    #[test]
    fn a_modifier_moves_the_shape() {
        let dist = Distribution::roll(&parse("2d8+4").unwrap().dice).unwrap();
        assert_eq!(dist.min, 6);
        assert_eq!(dist.max(), 20);
        assert_eq!(dist.outcomes, 64);
        assert_eq!(dist.ways.len(), 15);
    }

    /// The classic ability score — 4d6 keeping the best three — against the number a table
    /// of GMs knows: 12.2446.
    #[test]
    fn four_six_keep_three_is_the_number_gms_know() {
        let dist = Distribution::roll(&parse("4d6kh3").unwrap().dice).unwrap();
        assert_eq!(dist.min, 3);
        assert_eq!(dist.max(), 18);
        assert_eq!(dist.outcomes, 1296);
        assert!((dist.mean() - 12.2446).abs() < 1e-4, "mean {}", dist.mean());
    }

    /// Keep-lowest is keep-highest turned inside out: replace every die `d` with `7-d` and
    /// the three worst become the three best, so the two distributions mirror and their
    /// means add to 21 (three dice, seven faces each).
    #[test]
    fn keep_lowest_is_the_mirror_of_keep_highest() {
        let high = Distribution::roll(&parse("4d6kh3").unwrap().dice).unwrap();
        let low = Distribution::roll(&parse("4d6kl3").unwrap().dice).unwrap();
        assert_eq!(low.outcomes, high.outcomes);
        let mut mirrored = high.ways.clone();
        mirrored.reverse();
        assert_eq!(low.ways, mirrored, "the counts mirror, not just the mean");
        assert!((low.mean() - (21.0 - high.mean())).abs() < 1e-9);
        assert_eq!(low.min, 3);
        assert_eq!(low.max(), 18);
    }

    /// The order statistic the plan says the tests must pin: advantage is "the higher of two
    /// d20s", and the closed form a hand can check is 1 − (12/20)².
    #[test]
    fn advantage_is_the_order_statistic_a_hand_can_check() {
        let dist = Distribution::roll(&parse("2d20kh1").unwrap().dice).unwrap();
        assert_eq!(dist.outcomes, 400);
        assert_eq!(dist.min, 1);
        assert_eq!(dist.max(), 20);
        // P(max ≥ 13) = 1 − (12/20)² = 0.64
        assert!((dist.chance_at_least(13) - 0.64).abs() < 1e-12);
        // A natural 20 on either die: 1 − (19/20)² = 0.0975
        assert!((dist.chance_of(20) - 0.0975).abs() < 1e-12);
    }

    /// Disadvantage is the same walk from the other end: both dice must clear the bar.
    #[test]
    fn disadvantage_needs_both_dice() {
        let dist = Distribution::roll(&parse("2d20kl1").unwrap().dice).unwrap();
        // P(min ≥ 13) = (8/20)² = 0.16
        assert!((dist.chance_at_least(13) - 0.16).abs() < 1e-12);
        assert!((dist.chance_of(20) - 0.0025).abs() < 1e-12);
    }

    /// Keep/drop and plain rolls agree when nothing is dropped — the two code paths cannot
    /// disagree about the same roll.
    #[test]
    fn keeping_everything_is_the_plain_convolution() {
        let plain = Distribution::roll(&parse("3d8").unwrap().dice).unwrap();
        let keeping = Distribution::roll(&parse("3d8kh3").unwrap().dice).unwrap();
        assert_eq!(plain, keeping);
    }

    /// Meeting the number counts. The ruling is "an AC means you hit it", so the boundary
    /// outcome is included on both sides of the comparison.
    #[test]
    fn meeting_the_number_counts() {
        let dist = Distribution::roll(&parse("1d20").unwrap().dice).unwrap();
        assert_eq!(dist.at_least(16), 5, "16, 17, 18, 19, 20");
        assert!((dist.chance_at_least(16) - 0.25).abs() < 1e-12);
        assert_eq!(dist.at_least(21), 0);
        assert_eq!(dist.at_least(1), 20);
    }

    /// A roll nobody could read at render time refuses instead of hanging.
    #[test]
    fn a_roll_too_wide_to_work_out_refuses() {
        let dice = parse("100d1000kh50").unwrap().dice;
        assert_eq!(Distribution::roll(&dice), Err(DistError::TooWide));
        let dice = parse("100d1000").unwrap().dice;
        assert_eq!(Distribution::roll(&dice), Err(DistError::TooManyOutcomes));
    }

    /// The shape is named only when a reader can hold it: a die yes, 2d8 yes, 4d6kh1 no.
    #[test]
    fn the_shape_is_named_only_when_it_is_worth_reading() {
        assert_eq!(
            Distribution::roll(&parse("1d6").unwrap().dice)
                .unwrap()
                .named_shape()
                .as_deref(),
            Some("1 2 3 4 5 6")
        );
        assert!(Distribution::roll(&parse("2d6").unwrap().dice)
            .unwrap()
            .named_shape()
            .is_some());
        assert!(
            Distribution::roll(&parse("2d8").unwrap().dice)
                .unwrap()
                .named_shape()
                .is_none(),
            "fifteen totals is a curve, not a list"
        );
        assert!(Distribution::roll(&parse("3d8").unwrap().dice)
            .unwrap()
            .named_shape()
            .is_none());
    }
}
