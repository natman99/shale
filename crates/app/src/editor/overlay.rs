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

use gpui::{px, Context, IntoElement, LineWrapperHandle, Pixels, SharedString, Styled};
use gpui::{InteractiveElement, ParentElement};
use shale_core::diff::{self, Hunk};
use shale_core::transclude::{Body, EmbedLine, EmbedRow, Origin};
use shale_core::vim::Mode;

use crate::line::marks::{self, GutterMark};
use crate::line::phantom::{PhantomKind, PhantomLine, INDENT_STEP};
use crate::line::DisplayLine;
use crate::theme;

use super::{embeds, MarkdownEditor, View};
use crate::line::LineElement;

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
                    // The gutter is part of the shape a test can pin. A dot that is not
                    // in the frame cannot be asserted on; a dot in the frame but not on
                    // screen is a different bug, and the paint tests' problem.
                    let gutter = self.embed_gutter(
                        &lines
                            .iter()
                            .map(|line| line.origin.clone())
                            .collect::<Vec<_>>(),
                    );
                    for (EmbedLine { depth, origin, row }, row_gutter) in
                        lines.into_iter().zip(gutter)
                    {
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
                        let dot = if row_gutter.iter().any(|mark| {
                            matches!(
                                mark,
                                GutterMark::Glyph {
                                    mark: marks::Mark::Dot,
                                    ..
                                }
                            )
                        }) {
                            crate::style::GLYPH_DIRTY
                        } else {
                            ""
                        };
                        out.push(if at.is_empty() {
                            format!("{}{dot}{text}", "  ".repeat(depth))
                        } else {
                            format!("{}{dot}{at} {text}", "  ".repeat(depth))
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
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Block {
    /// The lines to paint. Empty when the block is collapsed or is a bare badge.
    pub(crate) lines: Vec<BlockLine>,
    /// Set when collapsed: the `−N +M` that stands in for the lines.
    pub(crate) badge: Option<String>,
    /// The hunk the strip's marker is on, which paints a wider bar.
    pub(crate) current: bool,
    pub(crate) kind: BlockKind,
    /// The block this view is *editing* (roadmap P4.0b): its rows are painted as
    /// ordinary line elements over the embedded note's buffer, so they carry a caret,
    /// a selection and a click target. Every other embed block is a picture.
    pub(crate) entered: bool,
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
#[derive(Debug, Clone, PartialEq)]
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
    /// What this row hangs in the gutter, decided by the frame and carried to whichever
    /// element paints the row — a picture or a line element, since one embedded note's
    /// rows can be either inside the same block. Today: the dot for a note ahead of its
    /// file.
    pub(crate) gutter: Vec<GutterMark>,
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
                    entered: false,
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
                                // ...and nothing to hang in the gutter beyond the
                                // proposal's own rule.
                                gutter: Vec::new(),
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
    /// The gutter marks for a body's rows: a dot on the first row of each note's run
    /// inside the block, when that note is ahead of its file (roadmap P4.0b).
    ///
    /// Per run and not per block, because a nested embed's rows are a *different* note's
    /// text — the outer block's top row is not where its unsaved words are. The host's
    /// tab dot is already transitive, so the tab says *that* something is unwritten; this
    /// says *which block on screen* it is in, which is the half a tab cannot tell you —
    /// and an embedded note with no tab of its own has nothing else to light.
    fn embed_gutter(&self, origins: &[Option<Origin>]) -> Vec<Vec<GutterMark>> {
        let mut shown: Option<std::path::PathBuf> = None;
        origins
            .iter()
            .map(|origin| match origin.as_ref() {
                Some(o) if shown.as_ref() != Some(&o.path) => {
                    shown = Some(o.path.clone());
                    if self.notes.get(&o.path).is_some_and(|note| note.dirty.get()) {
                        vec![GutterMark::Glyph {
                            mark: marks::Mark::Dot,
                            color: self.palette.accent,
                        }]
                    } else {
                        Vec::new()
                    }
                }
                // A badge row belongs to no note, and does not end the run of the note
                // whose rows surround it.
                _ => Vec::new(),
            })
            .collect()
    }

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
        // The entered block's rows are painted from the embedded note's buffer, so the
        // links coloured in them are that note's own — cut from that buffer, and only
        // recut when that buffer moved. The host's list is still refreshed for the
        // host's rows; the two are not the same offsets.
        if let Some(embed) = self.entered.as_mut() {
            let change = embed.note.buffer.borrow().change_id();
            if embed.links_change != change {
                embed.links_change = change;
                embed.links =
                    shale_core::markdown::extract_wikilinks(embed.note.buffer.borrow().text());
            }
        }
        // Re-derived every frame, so the anchor cannot outlive the block it points at.
        self.entered_anchor = None;
        let mut painted_entered = false;
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
                    entered: false,
                },
                Body::Lines(rows) => {
                    let mut lines: Vec<BlockLine> = rows
                        .into_iter()
                        .map(|EmbedLine { depth, origin, row }| match row {
                            EmbedRow::Text(text) => BlockLine {
                                text,
                                depth,
                                badge: false,
                                origin,
                                gutter: Vec::new(),
                            },
                            EmbedRow::Badge(text) => BlockLine {
                                text,
                                depth,
                                badge: true,
                                origin: None,
                                gutter: Vec::new(),
                            },
                        })
                        .collect();
                    let gutter = self.embed_gutter(
                        &lines
                            .iter()
                            .map(|line| line.origin.clone())
                            .collect::<Vec<_>>(),
                    );
                    for (line, gutter) in lines.iter_mut().zip(gutter) {
                        line.gutter = gutter;
                    }
                    Block {
                        // One level of editing, so at most one block of the frame is
                        // the one being typed into.
                        entered: self.entered.as_ref().is_some_and(|embed| {
                            lines.iter().any(|line| {
                                line.origin.as_ref().is_some_and(|o| o.path == embed.path)
                            })
                        }),
                        lines,
                        badge: None,
                        current: false,
                        kind: BlockKind::Embed,
                    }
                }
            };
            // The frame is the only place that knows which host row a body hangs off,
            // and while a block is being edited that row is the caret's scroll target.
            if block.entered {
                self.entered_anchor = Some(row);
                self.entered_host_row = Some(row);
                painted_entered = true;
            }
            frame.after.entry(host).or_default().push(block);
        }
        // Ruling Q5: if the block the caret is inside is not on screen any more — the
        // section was deleted, the note went away, the line that asked for it folded —
        // then the caret is editing something that no longer exists here, and it leaves
        // with a line saying so. A *view* block of the same note badges instead,
        // because there is no caret inside it to rescue: the caret decides.
        if !painted_entered {
            self.embed_gone = self.entered.as_ref().map(|embed| embed.path.clone());
        }
    }

    /// Which rows of a block live preview shows as the file holds them: the caret's
    /// and the selection's, and only in the block being edited. A block that is only
    /// being read has no caret inside it, so it reads rendered; preview mode hides
    /// markers everywhere, in the block exactly as in the note.
    fn block_reveal(&self, block: &Block) -> embeds::Reveal {
        if !block.entered || self.view != View::Live {
            return embeds::Reveal::default();
        }
        // The caret's buffer, which while the caret is inside is the embedded note's.
        let buffer = self.buffer.borrow();
        let selection = match self.vim.mode() {
            Mode::Visual | Mode::VisualLine => self.vim.selection_range(&buffer),
            _ => buffer.selection(),
        };
        embeds::Reveal {
            cursor: Some(buffer.cursor()),
            selection,
        }
    }

    /// The phantom rows for a run of blocks, in order: a collapsed block's count first
    /// (there is only one line of it), then the lines it would add.
    ///
    /// They are shaped at the note's body size rather than the host row's, because the
    /// lines a proposal brings are new text, not a heading the note already had.
    ///
    /// `width` is what the host's own rows get to wrap in, and `wrappers` are the note's:
    /// a block's rows are measured with the same faces and sizes, or a break point lands
    /// in the middle of a word the paint is still drawing whole.
    pub(crate) fn phantom_lines(
        &self,
        blocks: &[Block],
        char_width: Pixels,
        editor: gpui::Entity<MarkdownEditor>,
        width: Pixels,
        wrappers: &mut [LineWrapperHandle],
    ) -> Vec<gpui::AnyElement> {
        let mut out = Vec::new();
        for block in blocks {
            if let Some(badge) = block.badge.clone() {
                out.push(
                    PhantomLine {
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
                        gutter: Vec::new(),
                        hits: Rc::clone(&self.phantom_hits),
                    }
                    .into_any_element(),
                );
            }
            // An embed's rows are dressed the way live preview dresses the note's own:
            // scanned over the **whole body at once** (a fence that opens on one row and
            // closes on another is invisible to a per-row scan), then markers hidden and
            // the spans re-based onto what is actually painted. A proposal's rows are
            // not: proposed text is a change being offered, and the syntax it carries is
            // part of what the reader is being asked to accept.
            let texts: Vec<String> = block.lines.iter().map(|l| l.text.clone()).collect();
            // An entered block is painted from the embedded note's own buffer, so each
            // row's paint is anchored where that row starts in the note. The rest of
            // the frame's rows are pictures and need no anchor.
            let starts: Vec<usize> = if block.entered {
                let embed = self
                    .entered
                    .as_ref()
                    .expect("an entered block has a caret note");
                block
                    .lines
                    .iter()
                    .map(|line| {
                        line.origin
                            .as_ref()
                            .filter(|o| o.path == embed.path)
                            .map(|o| embed.note.buffer.borrow().line_start(o.row))
                            .unwrap_or(0)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let painted = if block.kind == BlockKind::Embed {
                embeds::paint_body(&texts, &starts, self.block_reveal(block))
            } else {
                Vec::new()
            };
            // Where each row breaks. A block has no viewport of its own — the host
            // scrolls, ruling 12 — so a row longer than the pane would simply leave the
            // frame with no way back. A nested row has less room by the step its depth
            // costs it; the block being edited sits at the host's own margin.
            let ranges: Vec<Vec<(usize, usize)>> = block
                .lines
                .iter()
                .enumerate()
                .map(|(ix, line)| match painted.get(ix) {
                    Some(row) if !line.badge => {
                        let room = (width - px(line.depth as f32 * INDENT_STEP)).max(px(0.));
                        embeds::wrap_body(std::slice::from_ref(row), room, wrappers, &self.palette)
                            .pop()
                            .unwrap_or_else(|| vec![(0, row.text.len())])
                    }
                    // A badge is one line by construction.
                    _ => vec![(0, line.text.len())],
                })
                .collect();
            // The block being edited's rows go in one wrapper, so the block has one
            // pointer handler rather than one per row.
            let mut rows: Vec<gpui::AnyElement> = Vec::new();
            for (ix, line) in block.lines.iter().enumerate() {
                let (full, full_spans) = match (block.kind, line.badge) {
                    (BlockKind::Embed, false) => {
                        (painted[ix].text.clone(), painted[ix].spans.clone())
                    }
                    // A stopped embed is a name, not markdown: it keeps the link
                    // colour and nothing else.
                    _ => (line.text.clone(), Vec::new()),
                };
                for (part, &(lo, hi)) in ranges[ix].iter().enumerate() {
                    // What belongs to the row belongs to its first display line: the
                    // gutter dot and the block's rule mark the row, not every piece of
                    // it. A markdown span that straddles a break becomes two, each
                    // measured from the piece it now lives on.
                    let first = part == 0;
                    let spans: Vec<(usize, usize, shale_core::mdstyle::Style)> = full_spans
                        .iter()
                        .filter(|(s, e, _)| *s < hi && *e > lo)
                        .map(|span| (span.0.max(lo) - lo, span.1.min(hi) - lo, span.2))
                        .collect();
                    // The block being edited is not a picture: its rows are line elements
                    // over the embedded buffer, which is what gives them a caret, a
                    // selection and a click that lands on the right byte (roadmap P4.0b).
                    if block.entered && !line.badge {
                        if let (Some(origin), Some(row)) = (line.origin.as_ref(), painted.get(ix)) {
                            rows.push(
                                self.embed_line(
                                    origin,
                                    row,
                                    (lo, hi),
                                    char_width,
                                    &editor,
                                    if first { &line.gutter } else { &[] },
                                )
                                .into_any_element(),
                            );
                            continue;
                        }
                    }
                    out.push(
                        PhantomLine {
                            text: full[lo..hi].to_string().into(),
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
                            gutter: if first {
                                line.gutter.clone()
                            } else {
                                Vec::new()
                            },
                            hits: Rc::clone(&self.phantom_hits),
                        }
                        .into_any_element(),
                    );
                }
            }
            if !rows.is_empty() {
                // A word inside the block being edited is a word the checker is
                // looking at, so the pointer asks about it the way it asks about the
                // note's own. Which row it is asking about comes from the bands this
                // frame painted, and the number it reports is tagged as the other
                // note's — the window cannot tell that from the number alone.
                out.push(
                    gpui::div()
                        .w_full()
                        .children(rows)
                        .on_mouse_move({
                            let editor = editor.clone();
                            move |event: &gpui::MouseMoveEvent, _window, cx| {
                                let at = event.position;
                                let asked =
                                    editor.read_with(cx, |editor, _cx| editor.embed_hover_at(at));
                                if let Some((row, offset)) = asked {
                                    editor.update(cx, |_, cx| {
                                        cx.emit(crate::editor::EditorEvent::Hovered {
                                            offset,
                                            line: crate::editor::LineRef::Embed(row),
                                            at,
                                        })
                                    });
                                }
                            }
                        })
                        .into_any_element(),
                );
            }
        }
        out
    }

    /// One row of the entered embed, as the same element a note paints its own rows
    /// with — same runs, same caret, same selection, same click-to-offset — measured
    /// from the embedded note's buffer instead of this tab's (roadmap P4.0b).
    ///
    /// It hangs inside the host's wrapper div rather than joining the scroller's
    /// children, so the display-line index space stays the host's and the scroll
    /// target of a caret in the block is the host line the block hangs off.
    fn embed_line(
        &self,
        origin: &shale_core::transclude::Origin,
        painted: &embeds::PaintedRow,
        segment: (usize, usize),
        char_width: Pixels,
        editor: &gpui::Entity<MarkdownEditor>,
        gutter: &[GutterMark],
    ) -> LineElement {
        let embed = self
            .entered
            .as_ref()
            .expect("only the entered block reaches here");
        let buffer = embed.note.buffer.borrow();
        // The row's painted map covers the whole buffer row; a soft-wrapped row hands
        // one segment of it to this element, and the slice keeps the note offsets the
        // segment came from. `start..end` is the buffer span the segment stands for,
        // hidden markers inside it included, which is what the host's display lines
        // hand their elements too.
        let start = painted.map.offset_for_index(segment.0);
        let end = painted.map.offset_for_index(segment.1);
        let map = painted.map.slice(segment.0, segment.1);
        let text = map.painted_text(buffer.text());
        let cursor = buffer.cursor();
        // The embedded note's own wiki-links on this row: coloured like links, and
        // followed by `gd` or ctrl-click, because a link inside a block you are typing
        // into that does nothing is a hole in the block, not in the note.
        // The checker's notes for this note, clipped to the row — the server is looking
        // at this buffer while the caret is inside it.
        let marks: Vec<(usize, usize)> = embed
            .marks
            .iter()
            .filter(|(s, e)| *s < end && *e > start)
            .map(|&(s, e)| (s.max(start), e.min(end)))
            .collect();
        let links: Vec<(usize, usize, bool)> = embed
            .links
            .iter()
            .filter(|link| link.start < end && link.end > start)
            .map(|link| {
                (
                    link.start.max(start),
                    link.end.min(end),
                    self.is_known(&link.target),
                )
            })
            .collect();
        let selection = match self.vim.mode() {
            Mode::Visual | Mode::VisualLine => self.vim.selection_range(&buffer),
            _ => buffer.selection(),
        };
        // The caret's display row, in the *embedded* note's display model — which is
        // the editor's slot while the caret is inside (ruling 19).
        let cursor_row = buffer.display_index_for(&self.slot, cursor);
        let line_sel = selection.and_then(|(anchor, head)| {
            let (lo, hi) = (anchor.min(head), anchor.max(head));
            if hi <= start || lo >= end {
                return None;
            }
            let s = map.index_for_offset(lo);
            let e = map.index_for_offset(hi);
            (e > s).then_some((s, e))
        });
        // A wrapped row belongs to one segment for caret purposes: the boundary between
        // two segments goes to the later one, except at the end of the row, where there
        // is no later one and the caret sits after the last character.
        let holds_caret = cursor >= start && (cursor < end || end == buffer.line_end(origin.row));
        let cursor_col = (cursor_row == origin.row && self.view != View::Preview && holds_caret)
            .then(|| map.index_for_offset(cursor));
        LineElement {
            editor: editor.clone(),
            embed: true,
            row: origin.row,
            index: origin.row,
            start,
            end,
            text: text.into(),
            map,
            hidden: Vec::new(),
            quotes: Vec::new(),
            line_sel,
            cursor_col,
            cursor_dim: self.cursor_dim,
            insert: self.vim.mode() == Mode::Insert,
            // The embedded note's own links on this row: a link inside a block you are
            // typing into that does nothing is a hole in the block, not in the note.
            links,
            spans: painted.buffer_spans.clone(),
            // The checker's notes for this note, clipped to the row: the server holds
            // this buffer while the caret is inside it.
            marks,
            // No search matches: `/` asks the note's own buffer and stops at the seam,
            // so there is nothing to draw in a block that belongs to another note.
            hits: Vec::new(),
            active: None,
            size: match painted.heading {
                Some(level) => theme::font_size(level, self.font_size),
                None => self.font_size,
            },
            dim: 1.0,
            block: false,
            rule: false,
            callout: None,
            callout_mark: false,
            table: None,
            image: None,
            char_width,
            palette: Rc::clone(&self.palette),
            fold: None,
            diff: None,
            gutter: gutter.to_vec(),
        }
    }
}
