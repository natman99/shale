# Shale — Dependency-Ordered Roadmap

> **For Hermes:** implement phase by phase; each phase's exit criteria must pass
> before the next starts. Within a phase, tasks are independent unless marked
> "→" (sequential). Feature designs live in `brainstorm.md`; the graph's
> implementation detail lives in `graph-view-plan.md`.

**Goal:** order every agreed feature so nothing is built on a foundation that
doesn't exist yet, and so the biggest unlock (the DB) lands once and feeds many
features.

**Date:** 2026-10-28. Repo state read from git log `9bce88f` + PROJECT.md
"Current status".

---

## Where Shale actually stands

**Done and solid** (from PROJECT.md status): live preview (stages 1/2a/2b —
markers hidden, callouts, collapsed rows, inline images), a deep vim mode
(motions, counts, text objects `iw/aw/ip/ap`, visual, `.` repeat, folding
`za/zc/zo/zd/zM/zR`), soft wrap, tables (model + editing + grid), file tree
with create/rename/delete **and link rewriting**, wiki-link
extraction/resolution/backlinks, `gx`/ctrl+click navigation, quick-open,
command palette, `:` command line, vault search, fuzzy matcher, theming (20
palettes + `.shale/themes/*.toml`), file watching, spellcheck/grammar via
harper-ls with the popup + code actions, image embeds, MCP stdio server
(list/read/write/search/backlinks), headless gpui test harness.

**Unbuilt from the original Goals list:** Tabs, Tiling splits, Graph view,
Database (SQLite), Database tables/Obsidian-bases, PDF export, Templates,
"Raw" mode, tree-sitter code highlighting, `/` search.

