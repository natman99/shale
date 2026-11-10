//! The `---` block at the top of a note, read as data.
//!
//! This is the parser P4.3a was priced against: the vault's notes carry frontmatter,
//! and until now nothing in `crates/` read it, which is why the `edges` table has had
//! no writer and a `.shq` query could only ever ask about paths, body tags and body
//! links. The design and the edge ledger are in `properties-plan.md`; the numbers in
//! the comments below are its EDGE cases, and each one has a test.
//!
//! # What is in scope
//!
//! The `---` fence at byte 0; `key: value` pairs; block lists (`- item`); flow lists
//! (`[a, b]`, read-only — Obsidian writes block lists); single- and double-quoted
//! scalars with escapes; comments and blank lines; an empty block; and JSON
//! frontmatter, parsed with the `serde_json` already in the tree and never rewritten
//! back as YAML (EDGE-14).
//!
//! # What is not, on purpose
//!
//! Nested maps and sequences of maps, anchors and aliases (`&x` / `*x`), explicit tags
//! (`!!str`), multi-document separators, multi-line scalars (`|`, `>`), and JSON inside
//! a YAML value. None of these are errors and none are dropped: a value Shale cannot
//! read is indexed **inert** as [`Kind::Nested`] with its raw text preserved, and Shale
//! never rewrites the block, so a plugin-authored structure survives Shale untouched
//! (ruling §1.5). Anchors and tags land as [`Kind::Text`] carrying the text as written,
//! which is the same rule seen from the other side: Shale stores what the author typed,
//! not a guess at what it meant.
//!
//! # Error posture
//!
//! Inherited whole from the indexer's: **one unreadable block costs its own note's
//! props, not the pass.** A block that will not parse at all ([`Block::malformed`])
//! leaves the note with no props and says why, and the caller reports it the way the
//! index reports a note it could not read.

/// Whether a key feeds the `tags` projection. Case-significant, like every other
/// key: `Tags:` is your key, `tags:` is Shale's.
pub fn is_tag_key(key: &str) -> bool {
    key == "tags" || key == "tag"
}

/// What kind a property's value is.
///
/// Stored as text in `props.kind`, and the whole trick of the layout: the type
/// question is answered in Rust, at index time, where a parser exists — not in SQL,
/// where `CAST('low' AS REAL)` is `0` and a monster whose `cr:` is prose would sort
/// first without complaining.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Text,
    Number,
    Date,
    DateTime,
    Bool,
    /// A value that is a wikilink, and therefore a candidate for the `edges` table.
    Link,
    /// Structure Shale does not read (a nested map, a block scalar). Indexed so the
    /// key still shows up in the census, and never rewritten (ruling §1.5).
    Nested,
}

impl Kind {
    /// The spelling the column holds.
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Number => "number",
            Kind::Date => "date",
            Kind::DateTime => "datetime",
            Kind::Bool => "bool",
            Kind::Link => "link",
            Kind::Nested => "nested",
        }
    }

    /// The spelling a declaration in `.shale/props.toml` uses. Unknown is not an
    /// error worth failing a vault over: the key keeps its inferred kind.
    pub fn parse(name: &str) -> Option<Kind> {
        match name.trim().to_ascii_lowercase().as_str() {
            "text" | "string" => Some(Kind::Text),
            "number" | "num" => Some(Kind::Number),
            "date" => Some(Kind::Date),
            "datetime" | "time" => Some(Kind::DateTime),
            "bool" | "boolean" | "checkbox" => Some(Kind::Bool),
            "link" => Some(Kind::Link),
            "nested" => Some(Kind::Nested),
            _ => None,
        }
    }
}

/// One key as the whole vault writes it. The census is what makes a kind
/// disagreement something a person can see: the index keeps each value's own kind
/// rather than smoothing a key into one type, so the answer to "what is `cr`?" is a
/// list, and a list longer than one is the typo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCensus {
    /// The key as authored, case and spaces kept.
    pub key: String,
    /// How many notes write it. Not the same as `values`: one note can write a key
    /// twice, and a reader deciding whether a key is worth renaming cares about
    /// notes.
    pub notes: usize,
    /// How many values the vault holds under it, counting repeats within a note.
    pub values: usize,
    /// Every kind those values come in, in `Kind`'s own order. More than one is a
    /// disagreement, not a mixture the index has reconciled.
    pub kinds: Vec<Kind>,
}

/// One authored property, as its row in `props` will hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prop {
    /// Exactly as authored: case kept, spaces allowed (EDGE-6, EDGE-7). Nothing here
    /// ever lowercases or substitutes `_`, so a vocabulary cleanup is a rename the
    /// author asks for rather than something that happened to them.
    pub key: String,
    /// The authored text, unquoted. The one exception is a key *declared* numeric
    /// whose value is a bare fraction: see [`normalize_number`].
    pub value: String,
    /// The value's kind, inferred from the value itself unless the key is declared
    /// (ruling §1.2). Per-value rather than per-key on purpose: a key whose values
    /// disagree is a typo worth *seeing*, and the census that shows it is a
    /// `GROUP BY key, kind` over these rows.
    pub kind: Kind,
    /// `None` for a scalar; `0`-based for one item of a list. A key that appears twice
    /// restarts its ordinals, which is what makes a duplicate findable in SQL:
    /// `COUNT(*) > COUNT(DISTINCT COALESCE(ordinal, -1))` for that note and key.
    pub ordinal: Option<usize>,
}

