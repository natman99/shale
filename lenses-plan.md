# Graph lenses — plan (roadmap P4.16)

Written 2026-11-17. Design pass for **P4.16**, whose shape was rewritten on 2026-11-08
("a lens is a saved `.shq`, not a filter format") and ruled into its final form on
2026-11-13. This file is the machinery survey, the edge ledger, the milestones and the
rulings still owed. It does not reopen the two ruled rounds.

## 1. Already ruled — not re-litigating

From `roadmap.md`, P4.16:

- A lens is **a saved `.shq`**. The query narrows the node set; the graph's display layer
  (edge kinds, hues) stays a separate concern, so the two can never disagree.
- **Ghosts are allowed through** — a ghost inside a lens is a visible plot hole, and the
  graph is where plot holes belong.
- **`relative: current` is refused for graph lenses**: a graph tab has no host note, and
  inventing one from the adjacent pane is implicit context. The error says so.
- The verb is **`:lens <name>`**, plus a **picker on the graph tab**. Lens files live under
  `<vault>/.shale/lenses/`. The tab's title reads **`Graph · <lens>`** while one is on;
  `:lens none` clears it. The last lens is **remembered with the session**. The picker shows
  each lens's **node count**.
- A lens **filters**: only the matching nodes are drawn. Not the whole vault with the
  misses dimmed — a lens is a different view of the graph, not a filter over a table.
- Measure the **force layout** over a large selected set before promising a vault-wide
  lens. The 5k paint numbers (2.3–2.6 ms) are not the constraint; the layout is.

## 2. The machinery it rides (measured against the tree, 2026-11-17)

Almost all of it exists. The new part is one dimension on the filter mask and one door.

| Piece | Where it lives | What it already answers |
| --- | --- | --- |
| Ask a saved query | `crates/core/src/query_note.rs:186` `ask(text, stem, host, conn) -> Outcome` | The whole chain — parse, anchor, ask, sort — in one call, with `Answered { name, rows }`, `Empty { name }` and `Refused { why }` (ruling 44: the sentences are written once, in core). |
| A relative query with no host note | same file, EDGE-12 | Already refused out loud for a graph tab. The lens ruling needs no new refusal code. |
| The answer's shape | `crates/core/src/db/query.rs:69` `Rows { columns, rows: Vec<Vec<Cell>> }` | Cells typed at index time; the first cell naming a note is that note (the app's rule, `crates/app/src/editor/query_view.rs`). |
| The graph's filter | `crates/core/src/graph/filter.rs:15` `GraphFilter`, `:66 apply(index, filter) -> Vec<bool>` | A keep-mask per `NodeIndex`, already chained over six dimensions (orphans, existing-only, tags, search, folder scope, local scope). Edges need no mask: an edge is drawn exactly when both ends are visible. |
| Folder scope's neighbour rule | `filter.rs:84-93` | Tags and ghosts survive a scope when a visible note links them — the shape a lens wants for tags. |
| The door that runs a query | `crates/app/src/workspace/queries.rs:81` `run_query`, `open.rs:60` `PickerTarget::RunQuery` | Reads the file (the file is the authority), refuses politely with the index off (`index.enabled = false`), draws the answer. |
| Search → SQL | `crates/core/src/db/index.rs:1588` "A search as the SQL a hand-written `.shq` would hold (P4.3 M6)" | Keep-a-search already turns a typed search into the SQL a `.shq` would hold. This is the free ride for "save this picture as a lens". |
| The graph tab's title | `crates/app/src/workspace/tabs.rs:208-211` | The chip already special-cases the graph tab (`"graph"`), because it has no file name. `Graph · <lens>` is one branch here. |
| What the graph remembers | `crates/core/src/session.rs:153` `GraphFilterSeat`, `:276` `Session::graph_filter` | Per-tab filter seat (orphans, missing, tags, search, local depth) plus `GraphSeat` for the camera. A lens name joins this struct. |
| The filter strip | `crates/app/src/graph/panel.rs` | Chips, a search line that narrows as it is typed, and its own keymap context (`graph-filter`) so `hjkl`/`f`/`+`/`-` stay live. |
| The graph's node set | `crates/core/src/graph/index.rs:733` | Notes, attachments, tags, ghosts **and queries** (`NodeKind::Query`) are nodes; `list_queries()` feeds the index. |
| The measurement tool | `crates/app/src/tests/graph/paint_bench.rs` `settled_frame(root, dim)` | Settles the sim on the real fixture vaults (`examples/graph/organic-*`, 100/1k/5k) and reports frames. |

