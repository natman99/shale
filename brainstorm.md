# Shale — Feature Brainstorm (living document)

Status legend: ✅ agreed / 🔭 solid maybe, needs more thought / ❄️ parked, revisit later.
This file is refined as ideas are discussed; move decisions into PROJECT.md when they
graduate from brainstorm. (Graph-specific design lives in `graph-view-plan.md`.)

Last refined: 2026-10-29 (TTRPG/D&D toolkit section added).

---

## Agent-native editing (the positioning bet)

### MCP server architecture ✅
- **App running:** in-app MCP server speaks HTTP on localhost (an already-running
  process can't be stdio). Discovery via lockfile/port file in `.shale`. Only this
  mode exposes live state: current note, selection, jump chain, open tabs, and
  editor-driving commands (open a note, run a search, focus the graph).
- **App closed:** standalone `shale-mcp` binary (crates/mcp) speaks stdio (the default
  transport agents use for spawned servers) and serves vault read/write/search + the
  `.shale` DB. Live-state tools are simply omitted.
- **Both running:** standalone checks the lockfile at startup; proxies to the app's
  HTTP port or refuses with "connect to the app." One rule, no split-brain.
- **Consistency:** SQLite WAL mode handles app+standalone concurrent access; the
  app's startup reconciliation thread picks up what the standalone changed.
  Provenance still works — the standalone stamps `mcp` in the DB itself.

### Edit provenance ✅ (also a prerequisite for freshness glow & time machine)
- DB records every edit with source: `user | mcp | rename | template` + timestamp.
- Enables: "last touched by" badges, agent-touch heatmap, "everything the agent
  touched this week" query, link-health history.

### MCP pulses + cached-edit catch-up ✅ (refined from "solid maybe")
- Live: each MCP edit spawns a ripple on the graph (render-only, never touches sim).
- Edits while the graph tab is closed are recorded in the DB. On graph open, do NOT
  replay ripples (a 50-edit burst would explode). Instead: **freshness glow** —
  nodes with recent edits get a soft age-decaying halo (state derived from DB edit
  timestamps, not event replay). Two mechanisms, one data source:
  - ripples = live events (tab open),
  - glow = derived state (survives tab close/reopen; also feeds heatmap and
    link-age coloring later — same age→color machinery).
- Catch-up window: edits newer than ~10 min glow; halo intensity = f(edit age).

### Agent edit review ✅
- MCP edits land as pending diffs (review strip, per-hunk accept/reject,
  keyboard-driven). Pairs with ripples: ripples show *where*, review shows *what*.
- **Pending edits are stored in plain text** (e.g. `.shale/pending-edits/*.json`
  or `.toml` — human-readable, git-diffable), NOT only in the DB cache; the DB
  may index them but the files are the source of truth, so a corrupt DB never
  loses a pending edit.
- **On quit:** prompt if edits are pending. If the user closes without answering
  (or the app is killed), the pending file stays on disk and is re-offered next
  launch. Nothing is silently applied or discarded.
- Design pass owed: where the strip lives, keybindings, diff rendering (reuse the
  time-machine diff renderer once it exists).

### Shutdown integrity rule ✅ (binding, app-wide)
- In-process file IO must finish before the app exits: handle SIGTERM (compositor
  hotkey closes windows this way on Wayland) as a graceful shutdown — flush the
  DB, write pending edits, finish in-flight note saves. Same for window-close
  events. SIGKILL is out of scope (instant death, accept the loss).
- Practical shape: a shutdown flag + completion barrier the main loop drains
  before exiting; all writes are atomic (write-temp + rename) so even a hard kill
  can't leave half-written files.

### Scoped agent vaults ✅
- Per-connection permission profiles for the MCP server:
  `[mcp.profile."writing-help"] read = "*" write = ["inbox/", "drafts/"]
  deny = ["private/"] tools = ["read","search","append"]` — path globs + tool
  allowlist, selected via `shale-mcp --profile name` or per connection.
- Makes agent access *comfortable*: free rein over drafts without touching the
  journal or being able to delete.
- **Review mode becomes an attribute of the profile** (trusted profile writes
  directly; untrusted queues for edit review) — ties the two features together.
- Cost: path-glob checks + tool allowlist in the mcp crate; tools are already
  enumerated.

---

## Vim-native Markdown

### Markdown text objects ✅
- `diL` inside wiki-link, `cit` table cell, `da#` section under heading, `ci*`
  callout body, `da``` ` code block contents, `diq` smart quotes.
- The markdown parser already knows these spans; mostly wiring in `core`.

### Marks + bookmarks panel ✅
- All marks global in Shale (like vim's `A-Z0-9`): `m{char}` sets note+position,
  `'{char}` jumps from any note, opening the file if needed.
- Stored in DB: `(name, path, position, label, created)`.
- Panel: marks + named bookmarks (`:bookmark add weekly-review`) + the **jump
  chain** ("everywhere I've been and want to return to"). Fuzzy filter, Enter
  jumps, `d` deletes.
- MCP can list bookmarks/marks → "what is the user working on?" includes their
  own bookmarks.

### Vault-wide ex commands ✅
- `:Gsub /draft/final/g tag:work` — substitution scoped by DB/search index, with a
  preview listing affected notes before applying. `:Gsort`, `:Gdelete` w/ confirm.
- Composes command palette + search + DB.

### Macros → plugins ladder (refining)
- **Tier 1 ✅ vim registers:** `q{char}` record, `@{char}` replay. Session-scoped,
  pure vim muscle memory.
- **Tier 2 🔭 named TOML macros (explore at implementation time):** stored in
  `macros.toml` (app-writable, separate from hand-edited config; also per-vault
  `.shale/macros.toml`). Two forms:
  `keys = "..."` (replayed through normal key dispatch — respects modes/counts) or
  `commands = [...]` (invokes palette commands — robust to keymap changes).
  Record-from-palette flow: record → stop → name → written to TOML. Shows up in
  the command palette, fuzzy-findable, bindable. Recursion limit; macros can't
  write config.
  - **MCP-writable ✅:** the MCP server gets `macro.create / macro.list /
    macro.delete` tools — the agent can author macros for the user.
  - **Docs required ✅:** a command reference generated from the command registry
    (every command, args, keymap context) — shipped as `docs/` in the repo AND
    served as MCP resources so the agent can read it at runtime. Same doc
    pipeline serves the user (eventual `:help`).
- **Tier 3 🔭 Lua:** mlua (MIT/Apache-2.0), sandboxed (no `os`/`io`/stdlib by
  default; only `shale.*` APIs: buffer, selection, vault query, DB query, spawn).
  Scripts in `.shale/lua/` register commands + event hooks (`on_save`,
  `on_mcp_edit`, `on_note_open`). Hot-reloadable. Agents can author Lua — that's
  a real advantage. Build only when TOML macros demonstrably hit a wall; design
  macros so they promote cleanly (same command registry).
- **Keystone architecture:** one **command registry** — palette commands, TOML
  macros, MCP tools, and (later) Lua all speak to it. This is what makes the
  ladder work.
- **"MCP is the plugin system":** external processes extend Shale via MCP tools
  (drive editor, add query sources, react to events) — out-of-process by
  construction, so no ABI/sandbox problem. Plugins stay a non-goal until this
  trio demonstrably fails; the in-process candidate is Lua, never WASM or
  dynamic libs.

---

## The DB as a surface

### Query notes: `.shq` ✅
- Special folder `queries/` at vault root; plain text, git-able.
- `.shq` files: TOML frontmatter (name, sort, view options) + SQL body. Editable
  in the normal editor with syntax highlighting; the app renders them as
  editable-SQL + live results table.
- Embed anywhere: `![[query-name]]` renders the live table (transclusion resolver
  branch, same machinery as note embeds).
- Read-only enforcement at the connection level (not just sanitizing). Results
  cached by query-hash + vault mtime; refresh on vault-change + `:refresh`.
- MCP can create/edit `.shq` files → "make me a dashboard of X" writes a query
  note.

### Time machine ✅ (retention settled)
- Local per-note history: debounced snapshots in `.shale` (content + timestamp +
  provenance). Per-note scrubber timeline with diffs; restore any point.
- **Cadence (revised for autosave):** decoupled from saves — snapshot at
  editing-session end (gap > 10 min, configurable), checkpoint every ~3 min in
  a long session, before risky ops (MCP bulk edits, renames, restores), and on
  explicit `ctrl-s`/`:w`. Trivial diffs (< ~20 chars) are skipped. See
  "Autosave × time machine" above.
- **Retention:** every snapshot for 24h → hourly for 30d → daily thereafter.
  **Cap: 100 MB default, configurable**, oldest thinned first.
- **Storage:** store diffs against the previous snapshot, not full copies — text
  notes compress enormously, which is why generous retention is nearly free.
  (Answer to "why keep so much": diffs make it cheap; the value is undoing a
  bad bulk agent edit or a bad rename discovered days later, not backup-per-se —
  the vault itself, especially under git, is the backup.)
- **DB backup:** copy the DB into `.shale/backups/` once a day, keep a week
  (7 copies; DB is big and disposable — it's cache/transient data, notes are
  the source of truth, so no need for more).
- Local and free (Obsidian gates version history behind paid sync). Pairs with
  graph link-age coloring.

### Link health & rename propagation ✅
- Rename a heading → fix `[[note#heading]]` anchors vault-wide; rename a tag
  vault-wide (extends `rename.rs`).
- Link-health report: broken links, near-duplicate titles (fuzzy), orphan
  clusters, unused attachments. DB makes it instant.

### Word counts ✅
- DB `word_count` column, updated incrementally on save.
- Status bar: current note. `:wordcount [folder]`: vault total or any subtree
  (`SUM WHERE path LIKE 'folder/%'` — instant).

---

## Editor experiences (custom-renderer advantages)

### Outliner mode ✅
- **Not a separate data model:** the buffer stays plain Markdown. The outliner is
  a set of operations on the list tree parsed from the existing markdown parser;
  every operation compiles to text edits, so undo/vim/dirty-tracking/file-format
  all work for free.
- `outliner` keymap context active when cursor is on a bullet; off a bullet =
  normal vim.
- Keys: `Tab`/`Shift-Tab` indent/dedent subtree; `J`/`K` move subtree; `o`/`O`
  sibling with auto bullet marker; `dq`/`y` operate on subtree; fold subtrees via
  `fold.rs`.
- Subtree = bullet + deeper-indented lines + continuation paragraphs + checkbox
  state. Fence-aware (`-` in a code block is never a bullet). Ordered lists
  renumber on move (config: literal vs `1.` style).
- Bullets-as-pages: `gd` on a bullet whose text is a wiki-link opens it;
  "expand to note" turns a bullet into a new note + `[[link]]`.
- Edge cases to spec at implementation: mixed markers at one depth, blank lines
  inside subtrees, task checkboxes, Tab-vs-completion conflicts (insert mode
  unaffected).

### Editable transclusion ✅
- `![[note#section]]` rendered live and editable in place; edits flow to the
  source buffer. Cheap for us (we own buffer + renderer); this pattern is unusual
  because web-tech editors (Obsidian) make it hard — for us it's natural.
  Finicky to make *feel* good (cursor handoff, scroll sync, dirty state) —
  polish pass required.

### Focus / typewriter mode ✅
- Dim all but the current block (sentence/paragraph/section granularity, config);
  pure alpha ramp in the renderer. Optional typewriter scroll (active line pinned
  near center). Same dimming machinery as search-illumination — three sources,
  one system.

### Writing stats ✅
- Live word count, sentence-length histogram, reading time, most-used words this
  session (computed from the parsed buffer; HarperLS piggyback for grammar stats).
- Panel also hosts the per-folder word-count tree.

### Hover link preview ✅
- Wiki-link under cursor → floating card: first lines of the note + its graph
  neighborhood mini-render (reuses minimap machinery). Popup placement exists.

### Graph minimap ✅ (see graph-view-plan.md M5j)
- Corner mini-graph of the active note's depth-1 neighborhood, always visible in
  the editor; click-through to the full graph.

### Note pinning ✅ (demoted from a feature to a pane flag)
- Once tiling splits exist: `:pin` on a pane = dimmed reference mode — lower
  contrast, excluded from the jump chain, auto-closes when the last other pane
  closes. The pane system provides the layout; the *behavior* is the feature.

---

## Quick capture ✅ (socket design, cross-platform)
- `shale capture "text"` (or stdin/prompt): if the app is running, talks to a
  **unix socket** in `.shale` (Linux/macOS) / **named pipe**
  `\\.\pipe\shale-<vault-hash>` (Windows) — same socket doubles as the MCP
  standalone's app-detection lockfile. App appends to `inbox/` with timestamp +
  optional tag + toast. If not running: appends directly.
- No global hotkeys grabbed by the app (Linux compositors own keys — bind it in
  niri config). Windows *has* a global-hotkey API (`RegisterHotKey`) — possible
  later nicety, but the CLI+pipe is the primary path everywhere (works from any
  app, TTY, scripts).
- In-app capture command in the palette for when Shale is focused.

---

## Tag tree ✅
- Nested tags = `#work/project/a` convention; DB stores flat strings, tree built
  by splitting on `/`.
- Panel: collapsible tree, direct + inclusive rollup counts, fuzzy filter line,
  keyboard nav.
- Per-node actions: Enter = search that tag; `g` = filter graph to it (pairs with
  tag-colored graph nodes); `r` = rename tag vault-wide (link-health machinery,
  rewrites notes + DB incrementally); assign color → feeds graph groups.
- Editor synergy: `#` autocompletes existing tags from the DB — same data as the
  tree.

---

## Graph open questions (resolved)

### Tag-node coloring ✅
- Stable hash of the tag name → hue (manual coloring is annoying).
- **Distinctness rule ✅:** raw hash isn't enough (two tags can hash to near-identical
  hues). Assignment is deterministic with a collision-avoid pass: process tags in
  sorted order; candidate hue = hash(tag); if within a minimum hue distance (~20°
  in OKLCH) of an already-assigned tag, probe offsets until clear. Guarantees
  perceptible separation for the first ~12-16 tags (what the eye can reliably
  tell apart); past that, gracefully degrade to hash-random (overlap acceptable
  at high tag counts, per user).