/// What the fence at the top of a note says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The first and last line of the block, fences included, as 0-based line numbers.
    pub lines: (usize, usize),
    /// The same span in bytes, fences included, against the text that was parsed.
    /// This is what lets the spell and grammar checker stop calling YAML prose:
    /// `PROJECT.md` lists frontmatter as the known false-positive case, and the
    /// checker works in offsets, not lines.
    pub bytes: (usize, usize),
    /// Every property the block holds, in authored order, duplicates included.
    pub props: Vec<Prop>,
    /// Keys that appear more than once in this block (EDGE-8). Both are indexed; this
    /// is the flag the census surfaces, not a reason to drop one.
    pub duplicate_keys: Vec<String>,
    /// Set when the block is frontmatter but will not parse. The note then has **no**
    /// props — a half-read block would index a fact the author did not write.
    pub malformed: Option<String>,
}

impl Block {
    /// A block that parsed.
    fn parsed(
        lines: (usize, usize),
        bytes: (usize, usize),
        props: Vec<Prop>,
        duplicate_keys: Vec<String>,
    ) -> Block {
        Block {
            lines,
            bytes,
            props,
            duplicate_keys,
            malformed: None,
        }
    }

    /// A block that is frontmatter but could not be read.
    fn bad(lines: (usize, usize), bytes: (usize, usize), why: impl Into<String>) -> Block {
        Block {
            lines,
            bytes,
            props: Vec::new(),
            duplicate_keys: Vec::new(),
            malformed: Some(why.into()),
        }
    }
}

/// The frontmatter block a note opens with, if it opens with one.
///
/// `None` means there is no frontmatter at all, which is not an error: most notes have
/// none. It is `None` and not an empty block for three cases the ledger names — the
/// fence is not at byte 0 (EDGE-1: a thematic break later in the note is prose), there
/// is no closing fence (EDGE-2: an unterminated block is prose too), and something
/// other than a UTF-8 BOM precedes the fence (EDGE-4).
pub fn block(text: &str) -> Option<Block> {
    // A BOM is tolerated before the fence (EDGE-4), and the byte range has to be
    // stated against the text the caller handed over, so the offset carries it.
    let skip = if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let text = &text[skip..];
    let mut lines = text.split('\n');
    let open = lines.next()?.trim_end_matches('\r');
    if open != FENCE {
        return None;
    }
    // The body of the block, the line the closing fence is on, and where the block
    // ends in bytes. `...` closes a block as well as `---` (EDGE-2), because that is
    // what YAML says and what a hand-authored note sometimes does.
    let mut body: Vec<&str> = Vec::new();
    let mut close: Option<usize> = None;
    let mut at = skip + open.len() + 1;
    let mut end = at;
    for (ix, line) in lines.by_ref().enumerate() {
        let bare = line.trim_end_matches('\r');
        if bare == FENCE || bare == "..." {
            close = Some(ix + 1);
            end = at + line.len();
            break;
        }
        at += line.len() + 1;
        body.push(line);
    }
    let close = close?;
    Some(parse((0, close), (skip, end), &body))
}

/// The line that opens and closes a frontmatter block.
///
/// Public because the app writes a block too — the palette's add-properties command
/// — and two spellings of the fence is how a note ends up with a block the parser
/// does not see.
pub const FENCE: &str = "---";

