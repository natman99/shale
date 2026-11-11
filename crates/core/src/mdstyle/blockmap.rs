//! The block map: which parts of a note a `^id` can name.
//!
//! Split out from [`super::block`], which draws the spans, because it answers a
//! different question: not "what does this line look like" but "what are the units a
//! reader can address". The index files ids from it, the transclusion resolver cuts a
//! block out with it, the jump scrolls to it, and the `Style::BlockAnchor` span the
//! painter uses comes from this same scan — so there is exactly one answer to "what
//! blocks does this note have", and a colour can never claim an anchor the index
//! would not find.

use super::block::{
    callout_header, closes_fence, is_thematic_break, list_marker_end, opens_fence,
    quote_content_start,
};

/// What kind of block a block reference names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// Consecutive ordinary text lines.
    Paragraph,
    /// One list item: its marker, its own lines, and every nested child under it.
    ListItem,
    /// A blockquote: consecutive `>` lines that open no callout.
    Quote,
    /// A callout box (`> [!note] …`): the same consecutive `>` lines, with a header.
    Callout,
    /// A whole GFM table — header row, delimiter row and body rows together.
    Table,
    /// A fenced code block, both fence lines included.
    Code,
    /// One heading line, and not its section: `#Heading` already means the section,
    /// so `#^id` on a heading means the line and nothing else.
    Heading,
}

/// One addressable block of a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// First line of the block, 0-based, counting every line of the note.
    pub first_line: usize,
    /// Last line of the block, inclusive. Blank lines that trail a block are not
    /// part of it, which is what lets "does this anchor sit on the block's last
    /// line?" be a question this scan can answer.
    pub last_line: usize,
    /// Byte range of the block's lines, markers included.
    pub start: usize,
    pub end: usize,
    /// Column of the block's first non-blank character. A list item's nesting depth
    /// decides which item a nested child belongs to; zero for everything else.
    pub indent: usize,
}

/// A `^id` this scan recognized as naming a block.
///
/// A `^`-token this scan does *not* list stays ordinary text: `2^10`, a `[^note]`
/// footnote, an id typed in the middle of a paragraph, an id at the end of a table
/// row, and anything inside a fence or a properties block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAnchor {
    /// The id, without its leading caret.
    pub id: String,
    /// Byte range of the whole token, caret included.
    pub start: usize,
    pub end: usize,
    /// The line the token sits on, 0-based.
    pub line: usize,
    /// Index into [`BlockMap::blocks`] of the block the token names.
    pub block: usize,
}

/// Every block of a note and every anchor that names one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockMap {
    pub blocks: Vec<Block>,
    pub anchors: Vec<BlockAnchor>,
}

/// The block map of a whole note.
pub fn block_map(text: &str) -> BlockMap {
    let lines = note_lines(text);
    let tables = super::table_row_flags(&lines);
    let front = super::frontmatter(text, &lines);
    block_map_lines(&lines, &tables, front.map(|f| (f.whole.0, f.whole.1)))
}

/// The lines of a note as `(byte offset, content without the newline)` — the same
/// split [`super::style_spans`] walks, so the two agree about what a line is.
fn note_lines(text: &str) -> Vec<(usize, &str)> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let content_len = line.trim_end_matches(['\n', '\r']).len();
        lines.push((offset, &line[..content_len]));
        offset += line.len();
    }
    lines
}

/// The block map, given the line split and the table flags a caller already has.
///
/// `front` is a note's properties block as a byte range, when it has one: those
/// lines are data rather than prose, so they hold no blocks and no anchors — a
/// property whose value happens to end in `^abc` files no block.
pub(super) fn block_map_lines(
    lines: &[(usize, &str)],
    tables: &[bool],
    front: Option<(usize, usize)>,
) -> BlockMap {
    let mut map = BlockMap::default();
    let in_front =
        |offset: usize| front.is_some_and(|(start, end)| offset >= start && offset < end);

    let mut index = 0usize;
    while index < lines.len() {
        let (offset, content) = lines[index];
        // A line that is only an id belongs to the block above it, so it never
        // opens a block of its own.
        if content.trim().is_empty() || in_front(offset) || anchor_only(content) {
            index += 1;
            continue;
        }
        let indent = indent_of(content);
        let trimmed = content.trim_start();

        // A fenced code block: one block, both fences included. Nothing inside it
        // is an anchor, and the fence rules are the parser's own — the same ones
        // that hide a `#` from the heading scan.
        if let Some((fc, flen)) = opens_fence(trimmed.as_bytes()) {
            let last = fence_end(lines, index, fc, flen, &in_front);
            push_block(&mut map, BlockKind::Code, lines, index, last, indent);
            index = last + 1;
            continue;
        }

        // A heading is one line.
        if heading_markers(content) > 0 {
            push_block(&mut map, BlockKind::Heading, lines, index, index, indent);
            index += 1;
            continue;
        }

        // A thematic break divides blocks; it is not one.
        if is_thematic_break(content) {
            index += 1;
            continue;
        }

        // A table is its rows together. Rows are not addressable on their own.
        if tables.get(index).copied().unwrap_or(false) {
            let mut last = index;
            while last + 1 < lines.len() && tables.get(last + 1).copied().unwrap_or(false) {
                last += 1;
            }
            push_block(&mut map, BlockKind::Table, lines, index, last, indent);
            index = last + 1;
            continue;
        }

        // A blockquote or callout: consecutive `>` lines, one box.
        if trimmed.starts_with('>') {
            let mut last = index;
            let mut scan = index + 1;
            while scan < lines.len() && !in_front(lines[scan].0) {
                let text = lines[scan].1.trim_start();
                if text.is_empty() || !text.starts_with('>') {
                    break;
                }
                last = scan;
                scan += 1;
            }
            let header = &content[quote_content_start(content)..];
            let kind = if callout_header(header).is_some() {
                BlockKind::Callout
            } else {
                BlockKind::Quote
            };
            push_block(&mut map, kind, lines, index, last, indent);
            index = last + 1;
            continue;
        }

        // A list item, its continuation lines and its nested children.
        if list_marker_end(content).is_some() {
            index = scan_item(lines, index, &in_front, &mut map);
            continue;
        }

        // Ordinary text: a paragraph runs to the first blank line, or to the first
        // line that opens a construct of its own.
        let mut last = index;
        let mut scan = index + 1;
        while scan < lines.len() && !in_front(lines[scan].0) {
            let text = lines[scan].1;
            if text.trim().is_empty()
                || anchor_only(text)
                || opens_new_block(text, tables.get(scan).copied().unwrap_or(false))
            {
                break;
            }
            last = scan;
            scan += 1;
        }
        push_block(&mut map, BlockKind::Paragraph, lines, index, last, indent);
        index = last + 1;
    }

    file_anchors(&mut map, lines);
    map
}

