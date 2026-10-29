# Graph View Implementation Plan

**Goal:** Render the vault's wiki-link graph as an interactive tab — 2D first, projected 3D later — with force physics, filters, and keyboard navigation, drawn entirely through GPUI's paint API.

**Architecture:** A `graph` module in `shale-core` owns the data (petgraph adjacency built from the in-memory vault index) and a hand-rolled force simulation (spring + repulsion + centering, Barnes-Hut quadtree when it pays off). The app side is a `GraphView` entity rendered as a tab; all drawing goes through GPUI's `canvas()` element — `paint_quad` circles for nodes, batched `paint_path` strokes for edges, glyph painting for labels. No custom wgpu.

**Depends on:** the Tabs goal ("I want it to just be a tab. This won't be built until after tabs are built"). Everything in M1 below assumes a tab host exists; the core-crate work (M1a) does not and can start any time.

---

## Research findings (verified against the vendored gpui at tag v1.25.3)

Source of truth: `~/.cargo/git/checkouts/zed-a70e2ad075855582/8dd255c/crates/`.

**GPUI can draw the graph; custom wgpu is not required and not available.**

- `gpui::canvas(prepaint, paint)` (`gpui/src/elements/canvas.rs`) is the official escape hatch: a plain element whose paint callback gets `&mut Window`, i.e. the raw scene API, inside the normal layout tree.
- The Linux renderer is `gpui_wgpu` (via `gpui_platform`/`gpui_linux`), but it is **private**: there is no public accessor for `wgpu::Device`/`Queue` and no way to inject a render pass. The one external-surface primitive (`PaintSurface`, `gpui/src/scene.rs:768`) is macOS-only (`CVPixelBuffer`). Custom wgpu ⇒ fork gpui ⇒ reject.
- Scene primitives available from `Window` (`gpui/src/window.rs`):
  - `paint_quad(PaintQuad)` (window.rs:4684) — instanced, batched, the same pipeline every UI div uses. The WGSL shader computes rounded corners with an SDF (`gpui_wgpu/src/shaders.wgsl:362 quad_sdf`), so **a quad with all `corner_radii` = half its width is a crisp AA circle**. One instance per node. This is the workhorse.
  - `paint_path(Path, color)` (window.rs:4755) — lyon-tessellated fill/stroke (`gpui/src/path_builder.rs`: `move_to`/`line_to`, `StrokeOptions`, dash arrays). **Caveat:** paths rasterize through an intermediate pass (`wgpu_renderer.rs draw_paths_from_intermediate`); cost scales with path count, not vertex count. Batch all same-color edges into **one** `PathBuilder` (many subpaths) — target ≤ ~10 paths per frame.
  - `paint_glyph` (window.rs:4821) / `TextLayout::paint` — atlas glyphs for labels, alpha for fade.
  - `paint_image`/`paint_svg` exist but are irrelevant here.
- Scene capacity: instance buffers grow to 256 MB (`wgpu_renderer.rs:19 MAX_INSTANCE_BUFFER_SIZE`) with a "scene too large" guard. At ≤2k nodes (target scale) we are ~4 orders of magnitude away. No concern.
- Input: `ScrollWheelEvent` (`gpui/src/interactive.rs:522`), `on_mouse_down`/`on_mouse_up`/`on_mouse_move` on divs, `window.on_mouse_event` for raw events while a view is active. Keyboard via the existing keymap/focus-handle machinery.

**Physics.** Obsidian's graph is d3-force: many-body repulsion, link springs, center force, collide — its Forces panel exposes exactly center/repel/link strength/link distance. We replicate that model ourselves (full control for the constellation forces in M3, no JS-parity baggage). At ≤2k nodes, naive O(n²) repulsion is ~2M pair ops/tick — fine at 60 Hz in release; the Barnes-Hut quadtree is a cheap upgrade and stays in the plan as a switchable path.

**Crates.** `petgraph` (MIT) for adjacency only. Simulation is ours. No `fdg_sim` (no hook for cluster/constellation forces).

---

## Visual & interaction spec (agreed)

