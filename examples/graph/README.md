# Graph fixture vaults

Three generated vaults for driving the graph tab (`space g`, `:graph`) at
different scales. They are **not committed** — thousands of small files is not
what this tree is for. The generator is the source of truth:

```sh
python3 tools/graph_vaults.py            # write all three (~2 s, ~35 MB peak RSS)
python3 tools/graph_vaults.py 5k         # just one
python3 tools/graph_vaults.py --stats    # report on what is on disk, write nothing
```

Each vault has its own seed, so regeneration is byte-identical and the printed
stats are comparable between runs. Open one like any vault:

```sh
cargo run -p shale-app -- examples/graph/1k     # or target/release/shale examples/graph/5k
```

Opening a fixture writes its `.shale/` (config, DB) inside the fixture, which
`.gitignore` already covers.

## The three scales

| Fixture | Notes | Undirected edges | Note↔note | Tag | Ghost | Asset | Both ways | Out-degree median / p90 / max | Busiest note | Size |
|---|--:|--:|--:|--:|--:|--:|--:|---|--:|--:|
| `100` | 100 | 360 | 266 | 47 | 32 | 15 | 49 | 3 / 11 / 27 | 28 | 432 KB |
| `1k`  | 1 000 | 3 848 | 3 133 | 221 | 303 | 191 | 268 | 3 / 6 / 31 | 87 | 4 MB |
| `5k`  | 5 000 | 18 843 | 15 466 | 1 056 | 1 386 | 935 | 1 131 | 3 / 6 / 51 | 171 | 20 MB |

Why these three:

- **`100`** fits on one screen with orphans on, so every *feature* of the graph
  is checkable by eye in one look. It sits under `graph::sim::NAIVE_LIMIT`
  (256), so it exercises the O(n²) pair loop.
- **`1k`** is past that limit: Barnes-Hut, labels hitting their 300-per-frame
  cap, the first place where culling and label ranking matter.
- **`5k`** is the sim's real stress — ~19k edges, and the size at which frame
  time, the spatial hash, and (later) 3D sort start to cost something.

## The shape, which is the point

All three have the same skeleton, only scaled. Counts below are for `5k`.

- **Communities.** 40 clusters, one folder each, sized unevenly (40–200 notes).
  Each cluster has a `MOC` note that links to up to 40 members and to three
  other MOCs; members link back to their own MOC about half the time, so hub
  in-degree is far higher than anything else (busiest note: degree 171).
- **Skewed out-degree.** Notes take a role — `hub` (the MOCs), `star` (10–30
  links of their own, ~6% of notes), `bridge` (mostly cross-cluster, ~8%), and
  `leaf` (1–3 links, the rest). Median out-degree 3, p90 6, max 51; 757 notes
  have exactly one link and 307 have ten or more. That spread is what makes
  node sizing, the label degree cap, and `link_strength / min(deg_a, deg_b)`
  observable side by side.
- **One connected component.** `Welcome.md` links every MOC, and bridges stitch
  the clusters, so there is a dense core with sparse arms rather than 40
  isolated islands. About 7% of note edges go both ways (1 131 of the 5k
  vault's 15 466), which is roughly what a real vault looks like: links are
  usually answered, not always.
- **Depth, not just width.** `Chain/` holds a 300-note chain, `Chain 0001 →
  Chain 0002 → …`, with a long-range jump to a hub every 20 notes — for
  local-graph and thread (M5a) work, where hops matter more than counts.
- **Orphans.** 250 notes in `Orphans/`: three quarters link nothing and are
  linked by nothing (241 true orphans), the rest carry only tags, which still
  makes them non-orphan because a tag is an edge. Toggling the orphan filter on
  and off should visibly add exactly the bare ones.
- **Ghosts.** 239 ghost nodes: a shared pool (`Missing Page 01…40`, the busiest
  touched by 45 notes — they must collapse to one node each) plus one-off
  `Ghost of …` targets. "Existing files only" should hide every one of them.
- **Attachments.** 8 real 8×8 PNGs in `assets/`, embedded by ~18% of notes.
  They are real files, so they become attachment nodes and not ghosts.
- **Tags.** 14 tags including nested ones (`#tech/wgpu`, `#person/mira`,
  `#archive/2026`, `#area/design`), spread zipf-ish over ~22% of notes:
  `#project` on 468 down to `#parked` on 12. The tag layer is deliberately
  kept subordinate to the note layer — with naive zipf sampling the busiest tag
  touched a quarter of the notes and the communities vanished behind it.

## Correctness checks hidden in the fixtures

Each of these is a note you can open and a thing the graph must get right:

| Fixture note | What to check |
|---|---|
| `Welcome.md` | Walks the whole feature list; every link form resolves. |
| `Self Link.md` | Links to itself. `GraphIndex` ignores self-links: no self-edge, and it should not grow its own degree. |
| `Resolution Probe.md`, `Ambiguous/` | Two notes share the stem `A Double`. A bare link must land on `Ambiguous/A Double.md` (shortest path wins); the nested twin is only reachable by its exact path. |
| Any note with a `text` fence | `[[A fence is not a link]]` and `[[…]]` inside inline code must never become edges or ghost nodes. An orphan carrying only such a decoy stays invisible until the orphan filter is on. |
| Cluster MOCs and answered links | ~7% of note edges were written in both directions by two different notes. Those edges, and only those, must carry `EdgeMeta::bidirectional = true` (M4's arrow display depends on it). |

## Notes for whoever generates them

- `stats` at the end of a run resolves links with a simplification of
  `markdown::extract_wikilinks` and `Vault::resolve_link` just enough to report
  honest numbers. It is not a second implementation to test shale against — if
  the two ever disagree, shale is right and the generator should be fixed.
- Tuning lives in `SPECS` at the top of the script: note count, cluster count,
  MOC out-degree, ghost pool, chain length, orphan pool, seed. Changing a seed
  changes the whole vault, which is why the counts in the table above move with
  the shape rather than with the seed.
- Memory and time are nowhere near a problem (2 s and 35 MB RSS for all three),
  but the 5k fixture is 20 MB of tiny files; `du` it before committing, which is
  exactly why it is ignored.
