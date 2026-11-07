//! The inline half of agent edit review (roadmap P2.5b): a queued proposal painted in
//! the note where accepting it would land it.
//!
//! **The strip stays authoritative; the note is opportunistic.** Every decision — what
//! is queued, whose profile proposed it, whether it has been accepted — lives in
//! `crate::review` and the files under `.shale/pending-edits/`. What lives here is one
//! question: given *this* buffer's text, where would each hunk sit, and what would it
//! take away. A hunk that no longer fits the note has no placement at all, so it is not
//! painted; the strip still lists it, badged stale, and the note says nothing. An
//! overlay that showed a change the accept would refuse would be worse than no overlay.
//!
//! **Removals are not phantom.** The lines a proposal would take away are lines the
//! buffer already holds, so the overlay strikes the note's own text and puts a bar
//! beside it. Only the *additions* are phantom rows: they are not in the buffer, and
//! nothing that moves through the note — `j`/`k`, selection, search, folds, the ruler —
//! sees them. They hang off the display line they belong after, which is what keeps the
//! buffer's display model (and every motion computed from it) exactly as it was.
//!
//! **Placement is the editor's job, not the window's.** The buffer moves with every
//! keystroke, so a placement worked out when the queue changed is pointing at the wrong
//! lines the moment the user types. The window pushes the hunks; the editor re-places
//! them against its own text whenever it paints or is asked a key question.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{Context, SharedString};
use shale_core::diff::{self, Hunk};
use shale_core::transclude::{Body, EmbedLine, EmbedRow, Origin};

use crate::line::phantom::{PhantomKind, PhantomLine};
use crate::line::DisplayLine;
use crate::theme;

use super::{embeds, MarkdownEditor, View};

/// One queued proposal, as the editor needs it in order to paint it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InlineEdit {
    /// The queue's id for the proposal — the file stem, the same name the strip and the
    /// MCP tools use — which is what the collapse state is keyed by.
    pub(crate) id: SharedString,
    /// The hunks, in note order, exactly as the queue file holds them.
    pub(crate) hunks: Vec<Hunk>,
    /// Which hunk the review strip's marker is on, if any, so the note and the strip
    /// point at the same change rather than at two nearby ones.
    pub(crate) current: Option<usize>,
}

/// One hunk placed against this editor's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placed {
    /// Which [`InlineEdit`] this came from, and which hunk of it.
    pub(crate) edit: usize,
    pub(crate) hunk: usize,
    /// The proposal's id, carried so a caller can name the hunk without the list it
    /// came out of.
    pub(crate) id: SharedString,
    /// The buffer rows the proposal would take away. These are real rows: the overlay
    /// strikes them rather than drawing a copy.
    pub(crate) removed: Vec<usize>,
    /// The lines it brings, each group anchored **before** the given buffer row (a
    /// number past the last row means "at the end"). These are the phantom rows.
    pub(crate) added: Vec<(usize, Vec<String>)>,
    /// The hunk the strip's marker is on.
    pub(crate) current: bool,
    /// Whether its additions are collapsed to a count (`za` on one of its lines).
    pub(crate) collapsed: bool,
}

impl Placed {
    /// `−N +M`, the count a collapsed hunk shows.
    pub(crate) fn counts(&self) -> (usize, usize) {
        (
            self.removed.len(),
            self.added.iter().map(|(_, lines)| lines.len()).sum(),
        )
    }
}

impl MarkdownEditor {
    /// Hand the editor the proposals that concern the note it is holding.
    ///
    /// The window filters by note; the editor does not know what file it is showing, and
    /// does not need to. Repainting only when something actually changed keeps a queue
    /// beat from waking every window's note every second.
    pub(crate) fn set_pending_edits(&mut self, edits: Vec<InlineEdit>, cx: &mut Context<Self>) {
        if edits == self.pending_edits {
            return;
        }
        self.pending_edits = edits;
        cx.notify();
    }