- Fixed saturation/lightness band (OKLCH) so no muted greys and no neon; hues
  measured perceptually, not raw HSL degrees.
- **Hue families for nested tags ✅:** `#work/project/a` inherits the root tag's
  hue with a lightness/saturation step per depth — related tags read as one
  color family, matching the tag tree. Family cohesion wins inside a subtree;
  distinctness applies between roots.

### Drag-pin ✅
- Unpin on release (node rejoins the sim); Shift-drag pins permanently.

### Time-lapse → reworked as "vault history scrubber" 🔭
- Plain creation-date animation: skip as specced (pure novelty).
- The interesting version: a timeline scrubber at the bottom of the graph tab —
  drag it and the graph shows the vault **as it was at that date**: nodes exist
  if created before the scrub-date, edges if the link existed then, brightness
  by activity in the surrounding window.
- Data: time-machine snapshots + link `first_seen` (both already specced) —
  mostly a filter over existing data.
- Useful, not novelty: "what did my vault look like a year ago / before the
  agent got loose in it"; watch clusters form around projects. The optional
  playback button *is* the time-lapse, now worth watching.

#### History scrubber — details (refining)
- **Event model:** the DB keeps an append-only event log per note:
  `created | edited(count, provenance) | opened | linked(first_seen A→B) |
  renamed | deleted`. The scrubber is a query over this log at date D — no
  snapshots needed for the scrub itself (snapshots are for restore).