/// One list item and everything nested under it; returns the line after the item.
///
/// A marker deeper than this item's marker opens a block of its own, so a parent
/// and its child are both addressable: the parent's block spans the child, and the
/// child is the deepest block ending on its own line, which is what an id written
/// there attaches to.
fn scan_item(
    lines: &[(usize, &str)],
    start: usize,
    in_front: &dyn Fn(usize) -> bool,
    map: &mut BlockMap,
) -> usize {
    let indent = indent_of(lines[start].1);
    // The parent goes in before its children, so `blocks` reads in document order —
    // an item, then the items under it. That is what makes the deepest-block rule in
    // [`innermost_block`] the thing that decides who an id belongs to.
    let parent = map.blocks.len();
    push_block(map, BlockKind::ListItem, lines, start, start, indent);
    let mut last = start;
    let mut scan = start + 1;
    while scan < lines.len() && !in_front(lines[scan].0) {
        let text = lines[scan].1;
        if text.trim().is_empty() {
            // A blank line does not end the item by itself; the line after it does.
            scan += 1;
            continue;
        }
        if indent_of(text) <= indent {
            break;
        }
        if list_marker_end(text).is_some() {
            let child = map.blocks.len();
            scan = scan_item(lines, scan, in_front, map);
            last = last.max(map.blocks[child].last_line);
            continue;
        }
        last = scan;
        scan += 1;
    }
    let block = &mut map.blocks[parent];
    block.last_line = last;
    block.end = lines[last].0 + lines[last].1.len();
    scan
}

/// The line a fence opened at `start` closes on, or the note's last line when it
/// never closes.
fn fence_end(
    lines: &[(usize, &str)],
    start: usize,
    fc: u8,
    flen: usize,
    in_front: &dyn Fn(usize) -> bool,
) -> usize {
    let mut last = start;
    let mut scan = start + 1;
    while scan < lines.len() && !in_front(lines[scan].0) {
        let text = lines[scan].1.trim_start();
        last = scan;
        if !text.is_empty() && closes_fence(text.as_bytes(), fc, flen) {
            break;
        }
        scan += 1;
    }
    last
}

/// Whether `content` opens a block of its own, ending a paragraph above it.
fn opens_new_block(content: &str, is_table_row: bool) -> bool {
    let trimmed = content.trim_start();
    heading_markers(content) > 0
        || is_table_row
        || trimmed.starts_with('>')
        || list_marker_end(content).is_some()
        || opens_fence(trimmed.as_bytes()).is_some()
        || is_thematic_break(content)
}

/// The number of leading `#` markers on a heading line, or 0 for any other line.
fn heading_markers(content: &str) -> usize {
    let hashes = content.chars().take_while(|&c| c == '#').count();
    if hashes > 0
        && hashes <= 6
        && content[hashes..]
            .chars()
            .next()
            .is_none_or(|c| c == ' ' || c == '\t')
    {
        hashes
    } else {
        0
    }
}

/// The column of a line's first non-blank character. A tab counts as one column,
/// the way the rest of this scanner counts.
fn indent_of(content: &str) -> usize {
    content
        .char_indices()
        .find(|(_, c)| *c != ' ' && *c != '\t')
        .map(|(i, _)| content[..i].chars().count())
        .unwrap_or_else(|| content.chars().count())
}

