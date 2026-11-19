//! The window Shale opens when nothing was named and nothing can be opened.
//!
//! A desktop shortcut cannot name a vault without hard-coding a path, so the no-argument
//! start has to answer for itself: the vault it remembers if there is one, and otherwise a
//! screen that asks. This is that screen — one small window, no vault behind it, the recent
//! list and a folder dialog (roadmap P4.20, `vault-startup-plan.md`).
//!
//! It is deliberately not a Workspace with no vault: the Workspace assumes a vault in
//! everything it touches — services, session, tree, tabs — and a second shape of "half a
//! window" would be a second of everything. What the screen reuses is the picker widget
//! itself, so the rows, the filtering and the keyboard are the same ones `:vault` uses.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::Result;
use gpui::{
    App, Bounds, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, Styled, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowOptions, div, prelude::*, px, size,
};
use shale_core::RecentVaults;
use shale_core::theme::Palette;

use crate::identity;
use crate::picker::{self, PickerEvent, PickerItem};
use crate::theme;
use crate::workspace::vaults;

/// The key context the screen's own keys would live in. Nothing is bound there today —
/// the picker brings its own context — but the name is here so a later chord has a home
/// rather than borrowing the picker's.
pub(crate) const CONTEXT: &str = "vault-open";

/// What a row on the screen stands for.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum VaultTarget {
    /// One of the vaults this app remembers.
    Recent(PathBuf),
    /// A folder this app has never seen, through the desktop's folder dialog.
    OpenFolder,
    /// A folder this app makes, named in the field (ruling 2026-11-18: same round).
    NewVault,
}

/// What the command line and the recent list decided the app should do.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Startup {
    /// Open this vault — a path named on the command line, or the one remembered.
    Vault(PathBuf),
    /// Ask instead. The sentence is why: the remembered vault is gone, or another Shale
    /// has it. `None` is a first run with nothing to explain.
    Ask(Option<String>),
    /// A path was *named* and is no good. That is a terminal answer, not a window: a
    /// script must get an exit code rather than wait on a screen it cannot see (EDGE-V4).
    Refuse(String),
}

/// The startup decision, kept apart from the claiming so it can be tested without a
/// filesystem race or a window.
///
/// The command line wins over the memory, and a bad named path is refused rather than
/// forgiven: `shale ./typo` should say so where the command was typed.
pub(crate) fn startup_plan(named: Option<PathBuf>, recent: &RecentVaults) -> Startup {
    if let Some(named) = named {
        return match named.canonicalize() {
            Ok(root) if root.is_dir() => Startup::Vault(root),
            _ => Startup::Refuse(format!("{} is not a vault", named.display())),
        };
    }
    match recent.startup_vault() {
        Some(root) => Startup::Vault(root),
        // Nothing remembered at all: a first run, or a remembered folder that is gone.
        // The screen says which, because "pick a vault" without the sentence reads as a
        // program that forgot rather than one that looked.
        None => Startup::Ask(
            recent
                .last
                .as_ref()
                .map(|last| format!("{} is not a vault any more", last.display())),
        ),
    }
}

/// The screen: the recent vaults, a folder dialog, and the reason it is on screen.
pub(crate) struct VaultOpen {
    picker: Entity<picker::Picker<VaultTarget>>,
    /// Why this window exists: the remembered vault is gone, another Shale has it, or
    /// there is nothing to remember. Quiet text, because it is an explanation.
    reason: Option<String>,
    /// What the last choice refused. Drawn in the danger role, and kept until a choice
    /// works: the sentence belongs under the thing that failed (the same rule the graph's
    /// filter strip follows).
    refusal: Option<String>,
    /// The window this view is drawn in, so `escape` can close it the way a workspace
    /// closes its own rather than ending the run from under a frame.
    self_window: Option<WindowHandle<VaultOpen>>,
    palette: Rc<Palette>,
    /// The font the field is drawn in, so a typed vault name looks like the text it
    /// becomes — the same reason the workspace's prompts use the note's font.
    family: SharedString,
    size: f32,
    /// The name field for a vault that does not exist yet, while it is open.
    prompt: Option<Entity<crate::prompt::Prompt>>,
    /// The picker's events. One subscription, replaced if the picker is ever rebuilt.
    _events: Option<gpui::Subscription>,
    /// The name field's events, while it is open.
    _prompt_events: Option<gpui::Subscription>,
    /// What the last frame drew as its line, for the tests that read the screen off the
    /// frame rather than off the fields.
    #[cfg(test)]
    drawn: std::cell::RefCell<Option<(String, bool)>>,
}

