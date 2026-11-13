//! The long form of a readout: what a hover surface shows when the margin's one line
//! is not enough (roadmap P4.15 M3, `surfaces-plan.md`).
//!
//! The chip answers "roughly how good is this?" from the end of a line of prose. This
//! answers "what exactly can this roll do?" and has room for the shape: the totals laid
//! out as bars when there are few enough of them to read (ruling D), and the numbers —
//! average, range, the target's chance, the d20's exceptions — always.
//!
//! The output is **text rows**, not widgets: the surface that shows them is the same card
//! the wiki-link hover paints, so the arithmetic here ends where a `String` ends. No
//! layout, no colour, no interaction — which is also the scope guard. This is a readout,
//! not a probability calculator with a menu.
//!
//! The title is deliberately absent: the line's own text is what the note wrote, and the
//! surface holding the note is the one that knows it.

use super::dist::{DistError, Distribution};
use super::number;
use super::percent;
use super::stats::Stats;

/// How wide a bar may be. The card is a text surface, and a bar longer than this is a bar
/// the reader cannot compare against its neighbour at a glance.
const BAR: usize = 16;

/// The lines under a report's title, in reading order: the summary, then the target, then
/// the totals when they are worth laying out.
///
/// The distribution is computed twice — once inside [`Stats::of`] and once for the bars —
/// which is honest at hover time (once when the pointer arrives on a line, not per frame)
/// and keeps [`Stats`] a bag of numbers rather than a bag of vectors.
pub fn lines(expression: &Expression) -> Result<Vec<String>, DistError> {
    let stats = Stats::of(expression)?;
    let mut out = vec![format!(
        "avg {} · {}–{}",
        number(stats.mean),
        stats.min,
        stats.max
    )];
    if let Some(target) = expression.target {
        // The d20 reading wins when there is one, exactly as it does on the chip: the same
        // arithmetic with the two exceptions the table actually plays.
        let chance = match &stats.check {
            Some(check) => check.success,
            None => stats.meets.unwrap_or(0.0),
        };
        out.push(match target.word.verb() {
            Some(verb) => format!(
                "{} {verb} {} {}",
                percent(chance),
                target.word.name(),
                target.value
            ),
            None => format!("{} ≥ {}", percent(chance), target.value),
        });
    }
    if let Some(check) = &stats.check {
        let mut row = format!("nat 20 crits: {}", percent(check.crit));
        // The face the die has to show, stated once. The chip's percentage already implies
        // it; this is the arithmetic behind the percentage, for the reader who asks.
        if check.needed > 20 {
            row.push_str(" · only a nat 20 succeeds");
        } else if check.needed <= 1 {
            row.push_str(" · only a nat 1 misses");
        } else {
            row.push_str(&format!(" · needs {}+", check.needed));
        }
        if let Some(way) = check.advantage.label() {
            row.push_str(&format!(" · {way}"));
        }
        out.push(row);
    }
    let dist = Distribution::roll(&expression.dice)?;
    out.extend(bars(&dist));
    Ok(out)
}

/// The totals as bars, when there are few enough of them to read — the same gate
/// [`Distribution::named_shape`] uses, so the two surfaces cannot disagree about what
/// "worth reading" means. A roll whose totals are a curve gets no rows at all: its average
/// and range are the honest summary, and a hover card is not a spreadsheet.
fn bars(dist: &Distribution) -> Vec<String> {
    if dist.named_shape().is_none() {
        return Vec::new();
    }
    let peak = dist.ways.iter().max().copied().unwrap_or(0).max(1);
    dist.ways
        .iter()
        .enumerate()
        .map(|(index, ways)| {
            let total = dist.min + index as i64;
            let filled = (BAR as f64 * *ways as f64 / peak as f64).round() as usize;
            format!(
                "{:>3} {:<BAR$} {:>6}",
                total,
                "█".repeat(filled),
                percent(*ways as f64 / dist.outcomes as f64)
            )
        })
        .collect()
}

use super::parse::Expression;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::parse;

    fn lines_of(text: &str) -> Vec<String> {
        lines(&parse(text).expect("a request the grammar accepts"))
            .expect("a roll small enough to lay out")
    }

    #[test]
    fn a_roll_with_no_target_says_its_average_and_its_range() {
        let out = lines_of("2d8+4");
        assert_eq!(
            out[0], "avg 13 · 6–20",
            "the chip's numbers, on their own line"
        );
        assert_eq!(
            out.len(),
            1,
            "fifteen totals is a curve, so nothing else is said"
        );
    }

    #[test]
    fn the_totals_are_laid_out_when_they_are_worth_reading() {
        let out = lines_of("2d6");
        let peak = out
            .iter()
            .find(|row| row.starts_with("  7 "))
            .expect("2d6 makes a 7");
        assert_eq!(
            peak,
            &format!("  7 {}  16.7%", "█".repeat(16)),
            "the peak fills the bar and says its exact chance"
        );
        let floor = out
            .iter()
            .find(|row| row.starts_with("  2 "))
            .expect("2d6 makes a 2");
        assert!(floor.contains('█'), "a one-way total still shows a bar");
        assert!(
            floor.contains("2.8%"),
            "and the chance the bar is really worth"
        );
    }

    #[test]
    fn a_curve_gets_no_bars() {
        // Sixteen totals is not a shape a reader holds in their head at a table (ruling D),
        // so the report stops at the summary rather than printing a histogram nobody reads.
        let out = lines_of("3d6");
        assert!(!out.iter().any(|row| row.contains('█')), "{out:?}");
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_target_line_names_the_target_it_won() {
        let out = lines_of("d20+5 vs AC18");
        assert_eq!(out[0], "avg 15.5 · 6–25");
        assert_eq!(
            out[1], "40% hits AC 18",
            "the word the line used, echoed back"
        );
        assert_eq!(
            out[2], "nat 20 crits: 5% · needs 13+",
            "the arithmetic behind the percentage"
        );
    }

    #[test]
    fn a_dc_takes_its_own_verb_and_the_same_arithmetic() {
        let out = lines_of("d20+3 vs DC12");
        assert_eq!(out[1], "60% passes DC 12");
    }

    #[test]
    fn a_bare_threshold_claims_neither_verb() {
        let out = lines_of("2d6 vs 8");
        assert_eq!(out[1], "41.7% ≥ 8", "fifteen outcomes in thirty-six");
        assert_eq!(
            out.len(),
            2 + 11,
            "no d20 reading, and the totals still laid out"
        );
    }

    #[test]
    fn an_impossible_target_says_what_the_exception_leaves() {
        let out = lines_of("d20+1 vs AC25");
        assert_eq!(
            out[1], "5% hits AC 25",
            "arithmetic says 0%; the nat 20 says otherwise"
        );
        assert!(out[2].contains("only a nat 20 succeeds"), "{:?}", out[2]);
    }

    #[test]
    fn a_certain_target_says_what_the_exception_takes_away() {
        let out = lines_of("d20+20 vs AC2");
        assert!(out[2].contains("only a nat 1 misses"), "{:?}", out[2]);
    }

    #[test]
    fn advantage_is_said_once_and_plain_is_not_said_at_all() {
        let out = lines_of("2d20kh1 vs AC15");
        assert!(out[2].ends_with("advantage"), "{:?}", out[2]);
        let plain = lines_of("d20 vs AC15");
        assert!(!plain[2].contains("advantage"), "{:?}", plain[2]);
    }
}
