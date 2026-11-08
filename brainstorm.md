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
- **Pending edits are stored in plain text** (`.shale/pending-edits/*.toml` —
  human-readable, git-diffable), NOT only in the DB cache; the DB may index
  them but the files are the source of truth, so a corrupt DB never loses a
  pending edit.
- **On quit:** prompt if edits are pending. If the user closes without answering
  (or the app is killed), the pending file stays on disk and is re-offered next
  launch. Nothing is silently applied or discarded.
- **Design pass RULED 2026-11-01** (all of it; feeds roadmap P2.5):
  - **Split into P2.5a / P2.5b.** a = the safety story: queue, files, strip,
    keys, the `review = true` unblock, quit prompt — zero phantom-region risk.
    b = the inline overlay (agent-presence cluster, below). Nothing in a is
    thrown away by b; inline is additive by construction.
  - **The MCP server writes the pending file itself** (atomic temp+rename) and
    returns the edit id in its tool result; the app discovers via watcher +
    DB index. Falls out of the separate-binary architecture and gets two
    ruled behaviors free: edits queue while the app is down, and they survive
    `kill -TERM` because they were never in app memory.
  - **Format: TOML, one file per MCP write call**
    (`<timestamp>-<profile>-<tool>.toml`), `[[hunk]]` blocks embedding
    unified-diff text in multi-line literal strings — readable, git-diffable,
    serde already in the stack. One file per call so a kill can never
    half-write a multi-edit file.
  - **Strip placement: bottom, above the status bar.** The status bar owns
    whole-note facts and stays untouched; the note never shifts; it reads as
    vim's command area; it appears only when the queue is non-empty.
  - **Keys, two layers.** Strip focused: bare `j`/`k` hop, `y`/`x`
    accept/reject, `o` jump to the note, `za` collapse (a list widget — no
    collisions). Buffer focused: `]a`/`[a` hop the current-hunk marker, and a
    **review keymap context** layers in only when the cursor is on/inside a
    phantom region, where bare `y`/`x`/`za` work — so `y`/`x` never shadow
    yank/delete in prose (initiative-tracker keymap-context precedent).
  - **Diff renderer pulled forward:** P2.5a builds the shared line-level diff
    primitive (compute + paint); P4.2 reuses and upgrades it for the time
    machine instead of starting fresh. Engine: the pure-Rust `similar` crate
    (user ruling 2026-11-01: a small pure-Rust crate is fine; the no-bindings
    rule still stands). Word-level inline diffs within hunks deferred.
  - **Conflict policy: apply-or-stale.** Accept patches against the current
    buffer; if the diff context no longer matches (the user edited the same
    lines, or two agents' hunks overlap), the hunk flips to a `stale` badge
    in the strip — reject, or let the agent re-propose, are the only moves.
    No 3-way merge in v1. Overlapping pending hunks get flagged in the strip.
  - **Agent-side recovery tool:** a read-only `pending_edits` MCP tool lists
    the calling agent's own pending hunks with diff + status
    (`pending`/`stale`), so a broken patch is visible to the agent that made
    it. "Redo" is just a fresh write call re-anchored against the current
    buffer (queues as a new pending edit). The agent may **retract** its own
    pending edits — retract is the one queue mutation it keeps; accept stays
    user-only.
  - **No in-place editing of proposed hunks** in v1: accept/reject only —
    reject and re-ask the agent, or accept then fix it yourself.

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

### Agent presence & provenance visibility ✅ (2026-10-29, Grok-review cluster)
One visual language for "the agent is/has been here", three surfaces, one
data source (the event log + pending-edits index). All of it is
timestamp-derived: static paint, expiring on its own, zero animation,
zero frames when idle — and all of it is config-off-able as a group
(`[agent] presence = true`, plus per-surface toggles).

- **Event-log line-spans (P1 schema decision).** The `events` table grows
  optional `line_start/line_end`: the line range an edit touched, captured
  at edit time from the save diff (the provenance helper already knows it).
  Spans are **approximate and decaying**, not anchors — line numbers drift
  after later edits, which is fine because every consumer is a signal that
  expires in minutes. Schema comment must say so, so nobody later builds
  permanent annotations on them. Cheap now, painful to retrofit.
- **Provenance badges.** Gutter/tab/tree glyphs: "last touched by agent",
  "has pending review", "edited this session"; hover → the exact event-log
  row (who, when, what span). Note-level is nearly free; line-level rides
  the spans above.
- **File-tree ticks.** The tree is the only whole-vault surface, and today
  nothing marks agent-touched files there (gap this cluster closes): a
  small profile-colored dot next to any note a connected profile touched
  within the last N minutes. Distinct from the heatmap (which is
  edit/visit *frequency*); the tick is *presence now*. Same dot language
  as the tab badge and gutter tick — three surfaces, one fact.
- **Presence by inference, not declaration.** No `set_cursor`/`focus` tool
  required — presence is inferred from recent MCP reads/writes in the
  event log ("profile X touched these notes in the last N min"). Honest
  and works with any client. A live `focus` tool that lets a well-behaved
  agent show *where it's working right now* (ghost cursors) stays the
  aspiration, unblocked by this.
- **Editor-side ripple/glow.** The graph's M5b language extended into the
  note: recently agent-touched blocks get a soft age-decaying halo
  (<~10 min, intensity = f(age), same rule as the graph). Derived from
  event timestamps, drawn when dirty — never a ticking decay animation.
  `[editor] glow = true|false` (off is fully off).
- **Inline review overlay (hybrid; feeds the P2.5 design pass).** Pending
  hunks render *in the note* where they would apply: removed lines dimmed
  + strikethrough, added lines tinted, left-bar per hunk, collapsible to
  "−2 +4". Phantom regions: not in the buffer, so j/k, selection, wrap,
  folds, and search skip them for motion but paint them (the row-mapping
  functions in `line.rs`/hit-testing already centralize this). Keys follow
  the two-layer scheme ruled in "Agent edit review" (`]a`/`[a` in the
  buffer; bare `y`/`x`/`za` only inside the review keymap context on a
  hunk) — all registry commands, MCP-visible; the agent can never accept
  its own edits.
  Re-anchoring: the pending file's diff context re-matches the current
  buffer; confident match → stays inline, ambiguous/vanished → hunk leaves
  inline and lives only in the strip, badge still counts it. **The strip
  is authoritative, inline opportunistic** — nothing is ever invisible.
  Ruled 2026-11-01 as **P2.5b** (after the P2.5a strip-only safety story);
  colors follow the theme's git-diff palette (dim/strikethrough removals,
  tinted additions).

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
- **Jump-chain breadcrumb (ruled 2026-10-29):** the editor shows the chain as
  a one-line status-area breadcrumb (`aria ← session-12 ← hook-rats`), not a
  gutter filament — the gutter is reserved (diagnostics, folds, provenance
  ticks, surface widgets). The graph's M5a thread stays the visual signature:
  the editor shows the chain's *text*, the graph shows its *shape*.

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