impl VaultOpen {
    fn new(
        reason: Option<String>,
        palette: Rc<Palette>,
        family: SharedString,
        size: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        let rows = rows(cx);
        let picker = cx.new(|cx| {
            picker::Picker::new(
                "Open a vault",
                rows,
                Rc::clone(&palette),
                family.clone(),
                size,
                cx,
            )
        });
        let mut this = Self {
            picker: picker.clone(),
            reason,
            refusal: None,
            self_window: None,
            palette,
            family,
            size,
            prompt: None,
            _events: None,
            _prompt_events: None,
            #[cfg(test)]
            drawn: std::cell::RefCell::new(None),
        };
        this.watch_picker(cx);
        this
    }

    /// The picker's answer, carried out. Choosing a vault is the whole job: the window
    /// closes when the vault window is up, and stays when it is not, with the reason for
    /// the failure where the choice was made (EDGE-V7).
    fn watch_picker(&mut self, cx: &mut Context<Self>) {
        let picker = self.picker.clone();
        self._events = Some(cx.subscribe(&picker, |this, picker, event, cx| {
            match event {
                PickerEvent::Choose => {
                    let Some(chosen) = picker.read_with(cx, |picker, _cx| picker.chosen()) else {
                        return;
                    };
                    match chosen {
                        VaultTarget::Recent(path) => this.open(&path, cx),
                        VaultTarget::OpenFolder => this.ask_for_folder(cx),
                        VaultTarget::NewVault => this.ask_new_vault(cx),
                    }
                }
                // Nothing was chosen. The window goes, and with no vault window the run
                // ends on its own (EDGE-V9).
                PickerEvent::Cancel => this.close(cx),
                PickerEvent::Highlight => {}
            }
        }));
    }

    /// Open a vault and step aside. A vault this process already shows is focused by
    /// `open_vault_window` rather than opened twice, and the screen has still done its
    /// job: the reader is looking at a vault (EDGE-V8).
    fn open(&mut self, path: &Path, cx: &mut Context<VaultOpen>) {
        if let Err(e) = vaults::open_vault_window(path, vaults::OpenOptions::default(), cx) {
            self.refusal = Some(format!("{e:#}"));
            cx.notify();
            return;
        }
        // A choice that worked retires the explanation it was made under.
        self.reason = None;
        self.close(cx);
    }

