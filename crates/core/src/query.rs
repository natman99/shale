//! Which notes, not which lines: the operator grammar for vault search.
//!
//! P4.14 ruled **one grammar, one parser**, shared with `.shq` (P4.3): inventing it
//! twice is how the two surfaces end up disagreeing about what `tag:` means. So this
//! module knows only the *shape* of a query — the terms, the operators, and what
//! each operator is being asked — and says nothing about SQL, which notes match,
//! or where the answer comes from. The index answers ([`crate::db::Db::notes_matching`]).
//!
//! ## The grammar
//!
//! A query is a sequence of tokens. A token is either an **operator**
//! (`key:value`, or `key`/`key<op>value` for a property) or **text**. Text tokens
//! are joined back into one string and matched literally against a line, exactly
//! as vault search always matched — operators narrow *which notes* that line scan
//! runs over; they never change what a plain query means.
//!
//! ```text
//! tag:monster                the note carries the tag
//! path:notes/dnd/            the vault-relative path matches this glob
//! link:Zhentarim             the note links to that note (by name or path)
//! prop:cr<=5                 a property compared by kind
//! prop:session               the note writes that key at all
//! modified:7d                indexed as modified in the last 7 days
//! ```
//!
//! Values may be quoted when they hold spaces: `tag:"big bad"`,
//! `prop:"Max HP">=10`. Multiple operators **and** together.
//!
//! ## Two rules worth naming
//!
//! **A near-miss operator is refused out loud, not searched as prose.** Typing
//! `tags:monster` and getting "0 of 0 matching" for a literal string is a silent
//! lie; the query reports it in [`Query::unknown`] and the surface says so. A token
//! whose value starts with `//` is exempt — that is a URL, not a typo, and people
//! do search for `http://`.
//!
//! **A comparison never casts.** `prop:cr<=5` is a *numeric* comparison, so it
//! reads only rows the index recorded as numbers. A note that says `cr: low` is
//! not a note with a low challenge rating; it is a note whose `cr` means something
//! else, and it must not sort to the top of the list. SQLite would happily compute
//! `CAST('low' AS REAL)` as `0` and say nothing — the kind filter is what stops
//! that, and the same reasoning is why the index stores a `kind` column at all.

use crate::frontmatter::Kind;

/// How a property's value is being compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    /// `prop:key` with no operator: the note writes the key at all.
    Has,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cmp {
    /// A comparison that asks for an ordering, and therefore for numbers.
    pub fn orders(&self) -> bool {
        matches!(self, Cmp::Lt | Cmp::Le | Cmp::Gt | Cmp::Ge)
    }
}

/// One operator of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// `tag:foo` — the note carries this tag, from the `tags` table, which holds
    /// both body `#tags` and the `tags:` property (P4.3a M2).
    Tag(String),
    /// `path:glob` — matched against the vault-relative path with
    /// [`crate::glob::covers`], the same matcher the ignore rules use.
    Path(String),
    /// `link:Foo` — the note links to the note named `Foo`, by stem or by path.
    Link(String),
    /// `prop:key<op>value` — a property compared by kind.
    Prop {
        key: String,
        cmp: Cmp,
        /// `None` only for [`Cmp::Has`].
        value: Option<String>,
    },
    /// `modified:7d` / `modified:2026-11-01` — the note's indexed modification
    /// time. A duration counts back from now; a date counts from midnight.
    Modified(After),
}

/// The moment a `modified:` operator counts back from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After {
    /// Strictly later than "now minus this many seconds".
    Since(i64),
    /// Modified at or after this unix time (local midnight of the given date).
    At(i64),
}

/// A parsed query. Parsing never fails: what cannot be understood is reported
/// rather than guessed at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// The bare text, joined by single spaces, matched literally against a line.
    /// `None` when the query is operators only — which is a question about *notes*,
    /// and gets one row per note rather than one row per line.
    pub text: Option<String>,
    pub ops: Vec<Op>,
    /// Tokens that look like an operator and are not one — `tags:foo`, `path:x`.
    /// A surface must say these out loud.
    pub unknown: Vec<String>,
}

impl Query {
    /// Whether answering this query needs the vault index.
    pub fn needs_index(&self) -> bool {
        !self.ops.is_empty()
    }

