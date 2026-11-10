//! The markdown editor view: a scrollable, line-based text surface driven by
//! `shale_core::buffer::Buffer` and the vim engine.
//!
//! The view is split by what a frame does rather than by data type, and these
//! submodules are all part of the same `MarkdownEditor`, so they can reach its
//! private state:
//! - [`wrap`]: the display model — hidden markers, visible pieces, soft-wrap
//!   break points
//! - [`render`]: building the frame and wiring the vim key context
//! - [`actions`]: the handlers the key context binds
//! - [`input`]: gpui's input surface (IME, paste, clicks)
//! - [`overlay`]: a queued agent proposal painted in the note (roadmap P2.5b)

mod actions;
pub(crate) mod embeds;
pub(crate) mod frontmatter;
mod input;
pub(crate) mod overlay;
pub(crate) mod query_view;
mod render;
mod wrap;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use crate::note::Note;
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, Pixels, ScrollHandle, SharedString,
};
use shale_core::buffer::{Buffer, ViewSlot};
use shale_core::markdown::{extract_wikilinks, WikiLink};
use shale_core::mdstyle::{quotes, style_spans, Style, StyleSpan};
use shale_core::vault::VaultEntry;
use shale_core::vim::{Mode, Vim};
use shale_core::{theme::Palette, Vault};

use crate::dim;
use crate::line::HitLine;

use self::wrap::{CachedRow, RowPaint};

// Re-exported so the headless tests can shape text at the same size the editor
// does when a vault asks for none.
pub(crate) use crate::theme::DEFAULT_FONT_SIZE;

/// Which line space a row number is counted in.
///
/// The note on screen and the block being edited are two buffers, and each numbers
/// its own rows from zero. A row number carried between the editor and the window —
/// a pointer hover, the anchor of a panel — has to say which of the two it belongs
/// to, because the window looks rows up in the lines it painted and an embed's row
/// is not among them. Asked of the host's list, an embed's row finds either the
/// wrong line or, past the end of the note, no line at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRef {
    /// A line of the note the tab is showing, as that note's display list numbers it.
    Display(usize),
    /// A row of the block the caret is inside, as the embedded note numbers it.
    Embed(usize),
}

/// Events the editor emits for the workspace to handle.
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// `escape` was pressed in the note. The window decides what that dismisses: a
    /// preview card the pointer left behind, which has no keyboard of its own to receive
    /// the key (roadmap P6.2).
    DismissPreview,
    /// The note scrolled under the pointer's wheel. gpui's scroll wheel notifies the view
    /// that owns the scroller and nobody above it, so without this the window never hears
    /// that the text moved: a card or a panel parked over the note keeps the position it
    /// was placed at and never re-checks what the pointer is on. The note reports the
    /// wheel; the window decides what a scroll costs.
    Scrolled,
    /// The user asked to follow the wiki-link to `target`.
    FollowLink {
        target: String,
        /// The section the link names, if it names one (roadmap P4.6a). Carried with
        /// the target because opening the note is only half of what `gd` was asked to
        /// do; a `gD` creation ignores it, since a new note has no headings.
        anchor: Option<String>,
    },
    /// The user asked to follow the wiki-link to `target`, making the note first if
    /// it does not exist (`gD`). The editor does not know the vault: it reports what
    /// is under the cursor and lets the window decide what creating means.
    CreateLink { target: String },
    /// The caret is no longer inside an embed (roadmap P4.0b). It left because the
    /// user asked (`ctrl-o`), because they clicked the host's own text (ruling Q3), or
    /// because the block stopped existing under it (ruling Q5).
    ///
    /// `dirty` is whether the note being left is ahead of its file — the window says
    /// so, because a note with no tab of its own has nowhere else to show that.
    /// `reason` is set only when the exit was not the user's choice.
    EmbedExited {
        path: std::path::PathBuf,
        dirty: bool,
        reason: Option<String>,
    },
    /// The user clicked inside an embed's body (roadmap P4.0b). A click knows more
    /// than a caret: the painted row answers with the note it was cut from and the
    /// row of it, so this names the note outright.
    EnterEmbedAt {
        path: std::path::PathBuf,
        row: usize,
    },
    /// The note's text changed. The workspace uses this to keep the spell and
    /// grammar checker's copy of the document in step with the buffer.
    Changed,
    /// The pointer moved over the note, landing on buffer offset `offset` of
    /// line `line`, at the window position `at`.
    ///
    /// `line` says which line space the number is counted in: a row inside the
    /// block being edited belongs to the other buffer, and the window has to know
    /// that to find the band it was painted at.
    Hovered {
        offset: usize,
        line: LineRef,
        at: gpui::Point<Pixels>,
    },
}

impl EventEmitter<EditorEvent> for MarkdownEditor {}

/// How the note is presented, and whether it can be edited.
///
/// The three are Obsidian's three, named for what the user sees:
///
/// * [`View::Raw`] — the markdown source: no syntax colours, no hidden markers,
///   no heading sizes, no rules drawn across the row, no callout boxes, no table
///   grids, no embedded pictures. What the file on disk says, one character at a
///   time.
/// * [`View::Live`] — Shale's default: the markers on the rows the user is *not*
///   working on are hidden, so the note reads rendered while the syntax is there
///   the moment the cursor arrives.
/// * [`View::Preview`] — rendered all the way down, including on the row the
///   cursor is on, and the text is protected against edits. The cursor still
///   moves: a note that cannot be read from the top is no use.
///
/// The view belongs to the editor rather than the window because everything it
/// changes happens inside the frame the editor builds: which style spans are
/// computed at all, which rows hide their markers, and whether tables and
/// pictures are drawn as grids and images. The read-only half of [`View::Preview`]
/// is the one exception, and it is enforced down in
/// [`shale_core::buffer::Buffer::set_read_only`], because the text is reached by
/// more than one route.
/// What a query note's program answers, as one view measured it.
///
/// No name: the marker row above the table already names the query, and it is the
/// fold's own title rather than a second reading of the declaration that could
/// disagree with it.
#[derive(Debug)]
pub(crate) struct Answer {
    /// The answer itself: column names and cells, in the order the query — and the
    /// declaration's `sort` — put them in.
    pub(crate) rows: shale_core::db::Rows,
    /// The runner's own words, when it refused. A refusal is drawn, not swallowed,
    /// and it also opens the fold: a folded program with no table is a blank page.
    pub(crate) error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum View {
    /// The markdown source, unstyled and unhidden.
    Raw,
    /// Rendered except on the rows the user is working on. This is the view a
    /// note starts in.
    #[default]
    Live,
    /// Fully rendered, and read-only.
    Preview,
}

impl View {
    /// The word the status bar and the command line use for it.
    pub fn word(self) -> &'static str {
        match self {
            View::Raw => "raw",
            View::Live => "live",
            View::Preview => "preview",
        }
    }

    /// The view a saved session named (roadmap P1.8). Anything unrecognised is
    /// [`View::Live`], the view a note starts in — a session file written by a
    /// future Shale must not strand the next window in a view it cannot name.
    pub fn from_word(word: &str) -> View {
        match word {
            "raw" => View::Raw,
            "preview" => View::Preview,
            _ => View::Live,
        }
    }
}

/// What one display line is told to draw for `/`: every match that overlaps it,
/// and the current match if that overlaps too. Buffer byte ranges, clipped to the
/// line.
type LineHits = (Vec<(usize, usize)>, Option<(usize, usize)>);

/// What a closed fold inside a block puts on screen: the row it marks, that row's left
/// edge, the click ranges measured from that edge, and the words the fold says about how
/// much it is hiding.
#[cfg(test)]
pub(crate) type FoldMark = (usize, f32, Vec<(f32, f32)>, String);

/// What a hover card knows about a link, before any of it is drawn.
///
/// The editor answers this and the window turns it into a widget: the editor knows which
/// note the caret is in and how a link resolves from there, and the window knows the
/// palette, the font and where on the screen the card goes.
pub(crate) struct PreviewText {
    pub(crate) title: String,
    pub(crate) lines: Vec<String>,
    pub(crate) hidden: usize,
}

/// The embed this view is editing. See `MarkdownEditor::entered`.
pub(crate) struct Entered {
    /// The embedded note, borrowed from the window's registry — the same buffer a
    /// tab of that note would show, so the two views cannot disagree about it.
    note: Rc<Note>,
    /// Which note it is, as the registry keys it. The name the status bar shows and
    /// the way back to the body this block came from.
    path: std::path::PathBuf,
    /// The first and last rows of the embedded note on screen, inclusive: the section
    /// the embed names, not the whole note. An embed of `# Tasks` shows and edits
    /// `# Tasks`, and the seam stops `j`/`k` at that section's edges.
    first: usize,
    last: usize,
    /// The embedded note's own wiki-links, and the buffer revision they were cut from.
    ///
    /// They live here rather than beside [`MarkdownEditor::links`] because offsets are
    /// bytes of *this* note: the host's list, asked with the caret's offset inside this
    /// block, answers with a link from the wrong note. Painted on the block's rows, and
    /// asked by `gd`, `gD` and ctrl-click while the caret is inside.
    links: Vec<WikiLink>,
    links_change: u64,
    /// The checker's notes for the embedded note, as offsets into *its* buffer.
    ///
    /// The checker holds one document and follows the caret, so while the caret is in
    /// here the server is looking at this note — and its answers are bytes of this
    /// buffer. Filed among the host's marks they would underline the same byte of the
    /// wrong note.
    marks: Vec<(usize, usize)>,
    /// The host's view state, parked here while the caret is inside.
    ///
    /// The editor has one `slot`, and it belongs to whichever buffer the caret is
    /// in: display segments, folds and preview mode are offsets and flags over one
    /// buffer, and the host's mean nothing applied to the embedded note — a `j`
    /// measured against the host's rows would land on the wrong row of this one.
    /// The tab is still *painting* the host, though, so its wrap points and open
    /// folds have to survive the visit; this is where they wait.
    host_slot: ViewSlot,
}

