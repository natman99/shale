#!/usr/bin/env python3
"""The hub-shaped fixtures: `100`, `1k`, `5k` in `tools/graph_vaults.py`.

A deliberately engineered shape, not a plausible vault: big `MOC` hubs, explicit
star/bridge/leaf link budgets, one root hub, and a long chain. It is the shape
that *stresses* the graph — Barnes-Hut cost, label ranking, node sizing by
degree, orphan and ghost filtering — and the shape that settles into a dense
core with sparse arms.

The generated output is byte-identical across runs, which is a property worth
protecting: it is what lets someone diff a fixture after a sim tweak and attribute
every changed line to the tweak. So the sequence of `rng` calls below is part of
the fixture, not incidental detail — reordering a loop or resizing a list changes
the vault. `graph_vaults.py --stats` prints node and edge counts, and the
checksums those fixtures produce are the canary.
"""

import random
import shutil
from pathlib import Path

from graph_vault_common import GRAPH_DIR, SENTENCES, paragraph, png_bytes

# ---------------------------------------------------------------------------
# Vault specs
# ---------------------------------------------------------------------------

#: Fixture name -> shape. `notes` is the exact number of `.md` files written:
#: `clusters` covers the cluster notes, and the chain, orphan, root, and
#: ambiguous pools below are carved out of the same total.
SPECS: dict[str, dict[str, int]] = {
    "100": {
        "notes": 100,
        "clusters": 4,
        "hub_links": 14,   # out-degree of a cluster MOC
        "ghosts": 6,       # dangling targets shared across notes
        "chain": 12,       # length of the depth chain
        "orphans": 5,
        "seed": 101,
    },
    "1k": {
        "notes": 1_000,
        "clusters": 14,
        "hub_links": 26,
        "ghosts": 14,
        "chain": 60,
        "orphans": 50,
        "seed": 1_001,
    },
    "5k": {
        "notes": 5_000,
        "clusters": 40,
        "hub_links": 40,
        "ghosts": 40,
        "chain": 300,
        "orphans": 250,
        "seed": 5_001,
    },
}

#: Notes outside the clusters: the three root notes plus the three notes of the
#: colliding-stem pair (see `generate`). Subtracted from `notes` when the
#: clusters are sized, so the total stays exact.
EXTRA_NOTES = 6

#: Cluster folder names. A fixture needs `SPECS[k]["clusters"]` of them, and 40
#: is enough for the largest.
CLUSTERS = [
    "Atlas", "Borealis", "Cinder", "Dusk", "Ember", "Fathom", "Glacier",
    "Hollow", "Ivory", "Juniper", "Kestrel", "Lumen", "Meridian", "Nettle",
    "Onyx", "Prism", "Quill", "Riven", "Solace", "Tesseract", "Umbra",
    "Vellum", "Wisp", "Yonder", "Zephyr", "Ashfall", "Bracken", "Calder",
    "Drift", "Echoline", "Fenwick", "Glimmer", "Harrow", "Ironvale",
    "Jotun", "Kelwin", "Larkspur", "Mirewood", "Norbeck", "Ossuary",
]

#: Tags, zipf-weighted in this order: the first land on many notes, the last on
#: a few. Nested names are deliberate — `graph::index::extract_tags` accepts
#: internal hyphens, underscores, and `/`, and nested tags are a separate graph
#: node per full path.
TAGS = [
    "project", "reading", "idea", "todo", "tech/wgpu", "tech/rust", "meeting",
    "person/mira", "person/oskar", "area/design", "review", "archive/2026",
    "spinning-off", "parked",
]

#: Image files written into `assets/`. They are real PNGs, so embeds become
#: attachment nodes rather than ghosts.
ASSETS = [
    "diagram-atlas", "diagram-borealis", "map-core", "map-edge", "plot-degree",
    "plot-hops", "sketch-chain", "sketch-hub",
]


# ---------------------------------------------------------------------------
# Planning: who exists, who is a hub, who carries which tags
# ---------------------------------------------------------------------------

def note_title(cluster: str, i: int) -> str:
    """Title (also the file stem) for member `i` of `cluster`.

    Stems are unique across the vault except for the intentional `Ambiguous/`
    pair, so everywhere else a bare `[[Title]]` resolves exactly one way.
    """
    return f"{cluster} {i:04d}"


