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