/// Read the lines between the fences.
fn parse(lines: (usize, usize), bytes: (usize, usize), body: &[&str]) -> Block {
    // JSON frontmatter is a different shape, and Obsidian reads it, so it gets its
    // own arm rather than being forced through the line grammar (EDGE-14). It is read
    // and never rewritten: rewriting would silently reformat someone's file.
    let opens_json = body
        .iter()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.trim_start().starts_with('{'));
    if opens_json {
        return json_block(lines, bytes, body);
    }

    let mut props: Vec<Prop> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut duplicates: Vec<String> = Vec::new();
    let mut ix = 0;
    while ix < body.len() {
        let raw = body[ix];
        let line = raw.trim();
        // Blank lines and comments are not properties (EDGE-9). A `#` inside a quoted
        // value is part of the value, which is why comment handling happens here, on
        // the whole line, before any value is looked at (EDGE-10).
        if line.is_empty() || line.starts_with('#') {
            ix += 1;
            continue;
        }
        // A list item or an indented continuation with no key of its own is either
        // nested structure or a malformed block; the key branch below decides which.
        let (key, rest) = match split_key(line) {
            Some(pair) => pair,
            None => {
                return Block::bad(lines, bytes, "a line that is not a `key: value` pair");
            }
        };
        if seen.iter().any(|k| k == &key) && !duplicates.contains(&key) {
            duplicates.push(key.clone());
        }
        seen.push(key.clone());

        // A key with nothing after it introduces a block list, a nested map, or an
        // empty value (EDGE-18) — which of those the following lines decide.
        let rest = rest.trim();
        if rest.is_empty() {
            let (items, consumed) = block_items(body, ix + 1);
            match items {
                Items::List(items) => {
                    for (ordinal, item) in items.into_iter().enumerate() {
                        props.push(Prop {
                            key: key.clone(),
                            value: item,
                            kind: Kind::Text,
                            ordinal: Some(ordinal),
                        });
                    }
                    // The kinds are re-derived per item below, so a list of numbers
                    // is a list of numbers.
                    fix_kinds(&mut props, &key);
                    ix += 1 + consumed;
                    continue;
                }
                Items::Nested(raw) => {
                    props.push(Prop {
                        key,
                        value: raw,
                        kind: Kind::Nested,
                        ordinal: None,
                    });
                    ix += 1 + consumed;
                    continue;
                }
                Items::Empty => {
                    props.push(Prop {
                        key,
                        value: String::new(),
                        kind: Kind::Text,
                        ordinal: None,
                    });
                    ix += 1;
                    continue;
                }
            }
        }

        // A block scalar (`|`, `>`) is out of scope, and its value is the indented
        // region that follows: inert, raw, the same bucket a nested map lands in.
        if matches!(rest, "|" | ">" | "|-" | ">-" | "|+" | ">+") {
            let (items, consumed) = block_items(body, ix + 1);
            let raw = match items {
                Items::Nested(raw) => raw,
                Items::List(items) => items.join("\n"),
                Items::Empty => String::new(),
            };
            props.push(Prop {
                key,
                value: raw,
                kind: Kind::Nested,
                ordinal: None,
            });
            ix += 1 + consumed;
            continue;
        }

        // A comment is stripped before anything else is asked of the value, because
        // the quote state decides whether a `#` is a comment at all (EDGE-10).
        let rest = strip_comment(rest);
        // A value that is unquoted and still holds `: ` is malformed YAML — the split
        // point is genuinely ambiguous, so the block is refused rather than guessed at
        // (EDGE-11). `time: 10:30` is fine: a colon only ends a key when a space (or
        // the end of the line) follows it.
        if !is_quoted(rest) && rest.contains(": ") {
            return Block::bad(
                lines,
                bytes,
                format!("`{key}: {rest}` has an unquoted `: `"),
            );
        }

        // Asked before the flow list, because in strict YAML `[[Target]]` *is* a
        // nested sequence — and reading it as the link the author meant is the one
        // place this parser is wider than YAML on purpose (see [`wikilink`]).
        let (values, list) = if wikilink(rest).is_some() {
            (vec![unquote(rest)], false)
        } else {
            match flow_list(rest) {
                Some(items) => (items, true),
                None => (vec![unquote(rest)], false),
            }
        };
        for (ordinal, value) in values.into_iter().enumerate() {
            props.push(Prop {
                kind: value_kind(&value),
                value,
                key: key.clone(),
                ordinal: list.then_some(ordinal),
            });
        }
        ix += 1;
    }
    Block::parsed(lines, bytes, props, duplicates)
}

/// What follows a key with nothing after it.
enum Items {
    /// `- a`, `- b`, … at a deeper indent than the key.
    List(Vec<String>),
    /// Indented lines that are not list items: a nested map, a multi-line scalar, or
    /// anything else Shale does not read. Raw text, preserved (EDGE-13).
    Nested(String),
    /// Nothing follows at all.
    Empty,
}

/// The block list (or nested region) that belongs to the key at `at - 1`.
fn block_items(body: &[&str], at: usize) -> (Items, usize) {
    let mut items: Vec<String> = Vec::new();
    let mut nested: Vec<&str> = Vec::new();
    let mut ix = at;
    while ix < body.len() {
        let raw = body[ix];
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            // A blank line inside a list is allowed by YAML and says nothing about
            // where the list ends, so it is carried and the next line decides.
            ix += 1;
            continue;
        }
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            break; // back to column 0: the key's value is over
        }
        if let Some(item) = line
            .strip_prefix("- ")
            .or_else(|| (line == "-").then_some(""))
        {
            let item = item.trim();
            // A sequence of maps is out of scope, and an item that reads as a pair is
            // the tell. Guessing at it would index a fact the author did not write, so
            // the whole region goes inert with its text preserved instead.
            if split_key(item).is_some() {
                break;
            }
            items.push(unquote(item));
            ix += 1;
            continue;
        }
        // Indented, and not a list item: nested. Take every following indented line
        // verbatim, which is what keeps a `|` block scalar and a plugin's nested map
        // intact for the author and visible, badged, in `:props`.
        while ix < body.len() {
            let raw = body[ix];
            if !raw.trim().is_empty() && !raw.starts_with(' ') && !raw.starts_with('\t') {
                break;
            }
            nested.push(raw);
            ix += 1;
        }
        return (
            Items::Nested(nested.join("\n").trim_end().to_string()),
            ix - at,
        );
    }
    if items.is_empty() {
        (Items::Empty, ix - at)
    } else {
        (Items::List(items), ix - at)
    }
}

