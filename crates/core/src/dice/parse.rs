//! The `!p` grammar: `NdN[+/-m][khN|klN] [vs (AC|DC|) N]` (roadmap P7.9).
//!
//! Hand-rolled rather than a parser crate for the same reason the arithmetic is hand-rolled:
//! the grammar is one line long, and the useful part of parsing it is not accepting it but
//! **naming what went wrong** — `3d` has no die size, `2d8kh5` keeps more dice than it
//! rolls, `2d8+4 vs` asks about nothing. A generated parser refuses with a token list; this
//! refuses with a sentence the status bar can show.
//!
//! **One delta from the specced grammar** (`surfaces-plan.md`): the brainstorm wrote the
//! modifier before the keep clause, `NdN[+/-m][khN|klN]`, but the form a GM actually types
//! for an ability score is `4d6kh1+3` — keep first, modifier last. Both spellings parse to
//! the same term. The spec's order is the one nobody writes.

use crate::dice::{MAX_DICE, MAX_SIDES};

/// One dice term, as written: `4d6kh1+3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dice {
    /// How many dice are rolled.
    pub count: u32,
    /// How many faces each has.
    pub sides: u32,
    /// Added to the kept total — negative when the expression subtracts. The modifier moves
    /// the whole distribution, which is why `d20+3 vs AC16` needs a 13 and not a 16.
    pub modifier: i32,
    /// Which dice survive. `None` keeps every die.
    pub keep: Option<Keep>,
}

/// Which end of the roll survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// `kh1` — keep the highest N. With one kept die and two rolled, that is advantage.
    Highest(u32),
    /// `kl1` — keep the lowest N. With one kept die and two rolled, disadvantage.
    Lowest(u32),
}

impl Keep {
    /// How many dice survive.
    pub fn count(&self) -> u32 {
        match self {
            Keep::Highest(n) | Keep::Lowest(n) => *n,
        }
    }
}

impl Dice {
    /// How many dice reach the total: the kept number, or every die when nothing is dropped.
    pub fn kept(&self) -> u32 {
        match self.keep {
            Some(Keep::Highest(n)) | Some(Keep::Lowest(n)) => n,
            None => self.count,
        }
    }

    /// The one shape that has a crit and a natural-20 rule: a twenty-sided die whose result
    /// is a single die — a plain `d20`, or advantage and disadvantage as `2d20kh1` and
    /// `2d20kl1` (ruling C2 in `surfaces-plan.md`).
    pub fn is_d20_check(&self) -> bool {
        self.sides == 20 && self.kept() == 1
    }

    /// The same roll with the modifier taken off, which is the distribution of **the face
    /// that is showing** — the number the natural-20 and natural-1 rules are about.
    pub fn showing(&self) -> Dice {
        Dice {
            count: self.count,
            sides: self.sides,
            modifier: 0,
            keep: self.keep,
        }
    }
}

/// What the roll has to meet or beat: `vs AC16`, `vs DC 12`, `vs 15`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// The number the total must reach. **Meeting it counts** — "an AC means you hit it"
    /// (ruled 2026-11-12), so this is `≥` and never `>`.
    pub value: i32,
    /// Which word the writer used. It changes only the chip's language: the arithmetic for
    /// `AC` and `DC` is one rule with two vocabularies.
    pub word: TargetWord,
}

/// The word in front of the target number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetWord {
    /// `vs AC16` — an attack roll; the chip says "hits".
    Ac,
    /// `vs DC 12` — a check; the chip says "succeeds". Same math (ruled: an alias).
    Dc,
    /// `vs 15` — a threshold that claims neither, so the chip says "meets or beats".
    Bare,
}

/// A parsed `!p` line: the roll, and what it is being asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    /// The dice.
    pub dice: Dice,
    /// What they must meet or beat, when the line named one.
    pub target: Option<Target>,
}

