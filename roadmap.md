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
extraction/resolution/backlinks, `gd`/`gD` and ctrl+click navigation (`gx` still
works), quick-open, command palette, `:` command line, vault search, fuzzy matcher,
theming (20 palettes + `.shale/themes/*.toml`, switchable at runtime with `:theme`),
file watching, spellcheck/grammar via harper-ls with the popup + code actions, image
embeds, MCP stdio server (list/read/write/search/backlinks), headless gpui test
harness. Refreshed 2026-11-05 to add what shipped that week: the graph's local
neighbourhood and projected 3D mode, the start block a blank tab paints, `:w` naming
an unnamed note after its first line, and tab stealing between panes.

**Where the phases stand** (refreshed 2026-11-06, replacing the 2026-11-05
list): **Phases 0, 1 and 2 are closed, including P2.5b** — the inline review
overlay landed 2026-11-05 (with the hunk's rule moved into the gutter on 2026-11-06,
`b6b24b1`), so nothing is left in Phase 2 but the P2.6/7/8 items deferred to the
Phase 3 addendum. **Phase 4 has started.**
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
the original Goals list: Database tables/Obsidian-bases (P4.3 `.shq` — its first
commit, **P4.3a frontmatter indexing**, landed 2026-11-08, so the props table the whole
query story was priced against exists, and the query notes themselves have landed through
M2), PDF
export, Templates, tree-sitter code highlighting.
**What is being built next (refreshed 2026-11-10): P4.3a and P4.3 are both closed — the
`.shq` item ran M1 through M8 in two days (format and runner, the `:query` door, the folded
note with its answer drawn under it, `![[query-name]]` and the link it asserts, relative
queries, keep-a-search, the agent door, and the graph learning what a query is). What that
unblocks is the sequence the dependency order already wrote. **P4.3b is closed too** (2026-11-10:
schema v4's `headings` table, one heading scanner for the whole app, and `:sections` as its
reader — see its item below). Next is **P4.9** block refs, then the computed surfaces (P4.15)
and the consumers that were waiting — the graph lens (P4.16) and the GM screen (P7.4).**
P4.0 closed on 2026-11-08 (the hover card was its last owed item; `![[query-name]]` waits
on P4.3 and block refs on P4.9). **P4.3a's indexing half is closed:** M1 (the parser, the
`props` table, `:props`) and M2 (the projections — `tags`, `aliases`, and `edges` finally
with a writer) landed 2026-11-08, M3's kind census the same day, and the deferred
`aliases`-as-link-target ruling landed 2026-11-09 as its own commit. **P4.14 landed the
`prop:`/`tag:`/`path:` operators and the "why did this match" evidence early**, and its
grammar is one parser (`core::query`) that says nothing about SQL — so P4.3's `.shq`
bodies will read queries the same way, and P4.3's first commit is already behind it.
**M4 — live preview of the block — closed 2026-11-09**, and not as the plan wrote it:
the block's fences and quotes stay visible (they are the data), and the block is shown
by its **shape** instead — a background band and a hairline boundary in the new
`frontmatter_block_bg` role. **M5 — the vault-wide properties pane — closed 2026-11-09**:
`:keys` lists the vocabulary with counts and kind conflicts, and `:rename-key` renames a
key across every note that writes it. **P4.3a is closed.** What it leaves behind for
P4.3: `core::query` is the one grammar, `props` is the table a `.shq` body will read, and
the key rename is the migration path a saved query's vocabulary can lean on.
**P4.3 has started: M1 (the `.shq` file format and the read-only runner), M2 (the door:
`:query`, the picker that lists saved queries and draws their answers) and M3 (the note
itself: the program folded, its answer drawn underneath, the marker row carrying what the
file computed) landed 2026-11-10.** A `.shq` opened in a tab now reads as a question and
its answer rather than as SQL: `▸ Creature ratings · 2 rows` with the table hanging under
it, a cell spelled as a vault path opening that note, a refusal opening the program so the
reason is never a blank page. **M4 — `![[query-name]]` — landed the same day**, in two
steps: the embed draws the answer's table under the row that asked for it, and then M4b
filed the link the embed asserts, so the index, the link colour, `gd` and the answer's own
title row all agree the query is a file (ruling 33: four surfaces, one resolution order, or
the colour lies). Renaming a `.shq` now keeps its extension, stays out of the `notes` table,
and re-files the embeds that name it. **M5 — relative queries — landed the same day**:
`relative = "current"` in the declaration and `:current` in the body are checked against
each other and against the surface running the query, so the note holding the embed is the
anchor, a query viewed in its own file has none (a `.shq` is not a note), and a host-less
surface refuses in one sentence written once (rulings 36-38). **M6 — keep a search — landed
the same day**: the operators in the search box compile into the statement a `.shq` holds,
from the one builder the live search also reads, so saving a search and writing the file out
are the same act and the file answers what the search answered (ruling 40). The offer is the
last row of an operators-only search, `:save-query <search>` is the keyboard route, a name is
one name, and an existing query is never overwritten. **M7 — the agent door — landed the
same day**: `shale-mcp` lists, reads and runs saved queries on a connection that cannot
write, and the sentences a query answers with are written once, in core, for both doors
(rulings 41-44). **M8 — the graph learns what a query is — closed the item the same day**,
which is ruling 35's gap paid: `build` and `set_note` take a third entry list (the vault's
`.shq` files, the same list the index, the link colour and `gd` already read), a query node
is its own `NodeKind` in its own colour (`graph_query`, named in all nine built-in palettes
and audited against the note colour like every other kind), the panel counts queries apart
from notes, and `enter` opens the program the way it opens a note. Two things the picture
had to be told: a query's **program asserts no links** — `[[` inside SQL is SQL, so a `.shq`
has edges only from the notes that embed it — and a note embedding a **deleted** `.shq`
degrades to a ghost keyed by the old path, because that is what the editor colours the same
embed as. A query nobody has embedded is a node with no edges: an orphan the `orphans` chip
can hide, not an absence. **P4.3 is closed.** The
plan, an edge ledger (EDGE-1…24) and the rulings are in `queries-plan.md`.

**A six-item input round closed on 2026-11-10**, between M6 and M7: the arrow keys and Tab
answer in the note (arrows are actions, not vim motions, so they work with an operator
half-typed; Tab writes a literal tab and shift-Tab takes one level back), the mouse's side
buttons walk the jump list, escape in the graph puts down the chosen node instead of leaving
the tab, the default palette is
Shale's own dark with the desktop asked for dark or light (`theme = ""`), and every text
field — prompt, picker, graph filter — has a selection and the clipboard. Two things that
round taught the repo: a mouse-down takes keyboard focus for **any** button, so a root
`capture_any_mouse_down` has to stop propagation even when it refuses the click (EDGE-22);
and a config default that asks the desktop breaks every headless test, because the portal
client runs on threads the gpui test scheduler refuses. The record is `input-plan.md`.

The same day finished the graph's tab-ness. `note.focus-editor` still carried a branch that
jumped a graph tab back to the last note tab — a shortcut from before the app had panes and
tab pages, and the reason `ctrl-w l` read as "the graph closed". It is gone: the keyboard goes
to the tab on screen, leaving a graph tab is tab paging (`g t`/`g T`, `:tabnext`) as it is for
every tab, and `window.right`'s palette label now says "Focus the pane to the right" instead of
promising a note. Node selection gained an arrow form beside `hjkl`: `ctrl`+`shift`+arrow, with
the bare arrows left as the camera and `shift` still moving it three times as far. In the
*fields* — prompt, picker, graph filter — the same modifier pair does what it does everywhere
text is edited: `ctrl`+`shift`+arrow selects a word and `ctrl`+arrow moves by one, on the same
boundary `ctrl-backspace` already deletes from (`word_forward`, beside `word_back` in core).

**P4.0, transclusion — the editable half (closed 2026-11-08).** P4.0a
(read-only embeds) shipped on 2026-11-06, and **P4.0b — editing inside an embed — is
built through M4**: the buffer registry that made it possible (P4.17), entering with
`ctrl-]` or a click, the seam, per-note dirty state and the save story, links and the
checker following the caret, live preview inside the block. What is left there is small
and known: the hover card (click-to-open on a badge landed 2026-11-08). Soft-wrap for the
block's rows and folding inside a block are done (2026-11-08): a block being edited folds
with `za`, its disclosure is clickable, and the rows behind a closed fold leave both the
paint and the display model the caret's motions read. P3.2's move/exchange/rotate and
`:pin` remain open and are the cheap alternative if a session wants a small, low-risk
win.

**Plan audit (2026-11-05).** Every ruled design in `brainstorm.md` was checked against
the roadmap's items and the code. The finding: **the roadmap had been losing features
that were fully designed and explicitly agreed.** Twenty-eight features had no item
owning them; the significant ones:

- **Editable transclusion** — named in `PROJECT.md` as a pillar of the custom-renderer
  advantage, ruled in full including the binding nesting cap, and *no phase owned it*.
  P4.3, P5 and canvas C3 all lean on that resolver. Now **P4.0** (a: read-only embeds,
  b: editable in place).
- **Block references** — ruled, gates canvas C3 and hover preview. Now **P4.9**.
- **Trash** ("very important", ruled vault-local), **daily notes**, **attachment
  capture**, **outline pane**, **search operators**, **inline computed surfaces**,
  **graph lenses** — ruled, unowned. Now **P4.10–P4.16**.
- **The whole GM toolkit** (initiative tracker, inline dice, random generators, GM
  screen, spotlight ledger, secret blocks, agent GM assistant) — seven ruled features,
  zero roadmap lines. Now **Phase 7**.
- **`shale://` URI scheme**, **random note**, **tree copy/paste/duplicate** — now
  **P6.11–P6.13**.

Nothing was invented by this audit: every item above already has a ruled design in
`brainstorm.md`. What changed is that they are now scheduled somewhere, so "planned"
means "has an owner and exit criteria" rather than "someone thought about it".

