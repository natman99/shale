//! The markdown editor view: a scrollable, line-based text surface driven by
//! `shale_core::buffer::Buffer` and the vim engine.

use std::collections::HashSet;
use std::ops::Range;

use gpui::{
    div, fill, prelude::*, px, relative, rgb, App, Bounds, Context, CursorStyle, Element,
    ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, GlobalElementId, InspectorElementId, InteractiveElement, IntoElement, LayoutId,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Render, ScrollHandle, ShapedLine,
    SharedString, Style, Styled, TextRun, UTF16Selection, Window,
};
use shale_core::buffer::Buffer;
use shale_core::markdown::{extract_wikilinks, WikiLink};
use shale_core::vim::{Key, Mode, Outcome, Vim};

use crate::acts::*;

const LINE_HEIGHT: f32 = 20.0;
const FONT_SIZE: f32 = 14.0;

const BG: u32 = 0x1e1e1e;
const FG: u32 = 0xd4d4d4;
const CURSOR: u32 = 0xaeafad;
const SELECTION: u32 = 0x264f78;
const LINK: u32 = 0x7aa6f0;
const LINK_UNRESOLVED: u32 = 0x7d6f6f;

/// Events the editor emits for the workspace to handle.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// The user asked to follow the wiki-link to `target`.
    FollowLink { target: String },
}

impl EventEmitter<EditorEvent> for MarkdownEditor {}