/// Why a line is not a dice expression, in words a status bar can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Nothing to read.
    Empty,
    /// Text where a dice term belongs, with the text that was found.
    NotDice(String),
    /// `3d` — a die with no number of faces.
    NoSides,
    /// `2d0` — a die with no faces to land on.
    ZeroSides,
    /// More dice than the module will compute.
    TooManyDice(u32),
    /// A die with more faces than the module will compute.
    TooManySides(u32),
    /// `2d8kh` — a keep clause with no number.
    KeepWithoutCount,
    /// `2d8kh0` — keeping nothing is not a roll.
    KeepsNone,
    /// `2d8kh5` — keeping more dice than are rolled.
    KeepsMoreThanRolled {
        /// How many the line asked to keep.
        keep: u32,
        /// How many the line rolls.
        rolled: u32,
    },
    /// `2d8+4 vs` — a target clause with no number.
    TargetWithoutValue,
    /// Text after a complete expression, which means the line means something else.
    Trailing(String),
}

impl ParseError {
    /// The sentence the chip puts in the status bar.
    pub fn said(&self) -> String {
        match self {
            ParseError::Empty => "no dice to work out".to_string(),
            ParseError::NotDice(found) => format!("“{found}” is not a dice expression"),
            ParseError::NoSides => "a die needs a number of faces: `2d8`, not `2d`".to_string(),
            ParseError::ZeroSides => "a die with no faces cannot land".to_string(),
            ParseError::TooManyDice(n) => {
                format!("{n} dice is past what Shale works out ({MAX_DICE} is the most)")
            }
            ParseError::TooManySides(n) => {
                format!("a d{n} is past what Shale works out (d{MAX_SIDES} is the most)")
            }
            ParseError::KeepWithoutCount => "keep what? `4d6kh1` keeps the highest one".to_string(),
            ParseError::KeepsNone => "keeping no dice is not a roll".to_string(),
            ParseError::KeepsMoreThanRolled { keep, rolled } => {
                format!("cannot keep {keep} of {rolled}: the roll has fewer dice than that")
            }
            ParseError::TargetWithoutValue => "meets or beats what? `vs AC16`".to_string(),
            ParseError::Trailing(rest) => format!("nothing after a dice expression ({rest})"),
        }
    }
}

/// The `!p` word that asks for this chip, and nothing else: a bare `2d8+4` in a sentence is
/// ordinary writing and gets no readout (ruling A).
pub const REQUEST: &str = "!p";

/// Whether a line is a request for the chip, and if so the expression after `!p`.
///
/// The prefix has to be the whole first word — `!p 2d8+4`, not `!private` and not `see !p`
/// — because the chip is a surface on a line the user wrote, not a search of it.
pub fn request(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix(REQUEST)?;
    if rest.is_empty() {
        return Some("");
    }
    let spaced = rest.starts_with([' ', '\t']);
    spaced.then_some(rest.trim_start())
}

/// Read one expression: the dice, and the target if the line named one.
pub fn parse(text: &str) -> Result<Expression, ParseError> {
    let mut at = Cursor::new(text);
    let dice = at.dice()?;
    let target = at.target()?;
    at.finish()?;
    Ok(Expression { dice, target })
}