pub struct MarkdownEditor {
    /// The editor's focus handle; the line element needs it to register input
    /// during paint, so it is crate-visible.
    pub(crate) focus_handle: FocusHandle,
    /// The buffer the caret lives in — shared, not owned.
    ///
    /// From P4.0b the same note can be on screen twice at once: open in a tab
    /// and embedded in the note beside it. Two copies of the text would drift
    /// (and the second one's save would quietly throw away the first's edits),
    /// so a note has exactly one buffer per window and every view of it borrows
    /// this one. The registry that hands them out is `Workspace::notes` (P4.17).
    ///
    /// **This is the caret's buffer, not the note this tab is showing.** Those are
    /// the same handle today and will come apart when an embed is entered (M3):
    /// the caret moves into the embedded note while the tab keeps showing its own.
    /// Anything that paints, wraps, folds, styles or saves the tab's note says
    /// `self.note.buffer` instead — the split was drawn while both sides were the
    /// same object, so it could be checked by the compiler rather than by memory.
    ///
    /// `RefCell` rather than a gpui `Entity` because the vim engine is handed
    /// `&mut Buffer` and the view's `ViewSlot` in the same call: through an
    /// `Entity`, `self.buffer.borrow().update(cx, ..)` would borrow `self` to reach
    /// `self.slot` from inside its own closure. A borrow here is a plain local.
    buffer: Rc<RefCell<Buffer>>,
    /// How *this* view lays out `buffer`: the soft-wrap break points measured at
    /// this width. It sits beside the buffer rather than inside it because the same
    /// note can be laid out two ways at once — two panes at different widths, a tab
    /// and an embed — and a display question asked of the wrong layout answers with
    /// another view's line breaks. Every display-aware buffer call names this slot.
    slot: ViewSlot,
    vim: Vim,
    /// The text Shale last put on the system clipboard, or last read from it. It is
    /// what makes the mirror cheap and honest: a yank only writes when its text
    /// differs, and a `p` only treats the clipboard as somebody else's copy when it
    /// differs too.
    clipboard_mirror: Option<String>,
    scroll_handle: ScrollHandle,
    /// Display lines captured during the last paint, for hit-testing. Written
    /// by the line element during paint, so it is crate-visible.
    pub(crate) hit_lines: Vec<HitLine>,
    /// Cached wrap boundaries: (buffer change id, wrap width, per-row starts and
    /// the live-preview state they were measured in).
    wrap_cache: Option<(u64, Pixels, Vec<CachedRow>)>,
    /// Markdown style spans for the current text (buffer byte offsets), cached
    /// until the buffer changes.
    style_cache: Option<(u64, Vec<StyleSpan>)>,
    /// The straight quotes the frame draws curly: (buffer change id, ordered
    /// `(buffer offset, glyph)` pairs, one byte each in the note). Empty in raw
    /// mode, which paints the file exactly as it is on disk.
    quote_cache: Option<(u64, Vec<(usize, char)>)>,
    /// Wiki-link spans in the current buffer text (buffer byte offsets).
    links: Vec<WikiLink>,
    /// The vault the note lives in, which an image embed's target is resolved
    /// against. `None` until the workspace supplies one, which is how the
    /// headless tests run without a vault (and without images).
    vault: Option<Rc<Vault>>,
    /// The vault index a query note's program runs against. The same `Db` the window
    /// runs `:query` with, so the file's table and the picker's rows are one answer
    /// from one read-only door — there is no second runner for agents or views to
    /// disagree with.
    db: Option<std::sync::Arc<shale_core::db::Db>>,
    /// The answer this view last measured, with the buffer's change id it was measured
    /// from. Re-run when the text moves or the index does; not on every frame.
    answer: Option<(u64, std::rc::Rc<Answer>)>,
    /// The vault's attachment files (everything that is not a note), for
    /// resolving `![[image.png]]`.
    attachments: Vec<VaultEntry>,
    /// Lowercased note names present in the vault, for resolved/unresolved
    /// link styling.
    known: HashSet<String>,
    /// Whether the input handler was registered during the current frame's
    /// paint. gpui only routes typed characters to a handler registered while
    /// painting the frame, so exactly one line element does it per frame.
    pub(crate) input_registered: bool,
    /// The family the editor is drawn with: what the config asked for until the
    /// first frame resolves it against the installed fonts.
    font_family: SharedString,
    /// Whether `font_family` has been resolved against the system's fonts.
    font_family_resolved: bool,
    /// The colours everything is drawn with, from the vault's theme. Shared so a
    /// theme change is one assignment rather than a rebuild of the view.
    palette: Rc<Palette>,
    /// Body text size in pixels; headings are scaled from it.
    font_size: f32,
    /// (start, end) buffer byte ranges the spell and grammar checker has
    /// something to say about, drawn as wavy underlines. The workspace owns the
    /// checker and pushes these; the editor only paints them.
    marks: Vec<(usize, usize)>,
    /// (start, end) buffer byte ranges the `/` search matched, drawn with a
    /// background. Like [`Self::marks`] these are pushed by the workspace, which
    /// owns the query and decides which match the cursor is on; the editor only
    /// paints them.
    hits: Vec<(usize, usize)>,
    /// The one match the cursor is on, drawn over the others so the current
    /// position in a run of `n`/`N` is visible. `None` when there is no match.
    active_hit: Option<(usize, usize)>,
    /// Which of the three views the note is in: raw source, live preview, or
    /// fully rendered and read-only. See [`View`].
    view: View,
    /// Draw everything but the block the cursor is in at reduced strength, so the
    /// note reads one block at a time. The window turns it on with `space d` or
    /// `:focus`; raw mode ignores it, because raw mode's whole point is showing the
    /// file as it is.
    focus: bool,
    /// How far the lit region reaches: the cursor's block, or its whole heading
    /// section. See [`dim::Scope`].
    focus_scope: dim::Scope,
    /// Keep the cursor's display line near the middle of the window as it moves.
    typewriter: bool,
    /// The cursor offset the last typewriter centring was done for. The frame
    /// re-centres only when the caret actually moved, so it never fights a reader
    /// who scrolled by hand.
    typewriter_anchor: Option<usize>,
    /// True when this editor's pane is not the window's active pane: its cursor
    /// block paints faded instead of solid, so the eye jumps to the live pane.
    /// The workspace sets this per pane while painting; the editor never decides
    /// it on its own.
    pub(crate) cursor_dim: bool,
    /// The agent proposals queued for the note this editor is holding, as the overlay
    /// needs them (roadmap P2.5b). The window pushes these; the editor only places and
    /// paints them. See [`overlay`].
    pending_edits: Vec<overlay::InlineEdit>,
    /// Which of those proposals' additions the user has collapsed to a count with `za`,
    /// by proposal id and hunk. Transient: it belongs to the frame, not to the queue,
    /// and a proposal that goes stale and comes back is offered open again.
    collapsed_inline: HashSet<(SharedString, usize)>,
    /// The bodies of this note's `![[transclusions]]`, cached across frames and dropped
    /// when the vault changes (roadmap P4.0a).
    embeds: embeds::Cache,
    /// The embed this view is editing, if any (roadmap P4.0b).
    ///
    /// Entering an embed moves the caret into the other note's buffer without moving
    /// this tab off its own: `buffer` becomes the embedded note's, `note` stays the
    /// host's, and the host keeps painting its own rows with the embed's body hanging
    /// off them. That is why the two handles are named apart — see `buffer`.
    ///
    /// One level only: a read-only embed of a note that is itself being edited stays
    /// read-only, and entering an embed while inside one is refused.
    entered: Option<Entered>,
    /// The fold state this view holds for each embed it has edited (ruling 16). A
    /// block's view exists only while the caret is inside it, but the folds made there
    /// belong to *this view of that note*, and dropping them on `ctrl-o` would make `za`
    /// feel like a keystroke that wears off. Keyed here, on the host's editor, because
    /// that is whose view it is: the same note open in another tab folds separately.
    embed_slots: HashMap<PathBuf, ViewSlot>,
    /// The notes this view has typed into *through* an embed: `(path, note)`, in the
    /// order they were first touched.
    ///
    /// Three jobs, all of them the same fact seen from a different side. The tab's dot
    /// is transitive — a host with an embed holding unsaved edits is a host with
    /// unsaved edits (ruling 17). The write path needs to know which extra files a
    /// `:w` here owes (ruling 18). And holding the note is what stops one the caret
    /// typed into, and which no tab shows, from being dropped the moment the caret
    /// steps out — the worst kind of wrong an editor can do.
    pub(crate) touched: Vec<(std::path::PathBuf, Rc<crate::note::Note>)>,
    /// The note this view is showing, shared with every other view of it (and
    /// with the window's registry). The text, the caret and the undo history are
    /// the note's; `dirty` is the note's too, so two views of one file cannot
    /// disagree about whether it has unsaved edits. See [`crate::note`].
    note: Rc<Note>,
    /// Where the phantom rows of the last frame were painted, and what each one is a
    /// line of. Rebuilt every paint, the same way `hit_lines` is: it is the only way
    /// back from a pixel on an embed row to the note that row belongs to (roadmap
    /// P4.0b), and a stale one would answer for a row that is no longer on screen.
    phantom_hits: Rc<RefCell<Vec<crate::line::phantom::PhantomHit>>>,
    /// Whether the pointer is over a phantom row that opens something. The editor's own
    /// mouse-move handler sets it, and the cursor it draws follows: a row that answers a
    /// click says so before the click, the way every other clickable thing in the app does.
    pub(crate) hover_door: bool,
    /// Where the *entered* embed's rows were painted this frame, each carrying the
    /// byte range of the embedded note it covers. Rebuilt every paint like `hit_lines`,
    /// and kept apart from them for the same reason the rows are `embed` rows: a click
    /// in the block answers with an offset in the other note (roadmap P4.0b).
    pub(crate) embed_hits: Vec<crate::line::HitLine>,
    /// Which row of the *host* the entered block hangs off, recorded by the frame that
    /// places it. The block's rows live inside that host line's wrapper, so it is the
    /// only scroll target a caret in the block can have: the ruling is that the host
    /// scrolls and the embed never gets its own viewport.
    entered_anchor: Option<usize>,
    /// Set by the frame when the block the caret is inside was not painted — the
    /// section it was editing is gone (ruling Q5). Render acts on it, because only
    /// render has a context to leave the embed through.
    embed_gone: Option<std::path::PathBuf>,
    /// The host row the entered block was last seen hanging off. Unlike
    /// `entered_anchor` this survives a frame in which the block is not painted, which
    /// is exactly the frame ruling Q5 cares about: the section vanished, and the caret
    /// goes back to the `![[…]]` row it came from — the last one it had.
    entered_host_row: Option<usize>,
    /// The window's other open notes. An embed reads a note from here when this
    /// window has it in hand, and from the file when it does not (roadmap P4.0b
    /// ruling 12) — so an embed of a note being edited in another tab shows the
    /// edits, not the last saved version.
    notes: Rc<crate::note::Registry>,
}