What does **not** exist anywhere: a lens file format (none needed — it is a `.shq`), a
filter dimension holding a set of paths, a door that writes a lens, and the tab title's
lens segment.

## 3. EDGE ledger

Rows state the settled rule. Where a row is a recommendation awaiting a ruling it says
so in §5 by number.

- **EDGE-L1 — a lens is not a note and not a vault file.** It sits under
  `<vault>/.shale/lenses/`, so it is out of the tree, out of quick-open, out of the index
  and out of the agent's note tools. It is plain text and hand-editable all the same.
- **EDGE-L2 — the lens is a mask, not a second graph.** It enters `GraphFilter` as one
  more dimension and `apply()` chains it with the rest. There is no second node set, no
  second camera, no second sim.
- **EDGE-L3 — a lens combines with the strip's dimensions** (ruling 5 below). The strip's
  chips still apply inside a lens; a lens that switched the chips off would leave the
  strip lying about the picture.
- **EDGE-L4 — tags follow their notes, ghosts pass through.** A tag node survives a lens
  when a lens note links it — the rule folder scope already uses (`filter.rs:84`). A ghost
  named by a lens note is drawn (ruled 2026-11-08).
- **EDGE-L5 — an answer naming no node is counted, not hidden.** A `.shq` can answer with
  a path that has no graph node (a note the current node set excludes, an odd column). The
  strip says how many answers had no node, in the same spirit as a ghost: the picture
  should not quietly disagree with the answer.
- **EDGE-L6 — a lens that fails keeps the last good picture.** A lens file edited into a
  broken query, or a query that errors against the index, does not empty the graph. The
  strip names the lens and says it is not answering, with the database's own sentence.
- **EDGE-L7 — the lens re-runs when the index moves or the lens file changes**, the same
  rule an embedded query follows (`overlay.rs:644`). A lens that never re-runs shows a
  picture of a vault that no longer exists. But the query and the layout are two costs:
  re-running the query is one SQL call, while re-heating the layout on every index move
  would pay ~0.9 s of sim work for a 2000-node answer while the reader is typing
  (`PROGRESS.md`, 2026-11-17). So: re-run the query on an index move, and disturb the
  layout only when the answer's **node set actually changed**. Nodes entering and leaving
  go through the existing filter-change path, which already handles nodes appearing and
  disappearing.
- **EDGE-L8 — the lens belongs to the graph tab**, like the camera and the filter seat.
  Two graph tabs can wear two lenses; each is restored with its tab.
- **EDGE-L9 — `:lens none` is a name, not a mode.** Clearing the lens is the same
  dimension set to `None`; there is no "lens off" state distinct from "no lens".
- **EDGE-L10 — the picker's counts are measured when the picker opens**, one query per
  lens on the read-only connection. A lens that fails shows `—` rather than a count, and
  the row still opens the lens (EDGE-L6 keeps the picture).
- **EDGE-L11 — a lens whose file is deleted while it is on** leaves the picture standing
  and says the lens is gone; the next `:lens` or a restart clears it. Deleting a lens is
  not an event that reaches into a live view.
- **EDGE-L12 — the index being off is a refusal, not an empty lens** (EDGE-14's rule,
  already in `run_query`): with no index there is nothing to answer from, and "no note
  answers this" would be a lie about the vault rather than about the setting.
- **EDGE-L13 — a lens does not make a graph tab a host note.** `relative: current` stays
  refused for a lens (ruled), and the lens's own answer is the only context a graph tab
  gets.
- **EDGE-L14 — the count in the tab title and the count in the strip come from one
  answer.** One producer, as the embed's badge rule requires: a title that re-derived the
  number could disagree with the strip.