**Parked threads** (PROJECT.md, the other agent's list): scrolling
popup panel, scrolling picker list, hover-capability check, live grammar fixes,
`K` with count, horizontal scrolling, `bench/` work. Five that were on this list
are now done in Phase 0: **`/` search** (P0.5, 2026-10-28), **autosave**,
**persisting the theme switch**, **config → `<vault>/.shale/`**, and **module splits**
(nothing is over 1000 lines anymore).

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
link resolver + renderer (exist) ──► P4.0 transclusion ──┬─► ![[query]] embeds (P4.3)
                                                        ├─► hover preview (P6.2)
                                                        └─► P4.9 block refs ──► canvas C3
props index (P4.3a) ──► .shq columns, `prop:` operator (P4.14), typed edges (fills
                        `edges`), tag tree + graph hues from frontmatter, GM toolkit,
                        sidebar tags/properties (P4.13)
headings index (P4.3b, AFTER P4.3a) ──► section-grouped chips (P7.10), sidebar outline
                        (P4.13) from the index, block cards (C3), `#heading` anchors
relative queries (P4.3) ──► scene cards, cue/counter lists, per-session prep,
                        graph lenses (P4.16: a lens IS a saved .shq)
```

The last edge was missing until the 2026-11-05 plan audit: three phases depend on a
transclusion resolver that no phase owned and no code had.

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
    **Then vs now (2026-11-08):** that deferral pointed at P3.3, which shipped as *Graph
    M1a* and never parsed frontmatter — so the pointer dangled and `edges` stayed empty
    (no writer anywhere in `crates/`). Frontmatter indexing is now owned by **P4.3a**,
    and it is the pass that fills `edges`. Recorded here so the next implementing agent
    does not read "until P3.3" as still true.
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
  the one table (28 rows then, 54 now — `docs/commands.md` is the live list), and
  `workspace/commands.rs` is gone. Each row carries a
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
  - **P2.5b — inline overlay (✅ 2026-11-05):** the same hunks painted in the
    note they belong to. A removal is **not** phantom — the note's own row is
    struck through in `diff_delete` with the hunk's bar down its left edge,
    because the line is already in the buffer. Only additions are phantom rows:
    they hang off the display line they belong beside, so the display-line index
    space stays exactly the buffer's and `j`/`k`, search, folds, the ruler and
    the caret never see them. `za` over a line under review folds that hunk's
    additions to a `−N +M` count; the strip's `za` still collapses the *list*
    row, and the two states are deliberately separate. A hunk that no longer
    fits has no placement, so it is not painted at all — the strip keeps
    listing it badged stale. Strip authoritative, inline opportunistic, as
    ruled. Two-layer keys: bare `y`/`x`/`za` over a line under review belong to
    the proposal; a count, a pending operator, or visual mode gives them back to
    vim. The note's `y` names the hunk under its cursor and moves the strip's
    marker there; the strip's `y` names the marked one.
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
into the Phase 3 addendum. **P2.5a landed 2026-11-01; P2.5b landed 2026-11-05,
so Phase 2 is closed on its own terms** (only the P2.6/7/8 addendum remains).

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
- **P3.7 Named workspaces** (brainstorm ✅ parity item, **owner assigned 2026-11-08** —
  it was ruled and unowned). It is sugar over machinery that already exists: the session
  file is one `Session` per window slot and the pane tree's leaves *are* the panes list,
  so a workspace is **a named set of window sessions** written to
  `.shale/workspaces/<name>.json`, with `:ws save name` / `:ws load name` / `:ws list`,
  and MCP `ws.list` / `ws.load` so an agent can set up a working context.
  **Contents ruled:** exactly what `session.json` already stores per window — the pane
  tree, the tabs, and the view decisions that already ride a session (graph camera, mode,
  filter). Nothing new is stored, so there is nothing to refactor later; the GM cockpit
  (brainstorm "Session-linked chips") is one named workspace and is the reason this is
  pulled into Phase 3 rather than left as Phase 6 polish.

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

- **P4.0 Transclusion resolver** (brainstorm ✅ "Editable transclusion"). **Added by
  the 2026-11-05 plan audit**: the design was ruled in full, `PROJECT.md` names
  editable transclusion as one of the two pillars of "custom renderer as advantage,
  not liability", and P4.3/P5/P6.10 all lean on a resolver that no phase owned and no
  code has. Split by dependency:
  - **P4.0a read-only embeds — mostly landed (2026-11-06).** What ships: the
    resolver (`crates/core/src/transclude.rs` — parse, resolve through the vault's
    link table, cut the anchored section, expand nested embeds, depth cap, chain),
    the body cache (`crates/app/src/editor/embeds.rs` — references re-scanned from
    the buffer every frame, bodies cached and dropped when the vault or the host
    changes), and the paint (`PhantomKind::Embed` — the body hangs under its
    `![[…]]` row with a rule in the gutter, stepped right per nesting level, styled
    by the source note's markdown). Badges cover the cap and the chain. 14 core + 9
    app tests.
    **Closed the same day (2026-11-06): marker hiding and heading size.** An embed's
    rows now go through the same `mdlayout::hidden_ranges` → `visible_pieces` →
    `PieceMap` path the note's own rows do (`embeds::paint_body`), so their `#`, `**`
    and backticks are hidden and their spans are re-based onto what is actually painted
    — plus the curly-quote substitutions, and the heading **size** for the row's level,
    which was the same inconsistency one level down: a hidden `#` at body size still
    reads wrong. The scan runs over the whole body at once and is cut per row, because a
    fence that opens on one row and closes on another is invisible to a per-row scan.
    An embed's rows hid their markers unconditionally, which was the right rule while
    nothing could put a cursor on one; since P4.0b landed, the block being edited follows
    the live-preview rule — the cursor's row and any selected row keep theirs, and the rest
    of the block reads rendered.
    **Closed since:** a hit-test from a phantom row back to the body row under it exists
    (`PhantomHit`, `transclude::Origin`), and `gd` on a wiki-link *inside* the block being
    edited follows that link, because the link list follows the caret. `gd` on the host's
    `![[…]]` line opens the embedded note in the tab, which is what a link does; `ctrl-]`
    is the door that edits in place.
    **What is left in this item:**
    1. **Click-to-open on a read-only embed — done 2026-11-08.** A badge (depth cap, a
       note already on the chain, or a section that stopped resolving) carries the link it
       stands for, records a clickable band, and a click on it emits the same
       `FollowLink` that `gd` on the `![[…]]` row does — one route, so a click and a
       keystroke cannot disagree about which note a written link means.
    1b. **Hover preview card — done 2026-11-08** (`crates/app/src/card.rs`). The last
       thing P4.0 owed. A link's first lines, through the same resolver an embed uses.
    2. `![[query-name]]` for `.shq` results waits on P4.3; block refs (`^abc`) on P4.9.
    Original framing: `![[note]]` and
    `![[note#heading]]` resolve through the existing link resolver and paint the
    target's lines inside the host; the host's buffer is untouched, the embed is a
    view. **It rides P2.5b's phantom rows** (`crates/app/src/editor/overlay.rs`,
    `crates/app/src/line/phantom.rs`): content painted inside the note that is not in
    the buffer, skipped by `j`/`k`, search, folds and the ruler, re-anchored against
    live text at paint time rather than cached. That is the hard half of an embed, and
    it is built and tested — an embed is a phantom block whose text comes from another
    file instead of from a queued hunk. What is genuinely new here is the resolver
    call at paint time, the nesting cap, and the source's markdown styling.
    The nesting cap is **binding**: 1 editable + 2 read-only = 3 levels deep,
    deeper embeds render as a name badge
    (clickable, no content). This half alone
    unblocks `![[query-name]]` in P4.3 and the embed case of P6.2.
  - **P4.0b editable in place — built through M4 (2026-11-07).** What landed: **M0/M0b**
    (the per-view trio — segments, folds, `read_only` — lifted into `core::buffer::ViewSlot`),
    **P4.17 steps 1-2** (the per-window note registry, `Rc<RefCell<Buffer>>` shared by tabs and
    embeds, `dirty` on the registry entry, `read_only` per view), **M2** (`transclude::Origin`,
    the `Sources` trait, the anchored section cut as row/line pairs, `PhantomHit`), **M3a/M3b**
    (enter with `ctrl-]` or a click, exit with `ctrl-o`; the caret's buffer split from the tab's;
    the block's own hit space; host-owned scrolling), **M4** (the save story: `touched`,
    transitive dirty dot, `:w` writing host and embed, Q1's materialised tab, Q3/Q4/Q5), and
    then the parity work the caret made necessary: the gutter dirty dot on a new extensible
    `GutterMark` path, links and the checker following the caret, a panel anchored inside a
    block that no longer strands the keyboard (`LineRef`), live preview inside the block, and
    raw mode stepping out of one, and soft-wrap for the block's rows (a wrapped row is
    several elements over one buffer row, each with a slice of the row's `PieceMap`, so a
    click on the tail still answers with the byte under it). **Owed:** the fold disclosure
    glyph on the `GutterMark` path, and the hover preview card (click-to-open landed
    2026-11-08: a badge now carries its link and opens the note it names).
    **Interaction design pass done 2026-11-06** — rulings,
    a 36-case edge ledger and milestones M0-M4 are in `transclusion-plan.md`; **all eight
    open questions were ruled the same day** (Q1-Q8, §7). Behavior was already ruled (undo buffer-local
    with a seam indicator; per-note dirty state; save/autosave covering host + embeds;
    closing a host with a dirty embed warns; never *silently* saving the embed). Ruled now:
    enter by click or `ctrl-]`, exit with `ctrl-o` (the jump chain, not a new stack), one
    level of editing, one scroll axis (the host's), the seam stops everything
    buffer-scoped, one caret per note per window, dirty display transitive.
    **Its first commit is a refactor, not a feature.** An editable embed would be the
    second holder of a note the one-note-per-window invariant forbids (`open_note` *steals*
    a tab so a note is open once per window), and `shale_core::Buffer` holds model
    (`text`/`undo`/`redo`), caret (`cursor`/`selection`/`desired_col`) **and per-view state**
    (`display_segments`, `folds`, `read_only`) in one bag, held **by value** inside
    `MarkdownEditor` — while `dirty` sits on the wrong side, per-view in the editor. So:
    **M0** lift the per-view trio into a `ViewState` (~14 core sites, nearly free), **M1**
    put the model in a per-window registry keyed by path (`Entity<Buffer>`; 132
    `self.buffer` sites in 6 app files, 0 in tests) and move `dirty` onto the entry, then
    **M2** the row list spans buffers, **M3** entry/exit, **M4** the save story. Building
    the visible feature first turns the tab-plus-embed collision into shipped behavior.
  - **Test bar:** dirty state across the boundary is one of the cases the "app-side
    testing bar" rule names — a headless integration test, not eyeballs.
- **P4.1 Time machine** (brainstorm ✅): diff-based snapshots in `.shale`,
  session-based cadence (session end + ~3-min checkpoints + pre-risky-op +
  explicit saves; trivial diffs skipped; `:snapshot [label]` manual command —
  see brainstorm "Autosave × time machine"), retention 24h full → 30d hourly
  → daily, **100 MB default cap, configurable**. Per-note scrubber + diff view.
- **P4.2 Diff renderer** — shared by the time machine and P2.5's edit review.
  The line-level diff primitive (compute + paint, `similar`-backed) ships in
  **P2.5a**, and **P2.5b (2026-11-05) built most of the paint half too**: `diff::place`
  (a hunk answered against live text, `None` when it no longer fits), struck-through
  removals in `diff_delete` with the hunk's rule in the gutter, phantom addition rows in
  `diff_add`, and the `−N +M` collapsed badge in `diff_changed`. What is left for P4.2
  is the *browsing* half — a snapshot list and a scrubber over two stored versions —
  not the diff and not its rendering. Do not rebuild any of that.
  **Deferred in from P4.0b (2026-11-06, ruled Q6):** painting a queued proposal *inside an
  editable embed* — struck rows and phantom additions composed within another overlay's
  block — was ruled out of P4.0b and belongs here, not in the embed work. The review strip
  is authoritative and already lists proposals for embedded notes meanwhile.
- **P4.3 Query notes `.shq`** (brainstorm ✅) — **CLOSED 2026-11-10** (M1-M8; plan, edge
  ledger and rulings in `queries-plan.md`): `queries/` folder, TOML
  frontmatter + SQL body, read-only connection, `![[query-name]]` embedding. This is the
  "Obsidian bases" goal. The plan's "results cached by query-hash + vault mtime" is
  deliberately **not** built (ruling C): caching before measuring is a second source of
  truth for an answer that costs one statement to recompute.
  - **P4.3a Frontmatter (properties) indexing — the FIRST commit of this item, not a
    sibling** (ruled 2026-11-08; design, edge-case ledger and milestones in
    `properties-plan.md`). Schema v3 adds `props(note, key, ordinal, value, kind)`; the
    hand-rolled YAML-subset parser lives in `shale-core`; `tags` / `aliases` / `edges`
    become projections (body `#tags` unchanged — regression test), which is also the pass
    that finally fills `edges`. A `.shq` written before this lands can only ask about
    paths, tags and links, so the columns arrive **with** the feature, not after it.
    M1–M3 of that plan are this item's scope; M4 (in-place preview of the block) and M5
    (the properties pane) are separate surfaces and may be scheduled apart.
    - **M1 landed 2026-11-08.** `crates/core/src/frontmatter.rs` is the hand-rolled
      YAML-subset parser, with the §5 ledger written as its test table (EDGE-1…18 each
      have a test that names the case); schema v3 adds `props` and drops-and-rebuilds
      from v2; `<vault>/.shale/props.toml` carries the declared-key overrides and
      EDGE-17's `cr: 1/4` → 0.25 normalization; `:props` prints one note's rows from
      the DB, with the no-index posture (EDGE-25) and badges for a key written twice
      and a value that was not read. The free ride landed with it: the spell/grammar
      checker drops a note that lands inside the block (`lsp/mapping.rs`), which is
      the false-positive case `PROJECT.md` had been carrying.
    - **M2 landed 2026-11-08 (projections).** `tags` is body `#tags` **plus** the
      `tags:`/`tag:` key's items (`#` optional either spelling), and the regression half
      is a test: a body-only note indexes exactly as it did before properties existed.
      `edges` finally has a writer — one directed row per `[[…]]` value keyed by the
      property that wrote it, ghosts included, resolved targets stored as paths. Two
      rulings came out of doing it, both recorded in `properties-plan.md`: **only the
      `[[…]]` spelling is link-shaped** (the plan's "or resolves to a note path" makes
      `name: Goblin` in `Goblin.md` a link to itself), and **a declaration is honored
      only where the value can carry it** (declaring `cr` numeric and storing `cr: low`
      as a number is the silent cast the plan refuses; the row keeps its kind so the
      census can show the conflict). **`aliases` is deferred, not forgotten:** there is no
      `aliases` table, and making `LinkResolver` alias-aware changes how `[[Foo]]`
      resolves vault-wide (alias-vs-name collisions), which is a ruling to make rather
      than take in passing — it lands as its own commit, additively (aliases consulted
      only when the existing three rules find nothing).
    - **Landed 2026-11-09 (a surface asked for out of the blue, not an M-milestone):**
      the palette's `note.add-properties` — write an empty frontmatter block at the
      top of the note and put the caret between the fences. Three rules came with it,
      all tested: **one insertion, so one `u` takes the whole block away** (split it
      into two edits and the undo leaves a fence behind — the test does exactly that
      to prove it discriminates); **never a second block** (a note with two blocks at
      the top is a note whose properties are a coin flip, and the parser reads the
      first — the command moves the caret into the one that is there and says so);
      and **a read-only view is refused out loud**, because the buffer would have
      dropped the edit in silence and the status line would have claimed a block that
      never appeared. It follows the caret rule `:w` and `:rename` already follow:
      inside an embed it writes the embedded note. Palette-only, deliberately —
      `:props`/`:properties` already read them, and a second `:` word for the same
      noun is a coin flip of its own.
    - **Landed 2026-11-09 (opening a note could paint it with the note that was there
      before).** `Buffer::change_id` counted edits from zero, so every note nobody had
      edited shared one id — and the view keys its style spans, its quote substitutions
      and its wrap points on that id. `set_note` already said in its own comment that
      `change_id` is per buffer and two notes can share one by coincidence, and cleared
      the wrap cache for exactly that reason; the styles and the quotes were not
      cleared. So switching from a long note to a short one applied the long note's
      spans: hidden markers cut bytes out of the middle of words (the garbled first
      frame), and a span starting past the short note's end ran the fence walk off the
      row table and indexed a row that does not exist (the panic). Two fixes, one
      behaviour test and one invariant test: the caches are cleared wherever the buffer
      under the view changes, and `change_id` now carries a per-buffer epoch in its high
      bits, so the next cache that forgets to clear cannot collide. The fence walk also
      refuses to index past the last row — a stale cache should mispaint, not crash.
    - **Landed 2026-11-09 (aliases as link targets).** The deferred ruling above, taken.
      `aliases:` is a **fourth rule** in `LinkResolver`, consulted only after the three
      file rules find nothing — so an alias can never steal `[[Test]]` from `Test.md`,
      which is the collision the ruling was waiting on. Two notes claiming one alias
      tie-break the way a stem tie does: shortest path, then alphabetical. There is still
      no `aliases` table: the rows are ordinary `props` rows and `Db::aliases()` is the
      one reader, because a second table is a second answer to keep in step. Ghost edges
      filed *before* an alias existed are re-filed by a pass over the `links` table — the
      shape `rename.rs` already uses — which is why no full-text rescan was needed. Two
      things the index owns that a window has to agree with: the set links are coloured
      from now includes alias names, re-read after every index write, and the startup
      reconcile pass runs *after* a window computes that set, so its report now fans a
      refresh out to the open windows. Tested at three layers: the resolver rule and its
      shadowing, the sweep and its refusal to re-file a link a file already answers, and
      `gd` on `[[anon]]` landing on the note that declares the alias.
    - **M3 landed 2026-11-08 (the kind census).** `Db::kinds_for_keys` reads the
      vault-wide kind set for the keys a note writes, and `:props` badges a key the vault
      holds a kind for *elsewhere* — `cr=5 (number, vault also text)`. The clean note is
      the one that needs telling, because a `cr <= 5` query reads every note that writes
      the key. **Scope correction, not a gap:** the `prop:` operator and "the first
      `.shq`" are not buildable inside P4.3a — there is no query language and no `.shq`
      note type in the tree (`core::search` is body-text search); they are **P4.3's first
      commit**, and P4.3a's job was to have the columns and the `props_key (key, kind)`
      index ready when it lands. Done. What is left of P4.3a is the M4/M5 surfaces —
      `aliases` landed the same day, its own commit, ruling above.
    - **Landed 2026-11-09 (the block says whether it is being read).** A user could not
      tell a properties block from a divider: live preview draws a leading `---` as a rule
      across the editor and hides the dashes, so a block the index is reading, a block the
      parser refused, and a thematic break someone wrote on purpose were one glyph. Now the
      fences keep their dashes and the fences and keys wear a new `frontmatter` palette
      role; a block the parser refuses — **or a `---` on the first line that never closes**,
      which the parser reports as "no block" and the note silently loses every property to —
      is drawn in `callout_danger`. The ranges come from `frontmatter::block` itself, not a
      second scanner, and the key spans come from the parser's own `key_cut`, so the colour
      cannot promise a property the index does not hold. The role is named in all 21
      built-in palettes and is a TOML key like any other (`design-language.md` §1: roles are
      additive and named in the same pass, not derived); the readability auditor now floors
      it at 2.5 and flags it when a theme paints it in `dim` or in `accent`. A `---` at the
      top followed by prose stays a rule — the line after the fence decides.
    - **Landed 2026-11-09 (a property link means the same thing as a body link).**
      `frontmatter::wikilink` decided "is this value a link" from the first two and last
      two characters, so `region: "[[Waterdeep#Government]]"` resolved the literal
      `Waterdeep#Government`, missed, and was filed in `edges` as a ghost — while the body
      scanner has always stripped the anchor and resolved Waterdeep. The same shape test
      called `group: [[A]] [[B]]` one link to `A]] [[B`. Now the value has to be **one**
      link: the anchor is dropped for resolution (`props.value` keeps what the author
      typed, so `prop:region="[[Waterdeep#Government]]"` still matches and the reader still
      sees where the link lands), a value holding two links is `text`, and
      `[[#a heading in this note]]` names no note. Several links on one key is a list, one
      per item — which is also the only shape strict YAML, and therefore Obsidian, reads:
      to a YAML parser an unquoted `[[A]]` is a nested flow sequence, and
      `[[A]] [[B]]` is a parse error that costs the note its whole block.
    - **Landed 2026-11-09 (the block's link colour is a claim about the index).** The
      inline scanner ran over block lines the same way it runs over prose, so
      `group: see [[Federation]] for details` wore a link colour while the indexer filed
      no edge for it — the same lie in miniature as colouring a divider as a block. Inside
      a block a link span survives only when it sits in a value `frontmatter::wikilink`
      accepts, which is the one shape the indexer turns into an edge; the meaning comes
      from the parser's own `wikilink`/`flow_list`/`unquote`/`strip_comment`, and only the
      value's position on the line is computed in `mdstyle`. The same pass drops *marker*
      spans inside the block, so live preview hides nothing there: an aliased property
      showed `ally: the Zhents` over a file saying `ally: "[[Zhentarim|the Zhents]]"`, and
      the brackets are exactly what tells a reader whether a value is a link at all.
      `the_link_colour_and_the_index_agree_on_every_shape` is the drift guard: it counts
      the values the parser files as links against the regions the painter covers, so the
      two cannot disagree quietly.
  - **Landed 2026-11-09 (P4.3a M4: the block's shape, not a second reading of it).**
      The plan wrote this milestone as "display segments / hidden ranges", and the
      `545fb25` ruling had already made that impossible: inside a properties block live
      preview hides **nothing**, because the dashes and quotes there are the data — a
      value's quotes are the difference between `cr: 3` and `cr: "3"`, which is a kind
      question, and the fences are the only place the file says where the block ends.
      What shipped is the block's shape: a background band across every row of the block
      and a hairline along its top and bottom edges, in a new palette role
      `frontmatter_block_bg`. §1 of `design-language.md` refuses a computed colour, so
      all 20 built-ins name it, and the auditor flags a band fainter than 1.05:1 against
      the page — `the_auditor_flags_a_properties_block_with_no_band` is the
      falsification, and painting nord's band in nord's background fails both it and
      `every_built_in_can_draw_a_properties_block`. The region is **one span**
      (`Style::FrontmatterBlock`) emitted from the parser's own block range, so the band
      cannot cover a row the index does not read as properties, and a thematic break —
      the same three dashes in the same punctuation colour — gets none at all. A block
      that never closes keeps its band (those rows *are* the block) and takes
      `callout_danger` for its boundary. The bits ride `RowStyle.frontmatter` into the
      line element, and an embedded note's block gets its band from the same scan
      (`embeds::paint_body`), so the same text does not read two ways depending on how
      it was reached. `the_properties_band_covers_the_block_and_nothing_else` pins the
      region to the parser's range; `a_block_that_never_closes_keeps_its_band_and_takes_the_error_colour`
      pins the error case.
  - **Landed 2026-11-09 (P4.3a M5, first half: the vault-wide key census).** `:keys`
      and the palette's **List the vault's property keys** put the vault's vocabulary on
      one screen: one row per key, the keys that reach furthest first, each saying how
      many notes write it, how many values it holds, and every kind those values come
      as. The read is one statement (`Db::key_census`, a `GROUP BY key` over `props`)
      rather than one query per key, and the kinds are sorted by the spelling the reader
      is shown, because `GROUP_CONCAT` promises no order. A key whose values disagree is
      shown with **both** kinds, brought forward in the row — ruling §8.4's "the panel
      lists keys whose values disagree in kind, with counts", with no modal and no
      warning on save. Choosing a row does not open a second list: it puts `prop:key` in
      the search field (`Picker::sourced_seeded`), because the search already answers
      "which notes write this" and can be widened from there; a key with a space or an
      operator character in it is quoted on the way in, or the grammar would read
      `prop:Max HP` as a key and a value. No index and an empty vault are different
      rows (EDGE-25 again).
  - **Landed 2026-11-09 (P4.3a M5, second half: the key rename — P4.3a CLOSED).**
      `:rename-key [old] [new]`, and the palette's **Rename a property key across the
      vault**. This is the migration path ruling §1.3 promises for a vocabulary cleanup
      — `Type` becomes `type` because someone asked, not because a tool decided the vault
      should be lowercase — so it rides the same machinery a file rename rides
      (`Rewrite` is now one type for both: path, new text, what changed). Three bounds on
      what gets touched, each a lie it refuses to tell: only the note's **block** (prose
      reading `cr: 5` is a statblock, not frontmatter, and rewriting it changes what the
      note *says*), only **top-level** property lines (a nested map's inner keys are
      indexed and never rewritten, ruling §1.5), and only the **key span** — the span
      comes from the parser's own `key_span`, the one the editor colours, so the rename
      and the colour cannot disagree about where a key ends. A block that will not parse
      is skipped: the index never claimed a key in it, so the note would change without
      the census having listed it. Keys are case-significant, which is what makes this a
      migration rather than a forced rewrite. What the window adds on top of core's
      rewrite: every note that changed is told to the index (Shale's own writes are
      stamped out of the watcher's view), the link names are re-read in case the key was
      `aliases`, the graph is refreshed because a link-shaped value moves with the key
      that held it, and a note open with unsaved edits is **skipped and named** — a check
      that now runs against the whole per-window buffer registry rather than the active
      editor alone, so a dirty note in another tab is protected too. `.shale/props.toml`
      is a hand-kept file and is not rewritten; the status line names the declaration the
      rename left pointing at nothing. Falsified: nested guard, malformed-block skip,
      case-significance, the index write, the rename mood of the pane, the dirty-note
      check (active-only and registry-wide), and the declaration suffix each turn a test
      red.
  - **Landed 2026-11-09 (P4.3 M1: the `.shq` format and the runner).**
    `query_note::QueryNote` reads the file — an optional TOML declaration between `+++`
    fences, then the SQL body — and `Db::run` answers it: one statement, a read-only
    connection, cells that keep SQLite's storage classes, and `:current` bound as a named
    parameter. The fence is `+++` and not `---` because a note's `---` block is YAML owned
    by `frontmatter` and SQL's line comments start with `--`, so a `---` above SQL would be
    read as a comment by the language below it. The declaration is optional — a saved search
    wants none — and a declaration key this version does not know is kept out of the struct
    rather than out of the file. A second statement is refused with the reader's own text
    quoted back: not because it could do damage on a read-only connection, but because a
    note that silently runs two statements answers a different question than the one the
    reader wrote. No result cache yet, deliberately — the plan wrote "cached by query-hash +
    vault mtime" before the index existed and before any query had been measured, and the key
    rule a cache owes (the anchor path belongs in the key, or a query cached for one session
    answers for the next) is written in `db/query.rs` so it knows what it owes. `.shq` is its
    own kind of file: `Vault::list_queries`, and out of `list_attachments`, because left in
    that list `![[my-query]]` would resolve to a query note and paint it as a broken image.
    Each rule falsified individually. Plan, edge ledger and six open rulings in
    `queries-plan.md`.
  - **Landed 2026-11-09 (P4.3 M2: the door).** `:query` and the palette's **Run a saved
    query**: the vault's `.shq` files, each labelled with the name it declares rather than
    the stem of the file it lives in, and the answer drawn in the picker — the columns
    once, then one row per answer. Two rulings were asked and taken: **the picker for M2,
    the in-view results region for M3**, and **a query note is a tree row but not a
    quick-open row** — the tree is where the author's own files live, and `space f` keeps
    answering one question. What the surface refuses to say silently, each a row: an answer
    of nothing ("no note answers this", EDGE-1), SQL the database refuses (its own words,
    EDGE-2), a body that is not a question (EDGE-3), a file that is not a query note
    (EDGE-6), and the index being off ("there is no vault index to read", EDGE-14 — the
    EDGE-25 posture, because "zero" and "I could not look" are different answers). A row
    whose first cell names a note in the vault *is* that note, which is what makes a saved
    query a door rather than a printout; a row that names no note stays a row, so a query
    over a half-populated campaign shows its plot holes instead of offering dead clicks
    (EDGE-16). NULL is drawn `∅` and an empty string `''` (EDGE-8), and an answer longer
    than 200 rows is cut with a row saying how much was cut (EDGE-9) — the core reads what
    the query answers, the surface decides what a person can scan, and the cut and the
    admission are one decision. The declaration's `sort` is honoured from the start,
    applied to the answer rather than spliced into the statement. The binding rules got a
    walker fix on the way: a `;` and a `:name` are found by **one** scan, because two
    walkers that disagree about where a quote ends would leave one rule a hole the other
    does not have; a binding the body never asks for is dropped (the surface offers an
    anchor whether or not this query wants one) and a body asking for a name nothing
    supplies is refused by name. The demo vault gained two query notes, and both were run
    against a copy of it to check the demo is not a broken embed waiting to happen.
    Fourteen surface tests, seven of them falsified.
  - **Ruled 2026-11-09 (M3, designed before it is built).** The file renders as editable SQL
    with a live table under the body, and the mechanism is **a fold, not a concealment
    system**: the declaration and the SQL body are folded by default and the fold's
    *rendering* is the table instead of an ellipsis. Live preview hides spans inside a row;
    hiding a whole multi-line region is a fold, and folds already own the hard parts —
    per-view state keyed by path, a marker row, and every motion, ruler and scroll that
    agrees with them. A second way for text to be invisible would be a second set of bugs to
    find twice. Three modes, one rule each: raw is the source with no table, live is the
    table with the SQL folded and *editing opens the fold*, preview is the titled table only.
    **No pagination** — preview already scrolls instead of moving the caret (`j`/`k` step,
    `gg`/`G` jump the ends), so a large answer is virtualized (draw the rows intersecting the
    viewport; height = rows × row height so the scroll extent stays honest) rather than paged
    into state that would have to keep step with the index. The marker row carries the
    answer's shape (`▸ Creature ratings · 2 rows`), and a refusal or an empty answer
    **reveals the source**, because a folded region with no table is a blank page. The line
    that keeps this from colliding with the properties-block ruling is **hide what computes,
    never hide what asserts**: there the brackets are the data, here the SQL is a program and
    the answer is the data. `/` searches the body and never a row (EDGE-23 — a hit is a caret
    position and phantom rows own no bytes), which is also the reason the picker stays: the
    picker is the searchable view of a query, the region is the readable one. Unfolding in
    preview, where `za` is swallowed, is both the clickable marker and a leader chord
    (EDGE-24). SQL highlighting is split — fences and `--` comments now, keywords at P6.6
    rather than a second hand-written scanner. An errored `![[query]]` shows its error inside
    the block: no hidden errors and state. Agents may **run** queries as well as write them,
    through the same `Db::run` on the same read-only connection — the guarantee stays strong
    through that door because there is no second runner. Two perf checks are owed rather than
    decided now: the cost of a query embedded in a query, and an uncapped read (the surface
    caps what it draws; the core reads what the query answers).
  - **Landed 2026-11-10 (M3).** `FoldKind` separates a query's fold from a heading's — a
    `.shq`'s `#`-looking lines are not sections, and its one region is the whole file, shut
    by default and titled with the query's name. The table hangs under the last *visible*
    line of the program, folded or open, as phantom rows that own no bytes: the caret cannot
    land in one, `/` cannot match one, and a row that names no note is not addressable at
    all. A cell spelled exactly as a vault path carries a door and the link colour, the
    picker's rule, so the same answer is a door in both surfaces or a dead end in both.
    `Cell::display` and `Rows::sort_by` moved to the core for the same reason. Two gates
    open the program, one per kind of wrong: a body the runner refuses **on shape** gets no
    fold at all (`fold::query_region` asks `db::check_question`, the runner's own question),
    and a body that fails **at run time** keeps its fold and opens it. An answer of no rows
    keeps the fold shut, because `the query answers nothing` is a reason the reader can read
    where they stand. Virtualization is deferred to the perf review: the region is capped at
    `MAX_ROWS` rows plus the line admitting the rest, which bounds a frame the way an embed
    block is bounded (ruling 18 in `queries-plan.md`).
  - **Landed 2026-11-10 (M7: the agent door).** `shale-mcp` gained `list_queries`,
    `read_query` and `run_query` (`crates/mcp/src/query_tools.rs`), so an agent asked for
    "a dashboard of X" writes a `.shq` and reads back what it answers instead of holding a
    line of SQL in a transcript. Three rules hold it: the runner moved rather than being
    copied (`db::run_on` is the act; `Db::run` is `check` + a read connection + that call),
    and the door opens the index through `Db::reader`, which answers `None` for a vault with
    no index rather than creating one to answer from (ruling 41, EDGE-14). A relative query
    names its host with a `note` argument or is refused in the editor's own sentence — an
    agent has no caret, so it says which note it means (ruling 42, EDGE-12). There is no
    `write_query`: `write_note` writes any file in the vault, `.shq` included, which is what
    keeps EDGE-18's review-queue promise for a query write without a second path to keep
    honest (ruling 43). And the sentences are one chain — `query_note::ask` parses, anchors,
    asks, sorts, and answers `Answered | Empty | Refused`, which the picker draws as rows and
    the agent door renders as text (ruling 44), so the two doors cannot disagree about the
    same file.
  - **P4.3b Headings index — AFTER P4.3a, and explicitly NOT inside it** (ruled
    2026-11-08). **Plan, rulings 46-52 and an edge ledger are in `headings-plan.md`
    (2026-11-10)**, and the survey that came with it found the item's real work: three
    scanners answer "what headings does this note have" (`transclude::heading_level`,
    `mdstyle::object::headings`, `fold::heading_rows`) and they disagree about `#` alone,
    `#\tText` and `> ## Title`. The table is four columns; agreeing on one scanner is the
    milestone. Schema **v4** adds `headings(note, line, text, depth)`, filled in the
    same indexer pass from `mdstyle`/`mdlayout` — derived, not authored, so it cannot
    lie, and it is dropped/rebuilt like the rest of the index. It is what lets a *query*
    group things by the section they sit in (cue chips, counter chips, scene cards), and
    it is already wanted by the outline pane (P4.13), `#heading` anchors, block refs
    (P4.9) and Canvas C3's block cards. **Do not grow P4.3a to cover it**: P4.3a is in
    flight, and a second table in the same commit turns a schema decision into a moving
    target for whoever is implementing it.
    **M1 closed (2026-11-10):** schema **v4** is live — `headings(note, line, text, depth)`,
    written in the note's own indexing pass from a new `mdstyle::object::heading_spans` (the
    parser's spans, keeping the text range and depth the old `headings()` threw away),
    delete-then-insert per note, dropped with the note, and readable through `Db::run`, which
    is what a saved query runs on. The survey's three-way disagreement held up under a test
    except in one place: the parser styles **no** heading inside a blockquote, so `> ## Title`
    is quoted text and gets no row — the fold and the anchor matcher already said the same,
    and the plan's EDGE-3 and question D were corrected rather than left as a guess.
    **M2 closed (2026-11-10): one scanner.** `heading_row` and `section_rows` now read
    `mdstyle::object::heading_lines`, and `heading_level`, `heading_text`, `transclude::fence`
    and `fold::heading_rows` are deleted — the last of those was a fourth copy of the same
    span walk, so the fold already agreed with the painter and the anchor matcher was the odd
    one out. What a reader can notice: `[[note#Tabbed]]` reaches `#\tTabbed` and that heading
    folds, and a fence closer now has to match its opener the way CommonMark says. No existing
    anchor or fold test was re-recorded; killing the shared scan turns 25 of them red at once,
    which is the proof the three surfaces really do read one walk now.
    **M3 closed (2026-11-10): P4.3b is done.** `:sections` outlines the note on screen from
    its **buffer** (indented by depth, each row naming its line) and, with a pattern, lists
    the vault from `headings` — each row naming the note the section lives in — and choosing
    either puts the caret on the heading. Where the index's line meets a dirty buffer the line
    is *checked* before it is trusted, with the heading's text as the fallback (ruling 53),
    which is also what lets a note with two same-named sections jump to the second one. The
    grouped-query half is tested through `Db::run`: what v4 can group by is the section
    itself, since `props` and `links` have no line column yet — the roadmap's promise that a
    query can group things by the section they sit in needs one, and that is a later bump
    rather than a promise this one quietly drops.
- **P4.4 Tag tree panel** (brainstorm ✅): rollup counts, fuzzy filter, Enter =
  search, `g` = filter graph, `r` = rename tag vault-wide, color assignment →
  graph groups. Plus `#` autocomplete from the DB.
- **P4.5 Marks + bookmarks panel** (brainstorm ✅): global marks (`m{char}` /
  `'{char}` across notes), named bookmarks, jump-chain view, MCP-readable.
- **P4.6 Link health + rename propagation** (brainstorm ✅). **Half shipped:**
  file-tree rename already rewrites the wiki-links that point at the note it moved
  or renamed (and every tab showing the old path follows). What is left is the
  *report*: heading-anchor renames, tag renames, and the broken-link /
  near-duplicate / orphan / unused-attachment list. Extends `rename.rs`.
- **P4.6a `gd` follows a heading anchor** — **done 2026-11-06.** `WikiLink` carries
  the anchor, `EditorEvent::FollowLink` carries it, and the workspace reveals the heading
  row in the note it just opened (`editor.reveal_heading`, over the *buffer* so a jump
  into a dirty note lands on the heading the reader can see). `[[#heading]]` with no
  target searches the note already on screen. `gD` ignores the anchor — a heading must
  never become part of a file name, and that is a test, not a comment. The lookup is
  `transclude::heading_row`, the same matching `![[note#heading]]` embeds use, so a jump
  and an embed cannot disagree about which heading a written anchor means.
  Original finding:

  **P4.6a was added 2026-11-06, found by asking a question rather than by the audit.** `gd` on `[[note#The Fall]]` currently opens
  `note` at the top: `extract_wikilinks` parses `WikiLink.target` as *"without alias or
  heading anchor"*, so the anchor is gone before `EditorEvent::FollowLink` is emitted
  and `follow_link` never learns a section was asked for. The cursor being on the
  heading part still jumps (the link's `start..end` covers the whole construct) — it just
  ignores which section. Nothing owned this: P4.6 covers heading-anchor *renames*, and
  P4.9's "the jump scrolls to it" is about `^abc123` block anchors. The audit walked
  `brainstorm.md`, and brainstorm never ruled on it.
  **Ruled (2026-11-06):** an anchor that names no heading still opens the note at the
  top and says `no heading "…" in …` in the status line — the note is almost certainly
  the right destination, and refusing would be worse. The cursor lands **on the heading
  line**, which is what `*` and the section text object (`da#`) treat as the section's
  anchor, so the jump composes with the motion the reader is likely to want next.
  **Cheap because P4.0a already built the lookup:** `transclude::section` answers "which
  line is this heading" (case-insensitive, fence-aware), and `open_note_at(path, line)`
  / `editor.reveal_line` are how search results already jump. The change is to keep the
  anchor on `WikiLink`, carry it in the event, and reveal the row after the note opens.
  Tests: the match, the miss (warning + top), and `gD` on a missing note, where the
  anchor is meaningless and must not leak into the created note's name.
- **P4.7 Writing stats panel**: live counts, sentence-length histogram, reading
  time, most-used words; hosts the per-folder word-count tree.
- **P4.8 Provenance as a first-class filter** (brainstorm ✅): user/agent/both
  in graph filters, tag tree, search, query notes; agent-touched node badges.
- **P4.9 Block references** (brainstorm ✅). **Added by the 2026-11-05 audit** — ruled
  design, no item, no code. `^abc123` anchor at the end of a paragraph or bullet;
  `![[note#^abc123]]` embeds a block (rides P4.0), `[[note#^abc123]]` links to one and
  the jump scrolls to it. `mdstyle` gains a block-anchor span; the transclusion
  resolver grows from note/section to block granularity. **Gates canvas C3's
  block-level cards** (`canvas-plan.md`) and the block case of P6.2.
  **Design pass ruled (2026-11-11): `blockrefs-plan.md`** — the survey, a 26-row edge
  ledger, milestones M1–M5, and all eleven rulings answered (recommendations taken; the
  table is in the plan's §3). **M1 closed:** `mdstyle::blockmap::block_map` is the one scan
  that answers "what blocks does this note have", and `Style::BlockAnchor` paints the id
  `dim` without ever hiding it. **M2 closed (2026-11-11):** schema **v5** is live —
  `blocks(note, id, line)` with `blocks_id`/`blocks_note`, written from `block_map` in the
  note's record pass and cleared with the note. **No primary key on purpose**: one id can
  name two blocks, in one note or in two (ruling 9), and a key would fail the index over a
  note the author wrote. `line` is the block's **first** line, not the token's, and the
  block's extent is not stored — the scan recomputes it. Readers: `Db::blocks_matching`
  (vault-wide, wildcards quoted) and `Db::block_id_taken` (exact, for `:block-id`'s
  collision check). A saved query can already group the vault by id, which is the whole
  reason the table exists. **M3 closed (2026-11-11):** the anchor is a **kind** now, not a
  string — `markdown::Anchor::{Heading, Block}`, decided by the link's spelling and shared
  by `WikiLink` and `EmbedRef`, so `gd` and `![[…]]` cannot mean two different things.
  `transclude::block_rows` cuts the block an id names; the embed draws it through the
  existing phantom rows, the hover card follows, and `reveal_block` lands the caret on the
  block's **first** line from a buffer scan — an id typed a second ago jumps before it is
  saved (EDGE-12). A missing block says `no block "^x" in …`, the embed badge's own words.
  The chained `[[note#H#^id]]` is refused by reading it literally (ruling 11). The recursion
  cap is pinned by test, not by comment: block embeds stop at `MAX_DEPTH` like everything
  else, and a block that embeds its own note is cut by the chain at the second mention.
  **M4 closed (2026-11-11): the two verbs.** `:block-id [id]` gives the block under the
  caret an id — **one insert, so one `u` takes it back**, the blank line a structured
  block's id stands on included (EDGE-21) — and refuses a second id on a block that has
  one, an id outside ruling 2's grammar, and an id the vault already uses
  (`Db::block_id_taken`, the check a buffer cannot do alone). With no argument it draws 6
  chars of `a-z2-7` and redraws while the draw is taken. The write is **checked by the
  scan**: each candidate spelling is applied to a copy and re-read, and the first that
  names *this* block wins, because a placement guess the scan reads as another block would
  hand the author a link that lies. `:blocks` is two halves like `:sections` — no pattern
  lists the note on screen from its buffer, a pattern lists the vault from `blocks` — and
  its rows say `^id · kind · line N` under the block's first line. EDGE-29 (new): a
  command reaches the palette through its **label**, not its name. **M5 closed
  (2026-11-11):** `docs/notes.md` says what a block is, where an id sits and what refuses;
  the demo tour carries a real one — `Welcome.md`'s first paragraph is `^start`, linked and
  embedded from the tour — and the probe that checked it read the demo files through the
  same resolver the window uses. **M6 closed (2026-11-11): the block text object**,
  ruling 10's — as `i^` / `a^` rather than the ruled `ib` / `ab`, because `b` and `B` are
  vim's letters for parens and braces and the object would have shadowed a motion the hand
  already knows. The caret is the mark a block id is written with, so `di^` deletes exactly
  the span `:block-id` labels and `#^id` resolves, one scan answering both. **P4.9 is
  closed.**
  Two edges the plan did not have: **a heading that carries an id still answers its name**
  (EDGE-27 — the id is cut before heading matching), and **an embed line can carry its own
  id** (EDGE-28 — `![[a]] ^x` was not an embed at all, so labelling a working transclusion
  would have silently un-embedded it).
- **P4.10 Trash** (brainstorm ✅, user called it "very important"; ruled **vault-local**,
  not OS trash). **Added by the audit.** Delete already rewrites links; deleting to a
  `.shale/trash/` instead of unlinking is the same path with one step added, and
  `vault.rs` already ignores a `.trash` name when walking. Needs: move-in with the
  original path recorded, a restore, an empty, and the tree/status surface for it.
- **P4.11 Daily notes** (brainstorm ✅). **Added by the audit.** `:today` opens
  `daily/YYYY-MM-DD.md` (folder and format from config), creating it from a template
  when missing — which makes P6.4's template variables the dependency, not a separate
  feature. Rides quick-open, the session file, and the start block.
- **P4.12 Attachment capture** (brainstorm ✅). **Added by the audit.** Paste or drop an
  image into a note → store it under the attachment folder (vault-root or adjacent,
  config) → insert `![[name]]`. The embed *renderer* exists (`line_image_embed`); the
  capture half does not. Two known renderer leftovers ride along: an embed inside a
  paragraph is not drawn as a picture, and a `|alias` suffix reads as "no size".
- **P4.13 Sidebar — outline, links, tags, properties** (brainstorm ✅). **Added by the
  audit; reshaped by the sidebar round of 2026-11-11 from "an outline pane" into a strip
  that answers four questions about one note.** A fixed strip on the **right**, opposite
  the file tree, mirroring the tree's hosting pattern (its own `FocusHandle`, its own
  keymap context, its own visibility toggle, session-restored) and its keyboard
  (`j k h l enter`, `escape` back to the note). It describes the focused note, or the
  note the pane last had when the strip itself has the keyboard (`Pane::note_mru`), and
  `:pin` sticks it to one note. Its sections: the heading tree (click to jump, current
  section marked on caret moves **and** on scroll, mirroring the note's folds, no block
  ids), **links to here** and **links from here** (backlinks have no window surface today
  — `docs/notes.md:151` says so out loud — and `links(source, target, target_kind)` has
  neither a `Db` reader nor a line column, which is M2's schema question), the note's
  tags, and its properties. `shift+j`/`shift+k` move a heading, taking its deeper
  sub-headings with it (`section_range` is already depth-aware), never re-parenting and
  never changing a level. The heading knowledge comes from `mdstyle`/`mdlayout`, the same
  units `da#` operates on; the `headings` table (P4.3b, closed 2026-11-10) is what lets
  it speak about notes it has not opened, and the buffer stays the source for the open
  one, because an unsaved heading is not in the index yet. Distinct from the canvas
  outline view in P5.6 — do not build one and call it both.
  **Round 2 settled the shape (2026-11-11).** **One view at a time**, chosen with `[`/`]`
  inside the strip or `:sidebar <name>` from anywhere (`:sidebar` alone stays the toggle,
  `space b` shows and hides). The strip is a stop in the window's `ctrl-w h/j/k/l` grammar:
  `ctrl-w l` at the right edge falls back to it the way `ctrl-w h` at the left edge falls
  back to the tree, and **both edges are unified on the seat** — the pane that last had the
  keyboard — with no wrapping. `Tab`/`shift-Tab` relevel a heading **and its subtree** (the
  children are whatever the scan found *before* the change, or a demotion re-parents a
  heading out from under itself). The outline follows the caret: the current section is
  marked in colour always, the list scrolls only when the mark itself moved, and while the
  strip has the keyboard its selection scrubs the note **without moving the caret** — `enter`
  is what moves it, and a heading jump inside the note on screen is a pure reveal, never a
  second tab and never a re-read. The header strip is chrome and carries the view's count.
  **Five milestones**, each with acceptance criteria and its own falsification pass, in
  `sidebar-plan.md`: M1 shell + outline (buffer-sourced, session-restored), M2 the two core
  structure acts (`mdstyle::move_section`, `mdstyle::relevel_section`), M3 links both ways
  (schema **v6**: `links.line`, so a backlink row lands on the line that links — a version
  bump and a rebuild, since the index is never migrated), M4 tags and properties, M5 `:pin`
  and `docs/sidebar.md`. **One behaviour change outside the strip:** `record_jump` refuses a
  same-note jump today (`jumps.rs:274`); the spec narrows that to "same note *and* same
  position" so `ctrl-o` walks between sections of one note (EDGE-S14).
  **M1 closed (2026-11-11):** the strip is on screen — `crates/app/src/sidebar.rs` (the
  model: the five view names, the buffer-sourced outline, the caret→section map) and
  `crates/app/src/workspace/sidebar.rs` (the hosting: focus, visibility, keys, rows drawn
  in the frame that shows them). `space b` / `:sidebar [on|off]` / `:sidebar <view>`, the
  palette rows, and `space b` in the strip's own context; `[`/`]` cycle the five names and
  the four unbuilt views say "this view is not built yet" rather than going quiet. The
  `ctrl-w` edges are unified on the seat and `first_pane`/`first_leaf` went with them;
  `ctrl-w j`/`k` from the strip stay put. The mark follows the caret and scrolls the list
  once, and the anti-fight rule is tested with a real wheel event, not by reading the code.
  `enter` is a pure reveal: the test asserts the buffer's `Rc` identity, not just the caret.
  Session fields (`sidebar_visible`, `sidebar_view`) are `Option`, so a pre-strip file
  restores without one. `docs/sidebar.md` arrived here rather than at M5. **Two deltas from
  the plan:** the width is a constant 260px (ruling G deferred resizing, so there is no
  `sidebar_width` field to restore yet), and the strip's subject guard means `ctrl-w l` from
  a graph, blank or query tab refuses rather than focusing an empty surface — the pane tests
  that assert that keystroke's old behaviour stay green.
  **M2 closed (2026-11-11):** the two structure acts live in `crates/core/src/mdstyle/section.rs`
  and are **pure** — they answer with byte-range replacements or with the reason the text
  will not take them, so the whole rule set is table-tested without a buffer (17 tests:
  fence, blockquote, list, table, setext-looking `---`, bare marker, trailing `^block-id`,
  both caps). The window applies the answer as one grouped edit, so one `u` restores a whole
  relevelled subtree. The doors are the strip's keys and nothing else: `shift-j`/`shift-k`
  move, `tab`/`shift-tab` relevel, and every refusal is one line in the status bar
  ("“E” has no sibling under the same parent, so it cannot move up"). **Four things the plan
  did not know it would need:** the line a moved heading lands on has to be *counted*, not
  borrowed from the sibling's old line number — the two sections are almost never the same
  length, and a selection keyed to the wrong line moves the wrong section on the next
  keystroke; the seam between two swapped sections is normalised to exactly one blank line,
  because moving the last section of a note that ends without a trailing blank would
  otherwise glue a heading onto the line above it; Shale's scanner styles a heading only when
  the `#` opens the line, so an indented `#` is not a section here either rather than a looser
  rule kept just for this module; and `Editor::is_read_only()` exists so a preview refuses out
  loud instead of the buffer's own guard swallowing the keystroke. `HeadingLine` gained
  `line_start` so touching a marker run does not rebuild the line table. All five of the
  plan's falsifications went red as intended: subtree computed after the change, clamp
  instead of refuse, whole-line rewrite instead of the marker run, the parent check removed,
  and the read-only guard removed.
  **M3 closed (2026-11-11): the links, both directions.** Schema **v6** is live: `links`
  gains `line INTEGER NOT NULL`, one row per (source, target, kind) holding the **first**
  occurrence's line, and — as planned — no migration, the old index is dropped and rebuilt
  (`SCHEMA_V5` survives as a `#[cfg(test)]` copy so the rebuild is a tested path).
  `Db::backlinks` and `Db::outgoing` are the first production readers of that table by
  target and by source: the graph walks the vault and `Vault::backlinks` is a filesystem
  scan, so neither surface had anything that could answer "which line links". A backlink row
  opens its note **on that line**; an outgoing row opens at the top, because the line the
  index holds in that direction sits in the note the reader came from. A link to a name that
  is no note keeps its row, spelled exactly as typed, and `enter` says so rather than
  inventing a file — attachments and `.shq` targets are rows too, being the author's own
  files rather than holes. `Row.line` became `Row.key` beside an explicit
  `Act::{Reveal, Open}`: a link row's key is its place in a list of *other* notes, and read
  as a buffer line it would move a section nobody pointed at. **Three things the plan did not
  know it would need:** the structure keys had to be pinned to the Outline view, which is the
  only one whose rows are this note's headings; switching views now opens on the new view's
  first row, because a cleared selection made `enter` straight after `]` a dead key that said
  nothing; and `index_key` exists because a tab may hold a path in either spelling while the
  index's keys are vault-relative. The index-off sentence is now one constant
  (`shale_core::db::NO_INDEX`) shared by `:props` and the strip, so an index-backed view with
  no index says which feature is missing instead of claiming "nothing links here". All five
  falsifications went red as intended: the line thrown away, ghosts filtered out, the line
  kept in the wrong direction, the outline-only guard removed, and the index-off sentence
  removed.
  **M4 closed (2026-11-11): tags and properties.** All five views are built. The Tags view
  lists the note's `#tags` **and** its `tags:`/`tag:` frontmatter through
  `shale_core::graph::note_tags` — a new shared merge, because the indexer was restating
  that loop by hand and the strip must not disagree with the index about what a note carries
  — and each row carries `Db::tag_census`, the new reader for "how many **other** notes write
  this", which excludes the described note by hand rather than subtracting, so a tag typed
  but not saved cannot push the count negative. The Properties view lists the frontmatter as
  authored, from the buffer; a value written `[[like this]]` is a link and opens through
  `resolve_target` (file rules, aliases, saved queries — the door `gd` walks), and anything
  else asks `prop:key` of the search, the `:keys` picker's hand-off, with a new `tag_token`
  beside `prop_token` so both operators quote the same way (P4.14's one grammar).
  **Three decisions the plan left open:** the census number is `Option` and drawn as nothing
  when there is no index, because a screenful of zeros would claim "nobody else writes this"
  about a vault nothing looked at; a click on a row that asks a vault question only selects
  it, since the search field takes the keyboard and a click in this strip never does; and an
  unresolved property link is marked at build time rather than at `enter`, so the row shows
  the hole the way an unresolved wiki-link does. Five falsifications, all red: no-index
  drawn as zero, the Tags view gated on the index, every value treated as a link, the census
  counting the note itself, and the frontmatter half dropped from the merge — the last one
  only after the new core test was rewritten, which had been written too weakly to bite and
  said so on its own.
- **P4.14 Search operators** (brainstorm ✅). **Added by the audit.** `tag:`, `path:`,
  `link:`, `modified:` and friends in vault search, over the P1 index. **One grammar,
  one parser, shared with `.shq` (P4.3)** — inventing it twice is how the two surfaces
  end up disagreeing about what `tag:` means.
  - **Landed 2026-11-09 (vault search; the grammar is `.shq`'s to reuse).**
    `crates/core/src/query.rs` is the parser and knows only the *shape* of a query —
    terms, operators, what each is asking — and says nothing about SQL, so P4.3 reads
    the same module rather than inventing a second dialect. `tag:` `path:` `link:`
    `prop:` `modified:`, quoted values for names with spaces, operators **and**
    together, and bare text keeps its old literal meaning: operators narrow *which
    notes* the line scan runs over. An operators-only query is a question about notes,
    so it answers one row per note instead of one row per line.
    `Db::notes_matching` is the reader — and this is the first production reader the
    `props`, `tags` and `links` tables have had (the 2026-11-09 audit's finding: all
    17 `FROM props` reads were inside `db/index.rs`, and `edges` had a writer and no
    reader). One statement per query, so the plan stays on `tags_tag`, `links_target`
    and `props_key (key, kind)`.
    Three rulings came out of building it, all in the code's doc comments and
    `docs/notes.md`: **a near-miss operator is refused out loud** (`tags:monster`
    searched as prose answers "0 of 0 matching", which reads as an empty vault rather
    than a typo; a value starting with `//` is exempt because that is a URL);
    **a comparison never casts, in either direction** (an ordering reads only rows the
    index recorded as numbers, and `prop:cr<=low` is refused rather than evaluated —
    `CAST('low' AS REAL)` is `0` on the query side too); and **`path:` speaks the
    ignore rules' glob dialect**, not SQL `LIKE`, so search and an agent profile's
    patterns cannot mean different things by `*.md`. With the index off, operators say
    so instead of answering zero — the EDGE-25 posture `:props` already uses.
    **Left for P4.3:** the same grammar inside a `.shq` body, and whatever operators
    the query language wants beyond these five (`edges` still has no reader — typed
    edge kinds are P4.16's).
  - **Landed 2026-11-09 (a row says *why* it matched).** An operator row names a
    note, and a name with nothing after it gives the user no way to check the answer:
    `tag:monster` and `tag:goblinoid` looked identical. So each row quotes the note's
    own line — `cr: 5 · notes`, `Digs in a [[lair]]. · notes` — with the part the
    query named brought forward in `match_text` and the rest left dim, which is the
    same standard text search already uses. Two rulings came with it:
    **the evidence comes from the note's text, not from the index's tables**
    (`crates/core/src/evidence.rs`). `tags(note, tag)` cannot say whether the note
    wrote `#character` in the body or `tags: character` in frontmatter, and a row
    showing the second when the note holds the first says something the note does
    not. The tables hold claims; a display string is not a claim. And **the scanner
    that built the index finds the evidence** — `mdstyle::style_spans`, the same one
    `extract_tags` uses — so a `#` inside a code fence is not evidence here either,
    and a row cannot point at the word "character" in prose while the tag that
    matched sits elsewhere. A bound is not the note's text either: `prop:cr<=5` on a
    note reading `cr: 3` forwards the `3` that satisfied the bound, never the `5`
    the query typed. When the index is ahead of the file, the row shows **no**
    evidence rather than a line the note no longer holds — pinned by a test that
    edits a note out from under its own index row.
    **The v4 door stays open (user, 2026-11-09):** Obsidian stores the line and
    character offset of each match so the lookup is a read rather than a scan. If we
    want that, it is a schema v4 that puts `line` and a byte offset on `tags`,
    `links` and `props`, plus a re-index — worth doing when something needs to *jump*
    to the position (the properties pane, a backlinks pane with positions), not for a
    label. Today's scan reads only the notes the index already chose, which is less
    than the text search beside it already costs per keystroke.
  - **Landed with it: the row is cut around the match, and a crash went with it.**
    Quoting the *start* of a line hides the answer when the line is long: a 78-character
    sentence clipped at 60 can cut the match away entirely. So the quote is a window
    centred on the forwarded span, `…` marking what was left out, and several operators
    split the row's evidence budget instead of the first one spending it.
    **The first version panicked in gpui's text run** (`space s`, `link:Welcome`, dead
    window on the last letter). The clip appended an ellipsis and then measured the
    forwarded range against the string *including* it, so a match straddling the cut put
    a run boundary inside the `…` — `invalid text run`, on the render path, no way back.
    The window counts in characters and returns byte ranges into what it actually built;
    `a_match_straddling_the_cut_cannot_end_a_run_mid_character` reproduces the original
    panic byte for byte against the old logic.
- **P4.15 Inline computed surfaces** (brainstorm ✅, policy ruled 2026-10-29, one widget
  to start). **Added by the audit.** App-offered, region-anchored facts (word count,
  reading time) — the status bar owns whole-note facts; user-authored widgets are
  plugins and out of scope; data tables are `.shq`. Hosts P4.7's counts.
  **Named instance (2026-11-08):** "the cue for the section the caret is in" (brainstorm
  "Session-linked chips") is a legitimate one — a line-level fact painted near the block,
  **not** in the status bar — and it is the *only* cue-aware surface allowed: no
  notification, no scroll-triggered motion (the motion contract in `design-language.md`
  reserves animation for toasts), no playback.
- **P4.16 Graph lenses** (brainstorm ✅, split ruling 2026-10-29). **Added by the audit.**
  Saved, named graph filters over the existing filter model — the constellation filter
  and `region:` props already do the mechanics; a lens is a saved filter with a name
  and a keybinding. Tag colouring rides P4.4's "color assignment → graph groups".
  - **Rewritten 2026-11-08: a lens is a saved `.shq`, not a filter format.** The query
    supplies the **node set** (a row must resolve to a note path — first column that
    resolves, or one named `path`; anything else refuses out loud and names the column);
    edges are the **induced subgraph** over that set from `links`, with `+neighbors` to
    widen one hop on the built local-neighbourhood machinery; the existing filter
    dimensions (tag, orphans, edge kinds, local depth) then **narrow** the set, so the
    two can never disagree. Ghosts are allowed through — a ghost in a lens is a visible
    plot hole. **`relative: current` is refused for graph lenses** (a graph tab has no
    host note; inventing one from the adjacent pane is implicit context, and the error
    says so). This shrinks the item from "invent a lens format" to "query + the
    edge-kind display layer", and it folds **canvas-as-saved-selection** (P6.10) into
    the same mechanism — that item becomes one more query. Measure the *sim* over a
    large selected set before promising a vault-wide lens; the 5k paint numbers
    (2.3–2.6 ms) are not the constraint, the force layout is.

- **P4.17 One note, one buffer per window (the registry)** — **added 2026-11-06 by the
  P4.0b design pass; it is P4.0b's first commit, not a later cleanup.** A per-window
  `path → Entity<Buffer>` registry that tabs *and* embeds borrow from, so a note is open
  once per window whether it got there by tab or by embed. Today `MarkdownEditor` holds a
  `Buffer` by value and each tab owns its editor, so two views of one note means two
  copies of the text with two undo stacks and two dirty flags. Prerequisite: M0, lifting
  `display_segments`, `folds`/`fold_cache` and `read_only` out of `Buffer` into per-view
  state (they are per-view today — two panes at different widths wrap the same text
  differently, and one note can be Preview in a tab and Live in an embed). `dirty` moves the
  other way, onto the entry. `apply_disk_changes` then walks the registry instead of the tab
  list, and both write paths (`write_tab`, `autosave`) cover entries no tab holds.
  **Landed 2026-11-06** (`crates/app/src/note.rs`, `Workspace::note_entry`). Two changes
  to the sketch, forced by the code rather than chosen: the entry holds
  `Rc<RefCell<Buffer>>` and not `Entity<Buffer>`, because the vim engine is handed
  `&mut Buffer` and `&mut ViewSlot` in one call and a gpui `Entity` cannot reach the
  window's other state from inside its own update closure; and the registry holds
  `Weak` handles, so a note dies when the last view lets go and no close path has to
  remember to unregister it. `dirty` moved off `MarkdownEditor` onto the entry, and
  `read_only` moved the other way, onto `ViewSlot` — preview is a property of a view,
  and on a shared buffer it would otherwise let a reading tab lock an editing one out
  (pinned by `preview_one_tab_does_not_lock_the_other_tab_of_the_same_note`).
  `apply_disk_changes` and the vault-wide rewrite update the entry once and then tell
  every view of it (`reload_views_of`). Still open, and owned by P4.0b's M4: the two
  write paths reaching entries no tab holds.
  **Worth landing on its own:** it is also what makes a preview pane beside a live editor,
  the same note in two panes, and cross-window consistency possible at all, and it retires
  the last-writer-wins hazard between a dirty tab and an outside write.
  **Followed up 2026-11-11, and the follow-up was not cosmetic:** that `Weak` design made
  every jump a data-loss door. A jump reads the arriving note into the tab on screen, so
  jumping out of a note nobody had written yet dropped its last view — the buffer died,
  the note left the bar, no dot lit for it, and `ctrl-o` back served the file on disk.
  The rule now sits in one place, `Workspace::load_target`: the tab on screen takes the
  arriving note unless it is the **only view** holding a note with unsaved edits, in
  which case the arriving note gets a tab of its own. `Workspace::views_of` counts views
  across panes, because the buffer belongs to the window — `ctrl-t`'s copy and
  `ctrl-w S`'s split make a note held twice safe to jump away from, which is what keeps
  the rule from growing a tab for nothing. `step_jump` goes to the tab that already holds
  the jump's destination rather than loading a second copy of it.

**Exit criteria:** restore any note to any retained point; a `.shq` embedded in
three notes renders live and refreshes on edit; tag rename rewrites every note
and the DB; "notes the agent modified this week" returns the right set.
**Added with P4.0/P4.9:** `![[a#^b]]` embeds one block, three deep at most, and the
fourth level shows a badge instead; a dirty embed cannot be lost silently; a deleted
note comes back from the trash with its links intact.

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
  **Half done 2026-11-08:** the card is built — resting the pointer on a wiki-link
  shows the target's first lines, markers hidden, resolved through the embed resolver
  so a section link previews its section and an open buffer wins over the file; it
  hangs off the link's row with the popup's placement rules and never takes the
  keyboard. The **mini graph neighborhood waits on the minimap (M5j)**, which has
  never been built: the card is text-only rather than a second graph renderer
  invented under it.
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
  **Registry ruled 2026-11-08:** profiles are a named registry keyed by `type:` / callout
  kind, **inferred-first and override-second** (`.shale/profiles.toml`), with **zero
  built-in profiles beyond the three named above** — the artifact vocabulary
  (`letter`, `handbill`, `inventory`, `sheet`, `session card`) is vault data, matching the
  zero-built-in-vocabulary rule already held for edge kinds. Screen-only profiles are
  cheap; the print path is where the bill is.
  - **P6.5a PDF font pipeline — its own item** (ruled 2026-11-08, scheduled). The real
    cost of the print sink is not layout: it is **glyph outlines and subsetting**. The
    screen path has gpui's font system; a PDF writer does not, and embedding nothing
    means the export is only as good as the reader's fonts. Scope: face selection from the
    profile, cmap + glyph run extraction, subset embedding, and the metric match that
    keeps the PDF identical to what the screen laid out. Land it before promising any
    multi-column print profile, and bench a page against the screen layout rather than
    eyeballing it.
- **P6.6 Tree-sitter code highlighting** — goal. **Ruled ✅ (2026-10-28): C
  bindings are fine here** — the user wants the developer-friendly feature; the
  SQLite-style exception now covers tree-sitter (bundled C, build-time).
  Use `tree-sitter` + `tree-sitter-markdown` (or per-language code-block
  grammars) for fenced-block highlighting. **A consumer already exists (2026-11-09):** the
  body of a `.shq` query note (P4.3) is a language the editor has no spans for; its fences
  and `--` comments are painted by the query work itself, and its keywords belong here
  rather than to a second hand-written scanner.
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

## Phase 6 addendum — the audit's orphans (2026-11-05)

Ruled designs that had no item anywhere. Small, mostly free rides on machinery that
already exists; listed here so they stop being invisible.

- **P6.11 `shale://` URI scheme** (brainstorm ✅). Open a note from outside the app —
  browser, launcher, a script. The live MCP port already answers `shale://status` and
  `shale://access` as *resources*; this is the other direction: register the scheme
  with the desktop and route it to the running window (or start one). The vault→window
  registry (`VaultServices` + the window registry) is the whole plumbing.
- **P6.12 Random note** (brainstorm ✅). `:random` opens a note at random, optionally
  inside the current graph filter or tag. Trivial over the index; the only decision is
  what the pool excludes (orphans? attachments? the note you're in?).
- **P6.13 Copy / paste / duplicate in the file tree** (brainstorm 🔭). The tree has
  `n` new, `r` rename, `shift-d` delete — nothing else, and users ask for it. Needs a
  ruling on what "duplicate" names the copy and where a pasted note lands (same folder,
  prompt, or the pane's current folder). Duplicate is the cheap half; clipboard-across-
  windows is the part that needs thought.

---

## Phase 7 — The GM toolkit (ruled designs, no schedule yet)

The D&D/GM cluster is designed in `brainstorm.md` and had **no roadmap presence at
all** — the 2026-11-05 audit found seven ruled features with no item. They are grouped
here because they share machinery (`.shq`, block references, computed surfaces, the
table model) and because they are the clearest thing Shale does that Obsidian does not.
The scope guard is binding: **a narrative combat tracker, not a VTT** — no grid, no
token-pushing, no range/LOS math.

- **P7.1 Initiative tracker** (brainstorm ✅) — a mode over a plain markdown table: the
  table *is* the state, an `initiative` keymap context on those lines, the renderer
  paints whose turn it is. Not a new surface.
- **P7.2 Inline dice** (brainstorm ✅) — `!dice 2d20+4` in a note; the roll is written
  back into the line (user ruling: dice results may mutate the source file;
  rolls-only-in-DB was offered and declined).
- **P7.3 Random generators** (brainstorm ✅) — the vault structure *is* the table: a
  folder of notes is a random table, frontmatter carries weights. No new table format.
- **P7.4 GM screen** (brainstorm ✅) — mostly saved queries (P4.3) plus a pane flag;
  the "screen" is a layout, not a feature.
- **P7.5 Spotlight ledger** (brainstorm ✅) — who has been on stage recently, over the
  event log; feeds the graph's freshness glow rather than duplicating it.
- **P7.6 Secret blocks** (brainstorm ✅) — a callout kind the agent profiles cannot
  read (`config/profile.rs` already scopes by path; this scopes by block), and the
  graph/search surfaces must respect it, not leak the text through a preview.
- **P7.7 Agent GM assistant** (brainstorm 🔭) — mostly already built as MCP tools; what
  is missing is the shaping, not the machinery. **Ruled 2026-11-08: the recap stays out.**
  It is a prompt over that machinery, not a surface — no session-capture mode, nothing
  pings the agent from the app, and **no recap sub-item is scheduled**; the broadsheet
  (P6.5) reads the GM's own session note instead.
- **P7.8 Combat log as a table** (brainstorm ✅, ruled 2026-11-08) — the session note's
  fight log is a markdown table (`Who | What | Amount | Session`), not prose: the table
  model is already built, so the feature is the convention plus the saved queries. Prose
  damage logs are refused because a parsed rollup answers *wrong forever*; a table can
  simply answer nothing until it is filled.
  **No mode** (one-special-table rule, brainstorm): shared table editing only — no keymap
  context, no active row, no renderer that knows what the rows mean. The initiative
  tracker is the toolkit's only table with a mode.
- **P7.9 Dice probability** (brainstorm ✅, ruled 2026-11-08) — `!p 2d8+4 vs AC16` renders
  hit %, average damage and crit range by exact convolution over the `!r` grammar
  (P7.2), pure Rust in core. Writes **nothing** back: a probability is a property of the
  expression, not an event. Order statistics for `khN`/`klN` are the part the tests pin.
- **Pointers on existing items:** P7.1 gains the ruled `:inspect` (one `![[…#statblock]]`
  line under the table, rewritten rather than stacked — see brainstorm "Initiative
  tracker"); P7.4 gains the **session spine** vocabulary (`campaign:`/`session:` props,
  `:session 12`) and the **party ledger as a saved query** — the write-back ledger is
  refused under the manual-tables parent rule.

- **P7.10 Session-linked chips (cue + counter) and relative queries** (brainstorm ✅,
  ruled 2026-11-08) — `.shq` frontmatter gains `relative: current` (a bound `:current`
  parameter over the existing `links` table; **the anchor path joins the cache key**), and
  two callout kinds ride built callout machinery: `> [!cue]` (heading-derived anchor,
  optional `for: ^ref` override, click copies the track name, **no notification, no
  scroll motion, no playback**) and `> [!counter]` (bar painted from `n / m` text; click
  edits **the ranges the chip painted**, one counter per callout, ambiguous body = no
  affordance, identity-based tests). Scene cards and the GM screen's per-session lists are
  the same query embedded, not features. **Gates: P4.3 (queries), P4.3b (headings index,
  for section grouping), P4.9 (the `for:` override); the GM cockpit is P3.7.**

**Dependency order:** P4.3 `.shq` → P4.9 block refs → P4.15 computed surfaces → P7.1 /
P7.3 / P7.4. P7.6 gates on the same provenance/filter work as P4.8. **P7.8 is the one GM
item buildable today** (table model + command registry are shipped); its *queries* wait
on P4.3/P4.3a. P7.9 waits on P7.2's dice grammar and nothing else. **P7.10 waits on P4.3
+ P4.3b**, and P4.3b is deliberately sequenced after P4.3a rather than folded into it.

---

## The next vim pass — deferred keys

A standing list, so a key decided against during one pass does not have to be
re-litigated in the next one. Each entry is a deferral, not a ruling against the
feature.

- **`ctrl-p` → vault search.** Deferred 2026-11-09, when the operator search landed
  and the user was told the key is free. It is genuinely unbound today: vim's
  `ctrl-p` (up a line) is not implemented here either, so nothing is being
  protected. It goes in during the next vim pass, where the pass that owns the
  motion can also decide whether `ctrl-p` should be *the motion* instead — vim
  users arriving from another editor will reach for it as quick-open regardless.
  One keymap row plus the `docs/keymap.md` entry when it lands.



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
- **A ✅ in `brainstorm.md` with no roadmap pointer is a defect, not a backlog entry**
  (ruled after the 2026-11-05 plan audit). The round that agrees a design must also
  name the phase item that owns it, in the same commit — otherwise the feature is
  invisible to whoever implements next, however firmly it was ruled. The audit found
  28 such features, including editable transclusion, which `PROJECT.md` calls a pillar
  of the app's advantage. **Audit recipe:** list `brainstorm.md`'s headings with their
  status markers, then grep each against `roadmap.md`'s `**P…` items *and* against
  `crates/` for code; ✅ with neither is an orphan, and keyword matching alone
  over-reports (a dependency-map mention is not an owner).

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
7. **What `:random` excludes** (P6.12) — orphans, attachments, the note you are
   standing in, archived folders? Trivial to change now, annoying to change once
   people's muscle memory depends on the pool.
8. **Tree copy / paste / duplicate semantics** (P6.13, still 🔭) — what a duplicate is
   named, and whether a paste lands beside its source, in the pane's current folder, or
   asks. Needs a ruling before it is built; the user asked for the feature on 2026-11-05
   and it has no design yet, only a gap.
9. **Daily-note defaults** (P4.11) — folder, filename format, and whether the template
   hook is P6.4's variable syntax or a plain file copy. Deciding the format late means
   migrating a year of notes.
10. **Embed badge vs stub** (P4.0a) — **partly settled in code (2026-11-06):** a
    too-deep embed and an embed already on the chain show a name badge (the author's
    alias if they gave one); a *broken* embed and an anchor that names no heading show
    nothing at all, leaving the `![[…]]` line as written, because raw text is the honest
    placeholder and an empty block reads as a successful embed. Still unsettled: whether
    a broken embed should get its own badge, a distinct one, or offer `gD`-style
    creation — that is P6.9's pass, and the code leaves the room for it.
11. ~~Editable-embed edge cases~~ **RESOLVED 2026-11-06** (Q1-Q8, `transclusion-plan.md`
    §7): a dirty embed with no tab **materialises a tab**; the same note embedded twice
    **renders both** — different sections side by side is the point — with the entered block
    editable and the others views that **badge when their section stops resolving**, no
    polling (the body cache keys on `Buffer::change_id`); clicking host text **exits**;
    `:rename` acts on the caret's note and `:w <name>` **refuses** inside an embed; an
    entered section that vanishes under the caret **exits with a status line** while a view
    block badges — *the caret decides*; agent proposals inside embeds are **deferred to
    P4.2**; `:q` closes the host's tab; and the entered embed does **not** ride the session
    file.
12. **GM key vocabulary** (P4.3a, and every GM-toolkit query after it) — the vault in
    hand mixes cases and spaces (`Type`, `AC`, `Max HP`, `First Downs`), and keys are
    case-significant with spaces legal. **Recommendation:** lowercase single-word keys as
    the convention (`type`, `cr`, `hp`, `ac`, `weight`, `region`, `campaign`, `session`),
    declared in `<vault>/.shale/props.toml`; existing mixed-case keys keep working and a
    key rename is the migration path, never a forced rewrite. Search quoting, not
    escaping: `prop:"Max HP"<=5`.
13. **`cr: 1/4` and declared-number normalization** (P4.3a) — YAML reads `1/4` as text,
    so a numeric `cr <= 5` query silently drops CR ¼ monsters, which is guaranteed to bite
    a D&D vault. **Recommendation:** for keys declared `number`, normalize a bare `n/m`
    at index time (`kind='number'`, decimal in `value`, the authored text kept for
    display) rather than asking authors to write `cr: 0.25`.
