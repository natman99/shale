//! The `![[transclusions]]` in a note, resolved for the frame that paints them
//! (roadmap P4.0a).
//!
//! The resolution itself is `shale_core::transclude`'s — pure, tested, no gpui. What
//! lives here is the question of *when*: the host's text is re-scanned every frame
//! (cheap, and it must be, because the line the user just typed is an embed the frame
//! has to show), but the bodies come from other files, and re-reading those every frame
//! would put disk I/O in the paint path.
//!
//! So the bodies are cached, keyed by the reference the author wrote — target and
//! anchor together, because `![[a#One]]` and `![[a#Two]]` are different rows of the same
//! file. The cache is dropped whole when the vault reports a change. That is coarser
//! than invalidating one entry per changed file, and deliberately so: the watcher
//! already knows which paths moved, but a note's embeds are a handful of small reads,
//! and a stale embed body — a note showing another note as it was before it was edited
//! — is the kind of wrong that costs more trust than a few syscalls.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use shale_core::mdlayout;
use shale_core::mdstyle::{quotes, style_spans, Style};
use shale_core::transclude::{self, Body, Sources};
use shale_core::vault::{Vault, VaultEntry};

use crate::preview::PieceMap;

/// One row of an embed body as it is painted: the text with its markdown markers
/// hidden, and the style spans re-based onto that text.
pub(crate) struct PaintedRow {
    pub(crate) text: String,
    /// Byte ranges over `text`, ready for `runs::build_runs`.
    pub(crate) spans: Vec<(usize, usize, Style)>,
    /// The heading level if this row is one. The note sizes its headings by level;
    /// an embedded heading that stayed at body size would be the same inconsistency
    /// the hidden markers used to be, one level down.
    pub(crate) heading: Option<u8>,
    /// The map this row was painted through, and its markdown spans, in the **note's**
    /// byte space rather than the joined body's.
    ///
    /// An entered embed paints its rows as ordinary line elements, and those answer
    /// clicks and place the caret by buffer offset — so they need the same map the
    /// phantom paint used, measured from the note rather than from a string that only
    /// exists for the frame. The shift is one addition per row, and only the caller
    /// knows which buffer each row came from: a body's rows can belong to different
    /// notes, when the embed has embeds of its own.
    pub(crate) map: PieceMap,
    pub(crate) buffer_spans: Vec<(usize, usize, Style)>,
}

