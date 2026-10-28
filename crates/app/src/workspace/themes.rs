//! Changing the palette while Shale runs, and previewing one while it is chosen.
//!
//! A palette is data — one `Rc<Palette>` the window shares with the editor — so
//! switching it is replacing that pointer and repainting, not restarting. That is
//! also what makes a *preview* cheap and reversible: the theme picker can draw the
//! whole window in the theme its highlighted row names, and `escape` puts back the
//! palette the picker was opened from.
//!
//! Two rules keep that from leaking:
//!
//! - **A preview is not an answer.** Until a row is chosen, the palette the window
//!   started with is remembered ([`Workspace::theme_backup`]) and restored when the
//!   picker closes without a choice.
//! - **One place decides what a name means.** Every path — the picker, `:theme
//!   <name>`, a preview — goes through [`shale_core::theme::palette_for`], so a
//!   built-in, a `catppuccin-` prefix, a `-dark` suffix and a vault theme file all
//!   resolve the same way here as they do at startup.

use std::rc::Rc;

use anyhow::Context as _;
use gpui::{Context, Window};
use shale_core::theme::{self, Palette};

use crate::acts::ChangeTheme;
use crate::picker::PickerItem;

use super::open::PickerTarget;
use super::Workspace;

impl Workspace {
    /// The theme picker: every built-in palette, then the vault's own theme
    /// files. The row the pointer or the selection is on is applied as a preview;
    /// `enter` keeps it and `escape` puts the old one back.
    pub(super) fn change_theme(
        &mut self,
        _: &ChangeTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut rows: Vec<(PickerItem, PickerTarget)> = Palette::BUILT_INS
            .iter()
            .map(|(name, _)| theme_row(name, "built-in"))
            .collect();
        rows.extend(
            theme::theme_names(self.vault.root())
                .iter()
                .map(|name| theme_row(name, "vault theme")),
        );
        // What `escape` goes back to. Set before the picker opens, because the
        // first preview happens as soon as the picker has a highlighted row.
        self.theme_backup = Some(Rc::clone(&self.palette));
        let label = format!("Change the theme (now: {})", self.theme_name);
        self.open_picker(&label, rows, window, cx);
    }

    /// `:theme <name>`: switch now, and say what is wrong with a name that is not
    /// a theme.
    pub(super) fn set_theme(&mut self, name: &str, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let palette = theme::palette_for(self.vault.root(), name)
            .with_context(|| format!("no theme {name:?}"))?;
        self.theme_backup = None;
        self.theme_name = name.to_string();
        self.apply_palette(Rc::new(palette), cx);
        Ok(())
    }

    /// Draw the window in `name`'s palette without committing to it.
    ///
    /// A name that does not resolve leaves everything as it is: a preview is not
    /// something the user typed, so there is nothing to correct and the status bar
    /// would only fill up with noise on every mouse-move.
    pub(super) fn preview_theme(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.theme_backup.is_none() {
            self.theme_backup = Some(Rc::clone(&self.palette));
        }
        let Ok(palette) = theme::palette_for(self.vault.root(), name) else {
            return;
        };
        // The same palette under a different name (an alias, or the row the
        // pointer crossed back over) is not a change, and repainting the whole
        // window for it is work for nothing.
        if palette == *self.palette {
            return;
        }
        self.apply_palette(Rc::new(palette), cx);
    }

    /// The preview the user chose: keep it, and remember its name.
    ///
    /// Clearing the backup is what stops the picker's close from putting the old
    /// palette back.
    pub(super) fn accept_theme(&mut self, name: &str, cx: &mut Context<Self>) {
        self.theme_backup = None;
        self.theme_name = name.to_string();
        cx.notify();
    }

    /// Put back the palette the theme picker started from, if a preview was in
    /// flight. Called when the picker closes without a choice.
    pub(super) fn revert_theme(&mut self, cx: &mut Context<Self>) {
        if let Some(palette) = self.theme_backup.take() {
            self.apply_palette(palette, cx);
        }
    }

    /// Hand a new palette to everything that draws with it: the window (whose
    /// render reads it for the sidebar, the panels and the status bar), the editor
    /// (the note and its underlines), and the open picker — which is drawn in the
    /// theme it is offering, so a preview is not hidden behind the panel showing
    /// it.
    fn apply_palette(&mut self, palette: Rc<Palette>, cx: &mut Context<Self>) {
        self.palette = Rc::clone(&palette);
        self.editor
            .update(cx, |editor, cx| editor.set_palette(palette.clone(), cx));
        if let Some(picker) = self.picker.clone() {
            picker.update(cx, |picker, cx| picker.set_palette(palette, cx));
        }
        cx.notify();
    }
}

/// One row of the theme picker: the name, and where the name comes from.
fn theme_row(name: &str, origin: &str) -> (PickerItem, PickerTarget) {
    (
        PickerItem {
            label: name.to_string().into(),
            detail: origin.to_string().into(),
            search: name.to_string().into(),
        },
        PickerTarget::Theme(name.to_string()),
    )
}