### Copy, paste, duplicate in the file tree 🔭
Asked 2026-11-05 ("do we have copy and paste in the file tree?") — no: the tree has
`n` (new), `r` (rename) and `shift-d` (delete), and there is no way to duplicate a note.
- **Duplicate** (`c` in the tree) is the piece people actually want and needs no
  clipboard: copy the note beside itself under the collision rule `note_name` already
  uses for first-line names (`Idea.md` → `Idea-2.md`).
- **Copy / paste** (`y`, `p`) wants a *vault* clipboard holding a path, not text — the
  editor owns the text clipboard, and sharing it would make `p` ambiguous. Pasting into a
  folder row places it there.
- **Move** is that clipboard with the source removed, and it owes what `:rename` already
  pays: every wikilink pointing at the old path rewritten, or the move quietly breaks the
  graph. That is why it needs its own command id and not a file copy.

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

### Editable transclusion ✅ (roadmap **P4.0** — a read-only embeds, b editable in
place; the interaction design pass is **done 2026-11-06**, in `transclusion-plan.md`)
- `![[note#section]]` rendered live and editable in place; edits flow to the
  source buffer. Cheap for us (we own buffer + renderer); this pattern is unusual
  because web-tech editors (Obsidian) make it hard — for us it's natural.
  Finicky to make *feel* good (cursor handoff, scroll sync, dirty state) —
  polish pass required.
- **Undo (v1 ruling, 2026-10-29): buffer-local with a seam indicator.** Ctrl-Z
  unwinds in whichever buffer holds the cursor; no shared undo timeline across
  the boundary. When local undo is exhausted, the next Ctrl-Z reports "next
  undo is in [[morgoth]]" instead of beeping — the discoverability of a shared
  chain without its hazards (the embedded note may be open elsewhere with its
  own undo state). A shared undo group can be layered on later; no data change
  blocks it.
- **Dirty/save:** each note keeps its own dirty state (it is its own file);
  save/autosave covers host + embeds together; closing a host with a dirty
  embed warns through the same machinery as closing a dirty tab. The host never
  *silently* saves the embed.
- **Nesting cap (binding, 2026-10-29): 1 editable embed + 2 read-only embeds =
  max 3 levels deep**, killing infinite embed loops by construction. Deeper
  embeds render as a **badge/sticker showing the note name** (clickable, but no
  content). `gd` on an embed jumps into the embedded note; `gd` with the cursor
  on a wikilink *inside* an embed jumps to that link instead.
- **Interaction pass ruled (2026-11-06), full text in `transclusion-plan.md`:**
  - **Enter** by clicking inside the body, or `ctrl-]` on the embed's own `![[…]]` row
    (vim's tag jump, free in Shale; `gr` rejected — it is vim's virtual replace).
    **Exit** with `ctrl-o`: vim records tag jumps in the jump list, so this is what a vim
    user already does, and Shale's jump chain already lands on the exact byte — the embed
    stack *is* the jump chain, not a second stack. `<esc>` never exits.
  - **One level of editing** (level-1 embeds only; nested stay read-only), and the status
    bar names the note the caret is in.
  - **One scroll axis: the host's.** No nested scroll fields — "feels too much like a
    browser and it gets confusing fast". A long embed is scrolled *through*.
  - **The seam is a buffer boundary and everything buffer-scoped stops there** — motions,
    counts, `gg`/`G`, search, folds, the ruler, undo. Stated so screen motions fall out:
    *the view scrolls freely; the caret never crosses the seam on its own.*
  - **One caret per note per window.** The caret stays in `Buffer`; a tab and an embed of
    one note share text *and* caret. Per-view carets are the expensive half, not taken.
  - **Dirty display is transitive, dirty state is per file**: the host tab's dot lights if
    the host or any embed it renders is dirty; `:w` inside an embed writes both.
- **Edge cases ruled the same day** (full ledger in `transclusion-plan.md` §5, rulings
  §7): a dirty embed with no tab **materialises a tab**, so unsaved work always has a
  visible home. **One note may appear several times in one host, showing different sections
  at once** — `![[gift#The middle]]` beside `![[gift#The end]]` — because that is a posture
  worth having without two panes; the block the caret is in is editable, the others are
  views, and a view whose section stops resolving **badges the missing section** with no
  polling (the body cache keys on `Buffer::change_id`, the same stamp `fold_cache` uses).
  Clicking host text exits; `:rename` acts on the caret's note and `:w <name>` refuses
  inside an embed; an entered section that vanishes under the caret exits with a status line
  while a view block badges — **the caret decides**; agent proposals inside embeds wait for
  P4.2; `:q` closes the host's tab; the entered embed does not ride the session file.