- **v1 = 2D global graph** as a tab: pan (drag / arrows), zoom (wheel / `+`/`-`, cursor-anchored), hover highlights a node's connections (Obsidian-style dimming), click opens the note, right-click context menu (open, reveal in tree, local graph later).
- **Node look:** circle, radius `base * (1 + sqrt(degree))` scaled by the Node-size setting; fill from the theme palette; hovered/selected get a ring. Labels fade in past a zoom threshold (Text-fade), drawn with `paint_glyph`, capped to the top-N-by-degree when zoomed out.
- **Edges:** one batched stroke path, theme-muted color; edges touching the hovered node drawn in a second brighter batch. **Arcs by default** — quadratic beziers (`PathBuilder::curve_to`, verified in `path_builder.rs:137`) curving gently away from the graph center; straight lines are a toggle. Arrows are an option (M4).
- **Filters (v1):** orphans on/off, existing-files-only, tags on/off (tag nodes are graph nodes linked to their note), search-term filter (reuse `core::fuzzy` against note names/paths). Filters re-heat the sim.
- **Keyboard:** `Tab` focuses the graph pane; `+`/`-` zoom, arrows pan, `Shift` = 3× speed; `Enter` opens the hovered/selected node; `Escape` returns focus to the editor. Graph keys live in a dedicated keymap context (`graph`) so they don't collide with vim normal-mode keys.
- **Physics behavior:** sim runs on a background thread at fixed timestep while the tab is open; alpha decays to a settle threshold and the sim **sleeps** (no redraws); a vault edit, filter change, or node drag re-heats alpha. Static mode (M3) computes a deterministic layout once and caches it.
- **3D (M2):** same sim in 3D, projected with a rotation matrix + perspective divide, painter's-algorithm sort (far→near), depth → node radius/alpha. Pure `paint_quad`/`paint_path`; no depth buffer needed.
- **Constellations (M3):** folder-attraction force + per-folder anchor points arranged on a ring/grid; parent-folder filter re-anchors children; nested constellations = recursive anchors.
- **Heatmap (M4):** node color from open/edit counts in the SQLite DB (see note below).

## Data source

Built live from the in-memory vault index: `core::markdown::extract_wikilinks` + `Vault::resolve_link` over `Workspace::entries`, same resolution rules as backlinks (shortest-unique-name, case-insensitive fallback). **Note for later: when the `.shale` SQLite DB lands, move edge extraction to the DB's incremental index and keep only the sim/rendering in memory.**

Edge semantics:
- Dedupe A→B and B→A into one undirected edge (petgraph `UnGraph`), but keep a directed multiset alongside for M4's arrow display.
- Unresolved wiki-links: **ghost nodes** (rendered dimmer, smaller) unless "existing files only" is on. This handles duplicate-note-name weirdness by resolving exactly as `resolve_link` does, and dangling links become visible structure instead of vanishing.
- Attachments as nodes: behind the same existing-only/tag-style toggle (v1: include image-link edges, dimmed).

## Performance & memory budget

- Node state as SoA arrays indexed by petgraph `NodeIndex`: positions/velocities `[f32;2]`, degree `u32`, color `u32`, anchor `u8` ⇒ ~48 B/node. **2k nodes ≈ 100 KB; 10k ≈ 500 KB.** Edges ~16 B each. Negligible; the 200 MB test budget is never in play from the graph itself.
- Per-frame CPU: transform + cull (off-screen nodes/edges skipped via the viewport rect) + sort for 3D. 2k nodes: trivial.
- Draw calls per frame: ~3–5 (node quads batched by color run, 2 edge paths, label glyphs). Node colors sorted once per filter change so the quad run-batching stays coherent.
- Sim thread: fixed 60 Hz timestep, alpha decay ~0.0227/tick (d3 parity), sleeps at alpha < 0.001. Positions ship to the UI thread as one `Vec<[f32;2]>` per tick via a gpui channel — 16 KB/tick at 2k nodes, garbage-collected per frame.
- Hit-testing: uniform spatial-hash grid rebuilt on settle (and on drag), O(1) per hover event.
- Labels: only nodes with screen radius > ~6 px get labels; hard cap 300 labels/frame, highest degree first.

### Idle contract (verified against the v1.25.3 backends — implementers, do not guess here)

