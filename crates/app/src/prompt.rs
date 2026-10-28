//! A one-line text prompt: the name for a new note, the new name in a rename, the
//! confirmation before a delete.
//!
//! The editor's input surface (`editor::input`) is built for a multi-line buffer
//! with vim modes, so a prompt has its own small one: an entity that implements
//! gpui's [`EntityInputHandler`] over a single `String`, plus a `prompt` key
//! context for the handful of keys that are not printable text. Printable
//! characters arrive through the input handler exactly as they do in the editor,
//! which on Linux is the only path that works for keys the keymap does not bind
//! (see the `ElementInputHandler` note in PROJECT.md).
//!
//! The field is drawn by [`PromptField`], a custom element, because gpui only
//! routes typed characters to a handler registered during the current frame's
//! paint — which is the one moment the field's bounds are known.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    div, fill, font, px, relative, rgb, App, Bounds, Context, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    GlobalElementId, InspectorElementId, InteractiveElement, IntoElement, LayoutId, ParentElement,
    Pixels, Render, ShapedLine, SharedString, Style as ElementStyle, Styled, UTF16Selection,
    Window,
};
use shale_core::theme::Palette;

use crate::acts::{
    PromptBackspace, PromptCancel, PromptDelete, PromptLeft, PromptRight, PromptSubmit,
};
use crate::theme::{self, RunStyle};

/// The key context the prompt's bindings live in. While it is open its keys win
/// over the editor's vim contexts, which is what lets `enter` mean "accept".
pub(crate) const CONTEXT: &str = "prompt";

/// Padding inside the field, in pixels.
const PAD: f32 = 4.;

/// What the user did with the prompt.
#[derive(Clone)]
pub(crate) enum PromptEvent {
    /// Enter: the field's contents, which the workspace validates.
    Submit(SharedString),
    /// Escape: nothing was answered.
    Cancel,
}

/// Why a prompt is open. The workspace keeps one of these beside the entity so it
/// knows what to do with the answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PromptKind {
    /// A name for a new note.
    NewNote,
    /// A name for a new folder.
    NewFolder,
    /// The new name of the note or folder being renamed.
    Rename,
    /// A delete, which the user confirms rather than accidentally triggering.
    Delete,
}

/// A single-line text field with a caret.
pub(crate) struct Prompt {
    focus_handle: FocusHandle,
    /// What the answer will be used for.
    kind: PromptKind,
    /// The question shown above the field.
    label: SharedString,
    /// What the field holds so far.
    text: SharedString,
    /// Byte offset in `text` where the caret sits. gpui's input handler speaks
    /// utf-16, so every conversion goes through the two helpers below.
    caret: usize,
    /// The colours the field is drawn with, taken from the workspace when the
    /// prompt is opened.
    palette: Rc<Palette>,
    /// The editor's resolved font family, so the field matches the note text.
    family: SharedString,
    /// Text size in points.
    size: f32,
}

impl Prompt {
    pub(crate) fn new(
        kind: PromptKind,
        label: impl Into<SharedString>,
        initial: &str,
        palette: Rc<Palette>,
        family: SharedString,
        size: f32,
        cx: &mut App,
    ) -> Self {
        let text = SharedString::from(initial.to_string());
        Self {
            focus_handle: cx.focus_handle(),
            kind,
            label: label.into(),
            caret: text.len(),
            text,
            palette,
            family,
            size,
        }
    }

    pub(crate) fn kind(&self) -> PromptKind {
        self.kind
    }

    /// What the field holds. Only the tests ask: the workspace reads the answer
    /// out of the `Submit` event.
    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The text the field paints: the caret always sits after a character, so an
    /// empty field still needs something to shape.
    fn painted(&self) -> SharedString {
        if self.text.is_empty() {
            SharedString::from(" ")
        } else {
            self.text.clone()
        }
    }

    fn insert(&mut self, text: &str) {
        let mut new = self.text.to_string();
        new.replace_range(self.caret..self.caret, text);
        self.caret += text.len();
        self.text = SharedString::from(new);
    }

    fn backspace(&mut self) {
        if self.caret == 0 {
            return;
        }
        let mut new = self.text.to_string();
        // One *character*, not one byte: a name can hold non-ASCII text.
        let previous = new[..self.caret]
            .chars()
            .next_back()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        let start = self.caret - previous;
        new.replace_range(start..self.caret, "");
        self.caret = start;
        self.text = SharedString::from(new);
    }

