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
  badges the view whenever it is not live. **Left:** the view is not in the config
  yet (config thread), and preview still shows a cursor rather than none.
- **P0.7 Markdown text objects** (brainstorm ✅): `diL` wiki-link, `cit` table
  cell, `da#` section, `ci*` callout, `da``` ` code block. `mdstyle` already
  reports these spans — mostly wiring into the vim engine.
- **P0.8 Focus / typewriter mode** (brainstorm ✅): per-block alpha ramp in the
  renderer. Establishes the dimming machinery that search-illumination and
  note-pinning later reuse.

**Exit criteria:** config lives in `.shale/` and a root `shale.toml` is *moved*
there on open rather than read from, autosave on by default, no file over 1000
lines, the popup and the picker both scroll, `/` and the three views work,
`ctrl-o`/`ctrl-i` walk the jump chain, all existing tests green.

---

## Phase 1 — The DB spine (the big unlock)

- **P1.1 SQLite in `crates/core`** — `rusqlite` (user ruled it in: bundled,
  build-time, small static dep; the "no external bindings" rule stands for
  everything else). DB at `<vault>/.shale/shale.db`, **WAL mode** (needed for
  app + standalone MCP concurrent access).
- **P1.2 Schema:**
  - `notes(path PK, created_at, modified_at, word_count, created_by,
    last_modified_by, open_count, edit_count)`
  - `tags(note, tag)` (flat strings; the tree is built by splitting on `/`)
  - `links(source, target, first_seen_at, last_seen_at)` — the `first_seen`
    that powers link-age coloring and the scrubber
  - `events(note, kind, at, provenance, detail)` — **append-only**; kinds:
    created/edited/opened/linked/renamed/deleted. This is the scrubber's data.
  - `marks(name, path, position, label, created_at)`
- **P1.3 Incremental indexing + startup reconciliation** — exactly as
  PROJECT.md's Database section promises: background thread reconciles files
  changed by external editors at launch, then ends; further updates are
  event-driven off the existing `watch` + save paths.
- **P1.4 Provenance stamped on every write path** — user save, MCP write,
  rename, template, capture. One helper; every writer calls it.
- **P1.5 Shutdown integrity rule** (brainstorm ✅, binding): SIGTERM = graceful
  shutdown (flush DB, write pending edits, finish in-flight saves); all writes
  atomic (temp + rename); SIGKILL out of scope. **Land this with the DB** —
  it's the first stateful thing that can be corrupted.
- **P1.6 DB backup** — daily copy to `.shale/backups/`, keep 7. DB is cache;
  notes are the source of truth.
- **P1.7 `:wordcount [folder]`** + status-bar count — first DB-backed feature;
  proves the spine end to end.

**Exit criteria:** DB rebuilds from an empty `.shale` in <1s on the demo vault;
external edit picked up within the watch interval; provenance correct for
user/MCP/rename writes; `kill -TERM` leaves a clean DB (verify with
`PRAGMA integrity_check`); `:wordcount` correct for vault and a subtree.

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
- **P2.4 Scoped agent vaults** (brainstorm ✅): `[mcp.profile."name"]` with
  read/write path globs, `deny`, and a tool allowlist; `shale-mcp --profile`.
  Review mode is an attribute of the profile.
- **P2.5 Agent edit review** (brainstorm ✅): pending edits as **plain-text
  files** in `.shale/pending-edits/` (DB only indexes them); review strip with
  per-hunk accept/reject; quit prompts, unanswered/killed → files persist and
  re-offer next launch. Needs a UX design pass first (strip placement, keys,
  diff rendering — reuse P4's diff renderer).
- **P2.6 Quick capture** (brainstorm ✅): `shale capture "text"` → unix socket
  (Linux/macOS) / named pipe (Windows) if the app is up, direct append if not.
  No app-grabbed global hotkeys on Linux; document the niri binding.
- **P2.7 `:Gsub` / vault-wide ex commands** (brainstorm ✅): scoped by
  search/DB, preview affected notes before applying. Registry + DB make this
  compose.

**Exit criteria:** palette and `:` dispatch the same command ids; docs
regenerate in CI/test; an external agent can drive the app over HTTP and its
edits queue as pending files that survive a `kill -TERM`; capture works with the
app both up and down.

---

## Phase 3 — Tabs, then the graph

- **P3.1 Tabs** (goal; gates everything here). Tab bar, `gt`/`gT` or
  `ctrl-w h/l` extension of the existing focus chords, close/dirty markers.
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
- **P5.4 Parked extras:** compare mode (two dates), ghost view, session view.

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
- **P6.5 PDF export** — goal. Needs a print path; decide: headless render to
  image pages → PDF, or a markdown→HTML→external converter (the latter breaks
  the no-external-deps preference; prefer the former).
- **P6.6 Tree-sitter code highlighting** — goal. **Ruled ✅ (2026-10-28): C
  bindings are fine here** — the user wants the developer-friendly feature; the
  SQLite-style exception now covers tree-sitter (bundled C, build-time).
  Use `tree-sitter` + `tree-sitter-markdown` (or per-language code-block
  grammars) for fenced-block highlighting.
- **P6.7 Horizontal scrolling + wide-glyph columns** (parked #7), `K` with
  count (#6), live grammar fixes (#5), hover capability check (#4).
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

## Open decisions the roadmap needs

1. ~~Tree-sitter = C bindings~~ **RESOLVED 2026-10-28:** the SQLite-style
   exception now covers it (PROJECT.md Tech Stack, and P6.6). Bundled C at build
   time is fine; runtime-linked system libraries still are not.
2. **Edit-review UX pass** before P2.5 — strip placement, keys, diff rendering.
3. ~~`/` search scope~~ **RESOLVED 2026-10-28:** `/` searches the **open buffer**;
   the vault stays `ctrl-shift-f` (see P0.5).
4. **PDF export route** — headless scene render vs external converter (P6.5).
5. ~~Graduating `brainstorm.md` ✅ items into PROJECT.md~~ **Half done
   2026-10-28:** the Goals list now carries checkmarks for what is built. The ✅
   feature designs themselves still live only in `brainstorm.md`; graduate them
   into PROJECT.md's Open threads as each one starts, not before.
6. **Config write-back shape** — settled in P0.1 (one location, a splice, a
   generated starter). What is *not* settled: whether Shale should ever write a
   key the user has never changed. Today it writes only what the user chose.
