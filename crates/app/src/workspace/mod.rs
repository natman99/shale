//! The window: the sidebar tree, the note, the status bar, and the vault edits
//! that connect them.
//!
//! Split into `keys.rs` (the keymap), `tree.rs` (the sidebar's keyboard state and
//! the rows it draws), `edits.rs` (the prompt and the vault edits behind its
//! answers), `checker.rs` (the window's half of the spellchecker), `notes.rs`
//! (opening a note and writing it back) and `render.rs` (the frame). All of them
//! are descendants of `workspace`, so they reach
//! `Workspace`'s private state directly; anything one of them calls on another is
//! `pub(super)`.

mod bar;
mod checker;
mod edits;
mod exit;
mod graph;
mod index;
mod jumps;
mod keys;
mod live;
mod notes;
mod open;
pub(crate) mod panes;
mod popups;
mod render;
mod review;
mod search;
mod services;
mod session;
mod start;
mod tabs;
#[cfg(test)]
mod testhooks;
mod themes;
mod tree;
pub(crate) mod vaults;
mod views;

pub(crate) use bar::STATUS_TTL;
pub(crate) use keys::keybindings;
pub(crate) use open::PickerTarget;
pub(crate) use render::vault_title;

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui::{prelude::*, Context, Entity, SharedString, Subscription, Task, Window};
use shale_core::db::{Db, Provenance};
use shale_core::theme::{self, Palette};
use shale_core::vault::{rel_text, VaultEntry};
use shale_core::watch::{Stamp, VaultChanges};
use shale_core::{Config, Vault};

use crate::acts::*;
use crate::editor::EditorEvent;
use crate::lsp::{Diagnostic, Fix, Spell};
use crate::picker;
use crate::prompt;

/// The whole window's state: what is open, what the sidebar shows, and the editor
/// that holds the note.
pub(crate) struct Workspace {
    focus_handle: gpui::FocusHandle,
    vault: Rc<Vault>,
    /// Which session slot this window owns: its own slice of
    /// `.shale/session.json`, so two windows on one vault remember different
    /// rooms and neither overwrites the other. Claimed from the vault's
    /// services at creation; freed when the window closes.
    session_slot: usize,
    /// The window's frame as of the last painted frame — render is where a
    /// `Window` is within reach, so the bounds are copied here rather than
    /// chased through every save path. Goes into the session on save.
    last_bounds: Option<shale_core::session::WindowGeom>,
    /// This workspace's own window, set by `vaults::open_vault_window` right
    /// after the window opens. `None` until then (and in tests that build a
    /// workspace without the registry), where an exit falls back to quitting
    /// the app the way a single-window Shale always did.
    self_window: Option<gpui::WindowHandle<Workspace>>,
    /// What the user asked for in `<vault>/.shale/config.toml`. The window reads
    /// it when it starts the checker, which `:harper on` does at any time, and
    /// replaces it when it writes one of its own settings back into that file.
    config: Rc<Config>,
    /// The palette the whole window is drawn with, shared with the editor so a
    /// theme change repaints everything at once.
    palette: Rc<Palette>,
    /// The name that palette was chosen by (`appearance.theme`, or what the theme
    /// picker or `:theme` picked since). Only the status line asks.
    theme_name: String,
    /// Whether the desktop's dark-or-light setting still gets to choose the
    /// palette. True only when `appearance.theme` named nothing, and cleared by
    /// the first answer -- the desktop's or the user's (`design-language.md` §15).
    follow_desktop: bool,
    /// The palette to go back to if the theme picker closes without a choice,
    /// because everything it previewed on the way was not an answer. `None` when
    /// no preview is in flight.
    theme_backup: Option<Rc<Palette>>,
    /// One buffer per note this window is showing (roadmap P4.17). A note's text,
    /// caret and undo history live here once, and every view of it — a tab, or
    /// from P4.0b an embed inside another note — borrows that one buffer, so two
    /// views cannot drift into two different versions of one file.
    ///
    /// `Weak` on purpose: a note is alive while some view holds it, and when the
    /// last one lets go the entry is simply stale. No close path has to remember
    /// to release it, which is the kind of thing this repo has historically gotten
    /// wrong.
    notes: Rc<crate::note::Registry>,
    /// Embedded notes the caret has finished editing but no tab shows.
    ///
    /// A note is normally kept alive by the view showing it: the registry holds a
    /// `Weak`, and when the last view lets go the buffer goes. An embed breaks that.
    /// The caret typed into a note no tab is showing, then left, and nothing is
    /// looking at it any more — without a strong reference here, the buffer and the
    /// unsaved edits inside it would be dropped on the way out. P4.0b M4 owns when
    /// these are released: a save writes them and lets go.

