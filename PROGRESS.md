# Shale — Progress (graph track)

This file belongs to the **graph agent** (working from `graph-view-plan.md`).
PROJECT.md belongs to the other agent; we keep our log here to avoid edit
conflicts. Design detail: `graph-view-plan.md`; placement: `roadmap.md` P3.3.

## Status: M2 core math complete — dimension-generic sim + projection

`crates/core/src/graph/` — new module. Touched elsewhere: `pub mod graph;` in
`crates/core/src/lib.rs`, `petgraph = "0.8"` (MIT, pure Rust) in the workspace
and core `Cargo.toml`. No app files touched.

### What's there (M2 additions)

- **`sim.rs` is now `Sim<const D: usize>`** — one codebase, monomorphized
  per dimension. `Sim<2>` is the flat graph (quadtree repulsion);
  `Sim<3>` is the 3D layout behind the projected view (octree). Same
  forces, same tuning, same determinism guarantees. Seeding: phyllotaxis
  spiral in 2D, Fibonacci sphere in 3D. `positions()` returns
  `&[[f32; D]]`.
- **`quadtree.rs` is now `Tree<const D: usize>`** — the flat-cell,
  iterative-stack Barnes-Hut generalizes to 8 children for D=3 (quadtree
  uses 4 of the 8 slots; `1 << D` is not a stable const-generic array
  length, so the children array is fixed `[u32; 8]`). θ=0.7 validated
  against the naive loop in both dimensions.
- **`proj.rs` — `Cam3` + `Projected`**: the projected pseudo-3D camera.
  yaw/pitch/pan/zoom/dist; per-point rotate → perspective divide →
  screen transform. `project_all` fills a caller-owned buffer (no
  per-frame allocs); `paint_order` is the painter's-algorithm sort
  (far→near); `screen_to_world_at_depth` inverts a screen point at a
  fixed *view-space* depth — that's the drag invariant while the world
  rotates under the cursor. Depth cues (fade/fog) are a scalar factor on
  `Projected.scale`; hit-testing projects to 2D and reuses `geom::Grid`.
- **Hot-loop fix**: the sim's inner loops indexed `self.pos[i][d]` per
  axis — double bounds-checking in the hottest code in the module.
  Destructuring once (`let pi = self.pos[i]`) roughly halved link+
  repulsion cost at n=5000 (naive repulsion 68.7→? see table; BH
  repulsion-only 4.9→5.6ms is noise-level, the win is in springs/
  collide).

### M2 verification

- `cargo test -p shale-core --lib`: **295 passed, 0 failed** (6 ignored
  benches). New coverage: 3D determinism, 3D settling, octree mass
  conservation in both dims, BH-vs-naive accuracy in 3D, projection
  round-trips (project→screen→world at fixed view depth), painter order
  monotonicity, behind-camera culling.
- clippy clean (the `needless_range_loop` allow on sim.rs is justified in
  a comment — const-generic dim loops can't be iterator-rewritten).
- Peak RSS: **7.3 MB** (was 6.8; the 3D tests add octrees, still
  nowhere near the 200 MB rule).

### M2 perf (release, 2026-10-28)

Mid-drag tick (reheat + one step, settle excluded — the earlier
`bench_tick` accidentally averaged the settle in; fixed):

| n    | 2D tick  | 3D tick  |
|------|----------|----------|
| 500  | 0.54 ms  | 0.81 ms  |
| 2000 | 5.14 ms  | 7.32 ms  |
| 5000 | 25.5 ms  | 33.0 ms  |

3D costs ~40% more than 2D (one more axis, 8-way octree). At the ≤2k
target both fit the 16.6 ms frame with room for painting. Settle
(300 ticks): 2D n=2000 1.48 s, 3D n=2000 2.15 s.

**glam: measured, rejected for the sim, not adopted.** `bench_proj`
(dev-dep only) compares the native scalar projection against glam
`Affine3A` + `Vec3A` per point:

| n     | native  | glam    |
|-------|---------|---------|
| 2000  | 6.5 µs  | 5.5 µs  |
| 5000  | 16.3 µs | 13.7 µs |
| 10000 | 33.1 µs | 28.0 µs |

glam wins ~20%, but projection at n=5000 is **16 µs against a 16,600 µs
frame budget** — 0.1%. The 2.6 µs delta is noise next to the painter
sort and the sim tick. Not worth a runtime dependency (the tech-stack
rule says as little in the tree as possible); glam stays a dev-dep for
the bench. The projection layer stays hand-rolled scalar — it's ~10
flops/point and LLVM already vectorizes the divide.

### What's there (M1a core)

- **`index.rs` — `GraphIndex`**: petgraph `UnGraph` over typed nodes
  (`Note | Attachment | Ghost | Tag`). Link resolution goes through the
  existing `Vault::resolve_link` / `resolve_attachment`, so the graph agrees
  with the editor by construction. A↔B links collapse to one edge with a
  `bidirectional` flag; self-links ignored; unresolved targets become ghosts
  deduped by raw target; a link to a since-deleted note degrades to a ghost
  keyed by the old path (no dangling indexes). Incremental
  `set_note`/`remove_note` rebuild the arena from the per-note link map — no
  file IO, deterministic node order (sorted paths) so the sim's seeded
  placement is reproducible. `backlinks_of` answers the backlinks panel's
  question from the graph. `extract_tags` rides `mdstyle::style_spans`, so
  fences/inline code/headings are already handled by the existing scanner.
- **`sim.rs` — `Sim`**: d3-force vocabulary (Obsidian runs d3): link springs
  with d3's `1/min(deg)` strength and bias, many-body repulsion with d3's
  exact law (`delta * strength·α/d²`, magnitude 1/d falloff), centering,
  optional collide, alpha decay 0.0228/tick to a sleep floor, d3's 0.6
  velocity damping, phyllotaxis initial placement from a seeded splitmix64.
  `pinned` mask for drag/orbit. NaN-safe (coincident-node jitter, sanitize
  pass). Pure data — no gpui, no IO — so it runs on a background thread per
  the idling contract; `is_settled()` is the "stop ticking, stop dirtying"
  signal. Node radius convention: `4 + 2*sqrt(degree)`.
- **`quadtree.rs` — Barnes-Hut**: swapped in above `NAIVE_LIMIT = 256`
  (measured crossover). θ=0.7 (d3 uses 0.9; the 1/d law needs the tighter
  bound to hold 5% per node against the exact loop — validated on a
  4000-point scatter in both dimensions).
- **`filter.rs` — `GraphFilter`**: orphans / existing-only / tags / fuzzy
  search (label + path, via `core::fuzzy`) / folder scope (constellation
  seed, M3) → visibility bitmask. Edges need no mask: drawn iff both ends
  are visible, so ghost edges drop with the ghosts.

### Verification (all run, all green)

- `cargo test -p shale-core --lib`: **262 passed, 0 failed** (25 of them
  `graph::`). Coverage includes: resolution parity with `resolve_link`,
  duplicate-stem shortest-path rule, embed→attachment nodes, ghost dedupe,
  self-link orphaning, incremental set/remove, stale-link→ghost, tag
  extraction grammar, determinism (same seed ⇒ identical layout), settling,
  component separation, spring pull, pinning, degenerate/NaN graphs,
  re-heat, Barnes-Hut ≤5% vs naive, filter toggles incl. folder scope with
  tags following their notes.
- Peak RSS of the core test binary: **6.6 MB** (with the 4k-node BH
  validation) — nowhere near the 200 MB rule.
- `cargo fmt` clean; clippy warnings in the new module addressed or
  pre-existing.

### Perf notes (measured 2026-10-28, release build, `bench_sim` + `probe_*` #[ignore] tests)

Settle = 300 ticks, preferential-attachment graph. **After the quadtree
rewrite** (flat 40-byte cells w/ stored centers, iterative explicit-stack
traversal, θ 0.5→0.7 — still passes the 5% accuracy test):

| n    | naive loop  | Barnes-Hut (new) |
|------|-------------|------------------|
| 500  | 446 µs/tick | ~230 µs/tick     |
| 2000 | 7.3 ms/tick | 4.5 ms/tick (full sim) |
| 5000 | 28.7 ms/tick| 22.8 ms/tick (full sim), 4.9 ms repulsion-only |

- Crossover measured at ~250 nodes (`probe_crossover`), so
  `NAIVE_LIMIT` dropped 2048→256. Visits at n=5000: 137/point (θ=0.7).
- Old recursive BH cost ~23 ns/visit; the rewrite is ~7 ns/visit incl.
  tree build. The first bench table (in git history) showed the pre-fix
  numbers.
- **glam: see the M2 verdict above** — measured at the projection layer,
  ~20% win on a 0.1%-of-frame workload, rejected as a runtime dep.
  (The earlier "parked" note from M1a is superseded.)
- Memory: core test binary peaks at 6.8 MB RSS incl. the 4k-node BH
  validation. Keep watching it as the module grows.

### Where the remaining perf is (written 2026-10-28, after the perf pass — for later)

Assessment of whether more *easy* wins exist in the graph core:

- **2D at target scale (≤2k): tapped out.** The 1.65 ms tick at n=2000
  is ~88% Barnes-Hut traversal. The cheap constant-factors (child_mask,
  Cell shrink, θ 0.8) are spent. What remains is SoA cell layout or SIMD
  traversal — real work, maybe 1.5-2x, not easy, and pointless when the
  tick is already ~10% of the frame budget.
- **3D at n=5000: one clear win sits there, off the target path.**
  Inference from the existing benches (3D tick 25.7 ms; BH repulsion
  probe 8.9 ms; links+integrate ~0.1 ms): **3D collide is ~16 ms** —
  the same O(n²)-flavored crime as before, in a new costume. The uniform
  grid sizes its cell by the *hub* radius (`2*max_radius+2` ≈ 66 world
  units in a 5k PA graph), and in 3D cell volume scales cubically, so
  each cell holds far more points than can actually touch and most pair
  checks are wasted. In 2D that waste was tolerable (447 µs); in 3D it
  dominates. Easy fixes if the scale target ever rises past 2k: cell
  sized by median radius + a radius-class second pass, or per-point
  multi-cell insertion keyed to each point's own radius. Not worth the
  complexity now — at n=2000 the 3D tick is 5.4 ms and fine.
- **The sim is no longer where the frame goes.** At target scale physics
  leaves ~14 ms for painting; the renderer side (painter sort, path
  batching, label glyphs) is where that gets spent. The next perf pass
  belongs against the *real renderer* (M1b), measured, not speculative.
- **The cheapest wins are already banked by design**: the idle contract
  (sim sleeps at settle) and the M3 static-layout cache mean settle cost
  is paid once per vault change, not per frame.

Bottom line: stop optimizing the core; revisit only if the scale target
rises (3D collide fix above) or when the renderer exists and can be
measured.

### Notes for M1b (the graph tab — waits on tabs, roadmap P3.1)
- `GraphIndex::set_note` is the hook the watcher/save path calls; re-heating
  (`sim.reheat(1.0)`) is the caller's job.
- Node indexes are only stable until the next rebuild — the sim, hit-test
  grid, and renderer must re-read them after structural changes.
- The sim ships positions as one `Vec<[f32;2]>` per tick; `is_settled()`
  gates the last frame, then nobody dirties the scene (idle contract).
- `filter::apply` is cheap; call it on filter change + rebuild, cache the
  bitmask.

### Log

- 2026-10-28 (perf pass, systematic): measured before touching anything —
  new `bench_breakdown` splits a hot tick per force. Findings, in order of
  what they changed:
  1. **Collide was the hotspot, not repulsion**: the O(n²) pair loop cost
     13.3 ms/tick at n=5000 — more than Barnes-Hut. Replaced with a
     uniform spatial grid (cell = 2*max_radius+2, intrusive linked-list
     buckets, 3^D neighbor walk, precomputed offsets + strides, cached
     cell index, static `max_radius`). 13.3→0.46 ms at n=5000 (~29x),
     2.04→0.16 ms at n=2000. Naive loop kept as the sparse-grid fallback.
     New test `grid_collide_matches_naive` pins grid == naive pair-forcing
     (overlaps + coincident pair).
  2. **Per-edge spring bias precomputed** (`bias` vec at construction;
     degrees are static) — the per-tick division/max was waste. `degree`
     field dropped (radius+bias capture everything).
  3. **Quadtree: child_mask u8** replaces the 8-slot NONE scan (leaf test
     + bit-walk when opening). **THETA 0.7→0.8** — margin probes
     (`probe_theta_margin[_clustered]`) show 0.6% (2D) / 2.8-3.1% (3D)
     worst error on uniform AND settled-clustered layouts; 0.9 measured
     3.6% in 3D, too thin against the 5% bound. **Cell shrunk**: `center`
     removed (threaded through insertion recursion instead — only
     insertion needs it), `half` → `width2` (pre-squared for the opening
     test). 2D traverse 4.26→3.85 ms at n=5000.
  4. **`probe_build_vs_traverse`**: tree build is only ~7% of repulsion
     (0.3 ms vs 3.9-8.4 ms traverse) — the traversal is the thing, and
     3D traverse is ~2.2x 2D (more cells open at θ; inherent).
  Net mid-drag tick (2D): n=500 543→291µs, n=2000 5.14→1.66ms,
  n=5000 25.5→5.1ms. 3D: n=2000 7.32→5.0ms, n=5000 33.0→27.1ms.
  2D n=5000 now fits 60fps too (was 40fps). Settle (300 ticks): 2D
  n=2000 1.48→0.51s, n=5000 7.28→1.50s; 3D n=2000 2.15→1.58s.
  296 tests green, clippy clean, peak RSS 6.9MB (down from 7.3).

- 2026-10-28: read `vault.rs` / `markdown.rs` / `mdstyle` APIs; wrote the
  module; fixed petgraph-0.8 API drift (no inherent `degree`); aligned the
  repulsion law with d3's exactly (the first draft mixed 1/d² and 1/d³
  forms); θ 0.9→0.5 for the 5% bound; validated BH against the naive loop.
- 2026-10-28: `geom.rs` (camera w/ cursor-anchored zoom + fit, grid
  hit-test, `hjkl` cone search, local-graph BFS mask, Prim MST for
  constellations) and `hues.rs` (OKLCH tag hues: stable hash, 20° root
  separation, family windows for nested tags, chroma-pull instead of
  clipping). Fixed an MST bug the new test caught (tree endpoints mixed
  array positions with world indices).
- 2026-10-28: perf pass on the quadtree after benching (see Perf notes):
  flat cells + iterative traversal + θ 0.7 → 6.2x repulsion speedup at
  n=5000; NAIVE_LIMIT 2048→256 at the measured crossover. glam evaluated
  and parked (notes above).
