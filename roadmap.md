# Shale — Dependency-Ordered Roadmap

> **For Hermes:** implement phase by phase; each phase's exit criteria must pass
> before the next starts. Within a phase, tasks are independent unless marked
> "→" (sequential). Feature designs live in `brainstorm.md`; the graph's
> implementation detail lives in `graph-view-plan.md`; the visual grammar every
> surface must obey lives in `design-language.md` (and when a surface and that
> file disagree, the surface is the bug).

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

**Where the phases stand** (refreshed 2026-11-04, replacing the 2026-10-28
list that still called tabs and the graph unbuilt): **Phases 0, 1 and 2 are
closed** — the one Phase 2 remainder is **P2.5b**, the inline review overlay.
**Phase 3:** P3.1 tabs done; P3.2 splits landed (a + b), and 2026-11-05 closed the
session's pane-tree half and pane resize (`ctrl-w >`/`<`) — move/exchange/rotate and
`:pin` are what is left of P3.2;
P3.3/P3.4 graph M1a + M1b done and measured (a whole 5k-note frame paints in
2.3–2.6 ms at whole-vault zoom, the sim beat self-stops, idle CPU ~1%). The
multi-window arc landed since: `VaultServices` (one index, watcher, backup loop
and live MCP per vault), per-window session slots, and remembered window frames
(`784e56f`). **There is no open process-lifetime issue** — the reported
"process lingers after the last window closes" was a `pgrep -f` artifact; every
exit path was re-measured clean (PROGRESS.md, 2026-11-04). Still unbuilt from
the original Goals list: Database tables/Obsidian-bases (P4.3 `.shq`), PDF
export, Templates, tree-sitter code highlighting.

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
  **The feature half is DONE 2026-10-31.** `shale_core::session` owns the file
  (incremental like the config: every field optional, unknown fields ignored,
  corrupt file = a log line and a fresh start); `workspace/session.rs` captures
  and restores. Saved on the autosave's beat and on every graceful exit (`:q`,
  the window manager's close, a termination signal); restored once in
  `Workspace::new`, before the first frame. Contents as specced: open note +
  cursor (a byte offset — what the buffer takes directly; a stale one clamps)
  + scroll (gpui's own offset, saved as it came), expanded folders, view, focus
  mode + scope, typewriter, and the jump chain — `ctrl-o` back through notes
  from before the restart is tested end to end. Restore order: note first (no
  jump recorded), then the frame's decisions, then the chain; a note deleted
  since last time opens nothing and costs nothing else. Scroll restore applies
  the saved offset, then gpui's *minimum* cursor reveal — the saved view stands
  unless the cursor was off it.
  **Not gating Phase 1's exit.** The half that needs no new feature (open note,
  cursor, folds, view) can land as soon as the view is a setting; the tab/pane
  half lands with P3.1/P3.2 and only extends the same file. **Still open: the
  tab/pane fields**, which wait on those features exactly as written above.

**Exit criteria:** DB rebuilds from an empty `.shale` in <1s on the demo vault;
external edit picked up within the watch interval; provenance correct for
user/MCP/rename writes; `kill -TERM` leaves a clean DB (verify with
`PRAGMA integrity_check`); `:wordcount` correct for vault and a subtree.
**Met 2026-10-31 except** MCP provenance (no MCP write path exists yet). The
SIGTERM half landed with P1.5 the same day. The demo-vault rebuild time is now a
log line to read — the startup pass reports `indexed`/`removed`/`took_ms` at
info level when there was work to do — and the `kill -TERM` +
`PRAGMA integrity_check` check is still to be done by hand. (`kill -9` is out of
scope by design: nothing runs; what survives it is what the last autosave beat
already wrote.)

---

## Phase 2 — Command registry + agent-native surface