### Focus / typewriter mode ✅ (semantic version ruled 2026-10-29)
- **Dim by semantic unit, not distance:** everything except the current unit
  drops to low alpha. Scopes: `sentence | block | section` (default block;
  `block` = the paragraph unit P0.8 shipped, `section` = the heading unit,
  the long-document mode; `sentence` is P0.9). The distance-ramp stays as a
  fourth option for anyone who liked the original.
  Unit boundaries come from `mdlayout`/`mdstyle` — the same knowledge the
  markdown text objects use (`ci*` callout, `da#` section); no fuzzy math.
- **Scope is session state, not config** (P0.8's ruling stands, 2026-10-30):
  `:focus block|section|sentence` sets it for the running window; it persists
  with the session file (P1.8), not with the settings the user edits by hand.
- **Laser mode dims the chrome too** — file tree, status bar, tab bar drop to
  the same low alpha. One keybinding cycles: off → unit → unit+laser.
- **Typewriter scroll:** active unit pinned near mid-screen. Must respect folds
  and soft-wrap row math; "gentle" = one short animation that is itself the
  dirty-marker, then stops — idle stays idle.
- Same dimming machinery as search-illumination and the agent glow — three
  sources, one alpha system.

### Writing stats ✅
- Live word count, sentence-length histogram, reading time, most-used words this
  session (computed from the parsed buffer; HarperLS piggyback for grammar stats).
- Panel also hosts the per-folder word-count tree.

### Inline computed surfaces ✅ (policy ruled 2026-10-29; one widget to start)
- Render-only widgets sourced from the DB, buffer untouched. **The policy is the
  feature; widgets are added only as real consumers appear.**
- **Placement: the note's margins only** — gutter beside specific lines, header
  strip above the note, or the status area. Never phantom lines inside prose.
- **The division of labor (binding):** the status bar owns whole-note/vault
  facts (the word count P1.7 puts there); a computed surface must be
  **region-anchored** — something the status bar structurally cannot do.
  This keeps the two from overlapping by construction: a surface never
  duplicates a fact the status bar already reports.
- **No user-authored widgets** — that's a plugin, and plugins are out. Data
  tables in notes are `.shq` (content the user chose); computed surfaces are
  small chrome the app offers.
- DB-read at render time, cached per note, updated on existing dirty beats; no
  new storage, no animation.
- **First widget: the rolls chip** — anchored to a `!r` line, hover shows that
  expression's roll history from the `rolls` table (consumer exists the day
  inline dice ship). Second candidate if the chip proves the pattern:
  current-section word count in the gutter. Expand only after the first lands.

### Hover link preview ✅ (roadmap **P6.2**; the embed and block cases wait on P4.0/P4.9)
- Wiki-link under cursor → floating card: first lines of the note + its graph
  neighborhood mini-render (reuses minimap machinery). Popup placement exists.

### Graph minimap ✅ (see graph-view-plan.md M5j)
- Corner mini-graph of the active note's depth-1 neighborhood, always visible in
  the editor; click-through to the full graph.
- **Constellation mode (ruled 2026-10-29):** while the graph tab has a
  constellation filter active, the minimap mirrors *that constellation* at
  miniature scale (MST star-lines included) instead of the note neighborhood —
  it mirrors where you're looking. Mode switches only on graph-tab focus so the
  two meanings never coexist; second camera + cached subgraph are already
  specced in M5j, so this is a mode, not machinery.

### Note pinning ✅ (demoted from a feature to a pane flag)
- Once tiling splits exist: `:pin` on a pane = dimmed reference mode — lower
  contrast, excluded from the jump chain, auto-closes when the last other pane
  closes. The pane system provides the layout; the *behavior* is the feature.

---

### The blank tab is a front door ✅ (built 2026-11-05)
- A pane with no note open shows a start block on the note's own surface: the vault's
  counts, its recent notes as clickable rows, and the doors as key-chips. It steps aside
  the moment the tab starts being a note (insert mode, or text in the buffer), so writing
  is never behind a menu.
- Recents are vault-wide — `recent` at the top level of `session.json` — so a second window
  on the same vault does not start cold. A note deleted from disk drops out of the list.
- An empty vault gets the same screen with one different line (`the vault holds no notes
  yet`) and a `new note` row: the first-run moment without a splash screen or a tour.
- Known soft edge: the link count in the vault line only appears once the graph index
  exists, because that is where links live. A count that flickers in is honest; a count
  that is guessed is not.

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

## Typed edges ✅ (general model, ruled 2026-10-29)

The graph's single untyped edge (wikilink) generalizes: **any frontmatter
property whose value is a wikilink is a typed edge** — the property name is
the edge kind. Same links-as-data trick as props-as-data and tasks-as-data.

- **DB:** one table `edges(src, kind, dst)` updated on save/index alongside
  `note_props`; body wikilinks stay kind `link`. Directed, one row per fact
  — never store both directions (sync-bug factory).
- **Zero built-in kinds.** The app ships no semantics for any kind;
  `allied:`/`rival:`/`member_of:` are vault conventions. This respects the
  lineage rejection: the app doesn't track bloodlines; if a kind is ever
  wanted, the user invents it and the graph draws it.
- **Graph view:** typed edges render as an edge layer per kind with display
  attributes (color, dashed, arrow) from a small registry
  (`.shale/edge-types.json` or `:edge-types`); filter UI grows an
  edge-kinds dimension next to orphans-on/off.