- **Scrub semantics:** node visible if created ≤ D; edge visible if first_seen ≤ D
  (and, if link removal is tracked, removed > D); node size = degree *at D*;
  brightness = activity in a window around D (window width is a control:
  day/week/month).
- **Layout stability while scrubbing (critical):** positions come from the
  *current* settled layout — scrubbing never re-runs the sim, so dragging the
  timeline makes nodes fade in/out in place. Watching a cluster grow in place is
  the whole point; nodes jumping around would kill it.
- **Playback:** play button, speed control (1×/4×/16×), auto-pause on activity
  bursts. Render-only: ripples fire as nodes appear (reuse M5b ripples).
- **Markers on the timeline:** ticks for big events — bulk MCP edit sessions
  (provenance), renames, your own writing bursts. Click a marker to jump.
  Makes the timeline legable: "that spike was the agent reorganizing my notes."
- **Compare mode 🔭:** split the scrubber into two dates (A|B): nodes only in A
  dim left-tinted, only in B right-tinted, in both neutral — "what changed
  between these two dates" at a glance.
- **Filterable provenance ✅:** scrub with a provenance filter on — "show only what
  the agent created", "only my edits". Turns the scrubber into an audit tool.
  This generalizes beyond the scrubber: **provenance is a first-class filter
  everywhere** (graph filter panel, tag tree, search, query notes) — the DB
  already stores created-by / last-modified-by per note, so "agent-created
  notes", "notes the agent modified this week" are one WHERE clause. The graph
  gets a filter toggle (user / agent / both), and node badges or ring styles can
  mark agent-touched notes even outside scrub mode.