- **EDGE-L15 — a filter change that does not heat the layout still has to ship a frame.**
  Found building M3: `rebuild_graph` re-slots the sim and `pump_graph` starts the tick
  loop, but `pump_graph` returns at once when the sim is already **settled** — and a lens
  that only removes nodes can leave it settled. The view then keeps the old picture and
  the old counts while the mask underneath has changed. `refresh_graph` is the door that
  ships summary, frame and filter whatever the sim's heat; M4's live re-run (EDGE-L7) has
  to go through the same door, not the strip's.

## 4. Milestones

Split by dependency; each ends with the gate green, its own tests, the falsifications
named under it, and the docs it owes.

- **M0 — the measurement, before anything is promised. Closed 2026-11-17.** The probe ran
  on `examples/graph/organic-1k` and `organic-5k`, keeping every *k*-th note so the
  selected set is spread rather than one neighbourhood, and the table is in `PROGRESS.md`
  ("What the force layout costs over a selected set"). The finding is a **bound**: settle
  cost is superlinear in the kept set (5 ms at 100 notes, 280 ms at 1000, 900 ms at 2000,
  4.3 s at 5000), but a selected set never costs more than the whole vault — the
  5000-of-5119 row and the whole-fixture row are the same number — and the whole vault is
  what the graph tab settles today. Ticks stay at 300 whatever the size, so the cost is
  per-tick work: 1.0–1.5 ms per tick at 1000 kept, 16–18 ms at 5000. A lens is always
  cheaper than the picture it replaces, which retires the cap question (ruling 7) and
  leaves the layout half of EDGE-L7 as the thing to build carefully.