- **Direction & inverse labels (option b, ruled):** the registry optionally
  declares inverse display names (`member_of` ↔ `has member`); storage
  stays one directed row, labels are presentation. The **authoring side
  never sees inverses** — you write `allied: [[zhentarim]]`, full stop;
  inverses only appear on surfaces rendering from the other end (backlinks
  panel grouped by kind, hover labels, `.shq` columns). Inference-first:
  a tiny built-in list of symmetric words needs no inverse; everything else
  defaults to `← kind` with the arrow showing direction until the user
  curates a phrase. Works with zero config, prettier the more you curate.
- **Direction is visible without labels:** typed edges get a small
  arrowhead (or source-taper) so unlabeled kinds can't mislead.
- **Ghost nodes inherit:** `rival: [[the-hidden-one]]` pointing at a
  nonexistent note = a ghost with a typed edge in — an unresolved rival is
  a visible plot hole; do not "fix" this.
- **Search/`.shq`:** `rel:rival:[[x]]`, `has-rel:allied` ride the
  search-operator grammar; edges are just a table for `.shq`.
- **Layout springs ❄️:** typed edges influencing sim forces (allies attract,
  rivals repel) is deferred — it makes edge kinds load-bearing for layout.
  v1 typed edges are render/filter only.
- Cost: one table, one registry, one filter dimension — the indexer already
  runs on save and the graph already batches edge paths. **Corrected 2026-11-08:**
  this line used to claim "the parser already reads frontmatter". It does not — there
  was no frontmatter parser anywhere in `crates/`, and the `edges` table had no writer.
  The pass that reads frontmatter is **P4.3a** (`properties-plan.md`), and it is what
  finally fills `edges`.

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
  - *Scrubber-linked note view ✅ (ruled 2026-10-29):* while the scrubber is
    active, the open note shows its past self — **overlay primary**: past-only
    content painted dimmed where it was, current-only content at full alpha,
    positions locked (the P4 diff renderer aligns blocks; same visual grammar
    as the review overlay — one language for "text that isn't currently
    there"). Side-by-side is a later toggle. **Read-only while scrubbing** —
    editing a past state is not a thing; restoring is the time machine's
    explicit separate verb. Data is P1 snapshots + event log; each scrub step
    paints once, zero continuous animation.

---

## Obsidian parity audit (2026-10-28)

Swept Obsidian's built-ins + core plugins + top community plugins against this
document. Everything not listed here is either already covered (quick switcher,
palette, bookmarks, hover preview, rename propagation, word count, file
recovery→time machine, Bases/Dataview→`.shq`) or a deliberate out (sync,
mobile, ~~Canvas~~ **reopened 2026-11-03 — see the Canvas section below and
`canvas-plan.md`**, slide mode, themes). The gaps, with rulings:

### Properties: frontmatter as data ✅ (owner **P4.3a** — the indexing is `.shq`'s first commit; full design in `properties-plan.md`)
- The DB indexes YAML frontmatter: `props(note, key, ordinal, value, kind)` updated on
  save/index. Without this, `.shq` can't answer "all notes with
  `status: draft`" — the query-note feature is crippled without it.
- **Ruled 2026-11-08** (the schema decision this section always asked for, and the
  orphan the 2026-11-05 audit left behind): one KV table with `kind` recorded **at index
  time**, because SQL's `CAST('low' AS REAL)` is `0` and a prose value then sorts first
  without complaining; `ordinal` NULL = scalar, 0-based = one item of a list. Not typed
  columns, not per-key generated views, not a JSON blob — all three priced against the
  same query in `properties-plan.md` §2.
- **`kind` is per key, vault-wide**: inferred by majority across the vault's values for
  that key, overridden by `<vault>/.shale/props.toml`. Obsidian's own rule, and the same
  inferred-first/override-second shape as the edge-type registry. A key whose values
  disagree in kind is a typo worth *showing* in `:props`, never a value worth casting.
- **`tags` / `aliases` / `edges` are projections**; props holds every key as authored.
  **Body `#tags` keep working exactly as before** — the frontmatter feed is additive, and
  that is a regression test, not a comment.
- **Hand-rolled YAML-subset parser in core** (no `serde_yaml` — unmaintained; no
  `unsafe-libyaml` — conformance we do not want). Nested/unsupported YAML is indexed
  inert and **never rewritten**: that is the Obsidian-plugin compatibility and
  no-data-loss rule. JSON frontmatter is read, never rewritten (Obsidian reads JSON and
  saves YAML; it is a Hugo/static-site thing, absent from every note vault checked).
  TOML `+++` deferred behind one fence-detection chokepoint.
- **Editing surface:** in-place live preview of the block now; a vault-wide properties
  pane later for the kind census and key rename; a separate focus-island strip is
  **refused** — it is a second caret holder and a borrowed-surface interaction pass.
- Editor: **ruled 2026-11-08 — in-place live preview of the block**, not a strip above
  the text: each frontmatter line rendered as a key chip + typed value widget where it
  is, buffer stays plain text (same principle as the outliner), `:raw` still shows the
  YAML exactly as written. A separate focus-island strip is refused (a second caret
  holder, and a borrowed-surface interaction pass it would not earn). Key autocomplete
  from the DB's distinct-key set.
- Type inference is **per key, vault-wide** (§ above), and it is sugar for sorting and
  queries — **no schema enforcement**: frontmatter stays free-form, and a key that
  disagrees with its declared kind is reported, never refused.
- Rename a property key vault-wide → link-health machinery applies.
- Search + tag tree + graph filters can all consult props once indexed.

### Tasks as data ✅ (cheap DB win)
- **Still unowned (checked 2026-11-08).** The props table lands with P4.3a; this one
  needs a ruling on whether it rides that same commit or waits for P4.14's `todo:` /
  `done:` operators. It must not become an orphan twice. The state vocabulary question
  is live either way: real vaults use `- [/]` for in-progress, so `done` as a boolean is
  already known to be the wrong shape.
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
- **Legal note (checked 2026-10-29):** operator *syntax* is functional
  interface — short command words aren't copyrightable expression
  (idea/merger), the grammar predates Obsidian (Google's `site:`, Vim's
  `:vimgrep`), and no asserted patents cover it. What would be infringement
  is copying Obsidian's docs/UI wording — we write our own help text.
  `agent:`/`prop:` are our own operators anyway.

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

### Trash ✅ (user: "very important"; ruled vault-local, not OS trash; roadmap **P4.10**)
- Deletes (file tree, `:Gdelete`, `:Gmove` leftovers) move the file into
  **`.shale/trash/`**, preserving the vault-relative path in the filename
  (`projects--idea.md` style) plus a sidecar entry in the DB: original path,
  deleted-at, provenance (who deleted it — "what did the agent delete" is a
  query).
- Commands: `:trash` (list), `:trash restore <name>` (back to the original
  path, recreating folders), `:trash purge [name|all]`. Retention: 30 days
  (configurable), then auto-purge. `.shale` is gitignored, so trash never
  pollutes the vault's history.
- Why vault-local over the OS trash: full control (restore knows the exact
  original path even if the folder is gone), identical behavior on every OS,
  no `trash`-crate dependency (tech-stack rule), and it composes with the DB
  event log. Links to the deleted note degrade to ghosts as already specced;
  restoring clears the ghost.

### Workspaces ✅ (session restore is the base; named workspaces are sugar)
- Pane layout + open tabs + active pane + per-pane cursor saved as JSON in
  `.shale/workspaces/<name>.json`.
- **Auto-save the session continuously** (cheap) and restore on launch —
  this is the "we need to save it anyways" part, and it makes crash-recovery
  feel like nothing happened.
- `:ws save name` / `:ws load name` / `:ws list` on top. MCP can list/load
  (agent sets up a working context).

### Block references ✅ (hooks straight into transclusion + hover preview; roadmap
**P4.9**)
- `^abc123` anchor at the end of a paragraph/bullet; `![[note#^abc123]]`
  embeds it; `[[note#^abc123]]` links to it (jump = scroll to block).
