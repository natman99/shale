//! The view itself: resolving the font, building this frame's display lines and
//! handing each one to a `LineElement`, and wiring the vim key context.

use std::rc::Rc;

use gpui::{
    div, font, prelude::*, px, rgb, Context, ImgResourceLoader, IntoElement, MouseButton,
    ParentElement, Render, SharedString, Styled, Window,
};
use shale_core::vim::{Key, Mode};

use crate::acts::*;
use crate::dim;
use crate::image;
use crate::line::{DiffRow, LineElement};
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
        self.phantom_hits.borrow_mut().clear();
        // And the entered embed's rows with it: they answer for the note they were cut
        // from, and a stale record would put a caret in a row that is no longer drawn.
        self.embed_hits.clear();
        self.input_registered = false;
        let context = match self.vim.mode() {
            Mode::Insert => "vim-insert",
            Mode::Visual | Mode::VisualLine => "vim-visual",
            Mode::Normal => "vim-normal",
        };

        let line_count = self.note.buffer.borrow().line_count();
        let cursor = self.note.buffer.borrow().cursor();
        // In visual mode the buffer keeps (anchor, head) with the head *on* the
        // last selected character, so the painted range comes from the engine,
        // which knows whether the selection is charwise or linewise.
        let selection = match self.vim.mode() {
            Mode::Visual | Mode::VisualLine => self.vim.selection_range(&self.note.buffer.borrow()),
            _ => self.note.buffer.borrow().selection(),
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
            shale_core::table::tables(self.note.buffer.borrow().text())
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
        // The host's folds, from the host's parked slot — see `host_slot`. The handle
        // is cloned because the slot borrow needs `self` mutably and the buffer read
        // needs it immutably, which is the same fact the two halves of this view are
        // made of.
        let host_buffer = Rc::clone(&self.note.buffer);
        // Asked every frame, because the setter no-ops when nothing changed: that
        // is what makes a mode change (raw shows the program, live folds it) take
        // effect without a hook of its own, and what keeps the region cache from
        // answering a query note with a markdown note's headings.
        // The query note's answer, measured before the fold is decided: a refusal
        // opens the program, because a folded region with no table is a blank page,
        // and a blank page is a refusal the reader cannot read.
        let answer = self.query_answer();
        let query = self.query_stem();
        let fold_program =
            self.view != View::Raw && answer.as_ref().is_none_or(|answer| answer.error.is_none());
        self.host_slot()
            .set_query_note(query.as_deref(), fold_program);
        let folds = host_buffer.borrow_mut().fold_view(self.host_slot());
        for (row, hidden) in collapsed.iter_mut().enumerate() {
            if folds.collapsed.get(row).is_some_and(|c| *c) {
                *hidden = true;
            }
        }
        let plans = table_view::plans_by_row(
            self.note.buffer.borrow().text(),
            &buffer_tables,
            line_count,
            &collapsed,
        );
        let table_rows: Vec<bool> = plans.iter().map(|plan| plan.is_some()).collect();
        // Images: a row that is nothing but an embed is painted as the picture
        // while the cursor is elsewhere in the note. Whether a row shows the
        // picture or its source follows the same live-preview rule as every
        // other marker, and a row drawn as a picture never soft-wraps.
        let active = self.active_rows(selection);
        let image_plans: Vec<Option<image::ImagePlan>> = match self.vault.as_ref() {
            // Raw mode shows an embed's source and never the picture.
            Some(vault) if self.view != View::Raw => {
                image::plans_by_row(self.note.buffer.borrow().text(), vault, &self.attachments)
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
        // One wrapper per (heading level, regular or wide face), so every row is
        // measured with the face and size it will be painted with. They outlive the
        // display-line pass because an embed's rows are measured with the same ones: a
        // block that wraps with its own wrappers would break somewhere the paint has no
        // glyphs for.
        let mut wrappers = self.body_wrappers(window);
        let display_lines = self.display_lines(
            wrap_width,
            &mut wrappers,
            &row_styles,
            &no_wrap,
            &paint,
            &collapsed,
        );
        // The review overlay (roadmap P2.5b): which rows a queued proposal would take
        // away, and the phantom lines that would replace them. Placed against this
        // frame's text, so a hunk that no longer fits the note is simply not here — the
        // strip still lists it, badged stale.
        let frame = self.frame(&display_lines);
        // The frame noticed the caret's block is gone (ruling Q5). Leaving it is done
        // here rather than in the frame because this is where the context is, and the
        // caret goes back to the host's `![[…]]` row it came from.
        if let Some(path) = self.embed_gone.take() {
            let host_row = self.entered_host_row.unwrap_or(0);
            self.exit_embed(
                Some(format!(
                    "{} no longer holds the section being edited",
                    path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string())
                )),
                cx,
            );
            let host = self.note.buffer.borrow();
            let row = host_row.min(host.line_count() - 1);
            let offset = host.line_start(row);
            drop(host);
            self.buffer.borrow_mut().set_cursor(offset);
        }
        // While the caret is inside an embed, the host paints no caret at all: the
        // parked position stays in the host's buffer but nothing draws it, so the
        // screen holds one caret and it is in the block being edited (ruling 14).
        let cursor_display = match &self.entered {
            Some(_) => usize::MAX,
            None => self
                .note
                .buffer
                .borrow()
                .display_index_for(self.host_slot_ref(), cursor),
        };
        // Focus mode: one alpha per buffer row, measured from the block (or the
        // heading section) the cursor is in. Raw mode is the file as it sits on
        // disk, so it never fades anything.
        let alphas = if self.focus && self.view != View::Raw {
            dim::row_alphas(
                self.note.buffer.borrow().text(),
                self.all_spans(),
                cursor,
                self.focus_scope,
                line_count,
            )
        } else {
            Vec::new()
        };
        // Typewriter scroll moves the viewport, so it happens before the frame is
        // built and only when the caret actually moved.
        if self.typewriter {
            self.typewriter_scroll(cursor, self.caret_scroll_item());
        }

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
            // The note is text, so the note's cursor is a caret — except over a row
            // that answers a click by opening something, which says so the way every
            // other clickable thing in the app does.
            .cursor(self.cursor_style())
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, _window, cx| {
                    let door = this.door_at(event.position).is_some();
                    if this.hover_door != door {
                        this.hover_door = door;
                        cx.notify();
                    }
                }),
            )
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
            // The text-object characters. Which object a pair character names is
            // the engine's call, and one it does not have is refused there.
            .on_action(cx.listener(|this, _: &VimOpenParen, window, cx| {
                this.normal(Key::Char('('), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimCloseParen, window, cx| {
                this.normal(Key::Char(')'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimOpenBrace, window, cx| {
                this.normal(Key::Char('{'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimCloseBrace, window, cx| {
                this.normal(Key::Char('}'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimLess, window, cx| {
                    this.normal(Key::Char('<'), window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &VimGreater, window, cx| {
                this.normal(Key::Char('>'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimQuote, window, cx| {
                    this.normal(Key::Char('"'), window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &VimApostrophe, window, cx| {
                this.normal(Key::Char('\''), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimSemicolon, window, cx| {
                this.normal(Key::Char(';'), window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &VimComma, window, cx| {
                    this.normal(Key::Char(','), window, cx)
                }),
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
            .on_action(
                cx.listener(|this, _: &VimTilde, window, cx| {
                    this.normal(Key::Char('~'), window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &VimUpperS, window, cx| {
                this.normal(Key::Char('S'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperC, window, cx| {
                this.normal(Key::Char('C'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperX, window, cx| {
                this.normal(Key::Char('X'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperY, window, cx| {
                this.normal(Key::Char('Y'), window, cx)
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
            .on_action(cx.listener(|this, _: &VimUpperW, window, cx| {
                this.normal(Key::Char('W'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimUpperB, window, cx| {
                this.normal(Key::Char('B'), window, cx)
            }))
            // `]` and `[`, bound so the engine can own the `]a`/`[a` review chords.
            // They are single keys rather than a two-stroke chord because a literal
            // bracket has to reach the engine as the argument of `f`, `t` or an
            // operator, and gpui holds the first key of a chord as pending input.
            .on_action(cx.listener(|this, _: &VimCloseBracket, window, cx| {
                this.normal(Key::Char(']'), window, cx)
            }))
            .on_action(cx.listener(|this, _: &VimOpenBracket, window, cx| {
                this.normal(Key::Char('['), window, cx)
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
            .on_action(cx.listener(Self::on_paste))
            .child(
                div()
                    .id("editor-scroll")
                    .size_full()
                    .overflow_scroll()
                    // The note's gutter, and it has to be *here* rather than on the
                    // workspace's note column: `overflow_scroll` clips to this div's own
                    // bounds, so anything a line paints left of its content edge — a
                    // callout's kind mark today, fold markers and line numbers after it —
                    // is outside the mask and simply never appears. Padding is inside the
                    // mask (gpui subtracts only borders), so padding on the scroller is
                    // what makes the gutter paintable at all. See `style::GUTTER`.
                    .pl(crate::style::GUTTER)
                    .track_scroll(&self.scroll_handle)
                    // gpui's wheel handler dirties the view that owns the scroller — this
                    // one — and nothing above it. The window has to hear about the scroll
                    // even though it is not the view being scrolled, because a card or a
                    // panel is placed by the window and anchored to a line in here.
                    .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
                    .font_family(self.font_family.clone())
                    .text_size(px(self.font_size))
                    .line_height(px(theme::line_height(self.font_size)))
                    .bg(rgb(self.palette.bg))
                    // The pane's card rounds its bottom corners; the note's
                    // background is the surface that reaches them, and gpui
                    // clips children to rectangles — so the round lives here.
                    .rounded_b(crate::style::PANE_RADIUS)
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
                                text: self.note.buffer.borrow().text()[dl.start..dl.end]
                                    .to_string()
                                    .into(),
                                map: PieceMap::identity(dl.start, dl.end),
                            }
                        } else {
                            LineLayout::build(
                                self.note.buffer.borrow().text(),
                                &line_pieces,
                                &line_subs,
                            )
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
                        // Preview is a reading view: no caret. The block sat
                        // on hidden syntax and images and read as a bug; the
                        // view scrolls instead of moving, so there is nothing
                        // for a caret to point at.
                        let cursor_col = (cursor_display == index && self.view != View::Preview)
                            .then(|| layout.map.index_for_offset(cursor));

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
                        // A properties block's band and boundary, from the same row
                        // facts the wrap model measured with.
                        let frontmatter = row_styles
                            .get(dl.row)
                            .copied()
                            .unwrap_or_default()
                            .frontmatter;
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
                        // The callout's mark belongs to the box, not to every line of it:
                        // the header row's first display line, so a wrapped header still
                        // gets one mark rather than one per segment.
                        let callout_mark = row_styles
                            .get(dl.row)
                            .copied()
                            .unwrap_or_default()
                            .callout_head
                            && dl.start == self.note.buffer.borrow().line_start(dl.row);
                        // How much of this line's colour survives. A row the
                        // dimming system does not mention is drawn in full.
                        let dim = alphas.get(dl.row).copied().unwrap_or(1.0);
                        // A heading whose section is folded shut carries a marker
                        // with the number of hidden rows, on the row's last
                        // display line (a wrapped heading gets one marker, not one
                        // per segment).
                        let fold = folds
                            .marker
                            .get(dl.row)
                            .cloned()
                            .flatten()
                            .filter(|_| dl.end == self.note.buffer.borrow().line_end(dl.row))
                            // A query note's marker says what the file computed and how
                            // much of it there is: folded, the file still answers a
                            // question about itself.
                            .map(|mut mark| {
                                if let (Some(title), Some(answer)) =
                                    (mark.title.as_ref(), answer.as_ref())
                                {
                                    let rows = answer.rows.rows.len();
                                    mark.title = Some(format!(
                                        "{title} · {rows} {}",
                                        if rows == 1 { "row" } else { "rows" }
                                    ));
                                }
                                mark
                            });
                        // An embedded image makes its display line as tall as the
                        // picture; every other line follows its text size.
                        let height = image
                            .as_ref()
                            .map(|image| f32::from(image.height))
                            .unwrap_or_else(|| theme::line_height(dl.size));
                        let editor = cx.entity();
                        // A row a queued proposal would take away: struck through, in
                        // the palette's red, with the hunk's bar down its left edge.
                        let diff = frame.struck.get(&dl.row).map(|current| DiffRow {
                            color: self.palette.diff_delete,
                            current: *current,
                        });
                        let before = frame
                            .before
                            .get(&index)
                            .map(|blocks| {
                                self.phantom_lines(
                                    blocks,
                                    char_width,
                                    cx.entity(),
                                    wrap_width,
                                    &mut wrappers,
                                )
                            })
                            .unwrap_or_default();
                        let after = frame
                            .after
                            .get(&index)
                            .map(|blocks| {
                                self.phantom_lines(
                                    blocks,
                                    char_width,
                                    cx.entity(),
                                    wrap_width,
                                    &mut wrappers,
                                )
                            })
                            .unwrap_or_default();

                        let line = div()
                            .w_full()
                            .h(px(height))
                            .child(LineElement {
                                editor: cx.entity(),
                                embed: false,
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
                                cursor_dim: self.cursor_dim,
                                insert: self.vim.mode() == Mode::Insert,
                                links: line_links,
                                spans: line_spans,
                                marks: self.marks_in(dl.start, dl.end),
                                hits: line_hits,
                                active: line_active,
                                size: dl.size,
                                dim,
                                block,
                                frontmatter,
                                rule,
                                callout,
                                callout_mark,
                                table,
                                image,
                                char_width,
                                palette: Rc::clone(&self.palette),
                                fold,
                                diff,
                                gutter: Vec::new(),
                            })
                            // Where the pointer is over the note is reported to the
                            // window, which decides whether that is a word the
                            // checker has something to say about. The line under the
                            // pointer is the only one that reports, and the offset is
                            // worked out from the text that was painted on it.
                            .on_mouse_move(move |event: &gpui::MouseMoveEvent, _window, cx| {
                                let offset = editor.read_with(cx, |editor, _cx| {
                                    editor.offset_at_pointer_x(
                                        crate::editor::LineRef::Display(index),
                                        event.position.x,
                                    )
                                });
                                if let Some(offset) = offset {
                                    editor.update(cx, |_, cx| {
                                        cx.emit(EditorEvent::Hovered {
                                            offset,
                                            line: crate::editor::LineRef::Display(index),
                                            at: event.position,
                                        })
                                    });
                                }
                            });
                        // The phantom lines hang off the display line they belong beside
                        // rather than joining the scroller's children. That keeps the
                        // display-line index space exactly as the buffer's display model
                        // counts it, which is what lets `j`/`k`, the ruler, the caret and
                        // scroll-to-item go on ignoring the proposal entirely — the ruled
                        // shape of "not in the buffer, so motion skips them but the frame
                        // paints them".
                        if before.is_empty() && after.is_empty() {
                            div().id(("line", index)).w_full().child(line)
                        } else {
                            div()
                                .id(("line", index))
                                .w_full()
                                .flex_col()
                                .children(before)
                                .child(line)
                                .children(after)
                        }
                    })),
            )
    }
}

impl MarkdownEditor {
    /// Typewriter scroll: put the caret's display line near the middle of the
    /// window, so the eye does not have to hunt for it after `j` walks off the
    /// bottom or `G` jumps away.
    ///
    /// It acts only when the caret moved, which is what leaves a reader who
    /// scrolled by hand alone: moving the viewport without moving the caret is
    /// their doing, not the frame's. The line's position comes from the previous
    /// frame's child bounds, which is one frame behind at worst and costs no
    /// measuring of its own.
    ///
    /// gpui's own scroll-into-view still runs; it only moves the viewport the
    /// minimum to bring an off-screen caret back, so once the caret is centred it
    /// has nothing left to do.
    fn typewriter_scroll(&mut self, cursor: usize, cursor_display: usize) {
        if self.typewriter_anchor == Some(cursor) {
            return;
        }
        self.typewriter_anchor = Some(cursor);
        let Some(line) = self.scroll_handle.bounds_for_item(cursor_display) else {
            // The first frame has no measured geometry to work from.
            return;
        };
        let view = self.scroll_handle.bounds();
        if view.size.height <= px(0.) {
            return;
        }
        // The same arithmetic gpui's own `scroll_to_item` does to bring a child to
        // the top of the viewport, plus enough to bring it to the middle instead.
        let middle = (view.size.height - line.size.height) / 2.;
        let max = self.scroll_handle.max_offset().y;
        let mut offset = self.scroll_handle.offset();
        offset.y = (view.top() - line.top() + middle).clamp(-max, px(0.));
        self.scroll_handle.set_offset(offset);
    }
}
