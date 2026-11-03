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

use std::path::PathBuf;
use std::rc::Rc;

use anyhow::Context as _;
use gpui::{Context, Window};
use shale_core::config::{self, Value};
use shale_core::theme::{self, Palette};

use crate::acts::ChangeTheme;
use crate::picker::PickerItem;

use super::open::PickerTarget;
use super::tabs::TabContent;
use super::Workspace;

impl Workspace {
    /// The theme picker: the theme on screen first, then every other built-in
    /// palette, then the vault's own theme files. The row the pointer or the
    /// selection is on is applied as a preview; `enter` keeps it and `escape` puts
    /// the old one back.
    ///
    /// The current theme leads because the picker opens with its first row
    /// highlighted: unless that row *is* the theme on screen, the top of the list
    /// is a theme the user is not looking at, and the row they meant is somewhere
    /// below their own selection.
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
        // Rotated rather than sorted: the rest of the list is in the order the
        // palettes are documented, and a user who knows a theme's name should not
        // have to hunt for it in alphabetical noise. Matched on the palette rather
        // than the name, because the same palette can be reached by more than one
        // name (an alias, a `-dark` suffix, a vault file that copies a built-in).
        let root = self.vault.root();
        let current = rows
            .iter()
            .position(|(item, _)| {
                theme::palette_for(root, item.label.as_str())
                    .is_ok_and(|palette| palette == *self.palette)
            })
            .unwrap_or(0);
        let (mut item, target) = rows.remove(current);
        item.detail = format!("{} · now", item.detail).into();
        rows.insert(0, (item, target));
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
        // A named theme is an answer, and the desktop does not get to overwrite
        // one later.
        self.follow_desktop = false;
        self.theme_backup = None;
        self.theme_name = name.to_string();
        self.apply_palette(Rc::new(palette), cx);
        Ok(())
    }

    /// `:theme export <name>`: the palette on screen, written out as a theme
    /// file a person can edit (§15).
    ///
    /// What is written is what is *drawn*, not what the config names: a theme
    /// that arrived through a `base`, an alias or a picker preview exports as the
    /// sixty-one colours the window is actually using, which is the starting
    /// point the command exists for.
    ///
    /// Two names are refused. A built-in's name would write a file the loader
    /// never reads, because `palette_for` answers built-ins before it looks in
    /// the vault. An existing file would bury whatever its author had already
    /// changed, and the command has no `--force` yet; deleting the file is the
    /// way to re-export over it.
    pub(super) fn export_theme(&self, name: &str) -> anyhow::Result<PathBuf> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "theme export needs a name");
        anyhow::ensure!(
            name != ".." && !name.contains('/') && !name.contains('\\'),
            "{name:?} is not a theme name"
        );
        anyhow::ensure!(
            Palette::built_in(name).is_none(),
            "{name} is a built-in theme; a file by that name would never be read"
        );
        let path = theme::ThemeFile::path(self.vault.root(), name);
        anyhow::ensure!(
            !path.exists(),
            "{} is already a theme; delete it to export over it",
            relative(self.vault.root(), &path).display()
        );
        let text = self.palette.to_theme_toml()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
        Ok(relative(self.vault.root(), &path))
    }

    /// §15: a vault that names no theme opens in the palette the desktop asks
    /// for. Started once, from `attach_window`, because the answer is not to
    /// hand: on Linux it is a portal round trip, and holding the first frame for
    /// it is worse than a window that starts dark and settles a moment later.
    ///
    /// Nothing is written to the config. Following the desktop is the absence of
    /// a choice, not a choice, and the next launch should still be able to follow.
    pub(super) fn follow_desktop_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let appearance = window.appearance();
        cx.spawn(async move |this, cx| {
            let Some(name) = crate::desktop::theme_name(appearance).await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                // The desktop answered late, and a theme chosen since then wins.
                if !ws.follow_desktop {
                    return;
                }
                ws.follow_desktop = false;
                match theme::palette_for(ws.vault.root(), name) {
                    Ok(palette) => {
                        ws.theme_name = name.to_string();
                        ws.apply_palette(Rc::new(palette), cx);
                        ws.say(format!("theme: {name} · the desktop's"), cx);
                    }
                    Err(err) => {
                        tracing::warn!("the desktop's theme {name:?} could not be loaded: {err:#}");
                    }
                }
            });
        })
        .detach();
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
        self.follow_desktop = false;
        self.theme_backup = None;
        self.theme_name = name.to_string();
        cx.notify();
    }

    /// Write a chosen theme into `<vault>/.shale/config.toml`, so the next launch
    /// opens in it, and keep the config this session holds in step with the file.
    ///
    /// Every path that *commits* a theme — the picker's `enter`, `:theme <name>` —
    /// has to call this; a preview must not. The error goes back to the caller
    /// rather than being reported here: the theme is already on screen, so the
    /// caller's "theme: X" is still true, and the two messages belong on one line.
    pub(super) fn persist_theme(&mut self, name: &str) -> anyhow::Result<()> {
        config::write_setting(
            self.vault.root(),
            "appearance",
            "theme",
            &Value::Str(name.to_string()),
        )?;
        let mut updated = (*self.config).clone();
        updated.appearance.theme = name.to_string();
        self.config = Rc::new(updated);
        Ok(())
    }

    /// Put back the palette the theme picker started from, if a preview was in
    /// flight. Called when the picker closes without a choice.
    pub(super) fn revert_theme(&mut self, cx: &mut Context<Self>) {
        if let Some(palette) = self.theme_backup.take() {
            self.apply_palette(palette, cx);
        }
    }

    /// Hand a new palette to everything that draws with it: the window (whose
    /// render reads it for the tab bar, the sidebar, the panels and the status bar),
    /// **every tab's** editor (the note and its underlines) or graph view, and the open
    /// picker — which is drawn in the theme it is offering, so a preview is not hidden
    /// behind the panel showing it.
    ///
    /// Every tab, not just the one on screen: a background tab is drawn the moment
    /// the user presses `g t`, and a tab that kept the old palette would be a theme
    /// change the user can un-do by changing the tab.
    fn apply_palette(&mut self, palette: Rc<Palette>, cx: &mut Context<Self>) {
        self.palette = Rc::clone(&palette);
        // The graph's labels are drawn in the note's face, so a theme that changes the
        // font changes them with it.
        let (family, size) = self.note_font(cx);
        for tab in self.tabs.iter() {
            match &tab.content {
                TabContent::Note { editor, .. } => {
                    editor.update(cx, |editor, cx| editor.set_palette(Rc::clone(&palette), cx))
                }
                TabContent::Graph(view) => {
                    view.update(cx, |view, cx| {
                        view.set_appearance(Rc::clone(&palette), family.clone(), size, cx)
                    });
                }
            }
        }
        if let Some(picker) = self.picker.clone() {
            picker.update(cx, |picker, cx| picker.set_palette(palette, cx));
        }
        cx.notify();
    }

    /// The status bar's suffix for the palette on screen: nothing when it reads,
    /// and otherwise how many problems the readability auditor found and the
    /// first of them in its own words.
    ///
    /// A vault's own theme file is judged by the same rules the built-ins are
    /// tested against (`design-language.md` §10), but only ever reported: an
    /// author who wants a low-contrast look keeps it, and the window still
    /// paints in the palette they asked for. Built-ins always come back empty,
    /// which is what the core test guarantees, so this only ever says anything
    /// about a palette someone wrote by hand.
    pub(super) fn readability_suffix(&self) -> String {
        let flags = theme::contrast::audit(&self.palette);
        if flags.is_empty() {
            return String::new();
        }
        let first = flags[0].to_string();
        // The rest go to the log: a status bar has room for one complaint, and
        // a theme with six is one bad colour choice rather than six.
        for flag in flags.iter().skip(1) {
            tracing::warn!("theme: {flag}");
        }
        match flags.len() {
            1 => format!(" · hard to read: {first}"),
            n => format!(" · hard to read, {} findings, first: {first}", n - 1),
        }
    }
}

/// `path` with the vault root removed, for a status line that has to fit: the
/// file is `.shale/themes/one.toml` to the person who just typed the command,
/// not `/home/nat/vaults/notes/.shale/themes/one.toml`.
fn relative(root: &std::path::Path, path: &std::path::Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
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
