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
    fill, font, point, px, relative, rgb, App, Bounds, ClipboardItem, Corners, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString,
    Style as ElementStyle, UTF16Selection, Window,
};
use shale_core::buffer::{word_back, word_forward};
use shale_core::theme::Palette;

use crate::style::CHIP_RADIUS;
use crate::theme::{self, RunStyle};

/// Padding inside the field, in pixels: [`crate::style::SPACE_1`]'s step.
///
/// An `f32` rather than a `Pixels` because most of what the field does with it is
/// hit-testing arithmetic against a character width, where the newtype would only
/// get unwrapped again.
pub(crate) const PAD: f32 = 4.;

/// A single line of text with a caret and, optionally, a selection.
///
/// The selection is one number — `anchor`, the end that stopped moving — with the
/// caret as the other end. That is how a keyboard selection actually works: shift
/// extends from wherever the key was pressed, and any plain motion leaves one
/// character behind. It also means no edit has to ask which end is which: the range
/// is normalised at the one place that reads it.
pub(crate) struct Field {
    /// What the field holds so far.
    text: SharedString,
    /// Byte offset in `text` where the caret sits. gpui's input handler speaks
    /// utf-16, so every conversion goes through the two offset helpers.
    caret: usize,
    /// The far end of a selection, if there is one. `None` is the usual state: the
    /// caret selecting nothing.
    anchor: Option<usize>,
}

