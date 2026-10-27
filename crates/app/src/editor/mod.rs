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

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, Pixels, ScrollHandle, SharedString,
};
use shale_core::buffer::Buffer;
use shale_core::markdown::{extract_wikilinks, WikiLink};
use shale_core::mdstyle::{style_spans, Style, StyleSpan};
use shale_core::vim::{Mode, Vim};

use crate::line::HitLine;

use self::wrap::CachedRow;

// Re-exported so the headless tests can shape text at the same size the editor
// does.
pub(crate) use crate::theme::FONT_SIZE;

/// Events the editor emits for the workspace to handle.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// The user asked to follow the wiki-link to `target`.
    FollowLink { target: String },
}

impl EventEmitter<EditorEvent> for MarkdownEditor {}

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
            wrap_cache: None,
            style_cache: None,
            links: Vec::new(),
            known: HashSet::new(),
            input_registered: false,
            font_family: "monospace".into(),
            font_family_resolved: false,
            dirty: false,
        }
    }

    /// Tell the editor which note names exist, so it can style resolved
    /// links differently from unresolved ones.
    pub fn set_known_links(&mut self, known: HashSet<String>, cx: &mut Context<Self>) {
        self.known = known;
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

    pub fn text(&self) -> String {
        self.buffer.text().to_string()
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
        tracing::debug!(lines = self.buffer.line_count(), "editor set_text");
        cx.notify();
    }

    pub fn mode(&self) -> Mode {
        self.vim.mode()
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

    /// The buffer offset a click at `x` on display line `index` lands on.
    #[cfg(test)]
    pub fn offset_at_x(&self, index: usize, x: Pixels) -> Option<usize> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| hit.offset_for_x(x))
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

    fn mark_dirty(&mut self) {
        self.dirty = true;
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