**Parked threads** (PROJECT.md, the other agent's list): `/` search, scrolling
popup panel, scrolling picker list, hover-capability check, live grammar fixes,
`K` with count, horizontal scrolling, `bench/` work. Four that were on this list
are now done in Phase 0: **autosave**, **persisting the theme switch**,
**config → `<vault>/.shale/`**, and **module splits** (nothing is over 1000 lines
anymore).

**Key insight:** three foundations unlock most of the brainstormed feature set:
1. **The DB + provenance + event log** → query notes, word counts, tag tree,
   bookmarks, link health, time machine, freshness glow, history scrubber,
   heatmap, graph layout cache, provenance filtering.
2. **Tabs** → the graph tab, local graph, note pinning.
3. **The command registry** → TOML macros, MCP tool parity, generated docs,
   future Lua.

---

## Dependency map

```
.shale/ relocation (P0) ──► SQLite DB (P1) ──┬─► provenance ──► edit review (P2)
                                  │          ├─► event log ──► history scrubber (P5)
                                  │          ├─► word_count ──► stats, :wordcount
                                  │          ├─► tags ──► tag tree ──► tag hues in graph
                                  │          ├─► links+first_seen ──► link health, link-age color
                                  │          ├─► snapshots ──► time machine ──► diff renderer
                                  │          └─► marks ──► bookmarks panel
                                  │
command registry (P2) ──┬─► TOML macros ─► Lua (later)
                        ├─► MCP tool parity + generated docs
                        └─► :Gsub family
MCP two-mode + socket (P2) ──► live agent state, capture, scoped profiles
Tabs (P3) ──► graph M1b ──► minimap, thread, pulses+glow, constellations, 3D
graph M1a (core) ── independent, START NOW
mdstyle list tree (exists) ──► outliner, markdown text objects   ← cheap, early
renderer alpha (exists) ──► focus mode, search illumination      ← cheap, early
```

---

## Phase 0 — Housekeeping & quick wins (no new foundations)

Small, unblocking, and three of them are already-parked user requests.

- **P0.1 Config & metadata into `<vault>/.shale/`** (parked #12) — **DONE
  2026-10-28, and the decision changed while it was being written:** there is no
  "keep reading the old root path". Shale has **one** config location,
  `<vault>/.shale/config.toml`. A vault that still holds a root `shale.toml` has
  that file **moved** into `.shale` on open (`Config::load_or_create`), and a
  vault with no config at all gets `Config::starter()` written: the core settings
  at their defaults, each with the comment that explains it. A write-back is a
  **splice** (`config/edit.rs`): one line's value is replaced, comments and the
  note beside the line included, and a key or table that isn't there is added —
  so the file stays incremental (only what differs from the default need be in
  it). Writes are atomic (temp + rename).
  **Persisting the live theme switch (parked #9) landed in the same pass:**
  choosing a theme — the picker's `enter` or `:theme <name>` — rewrites
  `appearance.theme` immediately; a *preview* never writes.
  Git: the demo vault's `.shale/config.toml` is committed **once** as the
  template (byte-identical to `Config::starter()`), then untracked and covered by
  `.gitignore`, so app-written config and the future DB never dirty the tree.
  **This landed before the DB**, so the DB's home is decided once: `.shale/`.
- **P0.2 Autosave** (parked #8) — **DONE 2026-10-28.** **2 s idle debounce**
  (`editor.autosave_after_ms`) + **on blur** (`observe_window_activation`) +
  **on window close** (`Window::on_window_should_close`, which also *refuses* the
  close when the note cannot be written) + the write-before-quit that already
  existed. `editor.autosave` defaults **on**. The status bar's `●` is the report;
  only a failed write prints. **Not** a no-op `:w` — with the setting off a no-op
  would lose work, so `:w` and `ctrl-s` stay real saves, and the close and quit
  paths write through `save` for the same reason. Pairs with P1's shutdown rule.
  Snapshot triggers deliberately do NOT key off
  autosaves (see brainstorm "Autosave × time machine").
- **P0.3 Module splits** (parked #10) — **DONE 2026-10-28.** The three over the
  rule are directories: `vim.rs` (2397) → `vim/{mod,motions,operators,tables,visual}.rs`
  + `vim/tests/`, `buffer.rs` (1786) → `buffer/{mod,lines,folds,display,motions,words,edits}.rs`
  + `buffer/tests.rs`, `theme/mod.rs` (1270) → `theme/{mod,builtins}.rs` +
  `theme/tests.rs`. **Nothing is over 1000 lines now**; `line.rs` (971) and
  `workspace/mod.rs` (970) are sitting on the line and split when next touched
  (see the module-sizes thread in Open threads for how each would go).
- **P0.4 Scrolling popup + scrolling picker** (parked #2, #3) — **DONE
  2026-10-28.** One model, `crates/app/src/list.rs` (`window_start` = minimal
  slide, never past the end), asked by both. The panel keeps a height it can show
  (`popup::VISIBLE_ROWS` = 8), numbers its rows absolutely so a digit still names
  the row it can see, says `{code} · {total} offers` when the list is longer than
  the panel, and **flips above its line** near the bottom of the window — which is
  why the dismiss edge is the one away from the note. `Popup::drawn()` is the one
  answer for the render, the panel's height and the click corridor. Picker:
  `RANK_LIMIT` 200 → 1000. **Left deliberately:** picker `VISIBLE_ROWS` still a
  fixed 12 (needs a measured row height), and no mouse-wheel scrolling. See the
  shared scroll model bullet in Open threads.
- **P0.5 `/` search** (parked #1) — **DONE 2026-10-28.** The open buffer, as
  decided; vault search stays `ctrl-shift-f`. `shale_core::search::find_all` is the
  matcher: literal, smart case, non-overlapping, byte ranges. `/` opens the one-line
  field at the bottom of the note (`PromptKind::Search`, which now emits `Changed`
  so the search answers while it is still being typed), `enter` closes the field and
  **keeps** the query and its matches, `n`/`N` walk them with wrap, `escape`
  abandons the search outright. The state lives in `workspace/search.rs`
  (`SearchState { query, hits, index }`); the editor is only told what to draw
  (`set_hits`), and a buffer edit re-runs the query so a highlight cannot drift onto
  whatever slid into its place. No new palette roles: matches are `highlight_bg`,
  the current one `selection`. See the `/` search thread in Open threads.
- **P0.5b Keybinding: free `ctrl-o` from QuickOpen** → jump-back (`ctrl-i`
  forward), per the Goals promise — **DONE 2026-10-29.** The chain is workspace
  state in `workspace/jumps.rs` (`Jumps { back, forward }`, `LIMIT` 100 like vim's
  jumplist), so it also feeds graph thread M5a, the bookmarks panel and hover
  history. A jump is **leaving one note for another** (wiki-link, quick-open, vault
  search, tree open, `:e`); the remembered position is the cursor's byte offset, so
  coming back lands on the line the user was reading. Re-opening the note that is
  already open is not a jump — that is also the disk-reload path. Choosing a new
  destination drops the forward half; `forget`/`rename` keep the chain off deleted
  and renamed notes. QuickOpen keeps `space f`, `:e` and the new `g f` chord.
  `ctrl-o`/`ctrl-i` are context-free, so their handlers stand down while a prompt,
  picker or popup owns the keyboard. **Next unit:** jumps *within* one note (`gg`,
  `G`, a long `/` hop) — same chain, different unit, no re-read. See the jump-chain
  thread in Open threads.
- **P0.6 "Raw" mode** — **DONE 2026-10-29.** It came in as one of the note's
  **three views** (`crates/app/src/editor/mod.rs`'s `View`: `Raw`, `Live`,
  `Preview`), because the user asked for preview mode in the same breath and the
  two are the same switch at opposite ends. The markdown scan still runs — folding,
  and whether a row is inside a fenced block, are answers about the *text* — and
  the frame decides what of it to show: raw passes no style spans, no link spans,
  no table plans and no image plans to the paint, and hides no markers; preview
  hides markers on **every** row, the cursor's own included, and makes the buffer
  read-only. Read-only lives on `Buffer` (`set_read_only`), guarded in every text
  mutator in `buffer/edits.rs` — including `undo`/`redo`, which are edits — because
  the text is reached by the vim engine, the table splices, paste and the IME
  handler by different routes. Ways in: `:raw` / `:live` / `:preview` (`source` and
  `src` alias raw, `view` aliases preview) and three palette rows; the status bar
  badges the view whenever it is not live. **2026-10-29 follow-up:** the spelling
  and grammar checker is **off in preview** — no wavy lines (hidden at the paint,
  by `MarkdownEditor::drawn_marks`, so the server's marks survive for `:live`), no
  panel from `K` or a hover, no count in the status bar, and a panel that was open
  is closed on the way in rather than left holding the keyboard. The view is now
  named **once**, by the accent-coloured badge, instead of by the badge and a status
  message. **Left:** the view is not in the config yet (config thread 15, with the
  `live_preview` key it replaces and that migration wrinkle), preview still shows a
  cursor rather than none, and the view chords (`space r` / `space l` / `space p`)
  are written up under P3.1, where the rest of the leader belongs.
- **P0.7 Markdown text objects** — **DONE 2026-10-29.** `i`/`a` now take `L` (a
  wiki-link, embed or `[text](url)`), `t` (a table cell), `#` (the section a heading
  opens), `*` (a callout box) and a backtick (a fenced block, or an inline code span
  when the cursor is not inside one) alongside `w` and `p`. `crates/core/src/mdstyle/object.rs`
  answers all four from the `style_spans` scan the live preview already runs — no
  second markdown parser, and the scan's fence awareness means a `#` in a code block
  cannot open a section — while the cell comes from the `Located` the table motions
  already parsed. **The insight that made this cheap:** a scope letter is a *literal
  argument*, and gpui sends unbound printable characters to the IME input handler,
  which the editor already forwards into `Vim::normal` while
  `waiting_for_literal()` — so `#`, `*` and the backtick needed **no keymap
  entries** (unlike the `space r` chord, whose second key is not an argument). A
  missing object cancels the operator, counts stay a word/paragraph feature, and a
  read-only note refuses every operator but yank. **Fixed the same day:** a callout
  that is **one line long** had an empty inner object, so `ci*` / `di*` / `yi*` did
  nothing at all — with no body lines its title text *is* what the box says, and that
  range stops at the header line's end so the `[!kind]` marker survives. **Left:**
  nothing from the brainstorm list — `diq` came in the same day (see P0.7c).
- **P0.7b The system clipboard, both ways** — **DONE 2026-10-29.** `space y` (`:yank`
  / `:clip`) out — the selection if there is one, otherwise the last yank — and `:paste`
  / `ctrl-shift-v` in, where a paste also fills the vim register so `p` and `.` repeat
  what came from outside. **Same day, at the user's request:** `y` and `d` are
  mirrored with the system clipboard the way Zed does it — the register is written out
  whenever it changes, and a `p` whose clipboard holds something Shale did not put
  there pastes that instead. The clipboard is read only on a key that will really
  paste (`Vim::awaiting_chord`), and the editor remembers the last text it wrote or
  read so neither half costs anything when nothing changed. That exposed a real gap:
  linewise `dd` never filled the register, so `p` after `dd` pasted a stale yank.
  `c` is a cut as well now: `cw` / `cc` fill the register, as visual mode already did.
- **P0.7c Smart-quote rendering** — **DONE 2026-10-29.** The rendering views draw
  the curly pair and the apostrophe; the file keeps the straight bytes the keyboard
  typed, and raw mode shows the file exactly as it is. `preview::PieceMap` now
  carries a third kind of piece: a substituted character is a piece of its own, one
  byte of note and as many painted as the glyph needs, so every other piece still
  maps with a single subtraction and nothing maps from the middle of a run.
  `Piece::painted_at` and `Piece::offset_at` are the only two places that know the
  rule, and spans, marks, links, the cursor, clicks and selection painting follow
  for free. One `quote_cache` per buffer change feeds three builders: the display
  line, a table cell, and the wrap model — which now measures the string the frame
  paints, with `RowPaint` carrying a row's pieces and its substitutions together
  (whether a row was rewritten is part of the wrap cache key).
  `offset_for_visible_index` is gone; the map does that job. The arithmetic and the
  reasoning are PROJECT.md thread 22. **Fixed the same day:** the scan treated a
  newline as if the previous line's last character were still before the mark, so a
  quotation on the line under a list item, a callout or a fence had no pair — not a
  quirk of the block parser, and a test now asks every object from the line below
  another block to prove the rest were never broken.
- **P0.8 Focus / typewriter mode** (brainstorm ✅) — **DONE 2026-10-30.**
  `crates/app/src/dim.rs` answers one question per buffer row: how far is this row
  from the block the caret is in? Three steps down to a floor of 0.3, so the rest
  of the note goes quiet rather than away. A block is a run of non-blank rows; a
  heading row and each blank row are blocks of their own, which is what makes the
  ramp follow the note's shape instead of counting lines. `scope = "section"` asks
  the markdown object parser for the heading section the caret sits under and lights
  all of it, falling back to the block when the caret is above the first heading.
  The alpha is applied to each coalesced run in `line/runs.rs` (one colour per run,
  not per byte) and to the code-block, callout and thematic-break quads; the caret,
  selection and search-hit backgrounds are never faded, and raw mode draws no fade
  at all, the same rule that keeps its markdown syntax and embeds. `space d` and
  `:focus`/`:dim` (badge in the status bar, no status message to sit beside it);
  `:typewriter` reports in the status line, because nothing on screen shows it.
  Typewriter scroll centres the caret using the previous frame's child bounds, so it
  costs no measuring of its own, and acts only when the caret moved, which leaves a
  reader who scrolled by hand alone.
  **Not config keys** (the user's ruling, 2026-10-30): they are state the user
  chose in a running window, so they belong with the session file, not with the
  settings they edit by hand — see P1.8, which now lists them. The window comes up
  with both off and the scope at `block`, and `:focus block|section` is how the
  scope is set for the session.
  **Left → all of it is P0.9:** the `sentence` scope is not implemented (the word
  parses and admits it is not built); the semantic-unit model the brainstorm rules
  is what P0.9 adds, with this ramp kept as a fourth granularity; laser mode and
  the three-state cycle chord are P0.9's; neither mode persists yet, which is
  P1.8's job rather than a config splice; no mouse wheel yet, which is what would
  make the ramp follow a scroll.

**Exit criteria:** config lives in `.shale/` and a root `shale.toml` is *moved*
there on open rather than read from, autosave on by default, no file over 1000
lines, the popup and the picker both scroll, `/` and the three views work,
`ctrl-o`/`ctrl-i` walk the jump chain, the markdown text objects answer `i`/`a`,
all existing tests green.

**Met 2026-10-30.** Phase 0 is closed. The last criterion to fall was the file
size: `line.rs`, `tests/popup.rs` and `workspace/mod.rs` were each over 1000 lines
and are now `line/{mod,runs,tests}.rs`, `tests/popup/{mod,keyboard,pointer}.rs`
and `workspace/{mod,testhooks}.rs`. The largest file in the tree is
`editor/mod.rs` at 948.

### Phase 0 addendum (2026-10-29 renderer rulings — one revisit)

- **P0.9 Focus mode completion** (reopens P0.8's scope, small) — **deferred behind
  Phase 1** (ruled 2026-10-30; Phase 0 is closed and the DB is the unlock).
  **Reconciled 2026-10-30, because P0.8 and the ruling are not the same feature.**
  What shipped is the **distance ramp**: one alpha per buffer row measured from the
  caret's row, three steps down to a floor, with the lit region being the caret's
  `block` or its heading `section`. What the brainstorm rules (semantic version,
  2026-10-29) is **dim by semantic unit**: everything outside the current unit drops
  to one low alpha — no ramp — at granularity `sentence | paragraph | section`,
  default `paragraph`, and the ramp survives only as **a fourth granularity for
  anyone who liked it**. So P0.8 is done and correct as the ramp; it is not the
  ruled default, and P0.9 is what makes it the ruled default:
  - **Unit dimming, not distance.** `dim::row_alphas` grows a second answer: rows in
    the unit are 1.0, rows outside it are the floor. The unit boundaries come from
    `mdstyle::object` — `Section` is already answered there (P0.8 uses it), and
    `paragraph` is the block model `dim.rs` already has, renamed to the ruled word
    with `block` kept as an alias.
  - **`sentence` is the one genuinely new range.** `:focus sentence` already parses
    and already admits it is not built (`focus scope sentence is not built yet;
    using block`), so the seam exists and the user is never misled in the meantime.
  - **Laser mode:** the chrome — file tree, status bar, tab bar when there is one —
    drops to the same low alpha. Renderer work in `workspace/render.rs` and the
    tree; no new state beyond a flag.
  - **One keybinding cycles** off → unit → unit+laser. Today `space d` is a
    two-state toggle, so the chord becomes a three-state cycle and the badge has to
    say which of the three it is in.
  - **The ramp stays**, as a fourth granularity word, so nothing shipped gets
    removed.
  - **Persistence follows P0.8's ruling: session state (P1.8), not config.**
  - **Why it waits:** the brainstorm wants focus dimming, search-illumination and
    the agent glow to be *three sources, one alpha system*. `dim.rs` is that seam
    already, and the other two sources arrive with P2/P5. Doing P0.9 now would
    settle the unit model once and then revisit it for the second source; doing it
    after Phase 1 lands the DB that both of those read from.

---

## Phase 1 — The DB spine (the big unlock)

- **P1.1 SQLite in `crates/core`** — **DONE 2026-10-30.** `rusqlite` 0.41 with
  **`bundled`**, so SQLite is compiled from source and every platform gets the same
  version (the user ruled `rusqlite` in; the "no external bindings" rule stands for
  everything else). `crates/core/src/db/`: `Db::open` puts the file at
  `<vault>/.shale/shale.db`, creates `.shale/` if the vault has no state directory
  yet, and starts the writer thread. Every connection sets `journal_mode = WAL`,
  `busy_timeout = 4000` and `synchronous = NORMAL` — WAL because the standalone
  `shale-mcp` and the app's readers have to work while the writer holds the write
  lock, NORMAL because a power loss losing the last few index rows is acceptable
  when the index can be rebuilt. The schema is applied on the calling thread before
  the writer starts, so a schema this build cannot handle is an error from `open`
  rather than something the queue discovers later. A DB written by a **newer**
  Shale is refused out loud: deleting `.shale/shale.db` is always a valid repair,
  silently rebuilding is not.
- **P1.2 Schema** — **DONE 2026-10-30** (`db/schema.rs`, version stamped in `PRAGMA
  user_version`, every statement `IF NOT EXISTS` so re-running it on launch is the
  normal path). The tables are exactly as listed below, with two notes:
  **timestamps are unix epoch seconds in an `INTEGER`** (SQLite renders them with
  `datetime(col, 'unixepoch')`, they sort and subtract, and it keeps a date library
  out of `shale-core` for a column the editor never parses), and **`events` carries
  an `id INTEGER PRIMARY KEY` rowid** the sketch below does not mention — the
  scrubber needs a stable order within the second a burst of events was stamped,
  and an append-only log is what a rowid is good at.
  - `notes(path PK, created_at, modified_at, word_count, created_by,`
    last_modified_by, open_count, edit_count)`
  - `tags(note, tag)` (flat strings; the tree is built by splitting on `/`)
  - `links(source, target, first_seen_at, last_seen_at)` — the `first_seen`
    that powers link-age coloring and the scrubber
  - `events(note, kind, at, provenance, detail, line_start, line_end)` —
    **append-only**; kinds: created/edited/opened/linked/renamed/deleted.
    This is the scrubber's data. The optional line-span is captured at edit
    time from the save diff and powers the provenance badges + editor glow
    (brainstorm: agent presence). **Spans are approximate, decaying
    signals — not anchors**; line numbers drift after later edits and that
    is acceptable because every consumer expires in minutes. Do not build
    permanent annotations on them.
  - `marks(name, path, position, label, created_at)`
  - `edges(src, kind, dst)` — typed edges (brainstorm: typed edges ✅):
    frontmatter link-properties become edges at index time; body wikilinks
    are kind `link`. One directed row per fact; display inverses live in the
    edge-type registry, never in the DB.
  **Version 2 (2026-10-31, with P1.3)** adds two columns the indexer needs and the
  sketch above implies but does not name: **`notes.size`** (bytes, so an edit that
  lands inside the second the note was last indexed is still a change — `modified_at`
  is only as fine as a second) and **`links.target_kind`** (`note` / `attachment` /
  `ghost`, which is what makes an unresolved link list possible at all; a ghost's
  `target` is what the author wrote, not a path).
  An index at an **older** version is **dropped and rebuilt**, never migrated: the
  notes hold everything the index has, so growing a migration framework around a
  cache would be machinery protecting nothing. A **newer** version is still refused
  out loud, because rebuilding there would throw away rows the newer Shale expects.
  `SCHEMA_V1` stays in `schema.rs` (test-only) so the rebuild test builds a real v1
  rather than a sketch of one.
- **P1.3 Incremental indexing + startup reconciliation** — **DONE 2026-10-31**
  (`db/index.rs` for the work, `workspace/index.rs` for the window's half).
  `Db::reconcile(&vault)` turns the index back into a signature — the same shape
  `Vault::signature()` returns — and diffs it against the disk with the watcher's own
  `VaultChanges::between`, so there is one diff in the project rather than two. It
  runs once, on a background thread, at startup, and ends; after that the watcher's
  ticks call `Db::sync` with the change list they already computed, and Shale's own
  save / create / rename / delete paths report themselves, because their writes are
  re-stamped out of the watcher's view and nothing else would ever notice them.
  `notes` (path, created/modified from the stamp, `size`, `word_count`), `tags`
  (reusing `graph::extract_tags`) and `links` (wiki-links through
  `Vault::resolve_link` then `resolve_attachment`, ghosts kept as written) are all
  filled.
  - **Reading and parsing happen outside the write transaction**, on the caller's
    thread: the writer is the vault's only one, and one unreadable note must cost its
    own row rather than the pass. `SyncReport.failed` names them and the pass stands.
  - **`created_at` is when the index first saw the note**, not the filesystem's birth
    time — `watch::Stamp` does not read one. Re-indexing never moves it, and a link's
    `first_seen_at` survives a re-index for the same reason (links are read back,
    replaced, and re-stamped with `last_seen_at`).
  - **The index is a cache and is treated as one**: `index.enabled = false` in the
    config, or a DB that will not open, leaves `Workspace.db` as `None`, which is a
    missing feature reported in the log rather than a broken window — the checker's
    posture. Every index call from the window is best-effort; a failed index is a log
    line and never fails the save that triggered it.
  - **The test harness turns the index off** (`tests::workspace`) so 250 keymap and
    paint tests do not each pay for a DB file and a writer thread; `tests::index`
    turns it back on with `indexed_vault`.
  - What is *not* indexed yet: frontmatter (so `edges` stays empty until P3.3), and
    the note's own `open_count` / `edit_count`, which need P1.8 to say what an open
    is. Line spans for `events` are not captured yet either — they want the save
    diff.
- **P1.4 Provenance stamped on every write path** — **half done 2026-10-31, and the
  half that is left does not exist yet.** `Provenance` is a closed enum (`User`,
  `Mcp`, `Rename`, `Template`, `External`) and every write path the app has stamps
  it: `notes_saved` / `note_created` / `note_deleted` carry `User`, the rename path
  carries `Rename`, the watcher carries `External`, and the value lands in both
  `notes.last_modified_by` and `events.provenance`. MCP, template and capture
  provenance arrive with the features that write notes (P3.4, P5.2); there is no
  separate helper to call, because the provenance is an argument on the one call each
  path already makes.
  **What counts as an event is a decision, not an accident:** `events` records
  create, delete, rename, and a modify whose provenance is *not* the user. A plain
  user save writes no row — autosave fires every two seconds of quiet, and what the
  user typed is git's history (PROJECT.md, "What the DB is *not* for"). The startup
  pass writes no events at all: "new to the index" is not the same statement as
  "created".
- **P1.5 Shutdown integrity rule** (brainstorm ✅, binding): SIGTERM = graceful
  shutdown (flush DB, write pending edits, finish in-flight saves); all writes
  atomic (temp + rename); SIGKILL out of scope. **Land this with the DB** —
  it's the first stateful thing that can be corrupted.
  **Landed 2026-10-31.** The correction the rule needed on the way in: "all writes
  atomic" was *not* yet true — `Vault::write` was a plain `fs::write`
  (truncate-then-write) until this. `crates/core/src/atomic.rs` is now the one
  implementation every file Shale owns goes through: a dot-prefixed temp file
  (`.name.shale-tmp`, so a crash leftover is invisible to the vault walk rather
  than becoming a note), `sync_all`, then `rename`; the config splice's private
  temp+rename now calls it too. Two things `rename` gets wrong by default, fixed
  in the module: it consults the *directory*, not the target, so a `chmod 444`
  note would be replaced over the user's lock (the write now refuses — which is
  what the refuse-to-close test depends on), and the replacement inode starts at
  the default mode, so the old mode is copied onto the staging file. No
  *directory* fsync: the one durability step whose cost is disproportionate, and
  after a power cut the worst case is the rename not yet recorded, never a corrupt
  note. `Db::flush()` is the drain — a `wal_checkpoint(TRUNCATE)` job queued behind
  everything queued before it (the channel is FIFO), leaving one DB file and no
  `-wal`. Signals: `main` installs SIGTERM/SIGHUP/SIGINT handlers that only do an
  atomic store (a signal context allows nothing else) and the window polls the flag
  every 200 ms (`workspace/exit.rs`): dirty note written, index flushed, then quit
  — and `quit`/`:q` now make the same promise, index flush included. SIGKILL stays
  out of scope by definition: that is what the atomic writes are for.
- **P1.5b Single-writer DB discipline** (binding, adopted 2026-10-29 from the
  external review): one dedicated writer thread owns the SQLite write
  connection; UI, MCP handlers, and reconciliation send write requests over a
  channel queue; readers use separate connections (WAL). Short transactions,
  never held across heavy work; visible-state writes post back to the UI
  thread; the P1.5 barrier drains in-flight DB work. Decide this **before**
  P1.1 is written — retrofitting single-writer discipline is painful.
  Full rule in PROJECT.md (Database).
  **Decided and landed with P1.1 (2026-10-30), which is why it is listed first.**
  `Db` hands out no write connection at all: `write` (waits) and `queue`
  (fire-and-forget) send a closure over an `mpsc` channel to the thread `open`
  spawned, which owns the only read-write connection; `read()` gives callers their
  own connection with `PRAGMA query_only = ON`, so a reader cannot become a second
  writer by accident. **One job = one transaction**, opened and committed by the
  writer, so a multi-statement update is atomic without the caller knowing about
  transactions — and a job that fails leaves no rows and records its failure, which
  every later write reports instead of queueing work into a broken connection.
  `shutdown()` is the P1.5 barrier (close the queue, let it drain, join, report);
  `Drop` does the same without the report, so a handle that is never closed cannot
  leak its thread. What is *not* done yet: posting visible-state changes back to
  the UI thread, which needs something worth refreshing (P1.3).
- **P1.6 DB backup** — daily copy to `.shale/backups/`, keep 7. DB is cache;
  notes are the source of truth.
  **Landed 2026-10-31.** `db/backup.rs`: `Db::backup()` copies the index with
  SQLite's online backup API, as a job on the writer thread — a plain file copy
  would race the checkpoint and a torn copy is worse than no copy, and the writer
  is the one place the source cannot change mid-copy (the single tolerated
  exception to "no file I/O on the writer", bounded by the page-step size). The
  destination stages through the same `.shale-tmp` convention as every atomic
  write, so P1.5's rule holds for the copies too. Files are
  `shale-YYYY-MM-DD.db` — UTC day, so the rotation does not move with the
  timezone, and the civil-date conversion is `civil_from_days` because a date
  library is not wanted for one format. Keep 7. The window's loop
  (`workspace/index.rs`): first attempt at startup, then hourly — a directory
  listing until the day rolls over. Needs only rusqlite's `backup` feature, the
  same bundled crate.
- **P1.7 `:wordcount [folder]`** + status-bar count — **DONE 2026-10-31.** The
  command line's `:wordcount [folder]` (and `:wc`) and a palette row for the whole
  vault; the count comes from `notes.word_count` with one `SUM`, not by reading the
  notes again, which is what makes it the spine's end-to-end proof. It answers in the
  status line — "9 words in 2 notes", "…under notes/" for a folder — and says *which*
  number it is: a vault with no index says there is no index rather than inventing a
  zero. A folder is a path prefix, so `notes/` and `notesx/` are different answers.
  **Word count is defined as** whitespace-delimited runs holding at least one letter
  or digit, so a heading's `#`, a bullet's `-` and bold's `**` are not words
  (`core::db::word_count`).
  **The status-bar half is a badge, and it does not come from the DB.** The open
  note's count sits at the right end of the bar in the chrome's quiet colour, counted
  from the **buffer** on every frame: it is the one number the user watches move as
  they type, and a count that lagged a save (2 s of quiet) behind would be exactly
  the stale state the bar's segments are arranged to avoid. It is *state*, not a
  message — no countdown, and it leaves the bar entirely when no note is open. Both
  paths call `shale_core::word_count`, so the badge and `:wordcount` cannot disagree
  about a note that is saved; they answer different questions (this note now / the
  vault as indexed).
- **P1.8 Session state — what the vault looked like last time** (the user asked
  for it 2026-10-29; simple, and it belongs with the `.shale/` work). Save on a
  graceful quit and on the autosave's beat; restore on open. Contents: the open
  note with its **cursor and scroll offset**, which folders are **expanded in the
  file tree**, which **view** the note was in, **focus mode and its scope** and
  **typewriter scroll** (those three came out of `[editor]` on 2026-10-30 for
  exactly this item: they are choices made in a window, not settings the user
  edits by hand), and — once P3.1/P3.2 land — the **tabs and the pane
  layout**, including which pane held the keyboard and which
  were pinned. The jump chain is worth saving too: `ctrl-o` back through notes
  from before the restart is what the feature is for.
  **File, not DB:** `.shale/session.json`. The DB is a cache that may be rebuilt
  and is derived from the notes; this is state the user chose, and it has to
  survive a rebuild. Same atomic write as P1.5.
  **Not gating Phase 1's exit.** The half that needs no new feature (open note,
  cursor, folds, view) can land as soon as the view is a setting; the tab/pane
  half lands with P3.1/P3.2 and only extends the same file.

**Exit criteria:** DB rebuilds from an empty `.shale` in <1s on the demo vault;
external edit picked up within the watch interval; provenance correct for
user/MCP/rename writes; `kill -TERM` leaves a clean DB (verify with
`PRAGMA integrity_check`); `:wordcount` correct for vault and a subtree.
**Met 2026-10-31 except** MCP provenance (no MCP write path exists yet). The
SIGTERM half landed with P1.5 the same day. The demo-vault rebuild time and the
`kill -TERM` + `PRAGMA integrity_check` check are still to be measured by hand.

---

## Phase 2 — Command registry + agent-native surface

- **P2.1 Command registry.** Unify the palette's list (`commands.rs`) and the
  `:` vocabulary (`ex.rs`) into one registry: id, label, args, keymap context,
  handler. Palette and ex become two front-ends over it.
- **P2.2 Generated docs from the registry** (brainstorm ✅): `docs/commands.md`
  in the repo **and** served as MCP resources so an agent can read the command
  reference at runtime. Same pipeline later backs `:help`.
- **P2.3 MCP two-mode architecture** (brainstorm ✅): in-app **HTTP on
  localhost** (live state: current note, selection, jump chain, tabs) +
  standalone `shale-mcp` **stdio** (vault + DB, no live tools) + lockfile in
  `.shale` for detection/proxying. The lockfile doubles as the capture socket.
  **Operational rules (binding, 2026-10-29 review):** probe the HTTP endpoint
  before offering live tools — never trust the lockfile blindly (PID reuse);
  fall back to vault+DB with an explicit "live tools unavailable". Expose a
  `shale://status` resource (`{app_running, live_tools}`) as the pollable
  mid-session contract for app open/close. Live tools queue behind modal
  overlays and never bypass the review path. Full rules in PROJECT.md (MCP).
- **P2.4 Scoped agent vaults** (brainstorm ✅): `[mcp.profile."name"]` with
  read/write path globs, `deny`, and a tool allowlist; `shale-mcp --profile`.
  Review mode is an attribute of the profile. **Enforcement is server-side
  only** (never trust the client), `deny` globs beat `write` globs, and every
  MCP path goes through the same `Vault` normalization/confinement as the
  app's file code — no fresh path logic in MCP handlers.
- **P2.5 Agent edit review** (brainstorm ✅): pending edits as **plain-text
  files** in `.shale/pending-edits/` (DB only indexes them); review strip with
  per-hunk accept/reject; quit prompts, unanswered/killed → files persist and
  re-offer next launch. Needs a UX design pass first (strip placement, keys,
  diff rendering — reuse P4's diff renderer). **Design-pass input (ruled
  2026-10-29):** hybrid — inline phantom-region overlay in the note (dimmed
  removals, tinted additions, collapsible hunks, `]a`/`[a`/`y`/`x`) + the
  strip as the authoritative navigation/count; hunks re-anchor by diff
  context and fall back to strip-only when ambiguous. Full shape in
  brainstorm.md (agent presence cluster).
- **P2.6 Quick capture** (brainstorm ✅): `shale capture "text"` → unix socket
  (Linux/macOS) / named pipe (Windows) if the app is up, direct append if not.
  No app-grabbed global hotkeys on Linux; document the niri binding.
- **P2.7 `:Gsub` / vault-wide ex commands** (brainstorm ✅): scoped by
  search/DB, preview affected notes before applying. Registry + DB make this
  compose.
- **P2.8 Agent presence & provenance visibility** (brainstorm ✅, 2026-10-29):
  provenance badges (gutter/tab/tree, hover → event row), file-tree ticks
  (profile-colored, presence-now, distinct from the heatmap), presence by
  inference from the event log, editor-side glow (`[editor] glow`,
  timestamp-derived, never a ticking animation). All rides P1.2's event
  spans + P2.3's connection info; all config-off-able as a group; zero
  animation, zero frames when idle.

**Exit criteria:** palette and `:` dispatch the same command ids; docs
regenerate in CI/test; an external agent can drive the app over HTTP and its
edits queue as pending files that survive a `kill -TERM`; capture works with the
app both up and down.

---

## Phase 3 — Tabs, then the graph

- **P3.1 Tabs** (goal; gates everything here). Tab bar, `gt`/`gT` or
  `ctrl-w h/l` extension of the existing focus chords, close/dirty markers.
  - **`ctrl-w` is reserved as the window prefix — spend nothing else on it.**
    Tabs and splits will want nearly the whole alphabet, and a user with vim
    muscle memory will type these whether or not Shale has them yet (an unbound
    chord is a no-op; a bound one is a wrong action):
    `h j k l` / arrows move between windows · `H J K L` / S-arrows move the window
    itself to an edge · `s` `v` split below / right, and neovim 0.10's `S` `V`
    split **with the current buffer** (Obsidian's "split keeping this note") ·
    `n` `d` `f` `]` `^` `i` new window, or a window on the word / file / tag /
    alternate / include under the cursor · `c` `q` `o` close the window, close it,
    close every *other* window · `=` `z` equalize sizes · `_` `|` `+` `-` `>` `<`
    maximize and grow · `r` `R` `x` rotate / exchange · `T` `t` move a window to a
    tab page · `p` put a register into a new buffer in a window · `g]` `g^` `g#`
    preview / count the word under the cursor. Tab pages live **outside** the
    prefix: `gt` `gT` and `:tabnew` / `:tabclose` / `:tabonly` / `:tabmove` /
    `:tabnext` / `:tabprev`. The `g` prefix currently uses `gg`, `gx`, `gf`, so
    `g t` and `g T` are free.
  - **Decision (2026-10-29): the three views do *not* go under `ctrl-w`, and they
    landed the same day on the `space` leader.** `ctrl-w p` / `r` / `t` are all
    taken by vim (`p` put-register, `r` rotate, `t` new tab with the window), so
    the views are `space r` (raw), `space l` (live) and `space p` (preview),
    beside `space f` quick-open and `space s` vault search. They are **vim-engine
    chords** (`Vim::after_space` in `crates/core/src/vim/mod.rs`), not keymap
    prefixes: `space` is vim's right motion, and a keymap prefix would make every
    space in normal mode feel dead. `space l` is the one that matters most — a
    note in preview refuses typing, so the way out cannot need the command line.
    `:raw` / `:live` / `:preview` stay as the ex way in.
    **The rule this exposed:** every key a leader chord uses must be bound in the
    keymap in its own right, or the keystroke never reaches the engine to be read
    as a chord. `f`, `l`, `p` and `s` already were; `r` had to be bound (`VimR`)
    even though vim's replace-the-character is not implemented here — the same
    reason `s` is bound. An unbound printable key in normal mode reaches nothing.
- **P3.2 Tiling splits** (goal) — same pane model as tabs; `:pin` (note
  pinning, brainstorm ✅) is a pane flag: dimmed, excluded from the jump chain,
  auto-closing.
- **P3.3 Graph M1a — start immediately, in parallel with P0/P1.** No
  dependencies: `crates/core/src/graph/{index,sim,quadtree,filter}.rs` +
  petgraph. See `graph-view-plan.md` Tasks 1–4.
- **P3.4 Graph M1b** — the graph tab, canvas painting, interactions, filter
  panel (`graph-view-plan.md` Tasks 5–8).
- **P3.5 Graph M2–M5** in the plan's order: local graph + projected 3D →
  constellations + static mode → heatmap/groups/arrows → signature features
  (thread, pulses+glow, MST star-lines, nebulae, hjkl + f-jump, illumination,
  drift, orbit focus, link-age, minimap).
  **Brainstorm rulings that feed M5 (2026-10-29):** typed edges as an edge
  layer + kind filter (P1.2 `edges` table; display registry with inverse
  labels; no layout springs in v1), hook→payoff dashed arcs, region
  clustering via `region:` props, minimap constellation mode (mirrors the
  graph tab's active filter only while that tab has focus).
  Note: **glow, link-age, and the heatmap all need P1** — schedule them after
  Phase 1 lands even though the graph itself doesn't.

**Exit criteria:** tabs and splits fully keyboard-driven; graph tab opens on a
2k-note synthetic vault at 60 fps with the sim settling then sleeping (verify
`FrameLoop::Parked` in logs + `pidstat` ~0% idle with drift off).

---

## Phase 4 — DB surfaces

- **P4.1 Time machine** (brainstorm ✅): diff-based snapshots in `.shale`,
  session-based cadence (session end + ~3-min checkpoints + pre-risky-op +
  explicit saves; trivial diffs skipped; `:snapshot [label]` manual command —
  see brainstorm "Autosave × time machine"), retention 24h full → 30d hourly
  → daily, **100 MB default cap, configurable**. Per-note scrubber + diff view.
- **P4.2 Diff renderer** — shared by the time machine and P2.5's edit review.
- **P4.3 Query notes `.shq`** (brainstorm ✅): `queries/` folder, TOML
  frontmatter + SQL body, read-only connection, results cached by query-hash +
  vault mtime, `![[query-name]]` embedding. This is the "Obsidian bases" goal.
- **P4.4 Tag tree panel** (brainstorm ✅): rollup counts, fuzzy filter, Enter =
  search, `g` = filter graph, `r` = rename tag vault-wide, color assignment →
  graph groups. Plus `#` autocomplete from the DB.
- **P4.5 Marks + bookmarks panel** (brainstorm ✅): global marks (`m{char}` /
  `'{char}` across notes), named bookmarks, jump-chain view, MCP-readable.
- **P4.6 Link health + rename propagation** (brainstorm ✅): heading-anchor
  renames, tag renames, broken-link/near-duplicate/orphan/unused-attachment
  report. Extends `rename.rs`.
- **P4.7 Writing stats panel**: live counts, sentence-length histogram, reading
  time, most-used words; hosts the per-folder word-count tree.
- **P4.8 Provenance as a first-class filter** (brainstorm ✅): user/agent/both
  in graph filters, tag tree, search, query notes; agent-touched node badges.

**Exit criteria:** restore any note to any retained point; a `.shq` embedded in
three notes renders live and refreshes on edit; tag rename rewrites every note
and the DB; "notes the agent modified this week" returns the right set.

---

## Phase 5 — The graph's history layer (needs P1 + P3 + P4)

- **P5.1 Vault history scrubber** (brainstorm ✅, full detail in
  `brainstorm.md`): query over the event log at date D; **positions come from
  the current settled layout — scrubbing never re-runs the sim**; activity
  window control; playback with speed control and ripples on appearance;
  timeline markers for bulk agent sessions/renames/writing bursts;
  provenance-filtered scrubbing; jump-chain-in-the-past overlay; respects the
  constellation filter.
- **P5.2 Stats timeline** ✅ — word-count sparkline above the scrubber.
- **P5.3 Export playback** ✅ — frames rendered from the scene → GIF/MP4 (no
  screenshot-tool dependency).
- **P5.4 Parked extras:** compare mode (two dates), session view.
- **P5.5 Scrubber-linked note view** ✅ (ruled 2026-10-29; was the parked
  "ghost view"): while the scrubber is active, the open note shows its past
  self — overlay primary (past-only content dimmed in place, current-only at
  full alpha, positions locked; P4 diff renderer aligns blocks, same visual
  grammar as the review overlay). Read-only while scrubbing; restore stays
  the time machine's separate verb. Each scrub step paints once.

---

## Phase 6 — Remaining goals & polish

- **P6.1 Outliner mode** (brainstorm ✅) — operations on the mdstyle list tree
  compiled to text edits; `outliner` keymap context on bullet lines;
  Tab/Shift-Tab, J/K subtree move, o/O sibling, subtree fold; bullets-as-pages
  (`gd`, expand-to-note).
- **P6.2 Hover link preview** — first lines + mini graph neighborhood (reuses
  the minimap machinery and the existing popup placement).
- **P6.3 TOML macros** (brainstorm 🔭) — `macros.toml`, `keys=` and
  `commands=` forms, record-from-palette, MCP-writable (`macro.create/list/
  delete`), documented from the registry.
- **P6.4 Templates + template variables** (`{{date}}`, `{{title}}`,
  `{{ask:x}}`, `{{selected}}`) — goal.
- **P6.5 PDF export / print surfaces** (goal; design ruled 2026-10-29, see
  brainstorm "Print surfaces"): the renderer's layout written to a second
  sink — a small pure-Rust PDF writer, no webview, no headless browser, no
  external converter. One export command with presets (plain / handout /
  statblock card / broadsheet) driven by the render-profile registry;
  screen and print share the profiles. Exports provenance-logged.
- **P6.6 Tree-sitter code highlighting** — goal. **Ruled ✅ (2026-10-28): C
  bindings are fine here** — the user wants the developer-friendly feature; the
  SQLite-style exception now covers tree-sitter (bundled C, build-time).
  Use `tree-sitter` + `tree-sitter-markdown` (or per-language code-block
  grammars) for fenced-block highlighting.
- **P6.7 Horizontal scrolling + wide-glyph columns** (parked #7), `K` with
  count (#6), live grammar fixes (#5), hover capability check (#4), status
  jump-chain breadcrumb (`aria ← session-12 ← hook-rats`, ruled 2026-10-29 —
  status area, not a gutter filament).
- **P6.8 Lua tier** — only if TOML macros demonstrably hit a wall.

---

## Sequencing notes

- **Start P3.3 (graph core) now**, in parallel with P0 — it has zero
  dependencies and is pure `core` + tests, so it can't conflict with UI work.
  **In flight (2026-10-28):** a second agent owns the graph track. Its files are
  `crates/core/src/graph/**`, `pub mod graph;` in `crates/core/src/lib.rs`, the
  `petgraph` lines in the workspace and core `Cargo.toml`, and `PROGRESS.md`.
  The other agent owns `PROJECT.md`, `roadmap.md`, `brainstorm.md` and everything
  else. Neither edits the other's files; `cargo` builds are still one at a time,
  so a build that fails inside `graph/` is the graph agent's work in progress,
  not a regression to fix from the outside.
- **P0.1 before P1** (the DB's home), **P1 before P2.5/P4/P5** (provenance,
  snapshots, event log).
- **P2.1 (registry) before P2.5/P2.7/P6.3** — commands, review actions,
  `:Gsub`, and macros all speak to it.
- **Tabs (P3.1) is the single biggest UI gate**; nothing in the graph plan
  before M1b can land without it.
- The **idle contract** (PROJECT.md "Graph animation & idling rules") applies
  from P0.8 (focus mode) onward, not just to the graph.
- Repo hygiene: one `cargo` build at a time, default `target/` (two parallel
  builds with separate `CARGO_TARGET_DIR` wedged the desktop once). Check for a
  running build before starting one.
- **Design gate for interaction-heavy features** (2026-10-29 review): ✅ items
  whose *interaction* is unspecified — edit-review strip, focus dimming,
  transclusion cursor handoff — get a short interaction design pass before
  implementation. This repo specifies behavior well and interaction feel
  thinly; the gate closes that gap.
- **Integration contract before graph M1b.** The two-agent file split is clean
  while the graph is pure `core`. Before M1b touches workspace state (tabs,
  jump chain, theme, idle flag), write a short contract doc: what the graph
  view may read/write, how it receives vault changes, how it respects the
  idle rules. Cheap insurance against the boundary breaking mid-phase.
- **App-side testing bar** (2026-10-29 review): the graph core has benches and
  unit tests; cross-cutting app behavior — pending edits surviving SIGTERM,
  transclusion dirty state, modes mutating live tables, provenance filters
  across surfaces — needs headless GPUI integration tests, not eyeballs.
  Each such feature lands with at least one integration test.
- **Obsidian parity ruling** (2026-10-29): round-trip the common cases
  cleanly; do not chase every edge case (properties-schema quirks, community
  plugin conventions, Canvas). Parity serves import, not identity.

## Open decisions the roadmap needs

1. ~~Tree-sitter = C bindings~~ **RESOLVED 2026-10-28:** the SQLite-style
   exception now covers it (PROJECT.md Tech Stack, and P6.6). Bundled C at build
   time is fine; runtime-linked system libraries still are not.
2. **Edit-review UX pass** before P2.5 — strip placement, keys, diff rendering.
3. ~~`/` search scope~~ **RESOLVED 2026-10-28:** `/` searches the **open buffer**;
   the vault stays `ctrl-shift-f` (see P0.5).
4. ~~PDF export route~~ **RESOLVED 2026-10-29:** headless render through the
   renderer's own layout into a pure-Rust PDF writer (no external converter);
   presets + profile registry per brainstorm "Print surfaces" (P6.5).
5. ~~Graduating `brainstorm.md` ✅ items into PROJECT.md~~ **Half done
   2026-10-28:** the Goals list now carries checkmarks for what is built. The ✅
   feature designs themselves still live only in `brainstorm.md`; graduate them
   into PROJECT.md's Open threads as each one starts, not before.
6. **Config write-back shape** — settled in P0.1 (one location, a splice, a
   generated starter). What is *not* settled: whether Shale should ever write a
   key the user has never changed. Today it writes only what the user chose.