- ID generation: command appends `^` + 6 random chars, lowercase base32
  (`a-z2-7`) — stable once written. Rationale (2026-10-29): content-hashed
  IDs die on every edit; sequential counters collide on merge/agent writes;
  word-slugs collide and go stale on reword (hover already shows the block's
  first words, so readability is covered). 6 chars = 16.7M values, birthday
  collision needs ~4k IDs for 1% risk, and the create command checks the DB
  regardless. Format-compatible with Obsidian's `^abc123` — notes authored
  there just work.
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
  Linux: `.desktop` entry + xdg-open; Windows: registry key.
- **Security rules (threat: any webpage can invoke the scheme):**
  1. *Vault allowlist:* only vaults present in `recent.json` are addressable;
     unknown vault name → refuse outright (no prompt — prompts train
     click-yes).
  2. *Containment:* the path is fully normalized (percent-decode, `..`,
     symlinks resolved) and must land inside the vault root; absolute paths
     outside it are rejected, not confirmed.
  3. *Read-only verbs:* the scheme may open a note and scroll to a line.
     No write/delete/command verbs, ever — a URI is untrusted input.
  4. *Confirm on unusual targets:* a path inside the vault but not yet in
     the DB index → confirm dialog showing the resolved path verbatim.
  5. The URI carries pointers only, never content — no temp-file surface.

### Table editing feel ❄️ (parked, user call 2026-10-29)
- `Tab` in a table cell → next cell (wraps to next row; at the last cell,
  creates a new row); `Shift-Tab` → previous. `table` keymap context;
  insert-mode Tab elsewhere untouched. Pairs with the `cit` text object.
- Parked as pure parity: normal-mode filling is already `cit` + `f|`
  (redundant with existing vim motions); the only gain is insert-mode
  chaining, which vim users won't reach for. Trivial to add later if
  table-filling turns out to be a real workflow.

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
- **`:inspect` ruled 2026-11-08**: it writes **one `![[morgoth#statblock]]` line
  directly under the table** — a read-only embed on P4.0's already-built machinery, so
  the statblock is plain text *below* the table (never inside a cell, never a hidden
  panel), undoable, and visible to `:raw`. Re-inspecting another combatant **rewrites
  that one line** instead of stacking, so a fight never accumulates five statblocks;
  `]t`/`[t` never touch it. It is the pre-P6.2 answer and retires when hover cards
  reach embeds.
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

### Combat log as a table ✅ (ruled 2026-11-08; free on the built table model)
- What real GM notes do today is prose with separators — `Thane deals 22 damage` under
  a `---` rule — which no query can read honestly. The ruling: **the log is a markdown
  table** (`| Who | What | Amount | Session |`), edited at table speed on the table
  model that already ships, and every "how did the fight go" question is a `.shq` over
  it: damage/heals/kills per PC per session, average damage by round, which combats
  someone went down in.
- **Prose is the failure mode, and it is why this is a ruling and not a convention**: a
  rollup parsed out of free text either lies quietly or refuses on the phrasing it did
  not expect. A table can answer nothing until it is filled; prose answers wrong forever.
- Same posture as the initiative tracker: the table *is* the state, no hidden store, no
  second source of truth, undoable text edits, provenance-logged like any other edit.
- Cost: no new subsystem at all — the table model is built, the queries wait on P4.3 /
  P4.3a like the rest of the cluster.

### Dice probability ✅ (ruled 2026-11-08; the other half of inline dice)
- `!p 2d8+4 vs AC16` renders **hit %, average damage, and crit range** next to the
  expression — the math GMs currently derive by hand in prose (a real vault contains
  `Chance of rolling above k where d is #dice = 1 - ((k - 1) / 20)^d` and a table of
  expected DPR written out longhand).