    /// The spell and grammar checker, when one is configured and was started.
    /// It owns a server process; the window only talks to it.
    spell: Option<Entity<Spell>>,
    /// The checker's notes on the open note. They live here rather than only in
    /// the checker so that the status bar can count them, and `K` or a resting
    /// pointer can open a popup for the word under the cursor without asking the
    /// server anything — which is also what makes that path testable.
    checker_notes: Vec<Diagnostic>,
    /// The popup the window is showing, if any. There is one slot and one only:
    /// opening another replaces it, and every way out goes through `close_popup`.
    popup: Option<Entity<crate::popup::Popup>>,
    /// What had the keyboard before the popup took it, so closing hands it back.
    popup_return_focus: Option<gpui::FocusHandle>,
    /// The line the popup sits below, tagged with the line space it is counted in:
    /// a panel about a word inside the block being edited hangs off a row of the
    /// other note. Its place is worked out again each frame, so the panel follows
    /// the note as it scrolls and goes away when the line does.
    popup_anchor: Option<crate::editor::LineRef>,
    /// Whether the pointer opened the popup. Only then does moving off the word
    /// close it; a popup asked for with `K` stays until the user says otherwise.
    popup_from_pointer: bool,
    /// Which marked range the popup is about, so a mouse-move over the same word
    /// does not reopen it and a checker answer can tell whether its subject is
    /// still marked.
    popup_note: Option<(usize, usize)>,
    /// What the checker offered about that range, in the order it offered it: the
    /// popup shows these titles, and a chosen row number indexes back into them.
    popup_fixes: Vec<Fix>,
    /// The hover preview card: what the note the pointer is resting on a link of
    /// points at. Its own slot rather than a second kind of `popup`, because it is a
    /// different kind of surface — the popup answers a question and takes the keyboard,
    /// the card answers nothing and must not. Still one floating thing at a time:
    /// opening either closes the other.
    card: Option<Entity<crate::card::Preview>>,
    /// The line the card hangs below, in the same tagged line space the popup's anchor
    /// uses, so a card about a link inside the block being edited hangs off a row of
    /// the other note. Re-placed every frame: it follows the note as it scrolls, and
    /// goes away when the line does.
    card_anchor: Option<crate::editor::LineRef>,
    /// Where in the link's row the card hangs: the byte the link starts at, so the card
    /// sits under the link rather than at the beginning of the line it is written on.
    card_link_start: Option<usize>,
    /// How far the note was scrolled when the floating surface last asked the pointer's
    /// question. Scrolling moves text under a pointer that never moved, and gpui reports
    /// no mouse move for it, so the pointer's question has to be asked again against the
    /// new paint.
    surface_scroll: f32,
    /// A scroll noticed during a frame, re-asked at the start of the next one — when the
    /// frame that drew the scrolled text has actually recorded where its rows went.
    surface_reask: bool,
    /// How many scroll frames re-*placed* the surface rather than only re-asking the
    /// pointer. Tests only, for the same reason as `scroll_reports`: whether the window
    /// re-renders is the live frame loop's decision, and the harness always renders.
    #[cfg(test)]
    surface_replace: usize,
    /// How many times the note reported a wheel scroll to the window. Tests only: the
    /// live frame loop decides which views re-render, and the test harness does not model
    /// that, so the event path is the part a headless test can actually see.
    #[cfg(test)]
    scroll_reports: usize,
    /// A focus to hand the keyboard to at the start of the next frame, for the one
    /// path that has to close a popup without a window to do it with (the file
    /// watcher). Setting this is what keeps "closing always returns focus" true.
    pending_focus: Option<gpui::FocusHandle>,
    /// The subscription to the open popup, replaced when another one opens.
    _popup_events: Option<Subscription>,
    /// The subscription that carries the checker's answers to the editor.
    _spell_events: Option<Subscription>,
    entries: Vec<VaultEntry>,
    /// The vault's folders, which is how an empty one still gets a tree row.
    dirs: Vec<VaultEntry>,
    /// Every name a wiki-link could point at. Kept on the window rather than handed
    /// once to one editor, because a tab opened at any point in the session has to
    /// resolve links exactly as the first one did.
    known: HashSet<String>,
    /// The files that are not notes, for the same reason: `![[image.png]]` resolves
    /// against these.
    attachments: Vec<VaultEntry>,
    /// The vault's link graph, built the first time the graph tab is opened and kept
    /// current from then on (see [`crate::graph`] and `graph-view-plan.md`).
    ///
    /// `None` means the graph has never been asked for: building it reads every note,
    /// so a window that never opens the tab never pays for it. **One per window** — the
    /// graph tab is a view over this, not a second copy of it, which is what lets a
    /// minimap or a local-graph view appear later without a second simulation.
    graph: Option<shale_core::graph::GraphIndex>,
    /// The window's one force simulation over [`Workspace::graph`], plus the slot map
    /// that says which node each body is (see [`crate::graph::GraphSim`]).
    ///
    /// Built with the index and re-slotted whenever the index changes underneath it,
    /// which is what lets a layout survive an edit: the arena's node indexes do not
    /// outlive a rebuild, but a note's path does. `None` until the graph is opened.
    graph_sim: Option<crate::graph::GraphSim>,
    /// The one-frame deferral that lets a local graph follow the note the window moves
    /// to (see `graph_local_follow`). Kept because a dropped gpui task is a cancelled
    /// one.
    /// The local graph's centre moved and the picture has not been re-made yet. A frame
    /// flag, not a task: see [`Workspace::graph_local_follow`].
    graph_follow_pending: bool,
    /// How many times this window has copied itself into the live snapshot. A test's
    /// way of asking whether anything republished while the window sat still, which
    /// the snapshot's own `updated_at` (epoch seconds) is too coarse to answer.
    #[cfg(test)]
    live_publishes: usize,
    /// The sim's beat: one task that ticks the layout while the graph tab is on screen
    /// and ends when the sim settles or the tab closes. One task, not one per tick —
    /// and it stops by itself, because a settled graph that keeps asking for frames is
    /// the idling rule broken.
    graph_beat: Option<Task<()>>,
    /// The index scan, while it is running.
    ///
    /// Building it reads every note in the vault and resolves every link in it, which on
    /// a large vault is seconds — long enough that doing it on the window's thread froze
    /// the whole window. So it runs on the background executor and the result is
    /// delivered back here. `Some` means a scan is in flight, which is also what the tab
    /// reads to say "building the graph".
    graph_build: Option<Task<()>>,
    /// What the vault did while that scan was in flight. The scan read the disk at a
    /// moment that has already passed, so a note saved or deleted mid-build would
    /// otherwise stay invisible in the graph until something else forced a rebuild; these
    /// are folded into the index as it lands.
    graph_pending: Vec<graph::GraphChange>,
    /// What the graph is showing. The window owns it rather than the view, because the
    /// mask it produces decides which nodes the **simulation** contains as well as which
    /// ones get painted, and the session file records it beside the camera.
    graph_filter: shale_core::graph::filter::GraphFilter,
    /// Which space the window's graph physics runs in — the graph tab's 2D/3D mode
    /// (roadmap P3.6). One per window because the simulation is one per window: a
    /// second view of the same vault looks at the same positions, in the same space.
    graph_dim: crate::graph::Dim,
    expanded: HashSet<PathBuf>,
    /// The window's panes. **One today** — P3.2a introduced the pane as the
    /// owner of a tab row before P3.2b adds the split commands that make more
    /// than one exist. The invariants that matter per pane (never an empty tab
    /// row, the keyboard follows the frame) live in [`tabs`]; the window adds
    /// only which pane is active, and every tab operation below reaches its
    /// tabs through [`Workspace::pane`] / [`Workspace::pane_mut`].
    panes: Vec<tabs::Pane>,
    /// Which pane has the keyboard. With one pane this never moves; a split
    /// will make `ctrl-w h/j/k/l` mean something.
    active_pane: usize,
    /// The tiling of the panes (roadmap P3.2): a tree of rows/columns whose
    /// leaves are indices into `panes`. Kept beside the flat list rather than
    /// wrapping it because panes hold entities and the tree is geometry;
    /// `panes` and `active_pane` are fixed up by every tree edit in `panes`.
    layout: panes::Node,
    /// The next tab id. Monotonic, because a tab's *index* moves when another tab
    /// closes and its editor's subscription needs an identity that does not.
    /// Window-level, not per pane: ids are how subscriptions find a tab wherever
    /// it ended up.
    tab_seq: usize,
    /// The title the window was last given, so a frame that did not change the note
    /// on screen does not ask the window manager for a new one (see
    /// [`Workspace::sync_window_title`]).
    last_window_title: Option<String>,
    /// The sidebar's own focus, so the tree can be driven from the keyboard.
    tree_focus: gpui::FocusHandle,
    /// The tree row the keyboard has selected.
    tree_selection: Option<PathBuf>,
    /// Whether the file tree is drawn. `space e` / `:tree` move it, and the note takes
    /// the width while it is away (see [`tree::Workspace::toggle_tree`][super::tree]).
    tree_visible: bool,
    /// The rows the last frame drew, top to bottom, which is what `j`/`k` step
    /// through: a collapsed folder's children are skipped for free.
    tree_rows: Vec<PathBuf>,
    /// The open prompt, and what it is asking for.
    prompt: Option<(prompt::PromptKind, Entity<prompt::Prompt>)>,
    /// The `/` search: the query, what it matched in the open note, and which
    /// match the cursor is on. It lives here rather than in the field so that
    /// `n`/`N` still work once the field has closed.
    search: search::SearchState,
    /// The jump chain: the notes the user came from and where in them they were,
    /// which `ctrl-o`/`ctrl-i` walk. See [`jumps`].
    jumps: jumps::Jumps,
    /// What had the keyboard when the prompt opened, so closing it puts the
    /// keyboard back where the user was working: a rename started from the tree
    /// leaves them in the tree.
    prompt_return_focus: Option<gpui::FocusHandle>,
    /// What every path in the vault looked like the last time the disk was
    /// checked, which is what the next check explains the difference to.
    signature: BTreeMap<PathBuf, Stamp>,
    /// The vault index, when one is configured and opened: the cache of notes,
    /// tags, links and word counts that `:wordcount` reads now and the graph,
    /// vault search and agent read later.
    ///
    /// `None` is a missing feature rather than a broken window — the same posture
    /// as a checker that will not start — because the notes are the source of truth
    /// and the index can always be rebuilt from them. It is shared by `Arc` so the
    /// startup pass can hold one handle on a background thread while the window
    /// keeps its own.
    db: Option<Arc<Db>>,
    /// The task that watches for a termination signal. Only started by `main`, and
    /// only for as long as the window is (see [`exit`]).
    _termination: Option<Task<()>>,
    /// The vault's shared services: the index handle, the live MCP channel, and
    /// the loops that must run once per vault rather than once per window (see
    /// [`services`]). Kept so the entity lives exactly as long as some window is
    /// showing its vault; the registry holds the same entity.
    _services: Entity<services::VaultServices>,
    /// The vault's live snapshot, shared with [`services::VaultServices`]: this
    /// window publishes into it, and the one server answers from it.
    live_snapshot: Option<Arc<Mutex<crate::live::Snapshot>>>,
    /// The review strip: the agent proposals waiting for the user, and the marker the
    /// review keys move (see [`crate::review`] and [`review`]). It is always there;
    /// it is only drawn when the queue is not empty.
    review: Entity<crate::review::Review>,
    /// The queue files the window could not read, which it complains about once rather
    /// than once a second.
    pending_unreadable: Vec<PathBuf>,
    /// The subscription to the open prompt, replaced when another one opens.
    _prompt_events: Option<Subscription>,
    /// The open picker, if one is open: quick-open, the command palette or a
    /// vault search. Every row carries what it stands for, so there is no side
    /// table to keep in step with a list that changes on each keystroke.
    picker: Option<Entity<picker::Picker<PickerTarget>>>,
    /// What had the keyboard when the picker opened, so closing it puts the
    /// keyboard back where the user was working.
    picker_return_focus: Option<gpui::FocusHandle>,
    /// The subscription to the open picker, replaced when another one opens.
    _picker_events: Option<Subscription>,
    status: SharedString,
    /// Bumped by every message the status bar is given. The countdown that clears
    /// a message writes only if the number it was started with is still the
    /// current one, so an older timer can never wipe out a newer message.
    status_seq: u64,
    /// The status-bar countdown currently parked. Replacing it drops the old one.
    _status_timer: Option<Task<()>>,
    /// The window-activation subscription: losing focus writes the note at once
    /// instead of waiting out the countdown. `attach_window` adds it, because that
    /// is where there is a window to observe.
    _activation_events: Option<Subscription>,
    /// The pointer-over-the-note subscription. It needs a window to hand the
    /// keyboard to, so `attach_window` adds it rather than `new` — and it follows the
    /// tab on screen, because a pointer can only hover over the note that is drawn.
    _hover_events: Option<Subscription>,
    /// Whether that subscription is pointing at a tab that is no longer on screen.
    /// Set by the tab paths that have no window to rehook with, and cleared by the
    /// next frame, which does.
    hover_stale: bool,
    /// Whether the keyboard has to follow the frame. A tab owns its editor, so the
    /// node with the keyboard is the drawn tab's editor; after a tab change the
    /// keyboard is still on the entity that is no longer drawn, and the next keystroke
    /// goes nowhere. Set by the tab paths that have no window to move it with, and
    /// cleared by the next frame, which does.
    focus_stale: bool,
    _keystroke_observer: Subscription,
}