/// The `^id` token at the end of a line, as `(byte offset of the caret, id)`.
///
/// The token is `[A-Za-z0-9-]+` — Latin letters, digits and dashes, Obsidian's
/// alphabet — and the caret must be preceded by whitespace or stand at the line's
/// first character. That one rule is what keeps `2^10`, `[^note]` and `^foo_bar`
/// ordinary text; it is the whole difference between a caret in prose and an anchor.
fn anchor_token(content: &str) -> Option<(usize, String)> {
    let trimmed = content.trim_end();
    let bytes = trimmed.as_bytes();
    let mut i = bytes.len();
    while i > 0 && is_id_byte(bytes[i - 1]) {
        i -= 1;
    }
    if i == bytes.len() || i == 0 || bytes[i - 1] != b'^' {
        return None;
    }
    let caret = i - 1;
    if caret > 0 {
        let before = trimmed.as_bytes()[caret - 1];
        if before != b' ' && before != b'\t' {
            return None;
        }
    }
    Some((caret, trimmed[caret + 1..].to_string()))
}

/// The same text with a trailing `^id` token cut off, when it has one.
///
/// A heading that carries a block id is still the section its name names: `## The Fall
/// ^fall` must answer `[[note#The Fall]]` as plainly as it answers `[[note#^fall]]`, or
/// adding an id to a heading would silently break every heading link to it (EDGE-27).
/// A heading whose *name* starts with the caret — `## ^foo` — keeps it: the caret
/// position is claimed by the block form, so no anchor spelling reaches that name, which
/// is EDGE-23's documented consequence rather than a bug to paper over.
pub fn without_block_anchor(text: &str) -> &str {
    match anchor_token(text) {
        Some((caret, _)) if caret > 0 => text[..caret].trim_end(),
        _ => text,
    }
}

/// A line whose whole content is an `^id` token: Obsidian's form for a structured
/// block's id. It belongs to the block above it, so the walk never opens a block
/// with it and the anchor pass below is the only thing that reads it.
fn anchor_only(content: &str) -> bool {
    anchor_token(content).is_some_and(|(caret, _)| caret == indent_of(content))
}

/// Append one block, spanning `first..=last`.
fn push_block(
    map: &mut BlockMap,
    kind: BlockKind,
    lines: &[(usize, &str)],
    first: usize,
    last: usize,
    indent: usize,
) {
    map.blocks.push(Block {
        kind,
        first_line: first,
        last_line: last,
        start: lines[first].0,
        end: lines[last].0 + lines[last].1.len(),
        indent,
    });
}

/// File every `^id` in the note, now that the blocks are known.
///
/// Two spellings, one attachment rule. An id at the end of a text line names the
/// block that ends on that line. An id alone on its own line, with a blank line
/// above it, names the block that ended above the blank — Obsidian's form for
/// lists, quotes and tables. Either way the *deepest* block ending there wins, so
/// an id written under a nested list item belongs to the item, not to its parent.
///
/// What is declined stays text: an id in the middle of a paragraph (no block ends
/// there), an id at the end of a table row or a quoted line (those blocks take the
/// own-line form only, which is also what keeps a cell's text from reading as an
/// id), and anything inside a fence or a properties block.
///
/// Two list forms are the exception, because Obsidian documents them and the blank
/// rule would decline them — see [`list_owner`].
fn file_anchors(map: &mut BlockMap, lines: &[(usize, &str)]) {
    for line in 0..lines.len() {
        let (offset, content) = lines[line];
        let Some((caret, id)) = anchor_token(content) else {
            continue;
        };
        let alone = anchor_only(content);
        // The block this token names ends on this line, or — for the own-line form —
        // on the last non-blank line above the blank line above the token. With no
        // blank above it, the own-line form stands inside something, and only a list
        // item's own lines qualify (see [`list_owner`]).
        let block = if alone {
            last_text_line(lines, line).map_or_else(
                || list_owner(map, line, true),
                |owner| innermost_block(map, owner),
            )
        } else {
            innermost_block(map, line).or_else(|| list_owner(map, line, false))
        };
        let Some(block) = block else {
            continue;
        };
        if !alone
            && !matches!(
                map.blocks[block].kind,
                BlockKind::Paragraph | BlockKind::ListItem | BlockKind::Heading
            )
        {
            continue;
        }
        map.anchors.push(BlockAnchor {
            start: offset + caret,
            end: offset + caret + 1 + id.len(),
            line,
            block,
            id,
        });
    }
}

/// The deepest block ending on `line`: greatest indent, then latest start.
fn innermost_block(map: &BlockMap, line: usize) -> Option<usize> {
    let mut best: Option<(usize, usize, usize)> = None; // indent, first_line, index
    for (index, block) in map.blocks.iter().enumerate() {
        if block.last_line != line {
            continue;
        }
        let key = (block.indent, block.first_line, index);
        if best.is_none_or(|seen| key > seen) {
            best = Some(key);
        }
    }
    best.map(|(_, _, index)| index)
}

/// The last non-blank line above `line`, but only when at least one blank line
/// separates them: the own-line form names the block it stands *under*, and the blank
/// is what says the block is finished. A token with running text above it is in the
/// middle of that block, which is the case EDGE-5 declines.
fn last_text_line(lines: &[(usize, &str)], line: usize) -> Option<usize> {
    let mut above = line;
    let mut blanks = 0usize;
    loop {
        if above == 0 {
            return None;
        }
        above -= 1;
        if lines[above].1.trim().is_empty() {
            blanks += 1;
        } else if blanks > 0 {
            return Some(above);
        } else {
            return None;
        }
    }
}