- Exact distribution by **convolution over the dice grammar already specced for `!r`**
  (`NdN[+/-m][khN|klN]`), pure Rust in `shale-core`, no crate. Keep-highest/lowest needs
  order statistics rather than a naive convolution, and advantage is the common case, so
  it is the part the tests must pin.
- Unlike `!r` this **writes nothing back**: a probability is a property of the
  expression, not an event, so there is no result to annotate and no rolls-table row.
  It is a computed surface (P4.15's policy: app-offered, region-anchored), not a state.
- Scope guard: it informs encounter building; it does not build the encounter. The
  budget check (Hard vs Deadly) stays deferred — see below.

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
- **Confirmed as not-a-feature 2026-11-08**: "the GM types shorthand, the agent expands
  it into prose" is **not** a Shale mode and involves **no pinging the agent from the
  app** — the agent is already inside Shale through the MCP server, and its writes
  already carry `agent:` provenance through the review strip. What Shale owes is only
  the *material* (template note + `.shq` aggregates + rolls table). Do not open a
  session-capture surface; there is nothing for it to be.

### Image pins ❌ (retracted 2026-11-08 — superseded by Canvas, not rejected on merit)
- The 2026-10-29 tabled design (pins as normalized x/y + link + label on an image note,
  markers drawn by the renderer, drag reusing graph drag-pin) is **cut**: the Canvas
  ruling (`canvas-plan.md`) hosts a dungeon or city map better, because a canvas node is
  a **live view of the real note** rather than a coordinate pointing at one, and
  `:canvas from-query` already gives the GM a board with no agent involved.
- Do not re-pitch pins as "the lightweight version": the coordinate layer is exactly
  what Canvas decided not to have (agents never emit coordinates; geometry is the
  layout's job). The VTT guard still stands either way — reference and annotation only,
  no tokens, no grid, no movement.

### Spotlight ledger ✅
- The DB knows session-note text; PC notes are wikilinked there. Count
  PC-name mentions (wikilinks + `aliases:` prop) per session → spotlight
  time per player as a bar chart in the GM screen. "Is someone getting
  shafted?" becomes a query instead of a vibe.
- Data source is the existing backlinks/mentions index; the only new part
  is a `mentions(note, session, count)` rollup updated on save, and a
  chart render in the GM screen (custom renderer draws bars).
- Murder-hobo variant for free: same rollup over `tag:deed` /
  `tag:wronged` conventions → "which factions have we wronged" is a
  backlink walk. Convention only, no subsystem.

### Print surfaces ✅ (export-to-PDF; the renderer's second sink)
- General path: the custom renderer already lays out text — PDF export is
  the same layout written to a different sink (via a small pure-Rust PDF
  writer; no webview, no headless browser).
- One export command with presets: plain note, handout (secret blocks
  stripped — rides the secret-block rule), statblock card, broadsheet.
- Print CSS analog: note-type render profiles select layout (columns,
  drop caps, rules) — the same callout/type machinery, not a stylesheet.
  **Profile registry (ruled 2026-10-29):** three named profiles keyed by note
  type/callout — `statblock` (two-column card, rules, accent header — looks
  like a physical card), `handout` (secret-stripped, serif-forward),
  `broadsheet` (columns, drop caps, masthead). Screen rendering and print use
  the same profiles: what you see is literally what exports.
- Exports are provenance-logged (what was printed, when, which preset) —
  "what did the GM hand out" stays a query.

### Broadsheet ✅ (rides print surfaces + recap generator)
- `broadsheet` note type: the print-surface layout profile typesets it as
  an in-world newspaper (columns, drop caps, rules, date masthead).
- Content source: the session-recap idea (template + `.shq` aggregates +
  rolls table) rewritten in tavern-gossip voice — the agent drafts, the GM
  edits, provenance marks the draft.
- The most shareable artifact of the toolkit: print it or export PDF and
  post it after every session.

### Graph lenses (split ruling, 2026-10-29)
- **Session lens** ❌ — dropped: the planned connected-node / date graph
  filters already cover it; no separate lens.
- **Hook→payoff arcs** ✅ — `#hook` edges pointing at `#payoff` notes;
  unresolved arcs render dashed into nothing. The point: the graph shows
  holes — to-do and unfinished threads are visible *in the drawing*, so
  "what did I leave dangling" needs no query.
- **Region clustering** ✅ — `region:` property feeds the community
  layout; arcs become visible clusters; a too-small arc or two clusters
  that never interact is a missed connection you can *see*.
- **Party graph** ❌ — dropped: PC↔NPC entanglement is already visible
  via standard wikilinks + backlinks panel; no dedicated lens.
- **Faction tension** 🔭 (still awaiting ruling) — `allied:`/`rival:`
  frontmatter edges render as a colored edge layer — conflict geometry
  at a glance.
- Kept principle: lenses are graph filters + edge layers over existing
  index data; no new storage.

### NPC knowledge graph ❄️ (parked, user call 2026-10-29)
- Fact notes + `knows:` frontmatter → graph-reachability as an NPC's mind;
  `:talk <npc>` scoped persona (leak-proof by vault slice), alibi
  timelines, `:gossip` propagation with `twist:` degradation.
- Parked: too far from the app's center of gravity right now (it pulls
  toward world-simulation). Lineage/influence edge tracking was rejected
  outright (user call 2026-10-29) — the graph tracks relationships people
  and things have, not bloodline/influence chains. Revisit only if the
  vault's shape already grows fact notes.

### Puzzle notes ❌ (rejected, user call 2026-10-29)
- Input-locked secret blocks (answer/dial/order/key locks, chain locks) —
  rejected: requires players using the app/vault, which is out of scope
  (see vault slices rejection). The lock *predicates* are sound ideas but
  have no host without player access; do not re-pitch without one.

### Per-player vault slices ❌ (rejected, user call 2026-10-29)
- Scoped player views of the GM vault — rejected: turns Shale into a
  full table app; not the route. This rejection also sinks puzzle notes
  and any player-facing surface. GM-side only.

### World clock ❌ / Overland travel graph ❌ (rejected, user call 2026-10-29)
- World clock (auto-advancing faction plans): rejected — world simulation
  is not the product. Overland travel graph: rejected — user doesn't
  track travel time; the graph stays a *relationship* graph, not a
  distance graph.

### Manual tables and lookups — PARENT RULE ✅ (binding, ruled 2026-11-08)
- **A table or a lookup stays something the GM maintains and reads; nothing advances,
  infers, or auto-populates it.** Query surfaces may *compute from* what is written
  (`.shq`, the spotlight rollup, the kind census) — they must not *write into* it, and
  no feature may keep its own clock.
- This sinks, permanently: the **NPC schedule grid** as anything but a hand-kept table
  plus one lookup (ruled 2026-11-08: `Morning/Afternoon/Evening/Night` stays manual —
  the app may query it, never advance it); **write-back ledgers** that push session
  numbers into a PC note's props (the party ledger is a saved query over the session
  tables instead — see "Party ledger as a query" below); and anything that re-pitches
  the world clock as "just a reminder".