def plan_clusters(spec: dict[str, int]) -> list[list[str]]:
    """Split the cluster notes into unevenly sized clusters, MOC first.

    Uneven sizes matter: a 12-note cluster and a 200-note one lay out very
    differently, and a uniform split would hide that.
    """
    pool = spec["notes"] - (spec["chain"] + spec["orphans"] + EXTRA_NOTES)
    n = spec["clusters"]
    rng = random.Random(spec["seed"] + 7)
    weights = [rng.uniform(0.4, 2.2) for _ in range(n)]
    sizes = [max(6, round(pool * w / sum(weights))) for w in weights]
    # Correct the rounding and the `max(6, …)` floor on the largest clusters so
    # the total is exactly `pool`.
    drift = pool - sum(sizes)
    order = sorted(range(n), key=lambda i: -sizes[i])
    i = 0
    while drift:
        j = order[i % n]
        step = 1 if drift > 0 else -1
        if sizes[j] + step >= 6:
            sizes[j] += step
            drift -= step
        i += 1
    clusters = []
    for name, size in zip(CLUSTERS, sizes):
        clusters.append([f"{name} MOC"] + [note_title(name, i) for i in range(1, size)])
    return clusters


def plan_roles(spec: dict[str, int], clusters: list[list[str]]) -> dict[str, str]:
    """Give every cluster note a link budget, which is what skews the degrees.

    * `hub`    the cluster MOC: wide out-degree, huge in-degree
    * `star`   an ordinary note with an unusually wide fan-out (10-30 links)
    * `bridge` a note whose links mostly leave its cluster
    * `leaf`   1-3 links, the overwhelming majority
    """
    rng = random.Random(spec["seed"] + 11)
    roles: dict[str, str] = {}
    for titles in clusters:
        members = titles[1:]
        roles[titles[0]] = "hub"
        # ~6% stars and ~8% bridges, everyone else a leaf.
        stars = max(1, round(len(members) * 0.06))
        bridges = max(1, round(len(members) * 0.08))
        picked = rng.sample(members, min(stars + bridges, len(members)))
        for t in picked[:stars]:
            roles[t] = "star"
        for t in picked[stars:]:
            roles[t] = "bridge"
        for t in members:
            roles.setdefault(t, "leaf")
    return roles


def plan_tags(titles: list[str], seed: int, notes: int) -> dict[str, list[str]]:
    """Spread `TAGS` over `titles` zipf-ish, but subordinate to the notes.

    Two constraints fight each other. Every tag has to be well represented (a
    100-note fixture where the last tag appears once teaches you nothing about
    tag nodes), and the tag layer must not swallow the note layer: with naive
    zipf sampling the busiest tag touched a quarter of the 5k vault and the
    graph's community structure disappeared behind fourteen megahubs.

    So the budget is fixed at roughly a fifth of the notes carrying a tag, split
    by `1/(k+1)^1.4`, and each tag claims a random sample of notes. Overlaps
    then put two tags on a few notes by chance, as in a real vault.
    """
    rng = random.Random(seed + 13)
    weights = [1.0 / (k + 1) ** 1.4 for k in range(len(TAGS))]
    budget = max(2 * len(TAGS), round(notes * 0.22))
    shares = [max(2, round(budget * w / sum(weights))) for w in weights]
    tags_of: dict[str, list[str]] = {t: [] for t in titles}
    for tag, want in zip(TAGS, shares):
        for title in rng.sample(titles, min(want, len(titles))):
            tags_of[title].append(tag)
    return tags_of


# ---------------------------------------------------------------------------
# Links
# ---------------------------------------------------------------------------

def pick_targets(
    rng: random.Random,
    title: str,
    cluster: list[str],
    clusters: list[list[str]],
    role: str,
    spec: dict[str, int],
) -> list[str]:
    """Choose one note's wiki-link targets: never itself, never a repeat.

    Out-degree follows the role, and that spread is the fixture's whole
    purpose: it is what makes node sizes, the degree cap on labels, and the
    spring tension `link_strength / min(deg_a, deg_b)` observable side by side.
    """
    members = cluster[1:]
    own_moc = cluster[0]

    if role == "hub":
        targets = rng.sample(members, min(spec["hub_links"], len(members)))
        others = [c[0] for c in clusters if c[0] != own_moc]
        targets += rng.sample(others, min(3, len(others)))  # the dense core
        return targets

    budget = {"star": rng.randint(10, 30), "bridge": rng.randint(3, 7),
              "leaf": rng.randint(1, 3)}[role]
    targets: list[str] = []
    # Rejection sampling: leaves in a tiny cluster can offer fewer candidates
    # than their budget, so bail out rather than spin.
    for _ in range(budget * 20):
        if len(targets) == budget:
            break
        if role != "bridge" and rng.random() < 0.45:
            # Up-link to the cluster hub — this is what builds its in-degree.
            cand = own_moc
        elif role == "bridge" and rng.random() < 0.6:
            other = rng.choice([c for c in clusters if c is not cluster])
            cand = other[0] if rng.random() < 0.5 else rng.choice(other[1:] or other)
        else:
            cand = rng.choice(members) if members else own_moc
        if cand != title and cand not in targets:
            targets.append(cand)
    return targets


