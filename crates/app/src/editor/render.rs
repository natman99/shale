//! The view itself: resolving the font, building this frame's display lines and
//! handing each one to a `LineElement`, and wiring the vim key context.

use gpui::{
    div, font, prelude::*, px, rgb, Context, CursorStyle, IntoElement, LineWrapperHandle,
    MouseButton, ParentElement, Render, Styled, Window,
};
use shale_core::vim::{Key, Mode};

use crate::acts::*;
use crate::line::LineElement;
use crate::preview::{LineLayout, PieceMap};
use crate::table_view;
use crate::theme;

use super::MarkdownEditor;
use super::FONT_SIZE;

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
        // Live preview: every row but the cursor's and the selection's is painted
        // without its markdown markers, and the wrap model has to measure the text
        // that is actually left visible. A table row hides its markers too, but
        // the grid paint applies them cell by cell.
        let hidden = self.hidden_markers(selection);
        let row_pieces = self.row_pieces(&hidden);
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
            self.display_lines(
                wrap_width,
                &mut wrappers,
                &row_styles,
                &table_rows,
                &row_pieces,
            )
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
            // The table row and column moves: alt-held keys, which the engine
            // keeps apart from the plain motions.
            .on_action(cx.listener(|this, _: &VimAltH, _, cx| this.normal(Key::AltChar('h'), cx)))
            .on_action(cx.listener(|this, _: &VimAltJ, _, cx| this.normal(Key::AltChar('j'), cx)))
            .on_action(cx.listener(|this, _: &VimAltK, _, cx| this.normal(Key::AltChar('k'), cx)))
            .on_action(cx.listener(|this, _: &VimAltL, _, cx| this.normal(Key::AltChar('l'), cx)))
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
                        let table = plans.get(dl.row).cloned().flatten();
                        // The visible pieces of this display line: the row's
                        // pieces clipped to it, which is the whole line when no
                        // markers were hidden.
                        let line_pieces: Vec<(usize, usize)> = row_pieces
                            .get(dl.row)
                            .map(|pieces| {
                                pieces
                                    .iter()
                                    .map(|&(s, e)| (s.max(dl.start), e.min(dl.end)))
                                    .filter(|&(s, e)| e > s)
                                    .collect()
                            })
                            .unwrap_or_default();
                        // A table row is painted as a grid, which decides what of
                        // the row to draw, so it gets the raw row and applies the
                        // hidden ranges itself cell by cell.
                        let layout = if table.is_some() {
                            LineLayout {
                                text: self.buffer.text()[dl.start..dl.end].to_string().into(),
                                map: PieceMap::identity(dl.start, dl.end),
                            }
                        } else {
                            LineLayout::build(self.buffer.text(), &line_pieces)
                        };

                        // Selection restricted to this display line, as indices
                        // into the painted text.
                        let line_sel = selection.and_then(|(a, h)| {
                            let (lo, hi) = (a.min(h), a.max(h));
                            if hi <= dl.start || lo >= dl.end {
                                return None;
                            }
                            let s = layout.map.index_for_offset(lo);
                            let e = layout.map.index_for_offset(hi);
                            (e > s).then_some((s, e))
                        });
                        // Cursor on this display line (insert mode draws a full
                        // bar; normal mode highlights the char under it). The
                        // cursor's own row always keeps its markers, so the index
                        // lands on a real character.
                        let cursor_col =
                            (cursor_display == index).then(|| layout.map.index_for_offset(cursor));

                        // Wiki-link spans overlapping this display line, as
                        // absolute buffer offsets, with whether they resolve to an
                        // existing note.
                        let line_links: Vec<(usize, usize, bool)> = self
                            .links
                            .iter()
                            .filter(|l| l.start < dl.end && l.end > dl.start)
                            .map(|l| {
                                (
                                    l.start.max(dl.start),
                                    l.end.min(dl.end),
                                    self.is_known(&l.target),
                                )
                            })
                            .collect();

                        // Markdown style spans overlapping this display line, as
                        // absolute buffer offsets.
                        let line_spans = self.spans_in(dl.start, dl.end);
                        // A fenced block's background spans the full width, so
                        // the element paints it as a quad rather than a run.
                        let block = row_styles.get(dl.row).copied().unwrap_or_default().block;
                        // A thematic break is drawn as a rule across the line in
                        // place of its dashes — but only when live preview hid
                        // them, which is exactly when the row is neither the
                        // cursor's nor the selection's.
                        let rule = row_styles.get(dl.row).copied().unwrap_or_default().rule
                            && hidden.get(dl.row).is_some_and(|r| !r.is_empty());

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
                                text: layout.text,
                                map: layout.map,
                                hidden: hidden.get(dl.row).cloned().unwrap_or_default(),
                                line_sel,
                                cursor_col,
                                insert: self.vim.mode() == Mode::Insert,
                                links: line_links,
                                spans: line_spans,
                                size: dl.size,
                                block,
                                rule,
                                table,
                                char_width,
                            })
                    })),
            )
    }
}