    /// Place every hunk against the current text. A hunk that does not fit — the same
    /// stale verdict [`diff::apply`] reaches — is simply absent, which is what keeps the
    /// overlay from painting a change the accept would refuse.
    ///
    /// Preview mode has no caret and is a reading view, and raw mode is deliberately the
    /// file as it sits on disk; a proposal is neither, so the overlay is off in both.
    pub(crate) fn placed(&self) -> Vec<Placed> {
        if self.view != View::Live {
            return Vec::new();
        }
        // The `Ref` is held for the whole placement pass: `text` is a slice of the
        // buffer, so the borrow has to outlive it. Nothing in here edits the buffer.
        let buf = self.note.buffer.borrow();
        let text = buf.text();
        let mut out = Vec::new();
        for (edit, proposal) in self.pending_edits.iter().enumerate() {
            for (hunk, h) in proposal.hunks.iter().enumerate() {
                let Some(at) = diff::place(text, h) else {
                    continue;
                };
                out.push(Placed {
                    edit,
                    hunk,
                    id: proposal.id.clone(),
                    removed: at.removed,
                    added: at.added,
                    current: proposal.current == Some(hunk),
                    collapsed: self.collapsed_inline.contains(&(proposal.id.clone(), hunk)),
                });
            }
        }
        out
    }

    /// The rows whose bare `y`/`x`/`za` belong to a proposal rather than to vim.
    ///
    /// A phantom row has no buffer row to stand on, so "the cursor is inside a phantom
    /// region" is answered as the rows around it: the struck rows the proposal would
    /// take away, and for a hunk that only adds, the row it would add above. That is the
    /// line the caret is on when the user is looking at an insertion, which is what makes
    /// `y` mean the same thing there as it does on a struck line.
    pub(crate) fn review_lines(&self) -> Vec<(usize, usize)> {
        if self.view != View::Live {
            return Vec::new();
        }
        let last = self.note.buffer.borrow().line_count().saturating_sub(1);
        let mut rows: Vec<usize> = Vec::new();
        for placed in self.placed() {
            rows.extend(placed.removed.iter().copied());
            for (anchor, _) in &placed.added {
                // Anchored before `anchor`; the caret for that spot is on `anchor` when
                // there is such a row, and on the row above when the addition is at the
                // end of the note.
                rows.push(if *anchor <= last { *anchor } else { last });
            }
        }
        rows.sort_unstable();
        rows.dedup();
        // Merged into spans because the engine asks "is the cursor on one of these", and
        // a two-element range per row would make that a walk of the whole list.
        let mut lines: Vec<(usize, usize)> = Vec::new();
        for row in rows {
            match lines.last_mut() {
                Some(last) if last.1 + 1 == row => last.1 = row,
                _ => lines.push((row, row)),
            }
        }
        lines
    }

    /// The overlay as this editor would paint it: one line per struck row and per
    /// phantom row, in the order the note draws them. For the tests, which should read
    /// the shape of an overlay rather than squint at pixels.
    #[cfg(test)]
    pub(crate) fn overlay_shape(&self) -> Vec<String> {
        let mut out = Vec::new();
        for placed in self.placed() {
            let (removed, added) = placed.counts();
            out.push(format!(
                "hunk {}: −{removed} +{added}{}{}",
                placed.hunk,
                if placed.collapsed { " [collapsed]" } else { "" },
                if placed.current { " [marked]" } else { "" },
            ));
            for row in &placed.removed {
                out.push(format!("  strike row {row}"));
            }
            // A collapsed hunk paints its count instead of its lines, which is what
            // [`MarkdownEditor::frame`] does with the same flag.
            if placed.collapsed {
                continue;
            }
            for (anchor, lines) in &placed.added {
                if lines.is_empty() {
                    continue;
                }
                out.push(format!("  before row {anchor}"));
                for line in lines {
                    out.push(format!("    + {line}"));
                }
            }
        }
        out
    }