- **No frame unless dirty.** gpui core gates drawing on `should_draw = invalidator.is_dirty() || force_render` (`gpui/src/window.rs:1884`). Never `cx.notify()` from a timer unconditionally; every continuous animation must be the thing that marks dirty, and it must stop when it has nothing to animate.
- **Platform frame loops differ.** Wayland (`gpui_linux`): demand-driven — the window parks on a `wl_callback` after present and wakes only on damage or compositor frame callback; settled + unfocused = true 0% CPU. Windows (`gpui_windows/src/platform.rs:437 begin_vsync_thread`): a `VSyncProvider` thread blocks on `DwmFlush()` and `RedrawWindow(RDW_INVALIDATE)`s every visible (not iconic) window each refresh — so a settled idle window still takes a ~60 Hz wakeup + dirty-check (sub-1% CPU, no scene rebuild, no present). Minimized windows are skipped. Don't "fix" this in app code; it's platform behavior.
- **Built-in throttle is not enough.** gpui already caps inactive windows to ~30 fps (`window.rs:1816`, `FrameRateLimit::Inactive`) and thermal-caps to 60 fps — but our animations keep the scene dirty, so they'd churn at 30 fps unfocused. All continuous animation must subscribe to window activation: `window.is_active(cx)` (`window.rs:7476`) + the activation observer (`Window::activation_observers`, surfaced via the existing activate/deactivate channels in `window.rs:1681-1728`).
- **On deactivate:** freeze the drift clock, stop shipping sim positions to the UI (the sim thread may keep ticking off-thread — it never touches `App`), let ripples expire without drawing, pause orbit rotation, pause the toast timers (same signal — one shared "window active" flag in the workspace; toasts already specced to pause unfocused in PROJECT.md). **On activate:** resume; the sim will have quietly settled in the background.
- **Drift (M5g) is the only opt-in continuous frame source.** Everything else (sim, ripples, springs, orbit) self-settles or is bounded by an explicit user action. With drift off and the sim settled, Shale must draw zero frames while idle.
- **Validation:** the Wayland frame loop is observable in logs — `wayland/window.rs` drives `FrameLoop::{Ticking, AwaitingCallback, Scheduled, Parked}`; a settled, unfocused graph tab should reach and stay `Parked`. No screenshot tool exists here, so verify idle behavior by log lines plus `top`/`pidstat` on the Shale process (expect 0.0–0.3% with drift off), and ask the user to eyeball the graph tab for motion when drift is off.

---

## Milestone M1a — core: graph index + simulation (no UI, startable before tabs exist)

New module `crates/core/src/graph/` (`mod.rs`, `index.rs`, `sim.rs`, `quadtree.rs`), `pub mod graph;` in `crates/core/src/lib.rs`. Add `petgraph = "0.8"` to `crates/core/Cargo.toml` (MIT, commercially fine — matches the project licensing rule).

### Task 1: `GraphIndex` — build the graph from the vault index

**Files:** Create `crates/core/src/graph/index.rs`; test inline `#[cfg(test)]` following `vault.rs`'s temp-vault test style.

```rust
/// A node in the vault graph: a real note, an attachment, a tag, or an
/// unresolved link target (ghost).
pub enum NodeKind { Note, Attachment, Tag, Ghost }

/// The vault's link graph: petgraph adjacency plus per-node metadata,
/// rebuilt incrementally when a note's links change.
pub struct GraphIndex {
    /// Undirected simple graph; parallel A->B/B->A links collapse to one edge.
    pub graph: petgraph::graph::UnGraph<NodeMeta, EdgeMeta>,
    /// path -> NodeIndex, for incremental rebuilds and hover/click mapping.
    pub by_path: std::collections::HashMap<PathBuf, petgraph::graph::NodeIndex>,
}

pub struct NodeMeta {
    pub kind: NodeKind,
    pub path: Option<PathBuf>,   // None for ghosts/tags
    pub label: String,           // note stem, tag name, or ghost target
    pub degree: u32,             // cached: drives node size
}

pub struct EdgeMeta { pub directed: bool } // true if links exist both ways
```

Build: iterate `entries`, `extract_wikilinks` each note's text, `resolve_link` each target; unresolved ⇒ Ghost node (deduped by raw target string). Reuse the backlinks pass in `vault.rs:403` — factor the shared "scan all notes for links" loop into one function both call (DRY; `backlinks` becomes a query over `GraphIndex`).

