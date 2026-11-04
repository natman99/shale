//! The vault's half of every window: the things that exist once per vault,
//! no matter how many windows show it (the shared-services step of the
//! multi-window plan).
//!
//! A window is chrome: tabs, a tree, an editor. A vault is a *place*, and a
//! place gets one index, one watcher walk, one live MCP channel — twice of
//! any of those is not a second opinion but a conflict: two SQLite writer
//! threads on one database, two walks of one tree, two servers fighting over
//! one `.shale` port lock. So those live here, in an entity the registry
//! keeps beside the vault's windows, and every window borrows them.
//!
//! The lifetime is the windows': the registry drops the entity when the last
//! window for the vault closes, the loops below hold only weak handles so
//! they stop with it, and `Drop` takes the MCP lockfile down the same way
//! [`Workspace::stop_live`] used to for a single window.

use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{App, Context, Entity};
use shale_core::db::Db;
use shale_core::{Config, Vault};

use crate::live::{LiveServer, Snapshot};

use super::Workspace;

/// How often the vault is re-walked at rest. The floor on the adaptive
/// interval (see [`WATCH_DUTY`]): a small vault stays interactive, and a big
/// one backs itself off by how long its own walk takes. Chosen for the
/// P3.4 exit-criteria check, all of it the watcher and none of it the graph.
const WATCH_INTERVAL: Duration = Duration::from_millis(750);

/// The ceiling on the adaptive interval. Past this a vault is so big that even
/// the walk at rest is rare, and external changes arriving within a quarter of
/// a minute is the promise; the tree also refreshes on any Shale-side write, so
/// the ceiling only slows *outside* edits, which is the rarer path.
const WATCH_INTERVAL_MAX: Duration = Duration::from_secs(15);

/// How many times the interval may exceed the walk it follows. The walk is
/// allowed at most ~1/300th of the clock: 0.4 ms vaults keep the 750 ms floor
/// (small vaults stay instant), and a 15 ms walk backs itself off to ~4.7 s,
/// which is the same ~0.3% idle the contract asks for at every vault size.
const WATCH_DUTY: u32 = 300;

/// How often the live channel republishes. Same as the old per-window beat:
/// shorter than anything an agent can act on.
const LIVE_REFRESH: Duration = Duration::from_millis(250);

/// The daily backup poll (roadmap P1.6).
const BACKUP_POLL: Duration = Duration::from_secs(3_600);

/// Everything a vault has exactly one of, shared by its windows.
pub(crate) struct VaultServices {
    pub vault: Rc<Vault>,
    pub config: Rc<Config>,
    /// The vault index. Windows hold a clone of this `Arc`; there is one
    /// SQLite connection and one writer thread behind it, whatever the
    /// window count.
    pub db: Option<Arc<Db>>,
    /// The live MCP channel: one server, one port, one lockfile. Windows
    /// publish into `live_snapshot`; the server answers from the copy.
    live: Option<LiveServer>,
    pub live_snapshot: Option<Arc<Mutex<Snapshot>>>,
    /// The workspaces showing this vault. Weak, so a closed window drops out
    /// of the loops on the next tick; the live beat and the watcher fan out
    /// through this list.
    windows: Vec<gpui::WeakEntity<Workspace>>,
    /// The session file, read once at startup: one slot per window that last
    /// showed the vault. Windows claim the lowest free slot, so a reopened
    /// window lands back in the memory of the window that stood there.
    sessions: shale_core::Sessions,
    /// Which slots a live window is holding.
    slots_in_use: std::collections::HashSet<usize>,
}

impl VaultServices {
    pub(crate) fn new(vault: Rc<Vault>, config: Rc<Config>, cx: &mut Context<Self>) -> Self {
        // The vault index is a cache the notes can always rebuild, so an index that
        // is switched off in the config or will not open is a missing feature rather
        // than a broken window: reported, and every window carries on without one.
        let db = if config.index.enabled {
            match Db::open(vault.root()) {
                Ok(db) => Some(db),
                Err(err) => {
                    tracing::warn!("no vault index: {err:#}");
                    None
                }
            }
        } else {
            None
        };
        let db = db.map(Arc::new);
        // The session file, read once. A file that does not parse is a log
        // line and a fresh start — the same posture the single-window session
        // always had: forgotten state is sad, a vault that will not open is
        // worse.
        let sessions = match shale_core::Sessions::load(vault.root()) {
            Ok(sessions) => sessions,
            Err(err) => {
                tracing::warn!("the session file was not understood, starting fresh: {err:#}");
                shale_core::Sessions::default()
            }
        };
        let mut services = Self {
            vault,
            config,
            db: db.clone(),
            live: None,
            live_snapshot: None,
            windows: Vec::new(),
            sessions,
            slots_in_use: std::collections::HashSet::new(),
        };
        // The one startup pass that catches what other programs did to the vault
        // while Shale was closed. After it, the watcher and the save paths keep the
        // index current between them.
        services.start_index(cx);
        services.start_backups(cx);
        // Roadmap P2.3: the live channel. It comes up before any window exists;
        // the first beat with a window in it fills the snapshot.
        services.start_live(cx);
        services
    }