- Shared-target-dir note: the other agent is active; checked for running
  cargo builds before each of mine, used the default `target/` only.
- Heads-up (9a78761): that commit swept in the staged `vim.rs ->
  vim/mod.rs` rename that was already in the index (staged by the other
  agent's split work). Content intact, 281 tests green — just know the
  rename lives in my commit, not theirs. `scripts/split_vim.py` was left
  untracked for them.
- Scope note: staying in `crates/core/src/graph/` — no `app/` files
  until the graph tab is unblocked (user instruction, 2026-10-28).
- 2026-10-28 (M2 core math): `Sim<const D>` + `Tree<const D>`
  dimension-generic (quadtree→octree, fixed `[u32; 8]` children since
  `1 << D` isn't a stable array length); new `proj.rs` (Cam3,
  perspective divide, painter sort, drag inverse at fixed view-space
  depth — the first test caught the inverse being written against
  world-z instead of view-z); hoisted `pos[i]` out of the per-axis
  loops (double bounds-checking was ~2x the spring cost); fixed
  `bench_tick` averaging the settle into the "mid-drag" number; benched
  glam vs native projection and rejected glam (tables above). glam is a
  dev-dep for the bench only.
- 2026-11-02 (spread tuning, user ask: nodes too clustered): `SimParams`
  `repel` -90 → **-200** (the only default changed). Measured with a
  temporary `probe_spread` (mean nearest-neighbor distance + RMS radius
  from centroid, `pa_edges` at n=500/2000, settled layouts):
  repel -90 → nn 25.3/24.2, rms_r 301/597; -150 → 31.8/30.7, 387/763;
  -200 → 36.4/35.0, 442/877; -260 → 40.2/40.6, 497/996 (n=500/2000).
  `link_distance` 34→44 moved nn only +2% and `center` 0.02→0.01 +2% —
  repulsion is the spread lever, spread scales ~sqrt(|repel|), settle
  time unchanged (300 ticks everywhere). Chose -200 (+44% nn, +47%
  radius): labels were colliding at -90, -260 sprawls too much. Probe
  removed after measuring; all 61 graph tests green. The app passes
  `SimParams::default()` verbatim (`crates/app/src/graph/sim.rs`), so
  the core default is the single lever — a Forces panel (settings)
  would expose these four numbers 1:1.
- 2026-11-03 (thread 39, the OOM): **the diverging layout was the spring
  law, not the repulsion** — it predates the -200 change (A/B: uniform
  springs diverge at BOTH -90 and -200; degree-divided settles at both).
  Diagnosis (measured on the 5k fixture): per-node effective spring
  stiffness `k_eff = sum strength*bias-share over edges` reaches **21.8**
  (tag hubs, maxdeg 468) against the explicit-Euler stability bound
  **k < 2(1+d)/d = 5.33** at damping 0.6 — above it the integrator's
  eigenvalue goes negative with |lambda| ~ k*d, i.e. oscillating growth
  ~12x/tick (matches the span bouncing 1e12 -> 6.7e8 -> 2e10). 3644/5261
  nodes were over the bound; the 100-fixture has no hub big enough, which
  is why it always looked fine. Fix: `apply_links` now divides each
  spring by `min(deg_a, deg_b)` — d3's actual law, the one
  `link_strength`'s doc comment already promised; it caps k_eff near
  `strength` for any graph shape. Guard rail: `integrate` clamps |vel| to
  2000/tick (healthy 5k settle peaks ~850, so it never binds; it turns
  any future exponential blow-up into linear growth the app's grid bound
  survives). Regression test `the_1k_fixture_settles_and_stays_bounded`
  (non-ignored, 3.7s for the whole graph suite). Also landed with it:
  `collide_pad` param (default 2.0 = old behavior) and `Camera::fit`
  framing the 90th percentile instead of the full bbox — outliers were
  shrinking the core to an unreadable knot (the user's spread complaint;
  probe showed force tuning alone can't help under a fit camera:
  nn/rms is ~constant across repel/pad/link_distance variants).
- 2026-11-03 (graph-open delay, core half of PROJECT.md thread 41):
  `GraphIndex::build` resolved every link with a linear scan over the
  entry list (`Vault::resolve_link`/`resolve_attachment`), quadratic in
  vault size. Measured on the 5k fixture (release): build **18.17 s**,
  of which scan+resolve 18.18 s and file reads only 61 ms — ~0.9 ms per
  link (19,975 links × 5,000 entries × 2 passes, the stem pass allocating
  `to_lowercase()` per entry; ghosts pay both passes and miss). Fix:
  `LinkResolver` in `vault.rs` — four hash tables (exact path, lowercased
  stem, lowercased full path, lowercased name-or-stem) built once per
  entry slice, O(1) per lookup; `Vault::resolve_link`/`resolve_attachment`
  now delegate to it so the Obsidian resolution rules live in exactly one
  place, and `Vault::backlinks` builds one resolver instead of one scan
  per link. `GraphIndex::build`/`set_note` build the two resolvers
  (notes, attachments) once. After: build **89 ms** (scan+resolve 34 ms,
  reads 94 ms) — ~200x. Drift guard: `link_resolver_ties_suffix_and_
  attachment_rules` pins the tie rules (shortest path wins, scan order
  breaks exact ties), the `.md`-suffix fallback, and the attachment
  name/stem/backslash rules; the existing vault+graph tests cover the
  rest. Also fixed a latent bug the old scan had: a non-UTF-8 file name
  bailed out of the whole attachment resolution (`?` inside the loop);
  the table skips such entries instead. Core 441 tests green; app
  compiles unchanged (the app half — backgrounding the scan — landed
  separately in thread 41).
- 2026-11-03 (design language): ruled the visual-design round and wrote it to
  a **new** root file, `design-language.md` (not brainstorm/roadmap — the app
  agent was mid-flight; §12 lists the hooks owed there). Ruled whole: additive
  `Palette` roles (`diff_add/delete/changed`, `agent`, `ui_disabled`,
  `graph_ghost/tag/attachment`) optional in TOML with computed fallbacks —
  P2.5a renders against `diff_*`, so that's the one that bites first;
  `SPACE_1..4` beside `CHIP_RADIUS`; one hover/selected/focused state grammar
  for every surface; elevation = border + bg-shift + dim, no shadows (gpui
  honesty); shared chrome type constants; motion tokens + a
  chrome-doesn't-animate-except-toasts contract (graph M5a untouched); glyph
  policy = formalize the unicode set already in use (`▸ ▾ ● ×`) in `style.rs`,
  drawn marks allowed **only** for callout types / graph node kinds / agent
  presence (the color-only-information accessibility fix), buffer content
  never icon-ified (Raw-mode posture, permanent); microcopy inherits the
  status bar's voice; contrast guard test over all palettes. Density/scale
  parked. Inventory that grounded it: chrome already carries `▾▸` (tree),
  `●` (dirty), `×` (tab close) informally; callout types and graph node kinds
  are color-only today.

## Canvas plan ruled into a file (2026-11-03)

`canvas-plan.md` (repo root, new): the Canvas reopen — the parity audit had
listed it as a deliberate out. Concept: a canvas is a second projection of
the vault (same node model as the graph, pinned layout), `.canvas` jsoncanvas
files are plain editable buffers, nodes are live views of buffers (never
copies), edges split into vault-backed typed links vs local decoration, and
the agent layer speaks topology only (semantic verbs, quadtree occupancy,
auto-relax that moves only agent nodes) — geometry is the sim's. 17-item
edge-case ledger (rename propagation MUST parse `.canvas`; broken JSON keeps
last-good render + bar warning; unknown keys preserved for Obsidian
interop). Milestones C1 (viewer + interop, Phase 3 addendum candidate) /
C2 (editing) / C3 (typed edges, query nodes, MCP tools). Six open rulings
recommended in §11; hooks owed to brainstorm/roadmap in §12 while those
files stay busy.

Canvas rulings landed (same day as the plan): all six open questions ruled
✅ (two edge kinds, `.canvas` as plain buffer, vault-citizen indexing +
rename parses `.canvas`, relax-moves-only-agent-nodes, ghost-accept-creates-
note, phase placement C1→Phase 3 addendum / C2=P5.6 / C3=P6.10). Hooks
closed: brainstorm audit line flipped + dated Canvas section, roadmap items
added. canvas-plan.md §11/§12 now read as ruled.

## P3.4 exit check + the watcher finding (2026-11-03, primary-dev takeover)

The other agent is gone; ownership notes consolidated (roadmap sequencing +
PROJECT.md). Ran P3.4's graph exit criteria on the 5k fixture with a new
`--graph` debug flag (main.rs): settled = **0 fps / 1.75 ms paint** (the tab's
own readout proves zero frames), mid-settle = **38 fps / 1.4 ms** — the beat's
16 ms timer + compositor round trip caps cadence near 40 Hz; paint holds ~8%
of the 16.6 ms budget, so capacity is not the limit. Headless
`the_beat_runs_the_layout_to_rest_and_stops` green. gpui v1.25.3 does NOT log
`FrameLoop::Parked` (internal Cell) — the idle contract's verification recipe
in graph-view-plan.md corrected accordingly.

Process idle measured 3.1% — NOT the graph: the vault watcher re-walks the
whole vault (`Vault::signature`, measured 15.6 ms at 5k / 2.8 ms at 1k /
0.4 ms at 100) every fixed 750 ms ≈ 2% duty. Fixed with a duty-cycle-adaptive
interval (`WATCH_DUTY = 300`, floor 750 ms, cap 15 s): small vaults keep the
instant 750 ms, a 15 ms walk backs itself to ~4.7 s. Re-measured idle:
**0.97%**; the remainder is the live-MCP publish beat (4 Hz) + review-strip
poll (1 Hz) — feature beats that postdate the contract's 0.0–0.3% band. The
graph's own idle contribution is zero frames. 413 app tests green.

## P3.2 tiling splits — a + b (2026-11-03)

P3.2a (`59715ec`): `Pane` owns `tabs/active/note_mru`; the Workspace holds
`panes: Vec<Pane>` + `active_pane`, and every tab operation routes through
`pane()/pane_mut()`. Pure refactor — all 413 app tests green through it.
P3.2b (`7f7c19d`): the pane tree (`workspace/panes.rs`) — Leaf/Split{Row|Column,
frac}, relative fractions, `rects()` for geometric focus, `remove_leaf` +
`renumber_after` (the tree must follow the flat list's renumbering or a leaf
names the wrong pane — caught by the `:only` test), split/close/only/equalize,
the `ctrl-w` alphabet in keys.rs, `:split/:vsplit/:close/:only` in ex.rs +
command.rs (docs/commands.md regenerated; the `window.close` label reworded
because "Close this pane, writing its notes first" fuzzy-matched the palette
query "new note"). Render: per-pane tab bars inside a recursive flex tree
(gpui has no `flex_ratio`; `flex_grow(f) + flex_basis(relative(0.))` is the
proportional share — and the root leaf needs it too, or the note's viewport
collapses: 23 tests caught that at once). Sweeps: write-all-tabs, any_dirty,
rename propagation, apply_palette, graph singleton, open_note's
find-don't-duplicate all became pane-aware. 14 new tests in tests/panes.rs;
427 app + 449 core green.

## The blank-split bug (2026-11-03, `95add8c`)

User report: opening any split left the editor blank — chrome (tab bars,
status) painted, note interiors did not; vertical splits also seemed to eat
the keyboard. Reproduced live with a `--split` debug flag + a temporary
geometry probe: each pane's editor scroll container measured **859 px wide
by 0 px tall**. The tab bar (28 px, flex_none) was the pane's entire height.

Cause: gpui does not stretch flex items the way CSS does. The split wrapper
was a plain div — Block layout, whose children ignore grow and size to
content — and the pane element only set its main axis (grow + basis). The
cross axis was never spelled out, so inside a split the pane column
collapsed to the tab bar and the editor under it got zero height. A single
pane worked because the root leaf sits directly in the pane area and its
grow/basis happened to be enough there.

Fix: `size_full()` on both the wrapper (plus `.flex()`, so its child
participates in flex) and the pane element. `flex_basis(relative(0.))`
still wins on whichever axis the parent splits along, so the fractions are
untouched.

Test lesson: the old `a_split` assertion (hit boxes from `hit_line_edges`)
passed *with the bug present* — the hit lines it read were painted in the
frame before the split. The guard that actually catches it is the editor's
layout height (`wrap_height()` via `pane_editor_height`): reverting the fix
makes the test fail with "pane 0's editor was given 0px of height". Paint
evidence from a previous frame is not evidence about the current layout.
428 app + 449 core green.

## Focused-pane rule (`a04881b`)

The palette's `ui_active_border` was documented as "the rule around
whatever the window currently has the keyboard: the focused pane of a
split" but splits never drew it. Tiled windows now frame the keyboard's
pane with it and the others with `ui_border`; single panes draw no frame.
Verified live via `--split` screenshot. Note on the sidebar: its row
highlight follows the note *on screen* (the active pane's note), so pane
switches move it — intended, not a keybinding leak; the tree's keyboard
selection is a separate bordered row that exists only while the tree has
focus.

## Panes as cards (`22b43ce`)

Panes round at `style::PANE_RADIUS` (8px): the rule curves, the tab bar
carries the top corners (`rounded_t`; chips are inset by `TAB_GAP` and
never reach the corner), the note's background carries the bottom
(`rounded_b` — on the editor's own bg div too, since gpui clips children
to rectangles, not to a parent's radii; each surface that reaches a
corner rounds itself). API note: `corner_radii` is the Style *field*; the
builders are the Tailwind-style `rounded`/`rounded_t`/`rounded_b`…
generated by `gpui_macros::styles.rs` — grepping `styled.rs` alone misses
them.

## Cards everywhere + faded cursors (`6f47f2e`)

Sidebar and status bar rounded at `PANE_RADIUS` with `gap_1` between all
cards; the sidebar's divider went (the gap is the separation). Sidebar
surface = `ui_status_bg`, not `bg`: the user's theme sets `bg == ui_bg`
and the card was pixel-invisible — chrome family reads on every theme.
Cursor: `MarkdownEditor::cursor_dim` (set by the workspace in
`pane_element` at paint time, `pix != active_pane`) → the line painter
draws the block at 35% alpha. Kept visible on purpose (other panes must
not look uneditable); the solid block + the rule mark the live pane.
Paint-time sync covers every `active_pane` write without touching ten
handlers; converges in one frame. Verified by pixels (sidebar card
5..330, gap, rule at 335) and by the `a_split` assertions.

## Preview as a reading view; vault switcher; one Shale per vault (2026-11-04)

**Preview (`f4638e6`).** The caret was the bug: it sat on hidden markdown
syntax and inline images, and motions walked it through characters the view
hides. Preview now hides the caret; `j`/`k`/arrows scroll the view (clamped
against `max_offset`, which gpui reports as a positive magnitude), `g`/`G`
jump its ends through the engine (so `g t`/`g T`/`g f` keep working), and
every caret motion and operator is swallowed. The keys a reader keeps —
leader chords, `/`, `n`/`N`, escape — still reach the engine; the branch
stands down while `awaiting_chord()` so multi-key chords survive.

**Recent vaults (`d0be06a`).** `shale_core::recent`: MRU list + `last` in
`$XDG_STATE_HOME/shale/recent.json`, capped at 10, atomic write. No-argument
startup opens `last`; the CLI arg stays. `:vault`/palette open the switcher
(recents with the current marked `here`, plus the folder dialog).

**One Shale per vault.** Exclusive `flock` on `.shale/lock` (kernel-released
on any death; pid written for humans), claimed **before the GUI exists** —
gpui's loop cannot end before its first window, so a refusal inside
`app.run` printed its message and then hung forever (the bug the user hit
twice). In-process, a registry maps vault → window: the same vault focuses
its window instead of starting a watcher/MCP twin; closing a window releases
the claim. `leave()` closes its own window (`self_window`, set at open)
instead of `cx.quit()` — otherwise switching vaults closed the app (the
other bug the user hit). Multi-window is now real: one process, one binary,
one set of threads per vault.

Tests: app 435, core 455. Live-verified: second instance refused in 4ms with
holder pid; reopen after close succeeds. Windows claim is `LockFileEx` TODO.

## Switcher follow-ups (uncommitted)

Three UX bites from using the switcher:

- **Stuck `j` after a switch.** gpui_linux runs key repeat itself: the timer
  fires into *whatever window holds keyboard focus now*, not the window the
  key went down in. Confirming the picker with a `j`-repeat in flight closed
  the asking window, focus moved, and the repeats migrated to the new
  window — it scrolled until the next keystroke bumped the repeat id. Fixed
  at the shale level by not closing the asking window (below); the gpui
  behavior stands (a window closed under a held key can still hand its
  repeats to the next focus).
- **Switching keeps the old window.** `switch_vault` no longer dispatches
  `Quit` on the asking window, and the folder-dialog path no longer calls
  `leave()`: the new vault opens beside the old, and closing the old one is
  the user's call.
- **`:` from the graph tab.** The graph key context had no colon binding, so
  the ex bar — `:only`, `:tabnext`, `:q` — was unreachable while the graph
  had the keyboard. Bound `:` → `RunCommandLine` in the graph context; the
  ex bar belongs to the workspace, not the note.

Tests: app 436 (switch test now asserts both windows stay open; new graph
test asserts the ex bar opens from the graph tab).

## VaultServices: one vault, one set of services, many windows

The piece that makes "one instance, multiple windows" true rather than just
multi-vault. The per-vault services were per-*window*: each Workspace opened
its own SQLite handle (own writer thread), started its own index pass, its
own watcher walk, and its own live MCP server — so a second window on the
same vault meant two servers fighting over one `.shale` port lock. That is
why same-vault multi-window was refused.

`workspace/services.rs`: `VaultServices` — one entity per vault, held by the
registry beside its windows. It owns the `Db` (windows clone the `Arc`), the
startup reconcile + daily backup loops, the live MCP server + snapshot, and
the watcher (one walk, fanned out to every window's `apply_disk_changes`).
Workspaces register themselves with their services (weak handles; the loops
prune and stop with the vault). The live beat publishes from every window
and the desktop-active one last, so the snapshot is where the user actually
is. `Drop` takes the MCP lockfile down when the vault's last window closes —
verified live: `mcp.lock` gone after close, reopen succeeds.

Registry: `windows` is now `vault → Vec<WindowHandle>`, plus `services`.
`open_vault_window` still focuses the first window (the switcher finds);
`open_extra_window` (`:new-window`, palette "New window on this vault")
opens another frame on the same services. The claim stays exactly one flock
per vault; it releases only with the last window.

Known wrinkles, deliberate for now: two windows on one vault share
`.shale/session.json` (last close wins) and each runs its own harper-ls.
Buffer-sharing across windows is a later step; the watcher+autosave path
already keeps them roughly in sync through disk.

Tests: app 438 (new: two windows share one services entry, closing one
keeps the vault claimed, closing both releases it).

## Per-window sessions + window geometry (2026-11-04)

The last of the multi-window wrinkles: `.shale/session.json` was one room
remembered for a whole vault, and windows never remembered where they sat.

**The file is now a list of slots.** `shale_core::Sessions { slots:
Vec<Session> }`; a `Session` grew a `window: Option<WindowGeom>` —
`{x, y, w, h, maximized}`. A file from the single-window era (a bare
`Session`, no `slots` key) reads as slot 0, so old vaults migrate by
themselves.

**The vault's services own the file** (`services.rs`): read once at startup;
`claim_slot` hands out the lowest slot no live window holds, `release_slot`
frees it when a window closes — the slot's *memory* stays in the file, so
the next window to claim it wakes up in that room. `put_session` writes the
whole file with only that window's slot changed: two windows can no longer
overwrite each other's room.

**The frame rides along.** render is the one place a `Window` is within
reach of the save paths, so each painted frame copies `window_bounds()` into
`last_bounds` (five floats; cheaper than a resize subscription), and
`save_session` puts it in the slot. On open, `build_window` reads the slot's
`WindowGeom` back: too-small or off-screen rects (the monitor it lived on is
gone) are dropped and the window opens centered; `maximized` comes back
maximized. X11 and Windows honor position and size fully; Wayland honors
the size hint and lets the compositor place the window — the data is stored
either way, the platform decides what to obey.

Verified live on niri: legacy session restored, close wrote the slots format
with the real tiled frame (`w: 2042, h: 1100`; position 0,0 because Wayland
does not report one).

**Corrected (2026-11-04): the lingering process was never real.** The note
above reported that after the last window closes the process can linger, and
that `41fa3d6` does it too. Re-measured against `784e56f` with the process
identified by its own `argv[0]` rather than by a name match, and **no exit
path lingers**:

| exit path | process after close |
| --- | --- |
| compositor close, one window | gone (< 2 s) |
| `SIGTERM` (the polled flag → `finish_termination`) | gone |
| `:q` → `app.quit` → `leave` | gone |
| compositor close, two windows on one vault, both closed | gone |
| `:q` with the graph tab open (sim beat running) | gone |
| `--split` (two panes), compositor close | gone |
| compositor close with `harper-ls` running | shale gone, `harper-ls` gone |

The session saves, the claim flock releases, the MCP lockfile goes, and the
spellchecker child goes with the parent.

**What the report was actually seeing:** `pgrep -f shale` — the check a
verifying agent reaches for — matches **its own shell**, because the command
line of every command run in this repo contains `/home/nat/projects/shale`.
With no Shale running at all it returns two live PIDs. The same trap in
reverse bit this re-measurement: a `kill` aimed at the tracked pid hit the
Hermes command wrapper, not Shale, and Shale survived it. The A/B against
`41fa3d6` was made with the same faulty check, so it proves nothing.

**The recipe, for the next verification:** name the process exactly
(`pgrep -x shale`, or read `argv[0]`), and close windows by id
(`niri msg action close-window --id N`) — the bare form closes the *focused*
window, which is usually not the one under test.

Tests: app 439 (new: two windows on one vault keep alpha/beta in separate
slots, close both, reopen, each wakes in its own room — and the frame is in
the file), core 457 (new: slots do not overwrite each other; a pre-slots
file becomes slot 0).

## P3.6 — projected 3D as a mode (2026-11-04)

The graph tab turns. `2` and `3` in the graph context, `:graph2d` / `:graph3d`
from the command line, and a palette row for each. 2D stays the default and the
flat path is untouched by the existence of the other one.

**Where the mode lives: the window, not the view.** One graph per window means
one set of positions, so "show me 3D" is a request to run *these* bodies in a
third axis, not a request for a second physics. `Workspace::graph_dim` owns it;
`GraphSim::build` is the thing that notices (`Dim` is its fourth argument) and
`Physics` — `Two(Sim<2>)` / `Three(Sim<3>)` — is the app-side seam that keeps
one of the two alive without making every caller generic over `D`. Positions are
spoken of as three axes in both arms, which is why the frame, the drag and the
mode switch all have one shape to carry. The view asks through
`Workspace::graph_set_mode`, the same never-touches-the-view door the filter
uses, because the ask starts inside the view's own action handler.

**A switch carries; it does not re-seed.** `GraphSim::build` now has four cases
instead of three: no previous, same shape, changed shape, **changed space**. The
fourth carries the arrangement by node identity, takes the missing axis from the
fresh sim's own seeded placement, and re-heats at `SWITCH_HEAT = 0.5` — above
`REHEAT = 0.3` (a flat layout has no depth to reveal, so a nudge will not open
the third axis) and below full heat (the map the user was reading is worth more
than a fresh scramble). Hand-pinned nodes stay pinned. Test
`turning_the_graph_carries_the_arrangement` reads the sim *before* the executor
runs, so it asserts the copy is exact rather than "close".

**One projection per frame, shared by the paint and the pointer.**
`crates/app/src/graph/proj.rs`: `Projection::build` projects every node of the
frame with the camera the canvas chose, indexes the projected points with the
same `GRID_MAX_SPAN` cell floor the flat frame uses, and sorts the painter's
order. The canvas hands it from prepaint to paint through a `Space` enum
(`Flat(Camera)` / `Deep(Rc<Projection>)`) and stores it in a cell the pointer's
handlers read, so what the user clicked and what the user saw are one answer.
The hit test is `GraphFrame::node_at`'s shape — including the clamp-to-box retry
that fixes thread 38 — on projected points with radii scaled by
`Projected::scale`.

**The cache learned that a camera rotates.** `cache::Key` carries a `Geometry`
(`Flat(Camera)` / `Deep(Cam3)`) instead of a `Camera`. A still projected camera
replays its link batch; an orbiting one pays the rebuild, which is the honest
cost of turning the graph rather than a thing to hide.

**Depth is real; the interleaving is not.** Nodes sort far→near and take radius
and alpha from the perspective factor. Links stay in their own layer under all
of them, because putting links between nodes by depth means one layer for both,
and gpui orders primitives in one layer by *kind* — the bug that put every link
on top of every node (thread 43). `paint_nodes3d` says so in its own doc comment
rather than letting the picture look like an accident.

**Gestures, decided rather than drifted:** the middle button pans in both spaces
(it is the pan button, and its meaning does not move with the mode); over empty
canvas the left button pans the flat view and **orbits** the projected one;
`hjkl` step in **screen** directions, because in a rotating world `l` means "the
next node to the right of what I have"; `f` re-frames and **keeps the angle**,
which is what makes it the recovery key rather than a second way to get lost.

**A binding rule changed on the way, and it is worth the paragraph.** The draft
rule said a restored camera that contains no node falls back to fit. Built that
way first — a one-shot guard in `render` — and it was wrong: the layout is
seeded and deterministic, so a restored view that misses the graph is almost
always a view the user chose before closing, and reframing under them is a fight
with a deliberate pan. The 3D case the rule was written for cannot arise once
the session keeps only the angle: the framing is re-derived from the layout every
launch, so a restored 3D view is on the graph by construction. PROJECT.md's
"2D / 3D mode rules" now says the restored camera is the user's and stays.

**Session:** the graph tab's seat grows `mode` (`"2d"` / `"3d"`) and
`orientation` (yaw, pitch — and deliberately no pan or zoom). Both optional and
skipped when absent, so every session file written before this reads exactly as
it did.

Tests: app 448 (new `tests/graph/three_d.rs`: the turn and the turn back; the
carried arrangement and the surviving pin; the pointer landing on the dot it was
aimed at in a space where world position and screen position have nothing to do
with each other; orbit vs pan; `f` keeping the angle; the seat writing the angle
and the next window opening turned; and the idle rule in the projected space —
`a_settled_projected_graph_stops_painting`, because a projected paint *writes* a
`Projection`, which is exactly where "the paint asks for the next paint" would
begin), core 457.

**The gate, measured** (release test binary, `5k` fixture at whole-vault zoom,
three runs each, machine load average ~7 on 16 cores — so read these as bands,
not points; thread 43's rule that sweep rows are not comparable holds):

| space | whole frame | whole frame, cached |
| --- | --- | --- |
| flat (production path) | 2.41–2.74 ms | 2.05–2.79 ms |
| projected | 3.36–3.71 ms | 3.07–3.36 ms |

The 2D row sits in the band thread 43 recorded (2.3–2.6 ms), so the flat path
did not move. The projected frame costs **~1 ms more at the worst zoom**, for a
comparable picture (4.7k nodes, ~16.8k links drawn against 18.5k) — that is the
`project_all`, the painter's sort, and the grid over projected points, and it
stays inside the 60 Hz budget on a 5,261-note stress vault. Closer in the 3D
rows read cheaper (0.53–0.70 ms) but they are **not the same picture**:
`Cam3::fit` frames the sphere, so the same nominal zoom shows far less
(64n/51l against 1136n/7291l at 3.0x). No claim is made from that pair.

### The first live pass: unusable, and the one number that caused it

The user opened the 5k fixture on a real window and the verdict was blunt: *"completely
unuseable. Everything is far too sensitive and the camera seems to be in the middle of
the cluster with no way to get bearing to move out of it."* Both halves were one bug.

`Cam3::fit` copied the 2D `Camera::fit` and **never touched `dist`**, which stayed at
the constant `DEFAULT_DIST = 2200` world units. The 5k layout spans ~7,000 units, so
the camera plane sat *inside* the cloud: for any node nearer than the plane,
`depth = (dist - z)` goes negative, the `NEAR = 50` clamp catches it, and
`scale = dist / 50` magnifies it ~44×. Half the vault was a wall of overlapping giants
painted on the near plane. "Too sensitive" was that, amplified: with the near nodes
44× blown up, one degree of orbit throws more across the screen than the whole picture,
and a wheel notch of screen magnification compounds with the bogus perspective factor
rather than doing anything a user could read.

Three changes, all in the camera rather than the renderer:

- **`fit` sets `dist` from the layout** — `dist = bounding_radius × FIT_DIST (3.5)`,
  clamped into the dolly band. A bounding radius is rotation-independent, so it does
  not drift while the user orbits. The near/far magnification ratio at that distance is
  ~1.9: perspective you can see, not a fisheye.
- **In 3D the wheel and `+`/`-` dolly the eye** (`Cam3::dolly_by`, clamped to
  1.6–14× the cloud's radius) instead of scaling the picture. Travel is the gesture
  that gets you out of a close view, and the clamp means "zoom in" can never become
  "be inside it" again. The frame carries its bounding radius (`GraphFrame::
  bounding_radius`, measured once per frame in `store_frame`) so the view can answer a
  wheel notch without a pass over 5k nodes.
- **Orbit down from 0.006 to 0.0025 rad/px** — a full sweep across a pane turns the
  world about 80°.

New pins: core `fit_keeps_the_eye_outside_the_cloud` (three layout sizes; no node may
land on the near clamp, magnification ratio under 2.5) and `the_dolly_never_crosses_
into_the_cloud`; app `the_wheel_travels_in_the_projected_space` (the eye moves, the
screen scale does not, and 24 notches cannot cross into the cloud). app 449 / core 459.

**Not yet re-checked by a human.** The numbers say the geometry is sane; whether it
*feels* right — orbit rate, dolly step, whether a bounding *sphere* frames the spiky
stress fixtures too conservatively (its radius is set by the longest spike, so the
dense core reads small) — needs the next live pass. If it does, the refinement is a
percentile radius with a hard floor at the true maximum, not a different constant.

### The second live pass: two gesture rulings, and one that was working

The mode is visible now, and the report was specific rather than vibes.

**"Switching views does cause the graph to relayout and move around slightly, but
it's less than initial open."** That is the design working, and the user's eye is
calibrated enough to measure it: `SWITCH_HEAT = 0.5` is deliberately between
`REHEAT = 0.3` (a shape change — a nudge, because nothing about the picture's
meaning changed) and full heat (a fresh vault). A flat layout has no third axis to
reveal, so turning the graph has to spend *some* energy to open one; anything less
and the two spaces would be the same picture seen twice.

**"The zoom acts weird kinda like a fisheye."** Correct, and it was my gesture
choice, not the geometry. The dolly was on the **bare** wheel, so every notch
changed the perspective factor along with the size: the near nodes grew faster than
the far ones, which the eye reads as the picture bulging rather than as the camera
moving. Ruled: **the wheel means magnify in both spaces** (anchored at the cursor,
same as the flat view — a gesture that changes meaning when the mode changes is a
gesture that has to be relearned), and the travel moves to **`ctrl` + wheel**, where
changing the depth of the perspective is the thing the user asked for. `+`/`-`
magnify in both spaces too. Pinned by the rewritten
`the_projected_wheel_magnifies_and_ctrl_travels`: the bare wheel moves the scale and
not the eye, `ctrl` moves the eye and not the scale, and 24 notches of travel still
cannot cross into the cloud.

**"Can we reverse the left mouse drag rotation directions? It's backwards from what
I expect."** Done: an orbit is **grab-and-turn** — drag right and the face of the
world you are holding travels right. Only the yaw needed flipping, because screen y
already points the other way and the pitch was right by accident of that; the new
core test `a_drag_turns_the_world_with_the_pointer` pins both axes by watching where
the node facing the camera actually goes on screen, since a sign in a rotation
function is precisely the thing nobody can check by reading it.

app 449 / core 460.

### Labels: a lower budget in 3D, the hover always named — and the cut that never sorted

Two asks, one bug found on the way.

**"The points end up much more closely bunched in 3d, can we lower that label cap a
bunch?"** Yes, and the cap should not have been shared in the first place: a third
axis seen from outside folds depth into the same screen area, so the same budget that
reads as sparse in the flat view reads as texture in the projected one. `LABEL_CAP_3D
= 120` (the flat view keeps 300 with its zoom ramp). Not a cost decision — the label
phase is 0.17–0.5 ms of a 3–4 ms frame.

**"Always display the label of the hovered node, in 2d and 3d."** Done in both. The
budget decides which nodes the *frame* names; the hover is an answer to a question, so
it joins the set rather than replacing a member, and it survives the 2D zoom fade
(the frame can be label-free and the hover still speaks) and the 3D dot-size gate (the
node under the pointer may be a small far one). A ring around an unnamed dot is a
question with no answer.

**The bug the test caught:** `label_slots` built its ranking tuple — degree first,
nearness breaking ties — then mapped it straight back to slots and **never sorted**, so
`truncate(120)` was keeping *the first 120 nodes by index*, not the hubs. The doc
comment promised hubs; the code delivered whatever order the index scan happened to
produce. Writing the cap test is what found it: the assertion "the cut kept the
highest-degree nodes" failed with the degree-0 leaf sitting in the chosen set. Pinned
now by `label_slots_respects_the_cap`, `the_hovered_node_is_always_named` and
`a_hovered_node_below_the_size_gate_is_still_named`, and PROJECT.md gained a short
"Label rules" block under Graph View.

app 452 / core 460.

**Follow-up from the live picture: the cap was the wrong mechanism.** 120 labels in a
bunched cloud is still 120 overlapping words — every eligible node sits in the same
band. Placement is now collision-aware: a label is drawn only if its screen box clears
every label already placed (by 3 px), highest degree first, and the hover placed before
all of them so it keeps its space. The cap stays as a ceiling. 3D only for now; moving
the flat view is a measured decision, since its label phase is inside the "2D numbers
must not move" gate. app 453 / core 460.

### The projected view: a sheet that opens, and labels that stop popping

Two live-pass complaints, both measured before being touched.

**"The 3d simulation seems to spread out in a plane a lot."** The physics is fine — a
layout *born* in 3D on the organic fixtures measures an axis-spread ratio of 0.75
(100 notes) and 0.62 (1k). The **switch** is what flattens: 2D→3D carried the flat x/y
and took the new axis from the fresh sim's seeded placement, which is a sphere of
radius ~10·√n — about a quarter of a settled vault's spread. The layout starts as a
sheet, and a sheet is a near-equilibrium: in-plane forces balance and there is no z
gradient to escape along, so `SWITCH_HEAT` alone cannot open it. Measured after a
switch: ratio **0.18 / 0.41**. The fix scales the seeded depth to the flat layout's own
spread before it is used: **0.69 / 0.96**, with the x/y spread essentially unchanged —
the same graph, opened into a volume rather than re-seeded. Pinned by
`turning_the_graph_opens_the_third_axis`.

**"There's a lot of popping."** Label placement is greedy, and greedy placement with no
memory contests the same real estate afresh every tick: two labels a few px apart trade
the spot back and forth while the graph is warm. Now the nodes named last frame take
first claim, in the order they were named (`remembered_first`, memory in
`GraphView::label_memory`). Collision still decides who fits, the ranking still decides
among newcomers, and a remembered label still has to fit — so the set changes when
something actually moves into a name's space, not every tick. Pinned by
`the_labels_named_last_frame_keep_first_claim`; the perceptual half of the claim is the
user's to confirm.

app 455 / core 460.

**Re-bench after the label and switch changes** (5k, projected): whole-vault 3.73 ms,
still-camera 3.51 ms — the same band as the 3.36–3.71 recorded when 3D landed, so
collision culling and the hysteresis memory cost nothing measurable. The flat view was
untouched by all of it.

### M2b: the local graph — the vault narrowed to the note on screen

A filter dimension, not a mode: `GraphFilter::local` (`LocalScope { center, depth }`) sits
beside `folder` in core's `apply`, so it composes with the search line, the kind toggles
and the folder scope, and both spaces get it for free. The BFS walks the vault as it is —
through nodes the kind filters are about to hide — because "two hops away" is a fact about
the links, not about what is on screen.

The **centre is the window's**: the most recently shown note in the active pane
(`note_mru`), and the neighbourhood follows the note the window moves to. The depth is
session state; the centre never is — a remembered centre can be a note the user deleted
on the way out. Depth 1-5, default 2, clamped in the app rather than trusted from `:`.

Reached four ways: the strip's `local N` chip, `[` / `]` (step) and `\\` (whole vault)
in the graph context, and `:graph-local [n]` / `:graph-global`.

One thing learned on the way and now a rule: a workspace-to-view write that hangs off tab
activation has to be **deferred one frame**. `graph_local_follow` runs from
`mark_note_mru`, and a tab is very often activated *by the graph view itself* — a node
click opens its note — so the view is already borrowed when it lands, and `refresh_graph`
writing to it there is a gpui double-borrow panic. The follow re-arms through a one-frame
task holding a weak handle.

Pinned by `the_local_graph_reaches_the_note_on_screen`,
`the_local_graph_follows_the_note_being_read`, `the_local_depth_is_remembered_by_the_session`,
`the_local_depth_comes_back_with_the_session`, and four core filter tests (the hop count,
the clamp, a missing centre staying inert, composition with the kind filters). Left undone
on purpose: a depth slider (the keys and the command line do the job; the strip would need
a second control kind) and per-pane centres. app 459 / core 464.

### Four things from using the local graph: the click line, the centre mark, and a camera that follows the neighbourhood

The first pass at M2b got back four reports from the person using it. Three fixes and one
note-to-file.

**A press is not a grab.** A node was pinned and snapped to the pointer on the first
mouse-move of a press, so an unsteady click on a tag threw the layout around — worst in the
projected view, where the world point under the cursor at the node's depth has nothing to do
with the node. Now nothing happens until the pointer travels past `CLICK_SLOP` (raised 3→6
px), and the same threshold decides both halves: under it the press is a click that opens the
note, past it a drag that begins by pinning the node **where it already is**. One line, so
the click and the drag can never disagree about where it is.

**A neighbourhood names its centre.** The note the window is showing gets a halo wider than
any hover's and a label the budget cannot cut, in both spaces — a local graph that does not
say which dot it is local *to* makes the user find the note they were reading by its shape.
The mark is a lookup by path, never a remembered slot: an index rebuild renumbers nodes under
the same path.

**The camera follows the neighbourhood, as a chase.** Widening the hops in the projected view
grows a cloud straight off the screen, so a depth step, a re-centre or a toggle now drifts the
view onto the neighbourhood it just changed — leaning toward the note, since the framing is of
the neighbourhood the frame holds and the centre is at its middle. It re-derives the fit every
tick instead of tweening to a fixed target, because the neighbourhood is still spreading out
when the drift starts; it gives up after ~1.5 s; any camera input ends it; and it never touches
yaw/pitch. `the_drift_arrives_where_fit_would` pins that it lands where `f` would have put the
camera.

**The follow was broken, and its own test could not tell.** `the_local_graph_follows_the_note_being_read`
asserted the neighbourhood was three notes wide — which is true of the old centre and the new
one alike. Reading *which* notes are shown found two real bugs. First, the follow re-centred on
the tab being activated, which still held the note the user was leaving: the path is only set
after `note_tab` has done its activation, so `mark_note_mru` runs again once it is. Second, the
deferral was a background-executor timer, which gpui's test harness never pumps — so the
re-centre never happened in a test, and the count assertion hid it. It is a frame flag consumed
by `on_next_frame` now, the same idiom as `hover_stale` and `focus_stale`, and the lesson is
written where it will be found: a background timer is not a one-frame deferral.

Clicking a tag to filter by that tag is noted in `graph-view-plan.md` as unbuilt work.
app 464 / core 463.

## The camera drift, rebuilt: frame-paced, and a spring that cannot overshoot

Used the local graph for a while and the camera follow came back as "choppy and jerky", with
"some desync with simulation and view state making the whole camera vibrate". Both halves were
real, and the first cut caused them:

**It was timer-paced, not frame-paced.** A background-executor timer firing every 16 ms is not a
frame loop: ticks land between frames, inside frames, or not at all depending on what the window
is doing, and the picture shows the sum of that. The drift is a chain of `on_next_frame`
callbacks now — one step per drawn frame, camera moved before the frame is laid out and painted —
and the chain stops re-arming when the camera has no ground left to cover, which is what returns
the window to idle. `the_drift_parks_itself_when_it_arrives` asserts a settled drift leaves zero
pending next-frame callbacks behind.

**It was chasing a target that was shaking.** The framing is re-derived from the frame every tick,
which is right (a widened neighbourhood is still spreading out under the forces), but it means the
target moves, and a camera springing straight at a moving target vibrates with it. The target is
now smoothed (τ = 0.15 s) and **starts at the camera rather than at the framing** — seeding it with
the framing puts it a whole move ahead on frame one, and the camera travels to where the target
*was* and comes back. That seeding bug is what `the_drift_arrives_where_fit_would` caught: 150 px
short of the framing, every time.

**And the spring overshot.** Stepped integration (`v += a·dt; x += v·dt`) at ω = 12 crosses the
target by a few pixels and returns, which reads as a bounce. The spring is integrated in closed
form now — `x(t) = (d + (v + ωd)t)·e^{-ωt} + target` — which cannot: from rest the displacement
keeps its sign for every t. Zoom and eye distance are sprung in log space, for the same reason the
wheel ruling put magnification and travel on separate gestures: they are multipliers.

Three rulings from the user shaped the shape of it: smoothness over responsiveness, no last-minute
snap (a slightly-off final position is paid for with smooth arrival), and glide on every forced
camera movement **except `f`**, which stays a snap because the mode switch already carries motion
and stacking animations on it is motion for its own sake.

Two test bugs surfaced on the way and are worth remembering, because both tests were green while
wrong: a `while !step()` loop that ran exactly one step, and a no-overshoot assertion that measured
the camera against where it *started* instead of against where it was going. The assertion that
holds is per-axis: the camera never gets to the far side of its target.
app 464 / core 466.


## The camera jolts the wrong way, and the dots pulsate (2026-11-04)

The vibration from the timer-paced drift was gone. Two things were left, and the user described
both precisely enough to chase: *"when I increase the number of links the camera seems to zoom in
before zooming out"*, and *"the circles change size slightly throughout the camera movement... they
pulsate and vibrate oddly... it also adds some desync in the halo effect"*.

**The size complaint was real, and it was arithmetic.** A node's drawn radius is
`radius × zoom × perspective(depth)`. The drift was springing zoom and eye distance as two separate
springs toward two separately smoothed targets, so the two drivers of one number were free to
disagree. Measured on a 61-note hub across one widening: the picture shrank 42 % while the hub's own
dot **grew 4.5 %**. The halo was never out of step — it is drawn from the same projection, the same
frame, the same radius as the dot — but a fixed 5 px ring is most of a 5 px dot and a hairline on a
20 px one, so it read as a different mark at every zoom.

**The jolt was the same disease one level up.** With two springs, the framing stalled for the first
five frames while the new notes flew outward: the picture ballooned to **1526 px from a start of
1124 (+36 %)** and was then pulled back to 853. Out-and-back on screen *is* "zoomed in before
zooming out", whatever the camera's numbers did.

Three changes, all measured:

- **One spring in the projected view: the eye distance.** `Cam3::fit` split into `fit_dist` (where
  the eye goes, from the bounding radius) and `fit_at_dist`/`fit_extent` (what the frame is, at a
  distance it is given). The drift springs `dist` and re-fits zoom and pan at the distance the eye
  has *reached*, so a node's dot and its picture can only ever move together.
- **The framing may lag a shrink, never a growth.** The τ = 0.15 s smoothing exists for layout
  jitter, and a neighbourhood the user just widened is a step, not jitter. The smoothed extent is
  unioned with the content's extent each step. Peak picture during a widening: **1526 px → 1403 px →
  1124 px** (no balloon at all; it shrinks monotonically to its framing).
- **Sizes are damped, rings are proportional.** Positions use the perspective factor in full; drawn
  radii use 40 % of the way to it (`PERSP_SIZE`), and the halo ring is `max(RING_PX, r × 0.25)`
  instead of a fixed width.

**A mechanism removed rather than kept.** The first answer to the jolt was a monotone-direction
clamp: track whether the content was still moving and forbid the framing from reversing while it
was. It compiled, it passed, it looked principled — and instrumented properly (counting only the
steps where it actually overrode the smoothed value, not the ones where it merely ran) it **bound
zero times** across widening, cliff-narrowing and graded-narrowing. The coherent camera had already
made the reversal impossible. ~6.5 KB of state and two constants deleted.

**And two tests that were decoration.** The first versions asserted a constant dot-to-picture ratio
and a never-reversing zoom, and passed with the fixes disabled — including against the old
four-spring design checked out of `32678c6`. They were rewritten against **real physics**: the
layout advanced two ticks per drift step (`graph_run_for`) instead of a frame being nudged by hand,
and the invariant moved from the camera's numbers to what is on screen — *the picture must not
balloon past its starting size during a widening*. That one fails at +24.9 % without the
cover-growth rule and at +35.8 % with the old design, and passes at 0 % now. The lesson is the same
one as the drift's earlier green-but-wrong tests: a test that cannot tell the fix from the bug is
not a test, and the way to find out is to break the fix on purpose.
app 464 / core 468.


## An out-of-bounds crash on a graph interaction (2026-11-05)

Reported unreproducibly: *"out of bounds crash on graph interaction. I was expanding the hop
size with `]` after changing the graph to 3d."* — `index out of bounds: the len is 12 but the
index is 12` at `crates/app/src/graph/proj.rs:204:61`.

The line was `Projection::node_at` sizing its own hit bound:

```rust
let biggest = frame.nodes.iter().enumerate()
    .map(|(slot, node)| node.radius * self.projected[slot].scale)
```

It walked **the frame's** nodes and indexed **its own** projection. A projection is a snapshot
built at paint time; the view's `frame` is whatever the sim shipped most recently. Between a
widened frame arriving and the next paint, the two have different lengths — 12 and 13 — and the
pointer's question walks off the end of the projection. The `]` in the report is the trigger, and
the mouse moving afterwards is the second half of it.

Two changes:

- `node_at` now walks `0..self.projected.len()` through the guarded `radius` accessor, so a
  projection can only ever answer about the picture it made.
- `store_frame` drops the projection when the node count changes, next to the hover/selection/drag
  clearing that was already there for the same reason — slots are only meaningful within one
  graph. The next paint builds a true one.

**Why the repro is headless and not an event test:** gpui's `VisualTestContext` paints after a
simulated keystroke and after a `window.update`, so the window between a frame arriving and that
frame being painted cannot be reached from the event path — an attempt at exactly that
(`simulate_keystrokes("]")` then a mouse move) came back green with the bug still in, because the
projection had been rebuilt by then. What does reach it is the projection's own contract: hold a
61-node frame, put it in front of the 2-node picture the view is still holding, and ask it what is
under the middle of the canvas. With the old loop that panics with the reported shape (`len is 2
but the index is 2`); with the fix it answers about the two nodes it actually projected.
app 464 / core 469.


## What an idle Shale actually costs (measured, 2026-11-05)

Baseline before any idling work. A release build, one window, no interaction, sampled from
`/proc` over 15 s (CPU from `stat`, wakeups from `status`; `sample.py` in the scratch dir):

| configuration | CPU | threads | RSS | voluntary wakeups/s |
|---|---|---|---|---|
| organic-1k, `--graph` showing | 0.55% | 25 | 30 MiB | 11.2 |
| organic-1k, note showing (no graph tab) | 0.60% | 25 | 29 MiB | 11.2 |
| organic-1k, `--graph`, `[mcp] live = false` | 0.47% | 24 | 30 MiB | **7.3** |
| organic-1k, `--graph`, `[spellcheck] enabled = false` | 0.60% | 24 | 30 MiB | 11.3 |
| organic-100, `--graph` | 0.27% | 25 | 26 MiB | 12.7 |
| the `harper-ls` child, idle | 0.00% | 19 | **141 MiB** | 0.0 |

What the numbers say:

- **The graph is free at rest.** With and without the graph tab is inside the noise, and 0.55% CPU
  at 1k notes rules out idle painting (a 1k paint is 2.4-3.7 ms; painting at frame rate would show
  ~15%). The beat parks when the layout settles, and the drift chain parks itself.
- **The live MCP beat is the largest wakeup source: ~4/s** (11.2 → 7.3 with it off). It republishes
  every 250 ms whether or not anything changed, and it calls `publish_live` on *every* window plus
  the active one again.
- **`harper-ls` is the largest memory item by far**: 141 MiB and 19 threads, ~5x the app's own
  30 MiB, to underline two words. It starts even when the window opens on the graph tab.
- **The vault walk is the largest CPU item** and scales with the vault (0.27% at 100 notes, 0.55%
  at 1k) — the `WATCH_DUTY` self-tuning is doing its job, and it is the one thing here that is
  already designed as a budget.
- **The review-queue poll is per window, not per vault** (`start_pending_watch` runs from
  `Workspace::new`), so N windows on one vault means N `read_dir`s a second. The watcher, live
  channel and backups are per-vault (`VaultServices`); this one was left on the window.
- ~7 wakeups/s remain with the live channel off, and they are not accounted for by the three beats
  (live 4 + walk 1.3 + queue 1). Unaccounted-for idle wakeups are the one thing this measurement
  cannot name; `strace -c -p` on a running window would.


## The live channel goes quiet (2026-11-05)

The idle measurement named two costs that were pure overhead, and both are now gone.

**The 250 ms publish beat.** Every window copied itself into the vault's snapshot four times a
second, whether or not anything had changed. The copy is now driven by drawing: `Workspace::render`
publishes, gated on the app's active window (or any window when none is active, which is the
unfocused-app and test case the old beat handled by publishing everything and the active one last).
An idle window draws nothing — measured with a probe in `render`: **0 renders over 15 s** with the
1k graph on screen — so an idle window publishes nothing. The answer is also *fresher* than it was:
the frame that moved the cursor is the publish, rather than up to 250 ms later.

**The accept loop's 50 ms poll.** `live/mod.rs` put the listener in nonblocking mode and slept
between accepts: 20 wakes/s, and `strace` showed 239 `accept4` calls in 12 s, every one of them
returning "nothing waiting". It blocks in `accept` now. `stop()` wakes it by connecting to its own
port — the loop checks the stop flag before it looks at the connection, so that connection is closed
unanswered — and waits on a channel with a 2 s timeout instead of joining, so a listener that never
wakes becomes a warning at shutdown rather than a window that will not close. Measured: graceful
exit in 535 ms, lockfile removed, `/status` and `/note` answering before and after.

Connections are still served inline, one at a time — that was already true with the poll, and
`REQUEST_TIMEOUT` (2 s) is what bounds how long one stalled client can hold the others.

**The payoff, same method as the baseline** (one window, organic-1k, `--graph`, no interaction):

| | before | after |
|---|---|---|
| wakeups/s (main thread) | 11.3 | **7.2** |
| `accept4` + `clock_nanosleep` per s | 40 | **0** |
| futex per s | 54 | 35 |
| syscalls per s (all threads) | ~520 | ~424 |
| CPU | 0.55% | 0.53% |

CPU did not move because the vault walk dominates it; the win is wakeups, which is the battery
number. What is left at rest: the walk (1.3/s), the queue poll (1/s, per window — next), and gpui
re-arming a timer fd for each of them.

The test that used to prove the beat worked (`the_beat_keeps_the_answer_current`) became
`the_answer_follows_what_the_window_draws`, which asserts both that a drawn edit publishes and that
600 ms of sitting still publishes nothing. The second half needed a publish counter: `updated_at` is
epoch seconds, too coarse to show a republish. Mutation-checked: with the render-publish removed the
test fails. app 464 / core 469.


## The queue beat moves to the vault (2026-11-05)

`start_pending_watch` ran from `Workspace::new`, so N windows on one vault read
`.shale/pending-edits/` N times a second. The read moves to `VaultServices`
(`start_queue_watch`, same 1 s), and what stays per window is the *judgement* — a hunk is
stale against that window's buffers, which may be holding a note another window has already
written out. `Workspace::scan_pending` is now "read then judge"; the beat calls
`VaultServices::fan_out_queue` → `Workspace::apply_pending_queue`. The read stays on the main
thread: the queue is a `read_dir` of a nearly-empty folder, not a walk of the vault, so the
walk's reason for a background thread is not this one's.

**The test was wrong before it was right.** `one_queue_reaches_every_window_on_the_vault`
opened two windows with the test helper `open_window`, which calls `Workspace::new` — and
`Workspace::new` builds its own `VaultServices`. Two windows, two beats, two one-window fan-out
lists: the test passed, and mutating `fan_out_queue` to reach only the first window still
passed. Rewritten through the registry (`open_vault_window` + `open_extra_window`, which is what
shares services in the app), the same mutation fails on the second window's assertion. Written
into `PROJECT.md` as a testing rule, because "use the test helper twice" looks like the same thing
and is not.

Also learned the easy way: gpui's test executor *does* pump a repeating beat — `Clock::pause`
past the interval plus `run_until_parked` runs one pass of it. The old comment in `review.rs`
("rather than waiting on simulated time") was about directness, not impossibility. app 464 /
core 470.


## `gd` follows a link, `gD` makes the note and follows it (2026-11-05)

The follow chord was `gx`; the LSP-shaped pair is now the front door — **`gd`** follows the
wiki-link under the cursor, **`gD`** follows it *and creates the note first* when the target
resolves to nothing. `gx` keeps working (one line in the chord table, and it is what the docs
and comments already say).

The chord is a core `Outcome` (`FollowLink` / the new `CreateLink`), the editor emits a new
`EditorEvent::CreateLink`, and the window decides what creating means — the engine still knows
nothing about the vault. `Workspace::follow_link` gained a `create` flag; `make_link_note` reuses
`:new`'s bookkeeping (`create_note` → `refresh_entries` → `index_created` → tree selection →
`open_note` → the status line), in that order, because opening a note says the note's name and the
creation is the thing worth saying.

**Where the new note lands (ruled 2026-11-05): beside the note in hand.** A link written inside
`areas/bread-v2/bread-v2.md` wants a note of that area; a target that spells a path out
(`[[areas/x/y]]`) is used verbatim, folders and all, and `create_note` makes the parents. The
folder is a tidiness choice, not a correctness one — Shale resolves links by shortest unique name
anywhere in the vault.

Not done, deliberately: the created note is **empty** (same as `:new` — no title line, no
template), and `[[note#heading]]` still lands at the top, because the resolver takes a raw target
with no alias and no anchor. Both belong with P4.6 (link health, heading anchors), which is where
the graph's tracked-link data pays off properly. app 465 / core 471.


## The window's shape survives a restart; `ctrl-w >`/`<` move the divider (2026-11-05)

Two open P3.2 items, one arc. A split window used to restore as **one pane** — and the
tree was not the only loss: the other panes' tabs went with it, because `capture_session`
asked `tab_state` for the *active* pane and nothing else.

**The file.** `Session` gained `panes: Vec<PaneSeat>` (one per leaf, in tree order: its
tabs, and which of them had the keyboard), `layout: Option<LayoutSeat>` (`leaf` /
`split{dir,frac,kids}`) and `active_pane`. The leaves carry no payload — the leaves *are*
the `panes` list, counted left-to-right, top-to-bottom, which is the order the window
paints them. `tabs`/`active_tab` stay and now mean "the pane with the keyboard's", which
is what they always were. `Session::pane_seats()` is the single reader: a file with no
`panes` answers as exactly one pane, so the old shape and the new one are one case in the
restore rather than two.

**The tree.** `Node::seat()` and `Node::from_seat(seat, leaves)` live in `panes.rs`: the
tree type converts to and from its own seat, so the geometry owns its own format.
`from_seat` answers `None` for a shape that does not describe exactly `leaves` panes, a
direction that is not `row`/`column`, or a kid given no space. A session file is plain
text a person can edit; a window that cannot make sense of the shape keeps the one pane it
opened with, puts every tab in it, and says so in the log — it does not paint nonsense and
does not index a pane that is not there.

**The restore** walks the panes in tree order, setting `active_pane` as it goes, because
every door it uses (`add_tab`, `load_note`, `open_graph`) works on the pane that has the
keyboard. The per-pane cleanup — drop tabs whose notes were deleted, keep at least one
tab, restore that pane's *own* active tab — runs per pane. The cursor and scroll seat is
still one seat: the pane with the keyboard's. The tab body moved into `restore_tab`, which
is now also where the graph tab's camera, mode and orientation come back.

**Resize.** `ctrl-w >`/`<` (and `+`/`-`, the same moves for a hand on the letter row) call
`Node::resize(leaf, delta)`, which finds the split the pane sits **directly** in: in a
stack inside a row, growing the bottom pane makes it taller, not wider. Fractions are
relative, so the pane takes `+0.25` of an equal share per press and the others keep their
numbers and take proportionally less. `MIN_FRAC = 0.2`: a pane may be squeezed to a sliver
but not out of existence, because a pane you cannot see is a pane you cannot hand the
keyboard back to. Keys only, no `:` row — `ctrl-w =` (equalize) is keys-only too.

Nine app tests (both halves of the fraction round-trip, the split restore with each note in
its pane and its own active tab, a nonsense tree dropped rather than trusted, the
nearest-split rule, a one-pane window refusing, the floor holding under 40 presses) and one
core test (a flat file reads as one pane), plus the round-trip extended to a 2:1 split.
Mutation-checked twice: not rebuilding the tree fails the restore test; refusing to recurse
fails the nearest-split test. app 480 / core 466.


## `:q` belongs to the pane, and a blank restored pane had no tab (2026-11-05)

Two reports from the user's testing of the new pane work.

**The crash.** `thread 'main' panicked at crates/app/src/workspace/tabs.rs:144: index out
of bounds: the len is 0 but the index is 0` on relaunch. The session had panes whose tab
lists were empty — every note closed before the window quit — and `restore_session` only
re-establishes the never-empty invariant *inside* the tab loop, which does not run when no
seat remembered a tab. The tree was already rebuilt by then, so the window came up with N
panes holding zero tabs each, and the first `Pane::active_tab()` — total by contract — had
nothing to answer with. Fixed with the invariant stated where it belongs: after the tabs
are restored, any pane with no tab gets one through `add_tab`, the same door startup and
`ctrl-t` use, and the keyboard goes back where it was. The test reproduces the panic
exactly (`a_session_of_blank_panes_still_has_a_tab_in_each`) before it passes.

**The exits.** Ruled: `:q` (and `:q!`, `:wq`) closes **the pane the command was typed in**
when there is more than one; the window quits only with the last pane — vim's `:quit`
closes the window it was typed in, and Shale's "window" for that purpose is the pane. And
closing the last tab in a pane closes the pane (Obsidian's rule: a split should not fill up
with panes nobody is looking at), with the last pane as the exception — there the tab is
cleared, as it always was, so a window never quits because a tab closed. `close_tab` now
takes the `Window` it was asked on, because closing a pane has to move the keyboard to the
one that survived.

Not done, on purpose: **moving a tab to another pane** has no key. Roadmap P3.2's
move/exchange/rotate is about moving *panes*; the tab half (steal-on-open, or an explicit
drag) is an open question with the user. app 484 / core 466.


## Opening a note another pane shows steals the tab (2026-11-05)

Asked: there is no way to move a tab between panes. True — and P3.2's
move/exchange/rotate is about moving *panes*, so the tab half was not on the roadmap.
Ruled: **steal**. Opening a note another pane already shows moves that tab into the pane
with the keyboard, rather than dragging the keyboard across the window to wherever the
note happened to be open.

The tab moves **whole**, which is cheap precisely because the tab owns its editor: cursor,
folds, view and undo history are inside the `Tab`, so a move is a `Vec::remove` +
`Vec::push` and the test proves it by editing in one pane, stealing the tab to another,
and pressing `u` there.

Two rules fall out of the existing invariants:
- The pane the tab came from gets a **blank tab**, not nothing. A pane is never empty, and
  a pane closing because a note was opened somewhere else is a side effect nobody asked
  for (unlike closing the last tab, which does close a pane — that was a deliberate
  request, this is a consequence).
- The source pane's `active` index has to be clamped on the way out. It is not the pane
  with the keyboard, so nothing else in the path would notice its last tab had gone — that
  is exactly the `index out of bounds: len is 1 but the index is 1` the first version of
  the test caught.

The **graph tab is the exception**: still one per window, still focused where it is. It is
a view of the vault rather than a note you are standing next to, and `open_graph` runs
during session restore, where relocating the tab would fight the file.

Test helper note: `repaint` (draw a frame *and* run the next-frame callbacks) moved from
`tests/graph/mod.rs` to `tests/mod.rs`. The keyboard follows a tab change one frame later
by design, so a test that types straight after a move is typing against a view that has
already left the tree — the trap is the same for panes as for graph. app 485 / core 466.

## A blank tab is a front door, not a void (2026-11-05)

`crates/app/src/workspace/start.rs` — new. A blank tab used to be an empty buffer: a caret
at the top of nothing, with every door out of it living in the docs.

**What it shows.** One line in the status bar's voice (`3 notes · 12 links`), the vault's
recent notes as clickable rows, and the doors as key-chips (`space f` open ·
`ctrl-shift-f` search · `space g` graph · `ctrl-n` new note). It sits on the note's own
surface at the note's text indent — no panel, no fourth elevation level, no animation — and
it is gone the moment the tab starts being a note: insert mode, or text in the buffer. Undo
back to nothing and the door is there again; the block follows the tab's state, it is not a
latch.

**This amends a design ruling.** §13's "every empty surface gets exactly one line" is right
for an empty *list* and wrong for a blank tab, which is the window's front door. The ruling
now says so, and the block keeps everything else that ruling forbids: no illustration, no
second voice, no motion, no new elevation. The parked first-run item closes with it — an
empty vault gets the same screen, one different line (`the vault holds no notes yet`) and
the one door that works (`new note`).

**Recents are the vault's, not the window's.** `recent` sits at the top level of
`.shale/session.json` (newest first, capped at 10, deduped by `Sessions::remember_note`),
recorded in `open_note`, so every door counts: picker, tree, `gd`, a tab click, the agent.
It goes out with whichever session save comes next — opening a note is not worth a disk
write of its own. A note deleted from disk drops out, because every row is a promise that
clicking lands somewhere.

**A row names its own pane.** Clicking a recent in a pane without the keyboard opens the
note *there* and takes the keyboard with it; the tab bar's rows already follow that rule,
and a start block in a split window would otherwise be a menu answering to the wrong pane.

The click path is the picker overlay's tested pair (`.occlude()` plus a stopped mouse-down),
so the tests cover the semantics — the predicate, the list, the pane a row belongs to —
rather than hunting pixels. The fold-marker tests show what a coordinate-based click test
costs when a component does not report its own bounds. Verified by eye on a real window:
`3 notes` / `recent` / three rows / the legend, at the note's indent, caret above it.
app 491 / core 467.


## A blank tab is a note being written, not an error state (2026-11-05)

**The dead end.** A blank tab takes typing — insert mode, characters, a caret — and then
refuses to keep it. `:w` answered `no file open`, autosave returned early at the same
missing path, and a quit with autosave off dropped text that existed nowhere else. The
tab looked like a page and behaved like a locked one.

**The rule.** A tab with no file is a note the user is writing, and the first line they
typed is its name. `:w` takes that line; `:w <name>` takes the name they typed instead.
Both are one write path (`write_tab`), so `:w`, `ctrl-s`, autosave and an exit all name
the note the same way — the note does not depend on which of them arrives first. Autosave
is the one that keeps quiet: a beat that reported "nothing to name it after" every couple
of seconds would be a nag, not a status line.

**The cleaning is core, not app** (`shale_core::note_name`): markdown block markers and
the characters the filesystem refuses fall out, the name is capped at 60 characters, a
name can never be a hidden file, and `..` segments drop so nothing lands above the vault.
Markers are stripped longest-first — `#` matching before `###` stops the strip dead,
because there is no space after the short match. A name taken from the first line and
already on disk gets a number (`Idea.md` → `Idea-2.md`): two notes that open with the
same line are the common case, and the user was not choosing a name at all.

**`<name>` is a different kind of request, so it fails differently.** A typed name may
place the note in a folder (`:w areas/idea` → `areas/idea.md`), and a typed name that
collides is *refused* rather than quietly suffixed — answering a specific request with
`idea-2.md` is not answering it. And `:w <name>` on a note that already has a file
refuses out loud, the whole command, pointing at `:rename`: the user's ruling
(2026-11-05) is that the command line must not clone or move a note by accident, and the
difference between the two is that `:rename` rewrites the links pointing at the old name.

**Asked in the same breath: does the file tree copy and paste?** No — it has `n`, `r` and
`shift-d`, and no way to duplicate a note at all. Recorded in `brainstorm.md`: duplicate
is the piece people want and needs no clipboard (it can reuse the collision rule above);
copy/paste wants a vault clipboard holding a *path*, not text, because the editor owns the
text clipboard; and move owes what `:rename` already pays — link propagation — which is why
it is its own command id and not a file copy.

app 500 / core 477. Nine app tests (name from the line, cleaned not verbatim, `:w <name>`
into a folder, the refusal on a named note, the refusal on a collision, the number on a
derived name, nothing written out of nothing, no escape from the vault, an exit naming
rather than losing) and ten in core for the naming itself.

## An idle run pays nothing for being killable (2026-11-15)

**The dead end.** The termination signal (P1.5) was caught with an atomic flag and *polled*:
every window asked "yet?" every 200 ms, forever. At rest that poll was the single biggest
wakeup source in the process — 5 wakeups a second per window, on top of everything else.
Being killable should cost an idle run nothing.

**The plumbing.** The handler still does only what signal context allows — an atomic store
plus a one-byte write to a `pipe2` self-pipe (nonblocking write end, blocking read end). A
small reader thread sleeps in `read()` until a byte lands and then sends on a
`futures::channel::mpsc::unbounded` — the same channel idiom the LSP reader thread uses.
One watcher task per process waits on that channel; when it wakes it walks
`VaultWindows::all_windows` — the same registry walk `:qall` takes — and runs
`finish_termination` for every window. No per-window task, no timer, and a `kill` now
answers for *every* window the process holds, not just the ones that had a watch attached.

**The gpui pitfall, recorded because it cost an hour:** a `Task` is *cancelled when its
handle drops*. `spawn_watcher` discarding the handle silently killed the watcher before it
ran. The registry owns the handle (`VaultWindows::set_termination_watcher`) because the
registry lives as long as the run.

**The flag is still the fact.** The watcher checks it before every wait, so a signal that
lands between `main` installing handlers and the watcher spawning is not lost — one of the
four tests pins exactly that race. The other three pin the promise: the dirty note reaches
the disk, the window closes, and two windows on two vaults are both written by one wake —
asserted by which file holds which words.

**Idle measurement** (headless gpui — no `WAYLAND_DISPLAY`/`DISPLAY`, so no GPU driver in
the numbers; `examples/graph/organic-1k` with Home.md open; 15 s windows; 3 runs each):

| build | main-thread wake/s | CPU | RSS |
|-------|-------------------|-----|-----|
| before (poll) | 6.7–7.0 | ~0.5% | 31.5 MiB |
| after (self-pipe + watcher) | 2.3–2.4 | 0.47–0.53% | 31.5–31.9 MiB |

The remaining 2.3/s is the vault watcher's 750 ms walk beat (~1.3/s) and the pending-queue
`read_dir` at 1 s (~1/s) — both intended behavior, both still on the clock.

app 900 / core 857 / fixture 1 / mcp 42 — 1800 passed, 0 failed. fmt and clippy clean.

## The vault's beats sleep until they have news (2026-11-15)

**The dead end.** After the exit fix, the two beats left at rest were the vault
watcher (walk every 750 ms–15 s) and the pending-queue read (every 1 s). Both
were *intended behavior* — the tree following the disk, agent proposals arriving
unasked — so the clocks stay. What went is the cost of agreeing nothing changed:
each loop was a foreground gpui task, and a foreground task wakes the main
thread at **every await** — timer, walk, all of it — even when the tick ends in
`continue`.

**The shape.** Both beats now look like the LSP reader thread: a background loop
with only `Send` captures (timer, walk/read, compare-against-last) and a
`futures::channel::mpsc::unbounded` for the news. A walk whose signature equals
the last one's never sends; a queue read that matches the last one never sends;
the folding task on the foreground side sits parked on the channel and the main
thread never hears the beat at rest at all. Termination is a shared
`AtomicBool` (the folding side raises it when the services entity is gone) plus
the channel's own send-failure for the other direction.

**Two pitfalls, both recorded because both cost a debugging cycle.**
1. gpui v1.25.3's `AsyncApp` is **not `Send`** (`Weak<AppCell>` is `Rc`-based),
   so a background loop cannot carry the app context across threads — the
   channel is the only bridge.
2. In gpui tests, `background_executor().timer()` is the **fake clock**: real
   waiting never fires it, and `advance_clock` always does. The new
   `tests/beats.rs` drives both loops deterministically with `advance_clock`
   steps — no wall-clock sleeps, 0.02 s for both.

**The behavior is pinned, not assumed.** `tests/beats.rs` opens a registry
window (the beats belong to the vault's services), changes the disk under it,
and asserts the change arrives *unasked* — the tree rows two outside notes
beat after beat, and a proposal lands in the review strip within a beat. These
are the regression gates for the unchanged-skip: a comparison that swallows a
real change fails loudly.

**Idle measurement** (headless, organic-1k with Home.md open, 15 s windows):

| build | main-thread wake/s | CPU | RSS |
|-------|-------------------|-----|-----|
| before (foreground loops) | 2.3–2.4 | 0.47–0.53% | 31.5 MiB |
| after (background + channel) | 0.0–0.1 | 0.20–0.33% | 31.7–32.0 MiB |

The walk and the queue read still run on schedule — on background threads,
which is where they belong. Combined with the exit fix, idle main-thread
wakeups are down from 6.7–7.0/s to 0.0–0.1/s, and the remaining beats are
only the ones with something to say.

app 902 / core 857 / fixture 1 / mcp 42 — 1802 passed, 0 failed. fmt and clippy clean.

## Cold start: what the 420 ms actually is (2026-11-15)

**The harness bug first.** The 8.5 s "paint outliers" were never the app. gpui
paints only when dirty, so two 50 ms polls can straddle the single first paint
with no pair-difference to see; the diff-based detector then reported the *next*
visual change — spell underlines arriving seconds later — as the paint. Fixed:
`paint_ns` is now the first screenshot with content (PIL extrema, numeric per
the bench rules), not the first pair-difference. The outliers vanished.

**The breakdown** (warm page cache, real display, phase markers since removed):

| phase | time |
|-------|------|
| exec → main → claim → `application()` | ~19 ms |
| shale's own vault work (config, db, sessions, beats, workspace) | ~19 ms |
| `cx.open_window` — first window | **~550 ms** |
| — of which NVIDIA ioctls (wgpu device + shaders) | ~234 ms |
| — of which font file I/O (461 files) | ~20 ms |
| — of which Vulkan ICD enumeration | ~3 ms |

`WGPU_BACKEND=vulkan` vs `gl` made no difference — the cost is the GPU device
and shader setup gpui does for the first window, on any backend. It is gpui
territory, not shale's: **shale's own startup is ~40 ms**, and the rest is the
toolkit meeting the graphics driver. Chasing it means patching gpui (persisted
pipeline cache, lazy device) — noted, not done.

**Honest cold-start numbers** (dnd vault, 155 notes, fixed harness, 5 runs):
mapped 372–439 ms, paint 410–496 ms, settled 629–713 ms, PSS ~141 MB. The PSS
is mostly the GPU driver (NVIDIA libs ~75 MB + enumerated Vulkan ICDs ~10 MB);
shale's own heap is **18 MB** (smaps breakdown) — the headless RSS of 31 MiB
remains the honest app-size number.

Note for the next profiler: `perf` is blocked on this machine
(`perf_event_paranoid=2` + SELinux); `strace -f -T` with per-syscall totals
attributed the startup well enough, and phase markers in the app beat both.

## Hot-path breakdown + the search cache (2026-11-15)

**The unbenched paths got benches.** `crates/core/src/bench_paths.rs`
(ignored, same style as the graph benches): the editor's per-paint styling,
the block map, vault-wide search, and the picker's fuzzy ranking. First
numbers, ranked by what a keystroke pays:

| path | cost |
|------|------|
| search, 1k-note vault | **6.7–6.8 ms/query** |
| fuzzy ranked, 10k names | 7.4 ms/call |
| fuzzy ranked, 1k names | 0.74 ms/call |
| style_spans, 8k-word note | 146 µs/call |
| block_map, 8k-word note | 43 µs/call |

Search is the picker's `ctrl-shift-f` row source, asked on every keystroke:
it reads every note from disk every time. A probe said the reads, not the
scan, are the cost (stat×1000 ≈ 2.5 ms, read×1000 ≈ 9 ms in the probe's
units; scan is noise next to the read).

**The fix: `search::NoteCache`.** Each note's text is kept with the `Stamp`
(size + mtime) read with it, and a query re-reads only notes whose stamp
changed. Freshness is *exactly* what search had: every note is still stated
on every query, so an edit made outside the app is seen before the note is
scanned — only the read is skipped when the stamp says the note cannot have
changed. Deleted notes drop their cached text. A 32 MiB budget bounds the
memory; over budget the cache keeps nothing and search behaves as before.
New `Vault::stamp` is the stat primitive. The Workspace holds the cache
per window (the picker's row source runs without `cx`, so it captures an
`Rc` clone); the cache lives for the window's life.

| build | warm query, 1k notes |
|-------|---------------------|
| before (read every note) | 6.7–6.8 ms |
| after (stamp-checked cache) | **2.9–3.2 ms** |

The first query still pays the reads; every later keystroke pays stats and
scan. The stat is now the floor (~2.5 ms for 1000 notes) — the next lever
is parallel stats or trusting the watcher's signature, both noted, neither
done: 3 ms is inside one frame.

**Pinned, not assumed.** Four core tests: cached answers equal direct
answers, an external edit shows up in the very next search, a deleted note
is not answered from cache, and an over-budget cache still answers. One app
test drives the real picker: warm a search, write the note from outside
while the picker is open, retype — the row is the new line.

app 903 / core 861 / fixture 1 / mcp 42 — 1807 passed, 0 failed. fmt and clippy clean.

## Fuzzy ranking: the picker's other per-keystroke path (2026-11-15)

`ranked` matched every candidate from scratch: the query was re-lowered per
candidate, the alignment allocated two vectors per query character, and the
parents matrix was copied row by row. Three changes, scoring untouched (the
12 existing tests pin placements, scores and ranges exactly):

- the query is lowered **once** per `ranked` call;
- a no-allocation **subsequence pre-check** rejects candidates the dynamic
  program would place nowhere — free for the common case of a picker full of
  non-matches;
- one **scratch** (chars, offsets, rows, parents) is reused across
  candidates, and parents are written only into cells a query character
  actually landed on — a traceback walks matched cells only, so stale
  entries are never read and the matrix needs no clearing.

| build | ranked, 1k names | ranked, 10k names |
|-------|------------------|-------------------|
| before | 736 µs | 7.40 ms |
| after | **512 µs** | **6.19 ms** |

The remaining cost is the dynamic program itself — O(query × candidate) per
matching row — and every matching candidate in the bench matches, so
pruning has nothing to prune. At vault sizes (1k names) half a millisecond
per keystroke is inside the frame; noted, not chased further.

## Search: the stat floor, parallelized (2026-11-15)

With the note cache in place, the warm keystroke's remaining cost was the
stats — one syscall per note, sequential. The stat is read-only, so
`NoteCache::search` states the notes through `std::thread::scope`: each
thread fills its own slice of the stamp vector, so the order — and the
results — are exactly the sequential ones. Vaults under 256 notes stay
single-threaded; the spawns would cost more than the stats they replace.

| build | warm query, 1k notes |
|-------|---------------------|
| read every note (before) | 6.7 ms |
| stamp-checked cache | 2.9 ms |
| + parallel stats | **1.85 ms** |

3.6× off the original per-keystroke cost. The scan of cached text and the
hit construction are what's left, and they are noise at vault sizes.

## The editor's frame: line geometry was quadratic (2026-11-15)

**The bench.** `crates/app/src/tests/editor_bench.rs` (ignored, release-only,
graph-paint-bench conventions): a real `MarkdownEditor` in a test window —
a forced frame, a keystroke plus the frame that shows it, and a `999j`
scroll. The first numbers were ugly and *superlinear*: 16× the note, 150×
the frame. quads stayed at 26, so paint was virtualized — the cost was
everything before it.

**The cause.** `Buffer::line_start(row)` scanned the text from byte 0, and
`line_count()` counted newlines across the whole text. The render path asks
for line geometry *per row* — row_styles, hidden_markers, row_paint,
display_lines, folds — so every frame cost rows × text. At 8000 words that
is 89 ms for `row_styles` alone, 104 ms for markers+paint, and a 560 ms
frame.

**The fix.** `Buffer` keeps a line-start table cached against `change_id`,
rebuilt only when the text moved; `line_start`/`line_end`/`line_count`
answer from it. The naive scan and the table are pinned equal by a new test
across insert, delete, undo, redo, `set_text`, and empty text.

**The bug the fix exposed — and the rule it taught.** `snapshot()` bumps
`change_id` *before* an edit, and `open_line_below` asks `line_end(row)`
*between* the bump and its own `replace_range`: the cache got stamped with
the fresh id and the stale text, and the splice that followed went
unannounced. Four app tests caught it red-handed (a painted line containing
a newline). The discipline now lives in one place: every text splice goes
through `Buffer::splice`, which moves the id *after* the text. A bump before
an edit is for the UI's sake; a cache must never be fillable between the
bump and the mutation.

| words | frame | keystroke | 999j |
|-------|-------|-----------|------|
| 2000 before | 39.8 ms | 59.9 ms | 155 ms |
| 2000 after | **5.0 ms** | **6.7 ms** | **21 ms** |
| 8000 before | 560 ms | 837 ms | 2.01 s |
| 8000 after | **28.3 ms** | **45.6 ms** | **132 ms** |

Scaling is linear now. What is left at 8k words is the per-keystroke re-wrap
of every row (the wrap cache is keyed on the whole-buffer change id) and
gpui laying out ~1000 display lines per frame — viewport-windowed wrap and
element virtualization are the next levers, noted not done. At real note
sizes (the dnd vault's longest notes) a frame is ~3–5 ms and a keystroke
~5–7 ms: inside one frame with room to spare.

app 903 / core 862 / fixture 1 / mcp 42 — 1808 passed, 0 failed. fmt and clippy clean.

## The wrap cache keys on what the measurement saw, not on the buffer (2026-11-15)

**The change.** The soft-wrap cache was keyed on the buffer's change id, so
every keystroke re-measured every row's break points. `CachedRow` now carries
what the measurement was a function of — the *painted* text (markers hidden,
quotes redrawn), the wrapper index (face and size), and one level up the
width — and a row is reused while those hold. A keystroke re-measures its own
row and nothing else.

**Two bugs the bench caught before this was true, both worth remembering.**

1. *Keying on the raw bytes misses style moving under them.* The bench's
   keystroke deletes characters (normal-mode `x`), and an eaten `**` cascades
   bold across the rest of the note: the raw row text is unchanged, but the
   painted text and the wrapper are not. Display lines piled up 66→681 as
   rows kept stale narrow-face breaks. The key had to be the painted text,
   not the row's bytes.
2. *Absolute break offsets go stale when the row shifts.* Breaks were cached
   as absolute buffer offsets; a deletion above a row shifts it left, the
   text comparison still passes, and the re-validation guard happily lets the
   shifted breaks through. Breaks are stored relative to the row start now.

Both were invisible in the unit tests and obvious in the bench's primitive
count (quads 26→2) and display-line count — the graph-paint bench's rule
that the primitive count rides along because it does not move with the
machine, paying for itself again.

| words | frame | keystroke | 999j |
|-------|-------|-----------|------|
| 500 before | 3.43 ms | 5.36 ms | 12.9 ms |
| 500 after | **1.19 ms** | **1.88 ms** | **5.6 ms** |
| 2000 before | 4.99 ms | 6.70 ms | 21.2 ms |
| 2000 after | 4.26 ms | 6.80 ms | 21.3 ms |
| 8000 before | 28.3 ms | 45.6 ms | 132 ms |
| 8000 after | 27.8 ms | 42.2 ms | 129 ms |

Honest reading: the win is at real note sizes, where the per-frame whole-note
re-measure was a big share of the frame — at 500 words a keystroke is now
1.9 ms. At 8000 words the keystroke is dominated by the *other* whole-buffer
passes that still run per change (restyle, row styles, marker/paint plans,
and gpui laying out ~1000 display lines); the wrap measure was only part of
it. Those passes are the next levers, noted not done.

app 903 / core 862 / fixture 1 / mcp 42 — 1808 passed, 0 failed. fmt and clippy clean.
## What the force layout costs over a selected set (measured, 2026-11-17)

The graph lens item (roadmap P4.16) has been deferring one number: "measure the *sim*
over a large selected set before promising a vault-wide lens; the 5k paint numbers
(2.3–2.6 ms) are not the constraint, the force layout is." A lens hides everything its
query did not answer, so the sim runs over the answer alone — the same `Vec<bool>` mask
`graph::filter::apply` already hands back. A temporary `#[ignore]` probe in
`crates/app/src/tests/graph/paint_bench.rs` (removed after this record) built the index
for the organic fixtures, kept every *k*-th note so the subset is **spread** rather than
one neighbourhood, and ran the sim to `settled()`. Release build, three runs, machine
lightly loaded (load average ~1.7); the band across runs was within ~5% except at 100
kept nodes, where the fixed cost makes the noise visible.

| kept notes | settle, total | worst tick |
| --- | --- | --- |
| 100 | 5.0–8.1 ms | 24–84 µs |
| 250 | 30–31 ms | 134–281 µs |
| 500 | 87–93 ms | 311–563 µs |
| 1000 | 279–292 ms | 1.0–1.5 ms |
| 2000 | 878–902 ms | 3.3–4.5 ms |
| 5000 | 4.33–4.42 s | 15.5–17.9 ms |
| 5119 (the whole fixture) | 4.33–4.34 s | 16.1–16.7 ms |

**The finding is a bound, not a warning.** Cost is superlinear in the kept set — about
50 ms per 1k kept at 100 nodes, about 870 ms per 1k at 5000 — but a selected set can
never cost more than the whole vault, and the whole vault is what the graph tab settles
today: the 5000-of-5119 row and the 5119 row are the same number. A lens is always
cheaper than the picture it replaces, so the item ships without a cap or a warning
threshold. The honest guard is the node count the strip already shows.

**Ticks do not grow** — 300 every time, which is the cooling schedule (`alpha_decay`
0.0228 down to `alpha_min` 0.001), so the cost is per-tick work rather than a longer
settle. The frame budget is the thing to watch: 1.0–1.5 ms per tick at 1000 kept,
3.3–4.5 ms at 2000, 16–18 ms at 5000. The last is over budget, and it is today's
whole-vault behaviour, not something a lens introduces.

**What it changes in the plan:** EDGE-L7 ("a lens re-runs when the index moves") needs a
second half. Re-running the *query* is one SQL call and cheap; re-heating the *layout* on
every index move would pay ~0.9 s of sim work for a 2000-node answer while the reader is
typing. So the rule is: re-run the query on an index move, and disturb the layout only
when the answer's node set actually changed. The re-heat cost itself was **not** measured
— the app's `GraphSim` keeps `reheat` private to its physics enum — but it is bounded by
the cold settle above, since a re-heat starts at a lower alpha.

## The lens is a filter dimension (P4.16 M1, 2026-11-17)

`GraphFilter` gained `lens: Option<LensScope>` — a name, and the vault-relative paths one
saved query answered. `apply()` chains it after folder scope and **ands** with it: a lens
worn on a folder-scoped graph is the intersection, which is what "combine" was ruled to
mean (EDGE-L3). Tags and ghosts go through the one neighbour pass folder scope already
had, so a tag survives a lens when a lens note carries it and a ghost is drawn when a lens
note names it (EDGE-L4). The chips and the search keep cutting inside the answer, which is
what stops the filter strip lying about a lensed picture.

Two rules the tests hold apart, because they look alike and are not:

- **An empty answer is an answer.** A lens that came back with nothing draws nothing. A
  local scope whose centre the index does not know leaves the graph alone, because the
  window owns that centre; a lens's answer is not the window's to second-guess.
- **A path with no node keeps nothing and is not an error.** Counting those answers is the
  app's job (EDGE-L5), because `apply` answers with a bitmask and nothing else.

Nothing in the app sets a lens yet: session restore carries `lens: None` and says so in a
comment, with `GraphFilterSeat.lens` owed by M6.

## A lens is a file the vault will not look at (P4.16 M2, 2026-11-17)

`crates/core/src/lens.rs` is a lens's whole life outside the graph: where it lives
(`<vault>/.shale/lenses/<name>.shq` — and `.shale` is already in the vault's ignore list,
which is what puts a lens out of the tree, out of quick-open, out of the index and out of
the agent's note tools while leaving it plain text: EDGE-L1), how a vault's lenses are
listed, how one is read by name, and how its answer becomes a set of note paths.

Three rules worth their weight:

- **A lens name is one name.** A separator is refused *before* the name-shape guard, on
  purpose: `../secret` is a path trying to walk out of the lens folder, and the sentence
  that says so is the one worth reading. The dot guard would also have stopped it, by
  accident of where the dots sit — and a refusal that lands by accident is one the next
  edit can lose.
- **A lens has no host.** `answer` always passes `host = None` into `query_note::ask`, so a
  query that declares `relative = "current"` is refused in the picker's own words for the
  same file (EDGE-L13). There is no lens-specific wording for the two doors to disagree in.
- **The column is found by name.** A saved search compiles to `SELECT path AS note`, so
  `note` is the name a lens reads; `path` is the hand-written query's habit; the first
  column is the fallback, because the file is the authority. A cell that is not text is not
  a path — it lands in the count of answers with no node (EDGE-L5) rather than being
  guessed at.

## The lens door: `:lens`, the picker, and a settled sim that ships nothing (P4.16 M3, 2026-11-17)

The verb and the picker are in `crates/app/src/workspace/graph.rs`, next to the filter code
that already owned the mask: `graph_lens_command` (`:lens [name]`), `graph_lens_action` (the
palette row), `graph_lens_chosen` (a picker row), `set_lens` (the one place a name becomes a
`LensScope`), and `open_lens_picker`. `PickerTarget::Lens(name)` carries the choice;
`Ex::Lens` and the `graph.lens` registry row carry the word.

What the door refuses to do matters as much as what it does:

- **The palette row asks; it never acts.** `:lens` with nothing typed and the palette's
  "Choose a lens for the graph" both open the picker, so there is no way to wear a lens
  nobody chose (ruling 5).
- **`none` is a name, not a mode** (EDGE-L9), and saying it with nothing on says "no lens is
  on the graph" rather than pretending to undo.
- **A lens that will not answer is not worn.** The status line gets core's sentence — the
  same one the picker and an embedded table give for that file — and the picture stands
  (EDGE-L6). An empty answer is different: it *is* an answer, so the picture goes empty.
- **The picker measures at the moment it opens** (EDGE-L10), one query per lens on one read
  connection, so two lenses cannot answer against two snapshots of the index. A lens whose
  query will not run shows `—` and is still choosable: the row is how the reader finds out.

The finding is **EDGE-L15**. The first version re-made the picture the way the strip's own
path does — `rebuild_graph` then `pump_graph` — and every test that checked the *counts*
stayed at three notes while the mask underneath had changed. `pump_graph` returns at once
when the sim is **settled**, and a lens that only removes nodes can leave it settled: nobody
ticks, so nobody ships a frame, and the view keeps the old picture and the old numbers.
`refresh_graph` is the door that ships summary, frame and filter whatever the sim's heat,
and M4's live re-run (EDGE-L7) owes the same.

Ten app tests (`crates/app/src/tests/lens.rs`) run the whole door through the real keymap
and picker; twelve mutations, all red (`falsify_lens_door.py`). `docs/graph.md` has a lens
section, `docs/state-files.md` lists `lenses/<name>.shq`, and `docs/commands.md` regenerated
the `graph.lens` row.

## The graph learned what an alias is (2026-11-17)

A link written by an alias — `[[the deep one]]` where `Deep.md` declares `aliases: [the
deep one]` — was drawn as a **dead node**, and the edge went to the dead node, leaving
`Deep.md` looking like nobody writes about it.

It was never a resolution bug. `LinkResolver`'s fourth rule is aliases, and
`with_aliases` is how a door learns them; the link colour, the hover card, the embed door
and `ctrl-]` all call it. `GraphIndex::build` and `set_note` built plain resolvers and
never did. The graph's own doc comment already states the rule the fix restores: *"four
surfaces, one order, or the picture disagrees with the text it is a picture of."* The
alias feature's comment in `refresh_link_names` goes further — *"the colour disagreeing
with the resolution … is the one disagreement this feature cannot have"* — and the graph
was the surface that had not been told.

Two things worth keeping:

- **An alias is declared in one note and written in another.** So the note that changed is
  not the note whose link it is: `set_note` on the saved note cannot reach the edge. A
  moved alias set now re-scans the vault (`graph_aliases_changed` → the same background
  build opening the tab runs), and while that scan is in flight a save is queued **even
  when an index is already standing** — otherwise the new index arrives without it.
- **An alias connects a link, never steals one.** The resolver's order (exact path, path
  with `.md`, stem, alias) holds in the graph, pinned by a fixture that has to put the real
  note in a folder: at the vault root the exact-path rule answers first and the test says
  nothing about the stem-vs-alias order. That fixture was too weak until the falsification
  run said so — swapping the two rules in the resolver stayed green.

Seven tests (three core, four through the live window), seven mutations, all red
(`falsify_alias_graph.py`). `docs/graph.md` says it in the reader's words now.

## The bar says what the graph is wearing (2026-11-17)

A lens on the graph had no indicator anywhere: the strip's filter line answers only while
the strip is focused, and the tab title's `· lens` segment is M4. So a narrowed picture
looked like the vault, which is the failure mode a lens has — the reader forgets it is on
and starts reasoning about absence.

The status bar now carries `lens <name>` beside the view badge, in the accent colour the
bar reserves for states that are not the default, and only while the **graph tab is on
screen**: on a note tab the segment would read as a property of the note, and a lens on a
graph you are not looking at belongs in that tab's title.

The test asserts what the frame **drew**, not what a helper could have computed: the frame
records the segment where the `.when` branch is taken, into a `#[cfg(test)]` cell cleared
at the top of every frame — the same shape `chrome_drawn` uses for the panel marks. A hook
that re-derived the string would have stayed green with the segment deleted from the bar
entirely, and that mutation is the first one the battery runs. Three mutations, all red
(`falsify_lens_bar.py`): no slot in the bar, the segment ignoring which tab is up, and a
segment appearing with no lens on.

## A graph pays a resting pointer a card (2026-11-17)

The alias fix closed with the user's next word: "aliases are just invisible now". The graph
has no way to say it — a frame's edges are pairs of node slots with no kind, so "draw alias
edges differently" would push a kind through the frame, the simulation, the filter and the
paint. The card is the cheaper half: the dot that was reached by an alias can say
`also known as: …` above the note's first lines.

The card is the same `card::Preview` the editor's link card uses, for the same reason — one
look answers "what is this?" wherever the pointer asks it. The difference is the **wait**:
500 ms of stillness (`card::NODE_DWELL`). Prose holds few links, so the editor answers the
moment the pointer finds one; a graph is dots packed together and sweeping across one is
ordinary, so an immediate card flashes at every dot on the way past.

The window asks the picture where the pointer came to rest, once per frame; the graph does
not tell it. Telling is the shape gpui refuses — the graph is usually being called *by* this
window when its hover changes — and asking turns out to be free as well: a frame that
reshuffles every slot simply answers differently next frame, so a changed frame costs no
extra bookkeeping. A press ends the rest (`resting_on` refuses while the drag is down), and a
graph tab sitting behind another tab is not asked at all — its hover is the last one the
pointer left there, and reading it raises a card over a note the pointer is on. That last one
was a bug the test found, not the code.

Two traps the tests walked into before they pinned anything. A two-note graph fits itself to
`MAX_ZOOM` (8.0), so the wheel that moves a dot in a small vault is the wheel **out** —
zooming in from the fit is the one wheel the view refuses, and a zoom-in test there proves
nothing. And the alias line reads the window's alias table, which arrives with a *delivered*
index pass: a test that writes notes after the window opens has a graph and no aliases, so
that vault is built the way the alias graph tests build theirs.

Twelve tests (`tests/graph/node_card.rs`), twelve mutations red. The ghost rule needed **both**
guards broken at once — the window's filter and the card's own path check each hold the line
alone — which is the battery's own lesson: a doubly-guarded rule needs a doubly-broken
mutation before a green suite means anything.

## The perf branch, measured against the tree it replaced (2026-11-18)

The perf work landed through a merge, so its numbers were recorded on a tree that has since
absorbed 22 commits of features — block refs, dice chips, the lens, the alias fix, the node
card — several of which add passes over the buffer per change. Two questions, both answered
with measurements rather than memory.

**Did the wins survive the merge?** Measured on both trees on the same machine minutes apart:
the perf tip (`7f960ab`) in a scratch worktree, and merged main (`bc71780`).

**What did the tree look like before the perf branch at all?** `75547da` — the merge base —
was built in a second worktree, and the benchmark files the perf branch *added*
(`editor_bench.rs`, `bench_paths.rs`) were copied onto it, so both sides run the same bench
code. The pre-perf tree has no note cache, so its cache bench was dropped; its plain search
line **is** the pre-perf keystroke cost.

| path | pre-perf `75547da` | merged main `bc71780` | |
|------|---|---|---|
| picker search, per keystroke, 1k notes | 6.56 ms | 1.90 ms (cache + parallel stats) | **3.5×** |
| fuzzy `ranked`, 1k names | 744 µs | 502 µs | 1.5× |
| fuzzy `ranked`, 10k names | 7.77 ms | 5.66 ms | 1.4× |
| `style_spans`, 8k words | 143 µs | 136 µs | flat |
| `block_map`, 8k words | 42.3 µs | 43.8 µs | flat |
| editor frame, 500 words | 3.59 ms | 1.18 ms | **3.0×** |
| editor keystroke, 500 words | 5.60 ms | 1.81 ms | **3.1×** |
| `999j` scroll, 500 words | 16.4 ms | 5.52 ms | **3.0×** |
| editor frame, 2000 words | 41.7 ms | 5.17 ms | **8.1×** |
| editor keystroke, 2000 words | 66.1 ms | 6.45 ms | **10.3×** |
| editor frame, 8000 words | 568 ms | 32.9 ms | **17×** |
| editor keystroke, 8000 words | 835 ms | 60.8 ms | **13.7×** |
| `999j` scroll, 8000 words | 2.00 s | 120 ms | **16.7×** |

The 8000-word rows are the shape of the thing: pre-perf the frame grew **super**linearly
(3.6 ms → 41.7 ms → 568 ms across 4× the words), because row geometry was rebuilt per row
from the buffer's start. Post-perf it grows roughly linearly (1.2 → 5.2 → 32.9 ms). The
`quads=26` column never moved, which is the check that the *work* changed and not just the
clock.

**Cold start did not move, and that is the finding.** Same harness (`bench/cold_start.py`,
fixed-window niri screenshots, 5 runs each, dnd vault), both binaries, minutes apart:

| | pre-perf | merged main |
|---|---|---|
| mapped | 366 ms | 367 ms |
| first content (paint) | 397 ms | 398 ms |
| settled | 613 ms | 614 ms |
| PSS at settled | 142.5 MB | 142.3 MB |

Startup is gpui meeting the GPU driver; shale's own work is the ~40 ms between mapped and
paint. The perf branch aimed at the hot paths — search, fuzzy, beats, the per-change editor
passes — and that is exactly where the movement is.

**The measurement lesson, which cost an hour to learn.** The first pass read main's warm
search at 2.7 ms and its 8000-word frame at 36–38 ms — a 40% "regression" against the perf
branch. It was the machine: those runs sat at a **load average of 5.6–6.7** (release builds
competing for the same cores). The same binaries at load **2.0–2.5** read 1.90 ms and
26.8 ms — indistinguishable from the perf branch. So: every number here carries a load
reading, a suspicious gap gets re-run rather than explained, and the primitive count is the
check that the work didn't change when the time did. A wall-clock benchmark without a load
average next to it is a story, not a measurement.

## A lens keeps up with the vault, and says its own name (P4.16 M4, 2026-11-18)

M4 is the lens becoming a live dimension rather than a thing read once. Three surfaces and
one behaviour:

- **The tab title wears it.** A graph tab reads `graph`, or `graph · <lens>`. Recorded per
  frame in a `#[cfg(test)] tab_labels` cell, because the rule the bar lens taught still
  holds: a hook that *re-derives* the string stays green with the string deleted from the
  bar, so the test asserts what the frame **drew**.
- **The strip carries a row for it** — `lens <name> · 3 notes`, or
  `lens <name> · not answering: <why>` — because the status line's sentence scrolls away and
  the picture's silence does not. An answer naming notes the graph has no node for appends
  `, 2 with no node` (EDGE-L5's count-out-loud, now in the place the reader is looking).
- **It re-runs when the vault moves** (EDGE-L7). `lens_follow` re-asks the query on an index
  move and compares the *node set*; the layout is only disturbed when that set moved, so a
  lens over a vault that edited its notes' text keeps its picture. A hand-edited `.shq`
  re-runs too: the vault's signature walk skips `.shale/` on purpose (EDGE-L1), so
  `lens::stamps()` reads the lens folder's mtimes beside it.
- **A lens that stops answering keeps the last good picture** (EDGE-L6) and carries the
  sentence into the strip; a lens file deleted from under the graph says
  "`<name>` is gone; the picture stays as it was" (EDGE-L11).

The falsification battery caught its own usual thing: the strip's row recorder was written
at **build** time, so it recorded whether a row *could* be drawn rather than whether one
**was** — moving it inside the `.when` draw branch turned the mutation red. Eight new tests,
all mutations red.

## The strip can keep the picture it is showing (P4.16 M5, 2026-11-18)

A lens used to be a file you write by hand. Now the graph can make one from the question
already sitting in its filter line: `alt-s` in the strip asks for a name (starting from one
made from the question), `:lens save <name>` is the same door with the name already given,
and **a saved lens goes on right away** (ruled 2026-11-18) — the sentence says
`saved lens `x`: 3 notes`, one clause for each half.

The save is nearly free, and that was the point of P4.3 M6: `db::compile_sql` already turns
a typed search into "the SQL a hand-written `.shq` would hold". The lens file is that
statement, plus the question kept above it as a comment so the file says what it was asked
in the words that were typed.

What the door refuses:

- **A question that cannot be kept is refused before a name is spent on it** — core's own
  sentences, the same three keep-a-search makes.
- **An existing lens is never written over** (ruling I). The file a reader wrote — and may
  edit by hand, since it is the authority (EDGE-L1) — outranks the picture just saved.
- **A lens name is one name, never a path.** `lens::read`'s guard is factored into
  `guard_name` and both doors use it, with separators checked *before* the shape guard so
  `../evil` reads as "a lens name is one name, not a path" rather than as a dot-name
  complaint. A refusal that lands by accident is a refusal the next edit can lose.
- **The strip's chips are not encoded into the file** (ruling 1). They stay dimensions, so
  a lens never freezes an `orphans` choice into a query its author will later read. The test
  pins it behaviourally rather than by string: save `modified:2020-01-01` with orphans
  hidden, then switch the chip back — the orphan comes back, *inside* the lens (EDGE-L3).

Two findings. **`modified:7d` will not save**, and that is correct: a duration counts from
the moment it runs, so written into a file it would mean "since the day it was saved" — the
refusal fired while the chips test was being written, and the test uses a date. And **the
filter line is still its own dimension after the save** (ruling 2): `tag:tag` fuzzy-matches
node *labels*, so the picture is empty until the reader takes the text out. The test clears
the line and then checks the lens's answer, rather than asserting the surprise.

Also fixed on the way, from the same family as a run-on sentence in a status line: three of
core's refusal messages had picked up runs of 14 spaces where a Rust `\`-continuation was
eaten by the tool that wrote them (`db/index.rs` twice, `graph/render.rs` once). The
falsification battery found a second thing: its `alt-s` anchor matched **twice**, because
the binding had been added to the keymap twice — the keymap docs test dedupes and stayed
green. Thirteen mutations, all red (`falsify_lens_m5.py`); seven new tests in
`tests/lens.rs`, 27 lens tests in all.