/// Split `key: value`, returning the key unquoted and the rest as written.
///
/// A colon ends the key only when a space or the end of the line follows it, which is
/// what makes `time: 10:30` one pair rather than a parse failure.
fn split_key(line: &str) -> Option<(String, String)> {
    let cut = key_cut(line)?;
    let key = unquote(line[..cut].trim());
    if key.is_empty() {
        return None;
    }
    Some((key, line[cut + 1..].to_string()))
}

/// Where a line's `key: value` colon is: the first `:` outside quotes with a space
/// (or the end of the line) after it.
fn key_cut(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut cut = None;
    for (ix, b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b':' if !in_single
                && !in_double
                && (ix + 1 == bytes.len() || bytes[ix + 1] == b' ') =>
            {
                cut = Some(ix);
                break;
            }
            _ => {}
        }
    }
    cut
}

/// The byte range of a line's key **as the author wrote it**, colon included, where
/// the line has one.
///
/// This reads [`key_cut`] — the same colon finder the parser uses — so the key the
/// editor colours and the key the index stores cannot drift apart. A coloured key that
/// is not the parsed key is a claim the parser cannot back up.
pub(crate) fn key_span(line: &str) -> Option<(usize, usize)> {
    let cut = key_cut(line)?;
    let start = line.len() - line.trim_start().len();
    if line[start..cut].trim().is_empty() {
        return None;
    }
    Some((start, cut + 1))
}

/// `[a, b]` as its items. Flow lists are read for compatibility; Obsidian writes block
/// lists, so nothing here needs to write this shape back (EDGE-12).
pub(crate) fn flow_list(value: &str) -> Option<Vec<String>> {
    let inner = value.trim().strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for c in inner.chars() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(c);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(c);
            }
            ',' if !in_single && !in_double => {
                items.push(unquote(current.trim()));
                current = String::new();
            }
            _ => current.push(c),
        }
    }
    items.push(unquote(current.trim()));
    Some(items)
}

/// Drop an unquoted trailing comment (EDGE-10).
///
/// A `#` only starts a comment outside quotes and when it opens the value or follows
/// whitespace, so `tags: "#draft"` keeps its hash and `status: draft # note` does not
/// carry the note.
pub(crate) fn strip_comment(value: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let bytes = value.as_bytes();
    for (ix, b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'#' if !in_single && !in_double && (ix == 0 || bytes[ix - 1] == b' ') => {
                return value[..ix].trim_end();
            }
            _ => {}
        }
    }
    value
}

fn is_quoted(value: &str) -> bool {
    let value = value.trim();
    (value.starts_with('"') && value.ends_with('"') && value.len() > 1)
        || (value.starts_with('\'') && value.ends_with('\'') && value.len() > 1)
}