/// A byte cursor over ASCII grammar. The text can hold anything — this only ever matches
/// ASCII, and hands back whatever it did not match as the error's own words.
struct Cursor<'a> {
    /// The whole line.
    text: &'a str,
    /// Where the cursor is, in bytes.
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, at: 0 }
    }

    /// The rest, untrimmed — what an error quotes.
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn spaces(&mut self) {
        while self.rest().starts_with([' ', '\t']) {
            self.at += 1;
        }
    }

    /// The next characters if they are the word, case-insensitively, and consume them.
    fn eat(&mut self, word: &str) -> bool {
        if self.rest()[..self.rest().len().min(word.len())].eq_ignore_ascii_case(word) {
            self.at += word.len();
            true
        } else {
            false
        }
    }

    /// A run of digits, and whether one was there at all.
    fn digits(&mut self) -> Option<&'a str> {
        let rest = self.rest();
        let len = rest.bytes().take_while(u8::is_ascii_digit).count();
        if len == 0 {
            return None;
        }
        let found = &rest[..len];
        self.at += len;
        Some(found)
    }

    /// Digits that fit, or the refusal that says why they do not.
    fn number(&mut self, too_many: impl Fn(u32) -> ParseError) -> Result<u32, ParseError> {
        let digits = self.digits().ok_or_else(|| too_many(0))?;
        digits
            .parse()
            .map_err(|_| too_many(digits.parse::<u64>().unwrap_or(u64::MAX) as u32))
    }

    /// The dice term.
    fn dice(&mut self) -> Result<Dice, ParseError> {
        self.spaces();
        let count = match self.digits() {
            Some(digits) => digits
                .parse::<u32>()
                .map_err(|_| ParseError::TooManyDice(u32::MAX))?,
            None => 1,
        };
        if !self.eat("d") {
            return Err(if self.at == 0 && count == 1 {
                ParseError::Empty
            } else {
                ParseError::NotDice(self.text.trim().to_string())
            });
        }
        let sides = self.number(|_| ParseError::NoSides)?;
        if sides == 0 {
            return Err(ParseError::ZeroSides);
        }
        if count == 0 {
            return Err(ParseError::TooManyDice(0));
        }
        if count > MAX_DICE {
            return Err(ParseError::TooManyDice(count));
        }
        if sides > MAX_SIDES {
            return Err(ParseError::TooManySides(sides));
        }

        // The two clauses that may follow, in either order — see the module note on why the
        // spec's order is not the only one accepted.
        let mut modifier = 0i32;
        let mut keep = None;
        loop {
            self.spaces();
            if self.eat("kh") || self.eat("KH") {
                let n = self.number(|_| ParseError::KeepWithoutCount)?;
                if n == 0 {
                    return Err(ParseError::KeepsNone);
                }
                if n > count {
                    return Err(ParseError::KeepsMoreThanRolled {
                        keep: n,
                        rolled: count,
                    });
                }
                keep = Some(Keep::Highest(n));
            } else if self.eat("kl") || self.eat("KL") {
                let n = self.number(|_| ParseError::KeepWithoutCount)?;
                if n == 0 {
                    return Err(ParseError::KeepsNone);
                }
                if n > count {
                    return Err(ParseError::KeepsMoreThanRolled {
                        keep: n,
                        rolled: count,
                    });
                }
                keep = Some(Keep::Lowest(n));
            } else if self.eat("+") {
                self.spaces();
                modifier += self.number(|_| ParseError::Trailing("+".to_string()))? as i32;
            } else if self.eat("-") {
                self.spaces();
                modifier -= self.number(|_| ParseError::Trailing("-".to_string()))? as i32;
            } else {
                break;
            }
        }

        Ok(Dice {
            count,
            sides,
            modifier,
            keep,
        })
    }

    /// The `vs …` clause, if there is one.
    fn target(&mut self) -> Result<Option<Target>, ParseError> {
        self.spaces();
        if !self.eat("vs") {
            return Ok(None);
        }
        self.spaces();
        let word = if self.eat("ac") {
            TargetWord::Ac
        } else if self.eat("dc") {
            TargetWord::Dc
        } else {
            TargetWord::Bare
        };
        self.spaces();
        let digits = self
            .digits()
            .ok_or(ParseError::TargetWithoutValue)
            .and_then(|digits| {
                digits
                    .parse::<i32>()
                    .map_err(|_| ParseError::TargetWithoutValue)
            })?;
        Ok(Some(Target {
            value: digits,
            word,
        }))
    }

    /// Nothing may follow a complete expression.
    fn finish(&mut self) -> Result<(), ParseError> {
        self.spaces();
        if self.rest().is_empty() {
            Ok(())
        } else {
            Err(ParseError::Trailing(self.rest().to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The forms a GM types, all of them.
    #[test]
    fn the_shapes_a_gm_types_all_parse() {
        assert_eq!(
            parse("d20").unwrap().dice,
            Dice {
                count: 1,
                sides: 20,
                modifier: 0,
                keep: None
            }
        );
        assert_eq!(parse("2d8+4").unwrap().dice.modifier, 4);
        assert_eq!(parse("1d8-1").unwrap().dice.modifier, -1);
        assert_eq!(
            parse("4d6kh1+3").unwrap().dice,
            Dice {
                count: 4,
                sides: 6,
                modifier: 3,
                keep: Some(Keep::Highest(1))
            }
        );
        // The spec's order, which nobody types, still parses to the same term.
        assert_eq!(
            parse("4d6+3kh1").unwrap().dice,
            parse("4d6kh1+3").unwrap().dice
        );
        assert_eq!(
            parse("2d20kh1+3 vs AC16").unwrap(),
            Expression {
                dice: Dice {
                    count: 2,
                    sides: 20,
                    modifier: 3,
                    keep: Some(Keep::Highest(1))
                },
                target: Some(Target {
                    value: 16,
                    word: TargetWord::Ac
                })
            }
        );
        assert_eq!(
            parse("d20+3 vs DC 12").unwrap().target,
            Some(Target {
                value: 12,
                word: TargetWord::Dc
            })
        );
        assert_eq!(
            parse("1d20 vs 15").unwrap().target,
            Some(Target {
                value: 15,
                word: TargetWord::Bare
            })
        );
        // Spaces where a hand would put them.
        assert_eq!(parse("2d8 + 4 vs AC 16").unwrap().target.unwrap().value, 16);
        assert_eq!(parse("2D20KL1+3").unwrap().dice.keep, Some(Keep::Lowest(1)));
    }

    /// `AC` and `DC` are one rule with two words, so the only difference in what is stored
    /// is the word itself.
    #[test]
    fn dc_is_an_alias_for_ac_and_only_changes_the_word() {
        let ac = parse("d20+3 vs AC12").unwrap();
        let dc = parse("d20+3 vs DC12").unwrap();
        assert_eq!(ac.dice, dc.dice);
        assert_eq!(ac.target.unwrap().value, dc.target.unwrap().value);
        assert_eq!(ac.target.unwrap().word, TargetWord::Ac);
        assert_eq!(dc.target.unwrap().word, TargetWord::Dc);
    }

    /// Every refusal names what broke, in words — the reason this parser is hand-rolled.
    #[test]
    fn every_refusal_names_what_broke() {
        let said = |line: &str| parse(line).map(|_| ()).unwrap_err().said();
        assert_eq!(said(""), "no dice to work out");
        assert_eq!(said("3d"), "a die needs a number of faces: `2d8`, not `2d`");
        assert_eq!(said("2d0"), "a die with no faces cannot land");
        assert_eq!(said("3 apples"), "“3 apples” is not a dice expression");
        assert_eq!(said("2d8kh"), "keep what? `4d6kh1` keeps the highest one");
        assert_eq!(said("2d8kh0"), "keeping no dice is not a roll");
        assert_eq!(
            said("2d8kh5"),
            "cannot keep 5 of 2: the roll has fewer dice than that"
        );
        assert_eq!(said("2d8+4 vs"), "meets or beats what? `vs AC16`");
        assert_eq!(
            said("2d8+4 vs AC16 and more"),
            "nothing after a dice expression (and more)"
        );
        assert!(said("101d20").contains("101 dice"));
        assert!(said("2d1001").contains("d1001"));
    }

    /// The `!p` prefix is a whole word at the start of the line: the chip answers a request,
    /// it does not search the prose for dice.
    #[test]
    fn the_request_word_is_a_whole_word_at_the_line_start() {
        assert_eq!(request("!p 2d8+4 vs AC16"), Some("2d8+4 vs AC16"));
        assert_eq!(request("  !p d20"), Some("d20"));
        assert_eq!(request("!p"), Some(""));
        assert_eq!(request("!private notes"), None);
        assert_eq!(request("roll !p 2d8"), None);
        assert_eq!(
            request("2d8+4"),
            None,
            "a bare expression is writing, not a request"
        );
    }
}