- **P2.1 Command registry** — **DONE 2026-11-01.** `crates/app/src/command.rs` is
  the one table (28 rows), and `workspace/commands.rs` is gone. Each row carries a
  stable dotted **id** (`note.save`, `theme.change`, `checker.on`), a **label**, the
  **keys** that reach it, the **ex** words that spell it, how its **args** are
  spelled, whether the palette lists it, and the gpui **action** that runs it. The
  palette builds its rows from the rows marked for it; the `:` line dispatches the
  *same* action for every command that takes no argument, so `:w`, `ctrl-s` and the
  palette row are one command by id. Only the argument forms (`:theme gruvbox`,
  `:focus section`, `:wordcount notes/`, `:focus on`) and the composite `:wq` are
  still acted on in `ex` — an action for a no-argument command cannot carry a value
  — and those still resolve to a registry id. Three tests keep the two vocabularies
  from drifting: every `ex` word in the table parses back to the id that lists it,
  every command `ex::parse` understands has an id in the table, and every `Ex`
  variant with no argument is in `no_arg_command`. **Not done here:** `keys` is
  written by hand rather than read out of the keymap, so a rebinding has to edit the
  row too; and `every()`/`ex()`/`args()` are `allow(dead_code)` until P2.2's docs
  generator reads them.
  **With it (asked for mid-flight):** `app.quit-force` — `:q!` quits and throws the
  note's unsaved edits away — and `:q` now **refuses** a dirty note when
  `editor.autosave` is off, pointing at `:w` or `:q!`. With autosave on it still
  writes on the way out, and an exit the user did not type (window manager, signal)
  still neither refuses nor discards. `:wq` became `write_quit`: write, then exit only
  if the write worked.
- **P2.2 Generated docs from the registry** (brainstorm ✅) — **DONE 2026-11-01 for
  the repo half.** `Command::docs()` renders `docs/commands.md` from the table: id,
  label, keys, every command-line spelling with its argument, and a generated list of
  the commands the palette does not offer. A test reads the file back and fails when it
  and the table disagree; `SHALE_UPDATE_DOCS=1 cargo test -p shale-app` rewrites it.
  **Why a file rather than only a function:** the standalone `shale-mcp` cannot link
  against the app binary, so the generated text is the artifact both sides can read.
  **Still open:** serving it as an MCP resource (P2.3) and backing `:help` with the
  same pipeline.
- **P2.3 MCP two-mode architecture** (brainstorm ✅) — **DONE 2026-11-01 for the
  read-only half.** In-app **HTTP on loopback** (`crates/app/src/live/`: a
  `127.0.0.1:0` listener, one request per connection, a token from the lockfile, and
  a `Snapshot` the window publishes on a 250 ms beat because the HTTP thread may not
  touch gpui state) + standalone `shale-mcp` **stdio** (`main.rs` the protocol, and
  `vault_tools.rs` / `live.rs` / `resources.rs`) + the lockfile at
  `.shale/mcp.lock` for detection and proxying. `shale_core::live` is the contract
  both ends speak, so the shapes cannot drift: the lock record, `x-shale-token`, the
  four paths (`/status`, `/note`, `/selection`, `/jumps`), `Status`, the client and
  `probe`. `[mcp] live` (on by default) decides whether a window binds a port at
  all.
  **Operational rules (binding, 2026-10-29 review) — all three now hold:** probe the
  HTTP endpoint before offering live tools, never the lockfile blindly (PID reuse is
  real — `a_lock_that_names_a_port_that_is_not_shale_is_not_believed` and
  `a_stale_lock_is_reported_and_ignored` are the rule as tests); fall back to the
  vault half with an explicit `live tools unavailable`, which is what the tools
  answer and what `shale://status` reports (`{app_running, live_tools, version,
  port, overlay}`); live tools queue behind modal overlays and never bypass the
  review path — the `overlay` field is reported now so the queue has something to
  read when it is built. Full rules in PROJECT.md (MCP).
  **Deferred on purpose:** the live tools are **read-only**. A tool that moves the
  user's screen is forbidden until the edit-review path exists, so `open_note`,
  setting a selection and walking the jump chain wait for **P2.5**; the DB tools the
  line above names wait for the same profiles that decide who may call them
  (**P2.4**); `tabs` in the live surface waits for **P3.1**. The lockfile doubling as
  the capture socket is **P2.6**.