pub struct MarkdownEditor {
    focus_handle: FocusHandle,
    buffer: Buffer,
    vim: Vim,
    scroll_handle: ScrollHandle,
    /// Shaped lines from the last paint, with their bounds, for hit-testing.
    hit_lines: Vec<(usize, ShapedLine, Bounds<Pixels>)>,
    /// Wiki-link spans in the current buffer text (buffer byte offsets).
    links: Vec<WikiLink>,
    /// Lowercased note names present in the vault, for resolved/unresolved
    /// link styling.
    known: HashSet<String>,
    /// Whether the input handler was registered during the current frame's
    /// paint. gpui only routes typed characters to a handler registered while
    /// painting the frame, so exactly one line element does it per frame.
    input_registered: bool,
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
            links: Vec::new(),
            known: HashSet::new(),
            input_registered: false,
            dirty: false,
        }
    }

    /// Tell the editor which note names exist, so it can style resolved
    /// links differently from unresolved ones.
    pub fn set_known_links(&mut self, known: HashSet<String>, cx: &mut Context<Self>) {
        self.known = known;
        cx.notify();
    }

    pub fn text(&self) -> String {
        self.buffer.text().to_string()
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

    pub fn cursor_position(&self) -> (usize, usize) {
        self.buffer.row_col(self.buffer.cursor())
    }

    /// Status-bar hint for a half-typed command, e.g. `3d` or `2`.
    pub fn pending_hint(&self) -> String {
        let count = self.vim.count().map(|c| c.to_string()).unwrap_or_default();
        let op = self
            .vim
            .pending_operator()
            .map(|c| c.to_string())
            .unwrap_or_default();
        if count.is_empty() && op.is_empty() {
            String::new()
        } else {
            format!(" {count}{op}")
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
        let (row, _) = self.buffer.row_col(self.buffer.cursor());
        self.scroll_handle.scroll_to_item(row);
    }

    fn normal(&mut self, key: Key, cx: &mut Context<Self>) {
        let before = self.buffer.text().to_string();
        let outcome = self.vim.handle_normal_key(key, &mut self.buffer);
        if self.buffer.text() != before {
            self.mark_dirty();
        }
        match outcome {
            Outcome::EnterInsert => self.vim.enter_insert(),
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

    fn insert_edit(&mut self, cx: &mut Context<Self>) {
        self.mark_dirty();
        self.ensure_cursor_visible();
        cx.notify();
    }

    // ----- action handlers ----------------------------------------------

    fn on_escape(&mut self, _: &VimEscape, _window: &mut Window, cx: &mut Context<Self>) {
        self.vim.enter_normal();
        cx.notify();
    }

    fn on_newline(&mut self, _: &Newline, _window: &mut Window, cx: &mut Context<Self>) {
        if self.vim.mode() == Mode::Normal {
            return;
        }
        self.buffer.insert("\n");
        self.insert_edit(cx);
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
        self.insert_edit(cx);
    }

    fn on_delete_forward(
        &mut self,
        _: &DeleteForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.delete_forward();
        self.insert_edit(cx);
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
        self.buffer.undo();
        self.mark_dirty();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.redo();
        self.mark_dirty();
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_copy(&mut self, _: &CopyText, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((a, h)) = self.buffer.selection() {
            let text = self.buffer.text()[a.min(h)..a.max(h)].to_string();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    }

    fn on_paste(&mut self, _: &PasteText, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.buffer.insert(&text);
            self.insert_edit(cx);
        }
    }

    fn on_click(&mut self, event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let y = event.position.y;
        let x = event.position.x;
        // Find the hit line containing y (or the closest one).
        let mut best: Option<(usize, &ShapedLine, Bounds<Pixels>)> = None;
        for (row, line, bounds) in &self.hit_lines {
            if bounds.top() <= y && y < bounds.bottom() {
                best = Some((*row, line, *bounds));
                break;
            }
            if best.is_none() && bounds.top() <= y {
                best = Some((*row, line, *bounds));
            }
        }
        if let Some((row, line, bounds)) = best {
            let col = line.closest_index_for_x(x - bounds.left());
            let offset = self.buffer.offset_of(row, col);
            if event.modifiers.control {
                self.refresh_links();
                if self.link_at(offset).is_some() {
                    self.follow_link_at(offset, cx);
                    return;
                }
            }
            self.buffer.set_cursor(offset);
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
        // Normal mode swallows all typed input.
        if self.vim.mode() == Mode::Normal {
            return;
        }
        if let Some(range) = range_utf16 {
            let start = self.offset_from_utf16(range.start);
            let end = self.offset_from_utf16(range.end);
            self.buffer.set_selection(start, end);
        }
        self.buffer.insert(new_text);
        self.insert_edit(cx);
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
        let (row, col) = self.buffer.row_col(start);
        let (_, line, line_bounds) = self.hit_lines.iter().find(|(r, _, _)| *r == row)?;
        let x = line.x_for_index(col);
        Some(Bounds::from_corners(
            gpui::point(line_bounds.left() + x, line_bounds.top()),
            gpui::point(line_bounds.left() + x + px(8.), line_bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let (row, line, line_bounds) = self
            .hit_lines
            .iter()
            .find(|(_, _, b)| b.top() <= point.y && point.y < b.bottom())?;
        let col = line.closest_index_for_x(point.x - line_bounds.left());
        Some(self.offset_to_utf16(self.buffer.offset_of(*row, col)))
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.hit_lines.clear();
        self.input_registered = false;
        let context = if self.vim.mode() == Mode::Insert {
            "vim-insert"
        } else {
            "vim-normal"
        };

        let line_count = self.buffer.line_count();
        let cursor = self.buffer.cursor();
        let selection = self.buffer.selection();
        self.refresh_links();
        tracing::trace!(lines = line_count, "editor render");

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
                    .font_family("monospace")
                    .text_size(px(FONT_SIZE))
                    .line_height(px(LINE_HEIGHT))
                    .bg(rgb(BG))
                    .text_color(rgb(FG))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_click))
                    .children((0..line_count).map(|row| {
                        let start = self.buffer.line_start(row);
                        let end = self.buffer.line_end(row);
                        let text: SharedString = self.buffer.text()[start..end].to_string().into();

                        // Selection restricted to this line.
                        let line_sel = selection.and_then(|(a, h)| {
                            let (lo, hi) = (a.min(h), a.max(h));
                            if hi <= start || lo >= end {
                                return None;
                            }
                            let s = lo.saturating_sub(start);
                            let e = hi.min(end) - start;
                            Some((s, e))
                        });
                        // Cursor column on this line (insert mode draws a full
                        // bar; normal mode highlights the char under it).
                        let cursor_col = if self.buffer.row_col(cursor).0 == row {
                            Some(cursor - start)
                        } else {
                            None
                        };

                        // Wiki-link spans restricted to this line, in
                        // line-relative byte offsets, with whether they
                        // resolve to an existing note.
                        let line_links: Vec<(usize, usize, bool)> = self
                            .links
                            .iter()
                            .filter(|l| l.start < end && l.end > start)
                            .map(|l| {
                                (
                                    l.start.saturating_sub(start),
                                    l.end.min(end).saturating_sub(start),
                                    self.is_known(&l.target),
                                )
                            })
                            .collect();

                        div()
                            .id(("line", row))
                            .w_full()
                            .h(px(LINE_HEIGHT))
                            .child(LineElement {
                                editor: cx.entity(),
                                row,
                                text,
                                line_sel,
                                cursor_col,
                                insert: self.vim.mode() == Mode::Insert,
                                links: line_links,
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

/// One painted line: text, optional selection and cursor quads.
struct LineElement {
    editor: Entity<MarkdownEditor>,
    row: usize,
    text: SharedString,
    line_sel: Option<(usize, usize)>,
    cursor_col: Option<usize>,
    insert: bool,
    /// (start, end, resolved) byte spans of wiki-links within `text`.
    links: Vec<(usize, usize, bool)>,
}

struct LinePrepaint {
    line: ShapedLine,
}

impl IntoElement for LineElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LineElement {
    type RequestLayoutState = ();
    type PrepaintState = LinePrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(LINE_HEIGHT).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let style = window.text_style();
        let font = style.font();
        let total = self.text.len();

        // One TextRun per wiki-link span (colored) plus the plain text
        // between them. TextRun lengths are utf-8 byte counts.
        let mut runs: Vec<TextRun> = Vec::new();
        let mut pos = 0usize;
        let plain = |len: usize| TextRun {
            len,
            font: font.clone(),
            color: rgb(FG).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        for &(start, end, resolved) in &self.links {
            let start = start.min(total);
            let end = end.clamp(start, total);
            if end <= pos {
                continue;
            }
            if start > pos {
                runs.push(plain(start - pos));
            }
            runs.push(TextRun {
                len: end - start,
                font: font.clone(),
                color: rgb(if resolved { LINK } else { LINK_UNRESOLVED }).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            pos = end;
        }
        if pos < total {
            runs.push(plain(total - pos));
        }

        let line = window
            .text_system()
            .shape_line(self.text.clone(), px(FONT_SIZE), &runs, None);
        LinePrepaint { line }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line = &prepaint.line;

        // gpui only routes typed characters to an input handler that was
        // registered during the current frame's paint, so exactly one line
        // element per frame registers it (with the bounds of the first painted
        // line, which is also what IME candidate windows anchor to).
        let register = self.editor.update(cx, |editor, _| {
            if editor.input_registered {
                false
            } else {
                editor.input_registered = true;
                true
            }
        });
        if register {
            let focus_handle = self.editor.read(cx).focus_handle.clone();
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.editor.clone()),
                cx,
            );
        }

        // Selection quad.
        if let Some((s, e)) = self.line_sel {
            let x0 = line.x_for_index(s);
            let x1 = line.x_for_index(e.max(s + 1));
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(bounds.left() + x0, bounds.top()),
                    gpui::point(bounds.left() + x1, bounds.bottom()),
                ),
                rgb(SELECTION),
            ));
        }

        if let Err(err) = line.paint(
            bounds.origin,
            px(LINE_HEIGHT),
            gpui::TextAlign::default(),
            None,
            window,
            cx,
        ) {
            tracing::warn!(row = self.row, "failed to paint line: {err:#}");
        }

        // Cursor quad.
        if let Some(col) = self.cursor_col {
            let x = line.x_for_index(col);
            let width = if self.insert {
                px(1.5)
            } else {
                let next = line.x_for_index((col + 1).min(self.text.len()));
                (next - x).max(px(7.))
            };
            window.paint_quad(fill(
                Bounds::from_corners(
                    gpui::point(bounds.left() + x, bounds.top()),
                    gpui::point(bounds.left() + x + width, bounds.bottom()),
                ),
                rgb(CURSOR),
            ));
        }

        // Record for hit-testing.
        self.editor.update(cx, |editor, _cx| {
            editor.hit_lines.push((self.row, line.clone(), bounds));
        });
    }
}