/// Dress an embed body the way live preview dresses the note's own rows.
///
/// The host note hides the `#` of a heading and the `**` around bold text unless the
/// cursor is on that row. An embed's rows have no cursor — they are not in the buffer,
/// and until P4.0b nothing can put one there — so the rule resolves the same way for
/// every one of them: markers hidden, always. Without this an embedded heading shows
/// its `#` next to a host heading that has hidden its own, and the two readings of the
/// same markdown sit one under the other.
///
/// The scan runs over the **whole body at once** and is then cut per row, because a
/// fence that opens on one row and closes on another is only visible to a scan that
/// sees both; styling rows one at a time would paint the second half of a code block
/// as prose.
/// `starts[ix]` is where line `ix` begins in the note it came from, in that note's
/// byte offsets — the anchor the joined coordinates are shifted onto. Pass an empty
/// slice when nobody needs the note's coordinates: the joined body is a string that
/// exists only for this frame, and a row painted as a picture has nothing to be
/// measured against.
pub(crate) fn paint_body(lines: &[String], starts: &[usize]) -> Vec<PaintedRow> {
    let joined: String = lines
        .iter()
        .map(|line| {
            format!(
                "{line}
"
            )
        })
        .collect();
    let spans = style_spans(&joined);
    // The straight quotes the frame draws curly, exactly as the note's own rows get
    // them: an embedded sentence should not be the one place typography changes.
    let subs: Vec<(usize, char)> = quotes::marks(&joined, &spans)
        .iter()
        .map(|mark| (mark.offset, mark.glyph()))
        .collect();
    let as_tuples: Vec<(usize, usize, Style)> = spans
        .iter()
        .map(|span| (span.start, span.end, span.style))
        .collect();

    let mut out = Vec::with_capacity(lines.len());
    let mut offset = 0usize;
    for (ix, line) in lines.iter().enumerate() {
        let (start, end) = (offset, offset + line.len());
        // The same row, measured from its own note. A row with no anchor stays in
        // joined space, which is where a picture lives.
        let shift = starts
            .get(ix)
            .map(|&note_start| note_start as isize - start as isize)
            .unwrap_or(0);
        let at = |x: usize| (x as isize + shift) as usize;
        let hidden = mdlayout::hidden_ranges(&joined, &spans, start, end);
        let pieces = mdlayout::visible_pieces(start, end, &hidden);
        let row_subs: Vec<(usize, char)> = subs
            .iter()
            .copied()
            .filter(|(at, _)| *at >= start && *at < end)
            .collect();
        let map = PieceMap::with_substitutions(&pieces, &row_subs);
        let heading = spans.iter().find_map(|span| match span.style {
            Style::Heading { level } if span.start >= start && span.start < end => Some(level),
            _ => None,
        });
        out.push(PaintedRow {
            // The pieces name offsets into `joined`, so the painted string is read
            // from the same string — and holds only this row, because only this
            // row's pieces are in the map.
            text: map.painted_text(&joined),
            spans: map.rebase_spans(&as_tuples),
            heading,
            map: PieceMap::with_substitutions(
                &pieces
                    .iter()
                    .map(|&(s, e)| (at(s), at(e)))
                    .collect::<Vec<_>>(),
                &row_subs
                    .iter()
                    .map(|&(offset_at, c)| (at(offset_at), c))
                    .collect::<Vec<_>>(),
            ),
            // The scanner's spans, unhidden and clipped to this row, in the note's
            // space: the line element rebases them onto the painted text itself, the
            // way every other note row does.
            buffer_spans: as_tuples
                .iter()
                .filter(|(s, e, _)| *e > start && *s < end)
                .map(|&(s, e, style)| (at(s.max(start)), at(e.min(end)), style))
                .collect(),
        });
        offset += line.len() + 1; // the newline the join put back
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    target: String,
    anchor: Option<String>,
}

/// The bodies this note's embeds resolve to, held across frames.
pub(crate) struct Cache {
    /// The vault's markdown notes, which the window already holds. Resolution is a
    /// table lookup against these; rebuilding the table per embed would cost more than
    /// reading the note.
    entries: Vec<VaultEntry>,
    /// The note holding the embeds, so the resolver's chain catches a self-embed.
    host: PathBuf,
    bodies: HashMap<Key, Body>,
    /// What the sources reported the last time round. When it has moved, every body
    /// is thrown out — which is how an embed of a note open in another tab follows
    /// that tab's unsaved edits (roadmap P4.0b ruling 16).
    stamp: u64,
}

/// Where an embedded note's text comes from: the window's copy if it has one, the
/// file otherwise.
///
/// The order is the whole point (roadmap P4.0b ruling 12). A note open in a tab with
/// unsaved edits is *in hand*; an embed of it in another tab shows what the user is
/// looking at, not what the disk remembers. Reading the file instead would show one
/// version of a note in one tab and another in the next.
pub(crate) struct OpenFirst<'a> {
    pub(crate) notes: &'a crate::note::Registry,
    pub(crate) vault: &'a Vault,
}

impl Sources for OpenFirst<'_> {
    fn text(&self, path: &Path) -> Option<String> {
        if let Some(note) = self.notes.get(path) {
            return Some(note.buffer.borrow().text().to_string());
        }
        self.vault.read(path).ok()
    }

    fn stamp(&self) -> u64 {
        self.notes.stamp()
    }
}

