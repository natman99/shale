//! The markdown editor view: a scrollable, line-based text surface driven by
//! `shale_core::buffer::Buffer` and the vim engine.

use std::collections::HashSet;
use std::ops::Range;

use gpui::{
    div, font, prelude::*, px, rgb, App, Bounds, Context, CursorStyle, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, IndentAdjustment, InteractiveElement, IntoElement,
    LineFragment, LineWrapperHandle, MouseButton, MouseDownEvent, ParentElement, Pixels, Render,
    ScrollHandle, SharedString, Styled, UTF16Selection, Window,
};
use shale_core::buffer::Buffer;
use shale_core::markdown::{extract_wikilinks, WikiLink};
use shale_core::mdstyle::{style_spans, Style, StyleSpan};
use shale_core::vim::{Key, Mode, Outcome, Vim};

use crate::acts::*;
use crate::line::{DisplayLine, HitLine, LineElement};
use crate::table_view;
use crate::theme;

// Re-exported so the headless tests can shape text at the same size the editor
// does.
pub(crate) use crate::theme::FONT_SIZE;

/// Events the editor emits for the workspace to handle.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// The user asked to follow the wiki-link to `target`.
    FollowLink { target: String },
}

/// How a buffer row will be presented, derived from its markdown style spans.
/// The wrap model has to measure a row with the face and size it will be painted
/// with, or the break points stop matching what the user sees.
#[derive(Clone, Copy, Default)]
struct RowStyle {
    /// The row is painted with a face wider than the regular one (a heading,
    /// strong or quoted text).
    wide: bool,
    /// The largest heading level on the row; 0 is body text.
    heading: u8,
    /// The row belongs to a fenced code block, which gets a full-width
    /// background.
    block: bool,
}