impl Workspace {
    /// The vault this window reads and writes.
    pub(crate) fn vault_root(&self) -> &std::path::Path {
        self.vault.root()
    }

    /// Which slice of the session file this window owns.
    pub(crate) fn session_slot(&self) -> usize {
        self.session_slot
    }

    /// The window this workspace is drawn in, once it exists. One process can
    /// hold several windows, so an exit closes *this* one rather than quitting
    /// the app; the run ends on its own when the last window goes.
    pub(crate) fn set_self_window(&mut self, window: gpui::WindowHandle<Workspace>) {
        self.self_window = Some(window);
    }

    /// The test-shaped constructor: a workspace that owns its services from
    /// the vault and config. The real path is [`Workspace::new_with`] — the
    /// registry makes the services first, so several windows can share them.
    #[cfg(test)]
    pub(crate) fn new(vault: Rc<Vault>, config: Rc<Config>, cx: &mut Context<Self>) -> Self {
        let services = cx.new(|cx| services::VaultServices::new(vault, config, cx));
        Self::from_services(services, cx)
    }

    /// A window on a vault this process is already showing: the services come
    /// from the registry, so the index, the watcher walk and the MCP channel
    /// stay single no matter how many windows look at the vault.
    pub(crate) fn new_with(
        services: Entity<services::VaultServices>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::from_services(services, cx)
    }

