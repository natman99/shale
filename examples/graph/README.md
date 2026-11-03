# Graph fixture vaults

Six generated vaults for driving the graph tab (`space g`, `:graph`) by eye. They
are **not committed** — the largest is 5000 tiny files. The generator and this
file are the source of truth:

```sh
python3 tools/graph_vaults.py                 # write all six (~4 s, ~39 MB peak RSS)
python3 tools/graph_vaults.py organic-1k      # just one
python3 tools/graph_vaults.py --stats         # report on disk, write nothing
python3 tools/graph_vaults.py --list          # fixture names and shapes
```

Open one like any vault:

```sh
cargo run -p shale-app -- examples/graph/organic-1k    # or target/release/shale …
```

Opening a fixture writes its `.shale/` (config, DB) inside the fixture, which
`.gitignore` already covers. Each fixture has its own seed, so regeneration is
byte-identical and `--stats` numbers are comparable across runs and machines.

## Two shapes, because they test different things

| Fixture | Notes | Edges (note/tag/ghost/asset) | Out-degree med/p90/max | Busiest note | Busiest node | Groups, size med/max | Components | Orphans |
|---|--:|---|--:|--:|--:|---|---|--:|
| `100` | 100 | 360 (266/47/32/15) | 3 / 11 / 27 | 28 | 28 | 10, 9/27 | 1 (100%) | 5 |
| `1k` | 1 000 | 3 848 (3 133/221/303/191) | 3 / 6 / 31 | 87 | 96 | 20, 60/97 | 1 (100%) | 45 |
| `5k` | 5 000 | 18 843 (15 466/1 056/1 386/935) | 3 / 6 / 51 | 171 | 468 | 46, 100/300 | 1 (100%) | 241 |
| `organic-100` | 100 | 171 (119/48/1/3) | 2 / 4 / 10 | 10 | 10 | 30, 2/12 | 7 (75%) | 6 |
| `organic-1k` | 1 000 | 1 355 (1 030/272/23/30) | 1 / 3 / 10 | 10 | 15 | 276, 2/120 | 83 (57%) | 77 |
| `organic-5k` | 5 000 | 6 780 (5 145/1 363/136/136) | 1 / 3 / 10 | 10 | 75 | 1 341, 2/600 | 378 (56%) | 374 |

**`100` / `1k` / `5k` — the hub shape.** Engineered to stress: `MOC` hubs with
14–40 outgoing links, `star` notes with 10–30 links of their own, one root
`Welcome` reaching every cluster, a 300-deep `Chain/`, and tags treated as
global (`#project` on 468 notes of the 5k vault). It settles into **one**
component with a dense core, which is exactly what makes it good for measuring
frame cost, Barnes-Hut, label culling, node sizing, and the filters — and what
makes it useless for judging whether the layout can find groups. `100` is the
only fixture under `graph::sim::NAIVE_LIMIT` (256), so it is the only run of the
O(n²) pair loop.

**`organic-100` / `-1k` / `-5k` — the realistic shape.** A vault shaped like
someone's actual vault, so natural clustering has something to find:

- **Topic folders of 2–9 notes** — `projects/quadtree-work/`,
  `books/camera-2025/` — where each note links one to three of its siblings.
  174 of them in the 1k vault, 839 in the 5k, against the hub fixtures' 20 and
  46 folders of ~60–100 notes each.
- **Bridges between specific notes, not between hubs.** About one note in ten
  links outside its folder (11% in `organic-5k`) — but so does roughly one in
  twelve in the hub shape (8%), so the *rate* is not the point. What matters is
  which notes carry the bridge. There, every cluster's `MOC` links three other
  `MOC`s and `Welcome` links all of them, so any note is two hops from any
  other; here a bridge joins two leaf notes, and nothing structurally joins two
  folders. That is why the hub shape collapses into one component and this one
  falls into 378.
- **Real furniture at realistic degree.** A `Home` and a `reading-list` naming a
  few clusters each; a `journal/` of dated notes that mostly chain to yesterday,
  with the gaps a real journal has; an `ideas/` folder of half-written
  singletons, some tagged, most untouched.
- **Cluster-local tags, with a spread cap.** Palettes belong to an area
  (`#wip`, `#fix-it`, `#recommend`), ~31% of notes carry one, and no tag may
  touch more than 9% of the cluster count — because a tag node is an edge to
  every note carrying it, and an uncapped `#wip` reached 53 notes in the 1k
  fixture, quietly gluing a dozen folders together. The current heads are 15
  notes (1k) and 75 (5k).