**Verify:** `cargo test -p shale-core graph::index` — cases: exact/stem/case-insensitive resolution matches `resolve_link` tests; A↔B collapses to one edge with `directed: true`; duplicate-name notes resolve per shortest-unique rules; ghost dedupe; self-links ignored.

### Task 2: Force simulation (2D)

**Files:** Create `crates/core/src/graph/sim.rs`.

```rust
/// d3-force-style simulation: link springs, pairwise repulsion, centering.
/// Deterministic: seeded PRNG for the initial phyllotaxis placement.
pub struct Sim {
    pub pos: Vec<[f32; 2]>,      // SoA, indexed by NodeIndex index()
    pub vel: Vec<[f32; 2]>,
    pub alpha: f32,              // re-heat by setting to 1.0
    pub params: SimParams,
}

pub struct SimParams {
    pub repel: f32,        // many-body strength (negative = repel), d3 default -30-ish scaled
    pub link_strength: f32,// spring k, d3 default 1/min(deg_a,deg_b)
    pub link_distance: f32,// d3 default 30
    pub center: f32,       // pull toward origin
    pub collide: bool,     // radius-based overlap push (keeps big nodes readable)
}
```

One `step(dt)` = apply forces → velocity Verlet with damping → decay alpha. O(n²) repulsion behind a `const NAIVE_LIMIT: usize = 2048`.

**Verify:** `cargo test -p shale-core graph::sim` — determinism (same seed ⇒ byte-identical layout after N steps); a 2-component graph separates; alpha decays below threshold; no NaNs on a single-node or empty graph.

### Task 3: Barnes-Hut quadtree (switchable at >2k nodes)

**Files:** Create `crates/core/src/graph/quadtree.rs`. Standard θ=0.9 quadtree; `repulsion(node) -> [f32;2]` in place of the naive loop. **Verify:** on a 4k-node random graph, BH force vector within 5% of naive per node; sim still settles.

### Task 4: Filters as a pure view over the index

**Files:** Create `crates/core/src/graph/filter.rs`.

```rust
pub struct GraphFilter {
    pub orphans: bool,          // show degree-0 notes
    pub existing_only: bool,    // drop ghosts
    pub tags: bool,             // include tag nodes
    pub search: Option<String>, // fuzzy (core::fuzzy) against label+path
    pub folder: Option<PathBuf>,// M3: constellation scope; None = whole vault
}
/// The subset of NodeIndexes that pass, as a bitmask for the sim+renderer.
pub fn apply(index: &GraphIndex, f: &GraphFilter) -> Vec<bool>
```

**Verify:** unit tests per toggle; search filter reuses `fuzzy` scoring tests.

Commit after each task (`feat(core): graph index`, etc.).

## Milestone M1b — app: the graph tab (after tabs land)

New module `crates/app/src/graph/` (`mod.rs` — the `GraphView` entity; `render.rs` — canvas painting; `interactions.rs` — mouse/keyboard; `panel.rs` — filter popover). Wire into the tab host in `workspace/` as a tab kind alongside file tabs; command `graph:open` in `workspace/commands.rs`; ribbon button.

### Task 5: `GraphView` entity + tab wiring

`GraphView` holds: `GraphIndex` handle (rebuilt on vault edits via the existing watch path), `Sim`, camera `{ pan: Point, zoom: f32 }`, hover/selection, filter bitmask, spatial grid. Owns the sim thread: spawn on first open (`cx.background_spawn`), stream positions back, `cx.notify()` per batch; stop when the tab closes. Re-heat on edit/filter/drag.

### Task 6: Canvas painting