    fn from_services(services: Entity<services::VaultServices>, cx: &mut Context<Self>) -> Self {
        let (vault, config, db, live_snapshot) = services.read_with(cx, |s, _| {
            (
                s.vault.clone(),
                s.config.clone(),
                s.db.clone(),
                s.live_snapshot.clone(),
            )
        });
        // The workspace tells the services it is looking at their vault, so
        // the watcher fan-out and the live beat reach it.
        let me = cx.entity();
        services.update(cx, |s, _| s.add_window(&me));
        // This window's slice of the session file: the lowest slot no window
        // is holding, so a reopened window lands back in the memory of the
        // window that used to stand here.
        let session_slot = services.update(cx, |s, _| s.claim_slot());
        let entries = vault.list_markdown().unwrap_or_default();
        let dirs = vault.list_dirs().unwrap_or_default();
        let signature = vault.signature().unwrap_or_default();
        // Folders start **closed**. The tree is the vault, but a vault with a few
        // thousand notes and every folder open is a few thousand rows laid out on every
        // frame, and a sidebar nobody can scan is not a tree. What the user opens is
        // remembered in the session, so this default is only what a fresh vault (or a
        // vault where they closed everything) looks like.
        let expanded = HashSet::new();
        let every_name = note_keys(&entries);
        // The files that are not notes, which is what `![[image.png]]` resolves
        // against. Both this and the link names live on the window rather than being
        // handed to one editor, because every tab opened later has to see the vault
        // exactly as the first one does.
        let attachments = vault.list_attachments().unwrap_or_default();
        // The palette comes from `appearance.theme`: a built-in name or a
        // `<vault>/.shale/themes/<name>.toml` file. A theme that cannot be found
        // or parsed must not stop the editor from starting, so it is reported —
        // in the log *and* in the status bar, because a window that opens in a
        // palette nobody chose should say why without a log file — and the
        // default palette is used. The name is kept, so the status line can
        // say what is on screen after a live switch.
        let configured = config.appearance.theme.trim().to_string();
        // Naming nothing is not the same as naming the default: it is the vault
        // asking the desktop instead (§15), and the flag has to be read before
        // `configured` is moved into the palette below.
        let follow_desktop = configured.is_empty();
        let (palette, theme_name, theme_error) = match theme::palette_for(vault.root(), &configured)
        {
            Ok(palette) => (
                palette,
                if configured.is_empty() {
                    Palette::DEFAULT_NAME.to_string()
                } else {
                    configured
                },
                None,
            ),
            Err(err) => {
                tracing::warn!("could not load the configured theme: {err:#}");
                (
                    Palette::DEFAULT,
                    Palette::DEFAULT_NAME.to_string(),
                    Some(err),
                )
            }
        };
        let palette = Rc::new(palette);
        // Focus mode and typewriter scroll deliberately do *not* come from the
        // config: they are state the user chose in this window, and they belong
        // with the session file (roadmap P1.8) rather than the settings they edit
        // by hand. The editor's own defaults — both off, scope `block` — stand.
        // Spell and grammar checking runs in a language server (`harper-ls` by
        // default). A server that is not installed, or will not start, is a
        // missing feature rather than a broken editor: it is reported and the
        // window carries on without one.
        let (spell, spell_events) = if config.spellcheck.enabled {
            match Spell::start(&config.spellcheck, cx) {
                Ok(spell) => {
                    // The checker notifies when it has new answers; those become
                    // the editor's wavy underlines.
                    let events =
                        cx.observe(&spell, |ws, spell, cx| ws.show_checker_notes(spell, cx));
                    (Some(spell), Some(events))
                }
                Err(err) => {
                    tracing::warn!("no spellcheck: {err:#}");
                    (None, None)
                }
            }
        } else {
            (None, None)
        };
        // Keybinding bugs on Linux are almost always "what did the platform
        // actually deliver, and which action did it resolve to", so log both —
        // at trace level, since this fires for every keystroke and floods the
        // console at debug. Enable with RUST_LOG=shale=trace.
        // The subscription has to be kept alive or the observer is dropped.
        let keystroke_observer = cx.observe_keystrokes(|_ws, event, _window, _cx| {
            tracing::trace!(
                key = event.keystroke.key.as_str(),
                modifiers = ?event.keystroke.modifiers,
                action = event.action.as_ref().map(|action| action.name()),
                contexts = ?event.context_stack,
                "keystroke"
            );
        });
        // The strip is drawn from the same palette as everything else, so a theme
        // change repaints it with the rest of the window.
        let review = cx.new(|cx| crate::review::Review::new(Rc::clone(&palette), cx));
        let mut workspace = Self {
            focus_handle: cx.focus_handle(),
            vault,
            session_slot,
            last_bounds: None,
            self_window: None,
            config,
            palette,
            theme_name: theme_name.clone(),
            follow_desktop,
            theme_backup: None,
            notes: Rc::new(crate::note::Registry::default()),

            spell,
            checker_notes: Vec::new(),
            popup: None,
            popup_return_focus: None,
            popup_anchor: None,
            popup_from_pointer: false,
            popup_note: None,
            popup_fixes: Vec::new(),
            card: None,
            card_anchor: None,
            card_link_start: None,
            surface_scroll: 0.0,
            surface_reask: false,
            #[cfg(test)]
            scroll_reports: 0,
            #[cfg(test)]
            surface_replace: 0,
            pending_focus: None,
            _popup_events: None,
            _spell_events: spell_events,
            entries,
            dirs,
            known: every_name,
            attachments,
            // Not built at startup: the graph reads every note, and most windows never
            // ask for it (see [`Workspace::open_graph`]).
            graph: None,
            graph_sim: None,
            graph_follow_pending: false,
            #[cfg(test)]
            live_publishes: 0,
            graph_beat: None,
            graph_build: None,
            graph_pending: Vec::new(),
            // Everything on, which is Obsidian's default graph and core's.
            graph_filter: shale_core::graph::filter::GraphFilter::default(),
            graph_dim: crate::graph::Dim::Two,
            expanded,
            // The window starts with no tabs and gets its first one below, which is
            // the same door `ctrl-t` uses: one place decides what a new tab's editor
            // is wired up with. One pane today; see the field docs.
            panes: vec![tabs::Pane::new(Vec::new())],
            active_pane: 0,
            layout: panes::Node::Leaf(0),
            tab_seq: 0,
            // Nothing has been given to a window that does not exist yet; the
            // first frame names it.
            last_window_title: None,
            tree_focus: cx.focus_handle(),
            tree_selection: None,
            // Every window starts with its tree showing; `restore_session` is what
            // hides it again for a vault whose user had it hidden.
            tree_visible: true,
            tree_rows: Vec::new(),
            prompt: None,
            prompt_return_focus: None,
            search: search::SearchState::default(),
            jumps: jumps::Jumps::default(),
            picker: None,
            picker_return_focus: None,
            _picker_events: None,
            signature,
            db,
            _services: services,
            _termination: None,
            live_snapshot,
            review,
            pending_unreadable: Vec::new(),
            _prompt_events: None,
            status: "".into(),
            status_seq: 0,
            _status_timer: None,
            _activation_events: None,
            _hover_events: None,
            hover_stale: false,
            focus_stale: false,
            _keystroke_observer: keystroke_observer,
        };
        // The first tab, before the session is read: restoring a note means putting
        // it in a tab, and there has to be one to put it in.
        workspace.add_tab(cx);
        // Roadmap P1.8: the window comes up where the last one left it, before
        // anything else has a chance to look at what "where" was.
        workspace.restore_session(cx);
        // Roadmap P2.5a: the queue of agent proposals is read before the window is on
        // screen, so a proposal that arrived while Shale was closed is on screen the
        // moment it is. The beat that keeps reading it belongs to the vault, not here
        // ([`VaultServices::start_queue_watch`](super::services)).
        workspace.scan_pending(cx);
        // The first snapshot the live server can answer with: the channel itself
        // came up with the vault's services, and this fills it with the note the
        // session restored.
        workspace.publish_live(cx);
        // A theme that failed to load is said here as well as logged: the window
        // opens in the default palette, and the one thing the user should learn
        // without opening a log file is *why the theme changed*. It is the last
        // thing said, so nothing later at startup can bury it.
        if let Some(err) = theme_error {
            workspace.say(format!("theme failed: {err:#}"), cx);
        } else {
            // A theme that loads but paints something too faint to read is a
            // theme with a bug in it rather than a broken one: the window keeps
            // the palette the user chose and says what is wrong with it. Only a
            // hand-written palette ever has anything to say here, because the
            // built-ins are tested against the same rules.
            let note = workspace.readability_suffix();
            if !note.is_empty() {
                workspace.say(format!("theme {theme_name}{note}"), cx);
            }
        }
        workspace
    }