/// The two list forms the blank rule would decline, both from Obsidian's own manual
/// ("specific lines within a list") and from this plan's EDGE-7.
///
/// `alone`: an id standing on its own line among a list item's continuation lines —
/// the manual's `- Gemmy` / `Paperclip / Pen` / `^37006f`. The innermost item whose
/// extent covers the line owns it, so an id written inside a nested item belongs to the
/// nested item.
///
/// Not `alone`: an id at the end of a **parent's** bullet line, with its children below
/// it. No block *ends* on that line, so [`innermost_block`] finds nothing; the item
/// whose marker sits on the line does, and its block already spans its children — which
/// is what an id on a parent is for (ruling 1).
fn list_owner(map: &BlockMap, line: usize, alone: bool) -> Option<usize> {
    let mut best: Option<((usize, usize, usize), usize)> = None;
    for (index, block) in map.blocks.iter().enumerate() {
        if block.kind != BlockKind::ListItem {
            continue;
        }
        let holds = if alone {
            block.first_line < line && line <= block.last_line
        } else {
            block.first_line == line
        };
        if !holds {
            continue;
        }
        let key = (block.indent, block.first_line, index);
        if best.is_none_or(|(seen, _)| key > seen) {
            best = Some((key, index));
        }
    }
    best.map(|(_, index)| index)
}

// ---------------------------------------------------------------------------
// Writing an id: the create verb's half of the scan (P4.9 M4).
// ---------------------------------------------------------------------------

/// One byte of an id. [`anchor_token`] walks a line with this set and
/// [`valid_block_id`] asks it of a whole id, so the grammar the scan accepts and the
/// grammar the create command accepts are one statement, not two (ruling 2).
fn is_id_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

/// Is `id` a legal block id: `[A-Za-z0-9-]+`, case-sensitive.
pub fn valid_block_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(is_id_byte)
}

/// Which block the byte at `offset` sits in — the innermost one, so a caret inside a
/// nested list item names the item the reader is looking at and not its parent. The
/// tie-break is [`list_owner`]'s: deeper indent, then later line.
pub fn block_at(map: &BlockMap, offset: usize) -> Option<usize> {
    let mut best: Option<((usize, usize, usize), usize)> = None;
    for (index, block) in map.blocks.iter().enumerate() {
        if offset < block.start || offset > block.end {
            continue;
        }
        let key = (block.indent, block.first_line, index);
        if best.is_none_or(|(seen, _)| key > seen) {
            best = Some((key, index));
        }
    }
    best.map(|(_, index)| index)
}

/// The id a block already carries, if it carries one (EDGE-10).
pub fn block_id(map: &BlockMap, block: usize) -> Option<&str> {
    map.anchors
        .iter()
        .find(|anchor| anchor.block == block)
        .map(|anchor| anchor.id.as_str())
}

/// Where an id goes, and what to write there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIdWrite {
    /// The byte to write at.
    pub at: usize,
    /// The text to write: whitespace, sometimes a blank line, then `^id`.
    pub insert: String,
    /// Just past the token — where the editor leaves the caret, so the reader can keep
    /// typing on the id they were given (EDGE-21).
    pub caret: usize,
}

/// The edit that gives `block` the id `id`, or `None` when no spelling would name it.
///
/// Candidate spellings are tried in order and each is **re-read by this same scan**: the
/// anchor has to land inside the text just written *and* name the block it was written
/// for. The placement rules below are a guess about where the scan will accept an id;
/// the scan is the authority. A command that wrote an id the scan reads as naming a
/// different block would hand the author a link that lies, so the guess is checked
/// rather than trusted.
pub fn with_block_id(text: &str, id: &str, block: usize) -> Option<BlockIdWrite> {
    let map = block_map(text);
    let target = map.blocks.get(block)?;
    let (kind, first_line, start) = (target.kind, target.first_line, target.start);
    for (at, insert) in id_candidates(&map, target, id) {
        let mut out = String::with_capacity(text.len() + insert.len());
        out.push_str(&text[..at]);
        out.push_str(&insert);
        out.push_str(&text[at..]);
        let after = block_map(&out);
        let Some(written) = after
            .blocks
            .iter()
            .position(|b| b.kind == kind && b.first_line == first_line && b.start == start)
        else {
            continue;
        };
        if let Some(anchor) = after.anchors.iter().find(|a| {
            a.id == id && a.block == written && a.start >= at && a.end <= at + insert.len()
        }) {
            return Some(BlockIdWrite {
                at,
                insert,
                caret: anchor.end,
            });
        }
    }
    None
}