- **Jump-chain overlay:** during playback, the thread (M5a) can show the jump
  chain *as it was* at date D — your navigation path through the past vault.
- **Scope:** whole-vault by default; respects the constellation filter (scrub
  one folder's history).
- **More ideas (parked 🔭):**
  - *Ghost view:* at date D, show current-but-not-yet-existing nodes as faint
    outlines ("what was coming") — toggle.
  - *Session view:* instead of dates, step through *your editing sessions*
    (gaps > N min split sessions); scrub session-by-session — "what did I do
    last Tuesday" in 6 steps instead of a continuous drag.
  - *Export ✅ (fun novelty, approved):* save a playback as a short animation
    (frames → GIF/MP4) for sharing vault progress; render frames from the scene
    directly (no screenshot tool dependency).
  - *Stats timeline ✅:* word-count-over-time sparkline above the scrubber (DB
    word_count history) — the graph plus "how big was the vault then".

---

## Obsidian parity audit (2026-10-28)

Swept Obsidian's built-ins + core plugins + top community plugins against this
document. Everything not listed here is either already covered (quick switcher,
palette, bookmarks, hover preview, rename propagation, word count, file
recovery→time machine, Bases/Dataview→`.shq`) or a deliberate out (sync,
mobile, Canvas, slide mode, themes). The gaps, with rulings:

### Properties: frontmatter as data ✅ (P1 schema decision — do it in the DB spine)
- The DB indexes YAML frontmatter: `note_props(note, key, value)` updated on
  save/index. Without this, `.shq` can't answer "all notes with
  `status: draft`" — the query-note feature is crippled without it.
- Editor: a Properties strip at the top of the note (editable key/value table,
  rendered from the raw YAML block — buffer stays plain text, same principle as
  the outliner). Key autocomplete from the DB's distinct-key set.
- Light type inference (date, number, list) for sorting/queries; no schema
  enforcement — frontmatter stays free-form.
- Rename a property key vault-wide → link-health machinery applies.
- Search + tag tree + graph filters can all consult props once indexed.

### Tasks as data ✅ (cheap DB win)
- DB table `tasks(note, line, text, done, due, status)` updated incrementally
  on save (the markdown parser already sees checkboxes).
- Enables: task dashboard `.shq`s for free; toggle-done from query results
  edits the source text (same compile-to-text-edits rule as the outliner);
  "open tasks in this note" counts in the status bar.
- Due-date convention: parse `📅 YYYY-MM-DD` and `due:YYYY-MM-DD` suffixes
  into the `due` column (text stays as written). No recurrence engine —
  that's Tasks-plugin territory, out of scope.

### Search operators ✅
- `ctrl-shift-f` grammar: `tag:work`, `path:projects/`, `file:name`,
  `"exact phrase"`, `/regex/`, `todo:`/`done:`, `created:>2026-01-01`,
  `modified:week`, `agent:` (provenance — ours alone), `prop:key=value`
  (rides the props index).
- All DB-backed → instant. `/` open-buffer stays plain fuzzy; operators are
  the vault-search grammar.

### Daily notes ✅
- `:today` opens/creates `journal/YYYY-MM-DD.md` (folder configurable);
  `:day 2026-10-28` jumps to a specific one.
- Calendar popover: month grid, dots on days that have a note (the DB knows),
  click to jump. `:calendar`.
- Creation seeds a minimal skeleton (frontmatter + `# date` heading). Full
  templates stay an open question; daily notes don't wait on it.
- Composes with the scrubber's session view and capture (capture can append
  to today's daily instead of inbox — config).

### Attachment capture ✅
- `ctrl-v` with an image on the clipboard → save under `attachments/`
  (vault-root or adjacent, config) → insert `![[name]]`. Drag-drop a file
  onto the editor does the same.
- Naming: `YYYY-MM-DD-HHMM-<original-or-hash>.ext` (sortable, collision-free).
- Graph already models attachment nodes; this feeds them.

### Trash ✅ (user: "very important")
- Every delete (file tree, `:Gdelete`, `:Gmove` leftovers) goes to the **OS
  trash** via the `trash` crate (MIT, pure Rust, cross-platform) — not
  vault-local (stays out of git, restorable from any file manager).
- The DB event log records the deletion (path + timestamp + provenance), so
  "what did the agent delete" is a query, and links to the deleted note
  degrade to ghosts as already specced.
- 30-day recovery is the OS's job; the time machine keeps the *content*
  history if the note ever had one.

### Workspaces ✅ (session restore is the base; named workspaces are sugar)
- Pane layout + open tabs + active pane + per-pane cursor saved as JSON in
  `.shale/workspaces/<name>.json`.
- **Auto-save the session continuously** (cheap) and restore on launch —
  this is the "we need to save it anyways" part, and it makes crash-recovery
  feel like nothing happened.
- `:ws save name` / `:ws load name` / `:ws list` on top. MCP can list/load
  (agent sets up a working context).

### Block references ✅ (hooks straight into transclusion + hover preview)
- `^abc123` anchor at the end of a paragraph/bullet; `![[note#^abc123]]`
  embeds it; `[[note#^abc123]]` links to it (jump = scroll to block).
- ID generation: command appends `^` + 6 random chars (stable once written;
  content-hashed IDs break on edit, rejected).
- mdstyle gains a block-anchor span; the transclusion resolver grows from
  note/section to block granularity — and editable transclusion gets its
  finest atom for free. Hover preview on a block ref shows just that block.
- Graph edges stay note-level (Obsidian's rule); block refs enrich the
  backlinks panel with the exact block.

### Outline pane ✅
- Heading tree in a sidebar: click to jump, current heading highlighted while
  scrolling, respects folds. Distinct from outliner *editing* — this is
  navigation. Reuses the parser's heading spans.

### Unresolved link → create note ✅
- The editor half of ghost nodes: command on cursor-over-link (`ctrl-enter`
  or `o` in a popup) creates the note at the path `Vault::resolve_link` says
  it would resolve to, and opens it.
- Graph: click a ghost ring → same create. Ghosts stop being dead ends.

### Random note ✅
- `:random [folder]` — one SQL query (exclude current note; respects active
  filters). Zettelkasten staple, trivial on the DB.

### Vault switcher ✅
- Recent vaults list in `.shale/recent.json`; startup picker when no vault
  arg; `:vault` to switch. One vault per app instance (switch = close +
  reopen) — keeps window/DB state simple.

### `shale://` URI scheme ✅
- `shale://open/<vault>/<path>` (+ `?line=`) → browser/external handoff.
  Linux: `.desktop` entry + xdg-open; Windows: registry key. Security: only
  paths inside a *known* (recent) vault open without confirmation.

### Table editing feel 🔭 (awaiting ruling)
- `Tab` in a table cell → next cell (wraps to next row; at the last cell,
  creates a new row); `Shift-Tab` → previous. `table` keymap context;
  insert-mode Tab elsewhere untouched. Pairs with the `cit` text object.

### Math ❄️ / Mermaid ❄️
- Both parked (user call, 2026-10-28). Revisit after live-preview core lands;
  math may take the "clean source + hover-render" middle path rather than
  full typesetting in the custom renderer.

---

## TTRPG / D&D toolkit (2026-10-29)

D&D is the flagship domain vault: most of it rides already-approved spine
features (props index, `.shq`, tasks table, block refs, graph filters,
callout rendering). Only the two below are new subsystems worth their own
build; the rest is configuration of things that already exist.

### Secret blocks ✅
- `> [!secret]` callout variant: renders collapsed/dimmed with a lock affordance;
  click (or `:reveal`) expands in place. Source stays plain markdown — the
  buffer never changes, same principle as all live-preview rendering.
- `:reveal-all` / `:hide-all` toggle for the whole buffer (GM display mode;
  config default per-vault: revealed for a GM vault, hidden for a player vault).
- **Player-handout export**: command (and MCP tool) that exports a note to
  markdown/HTML with every secret block *stripped* — the handout is what gets
  printed or sent, not the source note. Nested secrets strip with the parent.
  Provenance: export is recorded in the DB event log (what was handed out, when).
- Composes with transclusion: embedding a note that contains secrets shows the
  secret blocks in their collapsed state, never silently expanded.
- Rationale: no markdown app distinguishes GM knowledge from player knowledge;
  the custom renderer makes it a callout-type decision, not a plugin hack.

### Inline dice ✅
- Syntax: `!r 1d20+5` (also `!r 2d8+1d6-2`, `!r 4d6kh3` keep-highest for stats,
  `!r 1d%` for percentile). Renders as the expression + result + individual
  dice, styled inline; re-render never re-rolls.
- Determinism: the roll is resolved once and the result is written back into
  the source as an annotation — `!r 1d20+5 → 17 [12, +5]` — so the file stays
  the single source of truth, plain text survives export, and the app is
  crash-safe. Re-running the roll is an explicit command (`:reroll` on cursor).
- DB `rolls(note, expr, result, dice, ts)` table → "every nat 1 this campaign",
  per-session roll history, average-damage queries via `.shq`.
- `!r` in a *table cell* is the hook for random tables later (see parked) —
  syntax reserves nothing extra, so tables can be added without a redesign.
- No dice-notation crate dependency unless it's tiny/MIT; the grammar
  (`NdN[+/-m][khN|klN]`) is small enough for shale-core + tests.

### Random tables ❄️ (parked, user call 2026-10-29)
- Markdown tables as rollables (`:rolltable <name>` → weighted row pick,
  highlight + log). Parked, not rejected: the inline-dice `!r` syntax and the
  rolls table leave the door open; revisit once inline dice ships. Likely
  superseded by "random generators" below (tagged note-collections beat
  hand-written tables for anything reusable).

### Initiative tracker ✅ (a mode over a plain-text table, not a new surface)
- The distinction from `.shq`: `.shq` is a read-only query projection;
  initiative is an *editing mode* with a cursor (whose turn, which round) and
  fast mutation at table speed. The tracker IS a plain markdown table in the
  session note — `| Pos | Combatant | Init | HP | Conditions |` — and the
  feature is the mode on top. No hidden state: the table is the state.
- `combat` keymap context: `]t`/`[t` next/prev turn, `:damage 14` /
  `:heal` on the active row, `:condition restrained 2` (counts tick down at
  turn end), `:round` advances. Commands mutate rows the way `:Gmove`
  mutates lines — plain text edits, undoable, provenance-logged.
- **Manual initiative**: any Init cell can be typed by hand (real dice,
  Roll20, a dice app). `!r 1d20+3` in the cell is optional sugar, not
  required — the tracker never demands the roll happened in-app.
- **Wikilinks in Combatant cells**: `[[morgoth]]`, `[[ara#statblock]]` —
  the mode resolves the link to open/inspect the statblock or player sheet
  (`:inspect` on the active row), and the agent can follow the same edges.
  Hover preview works like everywhere else.
- Renderer: active row highlighted + round counter in the table header,
  dead/dying rows dimmed. All render-only; source stays a normal table.
- **MCP**: the table's state (rows, active row, round) is readable via the
  MCP server, and the agent can act on it (damage, condition, advance turn)
  through the same row-mutating commands — agent edits carry `agent:`
  provenance like every other edit.
- Scope guard: narrative tracker only — no grid, no token-pushing (dragging
  avatars on a battle map), no range/line-of-sight geometry. That's VTT
  territory (Roll20/Foundry/Owlbear); we track turns and HP, positioning
  lives at the physical table.
- Synergy: initiative entries can be inline dice → every init roll lands in
  the `rolls` table; after combat the table is just text, so `.shq` answers
  "which combats lost someone", "average rounds per fight".

### Random generators ✅ (the vault structure IS the table)
- Generalize `:random` to filters: `:random tag:hook`,
  `:random type:monster cr<=5`, `:random tag:name prefix:orc` — one SQL
  query over the props/tags index (same machinery as search operators).
  No table syntax at all; tagged note-collections replace hand-written
  d100 tables.
- Weighting via optional `weight:` property (default 1).
- Exclude-recent: `not:seen-3-sessions` rides backlink timestamps so the
  same hooks don't recur every game.
- **Slot-machine spin**: the picker cycles candidates ~0.5s before landing
  (trivial for the custom renderer; at a table it's *fun* — markdown apps
  never get to be).
- **Chained generation**: `:generate encounter` = random monster
  (party-level filter) + random `tag:location` + random `tag:hook` →
  assembled into a new note from a template, provenance recorded. A
  random-encounter table with zero syntax; generalizes to names, treasure,
  rumors.

### GM screen ✅ (mostly saved queries + a pane flag)
- A note that's a stack of named `.shq` blocks (party ACs/HP, active quests
  with due dates, unresolved hooks, next-session date) + `:gm-screen`
  opening it pinned in a side pane, auto-refreshing.
- Only genuinely new machinery: `.shq` blocks get a `name:` so they're
  saved, composable, and referenceable. Everything else is existing
  surfaces.

### Agent GM assistant 🔭 (note for later — mostly already built)
- MCP + props + rolls + graph already answer: "which NPCs haven't appeared
  in 5 sessions" (backlink timestamps), "which hooks are unresolved"
  (ghost nodes tagged `#hook`), "what did the party swear revenge on"
  (search). Secret-awareness rides handout-export rules (agent reads
  secrets; exports strip them).
- The missing piece is packaging: a **session-recap generator** — template
  note + `.shq` aggregates + rolls table → draft recap the GM edits. A
  prompt template + a template note, not a feature. Revisit once the
  pieces above ship.

### Image pins ❄️ (tabled, user call 2026-10-29)
- Pins as data on an image note (normalized x/y + link + label in a
  `shale:pins` block); renderer draws numbered markers, click opens the
  note, drag reuses graph drag-pin machinery. Dungeon/city map as a hub.
- Scope line if it ever un-parks: reference/annotation only — no tokens,
  no grid, no movement (same VTT guard as the initiative tracker).

### Free rides (no new subsystems — note the conventions, don't build)
- Statblocks: `type: monster` + `cr/hp/ac` frontmatter → props index + `.shq`
  queries; a `statblock` callout type for pretty two-column rendering later.
- Quests = tasks table (checkboxes + `due:`); campaign/session notes =
  daily-notes pattern with a `campaign:` property (`:session 12`).
- Block refs (`^id`) for exact NPC quotes in session notes; relationship
  graph (NPC↔faction↔location) is just the existing graph + tag coloring.

---

## Explicitly out
- Sync, mobile, plugins (see "MCP is the plugin system"), live web apps in notes
  (no webview; possible later: static readable-content cards via pulldown-cmark's
  HTML handling, cached + read-only).

---

## Still open (decide before/while implementing)

### Decided 2026-10-28 (late round)
- **`/` search = open buffer** (vault search stays `ctrl-shift-f`).
- **`ctrl-o` freed from QuickOpen** → becomes jump-back (`ctrl-i` forward),
  per the Goals promise; QuickOpen keeps `space f` / `:e`. The jump chain
  becomes first-class state (feeds the graph thread M5a and bookmarks panel).
- **Autosave: 2 s idle debounce** + on-blur + on-quit (roadmap P0.2).

### Autosave × time machine (decided 2026-10-28)
- Autosave changed what "save" means: it used to be a deliberate act (a good
  snapshot boundary); now it's background noise every 2 s. **Snapshot triggers
  must decouple from saves.**
- **Chosen: session-based snapshots + checkpoints:**
  - An *editing session* = activity with gaps < N min (default 10,
    configurable — same session concept as the scrubber's session view).
  - Snapshot at **session end** (the settled state), plus a **checkpoint every
    ~3 min** during a long continuous session, plus **before risky ops**
    (MCP bulk edits, renames, restores), plus **explicit saves** (`ctrl-s`/`:w`
    still exist as deliberate acts worth marking).
  - **Dedupe rule:** skip a snapshot whose diff vs the last is trivial
    (< ~20 chars changed) — kills the "paused mid-sentence" noise.
  - **`:snapshot [label]` command ✅:** manual snapshot with optional label —
    the escape hatch before letting an agent near a folder; labeled entries
    sort to the top of the note's timeline.
  - The live file is always "now" — the time machine's head is the current
    buffer, so autosave protects the present and snapshots protect the past;
    a crash loses nothing that autosave already wrote.
- Why not snapshot-on-autosave: every typing pause would write a near-identical
  snapshot — the 100 MB cap survives it (diffs are tiny) but the timeline and
  scrubber markers become meaningless noise. History entries should be
  *meaningful states*, which is also what makes session view work for free.
1. ~~SQLite ruling~~ ✅ RESOLVED: SQLite is fine — best choice, small static
   (bundled, build-time) dependency; the "no external bindings" rule stands for
   everything else. **Tree-sitter ruled in the same way ✅ (2026-10-28):** C
   bindings fine, developer-friendly feature wanted.
2. ~~Time machine retention~~ ✅ RESOLVED (see Time machine section).
3. ~~Scoped agent vaults~~ ✅ RESOLVED: yes.
4. ~~Graph open questions~~ ✅ RESOLVED (hash hues w/ distinctness constraint,
   unpin-on-release + Shift-pin, time-lapse reworked as history scrubber 🔭).
5. **Agent edit review UX** — design pass owed (strip placement, keys, diff
   rendering); feature itself is ✅.
6. **Roadmap sequencing** — new features vs the unchecked core editor goals
   (live preview, vim, file tree, tabs). Tabs gate the graph; the DB gates query
   notes, word counts, tag tree, bookmarks, link health, time machine, glow.
   Draft a dependency-ordered roadmap.
7. **Graduate ✅ items into PROJECT.md** — pending action, not a decision.