    /// Wire up the part of the window that needs a window to act on: the pointer's
    /// position over the note, because opening a popup is a focus change.
    ///
    /// Called once per window, by the frame that builds it. Until it is, the window
    /// simply never hovers — which is why the rest of the window can be built, and
    /// tested, without one.
    ///
    /// This is also where the two window-shaped save hooks live: losing focus, and
    /// the window manager asking to close. Both need a window, so neither can live
    /// in `new`.
    pub(crate) fn attach_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._hover_events.is_some() {
            return;
        }
        self.watch_hover(window, cx);
        // A vault that named no theme in its config follows the desktop, which is
        // a question the window has to be up to ask (§15).
        if self.follow_desktop {
            self.follow_desktop_theme(window, cx);
        }
        self._activation_events = Some(cx.observe_window_activation(window, |ws, window, cx| {
            // Losing focus is the other half of "the user stopped typing": they
            // have gone somewhere else with a note still ahead of the disk, so it is
            // written now rather than after the countdown. Every tab, because every
            // tab is a note the user can see on the tab bar. Activating again has
            // nothing to catch up on.
            if !window.is_window_active() {
                ws.autosave_all(cx);
            }
        }));

        // The window manager's close button is the one exit that does not come
        // through `quit`, and gpui gives the window a hook for it: the callback
        // returns whether the window may close. On Wayland it is
        // `xdg_toplevel::Event::Close` that calls it.
        //
        // The write here is the one `quit` does rather than the autosave, because
        // `editor.autosave = false` means "do not write while I am working", not
        // "lose the note when the window goes away" — and with tabs open it means
        // every note on the tab bar, not just the one on screen. A note that cannot
        // be written keeps the window open, exactly as `:q` refuses to quit: the
        // status bar says why, and there is still a window to act on that from.
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_window, cx| {
            weak.update(cx, |ws, cx| {
                if !ws.write_all_tabs(cx) {
                    // A write failed: the window stays, with the reason in the
                    // status bar.
                    return false;
                }
                // Roadmap P1.8: the session goes with the note — this is the last
                // moment the window knows what it looked like.
                ws.save_session(cx);
                // The live channel does NOT go with this window: it belongs to
                // the vault, and it comes down when the vault's last window
                // closes (see [`services`]).
                // The queue beat does not go with this window either: it belongs to
                // the vault. The queue itself stays regardless — an unanswered
                // proposal is on disk for the next window to offer again (P2.5a).
                true
            })
            .unwrap_or(true)
        });
    }

    /// Point the hover subscription at the tab that is on screen.
    ///
    /// Only the note that is drawn can be hovered, so one subscription is enough —
    /// but it does have to be the drawn tab's. [`Workspace::show_tab`],
    /// [`Workspace::add_tab`] and [`Workspace::close_tab`] mark it stale when they move
    /// the frame, and the frame itself schedules the rehook, because this is the
    /// subscription that needs a window: opening a popup is a focus change.
    pub(super) fn watch_hover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The graph tab has no note to hover, so its subscription is simply dropped:
        // leaving the previous tab's in place would let a panel open for a note that is
        // no longer on screen.
        let Some(editor) = self.pane().active_tab().editor().cloned() else {
            self._hover_events = None;
            self.hover_stale = false;
            return;
        };
        self._hover_events = Some(cx.subscribe_in(
            &editor,
            window,
            |ws, _editor, event, window, cx| match event {
                EditorEvent::DismissPreview => ws.close_card(cx),
                EditorEvent::Scrolled => ws.note_scrolled(cx),
                EditorEvent::Hovered { offset, line, at } => {
                    ws.pointer_hovered(*offset, *line, *at, window, cx)
                }
                // Everything else the editor reports is handled by the subscription
                // the tab itself makes, which needs no window.
                EditorEvent::FollowLink { .. }
                | EditorEvent::CreateLink { .. }
                | EditorEvent::EnterEmbedAt { .. }
                | EditorEvent::EmbedExited { .. }
                | EditorEvent::Changed => {}
            },
        ));
        self.hover_stale = false;
    }

    /// Put a message in the status bar and start the countdown that clears it.
    ///
    /// The bar's other segments are *state*: the mode, the save dot, the checker's
    /// count and the search's readout are each true for exactly as long as they
    /// are on screen, so they clear themselves by changing. A message is different
    /// — it is a reply to something the user just did, and it goes stale. `:typewriter
    /// off` is worth reading once, not for the rest of the session, and a bar that
    /// keeps reporting it is a bar full of old information.
    ///
    /// So every message is parked with a timer, in the same shape as the autosave
    /// debounce: a new message *replaces* the parked task, which drops the old
    /// countdown and restarts the wait — that is what the tests catch. The sequence
    /// number is the belt to that brace, for a countdown that already woke up and
    /// has not applied yet: it writes only if the number it started with is still
    /// current, so it cannot clear a message that arrived after it.
    pub(crate) fn say(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.status = message.into();
        self.status_seq += 1;
        let seq = self.status_seq;
        self._status_timer = Some(cx.spawn(async move |ws, cx| {
            cx.background_executor().timer(STATUS_TTL).await;
            // The weak handle is what stops a countdown outliving its window.
            let _ = ws.update(cx, |ws, cx| {
                if ws.status_seq == seq {
                    ws.status = "".into();
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn toggle_dir(&mut self, path: PathBuf) {
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
    }

    /// Whether something other than the editor or the tree holds the keyboard.
    ///
    /// A prompt, a picker or a popup is a modal layer: it has its own bindings, and
    /// a context-free binding would otherwise fire through it. Every handler for a
    /// context-free key asks this before it moves anything underneath the layer.
    pub(super) fn overlay_owns_keys(&self) -> bool {
        self.prompt.is_some() || self.picker.is_some() || self.popup.is_some()
    }

    /// `ctrl-w h`: move the keyboard to the panel on the left.
    ///
    /// With a tiling split (roadmap P3.2) this steps to the pane on the left;
    /// the leftmost pane's left neighbour is the sidebar, which keeps the P1.8
    /// habit continuous — one `ctrl-w h` from the note, through panes, to the
    /// tree. The command is ignored while a prompt or a picker is up, because
    /// those own the keyboard and moving focus out from under them would leave
    /// them open and unreachable.
    pub(super) fn window_left(
        &mut self,
        _: &WindowLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        if self.focus_pane_relative(-1, 0, window, cx) {
            return;
        }
        self.focus_tree(&FocusTree, window, cx);
    }

    /// `ctrl-w l`: move the keyboard to the note.
    ///
    /// From the sidebar this enters the leftmost pane; from a pane it steps to
    /// the pane on the right, and the rightmost pane keeps the keyboard on its
    /// note — the old two-panel behaviour survives as the edge case.
    pub(super) fn window_right(
        &mut self,
        _: &WindowRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay_owns_keys() {
            return;
        }
        if self.tree_focus.contains_focused(window, cx) {
            if let Some(ix) = self.first_pane() {
                self.active_pane = ix;
                self.hover_stale = true;
            }
            self.focus_editor(&FocusEditor, window, cx);
            return;
        }
        if !self.focus_pane_relative(1, 0, window, cx) {
            self.focus_editor(&FocusEditor, window, cx);
        }
    }

    /// Fold a change made outside Shale into the window.
    ///
    /// The tree always follows the disk. The open note is re-read only when Shale
    /// holds no unsaved edits, because reloading would throw away what the user
    /// typed; a note deleted out from under a dirty buffer stays on screen so
    /// `ctrl-s` can write it back.
    pub(crate) fn apply_disk_changes(
        &mut self,
        after: BTreeMap<PathBuf, Stamp>,
        cx: &mut Context<Self>,
    ) {
        let changes = VaultChanges::between(&self.signature, &after);
        if changes.is_empty() {
            return;
        }
        tracing::debug!(
            added = changes.added.len(),
            removed = changes.removed.len(),
            modified = changes.modified.len(),
            "the vault changed on disk"
        );
        self.signature = after;
        self.refresh_entries(cx);
        // The index follows the disk as surely as the tree does, and gets the same
        // change list. Everything in it is best-effort: a failed index is a log
        // line, never a reason the window stops showing the vault.
        self.index_changes(&changes, Provenance::External);
        // The graph follows the disk as well, when it has been built at all.
        self.graph_changes(&changes);
        self.refresh_graph(cx);
        // A note that is gone cannot be jumped to, so it leaves the chain — the
        // same rule the explicit delete follows.
        for removed in &changes.removed {
            self.jumps.forget(removed);
        }
        // Every tab whose note moved on disk, not only the one on screen: a note in
        // the tab bar is a note the user is one key away from, and a buffer holding a
        // copy of something that is no longer true is the same trap either way.
        for index in 0..self.pane().tabs.len() {
            let Some(path) = self.pane().tabs[index]
                .path()
                .map(|path| path.to_path_buf())
            else {
                continue;
            };
            let Some(editor) = self.pane().tabs[index].editor().cloned() else {
                continue;
            };
            // Own text again: the question is whether this file's buffer is ahead of
            // this file on disk, which decides whether it is safe to re-read.
            let dirty = editor.read_with(cx, |editor, _cx| editor.own_text_is_dirty());
            if changes.was_removed(&path) {
                if dirty {
                    self.say(
                        format!(
                            "{} deleted from disk; your edits are still here",
                            rel_text(&path)
                        ),
                        cx,
                    );
                } else {
                    self.clear_tab(index, cx);
                    if index == self.pane().active {
                        self.checker_closed(cx);
                    }
                    self.say(format!("{} deleted from disk", rel_text(&path)), cx);
                }
            } else if changes.was_modified(&path) {
                if dirty {
                    self.say(
                        format!(
                            "{} changed on disk; ctrl-s will overwrite it",
                            rel_text(&path)
                        ),
                        cx,
                    );
                } else {
                    // Re-read into the tab that holds it. Going through `open_note`
                    // would move the note to the tab on screen, which is not what a
                    // background tab reloading under the user means.
                    match self.vault.read(&path) {
                        Ok(text) => {
                            // What reloads is the *note*, not the tab: the buffer is
                            // shared, so one update covers every view of it, and each
                            // view is then told to drop what it measured against the
                            // text that was there (roadmap P4.17).
                            if let Some(note) = self.notes.get(&path) {
                                note.buffer.borrow_mut().set_text(text.clone());
                                note.dirty.set(false);
                                self.reload_views_of(&path, cx);
                            } else {
                                editor.update(cx, |editor, cx| editor.set_text(text.clone(), cx));
                            }
                            if index == self.pane().active {
                                self.checker_opened(&path, &text, cx);
                            }
                            self.say(format!("{} reloaded from disk", rel_text(&path)), cx);
                        }
                        Err(err) => {
                            tracing::warn!(path = %path.display(), "re-read failed: {err:#}")
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// Tell every view of `path` that its note's text was replaced under it. The
    /// shared buffer is already carrying the new text; each view only has to drop
    /// what it had measured against the old one — wrap points, folds, the
    /// checker's underlines, the search's matches (roadmap P4.17).
    pub(super) fn reload_views_of(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        for pane in &self.panes {
            for tab in &pane.tabs {
                if let tabs::TabContent::Note {
                    path: slot, editor, ..
                } = &tab.content
                {
                    if slot.as_deref() == Some(path) {
                        editor.update(cx, |editor, cx| editor.note_reloaded(cx));
                    }
                }
            }
        }
    }

    /// Re-take the vault's signature. Shale's own edits change the disk, and the
    /// watcher compares against this, so without it a save of ours would come
    /// back as somebody else's edit.
    pub(super) fn resign(&mut self) {
        self.signature = self.vault.signature().unwrap_or_default();
    }
}

/// Lowercased link targets that resolve to an existing note: file stems and
/// vault-relative paths (with and without the `.md` extension).
fn note_keys(entries: &[VaultEntry]) -> HashSet<String> {
    let mut keys = HashSet::new();
    for entry in entries {
        keys.insert(entry.stem().to_lowercase());
        if let Some(s) = entry.path.to_str() {
            let lower = s.to_lowercase();
            keys.insert(lower.clone());
            if let Some(stripped) = lower.strip_suffix(".md") {
                keys.insert(stripped.to_string());
            }
        }
    }
    keys
}