    /// Whether the query is empty enough to have no answer at all.
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.ops.is_empty()
    }
}

/// The operator keys, in the spelling the user types. Lowercase; the parser
/// lowercases the key it reads, so `Tag:foo` works.
const KEYS: [&str; 5] = ["tag", "path", "link", "prop", "modified"];

/// Parse a query. See the module comment for the grammar.
pub fn parse(input: &str) -> Query {
    let mut q = Query::default();
    let mut text: Vec<String> = Vec::new();

    for token in tokens(input) {
        match operator(&token) {
            Some(Parsed::Op(op)) => q.ops.push(op),
            Some(Parsed::Unknown(raw)) => q.unknown.push(raw),
            None => text.push(token),
        }
    }

    if !text.is_empty() {
        q.text = Some(text.join(" "));
    }
    q
}

/// What a token turned out to be.
enum Parsed {
    Op(Op),
    /// The token as typed, for the message.
    Unknown(String),
}

/// Split on whitespace, honouring `"..."` so a value can hold a space.
///
/// Quotes are stripped from the pieces they wrap; an unterminated quote runs to
/// the end of the input rather than failing, because a half-typed query is the
/// common case while the user is still typing.
fn tokens(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(current);
    }
    out
}

/// Read one token as an operator, or report it as text.
fn operator(token: &str) -> Option<Parsed> {
    // `prop:` is the only key whose value can itself hold an operator, and its
    // key can hold a space when quoted, so it is read by shape rather than by
    // splitting on the first colon.
    let (key, value) = token.split_once(':')?;
    let key = key.to_lowercase();
    if key == "prop" {
        if value.is_empty() || !prop_is_answerable(value) {
            return Some(Parsed::Unknown(token.to_string()));
        }
        return prop(value).map(Parsed::Op);
    }
    if !KEYS.contains(&key.as_str()) {
        // A URL is not a typo. Everything else shaped like an operator is.
        if value.starts_with("//") || key.is_empty() {
            return None;
        }
        return Some(Parsed::Unknown(token.to_string()));
    }
    if value.is_empty() {
        // `tag:` with nothing after it is a half-typed query, not "notes with an
        // empty tag": say nothing and let the user finish.
        return Some(Parsed::Unknown(token.to_string()));
    }
    let op = match key.as_str() {
        "tag" => Op::Tag(value.to_string()),
        "path" => Op::Path(value.to_string()),
        "link" => Op::Link(value.to_string()),
        "modified" => Op::Modified(after(value)?),
        _ => return None,
    };
    Some(Parsed::Op(op))
}

/// Read the body of a `prop:` token: `key`, an optional comparison, a value.
fn prop(rest: &str) -> Option<Op> {
    // The comparison operators, longest first so `<=` is not read as `<` then `=`.
    for (spelling, cmp) in [
        ("!=", Cmp::Ne),
        ("<=", Cmp::Le),
        (">=", Cmp::Ge),
        ("=", Cmp::Eq),
        ("<", Cmp::Lt),
        (">", Cmp::Gt),
    ] {
        if let Some((key, value)) = rest.split_once(spelling) {
            if !key.is_empty() {
                return Some(Op::Prop {
                    key: key.to_string(),
                    cmp,
                    value: Some(value.to_string()),
                });
            }
        }
    }
    // No comparison operator was found, so the token asks only whether the note
    // writes the key at all.
    Some(Op::Prop {
        key: rest.to_string(),
        cmp: Cmp::Has,
        value: None,
    })
}

/// Whether a `prop:` token can be answered. An ordering comparison against
/// something that is not a number is refused rather than evaluated: SQLite would
/// read `CAST('low' AS REAL)` as `0` on the query side too, and `prop:cr<=low`
/// would quietly mean "cr at most 0".
fn prop_is_answerable(token: &str) -> bool {
    match prop(
        token
            .trim_start_matches("prop:")
            .trim_start_matches("Prop:"),
    ) {
        Some(Op::Prop {
            cmp,
            value: Some(value),
            ..
        }) if cmp.orders() => value.parse::<f64>().is_ok(),
        _ => true,
    }
}