- **P2.4 Scoped agent vaults** (brainstorm ✅) — **DONE 2026-11-01, except the
  review half.** `[mcp.profile."name"]` in the vault's config with `read`/`write`
  path patterns, `deny`, a `tools` allowlist and `review`; `shale-mcp --profile name`
  picks one and a typo'd name stops the server rather than widening it. The patterns
  are `crates/core/src/glob.rs` — a folder (`journal`, or `journal/`), `*` inside one
  path segment, `**` across segments, `?` for one character — written so that what a
  human types by hand means what it looks like, and so a stray empty pattern covers
  nothing rather than everything. `shale_core::config::McpProfile` is the enforcement
  (`can_read`/`can_write`/`allows_tool`/`visible`), and `shale-mcp` applies it to
  every call: `deny` beats `read` and `write`, `list_notes`/`search_notes`/`backlinks`
  filter the **entries before they are searched** (filtering afterwards would leak
  both hits and counts), `tools/list` is narrowed by the same allowlist as
  `tools/call` so a client is never shown a tool that will only refuse, and the live
  surface withholds a note name the profile may not read (`path: null` with `hidden`
  beside it — withheld is not the same as none). A new `shale://access` resource lets
  an agent read its own scope instead of discovering it one refusal at a time, and
  `shale://status`'s `live_tools` is filtered by the profile so the two agree with
  `tools/list`. **Enforcement is server-side only** (never trust the client), and
  every path still goes through the same `Vault` normalization/confinement as the
  app's file code — no fresh path logic in MCP handlers; a profile narrows what is
  inside the vault, it never moves the wall around it.
  **Deferred:** `review = true` is parsed and **refuses writes** until P2.5 can keep
  the promise (fail closed, never write unreviewed because a flag said not to); the
  per-connection selection the brainstorm mentions ("or per connection") is the
  app-side half that needs to know which connection is which, which is P2.8's
  presence work; and the app does not yet show which profile wrote what.