- **M1 — the dimension. Closed 2026-11-17.** `GraphFilter.lens: Option<LensScope>`
  (`name` + the answer's vault-relative paths), chained in `apply()` after folder scope
  and **anding** with it, tags and ghosts decided by the one neighbour pass folder scope
  already had (EDGE-L4), and the kind chips and the search still cutting inside the answer
  (EDGE-L3). Six core tests: exactly the answered notes, tags and ghosts following their
  notes, an edge standing only where both ends survive, the chips and other scopes
  composing, an empty answer keeping nothing (unlike a local scope whose centre is
  missing), and an answer naming a path with no node keeping the rest. Six mutations, all
  red (`falsify_lens.py`): replace-not-and, dimension inert, answer ignored, tags/ghosts
  forced off, the neighbour pass not knowing about the lens, and an empty answer treated as
  no answer. The `inert` mutation exposed a weak assert on the way: the combine test was
  green with the dimension entirely absent until it also asserted the note that only the
  lens hides.
- **M2 — the lens files and the reader. Closed 2026-11-17.** `crates/core/src/lens.rs`:
  `dir` (`.shale/lenses`), `list` (stems, sorted; a folder, another extension and a hidden
  file are not lenses), `read` (a name is one name — separators refused **before** the
  name-shape guard, a missing lens named in its own sentence, a trailing `.shq` forgiven),
  `answer` (through `query_note::ask` with `host = None`, so EDGE-L13's refusal is `ask`'s
  own sentence and not a lens-invented one), `paths_from` (the column by name — `note`,
  then `path` — falling back to the first, and a cell that is not text is not a path), and
  `unmatched` against the `GraphIndex` (EDGE-L5). Fourteen core tests; ten mutations, all
  red (`falsify_lens_files.py`). Two weak spots caught on the way: the column rule was
  indistinguishable from "take the first column" until a test put `note` second, and
  `../secret` was being refused only by accident of the dot guard sitting in front of the
  separator guard — which made the first battery run report a false red. The guards are now
  ordered on purpose, with the reason in a comment.
- **M3 — the verb and the picker. Closed 2026-11-17.** `:lens <name>` puts a lens on the
  graph, `:lens none` takes it off (EDGE-L9 — and saying it with nothing on says so rather
  than pretending to undo), and `:lens` alone opens the picker (ruling 5). The registry row
  `graph.lens` and the palette row open the same picker rather than acting, so the palette
  never wears a lens nobody chose. The picker's rows carry what each lens answers **now**
  (EDGE-L10), measured on one read connection so two lenses cannot answer against two
  snapshots; a lens whose query will not run shows `—` and is still choosable, and a lens
  already on offers `no lens` as the first row. The file is read on use, so a hand edit
  takes effect the next time. Ten app tests (`tests/lens.rs`) cover the verb, the picker,
  the palette row, and the refusals: EDGE-L5's count-out-loud, EDGE-L6's standing picture,
  EDGE-L12's index-off sentence. Twelve mutations, all red (`falsify_lens_door.py`). The
  one real finding: the first version re-made the picture with `rebuild_graph` +
  `pump_graph`, which is what the strip's own path does — and every test that checked the
  **counts** stayed at three notes, because a settled sim means `pump_graph` has nothing to
  tick. That is EDGE-L15, and `refresh_graph` is now the door.
- **M4 — the picture and the title.** The graph view taking the mask, the tab chip reading
  `Graph · <lens>`, the strip naming the lens with its count, and EDGE-L6/L7's live
  behaviour (a failing lens keeps the picture; an index move re-runs it).
- **M5 — the door that makes one.** `s` in the filter strip saving the current picture as
  a lens through the existing search→SQL translation, `:lens save <name>`, and the
  hand-edit path staying true (the file is the authority, re-read on use).
- **M6 — session and docs.** `GraphFilterSeat.lens`, restore, `docs/graph.md`'s lens
  section, and the roadmap item closed.

## 5. Not here

- **Canvas-as-saved-selection (P6.10)** — the same mechanism, one more query; it lands
  with the canvas.
- **The GM screen (P7.4)** — a screen is mostly saved queries plus a pane flag; it waits
  on this item's machinery, not the other way round.
- **MCP control of the lens.** No live tool sets or reads a lens this round. The one-line
  door later is `live_status` naming the lens a graph tab is wearing.
- **Lenses committed to the vault.** The ruling puts them under `.shale/`, so they are
  local state. A vault-committed lens (a `.shq` marked as one) is a separate ruling, not
  an extension of this one.
- **Remembering folder scope in the session.** `GraphFilterSeat` has no `folder` field
  today; adding one is unrelated to lenses and is not slipped in here.
- **Dimming.** Ruled out: a lens hides, it does not dim.

## 6. Rulings — all ruled 2026-11-17

The user answered all nine in one reply; every recommendation was taken. Ruling 7 was
settled by the measurement rather than by taste.

1. **How a lens is made. RULED.** `s` in the graph's filter strip saves the picture as a
   lens, using the search→SQL translation keep-a-search already has
   (`db/index.rs:1588`), plus `:lens save <name>`. The strip's chips (orphans, missing,
   tags) are **not** encoded into the query — they stay filter dimensions, so a lens does
   not freeze a chip choice into a file the author will later want to edit by hand.
2. **Combine, not replace. RULED.** The lens is one more dimension (EDGE-L3): it says
   which notes belong; the chips still decide whether unlinked notes, ghosts and tags are
   drawn inside it.
3. **Tags follow their notes. RULED.** A tag survives a lens when a lens note links it —
   folder scope's rule, so the two scopes behave alike (EDGE-L4).
4. **Re-run on an index move or a lens-file change. RULED.** The embedded query's rule
   (EDGE-L7), with the layout half: the layout is disturbed only when the answer's node
   set actually changed.
5. **`:lens` alone opens the picker. RULED.** Mirrors `:query`.
6. **Answers with no node are counted, not dropped. RULED.** Reported in the strip
   (EDGE-L5).
7. **No cap, no warning. RULED** (and answered by M0 first). A selected set never costs
   more than the whole vault, which is what the graph tab settles today; the node count in
   the strip is the disclosure.
8. **One lens per graph tab. RULED.** Restored with its tab (EDGE-L8).
9. **The strip gets a lens row. RULED.** Lens name, node count and the no-node count —
   that is where the graph already says what it is drawing.