    /// The desktop's folder dialog. Cancelled, or no portal on this desktop: silence, and
    /// the screen stays — the same answer `:vault` gives (EDGE-V5).
    fn ask_for_folder(&mut self, cx: &mut Context<VaultOpen>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open a vault".into()),
        });
        cx.spawn(async move |this, cx| {
            let paths = match rx.await {
                Ok(Ok(Some(paths))) => paths,
                _ => return,
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let opened = cx
                .update(|cx| vaults::open_vault_window(&path, vaults::OpenOptions::default(), cx));
            match opened {
                Ok(_window) => {
                    let _ = this.update(cx, |this, cx| this.close(cx));
                }
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.refusal = Some(format!("{e:#}"));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// Ask for the name of a vault to make. A field rather than the folder dialog: a
    /// dialog that can choose a folder can rarely *make* one, and the reader who wants a
    /// new vault already has a name in mind (ruling 2026-11-18: new vault, same round).
    fn ask_new_vault(&mut self, cx: &mut Context<Self>) {
        let palette = Rc::clone(&self.palette);
        let prompt = cx.new(|cx| {
            crate::prompt::Prompt::new(
                crate::prompt::PromptKind::NewVault,
                "name the new vault",
                "",
                palette,
                self.family.clone(),
                self.size,
                cx,
            )
        });
        self.prompt = Some(prompt.clone());
        // A fresh question retires the last failure: the sentence under the field would
        // otherwise read as being about *this* one.
        self.refusal = None;
        self._prompt_events = Some(cx.subscribe(&prompt, |this, _prompt, event, cx| {
            match event {
                crate::prompt::PromptEvent::Submit(text) => {
                    let typed = text.to_string();
                    this.answer_new_vault(&typed, cx);
                }
                // Back out of the name: the field goes, the screen stays, and the
                // keyboard returns to the list it came from.
                crate::prompt::PromptEvent::Cancel => {
                    this.prompt = None;
                    this._prompt_events = None;
                    this.focus_picker(cx);
                    cx.notify();
                }
                crate::prompt::PromptEvent::Changed(_) => {}
            }
        }));
        self.focus_field(cx);
        cx.notify();
    }

    /// The name was answered: make the folder, then open it. A refusal leaves the field
    /// open with the name in it, so the fix is a keystroke rather than a re-choice
    /// (the same rule every other refusal in the app follows).
    fn answer_new_vault(&mut self, typed: &str, cx: &mut Context<Self>) {
        let name = typed.trim();
        if name.is_empty() {
            self.refusal = Some("a vault needs a name".to_string());
            cx.notify();
            return;
        }
        let target = match vault_path(name, home().as_deref()) {
            Ok(target) => target,
            Err(why) => {
                self.refusal = Some(why);
                cx.notify();
                return;
            }
        };
        if let Err(why) = make_folder(&target) {
            self.refusal = Some(why);
            cx.notify();
            return;
        }
        if let Err(e) = vaults::open_vault_window(&target, vaults::OpenOptions::default(), cx) {
            self.refusal = Some(format!("{e:#}"));
            cx.notify();
            return;
        }
        self.prompt = None;
        self._prompt_events = None;
        self.reason = None;
        self.close(cx);
    }

    /// Move the keyboard to the name field. Deferred, because this runs from inside a
    /// picker event: a view cannot reach into its own window mid-update (the re-entrancy
    /// rule the graph's filter line learned the hard way).
    fn focus_field(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.self_window else {
            return;
        };
        cx.defer(move |cx| {
            let _ = handle.update(cx, |view, window, cx| {
                if let Some(prompt) = &view.prompt {
                    let focus = prompt.read(cx).focus_handle(cx);
                    window.focus(&focus, cx);
                }
            });
        });
    }

    /// And back to the list, when the field goes away.
    fn focus_picker(&self, cx: &mut Context<Self>) {
        let Some(handle) = self.self_window else {
            return;
        };
        let picker = self.picker.clone();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_view, window, cx| {
                let focus = picker.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
            });
        });
    }

    /// Close this window. Deferred, because the close usually runs from inside this
    /// view's own update — a picker event, a dialog's answer — and gpui refuses to yank
    /// the window out from under its own frame.
    fn close(&mut self, cx: &mut Context<VaultOpen>) {
        let Some(handle) = self.self_window else {
            // A view that never learned its window (a test built without a window) has
            // nothing to close; the run ends the way any run with no windows does.
            cx.quit();
            return;
        };
        vaults::clear_startup_window(handle, cx);
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_this, window, _cx| window.remove_window());
        });
    }

    /// The window this view lives in. Also the handle the termination watcher closes, so
    /// a signal ends a run that is sitting on this screen (EDGE-V16).
    pub(crate) fn set_self_window(&mut self, window: WindowHandle<VaultOpen>) {
        self.self_window = Some(window);
    }

    /// The rows on offer, for the tests that ask which vaults the screen named.
    #[cfg(test)]
    pub(crate) fn targets(&self, cx: &gpui::App) -> Vec<VaultTarget> {
        self.picker.read(cx).offered().cloned().collect()
    }

    /// What the name field holds. Tests only.
    #[cfg(test)]
    pub(crate) fn prompt_text(&self, cx: &gpui::App) -> String {
        self.prompt
            .as_ref()
            .map(|prompt| prompt.read(cx).text().to_string())
            .unwrap_or_default()
    }

    /// Whether the name field is open. The test asks it after a refusal, because a field
    /// that closed itself would send the reader back through the list to fix one letter.
    #[cfg(test)]
    pub(crate) fn asking_name(&self) -> bool {
        self.prompt.is_some()
    }

    /// What the screen drew as its line under the title, and whether it drew it in the
    /// danger role. Read off the frame rather than re-derived from the fields, so a screen
    /// that stopped drawing the sentence fails the test instead of quietly agreeing with it.
    #[cfg(test)]
    pub(crate) fn drawn(&self) -> Option<(String, bool)> {
        self.drawn.borrow().clone()
    }
}

impl Render for VaultOpen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette.as_ref();
        // A refusal outranks the reason it happened under, and takes the colour the palette
        // keeps for a broken thing; the reason itself is quiet text, because it explains
        // where the reader is rather than what went wrong.
        let said = self.refusal.clone().or_else(|| self.reason.clone());
        // One decision picks the colour, and the test reads the colour rather than the
        // intent: a refusal drawn in the quiet role is a refusal the reader does not see.
        let colour = if self.refusal.is_some() {
            palette.callout_danger
        } else {
            palette.ui_dim
        };
        #[cfg(test)]
        {
            *self.drawn.borrow_mut() = said
                .clone()
                .map(|text| (text, colour == palette.callout_danger));
        }

        div()
            .key_context(CONTEXT)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            // The panel is a question, not a workspace: it sits near the top of a small
            // window with room under it, so a refusal row never pushes it off screen.
            .pt_6()
            .gap_2()
            .bg(gpui::rgb(palette.ui_bg))
            .child(
                div()
                    .w(px(520.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_color(gpui::rgb(palette.ui_fg))
                            .text_size(px(16.))
                            .child("Shale"),
                    )
                    .when_some(said, |d, said| {
                        d.child(div().text_color(gpui::rgb(colour)).child(said))
                    }),
            )
            // One question at a time: while the name is being typed, the list steps
            // back rather than sitting under a field the keyboard is no longer in.
            .child(
                div()
                    .w(px(520.))
                    .when_some(self.prompt.clone(), |d, prompt| d.child(prompt))
                    .when(self.prompt.is_none(), |d| d.child(self.picker.clone())),
            )
    }
}

