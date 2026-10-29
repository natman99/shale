//! The note's three views: raw source, live preview, and fully rendered preview.
//!
//! The decision itself lives in the editor ([`crate::editor::View`]), because
//! everything a view changes is inside the frame the editor builds — which spans
//! are computed, which rows hide their markers, whether tables are grids and
//! embeds are pictures, and whether the buffer takes edits at all.
//!
//! What the window owns is the way in and the way out: the palette rows, the
//! command line's words, and the status bar that says which view the note is in
//! when it is not the default one. Remembering the choice in the vault's config
//! is the part that has to wait for the config work.

use gpui::{Context, Window};

use crate::acts::{ViewLive, ViewPreview, ViewRaw};
use crate::editor::View;

use super::Workspace;

impl Workspace {
    /// `:raw`, and its palette row: the markdown source, with nothing rendered.
    pub(super) fn show_raw(&mut self, _: &ViewRaw, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_view(View::Raw, cx);
    }

    /// `:live`, and its palette row: Shale's default, rendered except on the rows
    /// the user is working on.
    pub(super) fn show_live(&mut self, _: &ViewLive, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_view(View::Live, cx);
    }

    /// `:preview`, and its palette row: rendered all the way down, and the note
    /// stops taking edits.
    pub(super) fn show_preview(
        &mut self,
        _: &ViewPreview,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_view(View::Preview, cx);
    }

    /// Put the note in `view` and say so where the user is looking. The editor
    /// owns what that means for the frame, including making the buffer read-only
    /// for preview; the window only reports, because a view that changes nothing
    /// (`:live` when live is already on) is worth one honest message.
    pub(super) fn show_view(&mut self, view: View, cx: &mut Context<Self>) {
        let already = self.editor.read(cx).view() == view;
        self.editor
            .update(cx, |editor, cx| editor.set_view(view, cx));
        self.status = if already {
            format!("already in the {} view", view.word()).into()
        } else {
            format!("{} view", view.word()).into()
        };
        cx.notify();
    }

    /// Which view the note is in. The status bar draws its own badge from the
    /// editor; this is what the tests ask.
    #[cfg(test)]
    pub(crate) fn view(&self, cx: &gpui::App) -> View {
        self.editor.read(cx).view()
    }
}
