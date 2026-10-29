//! The markdown editor view: a scrollable, line-based text surface driven by
//! `shale_core::buffer::Buffer` and the vim engine.
//!
//! The view is split by what a frame does rather than by data type, and these
//! submodules are all part of the same `MarkdownEditor`, so they can reach its
//! private state:
//! - [`wrap`]: the display model — hidden markers, visible pieces, soft-wrap
//!   break points
//! - [`render`]: building the frame and wiring the vim key context
//! - [`actions`]: the handlers the key context binds
//! - [`input`]: gpui's input surface (IME, paste, clicks)

mod actions;
mod input;
mod render;
mod wrap;

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, Pixels, ScrollHandle, SharedString,
};
use shale_core::buffer::Buffer;
use shale_core::markdown::{extract_wikilinks, WikiLink};
use shale_core::mdstyle::{style_spans, Style, StyleSpan};
use shale_core::vault::VaultEntry;
use shale_core::vim::{Mode, Vim};
use shale_core::{theme::Palette, Vault};

use crate::line::HitLine;

use self::wrap::CachedRow;

// Re-exported so the headless tests can shape text at the same size the editor
// does when a vault asks for none.
pub(crate) use crate::theme::DEFAULT_FONT_SIZE;

/// Events the editor emits for the workspace to handle.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// The user asked to follow the wiki-link to `target`.
    FollowLink { target: String },
    /// The note's text changed. The workspace uses this to keep the spell and
    /// grammar checker's copy of the document in step with the buffer.
    Changed,
    /// The pointer moved over the note, landing on buffer offset `offset` of
    /// display line `line`, at the window position `at`.
    Hovered {
        offset: usize,
        line: usize,
        at: gpui::Point<Pixels>,
    },
}

impl EventEmitter<EditorEvent> for MarkdownEditor {}

/// What one display line is told to draw for `/`: every match that overlaps it,
/// and the current match if that overlaps too. Buffer byte ranges, clipped to the
/// line.
type LineHits = (Vec<(usize, usize)>, Option<(usize, usize)>);

pub struct MarkdownEditor {
    /// The editor's focus handle; the line element needs it to register input
    /// during paint, so it is crate-visible.
    pub(crate) focus_handle: FocusHandle,
    buffer: Buffer,
    vim: Vim,
    scroll_handle: ScrollHandle,
    /// Display lines captured during the last paint, for hit-testing. Written
    /// by the line element during paint, so it is crate-visible.
    pub(crate) hit_lines: Vec<HitLine>,
    /// Cached wrap boundaries: (buffer change id, wrap width, per-row starts and
    /// the live-preview state they were measured in).
    wrap_cache: Option<(u64, Pixels, Vec<CachedRow>)>,
    /// Markdown style spans for the current text (buffer byte offsets), cached
    /// until the buffer changes.
    style_cache: Option<(u64, Vec<StyleSpan>)>,
    /// Wiki-link spans in the current buffer text (buffer byte offsets).
    links: Vec<WikiLink>,
    /// The vault the note lives in, which an image embed's target is resolved
    /// against. `None` until the workspace supplies one, which is how the
    /// headless tests run without a vault (and without images).
    vault: Option<Rc<Vault>>,
    /// The vault's attachment files (everything that is not a note), for
    /// resolving `![[image.png]]`.
    attachments: Vec<VaultEntry>,
    /// Lowercased note names present in the vault, for resolved/unresolved
    /// link styling.
    known: HashSet<String>,
    /// Whether the input handler was registered during the current frame's
    /// paint. gpui only routes typed characters to a handler registered while
    /// painting the frame, so exactly one line element does it per frame.
    pub(crate) input_registered: bool,
    /// The family the editor is drawn with: what the config asked for until the
    /// first frame resolves it against the installed fonts.
    font_family: SharedString,
    /// Whether `font_family` has been resolved against the system's fonts.
    font_family_resolved: bool,
    /// The colours everything is drawn with, from the vault's theme. Shared so a
    /// theme change is one assignment rather than a rebuild of the view.
    palette: Rc<Palette>,
    /// Body text size in pixels; headings are scaled from it.
    font_size: f32,
    /// (start, end) buffer byte ranges the spell and grammar checker has
    /// something to say about, drawn as wavy underlines. The workspace owns the
    /// checker and pushes these; the editor only paints them.
    marks: Vec<(usize, usize)>,
    /// (start, end) buffer byte ranges the `/` search matched, drawn with a
    /// background. Like [`Self::marks`] these are pushed by the workspace, which
    /// owns the query and decides which match the cursor is on; the editor only
    /// paints them.
    hits: Vec<(usize, usize)>,
    /// The one match the cursor is on, drawn over the others so the current
    /// position in a run of `n`/`N` is visible. `None` when there is no match.
    active_hit: Option<(usize, usize)>,
    dirty: bool,
}