/// Where an id for `block` could be written, best guess first.
fn id_candidates(map: &BlockMap, block: &Block, id: &str) -> Vec<(usize, String)> {
    match block.kind {
        // The id sits at the end of the block's last line, after whitespace.
        BlockKind::Paragraph | BlockKind::Heading => vec![(block.end, format!(" ^{id}"))],
        // A structured block has no last line an id could sit on — an id after a table
        // row is ordinary text (EDGE-8) — so it takes the own-line form: a blank line,
        // then the id alone (ruling 5's atom, ruled 4 for headings).
        BlockKind::Quote | BlockKind::Callout | BlockKind::Table | BlockKind::Code => {
            vec![(block.end, format!("\n\n^{id}"))]
        }
        BlockKind::ListItem => {
            let child = map.blocks.iter().find(|b| {
                b.kind == BlockKind::ListItem
                    && b.first_line > block.first_line
                    && b.first_line <= block.last_line
            });
            let Some(child) = child else {
                // No children: the item's own last line is the last line of the block,
                // which is the spelling the manual uses for a plain bullet.
                return vec![(block.end, format!(" ^{id}"))];
            };
            // Children end the block, so an id appended there would name the child
            // (deepest block wins). The item's own line goes between the item and its
            // child: indented past the item, so the walk keeps it inside the item, and
            // shallower than the child, so the item is the deepest thing holding it
            // (EDGE-7).
            let mut out = Vec::new();
            for indent in [
                block.indent + 2,
                child.indent.saturating_sub(1),
                block.indent + 1,
            ] {
                if indent <= block.indent || indent >= child.indent {
                    continue;
                }
                let line = " ".repeat(indent);
                out.push((block.end, format!("\n{line}^{id}")));
            }
            out
        }
    }
}

/// A fresh block id: six characters of `a-z2-7` (ruling 2), retried while the caller
/// says the candidate is taken.
///
/// No RNG crate: the seed is the clock mixed with a process counter, which is plenty
/// for ids that only have to be unique *within one vault* — and uniqueness is not
/// trusted to the seed anyway, because every candidate goes through `taken`.
pub fn new_block_id(taken: impl Fn(&str) -> bool) -> Option<String> {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut state = seed();
    for _ in 0..64 {
        let mut id = String::with_capacity(6);
        for _ in 0..6 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            id.push(ALPHABET[((state >> 33) & 31) as usize] as char);
        }
        if !taken(&id) {
            return Some(id);
        }
    }
    None
}

