//! A one-line text prompt: the name for a new note, the new name in a rename, the
//! confirmation before a delete.
//!
//! The editor's input surface (`editor::input`) is built for a multi-line buffer
//! with vim modes, so a prompt has its own small one: an entity that implements
//! gpui's [`EntityInputHandler`] over a [`Field`], plus a `prompt` key context for
//! the handful of keys that are not printable text. Printable characters arrive
//! through the input handler exactly as they do in the editor, which on Linux is
//! the only path that works for keys the keymap does not bind (see the
//! `ElementInputHandler` note in PROJECT.md).
//!
//! The field itself — box, text, caret and input-handler registration — is the
//! shared [`FieldElement`], which is also what the pickers draw.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    div, px, App, Bounds, Context, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Pixels, Point, Render, SharedString, Styled,
    UTF16Selection, Window,
};
use shale_core::theme::Palette;

use crate::acts::{FieldBackspace, FieldCancel, FieldDelete, FieldLeft, FieldRight, FieldSubmit};
use crate::field::{self, approx_char_width, Field, FieldElement, FieldOwner};

/// The key context the prompt's bindings live in. While it is open its keys win
/// over the editor's vim contexts, which is what lets `enter` mean "accept".
pub(crate) const CONTEXT: &str = "prompt";

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

/// A single-line text field with a caret, asking one question.
pub(crate) struct Prompt {
    focus_handle: FocusHandle,
    /// What the answer will be used for.
    kind: PromptKind,
    /// The question shown above the field.
    label: SharedString,
    /// What has been typed so far, and where the caret is.
    field: Field,
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
        Self {
            focus_handle: cx.focus_handle(),
            kind,
            label: label.into(),
            field: Field::new(initial),
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
        self.field.text()
    }
}

impl Focusable for Prompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FieldOwner for Prompt {
    fn field(&self) -> &Field {
        &self.field
    }

    fn palette(&self) -> &Rc<Palette> {
        &self.palette
    }

    fn family(&self) -> SharedString {
        self.family.clone()
    }

    fn size(&self) -> f32 {
        self.size
    }

    fn focus(&self) -> FocusHandle {
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
        let (text, used) = self.field.text_in_utf16(range_utf16);
        actual_range.replace(used);
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(self.field.caret_selection())
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
        self.field.replace_utf16(range_utf16, new_text);
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
        let offset = self.field.offset_from_utf16(range_utf16.start);
        let x = field::PAD + (offset as f32 * approx_char_width());
        Some(Bounds::from_corners(
            gpui::point(bounds.left() + px(x), bounds.top()),
            gpui::point(bounds.left() + px(x + 8.), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(
            self.field
                .utf16_index_for_x(f32::from(point.x), approx_char_width()),
        )
    }
}

impl Render for Prompt {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &FieldSubmit, _window, cx| {
                cx.emit(PromptEvent::Submit(this.field.text().to_string().into()));
            }))
            .on_action(cx.listener(|_, _: &FieldCancel, _window, cx| {
                cx.emit(PromptEvent::Cancel);
            }))
            .on_action(cx.listener(|this, _: &FieldBackspace, _window, cx| {
                this.field.backspace();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FieldDelete, _window, cx| {
                this.field.delete_forward();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FieldLeft, _window, cx| {
                this.field.step_caret(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FieldRight, _window, cx| {
                this.field.step_caret(1);
                cx.notify();
            }))
            .child(
                div()
                    .text_xs()
                    .mb_1()
                    .text_color(gpui::rgb(self.palette.dim))
                    .child(self.label.clone()),
            )
            .child(FieldElement::new(cx.entity()))
    }
}