    fn delete_forward(&mut self) {
        if self.caret >= self.text.len() {
            return;
        }
        let mut new = self.text.to_string();
        let width = new[self.caret..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        new.replace_range(self.caret..self.caret + width, "");
        self.text = SharedString::from(new);
    }

    /// Move the caret by `delta` characters, clamped to the field.
    fn step_caret(&mut self, delta: isize) {
        let mut new = self.caret as isize;
        if delta < 0 {
            new -= self.text[..self.caret]
                .chars()
                .next_back()
                .map_or(0, |c| c.len_utf8()) as isize;
        } else if delta > 0 {
            new += self.text[self.caret..]
                .chars()
                .next()
                .map_or(0, |c| c.len_utf8()) as isize;
        }
        self.caret = new.clamp(0, self.text.len() as isize) as usize;
    }

    // ----- utf16 <-> utf8 helpers (gpui's input handler speaks utf-16) -----

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.text.chars() {
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
        for ch in self.text.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }
}

impl Focusable for Prompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PromptEvent> for Prompt {}

impl EntityInputHandler for Prompt {
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
        Some(self.text[start..end].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.offset_to_utf16(self.caret)..self.offset_to_utf16(self.caret),
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
        // A name is one line, so anything the platform hands us that contains a
        // newline (paste, or an IME that confirms with Enter) is flattened: the
        // first line goes in, the rest are dropped.
        let first_line = new_text.lines().next().unwrap_or("");
        if let Some(range) = range_utf16 {
            let start = self.offset_from_utf16(range.start);
            let end = self.offset_from_utf16(range.end);
            let mut new = self.text.to_string();
            new.replace_range(start..end, first_line);
            self.caret = start + first_line.len();
            self.text = SharedString::from(new);
        } else {
            self.insert(first_line);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Composition is not tracked separately: the composing text is shown as
        // ordinary text, exactly as the editor does it.
        self.replace_text_in_range(range_utf16, new_text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let offset = self.offset_from_utf16(range_utf16.start);
        let x = PAD + (offset as f32 * self.palette_char_width());
        Some(Bounds::from_corners(
            gpui::point(bounds.left() + px(x), bounds.top()),
            gpui::point(bounds.left() + px(x + 8.), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let x = f32::from(point.x) - PAD;
        let column = (x / self.palette_char_width()).max(0.).round() as usize;
        let offset = self
            .text
            .char_indices()
            .map(|(i, _)| i)
            .nth(column)
            .unwrap_or(self.text.len());
        Some(self.offset_to_utf16(offset))
    }
}

impl Prompt {
    /// Rough advance of one character, used only for the two geometry questions
    /// the IME asks (where to put a candidate window, and which character a click
    /// landed on). The field's own painting uses the shaped line.
    fn palette_char_width(&self) -> f32 {
        f32::from(px(8.))
    }
}

impl Render for Prompt {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &PromptSubmit, _window, cx| {
                cx.emit(PromptEvent::Submit(this.text.clone()));
            }))
            .on_action(cx.listener(|_, _: &PromptCancel, _window, cx| {
                cx.emit(PromptEvent::Cancel);
            }))
            .on_action(cx.listener(|this, _: &PromptBackspace, _window, cx| {
                this.backspace();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PromptDelete, _window, cx| {
                this.delete_forward();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PromptLeft, _window, cx| {
                this.step_caret(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PromptRight, _window, cx| {
                this.step_caret(1);
                cx.notify();
            }))
            .child(
                div()
                    .text_xs()
                    .mb_1()
                    .text_color(gpui::rgb(self.palette.dim))
                    .child(self.label.clone()),
            )
            .child(PromptField {
                prompt: cx.entity(),
                palette: Rc::clone(&self.palette),
                family: self.family.clone(),
                size: self.size,
            })
    }
}

/// The field's painted form: a shaded box, the text, a caret, and the input
/// handler registration that makes printable characters reach [`Prompt`].
struct PromptField {
    prompt: Entity<Prompt>,
    palette: Rc<Palette>,
    family: SharedString,
    size: f32,
}

impl IntoElement for PromptField {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PromptField {
    type RequestLayoutState = ();
    type PrepaintState = ShapedLine;

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
        let _ = cx;
        let mut style = ElementStyle::default();
        style.size.width = relative(1.).into();
        style.size.height = px(theme::line_height(self.size) + PAD * 2.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let text = self.prompt.read(cx).painted();
        let font = font(self.family.clone());
        let run = RunStyle::plain(&self.palette).text_run(text.len(), &font);
        window
            .text_system()
            .shape_line(text, px(self.size), &[run], None)
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
        window.paint_quad(fill(bounds, rgb(self.palette.code_block_bg)));
        window.paint_quad(fill(
            Bounds::from_corners(
                gpui::point(bounds.left(), bounds.bottom() - px(1.)),
                gpui::point(bounds.right(), bounds.bottom()),
            ),
            rgb(self.palette.ui_border),
        ));

        let origin = gpui::point(bounds.left() + px(PAD), bounds.top() + px(PAD));
        if let Err(err) = prepaint.paint(
            origin,
            px(theme::line_height(self.size)),
            gpui::TextAlign::default(),
            None,
            window,
            cx,
        ) {
            tracing::warn!("failed to paint the prompt field: {err:#}");
        }

        let caret = self.prompt.read(cx).caret;
        let x = origin.x + prepaint.x_for_index(caret.min(self.prompt.read(cx).text.len()));
        window.paint_quad(fill(
            Bounds::from_corners(
                gpui::point(x, bounds.top() + px(PAD)),
                gpui::point(x + px(1.5), bounds.bottom() - px(PAD)),
            ),
            rgb(self.palette.cursor),
        ));

        // Typed characters only reach a handler registered during this frame's
        // paint, with this frame's bounds.
        let focus = self.prompt.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.prompt.clone()),
            cx,
        );
    }
}
