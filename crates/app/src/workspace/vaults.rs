//! One Shale, one window per vault.
//!
//! The process that opens a vault claims it (an exclusive `flock` on
//! `.shale/lock`) and keeps the claim for the life of the process; a second
//! Shale pointed at the same vault finds the door shut and says so. Inside
//! this process the same claim is kept in a registry, so a second window on
//! the same vault focuses the first instead of starting a second watcher,
//! index and MCP channel beside it. Opening another vault opens another
//! window in the same process — which is what makes the vault switcher cheap,
//! and what keeps the whole app to one copy of the binary, one allocation
//! pool, and one set of background threads.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui::{
    App, AppContext as _, BorrowAppContext, Bounds, TitlebarOptions, WindowBounds,
    WindowHandle, WindowOptions, px, size,
};
use shale_core::{Config, RecentVaults, Vault};

use gpui::{Context, PathPromptOptions, Window};

use crate::acts::ChangeVault;
use crate::identity;
use crate::picker::PickerItem;

use super::{start_watcher, vault_title, Workspace};
use super::PickerTarget;

/// What the first window does beyond simply existing: `--note`, `--graph`,
/// `--split`, `--open-after`. Later windows just open the vault.
#[derive(Default)]
pub(crate) struct OpenOptions {
    pub(crate) note: Option<PathBuf>,
    pub(crate) graph: bool,
    pub(crate) split: bool,
    pub(crate) open_after: Option<u64>,
}

/// Where the recent-vault list lives. The default is the XDG state directory
/// (`RecentVaults::store_path`); the tests point it at a temp dir so a test
/// run never edits anyone's real memory.
pub(crate) struct RecentStore(pub(crate) PathBuf);

impl gpui::Global for RecentStore {}

fn recent_store(cx: &App) -> PathBuf {
    cx.try_global::<RecentStore>()
        .map(|g| g.0.clone())
        .unwrap_or_else(RecentVaults::store_path)
}

pub(crate) struct VaultWindows {
    /// Canonical vault root → its window.
    windows: HashMap<PathBuf, WindowHandle<Workspace>>,
    /// The open claim files. Dropping one releases the flock, so these live
    /// exactly as long as the registry — which lives as long as the run.
    _claims: HashMap<PathBuf, File>,
    /// The termination flag the signal handlers raise; every window polls it.
    terminating: Arc<AtomicBool>,
    /// The first window's options, kept so a `--split` startup splits only
    /// the window it started in.
    first: bool,
}

impl VaultWindows {
    pub(crate) fn new(terminating: Arc<AtomicBool>) -> Self {
        Self {
            windows: HashMap::new(),
            _claims: HashMap::new(),
            terminating,
            first: true,
        }
    }
}

impl gpui::Global for VaultWindows {}

/// Claim the vault for this process: an exclusive `flock` on `.shale/lock`.
/// The kernel drops it when the process dies, however it dies, so a crashed
/// Shale never leaves a vault unopenable; a live one holds the door shut.
/// The file itself records the pid so a human can see who holds it.
///
/// Windows has no `flock`; the equivalent is `LockFileEx`, and until that is
/// written the claim is unix-only. Two instances on one Windows vault remain
/// possible, and remain a bad idea.
#[cfg(unix)]
pub(crate) fn claim_vault(root: &Path) -> Result<Option<File>> {
    use std::io::Read as _;
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    let path = root.join(".shale").join("lock");
    std::fs::create_dir_all(path.parent().unwrap())
        .with_context(|| format!("open {}", path.parent().unwrap().display()))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o644)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let mut buf = String::new();
        let _ = file.read_to_string(&mut buf);
        let holder = buf.trim();
        anyhow::bail!(
            "this vault is already open in another Shale{} — close that one first",
            if holder.is_empty() {
                String::new()
            } else {
                format!(" (pid {holder})")
            }
        );
    }
    let _ = file.write_all(format!("{}\n", std::process::id()).as_bytes());
    let _ = file.flush();
    Ok(Some(file))
}

/// The claim is unix-only for now; on other platforms the vault is not locked.
#[cfg(not(unix))]
pub(crate) fn claim_vault(_root: &Path) -> Result<Option<File>> {
    Ok(None)
}

