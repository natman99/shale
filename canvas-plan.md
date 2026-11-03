# Canvas Plan (ruled 2026-11-03; Phase 5/6 addendum, C1–C3)

Reopens the parity-audit "deliberate out" for Canvas (brainstorm.md, Obsidian
parity audit section) — this file IS the reopen ruling; the audit line gets a
pointer when brainstorm.md is free to touch (hooks owed, §10).

## The concept, one line

A canvas is not a whiteboard — it is a **second projection of the vault**: the
same node model as the graph view with a *pinned* layout, live buffer-backed
content, and typed edges that write real vault facts. The competitor framing
this beats: Obsidian/ZenNotes canvases are *pictures of* vault data; a Shale
canvas *is* vault data.

## Standing rulings from the discussion (binding)

- **Topology in, geometry out.** Agents never emit coordinates. The MCP canvas
  surface speaks semantic verbs; the simulation owns placement (§6).
- **A node is a view, never a copy.** Canvas note-nodes render from the same
  buffer entity the editor uses; the inline editor attaches to the real buffer
  (shared cursor, undo stack, dirty dot). No sync protocol — live by
  construction.
- **Scope guards (user-confirmed):** no freehand pen/sketching (shapes and
  arrows only), no webview/Excalidraw, no stored coordinates for pure graph
  views (computed stays computed), no agent coordinate access ever.
- GM-side posture unchanged: boards are GM tools, never player-facing.

## 1. Format: jsoncanvas

Adopt the Obsidian jsoncanvas spec (open, plain JSON, `.canvas`) for interop:
*Shale opens your Obsidian canvases as-is.* Extensions ride the spec's
custom-key allowance under a `shale/` prefix.

- **The file is a normal buffer.** `.canvas` is plain JSON text: it opens in
  the editor, vim-editable, Raw-mode honest. The 2D canvas view and the
  outline view (§5) are both *projections of the parsed buffer*; edits from
  any view are buffer edits; undo is buffer undo; the time machine covers
  canvases for free (it snapshots vault files). "The table is the state" —
  the modes-over-hidden-state principle applies verbatim.
- **Round-trip fidelity:** unknown keys (Obsidian plugin cruft) are preserved
  on save, never dropped — same philosophy as frontmatter.
- **Malformed JSON** (user vim-edited it into brokenness): the canvas view
  keeps the last good parse, marks itself stale, and says so in the status
  bar — the broken-theme posture (warn, never reject). The text view stays
  fully editable so the user can fix it.
- Node kinds mapped from spec: `text` (markdown string), `file` (note card /
  image / PDF-icon card), `group` (rectangle), `link` (URL card). Shale
  extension kinds: `query` (a `.shq` rendered as a card grid), `smart-group`
  (tag/query filter, members computed live, §5).

## 2. Architecture: reuse the graph renderer

One renderer, two layout modes. The graph tab already ships camera
(`pan_by`/`zoom_at`/`fit`), hit-testing, node drag, batched edge strokes,
glyph labels, culling, and the keyboard-takeover model on GPUI `canvas()`.

New module `crates/app/src/canvas/` (mirrors `graph/`): `mod.rs` (the
`CanvasView` entity — a tab kind alongside file and graph tabs), `render.rs`
(node rectangles, markdown LOD, edge strokes), `interactions.rs`,
`parse.rs` (JSON buffer ↔ canvas model, incremental reparse on buffer edit).
Core side: the canvas *model* (nodes/edges/groups as data) lives in
`shale-core` next to the graph module — the graph and canvas share the node
model, the quadtree, and the sim.

- **Layout modes:** pinned (coordinates in the file — canvases are always
  pinned) vs computed (the graph). `:canvas tidy` runs the sim over canvas
  nodes and writes settled coordinates back as a buffer edit (undoable).
