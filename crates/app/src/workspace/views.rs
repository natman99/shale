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
//! and the status bar's badge naming the view. Remembering the choice across a
//! restart is the session file's job (roadmap P1.8), not the config's.
//!
//! Focus mode and typewriter scroll live here too. They are not views — the note
//! is still live preview, still editable — but they are the same kind of thing:
//! a decision the frame makes that the vim engine knows nothing about, reached
//! from the palette, the command line and one leader chord. They are session state
//! as well, which is why neither is a config key.

use gpui::{Context, Window};

use crate::acts::{ToggleFocus, ToggleTypewriter, ViewLive, ViewPreview, ViewRaw};
use crate::dim;
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

    /// Whether focus mode is on, and how far it reaches. The badge reads the
    /// editor directly; these are for the tests.
    #[cfg(test)]
    pub(crate) fn focus_mode(&self, cx: &gpui::App) -> bool {
        self.editor.read(cx).focus_mode()
    }

    /// The scope focus mode lights: `"block"` or `"section"`.
    #[cfg(test)]
    pub(crate) fn focus_scope(&self, cx: &gpui::App) -> String {
        self.editor.read(cx).focus_scope().word().to_string()
    }

    /// Whether typewriter scroll is on.
    #[cfg(test)]
    pub(crate) fn typewriter(&self, cx: &gpui::App) -> bool {
        self.editor.read(cx).typewriter()
    }

    // ----- focus mode and typewriter scroll -------------------------------

    /// `space d`, and its palette row: turn focus mode around. The way out is the
    /// same two keys as the way in, which is what a toggle is for.
    pub(super) fn toggle_focus(
        &mut self,
        _: &ToggleFocus,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flip_focus(cx);
    }

    /// Focus mode, told rather than toggled: `:focus on|off`.
    pub(crate) fn set_focus(&mut self, on: bool, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.set_focus_mode(on, cx));
        // The status bar's badge is the only place the mode is reported, so the
        // command line says nothing: `:focus on` next to a badge saying `focus
        // block` would be the same label twice.
        cx.notify();
    }

    /// Focus mode, asked rather than told: a bare `:focus`.
    pub(crate) fn flip_focus(&mut self, cx: &mut Context<Self>) {
        let on = !self.editor.read(cx).focus_mode();
        self.set_focus(on, cx);
    }

    /// `:focus block|section`: how far the lit region reaches. The mode itself is
    /// left alone, because asking how wide the light should be is not a request to
    /// turn it on. Unlike `:focus on|off` this one does report in the status line:
    /// the badge that would otherwise say it only appears while the mode is on.
    pub(crate) fn set_focus_scope(&mut self, word: &str, cx: &mut Context<Self>) {
        let scope = dim::Scope::from_word(word);
        self.editor
            .update(cx, |editor, cx| editor.set_focus_scope(scope, cx));
        // `sentence` is named on the roadmap and not built, and `from_word` answers
        // it with `block`. Saying so is the difference between a command that did
        // something surprising and one that admits it.
        let message = if scope.word() != word {
            format!(
                "focus scope {word} is not built yet; using {}",
                scope.word()
            )
        } else {
            format!("focus scope {}", scope.word())
        };
        self.say(message, cx);
        cx.notify();
    }

    /// `:typewriter`, and its palette row: keep the cursor's line near the middle
    /// of the window. The bare `:typewriter` reaches this through the same action as
    /// the palette row does, so there is one flip, and `set_typewriter` is the one
    /// place that reports it in the status line.
    pub(super) fn toggle_typewriter(
        &mut self,
        _: &ToggleTypewriter,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let on = !self.editor.read(cx).typewriter();
        self.set_typewriter(on, cx);
    }

    /// Typewriter scroll, on or off. Unlike focus mode it has no badge — nothing
    /// about it is visible in the text — so the status line is the only place it
    /// can be reported, and the command line reports it there.
    pub(crate) fn set_typewriter(&mut self, on: bool, cx: &mut Context<Self>) {
        let was = self.editor.read(cx).typewriter();
        self.editor
            .update(cx, |editor, cx| editor.set_typewriter(on, cx));
        self.say(
            if was == on {
                format!("typewriter scroll is already {}", on_off(on))
            } else {
                format!("typewriter scroll {}", on_off(on))
            },
            cx,
        );
        cx.notify();
    }
}

/// The status line's word for a mode being on or off.
fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}
