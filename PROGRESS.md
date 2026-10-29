# Shale — Progress (graph track)

This file belongs to the **graph agent** (working from `graph-view-plan.md`).
PROJECT.md belongs to the other agent; we keep our log here to avoid edit
conflicts. Design detail: `graph-view-plan.md`; placement: `roadmap.md` P3.3.

## Status: M1a complete — core graph module, 25 tests green

`crates/core/src/graph/` — new module. Touched elsewhere: `pub mod graph;` in
`crates/core/src/lib.rs`, `petgraph = "0.8"` (MIT, pure Rust) in the workspace
and core `Cargo.toml`. No app files touched.

### What's there

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
- **`quadtree.rs` — Barnes-Hut**: swapped in above `NAIVE_LIMIT = 2048`.
  θ=0.5 (d3 uses 0.9; the 1/d law needs the tighter bound to hold 5% per
  node against the exact loop — validated on a 4000-point scatter).
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
- **glam: parked.** Not in the tree (gpui v1.25.3 has its own geometry);
  the sim is SoA scalar f32 that LLVM already vectorizes, and glam's
  `Vec2` is one SSE lane wide — no win there, same 8 bytes. Revisit at
  M2 for the *projection/camera layer only* (Mat4/Vec3A beats
  hand-rolled 4x4 math).
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