- **P2.5 Agent edit review** (brainstorm ✅; **design pass RULED 2026-11-01**,
  full shape in brainstorm.md "Agent edit review"). Split into:
  - **P2.5a — queue + strip (the safety story): DONE 2026-11-01.**
    Built as ruled, with three
    notes on what was *not* built: the **DB does not index the queue** (files are
    the source of truth; a table can mirror it later and nothing depends on it),
    the **inline overlay is P2.5b** (untouched), and `]a`/`[a` are **engine-owned
    strokes, not keymap chords** — binding `"[ a"` in `vim-normal` made gpui hold
    the `[` as pending chord input so it never reached the editor as text, which
    broke `f[`; the vim engine now owns the pair the way it owns `space`/`z`/`s`.
    Discovery is a 1 s poll of `.shale/pending-edits/`, not the watcher, because
    the watcher deliberately skips dot directories.
    MCP writes from a
    `review = true` profile stop being refused (P2.4's fail-closed flip) and
    land as **TOML files** in `.shale/pending-edits/` — one per write call,
    `[[hunk]]` blocks embedding unified-diff text — written by the **MCP
    server itself** (atomic temp+rename; queues while the app is down,
    survives `kill -TERM`); the app discovers via watcher + DB index (DB
    only indexes; files are the source of truth). **Bottom review strip**
    above the status bar, per-hunk accept/reject, two-layer keys (bare
    `j/k y/x o za` strip-focused; `]a`/`[a` + review keymap context in the
    buffer), all registry commands. Shared **line-level diff primitive**
    built here (engine: pure-Rust `similar`), reused by P4.2. Conflict
    policy **apply-or-stale** (no 3-way merge in v1); a read-only
    `pending_edits` MCP tool lets an agent see its own pending/stale hunks
    and re-propose (agent may retract its own; accept is user-only). Quit
    prompts; unanswered/killed → files persist and re-offer next launch.
    No in-place editing of proposed hunks.
  - **P2.5b — inline overlay (open):** phantom-region hunks in the note (dimmed
    removals, tinted additions, collapsible, `za`), re-anchored by diff
    context, strip-only fallback when ambiguous — strip authoritative,
    inline opportunistic. Full shape in brainstorm.md (agent presence
    cluster).
    - **Its colours already exist.** `Palette::diff_add` / `diff_delete` /
      `diff_changed` landed with the design-language sweep (2026-11-03,
      `design-language.md` §1, all 20 built-ins valued), so this task paints
      against named roles instead of inventing a green and a red at the call
      site. Same for `agent`, the one hue for the whole presence cluster.
- **Route 3 ruling (2026-11-01):** P2.5a is the last Phase 2 item to build;
  P2.6/P2.7/P2.8 below are **deferred to the Phase 3 addendum** — none of
  them gates anything, and P3.1 Tabs unblocks the graph track, which gates
  a lot.
- **P2.6 Quick capture** (brainstorm ✅; → Phase 3 addendum): `shale capture "text"` → unix socket
  (Linux/macOS) / named pipe (Windows) if the app is up, direct append if not.
  No app-grabbed global hotkeys on Linux; document the niri binding.
- **P2.7 `:Gsub` / vault-wide ex commands** (brainstorm ✅; → Phase 3 addendum): scoped by
  search/DB, preview affected notes before applying. Registry + DB make this
  compose.
- **P2.8 Agent presence & provenance visibility** (brainstorm ✅, 2026-10-29; → Phase 3 addendum):
  provenance badges (gutter/tab/tree, hover → event row), file-tree ticks
  (profile-colored, presence-now, distinct from the heatmap), presence by
  inference from the event log, editor-side glow (`[editor] glow`,
  timestamp-derived, never a ticking animation). All rides P1.2's event
  spans + P2.3's connection info; all config-off-able as a group; zero
  animation, zero frames when idle.

**Exit criteria (restriped 2026-11-01, route 3):** palette and `:` dispatch the
same command ids; docs regenerate in CI/test; an external agent can drive the
app over HTTP and its edits queue as pending files that survive a `kill -TERM`
— **P2.5a closes Phase 2**. Capture working app-up-and-down moved with P2.6
into the Phase 3 addendum. **P2.5a landed 2026-11-01, so Phase 2 is closed**
(P2.5b and the P2.6/7/8 addendum remain open).

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
  - **Landed 2026-11-02.** The tab bar (always visible, `×` per tab, a dirty dot on
    the tab it belongs to), `ctrl-t` / `:tabnew`, `g t` / `g T` (resolved inside the
    vim engine, not as a two-stroke keymap chord — a chord would swallow the `g`, and
    `t`/`T` were already bound), `:tabnext|:tabprev|:tabclose|:tabonly|:tabmove [n]`,
    clicking a row, clicking the `×`, and the session remembering the bar with each
    tab's view. **The editor belongs to the tab** (`workspace/tabs.rs`), which is what
    makes per-tab cursor/scroll/folds/view/undo real rather than a second table of
    per-note state; `Workspace::editor()` is total because the bar never goes empty.
    Exits owe every tab: `:wq`, a quit with autosave on, a blur and a window close
    write them all, and `:q` is refused by a dirty background tab when autosave is
    off. **Deferred, on purpose:** a tab bar that scrolls when the tabs outgrow the
    width (they share it and ellipsize instead), `3gt` counting to a tab number,
    per-tab jump chains and per-tab checker answers, and drag-to-reorder (`:tabmove
    [n]` is the door for now). P1.8's tab half is now landed; its **pane layout** half
    is still open and belongs to P3.2.
- **P3.2 Tiling splits** (goal) — same pane model as tabs; `:pin` (note
  pinning, brainstorm ✅) is a pane flag: dimmed, excluded from the jump chain,
  auto-closing. **Landed 2026-11-03 (P3.2a + P3.2b):** `Pane` owns the tab row
  (the pure refactor, every tab test green through it), and the window's editing
  area is a tiling tree — `Node::Leaf | Split{dir,kids,frac}` beside the flat
  `Vec<Pane>`, fractions relative. `ctrl-w s/v` split, `S/V` split with the
  buffer in hand (a second view, loaded past the find-don't-duplicate rule on
  purpose), `h/j/k/l` step geometrically with the sidebar as the leftmost
  pane's left neighbour, `q/c` close (notes written first; a failed write keeps
  the pane; the last pane refuses), `o` keeps only it, `=` equalizes. Each pane
  draws its own tab bar and the clicks name their pane; the graph stays one tab
  per **window** (a second `space g` focuses the pane showing it); a note open
  in another pane is found, not duplicated; exits, renames and theme changes
  sweep every pane. `:split/:vsplit/:close/:only` on the command line, palette
  rows for the two split shapes.
  **Landed 2026-11-05 (P3.2c):** the session file keeps the tree and every pane's
  tabs — `panes` (a `PaneSeat` per leaf, in tree order), `layout` (`leaf` /
  `split{dir,frac,kids}`) and `active_pane` — so a split window comes back split,
  with each note in the pane that held it, that pane's own active tab, and the
  divider where the user left it. A file from before this reads as one pane
  (`Session::pane_seats` is the single reader), and a tree that does not describe
  the panes it ships with is a log line and one pane holding all the tabs, not a
  panic. `ctrl-w >`/`<` (also `+`/`-`) give the pane in hand more or less of the
  split it sits **directly** in — relative fractions, 0.25 of an equal share per
  press, floored at 0.2 so a pane cannot be squeezed out of existence.
  The exits follow the pane: `:q`/`:q!`/`:wq` close the pane the command was typed in
  when there is more than one, and the window goes only with the last pane (vim's
  `:quit` shape). Closing the last tab in a pane closes that pane — Obsidian's rule, so a
  split does not fill up with panes nobody is looking at — with the last pane as the
  exception: there the tab is cleared, so a window never quits because a tab closed.
  And opening a note another pane already shows **steals the tab** into the pane with the
  keyboard — whole, because the tab owns its editor, so the cursor, folds, view and undo
  history travel (ruled 2026-11-05, replacing "found, and the keyboard goes there" with
  "found, and brought here"). The pane it left keeps a blank tab rather than closing; the
  graph tab is the exception, still one per window and focused where it is, because a
  restore must not relocate it. Tab-to-pane movement is therefore covered; the open item
  below is about moving **panes**.
  **A blank tab is a front door, not a void (2026-11-05):** a pane whose tab holds no note
  paints a start block on the note's own surface — the vault's counts, its recent notes as
  clickable rows, the doors as key-chips — and it steps aside the moment the tab starts
  being a note (insert mode, or text in the buffer), so the writing surface is never behind
  a menu. Recents are vault-wide: `recent` at the top level of the session file, newest
  first, capped at 10, notes no longer on disk dropped from the list. A row names its own
  pane, so a click in a pane without the keyboard opens the note there. An empty vault gets
  the same screen with one different line — `the vault holds no notes yet` and a `new note`
  row — which closes the design doc's parked first-run item and amends its empty-state
  ruling (§13).
  **A blank tab is a note being written (2026-11-05):** its first line names it when it
  is saved — by `:w`, by autosave, or by an exit — with the name cleaned by
  `shale_core::note_name` (markdown markers and filesystem-forbidden characters fall out,
  60 characters, never hidden, never above the vault) and a number added when the name is
  taken. `:w <name>` names it by hand and may place it in a folder; a typed name that
  collides is refused rather than suffixed, and `:w <name>` on a note that already has a
  file refuses out loud — the command line does not clone or move a note by accident;
  moving is `:rename`, which rewrites links (ruled 2026-11-05).
  **Still open for P3.2:** move/exchange/rotate (`H/J/K/L`, `x/X`, `r`) and `:pin`.