- The tell to watch for in a pitch: a feature whose value comes from *not having to
  update the table itself*. That is this rule's case, and it is refused here rather
  than at review.

### Consumed-line tracking ❌ (rejected, user call 2026-11-08)
- `used:` written back onto a quote/name/clothes line at pick time, so `:pick` never
  repeats a line at the table. Rejected as **too niche** to spend a subsystem and a
  write-back rule on, even though it rides the dice write-back precedent and answers a
  "things to track" item in a real vault. Do not re-pitch it as a free ride; if it ever
  returns it is a convention over block refs (P4.9), not a feature.

### `:pick` over a list in the note ❄️ (parked, user call 2026-11-08)
- Picking from a list *inside* one note (a numbered name list, a clothes-by-district
  list) rather than from a tagged collection of notes. Parked, not rejected — the
  argument that parked "random tables" (note-collections beat hand-written tables) does
  **not** fully hold, because real vaults keep these lists inside notes; the door stays
  open on the `!r` syntax and the `rolls` table. Needs P7.2 inline dice first.

### Encounter budget check ❄️ (deferred, user call 2026-11-08)
- "Is this fight Hard or Deadly" from monster CR × count against party level × count.
  The arithmetic is trivial; the cost is that the thresholds are **edition data**, so the
  ruling owed before it can be built is whether the table is vault data (matching the
  zero-built-in-vocabulary principle) or a built-in. Deferred with that question attached
  — do not build it on an assumed edition.

### Free rides (no new subsystems — note the conventions, don't build)
- Statblocks: `type: monster` + `cr/hp/ac` frontmatter → props index + `.shq`
  queries; a `statblock` callout type for pretty two-column rendering later.
  **Ruled 2026-11-08 ✅**: the `statblock` callout kind renders on screen as soon as
  callout kinds are wired (built today); the two-column **print card** waits on P6.5's
  render-profile registry, and screen and print use the same profile — what you see is
  what exports. The query half is gated on P4.3a, not free today.
- Quests = tasks table (checkboxes + `due:`); campaign/session notes =
  daily-notes pattern with a `campaign:` property (`:session 12`).
- **Session spine ruled 2026-11-08 ✅** (a vocabulary ruling, not a build): one note per
  session carrying `campaign:` and `session:` props, `:session 12` opening it on the
  daily-notes pattern. Then "the story so far" — today a hand-maintained prose section
  in a real vault — is a saved `.shq` over the session notes, and the party ledger is
  the same query read from the other end. Nothing writes into the PC notes.

### Party ledger as a query ✅ (ruled 2026-11-08 — the write-back version refused)
- Per-PC campaign bookkeeping (`Deaths`, `Damage Taken`, `First Downs`) exists in real
  vaults as a folder of near-empty notes, because maintaining those fields by hand is
  misery and nothing ever fills them. The ruling is **not** a `:session-end` command
  that populates props: that duplicates state, invents a sync story, and turns the vault
  into a spreadsheet database.
- The numbers are already decided once, in the session note's combat table. The ledger is
  a **saved `.shq` over those tables**, read per PC — no fields to leave empty, no
  second source of truth, and it obeys the manual-tables rule above.
- Block refs (`^id`) for exact NPC quotes in session notes; relationship
  graph (NPC↔faction↔location) is just the existing graph + tag coloring.

---

## Canvas ✅ (reopened 2026-11-03; full plan in `canvas-plan.md`)

