//! The note's three views: raw source, live preview, and fully rendered preview.
//!
//! The decision itself lives in the editor ([`crate::editor::View`]), because
//! everything a view changes is inside the frame the editor builds — which spans
//! are computed, which rows hide their markers, whether the checker's underlines
//! are drawn, whether tables are grids and embeds are pictures, and whether the
//! buffer takes edits at all.
//!
//! What the window owns is the way in and the two things that live outside the
//! editor: a panel about a checker's note, which has to go when preview comes on,
//! and the status bar's badge naming the view. Remembering the choice in the
//! vault's config is the part that has to wait for the config work.

use gpui::{Context, Window};

use crate::acts::{ViewLive, ViewPreview, ViewRaw};
use crate::editor::View;

use super::Workspace;

impl Workspace {
    /// `:raw`, and its palette row: the markdown source, with nothing rendered.
    pub(super) fn show_raw(&mut self, _: &ViewRaw, window: &mut Window, cx: &mut Context<Self>) {
        self.show_view(View::Raw, window, cx);
    }

    /// `:live`, and its palette row: Shale's default, rendered except on the rows
    /// the user is working on.
    pub(super) fn show_live(&mut self, _: &ViewLive, window: &mut Window, cx: &mut Context<Self>) {
        self.show_view(View::Live, window, cx);
    }

    /// `:preview`, and its palette row: rendered all the way down, and the note
    /// stops taking edits.
    pub(super) fn show_preview(
        &mut self,
        _: &ViewPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_view(View::Preview, window, cx);
    }

    /// Put the note in `view`.
    ///
    /// The editor owns what that means for the frame — the read-only buffer, the
    /// hidden markers, the hidden checker underlines. Two things live outside it:
    /// a panel about a checker's note has to go when preview comes on, because
    /// every row it offers is an edit the view now refuses; and the status bar's
    /// badge, which is the *only* place the view is reported. The command line
    /// deliberately says nothing, because a message naming the view next to a
    /// badge naming the view is the same label twice.
    pub(super) fn show_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if view == View::Preview {
            self.close_popup(window, cx);
        }
        self.editor
            .update(cx, |editor, cx| editor.set_view(view, cx));
        cx.notify();
    }

    /// Which view the note is in. The status bar reads the editor's own answer;
    /// this is what the tests ask.
    #[cfg(test)]
    pub(crate) fn view(&self, cx: &gpui::App) -> View {
        self.editor.read(cx).view()
    }
}