/// Make the folder a new vault will live in, or say why it will not. A folder that is
/// already empty is fine — the reader may have made it by hand — and so is one holding
/// only Shale's own state, which is a vault that was opened once and emptied. A folder
/// with things in it is someone's folder already, and "new vault" does not quietly
/// vault it.
pub(crate) fn make_folder(target: &Path) -> std::result::Result<(), String> {
    if target.exists() {
        if !target.is_dir() {
            return Err(format!("{} is a file, not a folder", target.display()));
        }
        let occupied = std::fs::read_dir(target)
            .map(|entries| {
                entries
                    .filter_map(|entry| match entry {
                        Ok(entry) if entry.file_name() == ".shale" => None,
                        Ok(_entry) => Some(()),
                        Err(_) => Some(()),
                    })
                    .next()
                    .is_some()
            })
            .unwrap_or(true);
        if occupied {
            return Err(format!("{} already has things in it", target.display()));
        }
        return Ok(());
    }
    std::fs::create_dir_all(target)
        .map_err(|e| format!("{} could not be made: {e}", target.display()))
}

/// Where a typed vault name points, given the folder a bare name lands under. A path
/// spelled out is honored as spelled, and `~/` is stripped because readers type it out
/// of habit and mean the same place.
pub(crate) fn vault_path(name: &str, home: Option<&Path>) -> std::result::Result<PathBuf, String> {
    if name == "." || name == ".." {
        return Err(format!("{name} is a folder you already have"));
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let home = home.ok_or_else(|| "there is no home folder to make a vault under".to_string())?;
    Ok(home.join(name.strip_prefix("~/").unwrap_or(name)))
}

/// The folder a bare vault name lands under: the place a person means when they ask for
/// "a vault called Dnd".
fn home() -> Option<PathBuf> {
    #[cfg(windows)]
    let key = "USERPROFILE";
    #[cfg(not(windows))]
    let key = "HOME";
    std::env::var_os(key).map(PathBuf::from)
}

/// The rows: the vaults this app remembers, then the two ways to reach one it has never
/// seen. The list leads because the common answer is the vault that was just open; the
/// `:vault` switcher leads with its action row because there the list is the whole point.
fn rows(cx: &App) -> Vec<(PickerItem, VaultTarget)> {
    let mut rows: Vec<(PickerItem, VaultTarget)> = vaults::recent_vaults(cx)
        .into_iter()
        .map(|path| (vaults::vault_item(&path, None), VaultTarget::Recent(path)))
        .collect();
    rows.push((
        PickerItem {
            label: "Open a folder…".into(),
            detail: "a vault this app has never seen".into(),
            detail_ranges: Vec::new(),
            search: "open folder other new dialog browse".into(),
        },
        VaultTarget::OpenFolder,
    ));
    rows.push((
        PickerItem {
            label: "New vault…".into(),
            detail: "a folder this app makes for you".into(),
            detail_ranges: Vec::new(),
            search: "new vault create make empty folder".into(),
        },
        VaultTarget::NewVault,
    ));
    rows
}

/// Open the screen. Small and centered: it is a question, not a workspace, and it keeps
/// no seat in any session file because there is no vault to keep one in (EDGE-V14).
pub(crate) fn open_startup_window(
    reason: Option<String>,
    cx: &mut App,
) -> Result<WindowHandle<VaultOpen>> {
    let palette = Rc::new(Palette::DEFAULT);
    let window = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Shale".into()),
                    ..Default::default()
                }),
                app_id: Some(identity::APP_ID.into()),
                #[cfg(target_os = "linux")]
                icon: Some(identity::window_icon(64)),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(560.), px(420.)),
                    cx,
                ))),
                ..Default::default()
            },
            move |window, cx| {
                cx.activate(true);
                let available = window.text_system().all_font_names();
                let family = theme::pick_mono_family(&available, "").into();
                let view =
                    cx.new(|cx| VaultOpen::new(reason, Rc::clone(&palette), family, 14.0, cx));
                let focus = view.read(cx).picker.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
                view
            },
        )
        .map_err(|_| anyhow::anyhow!("the window could not be opened"))?;
    // The view learns its own window from the handle `open_window` answers with: inside
    // the build closure there is only an untyped handle, and a view that cannot name its
    // window cannot close it (the way a workspace quits its own window, not the run).
    let _ = window.update(cx, |view, _window, _cx| view.set_self_window(window));
    // The registry holds the handle so the termination watcher can close this window too;
    // it is not a vault window, so it is not in the per-vault list.
    vaults::set_startup_window(window, cx);
    Ok(window)
}