impl Field {
    pub(crate) fn new(initial: &str) -> Self {
        let text = SharedString::from(initial.to_string());
        Self {
            caret: text.len(),
            text,
            anchor: None,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    // ----- selection -----

    /// The selected range, low offset first, or `None` when nothing is selected.
    /// An anchor equal to the caret is not a selection: it is what a shift motion
    /// leaves at the ends of the line, where there is nothing to extend over.
    pub(crate) fn selection(&self) -> Option<Range<usize>> {
        let anchor = self.anchor?;
        if anchor == self.caret {
            return None;
        }
        Some(anchor.min(self.caret)..anchor.max(self.caret))
    }

    /// The selected range, and the anchor cleared: what every edit calls before it
    /// changes the text, so "type over a selection" is the default rather than a
    /// case each edit has to remember.
    fn take_selection(&mut self) -> Option<Range<usize>> {
        let range = self.selection();
        self.anchor = None;
        range
    }

    pub(crate) fn selected_text(&self) -> Option<String> {
        let range = self.selection()?;
        Some(self.text[range].to_string())
    }

    /// Extend the selection one character left, starting one at the caret if there
    /// is no selection yet.
    pub(crate) fn select_left(&mut self) {
        self.anchor = Some(self.anchor.unwrap_or(self.caret));
        self.step_caret_unselected(-1);
    }

    pub(crate) fn select_right(&mut self) {
        self.anchor = Some(self.anchor.unwrap_or(self.caret));
        self.step_caret_unselected(1);
    }

    /// The start and end of the line. A one-line field has no lines above or below,
    /// so `up`/`down` mean these, the way they do in a shell's line editor.
    pub(crate) fn caret_to_start(&mut self) {
        self.anchor = None;
        self.caret = 0;
    }

    pub(crate) fn caret_to_end(&mut self) {
        self.anchor = None;
        self.caret = self.text.len();
    }

    pub(crate) fn select_to_start(&mut self) {
        self.anchor = Some(self.anchor.unwrap_or(self.caret));
        self.caret = 0;
    }

    pub(crate) fn select_to_end(&mut self) {
        self.anchor = Some(self.anchor.unwrap_or(self.caret));
        self.caret = self.text.len();
    }

    /// `ctrl`+`left`/`right`: the word boundary the caret lands on. A plain motion
    /// leaves any selection behind, as every other motion here does.
    pub(crate) fn caret_word_left(&mut self) {
        self.anchor = None;
        self.caret = word_back(&self.text, self.caret);
    }

    pub(crate) fn caret_word_right(&mut self) {
        self.anchor = None;
        self.caret = word_forward(&self.text, self.caret);
    }

    /// `ctrl`+`shift`+`left`/`right`: the selection grows by a word. Only the caret
    /// end moves — the anchor stays where the key was first pressed, which is the
    /// whole selection model in one line.
    pub(crate) fn select_word_left(&mut self) {
        self.anchor = Some(self.anchor.unwrap_or(self.caret));
        self.caret = word_back(&self.text, self.caret);
    }

    pub(crate) fn select_word_right(&mut self) {
        self.anchor = Some(self.anchor.unwrap_or(self.caret));
        self.caret = word_forward(&self.text, self.caret);
    }

    /// The whole line, which is what `ctrl-a` means here. In a shell's line editor
    /// `ctrl-a` is start-of-line; `home` already is that, and the field keeps both.
    pub(crate) fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.len();
    }

    /// Delete what is selected, if anything: `ctrl-x` after putting the text on the
    /// clipboard.
    pub(crate) fn delete_selection(&mut self) {
        let Some(range) = self.take_selection() else {
            return;
        };
        let mut new = self.text.to_string();
        new.replace_range(range.clone(), "");
        self.caret = range.start;
        self.text = SharedString::from(new);
    }

    /// The selection as character indices, which is what the painted band wants.
    pub(crate) fn selection_char_range(&self) -> Option<(usize, usize)> {
        let range = self.selection()?;
        Some((
            self.text[..range.start].chars().count(),
            self.text[..range.end].chars().count(),
        ))
    }

    /// Type at the caret. A selection is what gets typed over: that is the whole
    /// point of one, and every other edit in this file starts the same way.
    pub(crate) fn insert(&mut self, text: &str) {
        let mut new = self.text.to_string();
        if let Some(range) = self.take_selection() {
            new.replace_range(range.clone(), "");
            self.caret = range.start;
        }
        new.replace_range(self.caret..self.caret, text);
        self.caret += text.len();
        self.text = SharedString::from(new);
    }

    pub(crate) fn backspace(&mut self) {
        if let Some(range) = self.take_selection() {
            let mut new = self.text.to_string();
            new.replace_range(range.clone(), "");
            self.caret = range.start;
            self.text = SharedString::from(new);
            return;
        }
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
        if let Some(range) = self.take_selection() {
            let mut new = self.text.to_string();
            new.replace_range(range.clone(), "");
            self.caret = range.start;
            self.text = SharedString::from(new);
            return;
        }
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

    /// Delete the word before the caret — `ctrl-backspace`. The boundary comes
    /// from [`word_back`], the same one the note editor's backward word motion
    /// uses, so a word means the same thing in a query as in a note: the blanks
    /// just before the caret go with the word in front of them.
    pub(crate) fn delete_word_back(&mut self) {
        if let Some(range) = self.take_selection() {
            let mut new = self.text.to_string();
            new.replace_range(range.clone(), "");
            self.caret = range.start;
            self.text = SharedString::from(new);
            return;
        }
        let start = word_back(&self.text, self.caret);
        if start >= self.caret {
            return;
        }
        let mut new = self.text.to_string();
        new.replace_range(start..self.caret, "");
        self.caret = start;
        self.text = SharedString::from(new);
    }

    /// Move the caret by `delta` characters, clamped to the line.
    ///
    /// With a selection out, a plain motion does not step: it leaves the selection
    /// at the end the motion was heading for, which is what a user reading a wrong
    /// query expects from one arrow key.
    pub(crate) fn step_caret(&mut self, delta: isize) {
        if let Some(range) = self.selection() {
            self.caret = if delta < 0 { range.start } else { range.end };
            self.anchor = None;
            return;
        }
        self.step_caret_unselected(delta);
    }

    /// The step itself, for a caret that is not sitting on a selection.
    fn step_caret_unselected(&mut self, delta: isize) {
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

    /// The selection gpui should see — the real one, not the caret. The platform
    /// hands this back to [`Field::replace_utf16`] when an IME confirms or a paste
    /// lands, so a field that reported only its caret would insert beside a
    /// selection instead of replacing it.
    pub(crate) fn caret_selection(&self) -> UTF16Selection {
        let range = self.selection();
        UTF16Selection {
            range: self.offset_to_utf16(range.as_ref().map_or(self.caret, |r| r.start))
                ..self.offset_to_utf16(range.as_ref().map_or(self.caret, |r| r.end)),
            // `reversed` is gpui's way of saying the caret is at the range's start:
            // the anchor, the end that stopped moving, is at the other one.
            reversed: self.anchor.is_some_and(|anchor| anchor > self.caret),
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
        // A typed character reaches here with no range at all, so the field's own
        // selection is the only thing that can say what it should land on. Without
        // this, `shift-left` followed by a key inserts beside the selection and the
        // selection survives the key that was meant to consume it.
        let range = range.map(|r| self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end));
        let range = range.or_else(|| self.selection());
        self.anchor = None;
        if let Some(range) = range {
            let start = range.start;
            let end = range.end;
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
    /// The same field, for the keys that change it.
    fn field_mut(&mut self) -> &mut Field;
    /// The colours the field is drawn with.
    fn palette(&self) -> &Rc<Palette>;
    /// The resolved font family, so the field matches the note text.
    fn family(&self) -> SharedString;
    /// Text size in points.
    fn size(&self) -> f32;
    /// The focus the input handler is registered for.
    fn focus(&self) -> FocusHandle;
}

/// The selection and clipboard keys every field answers, wired onto the element that
/// carries its key context.
///
/// One macro because it is one widget. The prompt, the pickers and the graph's filter
/// each kept their own hand-copied table of field keys, and that is how the graph's
/// filter ended up with a field that could not reach the ends of its own line. The
/// text-editing keys (`backspace`, the arrows) stay where they were because their
/// follow-up differs; what is shared here is the whole set that has to agree.
///
/// `$after` is the owner's chance to react to the text changing: a picker reranks its
/// rows and re-emits the highlight, the prompt and the graph filter do nothing.
#[macro_export]
macro_rules! field_key_actions {
    ($el:expr, $cx:expr, $after:path) => {
        $el.on_action(
            $cx.listener(|this, _: &$crate::acts::FieldSelectLeft, _window, cx| {
                this.field_mut().select_left();
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldSelectRight, _window, cx| {
                this.field_mut().select_right();
                cx.notify();
            }),
        )
        // A one-line field has no line above or below, so `shift-up` and
        // `shift-down` are the ends of the line — which is what a shell's line
        // editor does with them, and what a user reaching for `shift-home`
        // expects when they overshoot.
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldWordLeft, _window, cx| {
                this.field_mut().caret_word_left();
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldWordRight, _window, cx| {
                this.field_mut().caret_word_right();
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldSelectWordLeft, _window, cx| {
                this.field_mut().select_word_left();
                cx.notify();
            }),
        )
        .on_action($cx.listener(
            |this, _: &$crate::acts::FieldSelectWordRight, _window, cx| {
                this.field_mut().select_word_right();
                cx.notify();
            },
        ))
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldSelectToStart, _window, cx| {
                this.field_mut().select_to_start();
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldSelectToEnd, _window, cx| {
                this.field_mut().select_to_end();
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldHome, _window, cx| {
                this.field_mut().caret_to_start();
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldEnd, _window, cx| {
                this.field_mut().caret_to_end();
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldSelectAll, _window, cx| {
                this.field_mut().select_all();
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldCopy, _window, cx| {
                if !$crate::field::copy(this, cx) {
                    // Nothing selected: the key does nothing, and says nothing. A
                    // field that cleared itself on a stray `ctrl-c` would be worse
                    // than one that ignored it.
                    return;
                }
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldCut, _window, cx| {
                if !$crate::field::cut(this, cx) {
                    return;
                }
                $after(this, cx);
                cx.notify();
            }),
        )
        .on_action(
            $cx.listener(|this, _: &$crate::acts::FieldPaste, _window, cx| {
                if !$crate::field::paste(this, cx) {
                    return;
                }
                $after(this, cx);
                cx.notify();
            }),
        )
    };
}

// ----- the clipboard, shared by every field -----
//
// The system clipboard, the same one the note editor's `y` writes: a query typed in
// a field should be able to leave Shale, and a query somebody copied elsewhere should
// be able to arrive in one. `false` is the answer when there was nothing to do, which
// is the caller's cue to leave the field alone rather than repaint it.

pub(crate) fn copy(owner: &impl FieldOwner, cx: &mut App) -> bool {
    let Some(text) = owner.field().selected_text() else {
        return false;
    };
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    true
}

pub(crate) fn cut(owner: &mut impl FieldOwner, cx: &mut App) -> bool {
    let Some(text) = owner.field().selected_text() else {
        return false;
    };
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    owner.field_mut().delete_selection();
    true
}

pub(crate) fn paste(owner: &mut impl FieldOwner, cx: &mut App) -> bool {
    let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
        return false;
    };
    // `insert` is the whole operation: it types over a selection when there is one,
    // which is what a paste onto selected text means.
    owner.field_mut().insert(&text);
    true
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
        // A field is a control in the window, so it takes the chrome's field
        // role rather than the note's inline-code background.
        //
        // Rounded along the top only, at the radius every other chip in the window
        // uses: the underline below is the field's edge, and rounding the bottom
        // corners would pull the fill away from it and leave the line floating.
        let mut background = fill(bounds, rgb(palette.ui_field_bg));
        background.corner_radii = Corners {
            top_left: CHIP_RADIUS,
            top_right: CHIP_RADIUS,
            bottom_left: px(0.),
            bottom_right: px(0.),
        };
        window.paint_quad(background);
        // The field's edge is the rule that says whether the window's keyboard is
        // on it: `ui_active_border` while it holds the keyboard, `ui_border`
        // otherwise (`design-language.md` §4). A field is normally the focused
        // control, so this is mostly the accent -- but a picker whose rows hold
        // the keyboard, or a second window, is the case where it is not.
        let focused = self.owner.read(cx).focus().is_focused(window);
        window.paint_quad(fill(
            Bounds::from_corners(
                point(bounds.left(), bounds.bottom() - px(1.)),
                point(bounds.right(), bounds.bottom()),
            ),
            rgb(if focused {
                palette.ui_active_border
            } else {
                palette.ui_border
            }),
        ));

        let origin = point(bounds.left() + px(PAD), bounds.top() + px(PAD));
        // The band goes *under* the text. A selection that hides the letters it
        // selected is worse than the mistake it was meant to explain, and the note
        // editor draws its selection the same way (§4).
        if let Some((start, end)) = self.owner.read(cx).field().selection_char_range() {
            window.paint_quad(fill(
                Bounds::from_corners(
                    point(
                        origin.x + prepaint.x_for_index(start),
                        bounds.top() + px(PAD),
                    ),
                    point(
                        origin.x + prepaint.x_for_index(end),
                        bounds.bottom() - px(PAD),
                    ),
                ),
                rgb(palette.selection),
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::Field;

    /// `ctrl`+`shift`+arrow grows the selection a word at a time, and `ctrl`+arrow
    /// moves the caret by a word and leaves the selection behind — the pair every text
    /// field has, which this one did not.
    #[test]
    fn words_are_the_unit_ctrl_moves_and_selects() {
        let mut field = Field::new("tag:monster order:cr");
        field.select_word_left();
        assert_eq!(
            field.selected_text().as_deref(),
            Some("cr"),
            "one key takes the word before the caret"
        );
        field.select_word_left();
        field.select_word_left();
        assert_eq!(
            field.selected_text().as_deref(),
            Some("order:cr"),
            "and keeps going, run by run: the word, the colon, the word again"
        );

        // A plain motion drops the selection rather than dragging it along, and crosses
        // the blank to the word before it — the same boundary `ctrl-backspace` deletes
        // from, because it is the same question.
        field.caret_word_left();
        assert_eq!(
            field.selection(),
            None,
            "the motion left the selection behind"
        );
        assert_eq!(field.caret, 4, "the start of `monster`");
        field.caret_word_right();
        assert_eq!(
            field.caret, 11,
            "and back at the blank's start, going the other way"
        );
    }

    /// The selection is one anchor with the caret as the other end, so the range is
    /// normalised at the one place that reads it: a selection made leftwards reads
    /// the same as one made rightwards.
    #[test]
    fn a_selection_reads_the_same_whichever_way_it_was_made() {
        let mut left = Field::new("goblin");
        left.select_left();
        left.select_left();
        assert_eq!(
            left.selection(),
            Some(4..6),
            "made leftwards, still reported low offset first"
        );
        assert_eq!(left.selected_text().as_deref(), Some("in"));

        let mut right = Field::new("goblin");
        right.caret_to_start();
        right.select_right();
        right.select_right();
        assert_eq!(right.selection(), Some(0..2));
        assert_eq!(right.selected_text().as_deref(), Some("go"));
    }

    /// What the platform sees has to be the real selection, in utf-16, with the
    /// caret's end marked: an IME confirming a candidate and a platform paste both
    /// replace what this reports.
    #[test]
    fn the_selection_reported_to_gpui_is_the_real_one() {
        let mut field = Field::new("one\n"); // a field is one line; the text is just bytes
        field.caret_to_start();
        field.select_right();
        let forward = field.caret_selection();
        assert_eq!(forward.range, 0..1);
        assert!(!forward.reversed, "the caret is at the range's end");

        let mut field = Field::new("goblin");
        field.select_left();
        let back = field.caret_selection();
        assert_eq!(back.range, 5..6);
        assert!(back.reversed, "the anchor is behind the caret");
    }

    /// A field can hold a name or a query with non-ASCII in it, and both the utf-16
    /// translation and the character step have to agree about where the caret is.
    #[test]
    fn a_selection_over_a_wide_character_is_still_one_character() {
        let mut field = Field::new("gobelin");
        // Replace the ASCII 'i' with a two-byte character through the platform path.
        field.replace_utf16(Some(5..6), "é");
        assert_eq!(field.text(), "gobelén");
        field.select_left();
        assert_eq!(
            field.selected_text().as_deref(),
            Some("é"),
            "one character, not one byte"
        );
        // The painted band wants character indices, not bytes.
        assert_eq!(field.selection_char_range(), Some((5, 6)));
    }

    /// Every edit consumes the selection first, so "type over a selection" is the
    /// default rather than a case each edit has to remember.
    #[test]
    fn every_edit_types_over_the_selection() {
        type Edit = (&'static str, Box<dyn Fn(&mut Field)>);
        let cases: [Edit; 4] = [
            ("g", Box::new(|f| f.insert("g"))),
            ("", Box::new(|f| f.backspace())),
            ("", Box::new(|f| f.delete_forward())),
            ("", Box::new(|f| f.delete_word_back())),
        ];
        for (expected, edit) in cases {
            let mut field = Field::new("goblin");
            field.caret_to_start();
            field.select_to_end();
            edit(&mut field);
            assert_eq!(
                field.text(),
                expected,
                "the whole line went, then {expected}"
            );
            assert_eq!(field.selection(), None, "and the selection went with it");
        }
    }
}