    /// What the transclusions would paint, for the tests: the row that asks, then the
    /// body it brings, indented by how deep each row sits, a stopped embed in brackets.
    #[cfg(test)]
    pub(crate) fn embed_shape(&mut self) -> Vec<String> {
        let Some(vault) = self.vault.as_ref() else {
            return Vec::new();
        };
        if self.view == View::Raw {
            return Vec::new();
        }
        let mut out = Vec::new();
        let sources = embeds::OpenFirst {
            notes: &self.notes,
            vault,
        };
        for (row, body) in self
            .embeds
            .resolve(vault, self.note.buffer.borrow().text(), &sources)
        {
            match body {
                Body::Nothing => {}
                Body::Badge(name) => out.push(format!("embed row {row}: badge {name}")),
                Body::Lines(lines) => {
                    out.push(format!("embed row {row}"));
                    for EmbedLine { depth, origin, row } in lines {
                        let text = match row {
                            EmbedRow::Text(text) => text,
                            // Brackets because a stopped embed is a name, not a line.
                            EmbedRow::Badge(text) => format!("[{text}]"),
                        };
                        // The address is part of the shape a test can pin: a painted
                        // row that does not know which note and row it came from
                        // cannot be clicked, caret-ed into, or checked for existing.
                        let at = origin
                            .map(|o| format!("@{}:{}", o.path.display(), o.row))
                            .unwrap_or_default();
                        out.push(if at.is_empty() {
                            format!("{}{text}", "  ".repeat(depth))
                        } else {
                            format!("{}{at} {text}", "  ".repeat(depth))
                        });
                    }
                }
            }
        }
        out
    }

    /// The proposal the cursor is standing on, if any — which is what decides whether
    /// `za` collapses a proposal or folds a section, and which hunk the strip's `y`/`x`
    /// should act on when the note has the keyboard.
    pub(crate) fn hunk_at_cursor(&self) -> Option<Placed> {
        let (row, _) = self
            .note
            .buffer
            .borrow()
            .row_col(self.note.buffer.borrow().cursor());
        self.placed()
            .into_iter()
            .find(|placed| self.covers(placed, row))
    }

    /// Whether a placed hunk speaks for buffer row `row`.
    fn covers(&self, placed: &Placed, row: usize) -> bool {
        if placed.removed.contains(&row) {
            return true;
        }
        let last = self.note.buffer.borrow().line_count().saturating_sub(1);
        placed.added.iter().any(|(anchor, _)| {
            let anchor_row = if *anchor <= last { *anchor } else { last };
            anchor_row == row
        })
    }

    /// Collapse a proposal's additions to a count, or bring them back (`za` on one of
    /// its lines). Returns whether the hunk is collapsed afterwards.
    ///
    /// This is the overlay's own collapse, keyed by proposal and hunk. The strip's `za`
    /// collapses a *proposal* to one row in the list; these are different widgets
    /// collapsing different things, and sharing the state would make one key do two
    /// things at once.
    pub(crate) fn toggle_inline_collapse(&mut self, id: &str, hunk: usize) -> bool {
        let key = (SharedString::from(id.to_string()), hunk);
        if !self.collapsed_inline.remove(&key) {
            self.collapsed_inline.insert(key);
            true
        } else {
            false
        }
    }
}

/// One phantom block: the lines one hunk would add, or the count it shows when the user
/// collapsed it — or the body of a `![[transclusion]]`, which hangs off the row that
/// asks for it the same way (roadmap P4.0a).
///
/// The two share a type because the geometry is the same in both cases: rows that are
/// not in the buffer, painted beside a host row, invisible to everything that moves
/// through the note. What differs is where the text came from and how it is dressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Block {
    /// The lines to paint. Empty when the block is collapsed or is a bare badge.
    pub(crate) lines: Vec<BlockLine>,
    /// Set when collapsed: the `−N +M` that stands in for the lines.
    pub(crate) badge: Option<String>,
    /// The hunk the strip's marker is on, which paints a wider bar.
    pub(crate) current: bool,
    pub(crate) kind: BlockKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockKind {
    /// A queued proposal's additions: the palette's green, no markdown styling (proposed
    /// text is a change being offered, not the note's prose), no indent.
    Proposal,
    /// Another note's lines: the note's own colours and markdown, stepped right by depth.
    Embed,
}

/// One row of a phantom block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockLine {
    pub(crate) text: String,
    /// How many embeds deep this row is: 0 for a proposal's rows, 1 for the body of the
    /// host's own `![[…]]`, deeper for an embed inside that.
    pub(crate) depth: usize,
    /// A row that names an embed rather than carrying its content — an embed past the
    /// depth cap, or one already on the chain. It wears the link colour, because the
    /// name is the thing the reader can act on.
    pub(crate) badge: bool,
    /// Which note this row is a line *of*, and which row of it. `None` for a
    /// proposal's rows (they belong to no note until they are accepted) and for a
    /// badge. This is what makes the painted row addressable: a click on it, a
    /// caret in it, and the check for "is this section still there" all start here
    /// (roadmap P4.0b).
    pub(crate) origin: Option<Origin>,
}