The parity audit's "deliberate out" was reversed under competitive pressure
(ZenNotes ships a canvas; Obsidian's jsoncanvas is an open format). The
reopen ruling and all six open questions were ruled ✅ in one round. The
concept: a canvas is a **second projection of the vault** — same node model
as the graph, pinned layout, live buffer-backed content, typed edges that
write real vault facts. Binding rulings:

- **Two edge kinds**: vault edges backed by real frontmatter typed links
  (delete either side → the other ghosts; they live in graph/DB because the
  link exists, not because it's drawn) vs local decoration edges (never
  enter graph/DB).
- **`.canvas` is a plain editable buffer** — vim-editable JSON; the 2D view
  and the outline view (`:canvas outline`) are projections of it; buffer
  undo, time machine, and conflict policy all fall out for free. Broken
  JSON: last-good render + bar warning (broken-theme posture). Unknown keys
  preserved (Obsidian interop; adopt the jsoncanvas spec).
- **Canvas files are vault citizens**: links out of `.canvas` are indexed
  (backlinks get a `canvas` badge), and **rename propagation must parse
  `.canvas` files** — C1 day one, non-negotiable.
- **A node is a view, never a copy**: note nodes render from the live
  buffer; the inline editor attaches to the real buffer (shared cursor,
  undo, dirty dot).
- **Topology in, geometry out**: MCP canvas tools are semantic verbs
  (`add_node(near:)`, `stack_below`, `group`, `link(a,b,type)`), quadtree
  occupancy makes overlap unrepresentable, auto-relax (collision-only sim
  ticks) moves **only agent-added nodes** — user-pinned never move. Agents
  never emit coordinates. Accepted batches are one undo unit through the
  P2.5 review strip.
- **Ghost node accept creates the note** (consistent with ghost links).
- Scope guards (confirmed): no freehand pen/sketching, no webview/
  Excalidraw, no stored coordinates for pure graph views, no agent
  coordinate access ever. GM-side board posture unchanged.
- Free rides flagged: `:canvas from-query` (GM board without any agent —
  the agent is the natural-language wrapper around this command), canvas-
  as-saved-selection graph filter, PNG/PDF export, board template gallery.

Milestones: **C1** viewer + jsoncanvas interop + link indexing/rename
(→ Phase 3 addendum, after tabs + graph milestones — mostly graph-renderer
reuse), **C2** editing (Phase 5), **C3** typed edges + query nodes + MCP
tools (Phase 6, rides `.shq` + typed edges + block anchors).

## Idle cost (measured 2026-11-05)

The idle baseline and what was cut from it are in `PROGRESS.md` ("What an idle Shale costs" and
the two follow-ups). Two things came out of the measurement and are **not** scheduled:

### Drop `harper-ls` after a long idle ❄️ (parked, user call 2026-11-05)
The spell server is 141 MiB and 19 threads sitting at 0% CPU — about five times the whole app —
and it starts even when the window opens on the graph tab. The fix is lazy: start on the first
text-buffer paint, stop after some minutes with no text buffer focused, restart on demand. The
hard part is honest: a restart costs a spawn plus an LSP handshake (~1s), which shows up as a
missing underline for a moment, and stopping has to flush the diagnostics in flight. The user's
ruling is "harper is fine for now" — memory is not the binding constraint today.

### Skip the D-Bus connection when the vault names a palette ❄️ (parked, 2026-11-05)
A `zbus::Connectio` thread is alive in every window, even when `.shale/config.toml` names a
palette explicitly, because the desktop-following path (`theme = ""`) asks the portal for its
dark-or-light setting. When a palette is named, that question is never asked, so the connection
— and the thread — should not exist. Small, clean, and worth doing when something else touches
theme loading.

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
5. ~~Agent edit review UX~~ ✅ **RESOLVED 2026-11-01** — full design pass in
   the "Agent edit review" section: P2.5a/P2.5b split, MCP-server-written TOML
   files, bottom strip, two-layer keys, `similar`-based diff primitive pulled
   forward from P4.2, apply-or-stale conflicts + `pending_edits` agent tool.
6. ~~Roadmap sequencing~~ ✅ **RESOLVED 2026-11-01 (route 3):** P2.5a next,
   then P3.1 Tabs → graph tab to unblock the graph track; P2.6/P2.7/P2.8
   deferred to the Phase 3 addendum (see roadmap "Sequencing notes").
7. **Graduate ✅ items into PROJECT.md** — pending action, not a decision.

## Roadmap audit (2026-11-05)

Every ✅ design in this file was checked against `roadmap.md`'s items and against
`crates/`. **Twenty-eight had no owner.** They are scheduled now; nothing below is a new
idea, only a phase number attached to a ruling that already existed:

| brainstorm section | now |
|---|---|
| Editable transclusion ✅ | **P4.0** (a: read-only embeds, b: editable in place) |
| Block references ✅ | **P4.9** |
| Trash ✅ ("very important") | **P4.10** |
| Daily notes ✅ | **P4.11** |
| Attachment capture ✅ | **P4.12** |
| Outline pane ✅ | **P4.13** |
| Search operators ✅ | **P4.14** |
| Inline computed surfaces ✅ | **P4.15** |
| Graph lenses ✅, tag-node coloring ✅ | **P4.16** (colour rides P4.4) |
| `shale://` URI scheme ✅ | **P6.11** |
| Random note ✅ | **P6.12** |
| Copy/paste/duplicate in the tree 🔭 | **P6.13** — still needs a ruling (roadmap open decision 8) |
| Initiative tracker ✅ | **P7.1** |
| Inline dice ✅ | **P7.2** |
| Random generators ✅ | **P7.3** |
| GM screen ✅ | **P7.4** |
| Spotlight ledger ✅ | **P7.5** |
| Secret blocks ✅ | **P7.6** |
| Agent GM assistant 🔭 | **P7.7** |

Verified as already scheduled elsewhere, so not added: MCP pulses (graph-view-plan
M5b), graph minimap (P3.4's M-list), drag-pin (graph-view-plan), properties/frontmatter
(P1.2 schema), macros (P6.3), focus/typewriter (P0.8/P0.9), marks (P4.5), tag tree
(P4.4), time machine (P4.1), link health (P4.6), query notes (P4.3), print surfaces and
broadsheet (P6.5), outliner (P6.1), hover preview (P6.2), note pinning (`:pin`, P3.2),
workspaces (P1.8 session slots — named workspaces are still sugar).

Verified as **already built**, so they were gaps in the audit's keyword pass rather
than gaps in the product: unresolved link → create note (`gD`), the vault switcher
(`:vault`), the blank tab's start block, `/` search (P0.5, 2026-10-28 — the roadmap's
"Parked threads" list still named it as parked; fixed).

New rule out of this, written into roadmap "Sequencing notes": **a ✅ here with no
roadmap pointer in the same commit is a defect, not a backlog entry.**
