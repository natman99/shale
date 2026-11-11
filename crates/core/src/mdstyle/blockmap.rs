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
    while i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'-') {
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
fn file_anchors(map: &mut BlockMap, lines: &[(usize, &str)]) {
    for line in 0..lines.len() {
        let (offset, content) = lines[line];
        let Some((caret, id)) = anchor_token(content) else {
            continue;
        };
        let alone = anchor_only(content);
        // The block this token names ends on this line, or — for the own-line form
        // — on the last non-blank line above the blank line above the token.
        let owner_line = if alone {
            let mut above = line;
            let mut blanks = 0usize;
            loop {
                if above == 0 {
                    above = usize::MAX;
                    break;
                }
                above -= 1;
                if lines[above].1.trim().is_empty() {
                    blanks += 1;
                } else {
                    break;
                }
            }
            if blanks == 0 {
                continue;
            }
            above
        } else {
            line
        };
        let Some(block) = innermost_block(map, owner_line) else {
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
    fn the_own_line_form_needs_the_blank_line_above_it() {
        // Without the blank, the token is not attached to anything: the note reads
        // as an id glued to the paragraph, and Shale declines to guess.
        let text = "a paragraph\n^q1\n";
        assert_eq!(kinds(text), vec![(BlockKind::Paragraph, 0, 0)]);
        assert_eq!(anchors(text), Vec::new());
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
}
