//! The view itself: resolving the font, building this frame's display lines and
//! handing each one to a `LineElement`, and wiring the vim key context.

use std::rc::Rc;

use gpui::{
    div, font, prelude::*, px, rgb, Context, CursorStyle, ImgResourceLoader, IntoElement,
    LineWrapperHandle, MouseButton, ParentElement, Render, SharedString, Styled, Window,
};
use shale_core::vim::{Key, Mode};

use crate::acts::*;
use crate::image;
use crate::line::LineElement;
use crate::preview::{LineLayout, PieceMap};
use crate::table_view;
use crate::theme;

use super::{EditorEvent, MarkdownEditor, View};

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
                    font_size = self.font_size,
                    regular_char_width = ?text_system.layout_width(regular, px(self.font_size), 'a'),
                    bold_char_width = ?text_system.layout_width(bold, px(self.font_size), 'a'),
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
        self.refresh_quotes();
        tracing::trace!(lines = line_count, "editor render");

        // Soft wrap: the scroll container's width from the previous frame tells
        // us where each row breaks; the buffer gets the same boundaries so j/k
        // can step across wrapped rows. Rows that will be painted with a wide
        // face are measured with it, so their break points match the paint.
        let wrap_width = self.scroll_handle.bounds().size.width;
        let row_styles = self.row_styles();
        // Tables are painted as aligned grids instead of raw markdown: one plan
        // per buffer row, and a row that has one never soft-wraps, which the wrap
        // model has to be told about so it agrees with the paint. Raw mode has no
        // tables at all — the grid, the hidden `|` and the collapsed delimiter row
        // are all rendering — so a table row is ordinary text there and wraps like
        // any other row.
        let buffer_tables = if self.view == View::Raw {
            Vec::new()
        } else {
            shale_core::table::tables(self.buffer.text())
        };
        // Rows live preview leaves out entirely: the fence lines of a code block
        // the cursor is not in, and a table's delimiter row (unless the cursor is
        // on it or on the header above it). Dropping them from the display model
        // is what keeps j/k, the scroll position and the ruler in step with what
        // is on screen.
        let mut collapsed = self.collapsed_rows(selection, &buffer_tables);
        // Folding: a closed section's rows take no display line at all, exactly
        // like a hidden fence line, which is what keeps j/k, the ruler and the
        // scroll position in step with what is on screen. The text is untouched.
        let folds = self.buffer.fold_view();
        for (row, hidden) in collapsed.iter_mut().enumerate() {
            if folds.collapsed.get(row).is_some_and(|c| *c) {
                *hidden = true;
            }
        }
        let plans =
            table_view::plans_by_row(self.buffer.text(), &buffer_tables, line_count, &collapsed);
        let table_rows: Vec<bool> = plans.iter().map(|plan| plan.is_some()).collect();
        // Images: a row that is nothing but an embed is painted as the picture
        // while the cursor is elsewhere in the note. Whether a row shows the
        // picture or its source follows the same live-preview rule as every
        // other marker, and a row drawn as a picture never soft-wraps.
        let active = self.active_rows(selection);
        let image_plans: Vec<Option<image::ImagePlan>> = match self.vault.as_ref() {
            // Raw mode shows an embed's source and never the picture.
            Some(vault) if self.view != View::Raw => {
                image::plans_by_row(self.buffer.text(), vault, &self.attachments)
            }
            _ => Vec::new(),
        };
        let image_rows: Vec<bool> = (0..line_count)
            .map(|row| {
                image_plans.get(row).is_some_and(|plan| plan.is_some())
                    && !active.get(row).copied().unwrap_or(false)
            })
            .collect();
        let no_wrap: Vec<bool> = (0..line_count)
            .map(|row| {
                table_rows.get(row).copied().unwrap_or(false)
                    || image_rows.get(row).copied().unwrap_or(false)
            })
            .collect();
        // Live preview: every row but the cursor's and the selection's is painted
        // without its markdown markers, and the wrap model has to measure the text
        // that is actually left visible. A table row hides its markers too, but
        // the grid paint applies them cell by cell.
        let hidden = self.hidden_markers(selection);
        // What each row is painted from: the pieces live preview leaves of it, and
        // the straight quotes the frame draws curly. They change what a row looks
        // like without changing its pieces, so both the wrap model and the paint
        // need them, and the map has to be built from both at once.
        let paint = self.row_paint(&hidden);
        // One character cell of the resolved face, which is what turns the grid's
        // character geometry into pixels.
        let char_width = {
            let text_system = window.text_system();
            let font_id = text_system.resolve_font(&font(self.font_family.clone()));
            text_system.layout_width(font_id, px(self.font_size), 'a')
        };
        let display_lines = {
            let text_system = window.text_system();
            // One wrapper per (heading level, regular or wide face), so every
            // row is measured with the face and size it will be painted with.
            let family = self.font_family.clone();
            let mut wrappers: Vec<LineWrapperHandle> = Vec::with_capacity(theme::SIZES * 2);
            for level in 0..theme::SIZES as u8 {
                let size = px(theme::font_size(level, self.font_size));
                wrappers.push(text_system.line_wrapper(font(family.clone()), size));
                wrappers.push(text_system.line_wrapper(font(family.clone()).bold(), size));
            }
            self.display_lines(
                wrap_width,
                &mut wrappers,
                &row_styles,
                &no_wrap,
                &paint,
                &collapsed,
            )
        };
        let cursor_display = self.buffer.display_index_for(cursor);

        // Ask gpui for each picture it will draw. It decodes on a background
        // thread and marks the view for redraw when the file arrives, so the
        // frame right after an embed is typed shows its source and the next one
        // shows the image.
        let images: Vec<Option<image::ImagePaint>> = image_rows
            .iter()
            .enumerate()
            .map(|(row, shown)| {
                if !*shown {
                    return None;
                }
                let plan = image_plans.get(row).cloned().flatten()?;
                let source = gpui::Resource::Path(plan.path.clone().into());
                match window.use_asset::<ImgResourceLoader>(&source, cx) {
                    Some(Ok(data)) => Some(image::draw_size(
                        &data,
                        window.scale_factor(),
                        wrap_width,
                        &plan,
                    )),
                    // The asset cache holds the failure, so this repeats every
                    // frame: trace level, and the row keeps showing its source.
                    Some(Err(err)) => {
                        tracing::trace!(path = %plan.path.display(), "image not loaded: {err:#}");
                        None
                    }
                    // Still decoding.
                    None => None,
                }
            })
            .collect();

        div()
            .key_context(context)
            .track_focus(&self.focus_handle)
            .size_full()
            .cursor(CursorStyle::IBeam)
            // Vim normal-mode keys.
            .on_action(
                cx.listener(|this, _: &VimH, window, cx| this.normal(Key::Char('h'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimJ, window, cx| this.normal(Key::Char('j'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimK, window, cx| this.normal(Key::Char('k'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimL, window, cx| this.normal(Key::Char('l'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimW, window, cx| this.normal(Key::Char('w'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimB, window, cx| this.normal(Key::Char('b'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimE, window, cx| this.normal(Key::Char('e'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimF, window, cx| this.normal(Key::Char('f'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperF, window, cx| {
                this.normal(Key::Char('F'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimT, window, cx| this.normal(Key::Char('t'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperT, window, cx| {
                this.normal(Key::Char('T'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimSpace, window, cx| {
                    this.normal(Key::Char(' '), window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &VimZero, window, cx| {
                    this.normal(Key::Char('0'), window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &VimCaret, window, cx| {
                    this.normal(Key::Char('^'), window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &VimDollar, window, cx| {
                this.normal(Key::Char('$'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimG, window, cx| this.normal(Key::Char('g'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperG, window, cx| {
                this.normal(Key::Char('G'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimX, window, cx| this.normal(Key::Char('x'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimD, window, cx| this.normal(Key::Char('d'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimC, window, cx| this.normal(Key::Char('c'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimY, window, cx| this.normal(Key::Char('y'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimP, window, cx| this.normal(Key::Char('p'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperP, window, cx| {
                this.normal(Key::Char('P'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimU, window, cx| this.normal(Key::Char('u'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimDigit1, window, cx| {
                this.normal(Key::Char('1'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimDigit2, window, cx| {
                this.normal(Key::Char('2'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimDigit3, window, cx| {
                this.normal(Key::Char('3'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimDigit4, window, cx| {
                this.normal(Key::Char('4'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimDigit5, window, cx| {
                this.normal(Key::Char('5'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimDigit6, window, cx| {
                this.normal(Key::Char('6'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimDigit7, window, cx| {
                this.normal(Key::Char('7'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimDigit8, window, cx| {
                this.normal(Key::Char('8'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimDigit9, window, cx| {
                this.normal(Key::Char('9'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimI, window, cx| this.normal(Key::Char('i'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperI, window, cx| {
                this.normal(Key::Char('I'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimA, window, cx| this.normal(Key::Char('a'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperA, window, cx| {
                this.normal(Key::Char('A'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimO, window, cx| this.normal(Key::Char('o'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperO, window, cx| {
                this.normal(Key::Char('O'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimV, window, cx| this.normal(Key::Char('v'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperV, window, cx| {
                this.normal(Key::Char('V'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperH, window, cx| {
                this.normal(Key::Char('H'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperJ, window, cx| {
                this.normal(Key::Char('J'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperK, window, cx| {
                this.normal(Key::Char('K'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperL, window, cx| {
                this.normal(Key::Char('L'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperD, window, cx| {
                this.normal(Key::Char('D'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimPipe, window, cx| {
                    this.normal(Key::Char('|'), window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &VimBackslash, window, cx| {
                this.normal(Key::Char('\\'), window, cx)
            }))
            // The table row and column moves: alt-held keys, which the engine
            // keeps apart from the plain motions.
            .on_action(cx.listener(|this, _: &VimAltH, window, cx| {
                this.normal(Key::AltChar('h'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimAltJ, window, cx| {
                this.normal(Key::AltChar('j'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimAltK, window, cx| {
                this.normal(Key::AltChar('k'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimAltL, window, cx| {
                this.normal(Key::AltChar('l'), window, cx)
            }))
            // Folding. `z` opens the command and the engine reads the second key,
            // so `za`/`zc`/`zo`/`zd` arrive through the actions those keys already
            // have; only the shifted command letters need bindings of their own.
            .on_action(
                cx.listener(|this, _: &VimZ, window, cx| this.normal(Key::Char('z'), window, cx)),
            )
            .on_action(cx.listener(|this, _: &VimUpperR, window, cx| {
                this.normal(Key::Char('R'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperM, window, cx| {
                this.normal(Key::Char('M'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperE, window, cx| {
                this.normal(Key::Char('E'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimDot, window, cx| this.normal(Key::Char('.'), window, cx)),
            )
            // Bound for the `space s` leader chord, which the engine owns.
            .on_action(
                cx.listener(|this, _: &VimS, window, cx| this.normal(Key::Char('s'), window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &VimR, window, cx| this.normal(Key::Char('r'), window, cx)),
            )
            // vim's command line: the window reads the word typed next. Not the
            // command palette, which is `ctrl-shift-p`.
            .on_action(cx.listener(Self::on_colon))
            // Shared editing keys.
            .on_action(cx.listener(Self::on_escape))
            .on_action(cx.listener(Self::on_newline))
            .on_action(cx.listener(Self::on_next_line))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete_forward))
            .on_action(cx.listener(Self::on_delete_word_back))
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
                    .text_size(px(self.font_size))
                    .line_height(px(theme::line_height(self.font_size)))
                    .bg(rgb(self.palette.bg))
                    .text_color(rgb(self.palette.fg))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_click))
                    .children(display_lines.iter().enumerate().map(|(index, dl)| {
                        let table = plans.get(dl.row).cloned().flatten();
                        // The picture this row is drawn as, if live preview chose
                        // the image over the row's source.
                        let image = images.get(dl.row).cloned().flatten();
                        // The visible pieces of this display line: the row's
                        // pieces clipped to it, which is the whole line when no
                        // markers were hidden, and its quotes narrowed to the same
                        // run of the row.
                        let row_paint = paint.get(dl.row).cloned().unwrap_or_default();
                        let line_pieces: Vec<(usize, usize)> = row_paint
                            .pieces
                            .iter()
                            .map(|&(s, e)| (s.max(dl.start), e.min(dl.end)))
                            .filter(|&(s, e)| e > s)
                            .collect();
                        let line_subs: Vec<(usize, char)> = row_paint
                            .subs
                            .iter()
                            .filter(|(offset, _)| *offset >= dl.start && *offset < dl.end)
                            .copied()
                            .collect();
                        // A table row is painted as a grid, which decides what of
                        // the row to draw, so it gets the raw row and applies the
                        // hidden ranges itself cell by cell. An image row has no
                        // text at all: the identity map keeps a click on the
                        // picture landing on the row's start, which is what brings
                        // the source back for editing.
                        let layout = if image.is_some() {
                            LineLayout {
                                text: SharedString::default(),
                                map: PieceMap::identity(dl.start, dl.end),
                            }
                        } else if table.is_some() {
                            LineLayout {
                                text: self.buffer.text()[dl.start..dl.end].to_string().into(),
                                map: PieceMap::identity(dl.start, dl.end),
                            }
                        } else {
                            LineLayout::build(self.buffer.text(), &line_pieces, &line_subs)
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
                        // existing note. Raw mode draws a link as the text that is
                        // in the file, brackets and all; `gx` and ctrl-click still
                        // follow it, because those ask the buffer rather than the
                        // paint.
                        let line_links: Vec<(usize, usize, bool)> = if self.view == View::Raw {
                            Vec::new()
                        } else {
                            self.links
                                .iter()
                                .filter(|l| l.start < dl.end && l.end > dl.start)
                                .map(|l| {
                                    (
                                        l.start.max(dl.start),
                                        l.end.min(dl.end),
                                        self.is_known(&l.target),
                                    )
                                })
                                .collect()
                        };

                        // Markdown style spans overlapping this display line, as
                        // absolute buffer offsets. Raw mode has no syntax colours,
                        // which is what makes it look like the file rather than
                        // like the note.
                        let line_spans = if self.view == View::Raw {
                            Vec::new()
                        } else {
                            self.spans_in(dl.start, dl.end)
                        };
                        // The `/` matches on this line, and the current one, as
                        // absolute buffer offsets; the element rebases them onto the
                        // text it paints.
                        let (line_hits, line_active) = self.hits_in(dl.start, dl.end);
                        // A fenced block's background spans the full width, so
                        // the element paints it as a quad rather than a run.
                        let block = row_styles.get(dl.row).copied().unwrap_or_default().block;
                        // A thematic break is drawn as a rule across the line in
                        // place of its dashes — but only when live preview hid
                        // them, which is exactly when the row is neither the
                        // cursor's nor the selection's.
                        let rule = row_styles.get(dl.row).copied().unwrap_or_default().rule
                            && hidden.get(dl.row).is_some_and(|r| !r.is_empty());
                        // A callout row gets a box painted behind its text, in the
                        // colour of its type; the markers stay visible on the row the
                        // cursor is on, exactly as with other markdown syntax.
                        let callout = row_styles.get(dl.row).copied().unwrap_or_default().callout;
                        // A heading whose section is folded shut carries a marker
                        // with the number of hidden rows, on the row's last
                        // display line (a wrapped heading gets one marker, not one
                        // per segment).
                        let fold = folds
                            .marker
                            .get(dl.row)
                            .copied()
                            .flatten()
                            .filter(|_| dl.end == self.buffer.line_end(dl.row));
                        // An embedded image makes its display line as tall as the
                        // picture; every other line follows its text size.
                        let height = image
                            .as_ref()
                            .map(|image| f32::from(image.height))
                            .unwrap_or_else(|| theme::line_height(dl.size));
                        let editor = cx.entity();

                        div()
                            .id(("line", index))
                            .w_full()
                            .h(px(height))
                            .child(LineElement {
                                editor: cx.entity(),
                                row: dl.row,
                                index,
                                start: dl.start,
                                end: dl.end,
                                text: layout.text,
                                map: layout.map,
                                hidden: hidden.get(dl.row).cloned().unwrap_or_default(),
                                quotes: line_subs,
                                line_sel,
                                cursor_col,
                                insert: self.vim.mode() == Mode::Insert,
                                links: line_links,
                                spans: line_spans,
                                marks: self.marks_in(dl.start, dl.end),
                                hits: line_hits,
                                active: line_active,
                                size: dl.size,
                                block,
                                rule,
                                callout,
                                table,
                                image,
                                char_width,
                                palette: Rc::clone(&self.palette),
                                fold,
                            })
                            // Where the pointer is over the note is reported to the
                            // window, which decides whether that is a word the
                            // checker has something to say about. The line under the
                            // pointer is the only one that reports, and the offset is
                            // worked out from the text that was painted on it.
                            .on_mouse_move(move |event: &gpui::MouseMoveEvent, _window, cx| {
                                let offset = editor.read_with(cx, |editor, _cx| {
                                    editor.offset_at_pointer_x(index, event.position.x)
                                });
                                if let Some(offset) = offset {
                                    editor.update(cx, |_, cx| {
                                        cx.emit(EditorEvent::Hovered {
                                            offset,
                                            line: index,
                                            at: event.position,
                                        })
                                    });
                                }
                            })
                    })),
            )
    }
}