- **Sim extension needed:** rectangular collision (canvas nodes are boxes
  with aspect ratios; the sim's collide is circular). Approximation:
  radius = half-diagonal is acceptable for v1; true rect-rect collide is a
  later refinement.
- **Groups:** geometric containment, center-inside wins; a group's rectangle
  auto-expands to cover members (Obsidian convention); nested groups allowed,
  border indents per level. Group rectangles move with their members during
  tidy and agent-relax (centroid + resize).

## 3. Canvas files are vault citizens (ruling recommended, §9-Q3)

- The DB indexes links **out of** `.canvas` files: a canvas mentioning a note
  appears in that note's backlinks with a `canvas` badge.
- **Rename propagation must parse `.canvas` files** — a note rename rewrites
  node file paths inside them. This is the single most important edge case in
  the whole plan: a rename that silently breaks canvases poisons the feature.
  The link machinery (LinkResolver, rename scan) treats `.canvas` as a
  link-bearing file type from day one of C1.
- **Ghost nodes:** a node whose target note is missing renders dashed (graph
  ghost convention). Accepting an agent-proposed ghost node creates the note
  (consistent with ghost-link behavior).
- **Canvas→canvas links** (file node pointing at a `.canvas`) render as an
  opening card, never nested embedding — recursion guard.

## 4. Edges: two kinds (the load-bearing distinction)

- **Vault edges** — a canvas edge *backed by a real typed link* (frontmatter
  link property, the typed-edges ruling). It carries `shale/type` + the
  identity of the backing link. It appears in backlinks, graph, and `.shq`
  queries *because it exists in the vault*, not because it is drawn. Deleting
  a vault edge on the canvas proposes deleting the backing frontmatter link
  (goes through the normal edit path, provenance-stamped). Deleting the
  frontmatter link makes the drawn edge a ghost (dashed) — same ghost
  machinery as links.
- **Local edges** — canvas-only decoration (plain jsoncanvas edges, no
  backing link). They never enter the graph or the DB link tables. They are
  how the GM draws "and then this happened" annotations without polluting
  vault semantics.
- Edge labels render the type word for vault edges (design-language: color +
  glyph, never color alone).

## 5. Live content

- **Note nodes render from the live buffer** — keystroke-to-keystroke,
  debounced to frame coalescing. LOD: zoomed out → title + first lines
  (cached); zoomed in → full markdown render, reusing the editor's inline
  markdown render machinery with viewport clipping. Overflow clips with a
  fade + indicator, never scrolls inside the node.
- **Inline editing:** double-click attaches the shared editor overlay to the
  **real buffer** — a canvas card and a split pane on the same note share
  cursor, undo, dirty dot. Full vim *inside* a node is a v2 luxury; the
  overlay is the v1 story.
- **Query nodes** render a `.shq` result as a card grid, live (rides the DB
  watcher). **Smart-groups** are groups whose membership is a saved filter —
  members computed, not placed; agent and tidy can lay them out but the
  membership is never stored.
- **Block-level cards:** a note node can target a block anchor (the
  transclusion machinery, 3-level nesting cap applies).
- **Hover preview** on a node = the hover-preview machinery (first lines +
  mini neighborhood). `Enter` opens the note; `g` opens the local graph
  centered on the node.

## 6. The agent layer (MCP canvas tools)

Rule: **the agent proposes topology; the simulation owns geometry.**

- **Semantic verbs only:** `canvas_open(path)`, `add_node(link, near:, side:)`,
  `stack_below(anchor)`, `row_after(anchor)`, `group([notes])`,
  `link(a, b, type)` (creates a *vault* edge = frontmatter link proposal),
  `canvas_snapshot` (returns a row/column outline, never coordinates).
- **Quantized slots** if position is exposed at all (coarse grid, small
  integers). Occupancy enforced server-side with the existing quadtree —
  overlap is unrepresentable, not discouraged.
- **Auto-relax** after any accepted agent batch: collision-only sim ticks
  that move **only agent-added/agent-moved nodes**. User-placed nodes never
  move without an explicit `:canvas tidy`.
- **Review strip:** canvas edits flow through P2.5 (TOML pending files,
  apply-or-stale, retract). An accepted batch is **one undo unit** (receipt
  posture — undo restores the whole board, and names files the user edited
  after the run, ZenNotes-workflows-style).
- **Concurrent agents:** per-agent pending files, aggregated in the strip —
  no new mechanism.
- **Staleness:** user edits the canvas while a proposal is pending → normal
  apply-or-stale rules.

## 7. Keyboard & commands

`f` hint-jump between nodes (M5e pattern), `hjkl` traversal, marquee select,
`g` group selection, `:canvas tidy`, `:canvas align h|v`, `:canvas
distribute`, `:canvas new`, `:canvas from-query <name>` (§8), `:canvas
outline` (toggle the text projection: indented outline — groups nest, nodes
are lines, edges are `key::` suffixes; fully vim-editable, two-way with the
2D view). Drag-and-drop from the file tree onto a canvas places a card
(free ride). Yank a node = copy its wikilink.

## 8. Free rides and GM boards

- **`:canvas from-query`** — run a `.shq`, lay the results out with the sim.
  This is the GM board *without any agent*: the agent is just the
  natural-language wrapper around this one command. Prep, tavern boards,
  recaps all reduce to query → board → typed edges.
- **Canvas as saved selection:** graph filter "show only nodes on canvas X" —
  a canvas is a hand-curated neighborhood, free via the filter machinery.
- **PNG/PDF export** of a canvas (renderer already paints everything; PDF
  print presets exist) — GM prints a session board as a handout.
- **Board templates:** starter canvases (session board, relationship map,
  recap) as plain files in a gallery, ZN-recipes-style: copy into vault, x
  hides one.
- **Minimap** shared with graph M5j. Canvases appear in quick-open/palette
  with a badge. Time-machine scrubber covers canvases for free (file
  snapshots).

## 9. Edge cases ledger (the careful part)

1. **Rename propagation parses `.canvas`** — non-negotiable, C1 day one (§3).
2. **Broken JSON** → last-good render + bar warning; text view always
   editable (§1).
3. **Unknown keys preserved** on save (plugin interop) (§1).
4. **Vault-edge ↔ frontmatter link lifecycle** — delete either, the other
   ghosts; never orphan silently (§4).
5. **Local edges never enter graph/DB** (§4).
6. **Auto-relax never moves user-pinned nodes** (§6).
7. **Group containment is geometric** (center-inside); straddlers belong to
   neither; groups move as units in tidy/relax (§2).
8. **Note deleted while its inline editor overlay is open** → overlay closes,
   node ghosts. **Note deleted while canvas open** → ghost node, hover shows
   "target missing", Enter offers create.
9. **Same note in two canvases / canvas + split** → all views ride one buffer
   entity; no state divergence possible by construction (§5).
10. **External edit of `.canvas`** (Obsidian, git pull) → watcher reloads the
    buffer; unsaved local canvas edits hit the existing external-change
    conflict policy — no new mechanism.
11. **Agent proposes a node for a nonexistent note** → ghost proposal;
    accept creates the note (§3).
12. **Canvas node → canvas file** → opening card, no nested embedding
    recursion (§3).
13. **500 live-markdown nodes** → LOD + per-node render cache invalidated on
    buffer change + off-screen culling (graph already culls). Budget: canvas
    idle = 0% (graph idle contract applies verbatim).
14. **Tidy on image-heavy canvases** → rect collide approximation
    (half-diagonal) is v1-acceptable; images keep aspect, sim treats boxes.
15. **Undo across views** → one buffer, one undo stack; tidy/agent-batch/
    drag are each one undo unit.
16. **Query node whose `.shq` is deleted** → node ghosts with the query name.
17. **Smart-group membership is never stored** — only the filter; tidy lays
    out members, membership recomputes on every DB change (§5).

## 10. Milestones

**C1 — viewer + interop (the demo).** jsoncanvas read/write (fidelity rules),
CanvasView tab, note/image/group/text/link nodes, LOD render, pan/zoom/select/
drag, `.canvas` link indexing + rename propagation, ghost nodes. Depends on:
tabs (P3.1), images (shipped). Mostly graph-code reuse.

**C2 — editing.** Edge authoring (ports, arrows, labels), inline editor
overlay on the real buffer, snapping + grid, marquee, align/distribute,
tidy + rect-collide sim extension, outline view, drag-from-tree. Where the
time goes.

**C3 — the differentiators.** Vault-backed typed edges, query nodes,
smart-groups, block cards, MCP canvas tools + auto-relax + review-strip
integration, `:canvas from-query`, canvas-as-selection graph filter, PNG/PDF
export, templates, GM board gallery.

## 11. Open questions — rulings needed

- **Q1 Two edge kinds as specced (§4)?** Recommended yes — it is the whole
  differentiator and the only way local decoration and vault semantics can
  coexist.
- **Q2 `.canvas` as a plain editable buffer (§1)?** Recommended yes — it is
  the modes-over-hidden-state principle; the alternative (opaque structured
  entity) breaks outline, vim, and time-machine-for-free.
- **Q3 Canvas files as vault citizens (backlinks badge, rename parses them,
  §3)?** Recommended yes, hard.
- **Q4 Auto-relax moves only agent nodes (§6)?** Recommended yes.
- **Q5 Accepting a ghost node creates the note (§3)?** Recommended yes
  (consistent with ghost links).
- **Q6 Phase placement:** C1 after tabs (P3.1) as early as Phase 3 addendum;
  C2 Phase 5; C3 with its dependencies (typed edges, `.shq`, block anchors).
  Recommended: schedule C1 in the Phase 3 addendum list next to P2.6–P2.8 so
  the demo lands while motivation is hot.

## 12. Hooks owed (when brainstorm/roadmap are free to touch)

- Flip the parity-audit "deliberate out: Canvas" line to a pointer:
  "reopened 2026-11-03 — see canvas-plan.md".
- Roadmap: add C1 to the Phase 3 addendum list; C2/C3 as Phase 5/6 items
  with dependency notes (tabs, typed edges, `.shq`, block anchors).
- brainstorm.md: the standing rulings list at the top of this file becomes a
  dated canvas section; cite the parent scope guards (no webview, GM-side
  only) rather than restating them.