/// A non-zero xorshift seed: the clock, salted by a counter so two calls in the same
/// nanosecond cannot produce the same id.
fn seed() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let salt = COUNTER.fetch_add(1, Ordering::Relaxed);
    (nanos ^ (salt << 33) ^ 0x9e37_79b9_7f4a_7c15)
        .wrapping_mul(0xff51_afd7_ed55_8ccd)
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::super::Style;
    use super::*;

    /// The blocks of `text`, as `(kind, first_line, last_line)`.
    fn kinds(text: &str) -> Vec<(BlockKind, usize, usize)> {
        block_map(text)
            .blocks
            .iter()
            .map(|b| (b.kind, b.first_line, b.last_line))
            .collect()
    }

    /// The anchors of `text`, as `(id, line, kind, first and last line of the block
    /// it names)`. The block's range is in here on purpose: two list items in one
    /// note are the same *kind*, so a test that only checked the kind could not tell
    /// a parent from its child.
    fn anchors(text: &str) -> Vec<(String, usize, BlockKind, usize, usize)> {
        let map = block_map(text);
        map.anchors
            .iter()
            .map(|a| {
                let b = &map.blocks[a.block];
                (a.id.clone(), a.line, b.kind, b.first_line, b.last_line)
            })
            .collect()
    }

    #[test]
    fn an_anchor_at_the_end_of_a_paragraph_names_the_paragraph() {
        let text = "The gem dashes through the paragraph.\nPen in hand, it keeps going. ^37066d\n";
        assert_eq!(kinds(text), vec![(BlockKind::Paragraph, 0, 1)]);
        assert_eq!(
            anchors(text),
            vec![("37066d".into(), 1, BlockKind::Paragraph, 0, 1)]
        );
    }

    #[test]
    fn a_caret_in_prose_is_never_an_anchor() {
        // Each of these is ordinary text: the caret is not preceded by whitespace,
        // or the token is not Obsidian's alphabet, or the token is not at the end.
        for text in [
            "The exponent is 2^10 in the recipe.\n",
            "See the footnote[^note].\n",
            "A slug like ^foo_bar is not an id.\n",
            "The id ^abc123 sits in the middle of the line.\n",
            "Greek, not Latin: ^\u{e9}\n",
            "A caret glued to a word: glued^abc123\n",
        ] {
            assert_eq!(anchors(text), Vec::new(), "not an anchor: {text:?}");
        }
    }

    #[test]
    fn an_id_in_the_middle_of_a_paragraph_is_ordinary_text() {
        // A block's atom is the whole paragraph, so an id that does not sit at its
        // end names nothing — the note keeps the bytes and files nothing.
        let text = "First line ^abc123\nsecond line of the same paragraph\n";
        assert_eq!(kinds(text), vec![(BlockKind::Paragraph, 0, 1)]);
        assert_eq!(anchors(text), Vec::new());
    }

    #[test]
    fn a_heading_anchor_names_the_line_and_not_the_section() {
        let text = "## The Fall of Zhent ^zhent\n\nThe city went under.\n";
        assert_eq!(
            kinds(text),
            vec![(BlockKind::Heading, 0, 0), (BlockKind::Paragraph, 2, 2),]
        );
        assert_eq!(
            anchors(text),
            vec![("zhent".into(), 0, BlockKind::Heading, 0, 0)]
        );
    }

    #[test]
    fn each_list_item_is_its_own_block() {
        let text = "- Gemmy ^gemmy\n- Unhelpful assistant\n";
        assert_eq!(
            kinds(text),
            vec![(BlockKind::ListItem, 0, 0), (BlockKind::ListItem, 1, 1),]
        );
        assert_eq!(
            anchors(text),
            vec![("gemmy".into(), 0, BlockKind::ListItem, 0, 0)]
        );
    }

    #[test]
    fn a_nested_item_is_a_block_of_its_own_and_takes_its_own_anchor() {
        // The parent spans the child (ruling 1), and the deepest block ending on the
        // line wins, so an id written on the child's line belongs to the child.
        let text = "- Gemmy\n    - Unhelpful assistant ^kid\n";
        assert_eq!(
            kinds(text),
            vec![(BlockKind::ListItem, 0, 1), (BlockKind::ListItem, 1, 1),]
        );
        assert_eq!(
            anchors(text),
            vec![("kid".into(), 1, BlockKind::ListItem, 1, 1)]
        );
    }

    #[test]
    fn a_list_item_continues_across_a_blank_line_when_the_next_line_is_indented() {
        let text = "- Gemmy\n\n    still the same item\n- Unhelpful assistant\n";
        assert_eq!(
            kinds(text),
            vec![(BlockKind::ListItem, 0, 2), (BlockKind::ListItem, 3, 3),]
        );
    }

    #[test]
    fn a_quote_or_a_table_takes_only_the_own_line_form() {
        // Obsidian's rule: an id for a structured block stands on its own line, with
        // a blank line above it. An id at the end of a quoted line or a table row is
        // text — which is also what keeps a cell's contents from reading as an id.
        let glued = "> quoted ^q1\n";
        assert_eq!(anchors(glued), Vec::new());
        assert_eq!(kinds(glued), vec![(BlockKind::Quote, 0, 0)]);

        let apart = "> quoted\n\n^q1\n";
        assert_eq!(
            anchors(apart),
            vec![("q1".into(), 2, BlockKind::Quote, 0, 0)]
        );

        let row = "| a | ^x |\n| - | - |\n";
        assert_eq!(anchors(row), Vec::new());

        let table = "| a | b |\n| - | - |\n| 1 | 2 |\n\n^tbl\n";
        assert_eq!(
            anchors(table),
            vec![("tbl".into(), 4, BlockKind::Table, 0, 2)]
        );
        assert_eq!(kinds(table), vec![(BlockKind::Table, 0, 2)]);
    }

    #[test]
    fn the_own_line_form_needs_a_blank_above_a_paragraph() {
        // Without the blank the token stands in the middle of running text, and Shale
        // declines to guess which block it meant. The list exception is its own test: a
        // list item's continuation lines are already a thing the scan tracks.
        let text = "a paragraph\n^q1\n";
        assert_eq!(kinds(text), vec![(BlockKind::Paragraph, 0, 0)]);
        assert_eq!(anchors(text), Vec::new());
    }

    /// EDGE-7: an id on a parent's bullet line names the item **and its children**. No
    /// block ends on that line, so the deepest-ending rule finds nothing; the item whose
    /// marker sits there does, and its block already spans the child.
    #[test]
    fn an_id_on_a_parent_bullet_names_the_item_and_its_children() {
        let text = "- Gemmy ^kid\n    - Unhelpful assistant\n- Someone else\n";
        assert_eq!(
            anchors(text),
            vec![("kid".into(), 0, BlockKind::ListItem, 0, 1)],
            "the parent's block spans its child, and the id names the whole thing"
        );
    }

    /// Obsidian's own list example: the id stands on its own line among the item's
    /// continuation lines, with no blank above it. The innermost item covering the line
    /// owns it, so an id written inside a nested item belongs to the nested item.
    #[test]
    fn an_id_on_its_own_line_inside_a_list_item_names_that_item() {
        let text = "- Gemmy\n    Paperclip / Pen\n    ^37006f\n- Unhelpful assistant\n";
        assert_eq!(
            anchors(text),
            vec![("37006f".into(), 2, BlockKind::ListItem, 0, 2)]
        );
        let nested = "- Gemmy\n    - Unhelpful assistant\n        ^37006f\n";
        assert_eq!(
            anchors(nested),
            vec![("37006f".into(), 2, BlockKind::ListItem, 1, 2)],
            "the innermost item, not the outer one"
        );
    }

    #[test]
    fn a_callout_is_one_block() {
        let text = "> [!cue] The Apotheosis\n> Strings, low and slow.\n";
        assert_eq!(kinds(text), vec![(BlockKind::Callout, 0, 1)]);
    }

    #[test]
    fn a_fence_is_one_block_and_holds_no_anchor() {
        // The id sits at the end of a code line, where it *would* be a token: what
        // declines it is the block's kind, not the walk missing it.
        let text = "```md\nnot a list\nand ^abc123\n```\n";
        assert_eq!(kinds(text), vec![(BlockKind::Code, 0, 3)]);
        assert_eq!(anchors(text), Vec::new());
    }

    #[test]
    fn a_properties_block_holds_no_blocks_and_no_anchors() {
        // The block is data. A value that happens to end in `^abc` files no block,
        // and the fences are not a thematic break.
        let text = "---\naliases: goblin ^abc123\n---\n\nThe goblin waits.\n";
        let map = block_map(text);
        assert_eq!(map.blocks.len(), 1, "only the paragraph: {:?}", map.blocks);
        assert_eq!(map.blocks[0].kind, BlockKind::Paragraph);
        assert_eq!(map.blocks[0].first_line, 4);
        assert_eq!(map.anchors, Vec::new());
    }

    #[test]
    fn blank_lines_trailing_a_block_are_not_part_of_it() {
        let text = "one\n\n\n\ntwo\n";
        assert_eq!(
            kinds(text),
            vec![(BlockKind::Paragraph, 0, 0), (BlockKind::Paragraph, 4, 4),]
        );
    }

    #[test]
    fn the_anchor_span_covers_the_caret_and_the_id() {
        let text = "hello ^abc123\n";
        let map = block_map(text);
        let a = &map.anchors[0];
        assert_eq!(&text[a.start..a.end], "^abc123");
    }

    #[test]
    fn the_anchor_is_painted_and_never_hidden() {
        // Ruling 6: the id is the note's claim on itself. It wears `dim` and stays in
        // the file the reader sees, so it must not join the marker set that live
        // preview cuts out of a row.
        let text = "hello ^abc123\n";
        let spans = super::super::style_spans(text);
        let anchor = spans
            .iter()
            .find(|s| s.style == Style::BlockAnchor)
            .expect("the anchor gets a span");
        assert_eq!(&text[anchor.start..anchor.end], "^abc123");
        assert!(
            !crate::mdlayout::is_marker(Style::BlockAnchor),
            "an anchor is not a marker: hiding it hides a claim"
        );
        let hidden = crate::mdlayout::hidden_ranges(text, &spans, 0, text.len() - 1);
        assert_eq!(hidden, Vec::new(), "live preview hides nothing here");
    }

    // ---------------------------------------------------------------------
    // Writing an id (P4.9 M4): where the create verb puts it, and what the
    // scan agrees to.
    // ---------------------------------------------------------------------

    /// The note with the id written, and the anchor the re-scan found for it.
    fn written(text: &str, id: &str, block: usize) -> (String, BlockAnchor) {
        let write = with_block_id(text, id, block).expect("an id can be written here");
        let out = format!("{}{}{}", &text[..write.at], write.insert, &text[write.at..]);
        let map = block_map(&out);
        let anchor = map
            .anchors
            .iter()
            .find(|a| a.id == id)
            .expect("the written id is an anchor");
        (out, anchor.clone())
    }

    /// A paragraph takes its id at the end of its last line, and the block it names
    /// is the whole soft-wrapped paragraph (EDGE-6).
    #[test]
    fn an_id_written_for_a_paragraph_sits_at_the_end_of_its_last_line() {
        let text = "the goblin\nwaits by the door\n\nnext paragraph\n";
        let (out, anchor) = written(text, "goblin", 0);
        assert_eq!(
            out, "the goblin\nwaits by the door ^goblin\n\nnext paragraph\n",
            "appended to the last line, after a space"
        );
        let map = block_map(&out);
        assert_eq!(anchor.block, 0);
        assert_eq!(
            (map.blocks[0].first_line, map.blocks[0].last_line),
            (0, 1),
            "and it still names the whole paragraph"
        );
    }

    /// Ruling 4: an id on a heading names the heading line, and the create verb
    /// writes it there — not at the end of the section.
    #[test]
    fn an_id_written_for_a_heading_names_the_heading_line() {
        let text = "## The Fall\nthe city went under\n";
        let (out, anchor) = written(text, "fall", 0);
        assert_eq!(out, "## The Fall ^fall\nthe city went under\n");
        let map = block_map(&out);
        assert_eq!(map.blocks[anchor.block].kind, BlockKind::Heading);
        assert_eq!(map.blocks[anchor.block].first_line, 0);
    }

    /// EDGE-8: an id after a table row is ordinary text, so a table's id stands on
    /// its own line below a blank — the one form the scan accepts for a structured
    /// block.
    #[test]
    fn an_id_written_for_a_table_stands_on_its_own_line_below_a_blank() {
        let text = "| a | b |\n| - | - |\n| 1 | 2 |\n";
        let (out, anchor) = written(text, "tid", 0);
        assert_eq!(out, "| a | b |\n| - | - |\n| 1 | 2 |\n\n^tid\n");
        let map = block_map(&out);
        assert_eq!(map.blocks[anchor.block].kind, BlockKind::Table);
        assert_eq!(
            map.blocks[anchor.block].last_line, 2,
            "the table, not the id line"
        );
    }

    /// A quote and a fenced block take the same own-line form.
    #[test]
    fn a_quote_and_a_code_block_take_the_own_line_form() {
        let (out, anchor) = written("> quote\n> more\n", "qid", 0);
        assert_eq!(out, "> quote\n> more\n\n^qid\n");
        let map = block_map(&out);
        assert_eq!(map.blocks[anchor.block].kind, BlockKind::Quote);

        let (out, anchor) = written("```rust\nlet x = 1;\n```\n", "cid", 0);
        assert_eq!(out, "```rust\nlet x = 1;\n```\n\n^cid\n");
        let map = block_map(&out);
        assert_eq!(map.blocks[anchor.block].kind, BlockKind::Code);
    }

    /// EDGE-7, the hard one: a parent item's id must name the item and not the child
    /// that ends its block. The id goes on a line of its own between the two indents.
    #[test]
    fn an_id_written_for_a_parent_item_names_the_item_and_not_the_child() {
        let text = "- parent\n    - child\n";
        let (out, anchor) = written(text, "pid", 0);
        let map = block_map(&out);
        assert_eq!(
            map.blocks[anchor.block].kind,
            BlockKind::ListItem,
            "a list item either way"
        );
        assert_eq!(
            map.blocks[anchor.block].first_line, 0,
            "the parent's block, not the child's:\n{out}"
        );
        assert_eq!(
            map.blocks[anchor.block].last_line, 2,
            "the item's block now holds the id line"
        );
    }

    /// The child, addressed by its own block index, keeps being the child.
    #[test]
    fn an_id_written_for_a_child_item_names_the_child() {
        let text = "- parent\n    - child\n";
        let map = block_map(text);
        let child = map
            .blocks
            .iter()
            .position(|b| b.first_line == 1)
            .expect("the child is a block");
        let (out, anchor) = written(text, "cid", child);
        let after = block_map(&out);
        assert_eq!(
            after.blocks[anchor.block].first_line, 1,
            "the child's block:\n{out}"
        );
    }

    /// Where there is no room for an own-line id between an item and its child, the
    /// command refuses rather than writing one the scan would read as naming the
    /// child. A refusal is a smaller lie than a link to the wrong block.
    #[test]
    fn an_item_with_no_room_for_its_own_id_is_refused() {
        // The child is nested one space deep: no indent sits between 0 and 1.
        let text = "- parent\n - child\n";
        assert_eq!(with_block_id(text, "pid", 0), None);
    }

    /// The caret decides the block, and the innermost one wins: a caret in a nested
    /// item names the item, not the parent that contains it.
    #[test]
    fn the_innermost_block_under_the_caret_wins() {
        let text = "- parent\n    - child\n";
        let map = block_map(text);
        let parent = block_at(&map, 2).expect("the parent's text");
        let child = block_at(&map, text.find("child").unwrap() + 2).expect("the child's text");
        assert_ne!(parent, child);
        assert_eq!(map.blocks[parent].first_line, 0);
        assert_eq!(map.blocks[child].first_line, 1);
        // A blank line between blocks holds no block at all.
        assert_eq!(block_at(&block_map("one\n\ntwo\n"), 4), None);
    }

    /// EDGE-10: the census of a block's ids, which is what lets the create verb
    /// refuse a second one instead of writing a block with two claims.
    #[test]
    fn a_block_that_already_has_an_id_says_which_one() {
        let map = block_map("the goblin waits ^goblin\n");
        assert_eq!(block_id(&map, 0), Some("goblin"));
        assert_eq!(block_id(&map, 99), None);
        assert_eq!(block_id(&block_map("plain\n"), 0), None);
    }

    /// Ruling 2's grammar, asked of a whole id: `[A-Za-z0-9-]+`, case-sensitive.
    #[test]
    fn the_id_grammar_is_the_scans_grammar() {
        for ok in ["a", "ab12cd", "AB", "a-b-c", "0"] {
            assert!(valid_block_id(ok), "{ok} is legal");
        }
        for no in ["", "^a", "a b", "a_b", "a.b", "a^b", "\u{e9}"] {
            assert!(!valid_block_id(no), "{no:?} is not");
        }
    }

    /// A generated id is six characters of `a-z2-7`, and every candidate goes through
    /// the caller's collision check — uniqueness is not trusted to the seed.
    #[test]
    fn a_generated_id_is_six_characters_of_the_declared_alphabet() {
        let alphabet = "abcdefghijklmnopqrstuvwxyz234567";
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            let id = new_block_id(|_| false).expect("an id");
            assert_eq!(id.len(), 6, "{id}");
            assert!(id.chars().all(|c| alphabet.contains(c)), "{id}");
            assert!(valid_block_id(&id));
            assert!(seen.insert(id.clone()), "{id} repeated");
        }
        // The check is consulted, not decorative: with every id starting with `a`
        // refused, no answer starts with `a`.
        for _ in 0..20 {
            let id = new_block_id(|id| id.starts_with('a')).expect("an id");
            assert!(!id.starts_with('a'), "{id} should have been refused");
        }
        // And a vault that somehow holds every candidate says so instead of looping.
        assert_eq!(new_block_id(|_| true), None);
    }
}