impl MarkdownEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // A tab that has no note yet owns its text outright: nothing else in the
        // window has a handle on it, and `:w` will register it under a name.
        let blank = Note::blank();
        Self {
            focus_handle: cx.focus_handle(),
            buffer: Rc::clone(&blank.buffer),
            note: Rc::clone(&blank),
            slot: ViewSlot::new(),
            vim: Vim::new(),
            clipboard_mirror: None,
            scroll_handle: ScrollHandle::default(),
            hit_lines: Vec::new(),
            marks: Vec::new(),
            hits: Vec::new(),
            active_hit: None,
            wrap_cache: None,
            style_cache: None,
            quote_cache: None,
            links: Vec::new(),
            vault: None,
            db: None,
            answer: None,
            attachments: Vec::new(),
            known: HashSet::new(),
            input_registered: false,
            font_family: "monospace".into(),
            font_family_resolved: false,
            // Until the vault's `appearance.theme` is applied, the default
            // palette, so a frame painted before then is not a surprise.
            palette: Rc::new(Palette::DEFAULT),
            font_size: DEFAULT_FONT_SIZE,
            view: View::Live,
            focus: false,
            focus_scope: dim::Scope::default(),
            typewriter: false,
            typewriter_anchor: None,
            cursor_dim: false,
            pending_edits: Vec::new(),
            collapsed_inline: HashSet::new(),
            embeds: embeds::Cache::new(),
            entered: None,
            embed_slots: HashMap::new(),
            touched: Vec::new(),
            phantom_hits: Rc::new(RefCell::new(Vec::new())),
            hover_door: false,
            embed_hits: Vec::new(),
            entered_anchor: None,
            embed_gone: None,
            entered_host_row: None,
            notes: Rc::new(crate::note::Registry::default()),
        }
    }

    /// Point this view at another note's shared state.
    ///
    /// Nothing here writes text: what the note holds *is* that note's current
    /// text, possibly with edits no file has seen yet, and replacing it with what
    /// the disk remembers would throw them away. Everything *view-shaped* goes
    /// instead — the wrap points and folds measured against the note this view was
    /// showing, the checker's underlines, the search's matches — and the wrap
    /// cache with them, because `change_id` is per buffer and two notes can share
    /// one id by coincidence.
    /// The file stem when this view's note is a query note (`.shq`) and the view
    /// is not raw; `None` otherwise.
    ///
    /// Raw is the source, and a fold whose whole job is to hide the program has
    /// no business in the view whose whole job is to show it — but the file is
    /// still a program and not a document there, so its `#`-looking lines are
    /// not sections either. The view decides which of those two facts matters.
    pub(crate) fn query_stem(&self) -> Option<String> {
        let path = self.embeds.host()?;
        if path.extension().and_then(|e| e.to_str()) != Some("shq") {
            return None;
        }
        Some(path.file_stem()?.to_string_lossy().into_owned())
    }

    pub(crate) fn set_note(&mut self, note: Rc<Note>, cx: &mut Context<Self>) {
        self.buffer = Rc::clone(&note.buffer);
        self.note = note;
        // Changing tab changes the note under the caret, so an embed being edited
        // here is left behind; its buffer outlives the view, held by the registry.
        self.entered = None;
        // And the writes owed to the note being left do not follow the tab to the
        // note arriving: they belong to the embeds of the text that was here.
        self.touched.clear();
        self.slot.clear();
        self.wrap_cache = None;
        // The styles and the curly quotes are keyed on the buffer's change id, and
        // that id belongs to a buffer that is no longer under this view. Clearing
        // them is not the only thing that keeps them honest (the id is unique per
        // buffer now) — it is also the end of the work of measuring a note that
        // stopped being the one on screen.
        self.style_cache = None;
        self.quote_cache = None;
        // The answer belongs to the program that was on screen, and that program is
        // gone: a query note's table must not be the last thing this view remembers.
        self.answer = None;
        self.vim.enter_normal();
        self.marks.clear();
        self.hits.clear();
        self.active_hit = None;
        cx.notify();
    }

    /// The disk changed under a note this view is showing: the shared buffer has
    /// already been updated by whoever noticed, so this view only has to drop what
    /// it had measured about the old text and repaint.
    pub(crate) fn note_reloaded(&mut self, cx: &mut Context<Self>) {
        self.slot.clear();
        self.wrap_cache = None;
        self.style_cache = None;
        self.quote_cache = None;
        // The answer belongs to the program that was on screen, and that program is
        // gone: a query note's table must not be the last thing this view remembers.
        self.answer = None;
        self.vim.enter_normal();
        self.marks.clear();
        self.hits.clear();
        self.active_hit = None;
        cx.notify();
    }

    /// Tell the editor which note names exist, so it can style resolved
    /// links differently from unresolved ones.
    pub fn set_known_links(&mut self, known: HashSet<String>, cx: &mut Context<Self>) {
        self.known = known;
        cx.notify();
    }

    /// Tell the editor which vault the note came from and what attachments it
    /// holds, so a row that is only an image embed can be painted as the picture.
    /// What the editor cannot discover for itself about transclusion: which note it is
    /// showing (the first link in the chain that stops an embed loop) and what notes the
    /// vault holds (what a link target resolves against).
    /// Where the last paint put each phantom row: `("gift.md:0", top, bottom)`, in the
    /// order they were painted.
    #[cfg(test)]
    pub(crate) fn phantom_rows(&self) -> Vec<(String, f32, f32, f32)> {
        self.phantom_hits
            .borrow()
            .iter()
            .map(|hit| {
                (
                    hit.origin
                        .as_ref()
                        .map(|o| format!("{}:{}", o.path.display(), o.row))
                        // A badge has no note-row address, but it does name a link, and a
                        // test needs to be able to point at it.
                        .or_else(|| hit.door.as_ref().map(|d| format!("badge:{}", d.target)))
                        .unwrap_or_default(),
                    hit.bounds.left().as_f32(),
                    hit.bounds.top().as_f32(),
                    hit.bounds.bottom().as_f32(),
                )
            })
            .collect()
    }

    /// Where the last paint put one of the note's own display lines.
    #[cfg(test)]
    pub(crate) fn line_band(&self, index: usize) -> Option<(f32, f32, f32)> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| {
                (
                    hit.bounds.left().as_f32(),
                    hit.bounds.top().as_f32(),
                    hit.bounds.bottom().as_f32(),
                )
            })
    }

    /// The note and row a point on the screen belongs to, when the point is on a
    /// phantom row of an embedded note.
    ///
    /// This is the seam's clickable half: the answer is a place in another buffer,
    /// which is what entering an embed moves the caret to (roadmap P4.0b).
    ///
    /// Nothing reads it yet — the click that enters an embed is M3 — but the record
    /// it answers from is built and pinned now, because a hit test written after the
    /// feature tends to be shaped by whatever the first caller needed.
    #[allow(dead_code)]
    pub(crate) fn origin_at(
        &self,
        point: gpui::Point<gpui::Pixels>,
    ) -> Option<shale_core::transclude::Origin> {
        self.phantom_hits
            .borrow()
            .iter()
            .find(|hit| hit.bounds.contains(&point))
            .and_then(|hit| hit.origin.clone())
    }

    /// The fold decisions this view has made. The workspace keeps them per note and
    /// hands them back when the note comes back — see [`shale_core::buffer::ViewSlot`].
    pub(crate) fn folds(&self) -> shale_core::fold::FoldState {
        self.slot.folds().clone()
    }

    /// Take back the fold decisions this view made about this note earlier.
    pub(crate) fn restore_folds(&mut self, folds: shale_core::fold::FoldState) {
        self.slot.restore_folds(folds);
    }

    /// The cursor the editor draws: a caret over the note's text, a hand over a row
    /// that answers a click by opening something. One decision, drawn and tested.
    pub(crate) fn cursor_style(&self) -> gpui::CursorStyle {
        if self.hover_door {
            gpui::CursorStyle::PointingHand
        } else {
            gpui::CursorStyle::IBeam
        }
    }

    /// The painted band of every phantom row of the last frame: `(opens something,
    /// left, top, bottom)`. A test uses it to put the pointer on a row it can name.
    #[cfg(test)]
    pub(crate) fn phantom_bands(&self) -> Vec<(bool, f32, f32, f32)> {
        self.phantom_hits
            .borrow()
            .iter()
            .map(|hit| {
                (
                    hit.door.is_some(),
                    hit.bounds.origin.x.as_f32(),
                    hit.bounds.origin.y.as_f32(),
                    hit.bounds.bottom().as_f32(),
                )
            })
            .collect()
    }

    /// The link a badge row stands for, if the point landed on one.
    ///
    /// A badge names a note the block did not paint, so the reader who clicks it is
    /// asking for that note. The door is the same one `gd` on the `![[…]]` row opens —
    /// one route, so a click and a keystroke cannot disagree about which note a written
    /// link means (roadmap P4.0a, leftover #1).
    pub(crate) fn door_at(
        &self,
        point: gpui::Point<gpui::Pixels>,
    ) -> Option<shale_core::transclude::EmbedRef> {
        self.phantom_hits
            .borrow()
            .iter()
            .find(|hit| hit.bounds.contains(&point))
            .and_then(|hit| hit.door.clone())
    }

    /// Hand this view the window's note registry. Without it an embed still works —
    /// it just reads files, which is what it did before embeds could be edited.
    /// The embed the caret's row is, if the row is one: a line that is nothing but
    /// `![[note#section]]` (roadmap P4.0b).
    ///
    /// The whole-line rule is the resolver's, not a convenience: an embed inside a
    /// sentence is a link the author is writing, and `ctrl-]` there should follow it
    /// the way `gd` does, not swallow the sentence.
    pub(crate) fn embed_under_caret(&self) -> Option<(String, Option<String>)> {
        let host = self.note.buffer.borrow();
        let (row, _) = host.row_col(host.cursor());
        let start = host.line_start(row);
        let end = host.line_end(row);
        let text = host.text()[start..end].to_string();
        let embed = shale_core::transclude::line_embed(&text)?;
        Some((embed.target, embed.anchor))
    }

    /// Step into the embed that shows `origin`'s note, landing the caret on
    /// `origin`'s row (roadmap P4.0b).
    ///
    /// The note has to be in this window's registry: a note being edited is a note
    /// the window holds, so the workspace opens it first if it is not. The editable
    /// range comes from the body this view is already showing, which makes "what can
    /// be edited" exactly "what is on screen".
    pub(crate) fn enter_embed(
        &mut self,
        origin: &shale_core::transclude::Origin,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.entered.is_some() {
            // One level of editing. Nested embeds stay read-only.
            return false;
        }
        let Some(note) = self.notes.get(&origin.path) else {
            return false;
        };
        let (Some(first), Some(last)) = self.embeds.rows_of(&origin.path) else {
            return false;
        };
        // The host's caret is not moved by entering, so it is still on the `![[…]]`
        // line when the caret comes back out — no bookkeeping needed for where the
        // author was standing.
        let row = origin.row.clamp(first, last);
        self.buffer = Rc::clone(&note.buffer);
        // The caret's slot becomes the embedded note's — the same one this view gave it
        // last time it was inside, folds and all (ruling 16) — laid out the way the block
        // is painted: one display line per row to start with. That is what `j`/`k` and
        // the ruler step over, and it has to agree with the paint or the two fall out
        // of step — the same rule the note's own wrap model obeys.
        let embed_slot = self.embed_slots.remove(&origin.path).unwrap_or_default();
        let host_slot = std::mem::replace(&mut self.slot, embed_slot);
        {
            let buf = self.buffer.borrow();
            let segments: Vec<Vec<usize>> = (0..buf.line_count())
                .map(|row| vec![buf.line_start(row)])
                .collect();
            self.slot.set_segments(segments);
        }
        // The block this caret is now inside has not been painted yet, so nothing is
        // known about where it hangs in the host.
        self.entered_host_row = None;
        self.entered = Some(Entered {
            note: Rc::clone(&note),
            path: origin.path.clone(),
            first,
            last,
            links: extract_wikilinks(note.buffer.borrow().text()),
            links_change: note.buffer.borrow().change_id(),
            // The checker has not been told about this note yet; the marks arrive when
            // it answers (workspace/notes.rs hands it over on entry).
            marks: Vec::new(),
            host_slot,
        });
        let start = self.buffer.borrow().line_start(row);
        self.buffer.borrow_mut().set_cursor(start);
        cx.notify();
        true
    }

    /// Step back out of the embed. The caret the embed was holding stays in the
    /// embedded buffer — one caret per note per window — and the host's caret is
    /// exactly where it was left.
    pub(crate) fn exit_embed(&mut self, reason: Option<String>, cx: &mut Context<Self>) {
        let Some(embed) = self.entered.take() else {
            return;
        };
        let (path, dirty) = (embed.path.clone(), embed.note.dirty.get());
        self.buffer = Rc::clone(&self.note.buffer);
        // The host's view state comes back exactly as it was left: the same wrap
        // points, the same open folds. Coming out of an embed should not be a reason
        // to re-measure the note. The embed's own view state — including any fold made
        // inside it — is kept aside, so the next visit finds it where it left it.
        self.embed_slots.insert(
            path.clone(),
            std::mem::replace(&mut self.slot, embed.host_slot),
        );
        cx.emit(EditorEvent::EmbedExited {
            path,
            dirty,
            reason,
        });
        cx.notify();
    }

    /// The note the caret is inside, if it is inside one. The command line asks,
    /// because `:rename` follows the caret (ruling Q4).
    pub(crate) fn entered_path(&self) -> Option<std::path::PathBuf> {
        self.entered.as_ref().map(|embed| embed.path.clone())
    }

    /// The note being edited was renamed: the block is still the same note, so the
    /// path it is reached through changes with it. Without this the next frame would
    /// not recognise its own block and would throw the caret out of an embed the user
    /// has just renamed — the opposite of what a rename means.
    pub(crate) fn embed_renamed(&mut self, from: &std::path::Path, to: &std::path::Path) {
        if let Some(embed) = &mut self.entered {
            if embed.path == from {
                embed.path = to.to_path_buf();
            }
        }
        // And the write owed to it follows the file, rather than being written to a
        // path that no longer exists.
        for (path, _) in &mut self.touched {
            if path == from {
                *path = to.to_path_buf();
            }
        }
    }

    pub(crate) fn is_entered(&self) -> bool {
        self.entered.is_some()
    }

    /// What the status bar calls the embed being edited. A segment that names the
    /// note, because "you are typing in another file" is the thing worth seeing
    /// before typing into it (ruling 3).
    pub(crate) fn entered_name(&self) -> Option<String> {
        self.entered
            .as_ref()
            .and_then(|embed| embed.path.file_stem())
            .map(|stem| stem.to_string_lossy().into_owned())
    }

    /// The byte of the embedded note under a point inside the entered block, if the
    /// point is on one of its rows (roadmap P4.0b). The block's rows are line elements
    /// over the embedded buffer, so they answer with an offset in *that* note — which
    /// is why their records are kept out of `hit_lines`, whose offsets the host reads
    /// as its own.
    pub(crate) fn embed_offset_at(&self, point: gpui::Point<gpui::Pixels>) -> Option<usize> {
        self.embed_hit_at(point)
            .map(|hit| hit.offset_for_x(point.x - hit.bounds.left()))
    }

    /// Which row of the block being edited the pointer is over, and the offset
    /// under it. Read from the bands this frame painted, so the row is the one the
    /// user is looking at rather than one inferred from where the pointer was.
    pub(crate) fn embed_hover_at(
        &self,
        point: gpui::Point<gpui::Pixels>,
    ) -> Option<(usize, usize)> {
        let hit = self.embed_hit_at(point)?;
        Some((hit.index, hit.offset_for_x(point.x - hit.bounds.left())))
    }

    fn embed_hit_at(&self, point: gpui::Point<gpui::Pixels>) -> Option<&HitLine> {
        self.embed_hits.iter().find(|hit| {
            hit.bounds.top() <= point.y
                && point.y < hit.bounds.bottom()
                && hit.bounds.left() <= point.x
                && point.x <= hit.bounds.right()
        })
    }

    /// Where the entered block's rows were painted: `(row of the embedded note, left,
    /// top, bottom)`. Test-only, and the same shape `phantom_rows` answers with so the
    /// two reads can be compared.
    #[cfg(test)]
    pub(crate) fn embed_rows(&self) -> Vec<(usize, f32, f32, f32)> {
        self.embed_hits
            .iter()
            .map(|hit| {
                (
                    hit.index,
                    hit.bounds.left().as_f32(),
                    hit.bounds.top().as_f32(),
                    hit.bounds.bottom().as_f32(),
                )
            })
            .collect()
    }

    /// The view state the *host* note is painted through: the editor's slot, or the
    /// one parked inside `entered` while the caret is elsewhere. Every paint and wrap
    /// call goes through this, because the tab keeps showing its own note however far
    /// the caret has wandered.
    fn host_slot(&mut self) -> &mut ViewSlot {
        match &mut self.entered {
            Some(embed) => &mut embed.host_slot,
            None => &mut self.slot,
        }
    }

    fn host_slot_ref(&self) -> &ViewSlot {
        match &self.entered {
            Some(embed) => &embed.host_slot,
            None => &self.slot,
        }
    }

    /// Pull the caret back inside the embed's section (roadmap P4.0b).
    ///
    /// Clamping after the keystroke rather than refusing the motion is what makes
    /// the seam one rule instead of several: `j`, `}`, `%`, a search's `n`, `gg` and
    /// `G` all stop at the section's edges without any of them knowing the edge
    /// exists. The ruling is that the seam is a buffer boundary and everything
    /// buffer-scoped stops there — this is where that gets enforced.
    fn clamp_to_embed(&mut self) {
        let Some(embed) = &self.entered else {
            return;
        };
        let (first, last) = (embed.first, embed.last);
        let buf = Rc::clone(&self.buffer);
        let mut buffer = buf.borrow_mut();
        let row = buffer.row_col(buffer.cursor()).0;
        if row < first || row > last {
            let target = row.clamp(first, last);
            let start = buffer.line_start(target);
            buffer.set_cursor(start);
        }
    }

    pub(crate) fn set_notes(&mut self, notes: Rc<crate::note::Registry>, cx: &mut Context<Self>) {
        self.notes = notes;
        cx.notify();
    }

    pub(crate) fn set_embed_context(
        &mut self,
        host: Option<&std::path::Path>,
        notes: Vec<shale_core::vault::VaultEntry>,
        cx: &mut Context<Self>,
    ) {
        self.embeds
            .set_host(host.unwrap_or(std::path::Path::new("")));
        self.embeds.set_entries(notes);
        cx.notify();
    }

    /// The vault moved: every cached embed body is now a guess.
    pub(crate) fn invalidate_embeds(&mut self, cx: &mut Context<Self>) {
        self.embeds.invalidate();
        cx.notify();
    }

    pub fn set_vault(
        &mut self,
        vault: Rc<Vault>,
        attachments: Vec<VaultEntry>,
        cx: &mut Context<Self>,
    ) {
        self.vault = Some(vault);
        self.attachments = attachments;
        cx.notify();
    }

    /// Hand this view the index a query note's program runs against, or `None` when
    /// the vault has none (`index.enabled = false`) — which is an answer the table
    /// says out loud rather than a table that draws nothing.
    ///
    /// Every call drops the cached answer, and that is the point: this is what the
    /// window calls after the index moved, so a note edited in another tab shows up
    /// in this file's table without the reader touching it.
    pub(crate) fn set_query_db(
        &mut self,
        db: Option<std::sync::Arc<shale_core::db::Db>>,
        cx: &mut Context<Self>,
    ) {
        self.db = db;
        self.answer = None;
        cx.notify();
    }

    /// What the query note in hand answers, as this view last measured it. `None`
    /// when the note is not a query note, or does not parse as one.
    ///
    /// The answer is re-run when the buffer's text moves (the program changed) or
    /// when `set_query_db` says the index moved. Not per frame: a frame runs on
    /// every keystroke, hover and scroll, and a query is not that cheap.
    pub(crate) fn query_answer(&mut self) -> Option<std::rc::Rc<Answer>> {
        self.query_stem()?;
        let id = self.note.buffer.borrow().change_id();
        if self
            .answer
            .as_ref()
            .is_some_and(|(cached, _)| *cached != id)
        {
            self.answer = None;
        }
        if self.answer.is_none() {
            let text = self.note.buffer.borrow().text().to_string();
            let note = shale_core::query_note::QueryNote::parse(&text).ok()?;
            let answer = match self.db.as_ref() {
                // EDGE-14 again: with the index off, "nothing answers this" would be
                // a lie about the vault rather than about the setting.
                None => Answer {
                    rows: shale_core::db::Rows::default(),
                    error: Some("there is no vault index to read (index.enabled = false)".into()),
                },
                Some(db) => {
                    // The anchor is the file the reader is standing in, the same
                    // binding `:query` makes; a body that never asks for `:current`
                    // never sees it.
                    let anchor = self
                        .embeds
                        .host()
                        .map(|path| path.to_string_lossy().into_owned());
                    let bindings: Vec<shale_core::db::Binding<'_>> = match anchor.as_deref() {
                        Some(current) => vec![shale_core::db::Binding::new(":current", current)],
                        None => Vec::new(),
                    };
                    match db.run(&note.sql, &bindings) {
                        Ok(mut rows) => {
                            if let Some(sort) = note.decl.sort.as_deref() {
                                rows.sort_by(sort);
                            }
                            Answer { rows, error: None }
                        }
                        Err(err) => Answer {
                            rows: shale_core::db::Rows::default(),
                            error: Some(format!("{err:#}")),
                        },
                    }
                }
            };
            self.answer = Some((id, std::rc::Rc::new(answer)));
        }
        self.answer
            .as_ref()
            .map(|(_, answer)| std::rc::Rc::clone(answer))
    }

    /// The font family from the config. Empty means "choose one"; an unknown
    /// name falls back to the preferred installed monospace families.
    pub fn set_font_family(&mut self, family: &str, cx: &mut Context<Self>) {
        if family.is_empty() || self.font_family_resolved {
            return;
        }
        self.font_family = family.into();
        cx.notify();
    }

    /// The palette the editor and its line elements draw with.
    pub fn set_palette(&mut self, palette: Rc<Palette>, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    /// The body text size in pixels, from `appearance.font_size`. Headings and
    /// line heights are scaled from it, and the soft-wrap model measures with
    /// it, so a frame that changed it has to re-measure every row.
    pub fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        if size <= 0. || (size - self.font_size).abs() < f32::EPSILON {
            return;
        }
        self.font_size = size;
        cx.notify();
    }

    pub fn text(&self) -> String {
        self.note.buffer.borrow().text().to_string()
    }

    /// The text of whichever buffer the caret is in — which is what the checker is
    /// holding, and what a change has to be sent as.
    pub fn caret_text(&self) -> String {
        self.buffer.borrow().text().to_string()
    }

    /// How many words the buffer holds, by [`shale_core::word_count`]: whitespace
    /// runs holding at least one letter or digit, so a heading's `#`, a bullet's `-`
    /// and bold's `**` are not words.
    ///
    /// It borrows the buffer rather than copying it, because the status bar asks on
    /// every frame. The same function backs `:wordcount`, so the badge and the
    /// command cannot disagree about a note that is saved.
    pub fn word_count(&self) -> usize {
        shale_core::word_count(self.note.buffer.borrow().text())
    }

    /// The body text size the editor paints and measures with.
    pub(crate) fn font_size(&self) -> f32 {
        self.font_size
    }

    /// The font family the editor resolves for painting, which the line element
    /// needs to shape text with the same face the wrappers measured.
    pub(crate) fn font_family(&self) -> SharedString {
        self.font_family.clone()
    }

    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.note.buffer.borrow_mut().set_text(text);
        self.note.dirty.set(false);
        // Another note, another layout: the break points were measured against the
        // text that was here, so they are meaningless until the next paint measures
        // this one. (They used to be cleared inside `Buffer::set_text`; the layout
        // is the view's, not the text's, so the view clears its own.)
        self.slot.clear();
        self.vim.enter_normal();
        // A different document, so the old note's underlines and matches are
        // meaningless.
        self.marks.clear();
        self.hits.clear();
        self.active_hit = None;
        tracing::debug!(
            lines = self.note.buffer.borrow().line_count(),
            "editor set_text"
        );
        cx.notify();
    }

    /// The ranges to draw as the checker's notes, as buffer byte offsets. Empty
    /// clears them.
    pub(crate) fn set_marks(&mut self, marks: Vec<(usize, usize)>, cx: &mut Context<Self>) {
        // Whichever buffer the caret is in is the one the checker was handed, so that is
        // the buffer these offsets belong to. The other note keeps the marks it already
        // has: they are still valid offsets into it, just not what the server is looking
        // at right now, and they refresh the moment the caret goes back.
        let changed = match self.entered.as_mut() {
            Some(embed) => {
                let same = marks == embed.marks;
                embed.marks = marks;
                !same
            }
            None => {
                let same = marks == self.marks;
                self.marks = marks;
                !same
            }
        };
        if changed {
            cx.notify();
        }
    }

    /// Which of the three views the note is in.
    pub fn view(&self) -> View {
        self.view
    }

    /// Put the note in another view.
    ///
    /// A view change is not part of an insert session, so the session ends with
    /// it: an open undo group over a buffer that has just stopped taking edits
    /// would mark the note dirty for nothing. The caches that hold what the
    /// *previous* view decided are dropped rather than revalidated — the style
    /// cache holds the spans raw mode refuses to compute, the quote cache holds
    /// the curly quotes raw mode refuses to draw, and the wrap cache holds break
    /// points measured with the old view's faces and hidden markers.
    pub fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        if view == self.view {
            return;
        }
        self.view = view;
        // Raw mode draws the `![[…]]` line and no body, so a caret inside a block
        // would be left inside something the frame refuses to paint: nothing on
        // screen shows where it is, and nothing can be done to what it is on. It
        // comes out before anything else reads the new view, so the caret is back
        // in the note the tab is showing on the same frame — and the host's caret
        // is wherever it was parked, which is the `![[…]]` row it came from.
        if view == View::Raw {
            self.exit_embed(Some("raw mode shows no embed to edit".to_string()), cx);
        }
        // Preview protects *this view*, not the text: another tab of the same note
        // can still be live, and one note has one buffer (roadmap P4.17).
        self.slot.set_read_only(view == View::Preview);
        // While the caret is inside a block, the slot that becomes current when the
        // caret comes back is the one parked in `Entered`. Marking only the current
        // one would hand back an editable view of a note in preview mode.
        if let Some(embed) = &mut self.entered {
            embed.host_slot.set_read_only(view == View::Preview);
        }
        self.vim.enter_normal();
        self.buffer.borrow_mut().end_undo_group();
        self.style_cache = None;
        self.quote_cache = None;
        self.wrap_cache = None;
        // A mode change can start or stop the computation: raw draws no table, and
        // live draws one, so the cached answer goes with the view that made it.
        self.answer = None;
        cx.notify();
    }

    // ----- focus mode and typewriter scroll ------------------------------

    /// Whether the frame dims everything but the block the cursor is in.
    pub(crate) fn focus_mode(&self) -> bool {
        self.focus
    }

    /// Turn focus mode on or off. The window owns the answer — `space d` or
    /// `:focus`, and the session file (roadmap P1.8) will own it across restarts —
    /// and the frame only asks whether to fade. Raw mode still draws no fade
    /// whatever this says.
    pub(crate) fn set_focus_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.focus {
            return;
        }
        self.focus = on;
        cx.notify();
    }

    /// How far the lit region reaches: the cursor's block or its whole section.
    pub(crate) fn focus_scope(&self) -> dim::Scope {
        self.focus_scope
    }

    /// Set how far the lit region reaches. Worth a repaint on its own: the same
    /// caret can light one paragraph or a whole section.
    pub(crate) fn set_focus_scope(&mut self, scope: dim::Scope, cx: &mut Context<Self>) {
        if scope == self.focus_scope {
            return;
        }
        self.focus_scope = scope;
        cx.notify();
    }

    /// Whether the frame keeps the cursor's line near the middle of the window.
    pub fn typewriter(&self) -> bool {
        self.typewriter
    }

    /// Turn typewriter scroll on or off. Turning it on re-centres straight away,
    /// which is what the user expects when they ask for it.
    pub fn set_typewriter(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.typewriter {
            return;
        }
        self.typewriter = on;
        self.typewriter_anchor = None;
        cx.notify();
    }

    /// The checker's ranges the editor is currently drawing.
    #[cfg(test)]
    pub(crate) fn marks(&self) -> &[(usize, usize)] {
        &self.marks
    }

    /// The checker's ranges the frame is allowed to draw, which is what the view
    /// decides. Only the tests ask.
    #[cfg(test)]
    pub(crate) fn marks_drawn(&self) -> Vec<(usize, usize)> {
        self.drawn_marks().to_vec()
    }

    /// The `/` matches to draw, and which one is current. Empty hits and `None`
    /// clear them, which is what `escape` does.
    pub(crate) fn set_hits(
        &mut self,
        hits: Vec<(usize, usize)>,
        active: Option<(usize, usize)>,
        cx: &mut Context<Self>,
    ) {
        if hits != self.hits || active != self.active_hit {
            self.hits = hits;
            self.active_hit = active;
            cx.notify();
        }
    }

    /// The matches the editor is currently drawing.
    #[cfg(test)]
    pub(crate) fn hits(&self) -> &[(usize, usize)] {
        self.hits.as_slice()
    }

    /// The current match the editor is drawing, if there is one.
    #[cfg(test)]
    pub(crate) fn active_hit(&self) -> Option<(usize, usize)> {
        self.active_hit
    }

    pub fn mode(&self) -> Mode {
        self.vim.mode()
    }

    /// Put the cursor on buffer row `row` (0-based, clamped) and scroll it into
    /// view. What vault search does with a hit; the cursor lands at the row's
    /// first character, which is where a reader wants it, and the vim engine is
    /// left in normal mode so the next keystroke is a motion.
    pub fn reveal_line(&mut self, row: usize, cx: &mut Context<Self>) {
        self.buffer.borrow_mut().goto_line(row);
        self.ensure_cursor_visible();
        cx.notify();
    }

    /// Move the caret to the heading an anchor names and scroll it into view.
    ///
    /// Returns whether the heading was found. The caller decides what to say when it
    /// was not — the workspace owns the status line, and the ruled answer is to leave the
    /// caret at the top of the note and warn, rather than to refuse the jump.
    ///
    /// The search runs over the buffer rather than the file on disk, so a jump into a
    /// note with unsaved edits lands on the heading the reader can see, not the one the
    /// disk remembers.
    pub(crate) fn reveal_heading(&mut self, anchor: &str, cx: &mut Context<Self>) -> bool {
        // The scan borrows the text; the jump below moves the caret and repaints,
        // so the scan's answer has to be a plain value before either can happen.
        let row = shale_core::transclude::heading_row(self.note.buffer.borrow().text(), anchor);
        match row {
            Some(row) => {
                self.reveal_line(row, cx);
                true
            }
            None => false,
        }
    }

    /// Put the cursor on the buffer byte `offset` and scroll it into view. What
    /// `/` and `n`/`N` do with a match: the cursor lands on the match's first
    /// byte, which is where the reader wants to be, and the vim engine is left
    /// alone, so the next keystroke is whatever mode the user was in.
    pub fn reveal_offset(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.buffer.borrow_mut().set_cursor(offset);
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn is_dirty(&self) -> bool {
        // Transitive on purpose: the host's tab lights when a note embedded in it
        // holds edits nobody has written, because closing that tab would leave them
        // nowhere (ruling 17).
        self.note.dirty.get() || self.touched.iter().any(|(_, n)| n.dirty.get())
    }

    /// Whether this note's *own* text is ahead of its file, ignoring embeds.
    ///
    /// The dot is transitive (ruling 17) and this is not. The question it answers is
    /// "is the copy on disk for THIS file out of date?", which is what decides whether
    /// Shale may write that file itself — an embed's unsaved edits say nothing about
    /// the file they are embedded in.
    pub(crate) fn own_text_is_dirty(&self) -> bool {
        self.note.dirty.get()
    }

    /// The notes this view owes a write to beyond its own file: embedded notes the
    /// caret has typed into that are still ahead of the disk. The tab's own note is
    /// excluded — its caller writes that one and knows its path.
    pub(crate) fn dirty_embeds(&self) -> Vec<(std::path::PathBuf, Rc<crate::note::Note>)> {
        self.touched
            .iter()
            .filter(|(_, note)| note.dirty.get() && !Rc::ptr_eq(note, &self.note))
            .cloned()
            .collect()
    }

    pub fn mark_clean(&mut self) {
        self.note.dirty.set(false);
    }

    /// Cursor position in buffer coordinates (row, byte column), 0-based.
    ///
    /// `pub(crate)` rather than test-only because the live MCP snapshot reports where
    /// the user is (roadmap P2.3).
    pub(crate) fn cursor_position(&self) -> (usize, usize) {
        self.buffer.borrow().row_col(self.buffer.borrow().cursor())
    }

    /// Buffer rows and wrapped display lines, for tests and diagnostics.
    #[cfg(test)]
    pub fn line_count(&self) -> usize {
        self.note.buffer.borrow().line_count()
    }

    #[cfg(test)]
    pub fn display_line_count(&self) -> usize {
        self.note.buffer.borrow().display_line_count(&self.slot)
    }

    /// One wrapper per (heading level, regular or wide face), so every row is measured
    /// with the face and size it will be painted with. They outlive the display-line pass
    /// because an embed's rows are measured with the same ones: a block that wraps with its
    /// own wrappers would break somewhere the paint has no glyphs for.
    pub(crate) fn body_wrappers(&self, window: &gpui::Window) -> Vec<gpui::LineWrapperHandle> {
        let text_system = window.text_system();
        let family = self.font_family.clone();
        let mut wrappers: Vec<gpui::LineWrapperHandle> =
            Vec::with_capacity(crate::theme::SIZES * 2);
        for level in 0..crate::theme::SIZES as u8 {
            let size = gpui::px(crate::theme::font_size(level, self.font_size));
            wrappers.push(text_system.line_wrapper(gpui::font(family.clone()), size));
            wrappers.push(text_system.line_wrapper(gpui::font(family.clone()).bold(), size));
        }
        wrappers
    }

    /// The width soft wrap was computed against: the scroll container's bounds
    /// from the previous frame.
    #[cfg(test)]
    pub fn wrap_width(&self) -> Pixels {
        self.scroll_handle.bounds().size.width
    }

    /// The height the editor was given by the layout: the scroll container's
    /// bounds from the previous frame. A pane whose column collapsed under its
    /// tab bar shows up here as zero (the P3.2 split-render bug).
    #[cfg(test)]
    pub(crate) fn scroll_probe(&self) -> (f32, f32, f32, f32) {
        let b = self.scroll_handle.bounds();
        (
            f32::from(self.scroll_handle.offset().y),
            f32::from(self.scroll_handle.max_offset().y),
            f32::from(b.size.height),
            f32::from(self.wrap_height()),
        )
    }

    #[cfg(test)]
    pub fn wrap_height(&self) -> Pixels {
        self.scroll_handle.bounds().size.height
    }

    /// The shaped lines produced by the last paint, with the byte range each one
    /// covers. Diagnostic aid: compares what is actually painted against the wrap
    /// model (a different painted font would show up here as a width mismatch).
    #[cfg(test)]
    pub fn painted_lines(&self) -> Vec<(usize, usize, Pixels)> {
        self.hit_lines
            .iter()
            .map(|hit| (hit.start, hit.end, hit.width))
            .collect()
    }

    /// Which rows the frame draws a properties band across, and where the boundary
    /// hairlines go: `(row, band, top, bottom, broken)` for every buffer row. The
    /// paint reads these same row facts, so this answers for what is drawn rather
    /// than being a second calculation of it.
    #[cfg(test)]
    pub fn frontmatter_rows(&self) -> Vec<(usize, bool, bool, bool, bool)> {
        self.row_styles()
            .iter()
            .enumerate()
            .map(|(row, style)| {
                (
                    row,
                    style.frontmatter & crate::line::FM_BAND != 0,
                    style.frontmatter & crate::line::FM_TOP != 0,
                    style.frontmatter & crate::line::FM_BOTTOM != 0,
                    style.frontmatter & crate::line::FM_BROKEN != 0,
                )
            })
            .collect()
    }

    /// How many painted pieces each display line has: one for ordinary text, one
    /// per cell for a table row. Diagnostic aid for the grid paint path.
    #[cfg(test)]
    pub fn painted_segments(&self) -> Vec<usize> {
        self.hit_lines
            .iter()
            .map(|hit| hit.segments.len())
            .collect()
    }

    /// The text each display line was painted with: the buffer's text with
    /// live-preview markers left out. This is how the preview tests see what the
    /// user would see.
    #[cfg(test)]
    pub fn painted_text(&self) -> Vec<String> {
        self.painted_pieces()
            .into_iter()
            .map(|pieces| pieces.concat())
            .collect()
    }

    /// The embedded rows this frame painted as *pictures* (the read-only blocks): the
    /// row of the embedded note behind each phantom row. Proposal additions carry no
    /// origin and are not counted.
    #[cfg(test)]
    pub(crate) fn painted_block_rows(&self) -> Vec<usize> {
        self.phantom_hits
            .borrow()
            .iter()
            .filter_map(|hit| hit.origin.as_ref().map(|origin| origin.row))
            .collect()
    }

    /// Where a block's folded headings put their disclosure: `(row, the row's left edge,
    /// the click ranges measured from that edge)` — the same space the click path asks
    /// the host's own lines in.
    #[cfg(test)]
    pub(crate) fn embed_fold_marks(&self) -> Vec<FoldMark> {
        self.embed_hits
            .iter()
            .filter_map(|hit| {
                let marker = hit.fold_marker.as_ref()?;
                Some((
                    hit.index,
                    hit.bounds.left().as_f32(),
                    marker
                        .ranges
                        .iter()
                        .map(|(a, b)| (a.as_f32(), b.as_f32()))
                        .collect(),
                    marker.line.text.to_string(),
                ))
            })
            .collect()
    }

    /// The same read for the block being edited: `(row of the embedded note, the
    /// text it was painted with)`. A row that hid its markers and a row that kept
    /// them are the same bytes in the buffer, so only the paint can tell them apart.
    #[cfg(test)]
    pub(crate) fn painted_embed_text(&self) -> Vec<(usize, String)> {
        self.embed_hits
            .iter()
            .map(|hit| {
                (
                    hit.index,
                    hit.segments
                        .iter()
                        .map(|segment| segment.line.text.to_string())
                        .collect::<Vec<_>>()
                        .concat(),
                )
            })
            .collect()
    }

    /// The painted text of every display line, one entry per painted piece: one
    /// for an ordinary line, one per cell for a table row.
    #[cfg(test)]
    pub fn painted_pieces(&self) -> Vec<Vec<String>> {
        self.hit_lines
            .iter()
            .map(|hit| {
                hit.segments
                    .iter()
                    .map(|segment| segment.line.text.to_string())
                    .collect()
            })
            .collect()
    }

    /// The byte ranges of the display lines the buffer currently models.
    #[cfg(test)]
    pub fn display_ranges(&self) -> Vec<(usize, usize)> {
        (0..self.note.buffer.borrow().display_line_count(&self.slot))
            .filter_map(|i| self.note.buffer.borrow().display_range(&self.slot, i))
            .collect()
    }

    /// Where a buffer offset is drawn on display line `index`, measured from that
    /// line's left edge. Test aid for the live-preview piece mapping.
    #[cfg(test)]
    pub fn x_for_offset(&self, index: usize, offset: usize) -> Option<Pixels> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| hit.x_for_offset(offset))
    }

    /// The window x of `offset` on the painted line `line` names: the same address
    /// [`Self::hit_line_bounds`] answers with, asked for one byte rather than the whole
    /// row. A surface that hangs off a word rather than a line needs this — the line's
    /// left edge is where the *text* starts, not where the link does.
    pub(crate) fn hit_line_x_at(&self, line: LineRef, offset: usize) -> Option<Pixels> {
        self.hit(line)
            .map(|hit| hit.bounds.left() + hit.x_for_offset(offset))
    }

    /// What the pointer would be asking about if it were resting at `at` right now: the
    /// buffer offset under it and the line that holds it.
    ///
    /// This exists because scrolling moves text under a pointer that never moved, and
    /// gpui reports no mouse move for that — so the question the pointer asked has to be
    /// asked again against the new paint. Unlike the click path there is no "nearest line
    /// above" fallback: a point in a gap is not resting on anything, and the honest answer
    /// is none. Rows of an embedded block are not answered here either — their hit records
    /// carry no byte ranges — so a block's own links keep following the mouse.
    pub(crate) fn hover_under(&self, at: gpui::Point<Pixels>) -> Option<(usize, LineRef)> {
        let hit = self
            .hit_lines
            .iter()
            .find(|hit| hit.bounds.top() <= at.y && at.y < hit.bounds.bottom())?;
        Some((
            hit.offset_for_x(at.x - hit.bounds.left()),
            LineRef::Display(hit.index),
        ))
    }

    /// The buffer offset a pointer at `x` on display line `index` lands on, using
    /// the text that was painted there.
    #[cfg(test)]
    pub fn offset_at_x(&self, index: usize, x: Pixels) -> Option<usize> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| hit.offset_for_x(x))
    }

    /// The buffer offset a pointer at the **window** x `x` lands on, on display
    /// line `index`.
    ///
    /// The line's text does not start at the window's left edge — the editor has
    /// margins, and a table row starts further in again — so the pointer is
    /// measured from where the line was painted. [`Self::offset_at_x`] takes the
    /// other coordinate space, measured from the line itself.
    pub fn offset_at_pointer_x(&self, line: LineRef, x: Pixels) -> Option<usize> {
        self.hit(line)
            .map(|hit| hit.offset_for_x(x - hit.bounds.left()))
    }

    /// The height each display line was painted at, which follows its text size
    /// (headings are taller).
    #[cfg(test)]
    pub fn painted_heights(&self) -> Vec<Pixels> {
        self.hit_lines
            .iter()
            .map(|hit| hit.bounds.size.height)
            .collect()
    }

    /// How much of each display line's colour survived the dimming system: 1.0
    /// is the palette's own, less is that colour pulled toward the background.
    /// This is how the focus-mode tests see what the frame actually drew.
    #[cfg(test)]
    pub fn painted_dims(&self) -> Vec<f32> {
        self.hit_lines.iter().map(|hit| hit.dim).collect()
    }

    /// The display lines that were painted as a thematic break's rule rather
    /// than as text.
    #[cfg(test)]
    pub fn painted_rules(&self) -> Vec<usize> {
        self.hit_lines
            .iter()
            .enumerate()
            .filter(|(_, hit)| hit.rule)
            .map(|(index, _)| index)
            .collect()
    }

    /// The display lines painted inside a callout's box (background and accent
    /// bar), which is how the paint tests check that the box follows the block.
    #[cfg(test)]
    pub fn painted_callouts(&self) -> Vec<usize> {
        self.hit_lines
            .iter()
            .enumerate()
            .filter(|(_, hit)| hit.callout)
            .map(|(index, _)| index)
            .collect()
    }

    /// The display lines that drew a callout's kind mark.
    ///
    /// The companion to [`Editor::painted_callouts`], and the whole point of it is the
    /// difference between the two lists: a box of four rows paints four boxes and exactly
    /// one mark, on the header row's first display line.
    /// Which painted rows carried an unsaved-work dot in the gutter, as
    /// `(the note's own rows, an entered embed's rows)` — positions in paint order.
    ///
    /// The second list is the one that matters for P4.0b: an embedded note being typed
    /// into has no tab of its own to light, and the dot is the only mark saying so.
    /// Which painted rows drew a checker's squiggle, as `(the note's own rows, an
    /// entered embed's rows)`. The two lists are the point: the checker holds whichever
    /// buffer the caret is in, and its notes must land on that one.
    #[cfg(test)]
    pub fn painted_marks(&self) -> (Vec<usize>, Vec<usize>) {
        let marked = |hits: &[crate::line::HitLine]| {
            hits.iter()
                .enumerate()
                .filter(|(_, hit)| hit.marked)
                .map(|(index, _)| index)
                .collect()
        };
        (marked(&self.hit_lines), marked(&self.embed_hits))
    }

    /// Which row of the entered block drew a caret, as its row in the embedded note's
    /// buffer — `None` when the block drew none.
    ///
    /// A row number rather than a paint position because a wrapped row is several painted
    /// lines over one buffer row, and the question being asked is "is the caret visible
    /// on *this line* of the block".
    #[cfg(test)]
    pub fn painted_embed_caret(&self) -> Option<usize> {
        self.embed_hits
            .iter()
            .find(|hit| hit.caret)
            .map(|hit| hit.index)
    }

    #[cfg(test)]
    pub fn painted_gutter_dots(&self) -> (Vec<usize>, Vec<usize>) {
        let dots = |hits: &[crate::line::HitLine]| {
            hits.iter()
                .enumerate()
                .filter(|(_, hit)| hit.gutter_dot)
                .map(|(index, _)| index)
                .collect()
        };
        (dots(&self.hit_lines), dots(&self.embed_hits))
    }

    #[cfg(test)]
    pub fn painted_callout_marks(&self) -> Vec<usize> {
        self.hit_lines
            .iter()
            .enumerate()
            .filter(|(_, hit)| hit.callout_mark)
            .map(|(index, _)| index)
            .collect()
    }

    /// The fold markers the last paint drew, as (display line, label). A marker
    /// is not buffer text, so it is not part of `painted_text`; this is how the
    /// paint tests see it.
    #[cfg(test)]
    pub fn painted_fold_markers(&self) -> Vec<(usize, String)> {
        self.hit_lines
            .iter()
            .filter_map(|hit| {
                hit.fold_marker
                    .as_ref()
                    .map(|marker| (hit.index, marker.line.text.to_string()))
            })
            .collect()
    }

    /// Where a line's fold markers were painted, as (display line, first x, last x) in
    /// window coordinates: what a click has to land on to toggle the fold. One entry
    /// per range, so a folded heading appears twice — the gutter's glyph, whose x's are
    /// left of the line's text, and the count that follows it.
    #[cfg(test)]
    pub fn fold_marker_bounds(&self) -> Vec<(usize, Pixels, Pixels)> {
        self.hit_lines
            .iter()
            .flat_map(|hit| {
                hit.fold_marker.iter().flat_map(move |marker| {
                    marker.ranges.iter().map(move |(x0, x1)| {
                        (hit.index, hit.bounds.left() + *x0, hit.bounds.left() + *x1)
                    })
                })
            })
            .collect()
    }

    /// The top and bottom y of display line `index` as it was painted, in window
    /// coordinates — what a test needs in order to click on that line.
    #[cfg(test)]
    pub fn hit_line_edges(&self, index: usize) -> Option<(Pixels, Pixels)> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| (hit.bounds.top(), hit.bounds.bottom()))
    }

    /// The left and right x of display line `index` as it was painted, in window
    /// coordinates: where the text actually starts, which is what the editor's
    /// margins are measured against.
    #[cfg(test)]
    pub fn hit_line_x(&self, index: usize) -> Option<(Pixels, Pixels)> {
        self.hit_lines
            .iter()
            .find(|hit| hit.index == index)
            .map(|hit| (hit.bounds.left(), hit.bounds.right()))
    }

    /// The painted record for a row, looked up in the line space it was counted
    /// in: the note's own display lines, or the block being edited's rows. The two
    /// lists both number from zero, which is exactly why a bare number cannot say
    /// which one it means.
    fn hit(&self, line: LineRef) -> Option<&HitLine> {
        let (list, index) = match line {
            LineRef::Display(index) => (&self.hit_lines, index),
            LineRef::Embed(index) => (&self.embed_hits, index),
        };
        list.iter().find(|hit| hit.index == index)
    }

    /// Where line `line` was painted, in window coordinates. The window places
    /// the popup just below it, and asks again each frame, which is what lets a
    /// popup follow the note as it scrolls.
    pub fn hit_line_bounds(&self, line: LineRef) -> Option<gpui::Bounds<Pixels>> {
        self.hit(line).map(|hit| hit.bounds)
    }

    /// Whether display line `index` is inside the scroll container's viewport as
    /// it was painted.
    ///
    /// Every line is laid out, scrolled away or not, so the window asks this
    /// before drawing a panel over a line: a panel over a word that is not on
    /// screen would float over the sidebar or the status bar instead.
    pub fn line_is_visible(&self, line: LineRef) -> bool {
        let Some(hit) = self.hit(line) else {
            return false;
        };
        let view = self.scroll_handle.bounds();
        hit.bounds.top() < view.bottom() && hit.bounds.bottom() > view.top()
    }

    /// Where the cursor is, as a buffer byte offset.
    pub fn cursor_offset(&self) -> usize {
        self.buffer.borrow().cursor()
    }

    /// Scroll the note by `dy` pixels without moving the caret, so a test can put
    /// different text under a pointer that has not moved.
    #[cfg(test)]
    pub(crate) fn scroll_by(&mut self, dy: Pixels) {
        let off = self.scroll_handle.offset();
        let max = self.scroll_handle.max_offset();
        let y = (off.y - dy).min(gpui::px(0.)).max(-max.y);
        self.scroll_handle.set_offset(gpui::point(off.x, y));
    }

    /// How far the note is scrolled, in gpui's own units and sign (the scroll
    /// handle's offset, saved and restored as it came). The session file's
    /// other half; see [`Self::restore_place`].
    pub fn scroll_y(&self) -> f32 {
        self.scroll_handle.offset().y.as_f32()
    }

    /// Put the note back where a saved session left it (roadmap P1.8): the
    /// scroll offset is applied as saved, and the cursor then goes to `offset`
    /// and is pulled into view with gpui's *minimum* scroll. So the saved
    /// scroll stands when the cursor was already on screen, and if it was not,
    /// the viewport moves only as far as showing the cursor requires.
    pub fn restore_place(&mut self, offset: usize, scroll_y: Option<f32>, cx: &mut Context<Self>) {
        if let Some(y) = scroll_y {
            self.scroll_handle
                .set_offset(gpui::point(Pixels::ZERO, Pixels::from(y)));
        }
        self.reveal_offset(offset, cx);
    }

    /// Whether the editor is in normal mode.
    ///
    /// The window asks because the pointer may only open a popup in normal mode:
    /// in insert, visual, or the middle of a vim command the keyboard is busy with
    /// something the user is doing, and taking it away would be worse than saying
    /// nothing.
    pub fn is_normal_mode(&self) -> bool {
        self.vim.mode() == Mode::Normal
            && self.vim.count().is_none()
            && self.vim.pending_operator().is_none()
    }

    /// Rewrite the note at the given byte ranges, last one first so the earlier
    /// offsets stay valid, as one undo step. Returns whether anything was done.
    ///
    /// This is how a checker's fix reaches the text: the window decides *which*
    /// ranges and what replaces them, and the editor keeps the single-owner rule
    /// over the buffer. The note is left dirty, so the checker is told about the
    /// new text by the usual path.
    pub fn apply_edits(
        &mut self,
        edits: &[(usize, usize, String)],
        cx: &mut Context<Self>,
    ) -> bool {
        if edits.is_empty() {
            return false;
        }
        let mut edits = edits.to_vec();
        edits.sort_by_key(|(start, _, _)| *start);
        self.buffer.borrow_mut().begin_undo_group();
        for (start, end, text) in edits.iter().rev() {
            self.buffer.borrow_mut().set_selection(*start, *end);
            self.buffer.borrow_mut().insert(&self.slot, text);
        }
        self.buffer.borrow_mut().end_undo_group();
        self.mark_dirty(cx);
        cx.notify();
        true
    }

    /// Cursor position in display coordinates (wrapped segments counted as
    /// lines, column counted in characters from the segment start), like a vim
    /// ruler.
    pub fn display_position(&self) -> (usize, usize) {
        let cursor = self.buffer.borrow().cursor();
        (
            self.buffer.borrow().display_index_for(&self.slot, cursor),
            self.buffer.borrow().display_col(&self.slot, cursor),
        )
    }

    /// Which row the caret is on, tagged with the line space that row is counted
    /// in. Inside a block being edited the number is a row of the other note, and
    /// anything that measures it against this window's painted lines has to know.
    pub fn caret_line(&self) -> LineRef {
        let row = self.display_position().0;
        if self.entered.is_some() {
            LineRef::Embed(row)
        } else {
            LineRef::Display(row)
        }
    }

    fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        // Dirty follows the caret, not the tab: typing inside an embed makes the
        // embedded note dirty, and the host's tab lights through it (ruling 17).
        if let Some(embed) = &self.entered {
            embed.note.dirty.set(true);
            let known = self
                .touched
                .iter()
                .any(|(_, note)| Rc::ptr_eq(note, &embed.note));
            if !known {
                self.touched
                    .push((embed.path.clone(), Rc::clone(&embed.note)));
            }
        } else {
            self.note.dirty.set(true);
        }
        // gpui queues an emit rather than running the handlers inside this
        // update, so the workspace is free to read the buffer back from its
        // handler.
        cx.emit(EditorEvent::Changed);
    }

    // ----- wiki links ---------------------------------------------------

    /// Recompute wiki-link spans for the current buffer text. Called before
    /// any link lookup or paint, since the buffer may have changed since the
    /// last call.
    fn refresh_links(&mut self) {
        self.links = extract_wikilinks(self.note.buffer.borrow().text());
    }

    /// The wiki-links of whichever buffer the caret is in, refreshed if that buffer
    /// moved since they were cut.
    ///
    /// The tab's list and the entered embed's are not interchangeable: `gd` asks with
    /// the caret's offset, and that offset is a byte of one buffer or of the other.
    /// Getting this wrong is the same mistake the `buffer` / `note.buffer` split exists
    /// to stop — a caret in one note answered out of another note's records.
    fn refresh_caret_links(&mut self) {
        match self.entered.as_mut() {
            Some(embed) => {
                let change = embed.note.buffer.borrow().change_id();
                if embed.links_change != change {
                    embed.links_change = change;
                    embed.links = extract_wikilinks(embed.note.buffer.borrow().text());
                }
            }
            None => self.refresh_links(),
        }
    }

    /// The links of the caret's buffer, as refreshed by [`Self::refresh_caret_links`].
    fn caret_links(&self) -> &[WikiLink] {
        match self.entered.as_ref() {
            Some(embed) => &embed.links,
            None => &self.links,
        }
    }

    // ----- markdown styling ---------------------------------------------

    /// Recompute the markdown style spans unless the buffer changed since the
    /// last paint. Called once per frame, before wrapping and painting.
    fn refresh_styles(&mut self) {
        let change_id = self.note.buffer.borrow().change_id();
        if self
            .style_cache
            .as_ref()
            .is_some_and(|(id, _)| *id == change_id)
        {
            return;
        }
        let spans = style_spans(self.note.buffer.borrow().text());
        self.style_cache = Some((change_id, spans));
    }

    /// Recompute the straight quotes the frame draws curly unless the buffer
    /// changed since the last paint. Called once per frame, just after
    /// [`Self::refresh_styles`], because the scan needs the style spans to know
    /// which runs are code and so must not answer for them.
    fn refresh_quotes(&mut self) {
        let change_id = self.note.buffer.borrow().change_id();
        if self
            .quote_cache
            .as_ref()
            .is_some_and(|(id, _)| *id == change_id)
        {
            return;
        }
        // Raw mode paints the file verbatim, so there is nothing to substitute and
        // no reason to look for any. Preview and live do: the note keeps its
        // straight quotes, only the glyphs change.
        let subs = match (self.view, self.style_cache.as_ref()) {
            (View::Raw, _) | (_, None) => Vec::new(),
            (_, Some((_, spans))) => quotes::marks(self.note.buffer.borrow().text(), spans)
                .iter()
                .map(|mark| (mark.offset, mark.glyph()))
                .collect(),
        };
        self.quote_cache = Some((change_id, subs));
    }

    /// The substitutions of every buffer row, in row order and note order within a
    /// row: `(buffer offset, glyph)` for each quote drawn as something else. Built
    /// with one forward walk, since rows and substitutions are both in note order.
    fn row_quote_subs(&self) -> Vec<Vec<(usize, char)>> {
        let Some((_, subs)) = self.quote_cache.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(self.note.buffer.borrow().line_count());
        let mut at = 0usize;
        for row in 0..self.note.buffer.borrow().line_count() {
            let end = self.note.buffer.borrow().line_end(row);
            let stop = at + subs[at..].partition_point(|&(offset, _)| offset < end);
            out.push(subs[at..stop].to_vec());
            at = stop;
        }
        out
    }

    /// What every buffer row is painted from: the pieces live preview leaves of
    /// it, and the quotes the frame draws curly. The wrap model and the paint both
    /// read this, and they have to agree exactly or the cursor and the glyphs come
    /// apart.
    fn row_paint(&self, hidden: &[Vec<(usize, usize)>]) -> Vec<RowPaint> {
        let subs = self.row_quote_subs();
        self.row_pieces(hidden)
            .into_iter()
            .enumerate()
            .map(|(row, pieces)| RowPaint {
                pieces,
                subs: subs.get(row).cloned().unwrap_or_default(),
            })
            .collect()
    }

    /// Every style span the frame has, in the scanner's order: what the dimming
    /// system asks about when it decides which block the cursor is in. Empty
    /// before the first scan.
    fn all_spans(&self) -> &[StyleSpan] {
        self.style_cache
            .as_ref()
            .map(|(_, spans)| spans.as_slice())
            .unwrap_or(&[])
    }

    /// The style spans overlapping `start..end`, clipped to it as absolute
    /// buffer offsets, in the order the scanner emitted them.
    fn spans_in(&self, start: usize, end: usize) -> Vec<(usize, usize, Style)> {
        let Some((_, spans)) = self.style_cache.as_ref() else {
            return Vec::new();
        };
        spans
            .iter()
            .filter(|s| s.start < end && s.end > start)
            .map(|s| (s.start.max(start), s.end.min(end), s.style))
            .collect()
    }

    /// The checker's ranges the frame is allowed to draw: none in preview, and
    /// the ones the workspace pushed in every other view.
    ///
    /// Preview hides them rather than dropping them, so leaving preview brings the
    /// underlines straight back without waiting for the server to answer again.
    fn drawn_marks(&self) -> &[(usize, usize)] {
        if self.view == View::Preview {
            &[]
        } else {
            &self.marks
        }
    }

    /// The checker's ranges that overlap `start..end`, clipped to it. Like
    /// [`Self::spans_in`] these are buffer byte offsets; the line element rebases
    /// them onto the text it actually paints.
    fn marks_in(&self, start: usize, end: usize) -> Vec<(usize, usize)> {
        self.drawn_marks()
            .iter()
            .filter(|(s, e)| *s < end && *e > start)
            .map(|&(s, e)| (s.max(start), e.min(end)))
            .collect()
    }

    /// The `/` matches that overlap `start..end`, clipped to it, and the current
    /// match clipped the same way (`None` when there is none or it is on another
    /// line). Buffer byte offsets, rebased by the line element like the marks.
    fn hits_in(&self, start: usize, end: usize) -> LineHits {
        let hits: Vec<(usize, usize)> = self
            .hits
            .iter()
            .filter(|(s, e)| *s < end && *e > start)
            .map(|&(s, e)| (s.max(start), e.min(end)))
            .collect();
        let active = self
            .active_hit
            .filter(|(s, e)| *s < end && *e > start)
            .map(|(s, e)| (s.max(start), e.min(end)));
        (hits, active)
    }

    /// Whether a link target names a note that exists in the vault.
    fn is_known(&self, target: &str) -> bool {
        let t = target.trim().to_lowercase();
        if self.known.contains(&t) {
            return true;
        }
        match t.strip_suffix(".md") {
            Some(stripped) => self.known.contains(stripped),
            None => false,
        }
    }

    /// The wiki-link containing `offset`, if any — `offset` being in the caret's buffer.
    pub(crate) fn link_at(&self, offset: usize) -> Option<&WikiLink> {
        self.caret_links()
            .iter()
            .find(|l| l.start <= offset && offset < l.end)
    }

    /// What a hover card can say about a link: the note's name, the first lines of what
    /// the link points at, and how many lines the card left out.
    ///
    /// Resolved exactly the way an embed resolves — the same vault list, an open buffer
    /// before the file, the same section anchor — so a card, a block and `gd` cannot
    /// disagree about what a written link means. `None` when the link points at no note:
    /// an unresolved link has nothing to preview, and a card that invented one would be a
    /// lie about the vault.
    pub(crate) fn preview_for(&self, link: &WikiLink) -> Option<PreviewText> {
        let vault = self.vault.as_ref()?;
        // The host is the note the *link* is written in, which when the caret is inside a
        // block is that block's note, not the tab's: a link inside an embed resolves
        // relative to the note holding it.
        let host = self
            .entered_path()
            .or_else(|| self.embeds.host())
            .unwrap_or_default();
        let embed = shale_core::transclude::EmbedRef {
            target: link.target.clone(),
            anchor: link.anchor.clone(),
            alias: link.alias.clone(),
        };
        let sources = embeds::OpenFirst {
            notes: &self.notes,
            vault,
        };
        let body = shale_core::transclude::resolve_from(
            vault,
            self.embeds.entries(),
            &host,
            &embed,
            &sources,
        );
        let shale_core::transclude::Body::Lines(rows) = body else {
            return None;
        };
        let written: Vec<String> = rows
            .iter()
            .map(|row| match &row.row {
                shale_core::transclude::EmbedRow::Text(text) => text.clone(),
                shale_core::transclude::EmbedRow::Badge(badge) => badge.label.clone(),
            })
            .collect();
        if written.is_empty() {
            return None;
        }
        // The note as it reads rather than as it is written: the same pass an embed's rows
        // go through, so a heading in the card is a heading and not a `#`.
        let painted = embeds::paint_body(&written, &[], embeds::Reveal::default());
        let shown = crate::card::VISIBLE_LINES;
        let title = match rows.iter().find_map(|row| row.origin.as_ref()) {
            Some(origin) => {
                let name = origin
                    .path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| link.target.clone());
                match link.anchor.as_deref().filter(|a| !a.trim().is_empty()) {
                    Some(anchor) => format!("{name} › {anchor}"),
                    None => name,
                }
            }
            None => link.target.clone(),
        };
        Some(PreviewText {
            title,
            lines: painted
                .into_iter()
                .map(|row| row.text)
                .take(shown)
                .collect(),
            hidden: written.len().saturating_sub(shown),
        })
    }

    fn follow_link_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        if let Some(link) = self.link_at(offset) {
            cx.emit(EditorEvent::FollowLink {
                target: link.target.clone(),
                anchor: link.anchor.clone(),
            });
        } else {
            tracing::debug!(offset, "no wiki-link at cursor");
        }
    }

    fn create_link_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        if let Some(link) = self.link_at(offset) {
            cx.emit(EditorEvent::CreateLink {
                target: link.target.clone(),
            });
        } else {
            tracing::debug!(offset, "no wiki-link at cursor");
        }
    }

    /// Pull the caret's display line into the viewport.
    ///
    /// gpui applies this in the next frame's prepaint, and only scrolls the
    /// minimum: when typewriter scroll has already put the caret in the middle of
    /// the window there is nothing for it to do, so the two can both run.
    /// Which scroller item stands in for the caret. Inside an embed that is the host
    /// line the block hangs off — the block's rows live *inside* that line's wrapper,
    /// so they are not items the scroller knows, and the ruling is that the host
    /// scrolls while the block never gets its own viewport.
    fn caret_scroll_item(&self) -> usize {
        match self.entered_anchor {
            Some(row) => {
                let host = self.note.buffer.borrow();
                let start = host.line_start(row.min(host.line_count() - 1));
                host.display_index_for(self.host_slot_ref(), start)
            }
            None => {
                let buffer = self.buffer.borrow();
                buffer.display_index_for(&self.slot, buffer.cursor())
            }
        }
    }

    fn ensure_cursor_visible(&self) {
        let index = self.caret_scroll_item();
        self.scroll_handle.scroll_to_item(index);
    }
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