/// Strip the quotes and undo the escapes a double-quoted scalar can carry.
pub(crate) fn unquote(value: &str) -> String {
    let value = value.trim();
    let inner = if value.starts_with('"') && value.ends_with('"') && value.len() > 1 {
        &value[1..value.len() - 1]
    } else if value.starts_with('\'') && value.ends_with('\'') && value.len() > 1 {
        // Single quotes in YAML take nothing literally except a doubled quote.
        &value[1..value.len() - 1]
    } else {
        return value.to_string();
    };
    if value.starts_with('\'') {
        return inner.replace("''", "'");
    }
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                // An escape Shale does not know is the author's text, kept.
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Re-infer the kinds of the rows just added for `key`, so a block list of numbers is
/// a list of numbers rather than a list of text.
fn fix_kinds(props: &mut [Prop], key: &str) {
    for prop in props.iter_mut().filter(|p| p.key == key) {
        prop.kind = value_kind(&prop.value);
    }
}

/// The kind of one value, by the narrow YAML 1.2 core rules the plan rules in.
///
/// Deliberately narrow, because the wide version is the Norway problem: `yes`, `no`,
/// `on` and `off` are **text**, exactly as Obsidian's checkbox type agrees. `007`,
/// `0x1f`, `1e5` and `1/4` are **text** too (EDGE-17) — a value that only *looks*
/// numeric is not indexed as one, because a query that silently reads `0x1f` as `0` is
/// worse than a query that misses it and a census that says so.
pub fn value_kind(value: &str) -> Kind {
    let value = value.trim();
    if value.is_empty() {
        return Kind::Text;
    }
    if value == "true" || value == "false" {
        return Kind::Bool;
    }
    if let Some(target) = wikilink(value) {
        let _ = target;
        return Kind::Link;
    }
    if is_date(value) {
        return Kind::Date;
    }
    if is_datetime(value) {
        return Kind::DateTime;
    }
    if is_number(value) {
        return Kind::Number;
    }
    Kind::Text
}

/// A value that is *one* wikilink, and the note it names.
///
/// Obsidian requires the quotes (`"[[Target]]"`); an unquoted `[[Target]]` is, in
/// strict YAML, a nested flow sequence, which would index as inert structure. Reading
/// it as a link instead is the one place this parser is wider than YAML on purpose: it
/// is what the author means, it cannot lose data, and it is what feeds `edges`
/// (EDGE-16).
///
/// Two things the shape has to survive:
///
/// * A heading anchor names a *section* of a note, not part of its name. The body
///   scanner has always stripped it (`markdown.rs`); this function used to keep it, so
///   `region: "[[Waterdeep#Government]]"` resolved as the literal
///   `Waterdeep#Government`, missed, and was filed as a ghost — the same link meaning
///   two different things depending on which side of the fence it was written on. The
///   anchor is dropped for resolution; `props.value` keeps the text as authored, so
///   `prop:region="[[Waterdeep#Government]]"` still matches and the reader still sees
///   where in the note the link lands.
/// * A value holding more than one link is not one link. `group: [[A]] [[B]]` starts
///   with `[[` and ends with `]]`, so the old shape test called it a link to
///   `A]] [[B` — a ghost nobody wrote. It is a value with links *in* it, which is what
///   `Kind::Text` says. The way to write several links is a list, one per item, which
///   is also the only shape strict YAML (and therefore Obsidian) can read.
pub fn wikilink(value: &str) -> Option<&str> {
    let value = value.trim();
    let inner = value.strip_prefix("[[")?.strip_suffix("]]")?;
    if inner.contains("[[") || inner.contains("]]") {
        return None;
    }
    let target = inner.split('|').next().unwrap_or(inner);
    let target = target.split('#').next().unwrap_or(target).trim();
    if target.is_empty() {
        // `[[#a heading in this note]]` names a section and no note.
        return None;
    }
    Some(target)
}

fn is_number(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() {
        return false;
    }
    let mut parts = digits.split('.');
    let whole = parts.next().unwrap_or("");
    let frac = parts.next().unwrap_or("");
    if parts.next().is_some() || whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // `1.` and `.5` are not numbers here either: the shape is `1` or `1.5`.
    if !frac.is_empty() && !frac.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // A leading zero means the author wrote something other than a number (`007`).
    if whole.len() > 1 && whole.starts_with('0') {
        return false;
    }
    true
}

fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(|b| b.is_ascii_digit())
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[8..].iter().all(|b| b.is_ascii_digit())
}

fn is_datetime(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 19
        && bytes[10] == b'T'
        && is_date(&value[..10])
        && bytes[11..13].iter().all(|b| b.is_ascii_digit())
        && bytes[13] == b':'
        && bytes[14..16].iter().all(|b| b.is_ascii_digit())
        && bytes[16] == b':'
        && bytes[17..].iter().all(|b| b.is_ascii_digit())
}

/// The number a declared-numeric key's value means, including the fraction spelling
/// the domain actually uses.
///
/// `cr: 1/4` is text in YAML, so a `cr <= 5` query silently drops every CR ¼ monster
/// (EDGE-17). For a key the vault *declares* numeric, a bare `n/m` is indexed as its
/// decimal. The note on disk keeps what the author wrote — the index is a query
/// surface, and the editing surface reads the buffer — so this is not a rewrite.
pub fn normalize_number(value: &str) -> Option<String> {
    let value = value.trim();
    if is_number(value) {
        return Some(value.to_string());
    }
    let (num, den) = value.split_once('/')?;
    let (num, den) = (num.trim(), den.trim());
    if !is_number(num) || !is_number(den) || den == "0" {
        return None;
    }
    let top: f64 = num.parse().ok()?;
    let bottom: f64 = den.parse().ok()?;
    let out = top / bottom;
    // A whole quotient is written whole: `2/1` is `2`, so the census and the display
    // agree with what a reader of the note would say.
    if (out - out.round()).abs() < f64::EPSILON {
        return Some(format!("{}", out.round() as i64));
    }
    Some(format!("{out}"))
}

/// Apply a vault's declaration for a key (ruling §1.2: declared wins) — **where the
/// value can carry the declared kind.**
///
/// The declaration is a claim about the *key*, and a prose value is a claim about the
/// row, and when they disagree the row keeps its truth. Storing `cr: low` as
/// `kind='number'` would be the silent cast the plan refuses: SQLite's
/// `CAST('low' AS REAL)` is 0, so a `cr <= 5` query would rank that monster first and
/// never say so. Keeping it `text` is what makes the disagreement *visible* — the kind
/// census is `GROUP BY key, kind`, and it can only show a conflict between two kinds
/// that both survive in the table.
///
/// `Number` is the only kind with a value to rewrite (EDGE-17's `1/4` → `0.25`);
/// `Text`/`Link`/`Nested` are not shape-checked, so a declaration for those is taken
/// at its word.
pub fn declare(prop: &mut Prop, kind: Kind) {
    let carries = match kind {
        Kind::Number => match normalize_number(&prop.value) {
            Some(value) => {
                prop.value = value;
                true
            }
            None => false,
        },
        Kind::Date | Kind::DateTime | Kind::Bool => value_kind(&prop.value) == kind,
        _ => true,
    };
    if carries {
        prop.kind = kind;
    }
}