```rust
canvas(
    |bounds, _window, _cx| Camera::new(bounds),          // prepaint: bounds only
    move |bounds, cam, window, _cx| {
        // 1. edges: one PathBuilder per color run
        let mut pb = PathBuilder::new();
        pb.style = PathStyle::Stroke(StrokeOptions::default()
            .with_line_width(link_thickness * cam.zoom));
        for (a, b) in visible_edges { pb.move_to(cam.w2s(a)); pb.line_to(cam.w2s(b)); }
        window.paint_path(pb.build().unwrap(), muted_color);
        // 2. hovered-node edges: second path, accent color
        // 3. nodes: paint_quad circles, sorted by color run
        for n in visible_nodes_sorted {
            let r = radius(n) * cam.zoom;
            let c = cam.w2s(pos[n]);
            window.paint_quad(PaintQuad {
                bounds: Bounds { origin: point(c.x - px(r), c.y - px(r)),
                                 size: size(px(2.0 * r), px(2.0 * r)) },
                corner_radii: Corners::all(px(r)),        // SDF circle
                background: fill(n).into(),
                border_widths: Edges::all(px(0.0)),
                ..Default::default()
            });
        }
        // 4. labels: window.text_system().paint or paint_glyph, alpha = fade curve
    },
)
```

Cull to viewport before painting; cap labels (300, degree-first). **Verify:** GPUI headless test (`gpui/src/test.rs`) opens the tab against a fixture vault, asserts frame paints and `painted_quads().len() == visible_nodes`.

### Task 7: Interactions