/// Where the overlay's pieces go in one frame.
pub(crate) struct Frame {
    /// Buffer row -> whether the strip's marker is on the hunk that would take it away.
    /// Every row here is painted struck through, in the palette's red, with a bar down
    /// its left edge.
    pub(crate) struck: HashMap<usize, bool>,
    /// Display-line index -> the blocks hanging **after** that line.
    pub(crate) after: HashMap<usize, Vec<Block>>,
    /// Display-line index -> the blocks hanging **before** that line, which is what an
    /// addition at the very top of a note needs.
    pub(crate) before: HashMap<usize, Vec<Block>>,
}

impl MarkdownEditor {
    /// Lay the placed hunks out over this frame's display lines.
    ///
    /// A block anchored before row `A` hangs after the last **visible** row above `A`,
    /// which is what keeps it in the right place when the rows around it are folded away
    /// or hidden by live preview. With nothing visible above it — an addition at the top
    /// of the note — it hangs before the first visible line instead.
    pub(crate) fn frame(&mut self, display: &[DisplayLine]) -> Frame {
        let mut frame = Frame {
            struck: HashMap::new(),
            after: HashMap::new(),
            before: HashMap::new(),
        };
        // The rows this frame actually draws, and their first and last display lines.
        let mut visible: Vec<usize> = Vec::new();
        let mut first_of: HashMap<usize, usize> = HashMap::new();
        let mut last_of: HashMap<usize, usize> = HashMap::new();
        for (index, line) in display.iter().enumerate() {
            match first_of.entry(line.row) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(index);
                    visible.push(line.row);
                }
                std::collections::hash_map::Entry::Occupied(_) => {}
            }
            last_of.insert(line.row, index);
        }
        for placed in self.placed() {
            for row in &placed.removed {
                frame.struck.insert(*row, placed.current);
            }
            let (removed, added) = placed.counts();
            for (anchor, lines) in placed.added {
                let block = Block {
                    // A collapsed hunk trades its lines for a count, which is what lets
                    // a twelve-line proposal be skimmed without twelve rows of green.
                    badge: placed.collapsed.then(|| format!("−{removed} +{added}")),
                    lines: if placed.collapsed {
                        Vec::new()
                    } else {
                        lines
                            .into_iter()
                            .map(|text| BlockLine {
                                text,
                                depth: 0,
                                badge: false,
                                // A proposed row belongs to no note until the
                                // proposal is accepted, so it has no address.
                                origin: None,
                            })
                            .collect()
                    },
                    current: placed.current,
                    kind: BlockKind::Proposal,
                };
                // The last visible row above the anchor: `visible` is in buffer order, so
                // walking backwards finds it without scanning the whole note again.
                let host = visible.iter().rev().copied().find(|r| *r < anchor);
                match host {
                    Some(host) => frame.after.entry(last_of[&host]).or_default().push(block),
                    None => {
                        if let Some(row) = visible.first().copied() {
                            frame.before.entry(first_of[&row]).or_default().push(block);
                        }
                    }
                }
            }
        }
        self.embed_frame(display, &mut frame, &last_of);
        frame
    }

    /// The transclusions (roadmap P4.0a): a row that is nothing but `![[…]]` shows the
    /// body of what it points at **under itself**.
    ///
    /// The host row stays on screen — it is the author's own text, editable, and the
    /// thing that says where the borrowed lines come from. The body hangs after it, which
    /// is the same anchor a proposal's additions use, so one insertion pass draws both.
    ///
    /// Raw mode shows none of it: raw mode is the file as it sits on disk, and an embed's
    /// body is not in this file.
    fn embed_frame(
        &mut self,
        _display: &[DisplayLine],
        frame: &mut Frame,
        last_of: &HashMap<usize, usize>,
    ) {
        let Some(vault) = self.vault.as_ref() else {
            return;
        };
        if self.view == View::Raw {
            return;
        }
        let sources = embeds::OpenFirst {
            notes: &self.notes,
            vault,
        };
        for (row, body) in self
            .embeds
            .resolve(vault, self.note.buffer.borrow().text(), &sources)
        {
            // A row folded away has no display line to hang a body under, and painting
            // one anyway would show content from behind a closed fold.
            let Some(&host) = last_of.get(&row) else {
                continue;
            };
            let block = match body {
                Body::Nothing => continue,
                Body::Badge(name) => Block {
                    lines: Vec::new(),
                    badge: Some(name),
                    current: false,
                    kind: BlockKind::Embed,
                },
                Body::Lines(rows) => Block {
                    lines: rows
                        .into_iter()
                        .map(|EmbedLine { depth, origin, row }| match row {
                            EmbedRow::Text(text) => BlockLine {
                                text,
                                depth,
                                badge: false,
                                origin,
                            },
                            EmbedRow::Badge(text) => BlockLine {
                                text,
                                depth,
                                badge: true,
                                origin: None,
                            },
                        })
                        .collect(),
                    badge: None,
                    current: false,
                    kind: BlockKind::Embed,
                },
            };
            frame.after.entry(host).or_default().push(block);
        }
    }

    /// The phantom rows for a run of blocks, in order: a collapsed block's count first
    /// (there is only one line of it), then the lines it would add.
    ///
    /// They are shaped at the note's body size rather than the host row's, because the
    /// lines a proposal brings are new text, not a heading the note already had.
    pub(crate) fn phantom_lines(&self, blocks: &[Block]) -> Vec<PhantomLine> {
        let mut out = Vec::new();
        for block in blocks {
            if let Some(badge) = block.badge.clone() {
                out.push(PhantomLine {
                    text: badge.into(),
                    family: self.font_family.clone(),
                    size: self.font_size,
                    palette: Rc::clone(&self.palette),
                    kind: match block.kind {
                        BlockKind::Proposal => PhantomKind::Badge,
                        BlockKind::Embed => PhantomKind::EmbedBadge,
                    },
                    current: block.current,
                    indent: 0,
                    spans: Vec::new(),
                    origin: None,
                    hits: Rc::clone(&self.phantom_hits),
                });
            }
            // An embed's rows are dressed the way live preview dresses the note's own:
            // scanned over the **whole body at once** (a fence that opens on one row and
            // closes on another is invisible to a per-row scan), then markers hidden and
            // the spans re-based onto what is actually painted. A proposal's rows are
            // not: proposed text is a change being offered, and the syntax it carries is
            // part of what the reader is being asked to accept.
            let texts: Vec<String> = block.lines.iter().map(|l| l.text.clone()).collect();
            let painted = if block.kind == BlockKind::Embed {
                embeds::paint_body(&texts)
            } else {
                Vec::new()
            };
            for (ix, line) in block.lines.iter().enumerate() {
                let (text, spans) = match (block.kind, line.badge) {
                    (BlockKind::Embed, false) => {
                        (painted[ix].text.clone(), painted[ix].spans.clone())
                    }
                    // A stopped embed is a name, not markdown: it keeps the link
                    // colour and nothing else.
                    _ => (line.text.clone(), Vec::new()),
                };
                out.push(PhantomLine {
                    text: text.into(),
                    family: self.font_family.clone(),
                    // A heading inside an embedded note is sized like a heading of
                    // that level anywhere else in the app.
                    size: match painted.get(ix).and_then(|row| row.heading) {
                        Some(level) => theme::font_size(level, self.font_size),
                        None => self.font_size,
                    },
                    palette: Rc::clone(&self.palette),
                    kind: match (block.kind, line.badge) {
                        (BlockKind::Proposal, _) => PhantomKind::Added,
                        (BlockKind::Embed, true) => PhantomKind::EmbedBadge,
                        (BlockKind::Embed, false) => PhantomKind::Embed,
                    },
                    current: block.current,
                    indent: line.depth,
                    spans,
                    // The row's address travels with it to the paint, which is where
                    // the on-screen band gets paired back up with it.
                    origin: line.origin.clone(),
                    hits: Rc::clone(&self.phantom_hits),
                });
            }
        }
        out
    }
}