/// Which line wrapper a row needs: one per heading level, regular and wide face.
fn wrapper_index(row: RowStyle) -> usize {
    row.heading as usize * 2 + usize::from(row.wide)
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
    /// Cached wrap boundaries: (buffer change id, wrap width, per-row starts).
    wrap_cache: Option<(u64, Pixels, Vec<Vec<usize>>)>,
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

    /// The byte ranges of the display lines the buffer currently models.
    #[cfg(test)]
    pub fn display_ranges(&self) -> Vec<(usize, usize)> {
        (0..self.buffer.display_line_count())
            .filter_map(|i| self.buffer.display_range(i))
            .collect()
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

    /// Status-bar hint for a half-typed command, e.g. `3d`, `2` or `df`.
    pub fn pending_hint(&self) -> String {
        let count = self.vim.count().map(|c| c.to_string()).unwrap_or_default();
        let op = self
            .vim
            .pending_operator()
            .map(|c| c.to_string())
            .unwrap_or_default();
        let motion = self
            .vim
            .pending_motion()
            .map(|c| c.to_string())
            .unwrap_or_default();
        if count.is_empty() && op.is_empty() && motion.is_empty() {
            String::new()
        } else {
            format!(" {count}{op}{motion}")
        }
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

    /// The style spans covering `start..end`, re-based to line-relative byte
    /// offsets, in the order the scanner emitted them.
    fn line_spans(&self, start: usize, end: usize) -> Vec<(usize, usize, Style)> {
        let Some((_, spans)) = self.style_cache.as_ref() else {
            return Vec::new();
        };
        spans
            .iter()
            .filter(|s| s.start < end && s.end > start)
            .map(|s| {
                (
                    s.start.saturating_sub(start),
                    s.end.min(end).saturating_sub(start),
                    s.style,
                )
            })
            .collect()
    }

    /// Per-buffer-row presentation facts the wrap model needs, gathered from the
    /// style spans in one pass (spans and rows are both in offset order, so a
    /// single forward walk is enough).
    fn row_styles(&self) -> Vec<RowStyle> {
        let line_count = self.buffer.line_count();
        let mut rows = vec![RowStyle::default(); line_count];
        let Some((_, spans)) = self.style_cache.as_ref() else {
            return rows;
        };
        let mut row = 0usize;
        for span in spans {
            while row < line_count && self.buffer.line_end(row) <= span.start {
                row += 1;
            }
            let style = theme::RunStyle::for_style(span.style);
            let wide = style.is_wide();
            let heading = match span.style {
                Style::Heading { level } => level,
                Style::HeadingMarker => 1,
                _ => 0,
            };
            let block = matches!(span.style, Style::CodeBlock | Style::CodeFence);
            let mut r = row;
            while r < line_count && self.buffer.line_start(r) < span.end {
                let row = &mut rows[r];
                row.wide |= wide;
                row.heading = row.heading.max(heading);
                row.block |= block;
                r += 1;
            }
        }
        rows
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

    /// Build this frame's display lines: buffer rows split at the wrap points
    /// gpui computes for the current width. The exact same boundaries are handed
    /// to the buffer, so vim's `j`/`k` step over wrapped rows like neovim's and
    /// the buffer's display-line index space is identical to the painted one (if
    /// they ever disagreed the cursor would be drawn on the wrong row).
    /// A row is measured with the face and size it will be painted with —
    /// `wrappers` holds one line wrapper per (heading level, wide or not), which
    /// is what keeps break points matching the paint. Boundaries are cached until
    /// the text or the width changes.
    fn display_lines(
        &mut self,
        width: Pixels,
        wrappers: &mut [LineWrapperHandle],
        rows: &[RowStyle],
        table_rows: &[bool],
    ) -> Vec<DisplayLine> {
        let line_count = self.buffer.line_count();
        let change_id = self.buffer.change_id();
        let cached = self
            .wrap_cache
            .as_ref()
            .filter(|(id, w, _)| *id == change_id && *w == width)
            .map(|(_, _, segs)| segs.clone());

        let mut segments: Vec<Vec<usize>> = Vec::with_capacity(line_count);
        for row in 0..line_count {
            let row_start = self.buffer.line_start(row);
            let row_end = self.buffer.line_end(row);
            let mut starts = vec![row_start];
            // A table row is painted as one grid row and never soft-wraps, so
            // the buffer's display model must agree or j/k and the painted lines
            // fall out of step.
            if table_rows.get(row).copied().unwrap_or(false) {
                segments.push(starts);
                continue;
            }
            if let Some(cached) = cached.as_ref() {
                // Re-validate even a cached boundary against the current row, so
                // a cache key that ever fails to notice an edit cannot push a
                // segment past the end of the row.
                if let Some(segs) = cached.get(row) {
                    for &s in segs {
                        if s > row_start && s < row_end && s > *starts.last().unwrap() {
                            starts.push(s);
                        }
                    }
                }
            } else if width > px(0.) {
                let text = &self.buffer.text()[row_start..row_end];
                let row_style = rows.get(row).copied().unwrap_or_default();
                let wrapper = &mut wrappers[wrapper_index(row_style)];
                for boundary in wrapper.wrap_line(
                    &[LineFragment::text(text)],
                    width,
                    IndentAdjustment::NoIndent,
                ) {
                    let s = row_start + boundary.ix;
                    if s > row_start && s < row_end && s > *starts.last().unwrap() {
                        starts.push(s);
                    }
                }
            }
            // width == 0 means no measured width yet (the first frame): one
            // display line per row.
            segments.push(starts);
        }
        if cached.is_none() {
            self.wrap_cache = Some((change_id, width, segments.clone()));
        }

        self.buffer.set_display_segments(segments.clone());
        let mut out = Vec::new();
        for (row, starts) in segments.iter().enumerate() {
            let row_end = self.buffer.line_end(row);
            let size = theme::font_size(rows.get(row).copied().unwrap_or_default().heading);
            for (i, &start) in starts.iter().enumerate() {
                let end = starts.get(i + 1).copied().unwrap_or(row_end);
                out.push(DisplayLine {
                    row,
                    start,
                    end,
                    size,
                });
            }
        }
        out
    }

    fn normal(&mut self, key: Key, cx: &mut Context<Self>) {
        let before = self.buffer.text().to_string();
        let outcome = self.vim.handle_normal_key(key, &mut self.buffer);
        let changed = self.buffer.text() != before;
        if changed {
            self.mark_dirty();
        }
        match outcome {
            Outcome::EnterInsert => {
                self.vim.enter_insert();
                // Everything typed from here belongs to one undo step. When an
                // operator (`cw`, `o`) already changed the text it also already
                // snapshotted, so join that step instead of starting a new one.
                if changed {
                    self.buffer.continue_undo_group();
                } else {
                    self.buffer.begin_undo_group();
                }
            }
            Outcome::FollowLink => {
                let cursor = self.buffer.cursor();
                self.refresh_links();
                self.follow_link_at(cursor, cx);
            }
            Outcome::Stay | Outcome::Unhandled => {}
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn insert_edit(&mut self, inserted: &str, cx: &mut Context<Self>) {
        // So `.` can replay an insert session, the engine collects the text.
        self.vim.note_inserted(inserted);
        self.mark_dirty();
        self.ensure_cursor_visible();
        cx.notify();
    }

    // ----- action handlers ----------------------------------------------

    fn on_escape(&mut self, _: &VimEscape, _window: &mut Window, cx: &mut Context<Self>) {
        self.vim.enter_normal();
        self.buffer.clear_selection();
        self.buffer.end_undo_group();
        cx.notify();
    }

    fn on_newline(&mut self, _: &Newline, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() != Mode::Insert {
            return;
        }
        self.buffer.insert("\n");
        self.insert_edit("\n", cx);
    }

    fn on_next_line(&mut self, _: &NextLine, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() == Mode::Insert {
            return;
        }
        self.buffer.next_line_first_non_blank();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.backspace();
        self.insert_edit("", cx);
    }

    fn on_delete_forward(
        &mut self,
        _: &DeleteForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.delete_forward();
        self.insert_edit("", cx);
    }

    fn on_move_left(&mut self, _: &MoveLeft, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_left();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_move_right(&mut self, _: &MoveRight, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_right();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_up();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.move_down();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(undo_depth = self.buffer.undo_depth(), "undo action");
        self.buffer.undo();
        self.mark_dirty();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(redo_depth = self.buffer.redo_depth(), "redo action");
        self.buffer.redo();
        self.mark_dirty();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_copy(&mut self, _: &CopyText, _window: &mut Window, cx: &mut Context<Self>) {
        let range = match self.vim.mode() {
            Mode::Visual | Mode::VisualLine => self.vim.selection_range(&self.buffer),
            _ => self.buffer.selection().map(|(a, h)| (a.min(h), a.max(h))),
        };
        if let Some((a, h)) = range {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                self.buffer.text()[a..h].to_string(),
            ));
        }
    }

    fn on_paste(&mut self, _: &PasteText, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.buffer.insert(&text);
            self.insert_edit(&text, cx);
        }
    }

    fn on_click(&mut self, event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let y = event.position.y;
        let x = event.position.x;
        // Find the display line containing y (or the closest one above it).
        let mut best: Option<&HitLine> = None;
        for hit in &self.hit_lines {
            if hit.bounds.top() <= y && y < hit.bounds.bottom() {
                best = Some(hit);
                break;
            }
            if best.is_none() && hit.bounds.top() <= y {
                best = Some(hit);
            }
        }
        if let Some(hit) = best {
            let offset = hit.offset_for_x(x - hit.bounds.left());
            if event.modifiers.control {
                self.refresh_links();
                if self.link_at(offset).is_some() {
                    self.follow_link_at(offset, cx);
                    return;
                }
            }
            // A click in visual mode extends the selection from its anchor.
            match (self.vim.mode(), self.buffer.selection()) {
                (Mode::Visual | Mode::VisualLine, Some((anchor, _))) => {
                    self.buffer.set_selection(anchor, offset)
                }
                _ => self.buffer.set_cursor(offset),
            }
            cx.notify();
        }
    }

    // ----- utf16 <-> utf8 helpers (gpui input uses utf16 offsets) --------

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.buffer.text().chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.buffer.text().chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }
}

impl EntityInputHandler for MarkdownEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let start = self.offset_from_utf16(range_utf16.start);
        let end = self.offset_from_utf16(range_utf16.end);
        actual_range.replace(range_utf16);
        Some(self.buffer.text()[start..end].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let cursor = self.buffer.cursor();
        let range = match self.buffer.selection() {
            Some((a, h)) => (a, h),
            None => (cursor, cursor),
        };
        Some(UTF16Selection {
            range: self.offset_to_utf16(range.0)..self.offset_to_utf16(range.1),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Normal and visual mode swallow typed input, except for the literal
        // argument of a half-typed command (`fr`, `diw`): unbound characters
        // never become actions, they arrive here.
        if self.vim.mode() != Mode::Insert {
            if self.vim.waiting_for_literal() {
                if let Some(c) = new_text.chars().next() {
                    if new_text.chars().count() == 1 {
                        self.normal(Key::Char(c), cx);
                    }
                }
            }
            return;
        }
        if let Some(range) = range_utf16 {
            let start = self.offset_from_utf16(range.start);
            let end = self.offset_from_utf16(range.end);
            self.buffer.set_selection(start, end);
        }
        self.buffer.insert(new_text);
        self.insert_edit(new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range_utf16, new_text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let start = self.offset_from_utf16(range_utf16.start);
        let index = self.buffer.display_index_for(start);
        let hit = self.hit_lines.iter().find(|h| h.index == index)?;
        let x = hit.x_for_offset(start);
        Some(Bounds::from_corners(
            gpui::point(hit.bounds.left() + x, hit.bounds.top()),
            gpui::point(hit.bounds.left() + x + px(8.), hit.bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let hit = self
            .hit_lines
            .iter()
            .find(|h| h.bounds.top() <= point.y && point.y < h.bounds.bottom())?;
        let offset = hit.offset_for_x(point.x - hit.bounds.left());
        Some(self.offset_to_utf16(offset))
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Resolve the editor's font family once against the installed fonts.
        // gpui matches family names literally, so `font("monospace")` on Linux
        // matches nothing and its fallback stack returns a proportional UI font
        // that also ignores bold and italic; `pick_mono_family` picks a real
        // monospace family instead. Logged once because it decides what the user
        // sees and is the first thing to check when styling looks wrong.
        if !self.font_family_resolved {
            self.font_family_resolved = true;
            let text_system = window.text_system();
            let available = text_system.all_font_names();
            let requested = self.font_family.to_string();
            let chosen = theme::pick_mono_family(&available, &requested);
            if !available.iter().any(|f| f.eq_ignore_ascii_case(&chosen)) {
                tracing::warn!(
                    requested = requested.as_str(),
                    "no installed monospace font family; text will be drawn with gpui's fallback font",
                );
            } else {
                let regular = text_system.resolve_font(&font(chosen.clone()));
                let bold = text_system.resolve_font(&font(chosen.clone()).bold());
                tracing::debug!(
                    family = chosen.as_str(),
                    regular_char_width = ?text_system.layout_width(regular, px(FONT_SIZE), 'a'),
                    bold_char_width = ?text_system.layout_width(bold, px(FONT_SIZE), 'a'),
                    "editor font family",
                );
            }
            self.font_family = chosen.into();
        }
        self.hit_lines.clear();
        self.input_registered = false;
        let context = match self.vim.mode() {
            Mode::Insert => "vim-insert",
            Mode::Visual | Mode::VisualLine => "vim-visual",
            Mode::Normal => "vim-normal",
        };

        let line_count = self.buffer.line_count();
        let cursor = self.buffer.cursor();
        // In visual mode the buffer keeps (anchor, head) with the head *on* the
        // last selected character, so the painted range comes from the engine,
        // which knows whether the selection is charwise or linewise.
        let selection = match self.vim.mode() {
            Mode::Visual | Mode::VisualLine => self.vim.selection_range(&self.buffer),
            _ => self.buffer.selection(),
        };
        self.refresh_links();
        self.refresh_styles();
        tracing::trace!(lines = line_count, "editor render");

        // Soft wrap: the scroll container's width from the previous frame tells
        // us where each row breaks; the buffer gets the same boundaries so j/k
        // can step across wrapped rows. Rows that will be painted with a wide
        // face are measured with it, so their break points match the paint.
        let wrap_width = self.scroll_handle.bounds().size.width;
        let row_styles = self.row_styles();
        // Tables are painted as aligned grids instead of raw markdown: one plan
        // per buffer row, and a row that has one never soft-wraps, which the wrap
        // model has to be told about so it agrees with the paint.
        let buffer_tables = shale_core::table::tables(self.buffer.text());
        let plans = table_view::plans_by_row(self.buffer.text(), &buffer_tables, line_count);
        let table_rows: Vec<bool> = plans.iter().map(|plan| plan.is_some()).collect();
        // One character cell of the resolved face, which is what turns the grid's
        // character geometry into pixels.
        let char_width = {
            let text_system = window.text_system();
            let font_id = text_system.resolve_font(&font(self.font_family.clone()));
            text_system.layout_width(font_id, px(FONT_SIZE), 'a')
        };
        let display_lines = {
            let text_system = window.text_system();
            // One wrapper per (heading level, regular or wide face), so every
            // row is measured with the face and size it will be painted with.
            let family = self.font_family.clone();
            let mut wrappers: Vec<LineWrapperHandle> = Vec::with_capacity(theme::SIZES * 2);
            for level in 0..theme::SIZES as u8 {
                let size = px(theme::font_size(level));
                wrappers.push(text_system.line_wrapper(font(family.clone()), size));
                wrappers.push(text_system.line_wrapper(font(family.clone()).bold(), size));
            }
            self.display_lines(wrap_width, &mut wrappers, &row_styles, &table_rows)
        };
        let cursor_display = self.buffer.display_index_for(cursor);

        div()
            .key_context(context)
            .track_focus(&self.focus_handle)
            .size_full()
            .cursor(CursorStyle::IBeam)
            // Vim normal-mode keys.
            .on_action(cx.listener(|this, _: &VimH, _, cx| this.normal(Key::Char('h'), cx)))
            .on_action(cx.listener(|this, _: &VimJ, _, cx| this.normal(Key::Char('j'), cx)))
            .on_action(cx.listener(|this, _: &VimK, _, cx| this.normal(Key::Char('k'), cx)))
            .on_action(cx.listener(|this, _: &VimL, _, cx| this.normal(Key::Char('l'), cx)))
            .on_action(cx.listener(|this, _: &VimW, _, cx| this.normal(Key::Char('w'), cx)))
            .on_action(cx.listener(|this, _: &VimB, _, cx| this.normal(Key::Char('b'), cx)))
            .on_action(cx.listener(|this, _: &VimE, _, cx| this.normal(Key::Char('e'), cx)))
            .on_action(cx.listener(|this, _: &VimF, _, cx| this.normal(Key::Char('f'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperF, _, cx| this.normal(Key::Char('F'), cx)))
            .on_action(cx.listener(|this, _: &VimT, _, cx| this.normal(Key::Char('t'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperT, _, cx| this.normal(Key::Char('T'), cx)))
            .on_action(cx.listener(|this, _: &VimSpace, _, cx| this.normal(Key::Char(' '), cx)))
            .on_action(cx.listener(|this, _: &VimZero, _, cx| this.normal(Key::Char('0'), cx)))
            .on_action(cx.listener(|this, _: &VimCaret, _, cx| this.normal(Key::Char('^'), cx)))
            .on_action(cx.listener(|this, _: &VimDollar, _, cx| this.normal(Key::Char('$'), cx)))
            .on_action(cx.listener(|this, _: &VimG, _, cx| this.normal(Key::Char('g'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperG, _, cx| this.normal(Key::Char('G'), cx)))
            .on_action(cx.listener(|this, _: &VimX, _, cx| this.normal(Key::Char('x'), cx)))
            .on_action(cx.listener(|this, _: &VimD, _, cx| this.normal(Key::Char('d'), cx)))
            .on_action(cx.listener(|this, _: &VimC, _, cx| this.normal(Key::Char('c'), cx)))
            .on_action(cx.listener(|this, _: &VimY, _, cx| this.normal(Key::Char('y'), cx)))
            .on_action(cx.listener(|this, _: &VimP, _, cx| this.normal(Key::Char('p'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperP, _, cx| this.normal(Key::Char('P'), cx)))
            .on_action(cx.listener(|this, _: &VimU, _, cx| this.normal(Key::Char('u'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit1, _, cx| this.normal(Key::Char('1'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit2, _, cx| this.normal(Key::Char('2'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit3, _, cx| this.normal(Key::Char('3'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit4, _, cx| this.normal(Key::Char('4'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit5, _, cx| this.normal(Key::Char('5'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit6, _, cx| this.normal(Key::Char('6'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit7, _, cx| this.normal(Key::Char('7'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit8, _, cx| this.normal(Key::Char('8'), cx)))
            .on_action(cx.listener(|this, _: &VimDigit9, _, cx| this.normal(Key::Char('9'), cx)))
            .on_action(cx.listener(|this, _: &VimI, _, cx| this.normal(Key::Char('i'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperI, _, cx| this.normal(Key::Char('I'), cx)))
            .on_action(cx.listener(|this, _: &VimA, _, cx| this.normal(Key::Char('a'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperA, _, cx| this.normal(Key::Char('A'), cx)))
            .on_action(cx.listener(|this, _: &VimO, _, cx| this.normal(Key::Char('o'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperO, _, cx| this.normal(Key::Char('O'), cx)))
            .on_action(cx.listener(|this, _: &VimV, _, cx| this.normal(Key::Char('v'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperV, _, cx| this.normal(Key::Char('V'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperH, _, cx| this.normal(Key::Char('H'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperJ, _, cx| this.normal(Key::Char('J'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperK, _, cx| this.normal(Key::Char('K'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperL, _, cx| this.normal(Key::Char('L'), cx)))
            .on_action(cx.listener(|this, _: &VimUpperD, _, cx| this.normal(Key::Char('D'), cx)))
            .on_action(cx.listener(|this, _: &VimPipe, _, cx| this.normal(Key::Char('|'), cx)))
            .on_action(
                cx.listener(|this, _: &VimBackslash, _, cx| this.normal(Key::Char('\\'), cx)),
            )
            .on_action(cx.listener(|this, _: &VimDot, _, cx| this.normal(Key::Char('.'), cx)))
            // Shared editing keys.
            .on_action(cx.listener(Self::on_escape))
            .on_action(cx.listener(Self::on_newline))
            .on_action(cx.listener(Self::on_next_line))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete_forward))
            .on_action(cx.listener(Self::on_move_left))
            .on_action(cx.listener(Self::on_move_right))
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_paste))
            .child(
                div()
                    .id("editor-scroll")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.scroll_handle)
                    .font_family(self.font_family.clone())
                    .text_size(px(FONT_SIZE))
                    .line_height(px(theme::LINE_HEIGHT))
                    .bg(rgb(theme::BG))
                    .text_color(rgb(theme::FG))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_click))
                    .children(display_lines.iter().enumerate().map(|(index, dl)| {
                        let text: SharedString =
                            self.buffer.text()[dl.start..dl.end].to_string().into();

                        // Selection restricted to this display line.
                        let line_sel = selection.and_then(|(a, h)| {
                            let (lo, hi) = (a.min(h), a.max(h));
                            if hi <= dl.start || lo >= dl.end {
                                return None;
                            }
                            let s = lo.saturating_sub(dl.start);
                            let e = hi.min(dl.end).saturating_sub(dl.start);
                            Some((s, e))
                        });
                        // Cursor on this display line (insert mode draws a full
                        // bar; normal mode highlights the char under it).
                        let cursor_col = if cursor_display == index {
                            Some(cursor - dl.start)
                        } else {
                            None
                        };

                        // Wiki-link spans restricted to this display line, in
                        // line-relative byte offsets, with whether they
                        // resolve to an existing note.
                        let line_links: Vec<(usize, usize, bool)> = self
                            .links
                            .iter()
                            .filter(|l| l.start < dl.end && l.end > dl.start)
                            .map(|l| {
                                (
                                    l.start.saturating_sub(dl.start),
                                    l.end.min(dl.end).saturating_sub(dl.start),
                                    self.is_known(&l.target),
                                )
                            })
                            .collect();

                        // Markdown style spans restricted to this display line,
                        // in line-relative byte offsets.
                        let line_spans = self.line_spans(dl.start, dl.end);
                        // A fenced block's background spans the full width, so
                        // the element paints it as a quad rather than a run.
                        let block = row_styles.get(dl.row).copied().unwrap_or_default().block;

                        div()
                            .id(("line", index))
                            .w_full()
                            .h(px(theme::line_height(dl.size)))
                            .child(LineElement {
                                editor: cx.entity(),
                                row: dl.row,
                                index,
                                start: dl.start,
                                end: dl.end,
                                text,
                                line_sel,
                                cursor_col,
                                insert: self.vim.mode() == Mode::Insert,
                                links: line_links,
                                spans: line_spans,
                                size: dl.size,
                                block,
                                table: plans.get(dl.row).cloned().flatten(),
                                char_width,
                            })
                    })),
            )
    }
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