/// Open `vault_path` in a window — this process's window for that vault, if
/// it already has one. Returns the window, or the reason the vault cannot be
/// opened (claimed elsewhere, unreadable, not a directory).
pub(crate) fn open_vault_window(
    vault_path: &Path,
    options: OpenOptions,
    cx: &mut App,
) -> Result<WindowHandle<Workspace>> {
    let root = vault_path
        .canonicalize()
        .with_context(|| format!("{} is not a vault", vault_path.display()))?;
    anyhow::ensure!(root.is_dir(), "{} is not a directory", root.display());

    // Already open here: the window exists, and two windows on one vault
    // would be two watchers and two MCP channels again — exactly what the
    // cross-process claim refuses.
    if let Some(window) = cx
        .global::<VaultWindows>()
        .windows
        .get(&root)
        .cloned()
    {
        // Deferred: the window we found may be the very window this call is
        // running inside (picking `here` does that), and gpui refuses
        // re-entering a window already on the stack.
        let found = window;
        cx.defer(move |cx| {
            let _ = found.update(cx, |_, window, _| window.activate_window());
            cx.activate(true);
        });
        return Ok(window);
    }

    // The claim: this process may already hold it — `main` claims the vault
    // before the GUI exists, so a vault owned by *another* process is refused
    // with a clean exit rather than a window that opens and closes. Only a
    // vault this run has not claimed yet needs claiming here.
    let held = cx
        .global::<VaultWindows>()
        ._claims
        .contains_key(&root);
    let vault = Rc::new(Vault::open(&root).context("opening vault")?);
    let claim = if held { None } else { claim_vault(&root)? };
    let (config, source) = Config::load_or_create(vault.root()).context("loading config")?;
    tracing::info!(?source, path = %Config::path(vault.root()).display(), "config");
    let config = Rc::new(config);

    // Remember the vault for the picker and for the next no-argument start.
    let store = recent_store(cx);
    let mut recent = RecentVaults::load_from(&store);
    recent.touch(&root);
    if let Err(e) = recent.save_to(&store) {
        // A memory that will not save is not a reason not to open a note.
        tracing::warn!(error = %e, path = %store.display(),
            "could not record the vault in the recent list");
    }

    let vault_name = vault_title(&root);
    let workspace = cx.new(|cx| Workspace::new(vault.clone(), config.clone(), cx));
    let ws_for_build = workspace.clone();
    let (first, terminating) = {
        let g = cx.global::<VaultWindows>();
        (g.first, g.terminating.clone())
    };

    let initial_note = options.note.clone();
    let window = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(vault_name.into()),
                    ..Default::default()
                }),
                // Filed under its own name rather than under whatever the
                // window manager guesses: X11 reads this as WM_CLASS, Wayland
                // as the xdg app_id, and `assets/shale.desktop` repeats it in
                // StartupWMClass so the desktop can find the entry, and
                // through it the mark (§13).
                app_id: Some(identity::APP_ID.into()),
                // The mark itself, for the one toolkit that takes it from the
                // program. A Wayland desktop gets the same mark from the
                // `.desktop` entry instead.
                #[cfg(target_os = "linux")]
                icon: Some(identity::window_icon(64)),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.), px(800.)),
                    cx,
                ))),
                ..Default::default()
            },
            move |window, cx| {
                cx.activate(true);
                // The pointer's position over the note can open a popup, and a
                // popup taking the keyboard is a focus change, so that part of
                // the window is wired up here, where there is a window.
                ws_for_build.update(cx, |ws, cx| ws.attach_window(window, cx));
                if let Some(note) = initial_note.clone() {
                    ws_for_build.update(cx, |ws, cx| ws.open_note(&note, cx));
                }
                if options.graph {
                    ws_for_build.update(cx, |ws, cx| ws.open_graph(cx));
                }
                ws_for_build
            },
        )
        .map_err(|_| anyhow::anyhow!("the window could not be opened"))?;

    // The workspace learns which window is its own, so quitting closes that
    // window and not the whole run.
    let _ = window.update(cx, |ws, _window, _cx| ws.set_self_window(window));

    // Whatever the window came up on holds the keyboard from the first frame,
    // so the very first keystroke after launch is an edit and not an ignored
    // key. Not `focus_editor`: a session that ended on the graph tab comes
    // back to the graph tab, and the keyboard has to be where the frame is.
    let _ = window.update(cx, |ws, window, cx| {
        if first && options.split {
            ws.split_pane(crate::workspace::panes::SplitDir::Row, false, window, cx);
        }
        ws.focus_active_tab(window, cx)
    });

    // The vault can change while Shale is open — Obsidian, git, another
    // terminal — so the tree and the open note follow the disk.
    start_watcher(&workspace, cx);

    // And a `kill` is answered the same way a quit is: the note to the disk,
    // the index checkpointed, then the window.
    workspace.update(cx, |ws, cx| ws.watch_for_termination(terminating, cx));

    if first {
        cx.update_global::<VaultWindows, _>(|g, _cx| g.first = false);
        if let Some(secs) = options.open_after {
            let ws = workspace.clone();
            let note = options.note.clone().unwrap_or_else(|| PathBuf::from("Welcome.md"));
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(secs))
                    .await;
                ws.update(cx, |ws, cx| ws.open_note(&note, cx));
            })
            .detach();
        }
    }

    // When the window closes — by quit, by the compositor, by the last tab's
    // `:q` — the registry forgets it and the claim file is released, so the
    // vault is openable again, here or by another process.
    cx.observe_release(&workspace, |ws, cx| forget_window(ws.vault_root(), cx))
        .detach();

    cx.update_global::<VaultWindows, _>(|g, _cx| {
        g.windows.insert(root.clone(), window);
        if let Some(claim) = claim {
            g._claims.insert(root, claim);
        }
    });
    Ok(window)
}