- **P3.3 Graph M1a — start immediately, in parallel with P0/P1.** No
  dependencies: `crates/core/src/graph/{index,sim,quadtree,filter}.rs` +
  petgraph. See `graph-view-plan.md` Tasks 1–4.
- **P3.4 Graph M1b** — the graph tab, canvas painting, interactions, filter
  panel (`graph-view-plan.md` Tasks 5–8). **Task 5 landed 2026-11-02**: the tab
  host now holds a `TabContent` (note or graph), `space g` / `:graph` / the palette
  row open one graph tab per window, and the index is kept current from the
  watcher. **Task 6 landed the same day**: the tab is a canvas — the window owns one
  force simulation whose layout survives an arena rebuild (positions carried by node
  identity), the camera frames the whole graph until the user takes it over, one
  self-stopping beat drives the layout, and nodes, links and labels are painted with
  the raw paint API. **Task 7 landed the same day**: the tab takes the pointer (hover,
  background pan, node drag with pin-on-shift, click to open) and the keyboard (`+`/`-`,
  arrows and shift-arrows, `hjkl` to pick a node, `enter` to open it, `f` to frame it
  again), with the hit test on a grid the frame carries. **Task 8 landed the same day,
  and with it M1b is done**: `/` opens a filter strip in the tab — three chips (`orphans`,
  `missing`, `tags`) and a search line that filters as it is typed — and the mask the
  filter produces gates the **simulation** as well as the paint, so a hidden note takes
  part in neither and the layout closes up. The counts row describes the screen rather
  than the vault, the filter is session state beside the camera rather than config, and
  the strip has a `graph-filter` key context of its own so the graph's `hjkl`, `f`, `+`
  and `-` stay out of a query.
  **Polish pass landed the same day** (the tab's first run on a real window): the wheel
  direction (down = zoom in), a floor under the label fade so small labels stop reading as
  black, a bigger header, the filter strip keeping the keyboard across a tab change, and a
  hit test that stops missing the nodes on the layout's bounding-box edge — core's `Grid`
  answers "nothing" for a query outside the box it was built from, which is where the
  screen→world round trip puts them (see PROJECT.md thread 38 for the request to the graph
  track). The jump chain now treats the graph as a place, so `ctrl-o` walks back to it from
  a note a node click opened. A temporary fps / paint-ms readout sits in the counts row
  (`graph/bench.rs`) for the exit-criteria check on `examples/graph/`, and comes out once
  that is verified.
  **Exit-criteria blocker found on the big fixtures (2026-11-03), closed core-side the same
  day:** the layout diverged at 1k and 5k notes (the bounding box grew ~10× per tick;
  numbers and the resolution in PROJECT.md thread 39) — uniform-strength springs, not the
  repulsion constant, fixed in `91930f2`. The app no longer dies trying either: the hit-test
  grid is bounded by the layout.
  **Paint phase measured and optimized the same day (PROJECT.md threads 42–43):** the paint
  phase now has a bench (`tests/graph/paint_bench.rs`, release-only, `#[ignore]`d) and on the
  5k fixture a whole graph frame costs **2.3–2.6 ms at whole-vault zoom, 0.9 ms at 3×, 0.7 ms
  at 10×** — down from 14.8–19 ms, from `Window::paint_layer` (gpui searches the scene's
  bounds tree for every primitive's draw order, and that search is not linear), hand-
  tessellated link quads written straight into gpui's vertex format, and a cache that replays
  a settled frame's links instead of rebuilding them. Three bugs came out of the same pass:
  the graph painted outside its pane, its labels were `black()` because a `TextRun` with
  `len: 0` is stepped over, and above ~16k visible links gpui drew **no links at all** (a
  path's vertices are u16-indexed). A fourth, from the batching itself: one layer over the
  canvas made gpui order by primitive *kind*, so every link landed on top of every node —
  hence one layer per phase. The fps / idle-sleep check is the remaining exit-criteria item;
  the `graph/bench.rs` readout stays, at the user's request.
  **The idle-sleep half closed 2026-11-05**: a probe in `Workspace::render` counted **0 renders
  over 15 s** with the 1k graph on screen, and the two idle pollers went — the live channel's
  250 ms publish beat (now driven by drawing) and the accept loop's 50 ms poll (now a blocking
  accept woken by a connect) — taking the window from 11.3 to 7.2 wakeups/s and ~520 to ~423
  syscalls/s (`29f2d5b`, `82f3cc9`; numbers in `PROGRESS.md`). The fps half stays open.
- **P3.5 Graph M2–M5** in the plan's order: local graph + projected 3D →
  constellations + static mode → heatmap/groups/arrows → signature features
  (thread, pulses+glow, MST star-lines, nebulae, hjkl + f-jump, illumination,
  drift, orbit focus, link-age, minimap).
  **Re-striped 2026-11-04:** M2 splits into **M2a projected 3D** (its own item
  below, P3.6, pulled ahead of everything else in this phase) and **M2b local
  graph**, which follows it.
  **M2b landed 2026-11-04**: `GraphFilter::local` is a scope dimension in core's
  `apply`, centred on the note the window is looking at, depth 1–5 (default 2),
  reached by the strip's `local` chip, `[` / `]` / `\` in the graph context, and
  `:graph-local [n]` / `:graph-global`. It composes with search, kinds and folder
  scope, and the depth rides the session seat.
  **Brainstorm rulings that feed M5 (2026-10-29):** typed edges as an edge
  layer + kind filter (P1.2 `edges` table; display registry with inverse
  labels; no layout springs in v1), hook→payoff dashed arcs, region
  clustering via `region:` props, minimap constellation mode (mirrors the
  graph tab's active filter only while that tab has focus).
  Note: **glow, link-age, and the heatmap all need P1** — schedule them after
  Phase 1 lands even though the graph itself doesn't.
- **P3.6 Projected 3D as a mode** (ruled ✅ 2026-11-04; the plan's M2a).
  **The core half is already built** — `bd04fd8` landed the dimension-generic
  sim (`Sim<2>`/`Sim<3>`, quadtree → octree, seeded in both dimensions) and
  `crates/core/src/graph/proj.rs`: `Cam3` with `orbit_by`, `zoom_at`,
  `project_all`, `painter_order`, `fit` and `screen_to_world_at_depth`, plus
  the hit-test recipe. The work is app-side: `GraphSim` runs `Sim<3>` in 3D
  mode, the frame carries `[f32;3]`, and paint, hit test and camera gain a
  projected branch. **Binding rules are in PROJECT.md "2D / 3D mode rules"**
  (mode not replacement, 2D default; no re-seed on a mode switch; the session
  remembers orientation and re-fits position; a camera showing nothing falls
  back to fit; links stay under depth-sorted nodes in v1; the hit-test grid
  uses projected bounds). The hard parts, stated honestly: the settled-frame
  link cache assumes a static camera, so orbiting pays a rebuild (key the cache
  on camera change); true depth interleaving of links and nodes would undo the
  one-layer-per-phase batching win (thread 43), so v1 approximates.
  **Gate:** the 2D numbers in thread 43 must not move, and the 3D frame is
  benched (`crates/app/src/tests/graph/paint_bench.rs`) and idle-park-checked
  before the mode is called done.
  **Landed 2026-11-04.** `2`/`3` in the graph context and `:graph2d`/`:graph3d`
  turn the tab; the mode is the *window's* (one sim per window), the session
  carries it beside the angle, and the projected path is `crates/app/src/graph/proj.rs`
  — one `Projection` per frame, shared by the paint and the pointer. One binding
  rule changed on the way: the "camera showing nothing falls back to fit" guard
  was ruled out (see PROJECT.md "2D / 3D mode rules") because it fights a
  deliberate pan, and the orientation-only memory makes it unnecessary in 3D.

**Sequencing ruling (2026-11-04):** P3.6 goes **before** P3.2's leftovers and
before the addendum. The rule it breaks, named plainly: *each phase's exit
criteria pass before the next phase starts* — Phase 3's splits half is still
open. What it costs: nothing structural. None of the deferred items gates
anything downstream, and Canvas C1 is specced as graph-renderer reuse that
*should follow* the graph milestones, so graph-first feeds it rather than
delaying it. P3.2's leftovers remain part of this phase's exit criteria and are
the next thing after P3.6.

**Exit criteria:** tabs and splits fully keyboard-driven; graph tab opens on a
2k-note synthetic vault at 60 fps with the sim settling then sleeping (verify
`FrameLoop::Parked` in logs + `pidstat` ~0% idle with drift off).
**Graph half verified 2026-11-03** (5k fixture, `--graph` debug flag): 0 fps /
1.75 ms paint settled, 38 fps mid-settle (beat cadence, not paint capacity —
1.4 ms of the 16.6 ms budget), sim beat self-stops (headless test), idle CPU
3.1% → 0.97% after the watcher duty-cycle fix; the graph's own idle frames are
zero. `FrameLoop::Parked` is not logged in gpui v1.25.3 — the plan's idle
contract section carries the corrected recipe and the full breakdown. The
"60 fps" wording measures the beat's cadence ceiling (~40 Hz through the
timer+compositor round trip); raising it means driving the beat from the frame
callback, a follow-up thread if wanted. **Tabs half met** (P3.1); **splits
half awaits P3.2.**

### Phase 3 addendum — the deferred Phase 2 remainder (route 3, 2026-11-01)

**Re-sequenced 2026-11-04:** the whole addendum now follows **P3.6** (projected
3D as a mode). Nothing in it gates that work, and Canvas C1 benefits from the
graph renderer settling first.

P2.6 (quick capture), P2.7 (`:Gsub`), and P2.8 (agent presence) keep their
full specs above under Phase 2 and land inside Phase 3 once tabs + the graph
tab are through. Rationale: none of the three gates any later item — P2.6 is
a standalone socket, P2.7 composes off the shipped registry + DB, and P2.8
rides P1.2's event spans (its tab-badge half *needs* P3.1 anyway). Suggested
slots: P2.8 right after P3.1 (badges want tabs), P2.6/P2.7 as quick wins
around the graph milestones. **Canvas C1** (ruled ✅ 2026-11-03, full plan in
`canvas-plan.md`) joins this addendum: viewer + jsoncanvas read/write +
note/image/group nodes + `.canvas` link indexing and rename propagation —
mostly graph-renderer reuse, but it needs P3.1 tabs and should follow the
graph milestones so the reuse is real. C2 (editing) is P5.6; C3 (typed
edges, query nodes, MCP canvas tools) is P6.10.

---

## Phase 4 — DB surfaces

- **P4.1 Time machine** (brainstorm ✅): diff-based snapshots in `.shale`,
  session-based cadence (session end + ~3-min checkpoints + pre-risky-op +
  explicit saves; trivial diffs skipped; `:snapshot [label]` manual command —
  see brainstorm "Autosave × time machine"), retention 24h full → 30d hourly
  → daily, **100 MB default cap, configurable**. Per-note scrubber + diff view.
- **P4.2 Diff renderer** — shared by the time machine and P2.5's edit review.
  The line-level diff primitive (compute + paint, `similar`-backed) ships in
  **P2.5a**; P4.2 reuses and upgrades it rather than starting fresh.
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
- **P5.6 Canvas C2 — canvas editing** (canvas-plan.md ✅ 2026-11-03): edge
  authoring (ports, arrows, labels), inline editor overlay attached to the
  real buffer, snapping + grid, marquee select, align/distribute,
  `:canvas tidy` (+ rectangular collide in the sim), outline view,
  drag-from-file-tree. Where the canvas time goes.

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
- **P6.9 A rounding pass over the custom-painted surfaces** (polish; the chrome
  half landed 2026-11-02 as `crates/app/src/style.rs::CHIP_RADIUS`, one radius the
  whole window reads). **Mostly done 2026-11-03** as step 4 of the design-language
  sweep (`design-language.md` §11.4, `413dda6`): the checker's panel was the miss —
  it used the raised triple's colours and none of its shape — and its rows and the
  review strip's are chips now. What is left is the note's block backgrounds, which
  need the layout pass to say which row of a fenced block is first and last; the
  decision for every surface, rounded and deliberately square, is written in
  `style.rs`. Everything gpui draws from a `Style` already takes it: a tab,
  a file-tree row, a picker row, the picker's panel, and the text field's box (top
  corners only — the underline *is* the field's edge, and rounding the bottom would
  leave it floating). What is left is painted by Shale's own renderer, so it needs a
  `Corners` handed to `paint_quad` rather than a style method: the line element's row
  backgrounds (active line, search hit, fold marker, focus-mode dimming), the inline
  code and code-block backgrounds, the table grid's cell highlight, the checker's
  wavy underline, the popup panel, the review strip and its badges, the status bar's
  badges, and the caret. **Not one radius for all of them** — a 2px underline does
  not want 6px — but one module deciding each.
- **P6.10 Canvas C3 — the differentiators** (canvas-plan.md ✅ 2026-11-03):
  vault-backed typed edges (two-edge-kind rule), query nodes + smart-groups,
  block-level cards, MCP canvas tools (semantic verbs, quadtree occupancy,
  auto-relax moving only agent nodes) through the P2.5 review strip,
  `:canvas from-query`, canvas-as-saved-selection graph filter, PNG/PDF
  export, GM board template gallery. Depends on P4.3 `.shq`, typed edges,
  and block anchors.

---

## Sequencing notes

- **Route 3 (ruled 2026-11-01):** build **P2.5a** next — the design pass is
  done, the lift is small (MCP-crate writes + watcher + one bottom widget +
  the diff primitive), and it is the agent-trust story that unblocks
  `review = true` profiles. Then jump to **P3.1 Tabs → P3.4 graph tab** to
  unblock the graph track's app/ work; a lot of features ride the graph.
  P2.6/P2.7/P2.8 wait in the Phase 3 addendum.
- **Start P3.3 (graph core) now**, in parallel with P0 — it has zero
  dependencies and is pure `core` + tests, so it can't conflict with UI work.
  **Consolidated 2026-11-03:** the second agent (the graph track) is gone; one
  primary developer now owns the whole repo — `crates/core/src/graph/**`, the
  app, and the planning files included. The file-ownership rules that guarded
  the two-agent period (neither editing the other's files, a failing `graph/`
  build being foreign work in progress) no longer apply; `PROGRESS.md` remains
  the graph track's progress log and `PROJECT.md` the thread record.
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
2. ~~Edit-review UX pass~~ **RESOLVED 2026-11-01:** full design pass ruled —
   P2.5a/P2.5b split, bottom strip, two-layer keys, diff primitive pulled
   forward (see brainstorm "Agent edit review" and the P2.5 item).
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