def decorate(rng: random.Random, target: str) -> str:
    """Render one link in a random wiki-link form.

    Plain, aliased (`[[T|alias]]`), and anchored (`[[T#Notes]]`) all resolve to
    the same note, so a fixture using only the plain form would never catch an
    extractor that mishandles the other two.
    """
    r = rng.random()
    if r < 0.15:
        return f"[[{target}|{target.split()[-1].lower()}]]"
    if r < 0.25:
        return f"[[{target}#Notes]]"
    return f"[[{target}]]"


# ---------------------------------------------------------------------------
# Note bodies
# ---------------------------------------------------------------------------

def note_body(
    rng: random.Random,
    title: str,
    targets: list[str],
    tags: list[str],
    ghost_names: list[str],
    mode: str = "full",
) -> str:
    """Write one note, exercising the constructs the graph index reads.

    `mode` decides how much connectivity the note is allowed to have:

    * `full`   links, tags, sometimes a dangling link and an image embed
    * `tagged` tags only — still not an orphan, because a tag is an edge
    * `bare`   no links and no tags, so a genuine orphan; code decoys stay

    The decoys (a fence and an inline span containing a wiki-link) are kept in
    every mode on purpose: they must never become edges or ghost nodes, and an
    orphan that survives with decoys is the proof.
    """
    lines = [f"# {title}", "", paragraph(rng, rng.randint(2, 5)), ""]
    if tags:
        lines += ["Tags: " + " ".join(f"#{t}" for t in tags), ""]

    if targets:
        lines += ["## Links", ""]
        for t in targets:
            lead = rng.choice(["see", "part of", "comes from", "answers"])
            lines.append(f"- {lead} {decorate(rng, t)} — {rng.choice(SENTENCES)}")
        lines.append("")

    if mode == "full":
        # Dangling links become ghost nodes: some shared across the vault (they
        # must collapse into one node), some unique to this note.
        if rng.random() < 0.25:
            g = rng.choice(ghost_names)
            lines += ["## Unresolved", f"A shared dangling link: [[{g}]].", ""]
        elif rng.random() < 0.06:
            lines += ["## Unresolved", f"A unique dangling link: [[Ghost of {title}]].", ""]
        # Embeds of real PNGs, so these are attachment nodes.
        if rng.random() < 0.2:
            asset = rng.choice(ASSETS)
            sized = f"|{rng.choice([240, 300, 480])}" if rng.random() < 0.4 else ""
            lines += ["## Figure", f"![[{asset}.png{sized}]]", ""]

    if rng.random() < 0.08:
        lines += ["```text", "[[Not A Link From A Fence]]", "```", ""]
    if rng.random() < 0.06:
        lines += ["Inline code holds `[[Another Decoy]]`, which is not a link.", ""]

    lines += ["## Notes", "", paragraph(rng, rng.randint(1, 3)), ""]
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Generating
# ---------------------------------------------------------------------------