/// JSON frontmatter: `---`, one JSON object, `---`.
///
/// Read with the `serde_json` already in the tree, flattened one level, and never
/// written back (EDGE-14). Anything nested inside is inert, same as in YAML.
fn json_block(lines: (usize, usize), bytes: (usize, usize), body: &[&str]) -> Block {
    let text = body.join("\n");
    let value: serde_json::Value = match serde_json::from_str(text.trim()) {
        Ok(value) => value,
        Err(err) => return Block::bad(lines, bytes, format!("JSON frontmatter: {err}")),
    };
    let Some(map) = value.as_object() else {
        return Block::bad(lines, bytes, "JSON frontmatter is not an object");
    };
    let mut props = Vec::new();
    for (key, value) in map {
        match value {
            serde_json::Value::Array(items) => {
                for (ordinal, item) in items.iter().enumerate() {
                    props.push(json_prop(key, item, Some(ordinal)));
                }
            }
            other => props.push(json_prop(key, other, None)),
        }
    }
    Block::parsed(lines, bytes, props, Vec::new())
}

fn json_prop(key: &str, value: &serde_json::Value, ordinal: Option<usize>) -> Prop {
    let (text, kind) = match value {
        serde_json::Value::String(s) => (s.clone(), value_kind(s)),
        serde_json::Value::Bool(b) => (b.to_string(), Kind::Bool),
        serde_json::Value::Number(n) => (n.to_string(), Kind::Number),
        serde_json::Value::Null => (String::new(), Kind::Text),
        // An object or an array inside a value is structure Shale does not read.
        other => (other.to_string(), Kind::Nested),
    };
    Prop {
        key: key.to_string(),
        value: text,
        kind,
        ordinal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The props of a note, as `(key, value, kind, ordinal)` — the shape a test reads
    /// without needing to know which field a row came from.
    fn props(text: &str) -> Vec<(String, String, Kind, Option<usize>)> {
        block(text)
            .map(|b| {
                assert!(b.malformed.is_none(), "unexpected: {:?}", b.malformed);
                b.props
            })
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.key, p.value, p.kind, p.ordinal))
            .collect()
    }

    /// The single property a block holds, for tests about one value.
    fn one(text: &str) -> Prop {
        let mut props = block(text).unwrap().props;
        assert_eq!(props.len(), 1, "expected exactly one prop");
        props.remove(0)
    }

    fn kinds(text: &str) -> Vec<Kind> {
        props(text).into_iter().map(|(_, _, k, _)| k).collect()
    }

    #[test]
    fn a_scalar_pair_and_its_inferred_kind() {
        assert_eq!(
            props("---\ntitle: Goblin Camp\n---\n"),
            vec![("title".into(), "Goblin Camp".into(), Kind::Text, None)]
        );
    }

    /// EDGE-1: a thematic break later in the note is not frontmatter.
    #[test]
    fn a_fence_that_is_not_at_the_top_is_prose() {
        assert!(block("# Title\n---\ntitle: no\n---\n").is_none());
    }

    /// EDGE-2: an unterminated block is prose, and says so by not being frontmatter.
    #[test]
    fn an_unterminated_block_is_not_frontmatter() {
        assert!(block("---\ntitle: Goblin\nbody text\n").is_none());
        // `...` closes a block as well as `---`.
        assert_eq!(
            props("---\ntitle: Goblin\n...\n").len(),
            1,
            "`...` closes the fence"
        );
    }

    /// EDGE-3: an empty block is zero props and not an error.
    #[test]
    fn an_empty_block_is_nothing_not_a_failure() {
        let b = block("---\n---\n").unwrap();
        assert!(b.props.is_empty());
        assert!(b.malformed.is_none());
    }

    /// EDGE-4: a BOM is tolerated; anything else before the fence is not frontmatter.
    #[test]
    fn a_bom_before_the_fence_still_opens_the_block() {
        assert_eq!(props("\u{feff}---\ncr: 1\n---\n").len(), 1);
        assert!(block(" \n---\ncr: 1\n---\n").is_none());
    }

    /// EDGE-5: CRLF parses identically.
    #[test]
    fn crlf_parses_the_same() {
        assert_eq!(
            props("---\r\ncr: 1\r\ntags: a\r\n---\r\n"),
            props("---\ncr: 1\ntags: a\n---\n")
        );
    }

    /// EDGE-6 and EDGE-7: keys with spaces round-trip, and case is significant.
    #[test]
    fn keys_are_kept_exactly_as_authored() {
        let got = props("---\nMax HP: 7\nmax hp: 5\n---\n");
        assert_eq!(
            got.into_iter().map(|(k, _, _, _)| k).collect::<Vec<_>>(),
            vec!["Max HP".to_string(), "max hp".to_string()]
        );
    }

    /// EDGE-8: a duplicate key indexes both rows and flags the note.
    #[test]
    fn a_duplicate_key_is_indexed_twice_and_flagged() {
        let b = block("---\ntags: a\ntags: b\n---\n").unwrap();
        assert_eq!(b.props.len(), 2, "neither one is silently dropped");
        assert_eq!(b.duplicate_keys, vec!["tags".to_string()]);
        // Ordinals restart per occurrence, which is what makes the duplicate findable
        // in SQL without a column for it.
        assert_eq!(
            b.props.iter().map(|p| p.ordinal).collect::<Vec<_>>(),
            vec![None, None]
        );
        let b = block("---\ntags:\n  - a\ntags:\n  - b\n---\n").unwrap();
        assert_eq!(
            b.props.iter().map(|p| p.ordinal).collect::<Vec<_>>(),
            vec![Some(0), Some(0)],
            "a repeated ordinal is the duplicate's signature"
        );
    }

    /// EDGE-9: comments and blank lines are not properties.
    #[test]
    fn comments_and_blank_lines_are_not_props() {
        assert_eq!(
            props("---\n# a comment\n\ncr: 1\n\n# another\n---\n"),
            vec![("cr".into(), "1".into(), Kind::Number, None)]
        );
    }

    /// EDGE-10: a `#` inside a quoted value is part of the value.
    #[test]
    fn a_hash_inside_quotes_is_part_of_the_value() {
        assert_eq!(
            props("---\ntags: \"#draft\"\n---\n"),
            vec![("tags".into(), "#draft".into(), Kind::Text, None)]
        );
        assert_eq!(
            props("---\nstatus: draft # a comment\n---\n"),
            vec![("status".into(), "draft".into(), Kind::Text, None)],
            "an unquoted `#` starts a comment, and the value stops there"
        );
    }

    /// EDGE-11: an unquoted value holding `: ` is malformed — refuse, do not guess.
    #[test]
    fn an_unquoted_colon_space_refuses_the_block() {
        let b = block("---\nsummary: a: b\n---\n").unwrap();
        assert!(b.malformed.is_some());
        assert!(b.props.is_empty(), "a half-read block indexes nothing");
        // A colon that is not followed by a space is part of the value.
        assert_eq!(
            props("---\ntime: 10:30\n---\n"),
            vec![("time".into(), "10:30".into(), Kind::Text, None)]
        );
    }

    /// EDGE-12: flow lists are read.
    #[test]
    fn a_flow_list_is_read() {
        assert_eq!(
            props("---\ntags: [a, b]\n---\n"),
            vec![
                ("tags".into(), "a".into(), Kind::Text, Some(0)),
                ("tags".into(), "b".into(), Kind::Text, Some(1)),
            ]
        );
    }

    /// A block list, with each item's kind inferred.
    #[test]
    fn a_block_list_keeps_its_order_and_its_kinds() {
        assert_eq!(
            props("---\ncrs:\n  - 1\n  - 1/2\n  - low\n---\n"),
            vec![
                ("crs".into(), "1".into(), Kind::Number, Some(0)),
                ("crs".into(), "1/2".into(), Kind::Text, Some(1)),
                ("crs".into(), "low".into(), Kind::Text, Some(2)),
            ]
        );
    }

    /// EDGE-13: nested structure is inert, raw, and still indexed.
    #[test]
    fn a_nested_value_is_inert_and_keeps_its_text() {
        let got = props("---\nquest:\n  name: The Lost Mine\n  goblins: 3\n---\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, Kind::Nested);
        assert!(
            got[0].1.contains("name: The Lost Mine"),
            "the raw text survives: {}",
            got[0].1
        );
    }

    /// A multi-line scalar is out of scope, and lands in the same inert bucket.
    #[test]
    fn a_block_scalar_is_nested_not_guessed() {
        let got = props("---\nnotes: |\n  line one\n  line two\n---\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, Kind::Nested);
    }

    /// EDGE-14: JSON frontmatter is read, one level, and never rewritten.
    #[test]
    fn json_frontmatter_is_read() {
        let mut got =
            props("---\n{\"cr\": 3, \"tags\": [\"a\", \"b\"], \"quest\": {\"x\": 1}}\n---\n");
        // A JSON object's order is not kept, and props rows have no order contract, so
        // the test asks which rows exist rather than what order they arrived in.
        got.sort();
        assert_eq!(
            got,
            vec![
                ("cr".into(), "3".into(), Kind::Number, None),
                ("quest".into(), "{\"x\":1}".into(), Kind::Nested, None),
                ("tags".into(), "a".into(), Kind::Text, Some(0)),
                ("tags".into(), "b".into(), Kind::Text, Some(1)),
            ]
        );
    }

    /// EDGE-16: a wikilink value is a link, quoted or not, and its target is what a
    /// projection will use.
    #[test]
    fn a_wikilink_value_is_a_link() {
        assert_eq!(
            props("---\nally: \"[[Zhentarim]]\"\n---\n"),
            vec![("ally".into(), "[[Zhentarim]]".into(), Kind::Link, None)]
        );
        assert_eq!(wikilink("[[Zhentarim|the Zhents]]"), Some("Zhentarim"));
        assert_eq!(
            kinds("---\nally: [[Zhentarim]]\n---\n"),
            vec![Kind::Link],
            "unquoted is what authors actually write"
        );
    }

    /// The anchor names a section of the note, not part of its name — which is what the
    /// body scanner has always done, and what this function used not to.
    #[test]
    fn a_link_to_a_heading_is_a_link_to_the_note() {
        assert_eq!(wikilink("[[Waterdeep#Government]]"), Some("Waterdeep"));
        assert_eq!(
            wikilink("[[Waterdeep#The Fall|its end]]"),
            Some("Waterdeep")
        );
        assert_eq!(
            props("---\nregion: \"[[Waterdeep#Government]]\"\n---\n"),
            vec![(
                "region".into(),
                "[[Waterdeep#Government]]".into(),
                Kind::Link,
                None
            )],
            "the row keeps the anchor the author wrote; only the resolution drops it"
        );
        assert_eq!(
            wikilink("[[#a heading in this note]]"),
            None,
            "a link to a section of this note names no note"
        );
    }

    /// Several links in one value is not one link, and inventing a shape for it would
    /// be inventing a dialect strict YAML — and therefore Obsidian — cannot read.
    #[test]
    fn a_value_holding_two_links_is_not_one_link() {
        assert_eq!(wikilink("[[Federation]] [[Empire]]"), None);
        assert_eq!(
            kinds("---\ngroup: [[Federation]] [[Empire]]\n---\n"),
            vec![Kind::Text],
            "the old shape test called this one link to `Federation]] [[Empire`"
        );
        assert_eq!(
            kinds("---\ngroup: [\"[[Federation]]\", \"[[Empire]]\"]\n---\n"),
            vec![Kind::Link, Kind::Link],
            "the list is how a value carries more than one link"
        );
    }

    /// EDGE-17: the YAML-core kinds, narrowly.
    #[test]
    fn kinds_follow_yaml_core_not_yaml_1_1() {
        assert_eq!(
            kinds(
                "---\na: true\nb: yes\nc: 007\nd: 0x1f\ne: 1e5\nf: 1/4\ng: 1.5\nh: -2\ni: 2024-01-31\nj: 2024-01-31T09:00:00\n---\n"
            ),
            vec![
                Kind::Bool,
                Kind::Text,
                Kind::Text,
                Kind::Text,
                Kind::Text,
                Kind::Text,
                Kind::Number,
                Kind::Number,
                Kind::Date,
                Kind::DateTime,
            ]
        );
    }

    /// EDGE-18: an empty value is present, and present is not the same as valued.
    #[test]
    fn an_empty_value_is_a_prop() {
        assert_eq!(
            props("---\nstatus:\n---\n"),
            vec![("status".into(), String::new(), Kind::Text, None)]
        );
    }

    /// The block's line range, which is what stops the checker calling YAML prose.
    #[test]
    fn the_block_reports_the_span_it_covers() {
        let b = block("---\ncr: 1\n---\nbody\n").unwrap();
        assert_eq!(b.lines, (0, 2));
        // The byte span is what the checker needs, and it ends at the closing fence,
        // not somewhere inside the prose below it.
        assert_eq!(b.bytes, (0, 13));
        let b = block("\u{feff}---\ncr: 1\n---\nbody\n").unwrap();
        assert_eq!(
            b.bytes,
            (3, 16),
            "a BOM shifts the span rather than hiding the block"
        );
    }

    /// EDGE-17's ruled fix: a declared-numeric key normalizes a bare fraction.
    #[test]
    fn a_declared_number_key_normalizes_a_fraction() {
        let mut prop = one("---\ncr: 1/4\n---\n");
        assert_eq!(prop.kind, Kind::Text, "as authored, it is text");
        declare(&mut prop, Kind::Number);
        assert_eq!((prop.value.as_str(), prop.kind), ("0.25", Kind::Number));
        assert_eq!(normalize_number("2/1").as_deref(), Some("2"));
        assert_eq!(normalize_number("0x1f"), None, "not a fraction either");
        assert_eq!(normalize_number("1/0"), None);
    }

    /// Every kind survives the trip through the column and back. Without this, a
    /// `nested` row came out of the DB as `text` — the spelling was written by one
    /// function and read by another that had never heard of it.
    #[test]
    fn every_kind_survives_its_own_spelling() {
        for kind in [
            Kind::Text,
            Kind::Number,
            Kind::Date,
            Kind::DateTime,
            Kind::Bool,
            Kind::Link,
            Kind::Nested,
        ] {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind), "{}", kind.as_str());
        }
        assert_eq!(Kind::parse("nonsense"), None);
    }

    /// Out-of-scope constructs are kept as text rather than dropped or guessed.
    #[test]
    fn anchors_and_tags_survive_as_text() {
        assert_eq!(
            props("---\nbase: &anchor value\ncopy: *anchor\n---\n"),
            vec![
                ("base".into(), "&anchor value".into(), Kind::Text, None),
                ("copy".into(), "*anchor".into(), Kind::Text, None),
            ]
        );
    }
}