    /// A workspace started showing this vault. Called by the workspace itself,
    /// so the loops reach it whether the window came from the registry or from
    /// a test's `add_window`.
    pub(crate) fn add_window(&mut self, workspace: &Entity<Workspace>) {
        self.windows.push(workspace.downgrade());
    }

    /// The session bookkeeping, kept here rather than per window so two
    /// windows on one vault each own a slot of the file and neither writes
    /// over the other's room.

    /// Claim the lowest session slot no live window is holding.
    pub(crate) fn claim_slot(&mut self) -> usize {
        let mut ix = 0;
        while self.slots_in_use.contains(&ix) {
            ix += 1;
        }
        self.slots_in_use.insert(ix);
        ix
    }

    /// Free a slot when its window closes. The slot's *memory* stays in the
    /// file — the next window to claim it wakes up in that room.
    pub(crate) fn release_slot(&mut self, ix: usize) {
        self.slots_in_use.remove(&ix);
    }

    /// What slot `ix` remembers, if the file has anything for it.
    pub(crate) fn session_at(&self, ix: usize) -> Option<shale_core::Session> {
        self.sessions.slots.get(ix).cloned()
    }

    /// Put slot `ix`'s memory and write the file. Best-effort like every
    /// other session beat: a shrug tomorrow, not a failed exit today.
    pub(crate) fn put_session(&mut self, ix: usize, session: shale_core::Session) {
        if self.sessions.slots.len() <= ix {
            self.sessions.slots.resize(ix + 1, shale_core::Session::default());
        }
        self.sessions.slots[ix] = session;
        if let Err(err) = self.sessions.save(self.vault.root()) {
            tracing::warn!("the session was not saved: {err:#}");
        }
    }

    /// The startup reconciliation: read the vault once on a background thread,
    /// index the notes the index does not already match, and stop.
    ///
    /// It ends on purpose. Roadmap P1.3 asks for a pass rather than a loop because
    /// the vault watcher is already running, and its ticks report everything after
    /// this point.
    fn start_index(&mut self, cx: &mut Context<Self>) {
        let Some(db) = self.db.clone() else {
            return;
        };
        // A copy of the vault rather than the `Rc`: the walk and the reading happen
        // on a background thread, and `Rc` cannot cross one.
        let vault = (*self.vault).clone();
        let watched = cx.entity().downgrade();
        cx.spawn(async move |_ws, cx| {
            let started = Instant::now();
            let report = cx
                .background_executor()
                .spawn(async move { db.reconcile(&vault) })
                .await;
            match report {
                Ok(report) => {
                    if !report.is_empty() {
                        // Info, not debug: this is the line that answers the
                        // roadmap's exit question — did the index rebuild, and
                        // how long did it take — and it only appears when there
                        // really was work to report, so a quiet startup stays
                        // quiet.
                        tracing::info!(
                            indexed = report.indexed,
                            removed = report.removed,
                            took_ms = started.elapsed().as_millis(),
                            "the vault index was brought up to date"
                        );
                    } else {
                        // The quiet case still says the pass ran and what it
                        // cost, at debug: the startup check wants to see the
                        // spine working on every launch, not only on the ones
                        // that had work to do.
                        tracing::debug!(
                            took_ms = started.elapsed().as_millis(),
                            "the vault index was already up to date"
                        );
                    }
                    for (path, why) in &report.failed {
                        // One note that cannot be read costs its own rows, not the
                        // vault's index, so it is named and the pass stands.
                        tracing::warn!("could not index {}: {why}", path.display());
                    }
                }
                Err(err) => {
                    // The index keeps working — a failed pass is not a failed
                    // handle — so the next watcher tick tries these notes again.
                    tracing::warn!("could not bring the vault index up to date: {err:#}");
                }
            }
            let _ = watched;
        })
        .detach();
    }