Wheel zoom anchored at cursor (`world = (screen - pan)/zoom`, adjust pan to keep the point under the cursor fixed); left-drag on background = pan; left-drag on node = pin+drag (node's position follows, sim re-heats, release unpins unless Shift-pinned); click = open note in a file tab; hover = set hover index (spatial grid hit-test), `cx.notify()`; right-click = context menu via existing popup machinery (`workspace/popups.rs`). Keyboard context `graph`: `+`/`-`/arrows/Shift/`Enter`/`Escape` per spec. **Verify:** headless tests for zoom-anchor math and hit-test; manual eyeball on niri (ask the user — no screenshot tool).

### Task 8: Filter panel

Small popover (reuse popup styling) with the four v1 toggles + a fuzzy search line (reuse `picker.rs` input). Changes flip the bitmask and re-heat. Persist filter state in the TOML config under `[graph]`. **Verify:** toggling orphans off removes degree-0 quads (headless quad count).

## Milestones M2–M4 (later, in order)

- **M2 Local graph + 3D.** Local graph = BFS depth-N subgraph from the active note (depth slider). 3D = same sim in `[f32;3]`, rotation via drag, perspective divide, painter's sort; `+`/`-`/arrows unchanged.
- **M3 Constellations + static mode.** Folder anchors + attraction force; parent-folder filter; nested anchors. Static mode: run sim to convergence off-thread, cache layout (positions keyed by path) in the `.shale` DB when it exists, else a `.shale/graph-layout.json` sidecar; deterministic seed ⇒ stable across launches.
- **M4 Heatmap + groups + arrows.** Heatmap colors from DB open/edit counts; Obsidian-style color groups (search-term → color rules); arrowheads on edges (second batched path). (Time-lapse dropped as novelty — superseded by the vault history scrubber in brainstorm.md.)

## Milestone M5 — signature features (approved; ordered as sub-milestones)

These are what make Shale's graph not Obsidian's. All are circles, curves, gradients, and alpha — no new render tech.

### M5a: The thread (jump-chain filament)

Wire the existing ctrl+o/ctrl+i jump-chain history (`workspace/` jump tracking) into `GraphView`: a ring buffer of the last ~12 visited node indexes with timestamps. Render as a **glowing polyline** through those nodes (bezier through the chain points, drawn as 2–3 stacked strokes: wide low-alpha accent under, thin bright over = fake glow), brightest at the present end, alpha decaying back through history. Beads: small filled dots at each visited node; click a bead to jump back along history. Updates live as the user navigates. Cheap: ≤12 nodes, one path batch. Headless test: chain of 3 opened notes ⇒ filament path exists with 3 beads.

### M5b: MCP pulses + freshness glow

The MCP server (crates/mcp) already informs the app of edits (toasts feature). Extend that event to the graph: each externally-changed path spawns a **ripple** — an expanding hollow ring (one `paint_quad` circle outline per ripple, radius growing, alpha decaying over ~1.5 s, max ~20 concurrent ripples). Ripples are render-only: they never touch the sim, so an agent editing 50 files causes zero physics churn. If the graph tab is open while the agent works, the vault visibly shimmers where it's writing. Test: push edit event, assert ripple list length and expiry.

**Freshness glow (replaces replayed ripples for closed-tab edits):** edits that happened while the graph tab was closed are NOT replayed on open (a 50-edit burst would explode). Instead, nodes with recent DB edit timestamps get an age-decaying halo (edits newer than ~10 min glow; intensity = f(age)). This is derived state from the DB, not event replay — and it shares the age→color machinery with M5i (link-age) and the M4 heatmap.

### M5c: Real constellations (MST star-lines + labels)

On top of M3's folder anchors: per constellation, compute a **minimum-spanning-tree** of its nodes (Prim, ~50 lines, edge weight = screen distance) and draw it as a dashed batched path (`StrokeOptions` dash array — verified in `path_builder.rs`) in a faint constellation hue: the "star lines" of an asterism. Label the folder name at the cluster's center of mass in small caps (glyph painting, letter-spaced). Nested constellations: each sub-cluster's MST draws inside the parent figure. Recompute MST only on settle or filter change, not per frame.

### M5d: Nebulae (cluster backdrops)

Soft hue blobs behind each constellation. **API note (verified):** `BackgroundTag` has only `Solid | LinearGradient | PatternSlash | Checkerboard` — **no radial gradient**. Two workable routes, pick in a spike:
1. **Layered-shadow nebula (preferred):** one `paint_drop_shadows` call per cluster with a large `blur_radius` and low-alpha hue — the shadow SDF on a big rounded quad *is* a soft blob. ~10 clusters = 10 shadow primitives.
2. **Baked sprite:** render a radial-gradient PNG once into the asset system, `paint_image` it scaled/alpha'd per cluster.
Nebulae sit at the bottom of the draw order, dim with the rest when a filter or search dims the graph. Toggleable; hue per folder from a stable hash of the folder path.

### M5e: Graph motions (hjkl + f-jump)

With the `graph` context focused, the graph is a buffer:
- **`hjkl`**: move selection to the nearest visible node in that direction — angular cone filter (±30° of the direction vector) over the spatial grid, nearest by distance. Selection gets the accent ring; the camera pans minimally to keep selection on screen.
- **`f`-jump**: press `f`; the ~30 nearest visible nodes get tiny two-letter labels (chord-style, letters avoiding home-row conflicts); typing the pair jumps selection there. `Escape` cancels. Reuse the label glyph path with a distinct high-contrast style.
- `Enter` opens the selected node (already specced); `gd` semantics match the editor's. These verbs compose with the thread (M5a): `ctrl-o`/`ctrl-i` walk the filament.

### M5f: Search as illumination

Change the search filter's *render* semantics: matches keep full color and gain a halo (soft shadow in accent), non-matches dim to ~10% alpha instead of vanishing. The sim is untouched (all nodes still participate — no re-heat on search scrub). Scrubbing the term turns lights on/off across the vault. The old hide-behavior stays available as the "filter" mode toggle; illumination is the default in the graph tab.

### M5g: Ambient drift (togglable)

After settle, add seeded per-node noise to *rendered* positions only (never the sim state): `pos + A*sin(t*f_i + p_i)`, A ≈ 0.2–0.4 px, periods 3–8 s, per-node frequency/phase from the same seeded PRNG as placement. Render-only ⇒ deterministic screenshots, no re-heating, no interaction with drag/pinning. Costs one sin per axis per visible node per frame (use a 256-entry sine LUT). **Config: `[graph] drift = true`**, off by default on the "performance" preset; the drift loop also caps at 30 fps when nothing else needs a frame.

### M5h: Orbit focus

Click-hold (or `o` on a selection) enters orbit: the selected note's neighbors are pulled onto a ring of radius R around it by a **soft target force** (spring toward ring position, angular positions spread evenly by current angle so nodes don't swap violently). Key constraint from the user: **exiting orbit must not bounce the graph.** Implementation:
- Orbit force is added as an *extra* force with its own strength that ramps in/out over ~0.5 s (ease curve), never by teleporting nodes.
- On exit, ramp strength to zero while alpha stays low (reheat only to ~0.1, not 1.0) — the base forces reassert gently; the layout was never destroyed, just bent.
- The selected node itself is pinned during orbit; its ring neighbors keep their sim velocities so release momentum is continuous.
- Slow ambient orbit: ring positions rotate ~2°/s while focused (the target points move, springs carry the nodes) — reads as a slow moon orbit, not a carousel.