impl Cache {
    /// The first and last rows of `path`'s note among the bodies this frame holds.
    ///
    /// An entered embed can only edit what it shows, and this is what it shows. A
    /// body can hold rows from other notes as well — an embed inside the embed — so
    /// the answer is this note's first and last row, and the rows between them
    /// belong to whoever they came from.
    pub(crate) fn rows_of(&self, path: &Path) -> (Option<usize>, Option<usize>) {
        let mut first: Option<usize> = None;
        let mut last: Option<usize> = None;
        for body in self.bodies.values() {
            if let Body::Lines(rows) = body {
                for line in rows {
                    if let Some(origin) = line.origin.as_ref().filter(|o| o.path == path) {
                        first = Some(first.map_or(origin.row, |f| f.min(origin.row)));
                        last = Some(last.map_or(origin.row, |l| l.max(origin.row)));
                    }
                }
            }
        }
        (first, last)
    }

    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            host: PathBuf::new(),
            bodies: HashMap::new(),
            stamp: 0,
        }
    }

    /// The vault's note list, refreshed whenever the window re-indexes.
    pub(crate) fn set_entries(&mut self, entries: Vec<VaultEntry>) {
        if entries != self.entries {
            self.entries = entries;
            // A note that appeared or went away can change what a target resolves to,
            // so the bodies keyed by the old list are no longer answers.
            self.bodies.clear();
        }
    }

    /// Which note this editor is showing. Its embeds are resolved relative to it, and
    /// it is the first link in the chain that stops an embed loop.
    ///
    /// Changing it drops the bodies: the host is an input to every answer, not just a
    /// label. A tab that moves from `a.md` to `b.md` must not keep showing `b`'s
    /// self-embed expanded because `a` was on screen when it was resolved.
    pub(crate) fn set_host(&mut self, host: &Path) {
        if host != self.host {
            self.host = host.to_path_buf();
            self.bodies.clear();
        }
    }

    /// Drop every body. Called when the vault reports a change, which is the coarse
    /// answer described in the module comment.
    pub(crate) fn invalidate(&mut self) {
        self.bodies.clear();
    }

    /// The embeds in `text`, in buffer-row order, with the body each one contributes.
    ///
    /// `Body::Nothing` rows are left out: an attachment embed is the image renderer's
    /// row, and a target that resolves to no note stays as the text the author wrote.
    pub(crate) fn resolve(
        &mut self,
        vault: &Vault,
        text: &str,
        sources: &dyn Sources,
    ) -> Vec<(usize, Body)> {
        // Any open note's text moving — including one embedded here — retires every
        // body. The scan below re-fills only the embeds this host actually has.
        let stamp = sources.stamp();
        if stamp != self.stamp {
            self.bodies.clear();
        }
        self.stamp = stamp;
        let mut out = Vec::new();
        for (row, line) in text.lines().enumerate() {
            let Some(embed) = transclude::line_embed(line) else {
                continue;
            };
            let key = Key {
                target: embed.target.clone(),
                anchor: embed.anchor.clone(),
            };
            let body = self
                .bodies
                .entry(key)
                .or_insert_with(|| {
                    transclude::resolve_from(vault, &self.entries, &self.host, &embed, sources)
                })
                .clone();
            if body != Body::Nothing {
                out.push((row, body));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(lines: &[&str]) -> Vec<(String, Vec<(usize, usize, Style)>)> {
        paint_body(
            &lines.iter().map(|l| l.to_string()).collect::<Vec<_>>(),
            &[],
        )
        .into_iter()
        .map(|row| (row.text, row.spans))
        .collect()
    }

    #[test]
    fn an_embedded_heading_loses_its_hashes_like_a_live_one() {
        let rows = painted(&["# Gift", "body"]);
        assert_eq!(rows[0].0, "Gift", "the `#` and its space are hidden");
        assert!(
            rows[0]
                .1
                .iter()
                .any(|(s, _, _)| (*s..rows[0].0.len()).contains(s)),
            "the heading style still covers the painted word"
        );
    }

    #[test]
    fn a_painted_row_knows_its_heading_level_so_it_can_be_sized() {
        let rows = paint_body(
            &["## The middle", "body"]
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>(),
            &[],
        );
        assert_eq!(rows[0].heading, Some(2));
        assert_eq!(rows[1].heading, None);
    }

    #[test]
    fn an_embedded_emphasis_loses_its_markers_and_keeps_its_style() {
        let rows = painted(&["Some **bold** prose"]);
        assert_eq!(rows[0].0, "Some bold prose");
        let bold = rows[0]
            .1
            .iter()
            .find(|(_, _, style)| matches!(style, Style::Strong))
            .expect("a strong span survives the rebase");
        assert_eq!(&rows[0].0[bold.0..bold.1], "bold");
    }

    #[test]
    fn a_fence_opening_on_one_row_still_styles_the_next() {
        let rows = painted(&["```", "let x = 1;", "```"]);
        assert_eq!(
            rows[1].0, "let x = 1;",
            "the code line is painted as written"
        );
        assert!(
            rows[1].1.iter().any(|(s, e, style)| *s == 0
                && *e == rows[1].0.len()
                && matches!(style, Style::CodeBlock)),
            "the whole row is styled as code because the scan saw the fence above it"
        );
    }

    #[test]
    fn an_embedded_quote_is_curled_like_the_notes_own() {
        let rows = painted(&["it's here"]);
        assert_eq!(rows[0].0, "it\u{2019}s here");
    }

    #[test]
    fn the_spans_tile_the_painted_text_and_not_the_source() {
        // `build_runs` requires run lengths that add up to the painted text exactly,
        // so a span left in source coordinates would style the wrong characters.
        let rows = painted(&["# **Both**"]);
        let (text, spans) = &rows[0];
        assert_eq!(text, "Both");
        for (start, end, _) in spans {
            assert!(
                *start <= *end && *end <= text.len(),
                "span {start}..{end} lies inside the {text:?} that is painted"
            );
        }
    }
}
