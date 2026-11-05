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