impl MarkdownEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            buffer: Buffer::default(),
            vim: Vim::new(),
            scroll_handle: ScrollHandle::default(),
            hit_lines: Vec::new(),
            marks: Vec::new(),
            hits: Vec::new(),
            active_hit: None,
            wrap_cache: None,
            style_cache: None,
            links: Vec::new(),
            vault: None,
            attachments: Vec::new(),
            known: HashSet::new(),
            input_registered: false,
            font_family: "monospace".into(),
            font_family_resolved: false,
            // Until the vault's `appearance.theme` is applied, the default
            // palette, so a frame painted before then is not a surprise.
            palette: Rc::new(Palette::DEFAULT),
            font_size: DEFAULT_FONT_SIZE,
            dirty: false,
        }
    }

    /// Tell the editor which note names exist, so it can style resolved
    /// links differently from unresolved ones.
    pub fn set_known_links(&mut self, known: HashSet<String>, cx: &mut Context<Self>) {
        self.known = known;
        cx.notify();
    }

    /// Tell the editor which vault the note came from and what attachments it
    /// holds, so a row that is only an image embed can be painted as the picture.
    pub fn set_vault(
        &mut self,
        vault: Rc<Vault>,
        attachments: Vec<VaultEntry>,
        cx: &mut Context<Self>,
    ) {
        self.vault = Some(vault);
        self.attachments = attachments;
        cx.notify();
    }

    /// The font family from the config. Empty means "choose one"; an unknown
    /// name falls back to the preferred installed monospace families.
    pub fn set_font_family(&mut self, family: &str, cx: &mut Context<Self>) {
        if family.is_empty() || self.font_family_resolved {
            return;
        }
        self.font_family = family.into();
        cx.notify();
    }

    /// The palette the editor and its line elements draw with.
    pub fn set_palette(&mut self, palette: Rc<Palette>, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    /// The body text size in pixels, from `appearance.font_size`. Headings and
    /// line heights are scaled from it, and the soft-wrap model measures with
    /// it, so a frame that changed it has to re-measure every row.
    pub fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        if size <= 0. || (size - self.font_size).abs() < f32::EPSILON {
            return;
        }
        self.font_size = size;
        cx.notify();
    }

    pub fn text(&self) -> String {
        self.buffer.text().to_string()
    }

    /// The body text size the editor paints and measures with.
    pub(crate) fn font_size(&self) -> f32 {
        self.font_size
    }

    /// The font family the editor resolves for painting, which the line element
    /// needs to shape text with the same face the wrappers measured.
    pub(crate) fn font_family(&self) -> SharedString {
        self.font_family.clone()
    }

    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.buffer.set_text(text);
        self.vim.enter_normal();
        self.dirty = false;
        // A different document, so the old note's underlines and matches are
        // meaningless.
        self.marks.clear();
        self.hits.clear();
        self.active_hit = None;
        tracing::debug!(lines = self.buffer.line_count(), "editor set_text");
        cx.notify();
    }

    /// The ranges to draw as the checker's notes, as buffer byte offsets. Empty
    /// clears them.
    pub(crate) fn set_marks(&mut self, marks: Vec<(usize, usize)>, cx: &mut Context<Self>) {
        if marks != self.marks {
            self.marks = marks;
            cx.notify();
        }
    }

    /// The checker's ranges the editor is currently drawing.
    #[cfg(test)]
    pub(crate) fn marks(&self) -> &[(usize, usize)] {
        &self.marks
    }

    /// The `/` matches to draw, and which one is current. Empty hits and `None`
    /// clear them, which is what `escape` does.
    pub(crate) fn set_hits(
        &mut self,
        hits: Vec<(usize, usize)>,
        active: Option<(usize, usize)>,
        cx: &mut Context<Self>,
    ) {
        if hits != self.hits || active != self.active_hit {
            self.hits = hits;
            self.active_hit = active;
            cx.notify();
        }
    }

    /// The matches the editor is currently drawing.
    #[cfg(test)]
    pub(crate) fn hits(&self) -> &[(usize, usize)] {
        self.hits.as_slice()
    }

    /// The current match the editor is drawing, if there is one.
    #[cfg(test)]
    pub(crate) fn active_hit(&self) -> Option<(usize, usize)> {
        self.active_hit
    }

    pub fn mode(&self) -> Mode {
        self.vim.mode()
    }

    /// Put the cursor on buffer row `row` (0-based, clamped) and scroll it into
    /// view. What vault search does with a hit; the cursor lands at the row's
    /// first character, which is where a reader wants it, and the vim engine is
    /// left in normal mode so the next keystroke is a motion.
    pub fn reveal_line(&mut self, row: usize, cx: &mut Context<Self>) {
        self.buffer.goto_line(row);
        self.ensure_cursor_visible();
        cx.notify();
    }

    /// Put the cursor on the buffer byte `offset` and scroll it into view. What
    /// `/` and `n`/`N` do with a match: the cursor lands on the match's first
    /// byte, which is where the reader wants to be, and the vim engine is left
    /// alone, so the next keystroke is whatever mode the user was in.
    pub fn reveal_offset(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.buffer.set_cursor(offset);
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Cursor position in buffer coordinates (row, byte column), 0-based.
    #[cfg(test)]
    pub fn cursor_position(&self) -> (usize, usize) {
        self.buffer.row_col(self.buffer.cursor())
    }

    /// Buffer rows and wrapped display lines, for tests and diagnostics.
    #[cfg(test)]
    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    #[cfg(test)]
    pub fn display_line_count(&self) -> usize {
        self.buffer.display_line_count()
    }

    /// The width soft wrap was computed against: the scroll container's bounds
    /// from the previous frame.
    #[cfg(test)]
    pub fn wrap_width(&self) -> Pixels {
        self.scroll_handle.bounds().size.width
    }

    /// The shaped lines produced by the last paint, with the byte range each one
    /// covers. Diagnostic aid: compares what is actually painted against the wrap
    /// model (a different painted font would show up here as a width mismatch).
    #[cfg(test)]
    pub fn painted_lines(&self) -> Vec<(usize, usize, Pixels)> {
        self.hit_lines
            .iter()
            .map(|hit| (hit.start, hit.end, hit.width))
            .collect()
    }

    /// How many painted pieces each display line has: one for ordinary text, one
    /// per cell for a table row. Diagnostic aid for the grid paint path.
    #[cfg(test)]
    pub fn painted_segments(&self) -> Vec<usize> {
        self.hit_lines
            .iter()
            .map(|hit| hit.segments.len())
            .collect()
    }

    /// The text each display line was painted with: the buffer's text with
    /// live-preview markers left out. This is how the preview tests see what the
    /// user would see.
    #[cfg(test)]
    pub fn painted_text(&self) -> Vec<String> {
        self.painted_pieces()
            .into_iter()
            .map(|pieces| pieces.concat())
            .collect()
    }

    /// The painted text of every display line, one entry per painted piece: one
    /// for an ordinary line, one per cell for a table row.
    #[cfg(test)]
    pub fn painted_pieces(&self) -> Vec<Vec<String>> {
        self.hit_lines
            .iter()
            .map(|hit| {
                hit.segments
                    .iter()
                    .map(|segment| segment.line.text.to_string())
                    .collect()
            })
            .collect()
    }

    /// The byte ranges of the display lines the buffer currently models.
    #[cfg(test)]
    pub fn display_ranges(&self) -> Vec<(usize, usize)> {
        (0..self.buffer.display_line_count())
            .filter_map(|i| self.buffer.display_range(i))
            .collect()
    }

    /// Where a buffer offset is drawn on display line `index`, measured from that
    /// line's left edge. Test aid for the live-preview piece mapping.
    #[cfg(test)]
    pub fn x_for_offset(&self, index: usize, offset: usize) -> Option<Pixels> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| hit.x_for_offset(offset))
    }

    /// The buffer offset a pointer at `x` on display line `index` lands on, using
    /// the text that was painted there.
    #[cfg(test)]
    pub fn offset_at_x(&self, index: usize, x: Pixels) -> Option<usize> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| hit.offset_for_x(x))
    }

    /// The buffer offset a pointer at the **window** x `x` lands on, on display
    /// line `index`.
    ///
    /// The line's text does not start at the window's left edge — the editor has
    /// margins, and a table row starts further in again — so the pointer is
    /// measured from where the line was painted. [`Self::offset_at_x`] takes the
    /// other coordinate space, measured from the line itself.
    pub fn offset_at_pointer_x(&self, index: usize, x: Pixels) -> Option<usize> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| hit.offset_for_x(x - hit.bounds.left()))
    }

    /// The height each display line was painted at, which follows its text size
    /// (headings are taller).
    #[cfg(test)]
    pub fn painted_heights(&self) -> Vec<Pixels> {
        self.hit_lines
            .iter()
            .map(|hit| hit.bounds.size.height)
            .collect()
    }

    /// The display lines that were painted as a thematic break's rule rather
    /// than as text.
    #[cfg(test)]
    pub fn painted_rules(&self) -> Vec<usize> {
        self.hit_lines
            .iter()
            .enumerate()
            .filter(|(_, hit)| hit.rule)
            .map(|(index, _)| index)
            .collect()
    }

    /// The display lines painted inside a callout's box (background and accent
    /// bar), which is how the paint tests check that the box follows the block.
    #[cfg(test)]
    pub fn painted_callouts(&self) -> Vec<usize> {
        self.hit_lines
            .iter()
            .enumerate()
            .filter(|(_, hit)| hit.callout)
            .map(|(index, _)| index)
            .collect()
    }

    /// The fold markers the last paint drew, as (display line, label). A marker
    /// is not buffer text, so it is not part of `painted_text`; this is how the
    /// paint tests see it.
    #[cfg(test)]
    pub fn painted_fold_markers(&self) -> Vec<(usize, String)> {
        self.hit_lines
            .iter()
            .filter_map(|hit| {
                hit.fold_marker
                    .as_ref()
                    .map(|marker| (hit.index, marker.line.text.to_string()))
            })
            .collect()
    }

    /// Where a line's fold marker was painted, as (display line, first x, last
    /// x) in window coordinates: what a click has to land on to toggle the fold.
    #[cfg(test)]
    pub fn fold_marker_bounds(&self) -> Vec<(usize, Pixels, Pixels)> {
        self.hit_lines
            .iter()
            .filter_map(|hit| {
                hit.fold_marker.as_ref().map(|marker| {
                    let left = hit.bounds.left() + marker.x;
                    (hit.index, left, left + marker.line.width())
                })
            })
            .collect()
    }

    /// The top and bottom y of display line `index` as it was painted, in window
    /// coordinates — what a test needs in order to click on that line.
    #[cfg(test)]
    pub fn hit_line_edges(&self, index: usize) -> Option<(Pixels, Pixels)> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| (hit.bounds.top(), hit.bounds.bottom()))
    }

    /// The left and right x of display line `index` as it was painted, in window
    /// coordinates: where the text actually starts, which is what the editor's
    /// margins are measured against.
    #[cfg(test)]
    pub fn hit_line_x(&self, index: usize) -> Option<(Pixels, Pixels)> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| (hit.bounds.left(), hit.bounds.right()))
    }

    /// Where display line `index` was painted, in window coordinates. The window
    /// places the popup just below it, and asks again each frame, which is what
    /// lets a popup follow the note as it scrolls.
    pub fn hit_line_bounds(&self, index: usize) -> Option<gpui::Bounds<Pixels>> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| hit.bounds)
    }

    /// Whether display line `index` is inside the scroll container's viewport as
    /// it was painted.
    ///
    /// Every line is laid out, scrolled away or not, so the window asks this
    /// before drawing a panel over a line: a panel over a word that is not on
    /// screen would float over the sidebar or the status bar instead.
    pub fn line_is_visible(&self, index: usize) -> bool {
        let Some(hit) = self.hit_lines.iter().find(|hit| hit.index == index) else {
            return false;
        };
        let view = self.scroll_handle.bounds();
        hit.bounds.top() < view.bottom() && hit.bounds.bottom() > view.top()
    }

    /// Where the cursor is, as a buffer byte offset.
    pub fn cursor_offset(&self) -> usize {
        self.buffer.cursor()
    }

    /// Whether the editor is in normal mode.
    ///
    /// The window asks because the pointer may only open a popup in normal mode:
    /// in insert, visual, or the middle of a vim command the keyboard is busy with
    /// something the user is doing, and taking it away would be worse than saying
    /// nothing.
    pub fn is_normal_mode(&self) -> bool {
        self.vim.mode() == Mode::Normal
            && self.vim.count().is_none()
            && self.vim.pending_operator().is_none()
    }

    /// Rewrite the note at the given byte ranges, last one first so the earlier
    /// offsets stay valid, as one undo step. Returns whether anything was done.
    ///
    /// This is how a checker's fix reaches the text: the window decides *which*
    /// ranges and what replaces them, and the editor keeps the single-owner rule
    /// over the buffer. The note is left dirty, so the checker is told about the
    /// new text by the usual path.
    pub fn apply_edits(
        &mut self,
        edits: &[(usize, usize, String)],
        cx: &mut Context<Self>,
    ) -> bool {
        if edits.is_empty() {
            return false;
        }
        let mut edits = edits.to_vec();
        edits.sort_by_key(|(start, _, _)| *start);
        self.buffer.begin_undo_group();
        for (start, end, text) in edits.iter().rev() {
            self.buffer.set_selection(*start, *end);
            self.buffer.insert(text);
        }
        self.buffer.end_undo_group();
        self.mark_dirty(cx);
        cx.notify();
        true
    }

    /// Cursor position in display coordinates (wrapped segments counted as
    /// lines, column counted in characters from the segment start), like a vim
    /// ruler.
    pub fn display_position(&self) -> (usize, usize) {
        let cursor = self.buffer.cursor();
        (
            self.buffer.display_index_for(cursor),
            self.buffer.display_col(cursor),
        )
    }

    fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        // gpui queues an emit rather than running the handlers inside this
        // update, so the workspace is free to read the buffer back from its
        // handler.
        cx.emit(EditorEvent::Changed);
    }

    // ----- wiki links ---------------------------------------------------

    /// Recompute wiki-link spans for the current buffer text. Called before
    /// any link lookup or paint, since the buffer may have changed since the
    /// last call.
    fn refresh_links(&mut self) {
        self.links = extract_wikilinks(self.buffer.text());
    }

    // ----- markdown styling ---------------------------------------------

    /// Recompute the markdown style spans unless the buffer changed since the
    /// last paint. Called once per frame, before wrapping and painting.
    fn refresh_styles(&mut self) {
        let change_id = self.buffer.change_id();
        if self
            .style_cache
            .as_ref()
            .is_some_and(|(id, _)| *id == change_id)
        {
            return;
        }
        let spans = style_spans(self.buffer.text());
        self.style_cache = Some((change_id, spans));
    }

    /// The style spans overlapping `start..end`, clipped to it as absolute
    /// buffer offsets, in the order the scanner emitted them.
    fn spans_in(&self, start: usize, end: usize) -> Vec<(usize, usize, Style)> {
        let Some((_, spans)) = self.style_cache.as_ref() else {
            return Vec::new();
        };
        spans
            .iter()
            .filter(|s| s.start < end && s.end > start)
            .map(|s| (s.start.max(start), s.end.min(end), s.style))
            .collect()
    }

    /// The checker's ranges that overlap `start..end`, clipped to it. Like
    /// [`Self::spans_in`] these are buffer byte offsets; the line element rebases
    /// them onto the text it actually paints.
    fn marks_in(&self, start: usize, end: usize) -> Vec<(usize, usize)> {
        self.marks
            .iter()
            .filter(|(s, e)| *s < end && *e > start)
            .map(|&(s, e)| (s.max(start), e.min(end)))
            .collect()
    }

    /// The `/` matches that overlap `start..end`, clipped to it, and the current
    /// match clipped the same way (`None` when there is none or it is on another
    /// line). Buffer byte offsets, rebased by the line element like the marks.
    fn hits_in(&self, start: usize, end: usize) -> LineHits {
        let hits: Vec<(usize, usize)> = self
            .hits
            .iter()
            .filter(|(s, e)| *s < end && *e > start)
            .map(|&(s, e)| (s.max(start), e.min(end)))
            .collect();
        let active = self
            .active_hit
            .filter(|(s, e)| *s < end && *e > start)
            .map(|(s, e)| (s.max(start), e.min(end)));
        (hits, active)
    }

    /// Whether a link target names a note that exists in the vault.
    fn is_known(&self, target: &str) -> bool {
        let t = target.trim().to_lowercase();
        if self.known.contains(&t) {
            return true;
        }
        match t.strip_suffix(".md") {
            Some(stripped) => self.known.contains(stripped),
            None => false,
        }
    }

    /// The wiki-link containing `offset`, if any.
    fn link_at(&self, offset: usize) -> Option<&WikiLink> {
        self.links
            .iter()
            .find(|l| l.start <= offset && offset < l.end)
    }

    fn follow_link_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        if let Some(link) = self.link_at(offset) {
            cx.emit(EditorEvent::FollowLink {
                target: link.target.clone(),
            });
        } else {
            tracing::debug!(offset, "no wiki-link at cursor");
        }
    }

    fn ensure_cursor_visible(&self) {
        let index = self.buffer.display_index_for(self.buffer.cursor());
        self.scroll_handle.scroll_to_item(index);
    }
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
