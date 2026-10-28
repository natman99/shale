//! The one-line text field the prompt and the pickers share.
//!
//! Both need the same three things: a line of text with a caret that edits by
//! character rather than byte, the utf-16 translation gpui's input handler
//! speaks, and a painted form with a caret in it. The entity that owns a
//! [`Field`] implements [`EntityInputHandler`] on top of the helpers here, and
//! draws itself with [`FieldElement`] — a separate element because gpui only
//! routes typed characters to a handler registered during the current frame's
//! paint, which is the one moment the field's bounds are known.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    fill, font, point, px, relative, rgb, App, Bounds, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, ShapedLine, SharedString, Style as ElementStyle, UTF16Selection, Window,
};
use shale_core::theme::Palette;

use crate::theme::{self, RunStyle};

/// Padding inside the field, in pixels.
pub(crate) const PAD: f32 = 4.;

/// A single line of text with a caret.
pub(crate) struct Field {
    /// What the field holds so far.
    text: SharedString,
    /// Byte offset in `text` where the caret sits. gpui's input handler speaks
    /// utf-16, so every conversion goes through the two offset helpers.
    caret: usize,
}

impl Field {
    pub(crate) fn new(initial: &str) -> Self {
        let text = SharedString::from(initial.to_string());
        Self {
            caret: text.len(),
            text,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Byte offset of the caret.
    pub(crate) fn caret(&self) -> usize {
        self.caret
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replace the whole line and put the caret at its end.
    pub(crate) fn set(&mut self, text: &str) {
        self.text = SharedString::from(text.to_string());
        self.caret = self.text.len();
    }

    pub(crate) fn insert(&mut self, text: &str) {
        let mut new = self.text.to_string();
        new.replace_range(self.caret..self.caret, text);
        self.caret += text.len();
        self.text = SharedString::from(new);
    }

    pub(crate) fn backspace(&mut self) {
        if self.caret == 0 {
            return;
        }
        let mut new = self.text.to_string();
        // One *character*, not one byte: a name or a query can hold non-ASCII text.
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

    pub(crate) fn delete_forward(&mut self) {
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

    /// Move the caret by `delta` characters, clamped to the line.
    pub(crate) fn step_caret(&mut self, delta: isize) {
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

    /// The caret as a character index, which is what a shaped line wants.
    pub(crate) fn caret_char_index(&self) -> usize {
        self.text[..self.caret].chars().count()
    }

    /// The text the field paints: the caret always sits after a character, so an
    /// empty field still needs something to shape.
    pub(crate) fn painted(&self) -> SharedString {
        if self.text.is_empty() {
            SharedString::from(" ")
        } else {
            self.text.clone()
        }
    }

    // ----- utf16 <-> utf8 helpers (gpui's input handler speaks utf-16) -----

    pub(crate) fn offset_from_utf16(&self, offset: usize) -> usize {
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

    pub(crate) fn offset_to_utf16(&self, offset: usize) -> usize {
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

    /// The selection gpui should see: the caret, selecting nothing.
    pub(crate) fn caret_selection(&self) -> UTF16Selection {
        UTF16Selection {
            range: self.offset_to_utf16(self.caret)..self.offset_to_utf16(self.caret),
            reversed: false,
        }
    }

    /// The text inside a utf-16 range, and the same range back (the handler is
    /// required to report what it actually used).
    pub(crate) fn text_in_utf16(&self, range: Range<usize>) -> (String, Range<usize>) {
        let start = self.offset_from_utf16(range.start);
        let end = self.offset_from_utf16(range.end);
        (self.text[start..end].to_string(), range)
    }

    /// Apply what the platform handed us: a typed character, a paste, or an IME
    /// confirmation. A field is one line, so anything carrying a newline is
    /// flattened to its first line.
    pub(crate) fn replace_utf16(&mut self, range: Option<Range<usize>>, new_text: &str) {
        let first_line = new_text.lines().next().unwrap_or("");
        if let Some(range) = range {
            let start = self.offset_from_utf16(range.start);
            let end = self.offset_from_utf16(range.end);
            let mut new = self.text.to_string();
            new.replace_range(start..end, first_line);
            self.caret = start + first_line.len();
            self.text = SharedString::from(new);
        } else {
            self.insert(first_line);
        }
    }

    /// Which character a click at `x` (relative to the field's left edge) landed
    /// on, as a utf-16 offset.
    pub(crate) fn utf16_index_for_x(&self, x: f32, char_width: f32) -> usize {
        let column = ((x - PAD) / char_width).max(0.).round() as usize;
        let offset = self
            .text
            .char_indices()
            .map(|(i, _)| i)
            .nth(column)
            .unwrap_or(self.text.len());
        self.offset_to_utf16(offset)
    }
}

/// What [`FieldElement`] needs from the entity that owns a [`Field`], so the
/// field can be painted and wired to the input handler without knowing whether
/// the owner is a prompt or a picker.
pub(crate) trait FieldOwner {
    /// The line of text and its caret.
    fn field(&self) -> &Field;
    /// The colours the field is drawn with.
    fn palette(&self) -> &Rc<Palette>;
    /// The resolved font family, so the field matches the note text.
    fn family(&self) -> SharedString;
    /// Text size in points.
    fn size(&self) -> f32;
    /// The focus the input handler is registered for.
    fn focus(&self) -> FocusHandle;
}

/// Rough advance of one character, used only for the two geometry questions the
/// IME asks (where a candidate window goes, and which character a click landed
/// on). The field's own painting uses the shaped line.
pub(crate) fn approx_char_width() -> f32 {
    f32::from(px(8.))
}

/// The painted field: a shaded box, the text, the caret, and the
/// [`ElementInputHandler`] registration that makes printable characters reach
/// its owner.
pub(crate) struct FieldElement<E> {
    owner: Entity<E>,
}

impl<E: FieldOwner + EntityInputHandler + 'static> FieldElement<E> {
    pub(crate) fn new(owner: Entity<E>) -> Self {
        Self { owner }
    }
}

impl<E: FieldOwner + EntityInputHandler + 'static> IntoElement for FieldElement<E> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<E: FieldOwner + EntityInputHandler + 'static> Element for FieldElement<E> {
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
        let size = self.owner.read(cx).size();
        let mut style = ElementStyle::default();
        style.size.width = relative(1.).into();
        style.size.height = px(theme::line_height(size) + PAD * 2.).into();
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
        let owner = self.owner.read(cx);
        let text = owner.field().painted();
        let family = owner.family();
        let size = owner.size();
        let run = RunStyle::plain(owner.palette()).text_run(text.len(), &font(family));
        window
            .text_system()
            .shape_line(text, px(size), &[run], None)
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
        let palette = Rc::clone(self.owner.read(cx).palette());
        window.paint_quad(fill(bounds, rgb(palette.code_block_bg)));
        window.paint_quad(fill(
            Bounds::from_corners(
                point(bounds.left(), bounds.bottom() - px(1.)),
                point(bounds.right(), bounds.bottom()),
            ),
            rgb(palette.ui_border),
        ));

        let origin = point(bounds.left() + px(PAD), bounds.top() + px(PAD));
        let size = self.owner.read(cx).size();
        if let Err(err) = prepaint.paint(
            origin,
            px(theme::line_height(size)),
            gpui::TextAlign::default(),
            None,
            window,
            cx,
        ) {
            tracing::warn!("failed to paint the field: {err:#}");
        }

        let caret = self.owner.read(cx).field().caret_char_index();
        let x = origin.x + prepaint.x_for_index(caret);
        window.paint_quad(fill(
            Bounds::from_corners(
                point(x, bounds.top() + px(PAD)),
                point(x + px(1.5), bounds.bottom() - px(PAD)),
            ),
            rgb(palette.cursor),
        ));

        // Typed characters only reach a handler registered during this frame's
        // paint, with this frame's bounds.
        let focus = self.owner.read(cx).focus();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.owner.clone()),
            cx,
        );
    }
}