/// Read a `modified:` value: a duration counted back, or a date.
fn after(value: &str) -> Option<After> {
    let value = value.trim();
    if let Some(unit) = value.strip_suffix(['d', 'h', 'm']) {
        let n: i64 = unit.parse().ok()?;
        let seconds: i64 = match value.as_bytes().last()? {
            b'd' => 86_400,
            b'h' => 3_600,
            _ => 60,
        };
        return Some(After::Since(n.checked_mul(seconds)?));
    }
    // `YYYY-MM-DD`, in the local timezone, from midnight.
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (
        parts.first()?.parse().ok()?,
        parts.get(1)?.parse().ok()?,
        parts.get(2)?.parse().ok()?,
    );
    Some(After::At(days_from_civil(y, m, d)? * 86_400))
}

/// A date as `YYYY-MM-DD`, UTC, from a unix timestamp.
///
/// The inverse of [`days_from_civil`], and the reason a surface that *shows* a
/// `modified:` answer can name the same day the operator compared. UTC on purpose:
/// a day should not move when the machine's timezone does. Howard Hinnant's
/// `civil_from_days` — a date library is not wanted for one format.
pub fn civil_date(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400) + 719_468; // days since 0000-03-01
    let era = days.div_euclid(146_097); // 400-year eras
    let doe = days.rem_euclid(146_097); // day of era
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of era-relative year
    let mp = (5 * doy + 2) / 153; // month counting from March
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since the unix epoch for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`, which shifts the year so March starts it).
fn days_from_civil(y: i64, m: i64, d: i64) -> Option<i64> {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// The kind a `prop:` comparison should read, given what the value looks like.
///
/// This is the rule that keeps `CAST('low' AS REAL)` from quietly becoming `0`:
/// a numeric comparison reads numeric rows, and nothing else.
pub fn wanted(cmp: Cmp, value: &str) -> Option<Kind> {
    if cmp == Cmp::Has {
        return None;
    }
    if cmp.orders() {
        return Some(Kind::Number);
    }
    // `=` and `!=` follow the value: a quoted value is text even when it looks
    // like a number, and a bare number is a number.
    match value.parse::<f64>() {
        Ok(_) => Some(Kind::Number),
        Err(_) => Some(Kind::Text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(input: &str) -> Vec<Op> {
        parse(input).ops
    }

    #[test]
    fn a_plain_query_is_still_just_text() {
        let q = parse("goblin lair");
        assert_eq!(q.text.as_deref(), Some("goblin lair"));
        assert!(q.ops.is_empty());
        assert!(!q.needs_index());
    }

    #[test]
    fn each_operator_is_read() {
        assert_eq!(ops("tag:monster"), vec![Op::Tag("monster".into())]);
        assert_eq!(ops("path:notes/"), vec![Op::Path("notes/".into())]);
        assert_eq!(ops("link:Zhentarim"), vec![Op::Link("Zhentarim".into())]);
        assert_eq!(
            ops("prop:cr<=5"),
            vec![Op::Prop {
                key: "cr".into(),
                cmp: Cmp::Le,
                value: Some("5".into())
            }]
        );
        assert_eq!(
            ops("prop:session"),
            vec![Op::Prop {
                key: "session".into(),
                cmp: Cmp::Has,
                value: None
            }]
        );
    }

    /// Text and operators coexist: the operators choose the notes, the text
    /// chooses the line.
    #[test]
    fn text_survives_around_operators() {
        let q = parse("tag:monster lair");
        assert_eq!(q.text.as_deref(), Some("lair"));
        assert_eq!(q.ops.len(), 1);
    }

    /// A quoted value can hold a space, and the quotes are not part of it.
    #[test]
    fn a_quoted_value_holds_a_space() {
        assert_eq!(ops(r#"tag:"big bad""#), vec![Op::Tag("big bad".into())]);
        assert_eq!(
            ops(r#"prop:"Max HP">=10"#),
            vec![Op::Prop {
                key: "Max HP".into(),
                cmp: Cmp::Ge,
                value: Some("10".into())
            }]
        );
    }

    /// `<=` must not be read as `<` followed by `=`, which would compare against
    /// the string `=5` and find nothing.
    #[test]
    fn the_two_character_operators_win_their_one_character_prefix() {
        for (spelling, cmp) in [("!=", Cmp::Ne), ("<=", Cmp::Le), (">=", Cmp::Ge)] {
            let q = parse(&format!("prop:x{spelling}5"));
            let Op::Prop { cmp: got, .. } = q.ops.first().expect("one operator") else {
                panic!("not a property operator: {q:?}");
            };
            assert_eq!(*got, cmp, "{spelling}");
        }
    }

    /// A near-miss operator is reported, not searched as prose: `tags:monster`
    /// returning "0 of 0 matching" for a literal string is a silent lie.
    #[test]
    fn a_misspelled_operator_is_refused_out_loud() {
        let q = parse("tags:monster");
        assert!(q.ops.is_empty());
        assert!(q.text.is_none());
        assert_eq!(q.unknown, vec!["tags:monster"]);
    }

    /// A URL is not a typo.
    #[test]
    fn a_url_is_text_not_a_misplaced_operator() {
        let q = parse("http://example.com");
        assert_eq!(q.text.as_deref(), Some("http://example.com"));
        assert!(q.unknown.is_empty());
    }

    /// A half-typed operator is not a question about empty values.
    #[test]
    fn a_trailing_colon_is_incomplete_not_a_query() {
        let q = parse("tag:");
        assert!(q.ops.is_empty());
        assert_eq!(q.unknown, vec!["tag:"]);
    }

    #[test]
    fn modified_reads_a_duration_and_a_date() {
        assert_eq!(
            ops("modified:7d"),
            vec![Op::Modified(After::Since(7 * 86_400))]
        );
        assert_eq!(
            ops("modified:24h"),
            vec![Op::Modified(After::Since(24 * 3_600))]
        );
        // The arithmetic is the point: a wrong epoch puts every note on the wrong
        // side of the filter, and nothing downstream can tell. These three values
        // are checked against the calendar, not against this function.
        let Op::Modified(After::At(secs)) = &ops("modified:2026-11-08")[0] else {
            panic!("expected a date");
        };
        assert_eq!(*secs, 1_794_096_000);
        // The day stamps a `modified:` answer names, and the rotation names backup
        // files by. Known pairs, including the two edges the algorithm gets wrong
        // if the March-based year is off: the January side of a year boundary, and
        // a leap day.
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(1_709_251_200), "2024-03-01"); // the March epoch shift
        assert_eq!(civil_date(1_704_067_199), "2023-12-31"); // one second before 2024
        assert_eq!(civil_date(951_782_400), "2000-02-29"); // the leap day
        assert_eq!(civil_date(1_793_404_800), "2026-10-31");
        assert_eq!(days_from_civil(1970, 1, 1), Some(0));
        assert_eq!(days_from_civil(2000, 3, 1), Some(11_017));
        assert_eq!(days_from_civil(2026, 11, 8), Some(20_765));
    }

    /// `prop:cr<=low` is the same silent lie as `cr: low`: SQLite reads
    /// `CAST('low' AS REAL)` as 0 on the query side too, so the comparison is
    /// refused instead of quietly meaning "cr at most 0".
    #[test]
    fn an_ordering_comparison_needs_a_number_on_its_right() {
        let q = parse("prop:cr<=low");
        assert!(q.ops.is_empty());
        assert_eq!(q.unknown, vec!["prop:cr<=low"]);
        // A number with a unit is not a number either.
        assert!(parse("prop:cr<=5kg").unknown.len() == 1);
        // `=` is not an ordering, and text is a fair thing to compare for equality.
        assert_eq!(parse("prop:cr=low").ops.len(), 1);
    }

    /// The rule that keeps a prose value from being compared as a number.
    #[test]
    fn a_comparison_asks_for_the_kind_its_value_is() {
        assert_eq!(wanted(Cmp::Le, "5"), Some(Kind::Number));
        assert_eq!(wanted(Cmp::Eq, "low"), Some(Kind::Text));
        assert_eq!(wanted(Cmp::Eq, "5"), Some(Kind::Number));
        assert_eq!(wanted(Cmp::Has, ""), None);
    }

    /// An unterminated quote is a half-typed query, not a parse failure.
    #[test]
    fn an_unterminated_quote_runs_to_the_end() {
        assert_eq!(ops(r#"tag:"mon"#), vec![Op::Tag("mon".into())]);
    }
}