Numbers worth comparing: the hub 5k vault has one component holding 100% of its
notes, a node of degree 468, and notes with up to 51 outgoing links; `organic-5k`
has 378 components with the largest holding 56%, a widest node of 75 (a tag),
and no note over 10 links. Edge density is a third of the hub shape (1.4 edges
per note vs 3.8).

One honest caveat: **modularity is not the discriminator.** The reporter prints
`Q` for the planted partition, and both shapes score high (0.89 for `5k`, 0.86
for `organic-5k`) because the hub fixtures' folders are internally dense too.
What separates the shapes is degree spread, component count, and group size, so
read those columns first.

## Ground truth for scoring the layout

Each organic fixture carries `.fixture.json`: a map of planted group → member
notes (topic folders, `journal`, each `ideas/<name>`, and an `index` group). The
dot-prefix keeps it out of shale's vault walk, and the git-ignore keeps it out of
the repo. That makes layout clustering measurable rather than eyeballed: take
screen positions after the sim settles, and check whether a group's members are
nearest neighbours, or run the usual overlap statistic against these labels. The
hub fixtures plant no file, so `--stats` falls back to grouping them by
top-level folder.

`--stats` per fixture prints: notes, edges by kind, edges written both ways,
out-degree percentiles, node degree percentiles, notes with exactly one link and
with ten or more, true orphans, ghost/tag/attachment node counts with the
busiest of each, and the clustering block (groups and their sizes, the share of
notes linking outside their group, the share of note edges inside a group, `Q`,
components and the largest one's share).

## The checks built into the fixtures

Both shapes carry the same traps; each is a thing the graph must get right, with
notes to open.

| Where | What to check |
|---|---|
| `Welcome.md` (hub), `Home.md` (organic) | Every wiki-link form resolves: plain, `[[T\|alias]]`, `[[T#Notes]]`. |
| `Self Link.md` (hub); organic notes with a "(yes, that link points back at this note)" line | Self-links produce no edge and no self-loop (`GraphIndex` must ignore them). |
| `Resolution Probe.md` + `Ambiguous/` (hub); `Home.md` + `books/renovation/` and `areas/home/renovation/` (organic) | Two notes share a stem; a bare link must land on the **shortest path**, and only an exact path reaches the nested twin. |
| Any note with a `text`/`sh` fence, or `[[…]]` in inline code | Must never become an edge or a ghost node. Organic orphans holding only a decoy stay invisible until the orphan filter is on. |
| `Missing Page NN` (hub), `chickens-follow-up` and friends (organic) | Ghost nodes: shared targets must collapse to one node each; "existing files only" hides them all. |
| `assets/*.png` | Real files, so embeds are attachment nodes and not ghosts. Hub fixtures have 8, organic 5–12. |
| Cluster hubs (hub) and `journal/` (organic) | Pairs written from both ends (~7% of note edges in the hub shape, ~23% in organic) must carry `EdgeMeta::bidirectional = true`; the rest must not. |
| Tags | Toggle `tags` and watch the note structure stay put. Organic tags should only thicken local neighbourhoods; if turning tags on merges the clumps, the spread cap has broken. |

## For whoever generates them

- Shape knobs live at the top of each module: `SPECS` plus `CLUSTER_SIZES`,
  `AREA_WEIGHTS`, `AREA_TAGS`, `TAGGED_SHARE`, `TAG_SPREAD`, `INDEX_*` in
  `tools/graph_vault_organic.py`; `hub_links`, `chain`, `orphans` in
  `tools/graph_vault_hubs.py`. Changing a seed changes the whole vault, so the
  counts in the tables above follow the shape, not the seed.
- The hub fixtures are **byte-identical on purpose** — that is what lets someone
  diff a fixture after a sim tweak and attribute every changed line to the tweak.
  Their `rng` call order is part of the fixture. Refactors that move code around
  are safe; reordering loops is not. Organic fixtures make no such promise yet.
- The reporter's link resolution deliberately approximates
  `markdown::extract_wikilinks` and `Vault::resolve_link` (exact path, then
  case-insensitive stem or name, shortest path winning a tie). It exists to
  report honest numbers, not to be a second implementation — if the two ever
  disagree, shale is right and the reporter should be fixed.
- Six vaults regenerate in about four seconds and 39 MB peak RSS; the largest on
  disk is 20 MB of small files, which is why none of it is committed.
