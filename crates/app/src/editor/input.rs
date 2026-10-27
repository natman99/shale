//! gpui's input surface for the editor: what the IME, paste and select-all see
//! (gpui works in utf-16 offsets, the buffer in utf-8 bytes), and the mouse
//! click that positions the cursor through the painted lines' hit records.

use std::ops::Range;

use gpui::{
    px, Bounds, Context, EntityInputHandler, MouseDownEvent, Pixels, UTF16Selection, Window,
};
use shale_core::vim::{Key, Mode};

use crate::line::HitLine;

use super::MarkdownEditor;

impl MarkdownEditor {
    pub(super) fn on_click(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