    /// The daily backup loop (roadmap P1.6): take today's copy if today's copy
    /// has not been taken, then ask again every hour.
    ///
    /// The first answer comes at startup, so a vault opened on a new day is
    /// backed up without waiting out the poll. The copy itself happens on the
    /// index's own writer thread ([`shale_core::db::Db::backup`], off the
    /// window's thread), and every outcome is a log line: a backup that fails is
    /// tomorrow's insurance question, not a reason to disturb the window.
    fn start_backups(&mut self, cx: &mut Context<Self>) {
        let Some(db) = self.db.clone() else {
            return;
        };
        let watched = cx.entity().downgrade();
        cx.spawn(async move |_ws, cx| loop {
            // The weak handle: when the vault's services are gone there is
            // nothing left to back up, and the loop ends with them.
            if watched.upgrade().is_none() {
                break;
            }
            // A handle per round trip: the loop outlives each backup, so the
            // `Db` is cloned into the job rather than moved.
            let db = db.clone();
            let outcome = cx
                .background_executor()
                .spawn(async move { db.backup() })
                .await;
            match outcome {
                Ok(Some(path)) => {
                    tracing::info!(path = %path.display(), "the vault index was backed up")
                }
                // Today's copy already existed: the normal answer, forty-seven
                // times a day.
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!("the vault index was not backed up: {err:#}");
                }
            }
            cx.background_executor().timer(BACKUP_POLL).await;
        })
        .detach();
    }

    /// Start the live channel: bind the port and keep the snapshot warm.
    ///
    /// A failure here is a log line and a missing feature, exactly as it was
    /// per-window: nothing else depends on the channel.
    fn start_live(&mut self, cx: &mut Context<Self>) {
        if !self.config.mcp.live {
            tracing::debug!("the live MCP channel is off: [mcp] live = false");
            return;
        }
        let shared = Arc::new(Mutex::new(Snapshot::default()));
        self.live_snapshot = Some(Arc::clone(&shared));
        match LiveServer::start(self.vault.root(), Arc::clone(&shared)) {
            Ok(server) => {
                // The port is in the lockfile for `shale-mcp` to read; the log line
                // is for the human wondering whether the channel came up at all.
                tracing::debug!(port = server.port(), "the live MCP channel is up");
                self.live = Some(server);
                let watched = cx.entity().downgrade();
                cx.spawn(async move |_ws, cx| {
                    loop {
                        cx.background_executor().timer(LIVE_REFRESH).await;
                        let Some(services) = watched.upgrade() else {
                            break;
                        };
                        // Publish from every window, and the active one
                        // last: the snapshot ends as the window the desktop
                        // says is active, while a window no desktop calls
                        // active — an unfocused app, a test context — still
                        // keeps the answer alive when it is there alone.
                        let (all, active) = services
                            .update(cx, |s, cx| {
                                s.windows.retain(|w| w.upgrade().is_some());
                                (
                                    s.windows.clone(),
                                    cx.active_window()
                                        .and_then(|w| w.downcast::<Workspace>()),
                                )
                            });
                        for ws in &all {
                            let _ = ws.update(cx, |ws, cx| ws.publish_live(cx));
                        }
                        if let Some(active) = active {
                            let _ = active.update(cx, |ws, _window, cx| ws.publish_live(cx));
                        }
                    }
                })
                .detach();
            }
            Err(err) => {
                self.live_snapshot = None;
                tracing::warn!("the live MCP channel is off: {err:#}");
            }
        }
    }
}

impl VaultServices {
    /// Take the channel down: stop answering and take the lockfile away. The
    /// same promise `stop_live` made when it lived per-window: the next
    /// `shale-mcp` to look finds nothing rather than a stale hint.
    pub(crate) fn stop_live(&mut self) {
        self.live_snapshot = None;
        if let Some(server) = self.live.take() {
            if let Err(err) = server.stop() {
                tracing::warn!("could not remove the MCP lock: {err:#}");
            }
        }
    }
}

impl Drop for VaultServices {
    fn drop(&mut self) {
        // The MCP lockfile goes with the vault's last window.
        self.stop_live();
    }
}

/// Start watching the vault for this vault's windows: every [`WATCH_INTERVAL`]
/// (or longer, if the walk itself says the vault is big — see the duty rule
/// below) the disk is re-walked on a background thread and any difference is
/// folded into every window by [`Workspace::apply_disk_changes`].
///
/// One walk per vault no matter the window count — that is the whole point of
/// the watcher living with the services. Polling rather than an inotify
/// dependency keeps `shale-core` free of external crates, and the signature
/// pass is the same one the index uses to decide which notes to re-index.
/// Only a weak handle to the services is held, so the last window closing
/// ends the loop.
pub(crate) fn start_watcher(services: &Entity<VaultServices>, cx: &mut App) {
    // A copy, not the `Rc`: the walk runs on a background thread, and `Rc` cannot
    // cross one.
    let vault = (*services.read(cx).vault).clone();
    let watched = services.downgrade();
    cx.spawn(async move |cx| {
        let mut interval = WATCH_INTERVAL;
        loop {
            cx.background_executor().timer(interval).await;
            let vault = vault.clone();
            let walked = cx
                .background_executor()
                .spawn(async move {
                    let started = std::time::Instant::now();
                    let signature = vault.signature()?;
                    Ok::<_, anyhow::Error>((signature, started.elapsed()))
                })
                .await;
            let Ok((signature, walked)) = walked else {
                continue;
            };
            // The walk times itself: the next interval is sized so the walk owns
            // at most ~1/300th of the clock, floored at the interactive 750 ms
            // and capped at [`WATCH_INTERVAL_MAX`].
            interval = (walked * WATCH_DUTY)
                .max(WATCH_INTERVAL)
                .min(WATCH_INTERVAL_MAX);
            let Some(services) = watched.upgrade() else {
                // The vault's last window closed, so there is nothing left to
                // keep current.
                break;
            };
            // Fan out: every workspace folds the same signature in. A closed
            // window is pruned on the way.
            let windows = services.read_with(cx, |s, _| s.windows.clone());
            let mut live = Vec::new();
            for ws in windows {
                if ws
                    .update(cx, |ws, cx| ws.apply_disk_changes(signature.clone(), cx))
                    .is_ok()
                {
                    live.push(ws);
                }
            }
            services.update(cx, |s, _| s.windows = live);
        }
    })
    .detach();
}