### M5i: Link-age coloring (togglable)

Config/filter toggle: edges colored by link age — the date the link first appeared between the two notes. Source: the `.shale` DB link-history table (add `first_seen` when the incremental indexer first records an edge; pre-DB fallback: file mtime of the linking note). Palette: old links in theme-muted, new links warm accent (a two-stop lerp; edges are batched by color run — quantize to ~8 age buckets so batching stays coherent). Same bucket machinery later feeds the heatmap node colors.

### M5j: Corner minimap (always-on neighborhood)

A small (~180×120 px, resizable, position: bottom-right of the editor, over the status bar area but not over it) always-visible mini-graph of **just the active note's neighborhood**: the note at center, direct links (depth 1; depth 2 at a stretch), same node/edge drawing as the main graph at miniature scale (smaller radii, no labels except the center note). Lives in the editor view, not the graph tab — it's ambient orientation: "where am I, what's around this note." Click a mini-node to open it; click the minimap itself to open the full graph tab centered on the current note (orbit-focus it). Rendering: reuse the graph's paint helpers with a second camera; the neighborhood subgraph is cached per active note (BFS depth-1 is O(deg)), so cost is ~deg quads + one path per frame. Toggleable in config (`[graph] minimap = true`); hides automatically in narrow windows.

### Revisit-later note (parked, not scheduled)

**Garden mode** (note-health coloring: stubs/orphans as seedling rings, tended notes glowing warm — "what to write next" rather than "what you wrote most"). User liked the idea but wants to revisit after the DB (word counts, edit counts) exists. Park here; pull into M4/M5i territory when the DB lands.

---

## Files touched (summary)

- Create: `crates/core/src/graph/{mod,index,sim,quadtree,filter}.rs`, `crates/app/src/graph/{mod,render,interactions,panel}.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml` (petgraph), `crates/core/src/vault.rs` (share the link-scan pass), `crates/app/src/workspace/{mod,commands,render}.rs` (tab kind + command + ribbon), `crates/app/src/main.rs` (keymap context `graph`), config schema (`[graph]` section)
- PROJECT.md: add a "Graph view: rendering via GPUI canvas (no custom wgpu); sim in core; tab host required first" note — **do this in a separate commit; another agent may have PROJECT.md open, re-read before writing.**

## Risks / tradeoffs

- **GPUI path batching discipline is the perf cliff.** One `paint_path` per edge kills frame time; batched subpaths don't. Enforce in code review: never call `paint_path` inside an edge loop.
- **`corner_radii` circles are SDF-exact** — verified in `shaders.wgsl` — but a node radius of 0 or NaN bounds would poison the quad batch; clamp radius ≥ 0.5 px.
- **Sim thread ↔ gpui:** positions cross via channel + `cx.notify()`; never touch `App` off-thread. If per-tick notify shows jank, coalesce to one notify per frame with the latest position buffer.
- **Ghost nodes vs. Obsidian parity:** Obsidian shows unresolved links as nodes; we match, but the "existing only" toggle must also drop edges *to* ghosts, not just the nodes (dangling edges are the classic bug).
- **Tabs dependency:** M1b is blocked on the tabs goal; M1a is not.
- **petgraph version drift:** pin `0.8`; the vendored zed tree doesn't use it, so it's a fresh download — fine, MIT.

## Open questions — all resolved (see brainstorm.md for full rationale)

1. **Tag-node coloring:** stable hash of the tag name → hue (no manual coloring). Constraint: hues must be visually distinct and strong enough — minimum saturation/lightness band, no repeated muted greys.
2. **Drag-pin:** unpin on release (node rejoins the sim); Shift-drag pins permanently.
3. **Time-lapse:** skip the plain creation-date animation (pure novelty). Replaced concept: **vault history scrubber** (brainstorm.md) — scrub the graph to any past date using time-machine snapshots + link `first_seen`; playback button = the time-lapse, now actually useful.

Resolved since drafting: arcs-on-by-default for edges (user approved); graph lives as a tab; 3D is projected pseudo-3D; own sim + petgraph; target scale ≤2k notes; v1 filters and keyboard as specced; freshness glow (age-decaying halo from DB edit timestamps) replaces replayed ripples for edits that happened while the tab was closed.