/// Seed the registry with a claim taken before the run began (see `main`).
pub(crate) fn seed_claim(root: PathBuf, claim: Option<File>, cx: &mut App) {
    if let Some(claim) = claim {
        cx.update_global::<VaultWindows, _>(|g, _cx| {
            g._claims.insert(root, claim);
        });
    }
}

/// Forget a window when it closes, so the vault can be opened again — in this
/// process or the next — and so the claim file is released with it.
pub(crate) fn forget_window(vault_root: &Path, cx: &mut App) {
    let root = vault_root.canonicalize().unwrap_or_else(|_| vault_root.to_path_buf());
    cx.update_global::<VaultWindows, _>(|g, _cx| {
        g.windows.remove(&root);
        g._claims.remove(&root);
    });
}

/// The recent-vault list the switcher shows: most recent first, minus any
/// that have stopped being directories.
pub(crate) fn recent_vaults(cx: &App) -> Vec<PathBuf> {
    RecentVaults::load_from(&recent_store(cx))
        .recent
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
}

/// Every vault this process has a window for. The tests count these; the
/// switcher marks the current vault from the window's own root instead.
#[cfg(test)]
pub(crate) fn open_vaults(cx: &App) -> Vec<PathBuf> {
    cx.global::<VaultWindows>().windows.keys().cloned().collect()
}

/// The window's half: the switcher's picker, and the two ways an answer is
/// carried out.
impl Workspace {
    /// `:vault` and the palette: the vaults this app remembers, most recent
    /// first, under a row that reaches the file dialog. The vault on screen
    /// is marked `here` — choosing it just focuses this window.
    pub(crate) fn change_vault(
        &mut self,
        _: &ChangeVault,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let here = self
            .vault_root()
            .canonicalize()
            .unwrap_or_else(|_| self.vault_root().to_path_buf());
        let mut rows: Vec<(PickerItem, PickerTarget)> = vec![(
            PickerItem {
                label: "Open a folder…".into(),
                detail: "a vault this app has never seen".into(),
                search: "other new folder prompt dialog".into(),
            },
            PickerTarget::VaultPrompt,
        )];
        for path in recent_vaults(cx) {
            let detail = path.display().to_string();
            let detail = if path == here {
                format!("{detail} · here")
            } else {
                detail
            };
            rows.push((
                PickerItem {
                    label: vault_title(&path).into(),
                    detail: detail.clone().into(),
                    search: detail.into(),
                },
                PickerTarget::Vault(path),
            ));
        }
        self.open_picker("Switch vault", rows, window, cx);
    }

    /// A named vault: open its window. The asking window stays open — the
    /// user closes it if they want it gone — which is also what keeps the
    /// keyboard honest: gpui's key-repeat timer follows focus, and closing
    /// the window under a half-finished keystroke hands that keystroke's
    /// repeats to the new window.
    pub(crate) fn switch_vault(
        &mut self,
        path: &std::path::Path,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = open_vault_window(path, OpenOptions::default(), cx) {
            self.say(format!("{e:#}"), cx);
            cx.notify();
        }
    }

    /// The file dialog's answer opens the chosen folder in a window here;
    /// the window that asked steps aside once the new one is up.
    pub(crate) fn prompt_for_vault(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open a vault".into()),
        });
        cx.spawn(async move |ws, cx| {
            let paths = match rx.await {
                Ok(Ok(Some(paths))) => paths,
                // Cancelled, or a desktop without a working portal: the
                // picker already closed, and the status bar said nothing,
                // so silence is the honest answer.
                _ => return,
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            // The asking window stays: the new vault opens beside it, and
            // closing the old one is the user's call, not this flow's.
            if let Err(e) =
                cx.update(|cx| open_vault_window(&path, OpenOptions::default(), cx))
            {
                let _ = ws.update(cx, |ws, cx| ws.say(format!("{e:#}"), cx));
            }
        })
        .detach();
    }
}