def generate(key: str) -> Path:
    """Write one fixture vault from scratch and return its path."""
    spec = SPECS[key]
    out = GRAPH_DIR / key
    if out.exists():
        shutil.rmtree(out)
    (out / "assets").mkdir(parents=True)
    for i, name in enumerate(ASSETS):
        colour = ((i * 37) % 200 + 40, (i * 91) % 200 + 30, (i * 53) % 200 + 60)
        (out / "assets" / f"{name}.png").write_bytes(png_bytes(colour))

    clusters = plan_clusters(spec)
    roles = plan_roles(spec, clusters)
    ghost_names = [f"Missing Page {i:02d}" for i in range(1, spec["ghosts"] + 1)]

    # Every title in the vault up front, so links can point at notes that have
    # not been written yet.
    cluster_titles = [t for titles in clusters for t in titles]
    chain = [f"Chain {i:04d}" for i in range(1, spec["chain"] + 1)]
    orphans = [f"Orphan {i:03d}" for i in range(1, spec["orphans"] + 1)]
    tags_of = plan_tags(cluster_titles + chain + orphans, spec["seed"], spec["notes"])
    rng = random.Random(spec["seed"])

    def write(folder: str, title: str, text: str) -> None:
        """Put one note on disk at `folder`/`title.md`."""
        d = out / folder if folder != "." else out
        d.mkdir(parents=True, exist_ok=True)
        (d / f"{title}.md").write_text(text, encoding="utf-8")

    # 1. Cluster notes: MOCs and their members.
    for titles in clusters:
        name = titles[0].removesuffix(" MOC")
        for title in titles:
            targets = pick_targets(rng, title, titles, clusters, roles[title], spec)
            write(name, title, note_body(rng, title, targets, tags_of[title],
                                         ghost_names))

    # 2. The root hub: reaches every MOC and seeds the chain, and spells out
    # what to look at when the graph tab opens.
    spine = [c[0] for c in clusters]
    write(".", "Welcome", "\n".join([
        "# Welcome", "",
        f"Fixture vault `examples/graph/{key}`, generated by",
        "`tools/graph_vaults.py`. Open the graph tab (`space g`, or `:graph`)",
        "and let the simulation settle.", "",
        "## Clusters", "",
        *[f"- [[{s}]] covers {s.removesuffix(' MOC')}" for s in spine],
        "", "## A chain with depth", "",
        f"- the long chain starts at [[{chain[0]}]]",
        f"- its far end is [[{chain[-1]}]]",
        "", "## Things to look for", "",
        "- a cluster hub with a huge in-degree next to a leaf with one link",
        "- a `star` note with twenty links of its own",
        "- small dim ghost nodes for dangling links, as in `[[Missing Page 01]]`",
        f"- {spec['orphans']} orphan notes: invisible until the orphan filter is on",
        "- tag nodes: #project #reading #idea #todo #tech/rust #person/mira",
        "- an attachment node: ![[map-core.png|240]]",
        "- a self-link that must not draw a self-edge: [[Self Link]]",
        "- every link form resolves alike: [[Resolution Probe|the probe]],",
        "  [[Resolution Probe#Notes]]",
        "- colliding stems: `[[A Double]]` must land on the shallowest match",
        "- a fence link that must not become an edge:",
        "```text", "[[A fence is not a link]]", "```", "",
    ]))

    # 3. The self-link note. Reachable, so it is not an orphan, and its own link
    # points at itself, which `GraphIndex` must ignore.
    self_body = note_body(rng, "Self Link", ["Welcome"],
                          tags_of[cluster_titles[0]], ghost_names)
    write(".", "Self Link",
          self_body + "\n## Self\n\nThis note links to itself: [[Self Link]].\n")

    # 4. The colliding-stem pair, and the note that points at it. `resolve_link`
    # takes the exact path first, then the shortest stem match.
    write(".", "Resolution Probe", "\n".join([
        "# Resolution Probe", "",
        "Two notes share the stem `A Double`. `Vault::resolve_link` keeps the",
        "shortest path, so this bare link must land on `Ambiguous/A Double.md`:",
        "", "- [[A Double]]", "",
        "and the nested twin is only reachable by its exact path:", "",
        "- [[Ambiguous/Deep/A Double.md]]", "- the pair's unique sibling:",
        "  [[B Single]]", "", "## Notes", "", paragraph(rng, 2), "",
    ]))
    write("Ambiguous", "A Double",
          "# A Double\n\nThe shallow twin. A bare `[[A Double]]` link should\n"
          "resolve here.\n\n## Notes\n\n" + paragraph(rng, 1) + "\n")
    write("Ambiguous/Deep", "A Double",
          "# A Double\n\nThe nested twin at `Ambiguous/Deep/A Double.md`. Only an\n"
          "exact-path link should reach this one.\n\n## Notes\n\n"
          + paragraph(rng, 1) + "\n")
    write("Ambiguous", "B Single",
          "# B Single\n\nUnique stem, reached from [[Resolution Probe]].\n\n"
          "## Notes\n\n" + paragraph(rng, 1) + "\n")

    # 5. The depth chain: one outgoing link each, a long thin arm, with a
    # long-range jump to a hub every 20 notes.
    for i, title in enumerate(chain):
        targets = chain[i + 1:i + 2]
        if i and i % 20 == 0:
            targets = targets + [clusters[i % len(clusters)][0]]
        write("Chain", title, note_body(rng, title, targets, tags_of[title],
                                        ghost_names))

    # 6. Orphans: nothing links to them and they link to nothing. Three quarters
    # are completely bare; the rest carry tags only, which still makes them
    # non-orphan under the index (a tag is an edge) — the split is deliberate.
    for i, title in enumerate(orphans):
        tagged = i % 4 == 0
        write("Orphans", title, note_body(rng, title, [],
                                          tags_of[title] if tagged else [],
                                          ghost_names,
                                          "tagged" if tagged else "bare"))
    return out
