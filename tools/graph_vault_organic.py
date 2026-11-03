#!/usr/bin/env python3
"""The organic fixtures: `organic-100`, `organic-1k`, `organic-5k`.

The shape these test is **natural clustering** — whether the layout pulls groups
of related notes together on its own. The hub fixtures (`100`/`1k`/`5k`) are the
wrong shape for that: a `MOC` with degree 171 and a tag carried by 468 notes each
stitch most of the vault into one blob, which is exactly what makes them good for
stress and useless for eyeballing communities. So the organic fixtures are built
the way a real personal vault is built:

  * **Small topic folders.** A cluster is one folder of 2–9 notes about one
    thing — `projects/wayland-surfacing/wayland-surfacing-open-questions.md` —
    and inside a cluster each note links one to three others. No cluster has a
    note with more than a handful of links.
  * **Sparse, specific bridges.** About one note in seven reaches outside its
    folder, usually to a neighbour in the same area (`projects/` → `projects/`),
    rarely across areas. Bridges join two notes that would plausibly be read
    together, not two hubs.
  * **Real vault furniture, at realistic degree.** A `Home` index and a
    `reading-list` (each ~10 links, not 40), a `journal/` of daily notes that
    mostly chain to yesterday, and an `ideas/` folder of half-finished singletons.
  * **Cluster-local tags.** Tag palettes belong to an area (`#wip`, `#perf`,
    `#recommend`), and only ~30% of notes carry one, so a tag node ties a few
    dozen notes together instead of a few hundred. Tags in the hub fixtures are
    global on purpose; here they would merge clusters and hide the very thing
    being tested.
  * **Mess that real vaults have.** Roughly one note in ten is an orphan, some
    links dangle at notes that were never written, one note in ~250 links itself
    by mistake, and two notes in different areas share a stem.

Every fixture also writes `.fixture.json`, a ground-truth map of note →
planted group. The layout's clustering can then be scored instead of admired:
`graph_vaults.py` reports how many note edges stay inside a group, the
modularity of that partition, and how many components the graph falls into.
"""

import json
import random
import re
import shutil
from dataclasses import dataclass, field
from datetime import date, timedelta
from pathlib import Path

from graph_vault_common import GRAPH_DIR, paragraph, png_bytes, write_note

# ---------------------------------------------------------------------------
# Vault specs
# ---------------------------------------------------------------------------

#: Fixture name -> shape. `notes` is the exact number of `.md` files: the
#: `journal_pct` and `ideas_pct` pools, the two index notes, and the colliding
#: pair are carved out of the same total, and whatever is left becomes topic
#: clusters.
SPECS: dict[str, dict[str, int]] = {
    "organic-100": {
        "notes": 100,
        "journal_pct": 12,   # daily notes, as a percentage of the vault
        "ideas_pct": 10,     # singletons in `ideas/`, mostly unlinked
        "ghosts": 5,         # dangling targets shared across notes
        "assets": 5,         # real PNGs in `assets/`
        "seed": 20_101,
    },
    "organic-1k": {
        "notes": 1_000,
        "journal_pct": 12,
        "ideas_pct": 10,
        "ghosts": 18,
        "assets": 8,
        "seed": 21_001,
    },
    "organic-5k": {
        "notes": 5_000,
        "journal_pct": 12,
        "ideas_pct": 10,
        "ghosts": 80,
        "assets": 12,
        "seed": 25_001,
    },
}

#: Cluster size range. Real topical groups are small; the weights favour the
#: bottom of the range, which is what makes each one a distinguishable clump.
CLUSTER_SIZES = [(2, 22), (3, 18), (4, 15), (5, 12), (6, 10), (7, 8), (8, 8), (9, 7)]

#: The vault's areas, weighted by how many clusters each holds. Folder names are
#: lower-case with no spaces, as most real vaults have them.
AREA_WEIGHTS = [("projects", 30), ("areas", 25), ("books", 20),
                ("references", 15), ("people", 10)]

#: Tag palettes, one per area. Names are short and local so a tag node ties
#: together notes that already sit near each other. Nested names exercise the
#: `/` in the tag grammar.
AREA_TAGS = {
    "projects": ["wip", "perf", "decision", "blocked", "rust/wgpu", "rust/sim",
                 "retro", "spike"],
    "areas": ["home/kitchen", "garden", "fix-it", "money", "health/sleep",
              "gifts", "errands", "winter"],
    "books": ["reading", "finished", "recommend", "fiction", "nonfiction",
              "reread", "quotes", "lending"],
    "references": ["tooling", "hardware", "bookmark", "cli", "archive/2025",
                   "archive/2026", "keyboard", "monitor"],
    "people": ["family", "friends", "birthday", "workmates", "gifts", "calls",
               "anniversary", "recipes"],
    "ideas": ["someday", "parked", "maybe", "someday/home"],
}

#: Topic words. A topic is a word plus an optional qualifier, and topic slugs
#: are sampled without replacement, so every folder — and so almost every note
#: stem — is unique. 72 words x 12 qualifiers outlasts the largest fixture's
#: ~750 clusters.
TOPICS = [
    "wayland", "swapchain", "frame-pacing", "allocator", "quadtree", "shader",
    "profiling", "font-shaping", "sqlite", "backups", "router", "nas",
    "sourdough", "espresso", "tomatoes", "freezer", "cast-iron", "preserves",
    "bread", "coffee", "cider", "hiking", "bike", "commute", "train",
    "passphrase", "insurance", "taxes", "pension", "garden", "compost",
    "bees", "chickens", "wool", "sewing", "shelving", "desk", "lighting",
    "paint", "plumbing", "boiler", "drafting", "film", "camera", "vinyl",
    "piano", "chess", "jigsaw", "sudoku", "sleep", "walking", "dentist",
    "physio", "knee", "allergies", "reading", "note-taking", "zotero",
    "vim", "tmux", "docker", "dns", "wireguard", "photos", "scans", "music",
    "podcasts", "board-games", "dnd", "guitar", "furniture", "stairs",
    "hammock", "kayak", "sourdough-starter", "canning", "sausage", "kefir",
    "tallow", "chisel", "handplane", "finish", "dovetail", "bookshelf",
    "desk-lamp", "soundproofing",
]
#: Qualifiers, so 4 areas x ~90 topics x these outlast the largest fixture's
#: ~850 clusters with room to spare. A real vault does have a `sourdough` and a
#: `sourdough-2026`, so the repeats read honestly rather than as counters.
QUALIFIERS = ["", "-2025", "-2026", "-2024", "-v2", "-home", "-work",
              "-small", "-long", "-north", "-south", "-winter", "-summer",
              "-second", "-weekend", "-old", "-main", "-maybe"]

#: Note names inside a cluster. Stems stay unique because a cluster samples its
#: roles without replacement and its topic is unique.
ROLES = [
    "notes", "summary", "open-questions", "next-steps", "decision",
    "comparison", "measurements", "shopping-list", "prices", "what-broke",
    "review", "draft", "lessons", "timeline", "links", "people", "budget",
    "checklist", "faq", "glossary", "first-try", "second-try", "fixes",
    "parts", "vendors", "questions", "setup", "results",
]

#: The role a cluster's anchor note takes: the one its siblings link to.
ANCHORS = ["notes", "summary", "plan"]

#: Real image files, written once per fixture and embedded by a few notes.
ASSET_NAMES = [
    "photo-shelf", "photo-sourdough", "photo-desk", "whiteboard-render",
    "whiteboard-garden", "diagram-pipes", "diagram-wiring", "plot-frame-times",
    "plot-sleep", "map-walks", "scan-receipt", "scan-lease", "photo-boiler",
    "photo-loom", "photo-bedframe", "diagram-vlan",
]

#: The two index notes. Real vaults do have them; what they must not have here
#: is the hub-of-everything degree the stress fixtures give their `MOC`s. An
#: index note names a few clusters and one or two notes from each, which is how
#: a person actually writes one — and it keeps the graph in pieces.
INDEX_CLUSTERS = 4
INDEX_PER_CLUSTER = 2


@dataclass
class Cluster:
    """One topic folder: a group of notes that mostly link to each other."""

    #: The vault area it sits in, e.g. `projects`.
    area: str
    #: The topic slug, e.g. `wayland-swapchain`. Unique across the vault.
    slug: str
    #: Note stems inside the cluster, anchor first.
    notes: list[str] = field(default_factory=list)

    @property
    def folder(self) -> str:
        """Vault-relative folder for this cluster, and its ground-truth group."""
        return f"{self.area}/{self.slug}"


# ---------------------------------------------------------------------------
# Planning: the folders, the dailies, the singles
# ---------------------------------------------------------------------------

def pick_area(rng: random.Random) -> str:
    """Pick an area for a cluster, weighted by `AREA_WEIGHTS`."""
    return rng.choices([a for a, _ in AREA_WEIGHTS],
                       weights=[w for _, w in AREA_WEIGHTS], k=1)[0]


def plan_clusters(spec: dict[str, int], rng: random.Random,
                  pool: list[str]) -> tuple[list[Cluster], list[str]]:
    """Fill the vault with topic clusters until the note budget runs out.

    Sizes come from `CLUSTER_SIZES`, biased small. Building clusters until the
    budget is exhausted (rather than fixing a cluster count) is what keeps the
    total note count exact without forcing awkward sizes.
    """
    budget = (spec["notes"] - spec["notes"] * spec["journal_pct"] // 100
              - spec["notes"] * spec["ideas_pct"] // 100 - 4)
    clusters: list[Cluster] = []
    while budget >= 2:
        size = min(budget, rng.choices([s for s, _ in CLUSTER_SIZES],
                                       weights=[w for _, w in CLUSTER_SIZES],
                                       k=1)[0])
        if not pool:  # topic words exhausted: number the leftovers
            slug = f"misc-{len(clusters) + 1}"
        else:
            slug = pool.pop()
        # The anchor's role is excluded from the siblings' roles, or two notes
        # in the cluster land on the same file name and one overwrites the
        # other — which quietly shortens the vault by a note per collision.
        anchor_role = rng.choice(ANCHORS)
        roles = rng.sample([r for r in ROLES if r != anchor_role], size - 1)
        clusters.append(Cluster(pick_area(rng), slug,
                                [f"{slug}-{anchor_role}"]
                                + [f"{slug}-{r}" for r in roles]))
        budget -= size
    if budget == 1 and clusters:  # a one-note cluster would be an oddity
        spare = [r for r in ROLES if r not in
                 [n.removeprefix(clusters[-1].slug + "-") for n in clusters[-1].notes]]
        clusters[-1].notes.append(f"{clusters[-1].slug}-{spare[0] if spare else 'extra'}")
    return clusters, pool


def plan_dailies(spec: dict[str, int], rng: random.Random) -> list[str]:
    """Daily notes as `YYYY-MM-DD`, going back from a fixed date.

    A real journal has gaps: some weekends were never written, so those dates
    simply do not exist — which is also why the chain is not one unbroken line.
    """
    count = spec["notes"] * spec["journal_pct"] // 100
    dates: list[str] = []
    cursor = date(2026, 10, 31)
    while len(dates) < count:
        if cursor.weekday() < 5 or rng.random() < 0.5:
            dates.append(cursor.isoformat())
        cursor -= timedelta(days=1)
    return sorted(dates)


#: How many notes carry a tag, and how wide a tag may spread. Both matter for
#: clustering: a tag node is an edge to every note that carries it, so a popular
#: area tag ties notes in a dozen unrelated folders together (measured: an
#: uncapped `#wip` reached 53 notes in the 1k fixture — the hub fixtures' failure
#: in miniature). The share is what a real vault looks like; the cap, as a
#: fraction of the cluster count, is what keeps one tag from spanning them all.
TAGGED_SHARE = 0.31
TAG_SPREAD = 0.09


def cluster_tags(clusters: list[Cluster], ideas: list[str], rng: random.Random) -> dict[str, list[str]]:
    """Spread each area's tag palette over about a third of its notes.

    Each palette's budget is split zipf-ish into a per-tag quota, capped at
    `TAG_SPREAD` of the cluster count, and any quota the cap clipped is handed
    to the tail tags. Splitting the budget up (rather than drawing a tag per
    note and rejecting the full ones) is what keeps the distribution smooth:
    rejection sampling left eleven tags pinned at exactly the cap, which no real
    vault looks like.
    """
    tags_of: dict[str, list[str]] = {}
    by_area: dict[str, list[str]] = {}
    for cluster in clusters:
        by_area.setdefault(cluster.area, []).extend(cluster.notes)
    by_area.setdefault("ideas", []).extend(ideas)

    cap = max(8, round(len(clusters) * TAG_SPREAD))
    for area, notes in sorted(by_area.items()):
        palette = AREA_TAGS[area]
        weights = [1.0 / (k + 1) ** 0.8 for k in range(len(palette))]
        budget = round(len(notes) * TAGGED_SHARE)
        quota = [min(cap, max(1, round(budget * w / sum(weights)))) for w in weights]
        # Hand the clipped-away demand to the tags with room left, biggest need
        # first, so the tail fills in rather than the head overflowing.
        short = budget - sum(quota)
        order = sorted(range(len(palette)), key=lambda i: cap - quota[i], reverse=True)
        i = 0
        while short > 0:
            j = order[i % len(order)]
            if quota[j] < cap:
                quota[j] += 1
                short -= 1
            i += 1
        for tag, want in zip(palette, quota):
            if want <= 0:
                continue
            for stem in rng.sample(notes, min(want, len(notes))):
                tags_of.setdefault(stem, [])
                if tag not in tags_of[stem]:
                    tags_of[stem].append(tag)
    return {stem: tags for stem, tags in tags_of.items() if tags}


# ---------------------------------------------------------------------------
# Links
# ---------------------------------------------------------------------------

def plan_links(
    rng: random.Random,
    clusters: list[Cluster],
    dailies: list[str],
    ideas: list[str],
    ghost_names: list[str],
    spec_assets: int,
) -> tuple[dict[str, list[str]], dict[str, str]]:
    """Decide every link before any note is written.

    Returns `(targets, extra)` where `targets` maps a note stem to the stems it
    links, and `extra` records the incidental constructs a note got — a dangling
    link, an embed, a self-link typo — so the body writer can render them.

    The probabilities are the shape of the fixture: a cluster holds itself
    together, one note in seven reaches out, and almost nothing reaches across
    areas. That is what a force layout has to be able to pull apart.
    """
    targets: dict[str, list[str]] = {}
    extra: dict[str, str] = {}
    stems = {c.slug: c.notes for c in clusters}
    area_slugs: dict[str, list[str]] = {}
    for c in clusters:
        area_slugs.setdefault(c.area, []).append(c.slug)
    all_notes = [n for notes in stems.values() for n in notes]

    for cluster in clusters:
        siblings = cluster.notes
        anchor = siblings[0]
        for i, stem in enumerate(siblings):
            mine: list[str] = []
            if stem == anchor:
                mine += rng.sample(siblings[1:], min(len(siblings) - 1,
                                                     rng.randint(1, 3)))
            else:
                if rng.random() < 0.78:
                    mine.append(anchor)
                if len(siblings) > 2 and rng.random() < 0.5:
                    other = rng.choice([s for s in siblings if s != stem])
                    if other not in mine:
                        mine.append(other)
            # Bridges: usually to a sibling cluster in the same area, rarely
            # somewhere else in the vault. These rates are what decide whether
            # the layout has anything to pull apart — at 0.12/0.04 the whole
            # vault welds into one component and Q lands around 0.6.
            if rng.random() < 0.07:
                nearby = [s for s in area_slugs[cluster.area] if s != cluster.slug]
                if nearby:
                    mine.append(rng.choice(stems[rng.choice(nearby)]))
            if rng.random() < 0.02 and all_notes:
                mine.append(rng.choice(all_notes))
            # Incidental noise a real vault always has.
            if rng.random() < 0.03:
                extra[stem] = f"ghost:{rng.choice(ghost_names)}"
            elif rng.random() < 0.035:
                extra[stem] = f"embed:{rng.choice(ASSET_NAMES[:spec_assets])}.png"
            elif rng.random() < 0.004:
                extra[stem] = f"self:{stem}"
            targets[stem] = list(dict.fromkeys(t for t in mine if t != stem))

    # Journal: yesterday, sometimes tomorrow, and one or two things worked on.
    for i, day in enumerate(dailies):
        mine: list[str] = []
        if i > 0:
            mine.append(dailies[i - 1])
        if rng.random() < 0.3:
            mine.append(rng.choice(dailies))
        if rng.random() < 0.2:
            mine.append(rng.choice(all_notes))
        if rng.random() < 0.06:
            extra[day] = f"ghost:{rng.choice(ghost_names)}"
        targets[day] = list(dict.fromkeys(t for t in mine if t != day))

    # Ideas: mostly a note that points nowhere, which is why a real vault has
    # orphans.
    for idea in ideas:
        mine = []
        if rng.random() < 0.1 and all_notes:
            mine.append(rng.choice(all_notes))
        if rng.random() < 0.05 and dailies:
            mine.append(rng.choice(dailies))
        targets[idea] = mine
    return targets, extra


# ---------------------------------------------------------------------------
# Bodies
# ---------------------------------------------------------------------------

def display_title(stem: str) -> str:
    """The H1 for a note: dates stay dates, other stems read as phrases.

    A real vault uses the file name as the title — a daily is `2026-10-20`, not
    "2026 10 20" — while a folder note like `board-games-home-parts` is easier
    to read spelled out.
    """
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", stem):
        return stem
    return stem.replace("-", " ").capitalize()


def link_text(rng: random.Random, stem: str) -> str:
    """One link, sometimes aliased the way people alias them.

    The hub fixtures cover the anchored form (`[[T#Notes]]`); here it is the
    plain and aliased forms, which is what a real vault is mostly made of.
    """
    if rng.random() < 0.12:
        return f"[[{stem}|{stem.rsplit('-', 1)[-1]}]]"
    return f"[[{stem}]]"


def body(rng: random.Random, title: str, links: list[str], tags: list[str],
         extra: str, folder_display: str) -> str:
    """Render one organic note: short, plain, and full of small real habits.

    `extra` is the incidental construct from `plan_links` — `ghost:Name`,
    `embed:file.png`, or `self:Name` (the accidental self-link). The `folder`
    line mirrors how people actually write: a bit of context above the links.
    """
    lines = [f"# {title}", ""]
    if rng.random() < 0.15:
        # Frontmatter, on a minority of notes. Shale has no frontmatter model,
        # so this is plain text to it — realism in the editor, nothing in the
        # graph.
        lines += ["---", f"created: 2026-{rng.randint(1, 10):02d}-"
                  f"{rng.randint(1, 28):02d}", f"area: {folder_display}", "---", ""]
    lines += [paragraph(rng, rng.randint(1, 2)), ""]
    if tags:
        lines += [f"Tags: {' '.join('#' + t for t in tags)}", ""]
    if links:
        lines += ["More:", ""]
        for t in links:
            lead = rng.choice(["see", "comes out of", "answers", "part of",
                               "contradicts"])
            lines.append(f"- {lead} {link_text(rng, t)} — {rng.choice(ASIDES)}")
        lines.append("")
    if extra.startswith("ghost:"):
        target = extra.removeprefix("ghost:")
        lines += [f"Wanted to write up [[{target}]] one day.", ""]
    elif extra.startswith("embed:"):
        asset = extra.removeprefix("embed:")
        sized = f"|{rng.choice([240, 320])}" if rng.random() < 0.4 else ""
        lines += [f"![[{asset}{sized}]]", ""]
    elif extra.startswith("self:"):
        # The accidental self-link: real vaults have these, and `GraphIndex`
        # must ignore it rather than loop forever.
        lines += [f"(Yes, that link points back at this note — a typo that "
                  f"stayed: [[{extra.removeprefix('self:')}]]).", ""]
    if rng.random() < 0.03:
        lines += ["```sh", "rg 'swapchain' crates/", "# [[not-a-link]]", "```", ""]
    return "\n".join(lines)


#: One-line asides after a link, so a link line reads like a person wrote it.
ASIDES = [
    "the numbers are in there", "needs a decision, not more notes",
    "this is where it started", "write it up properly someday",
    "half of it is out of date", "the useful half is the last section",
    "kept because of the price list", "superseded but still linked",
    "worth a second read", "the photo is the point",
    "the parts list is what matters", "compare it with the older one",
]


# ---------------------------------------------------------------------------
# Generating
# ---------------------------------------------------------------------------

def generate(key: str) -> Path:
    """Write one organic fixture vault, plus its `.fixture.json` ground truth."""
    spec = SPECS[key]
    out = GRAPH_DIR / key
    if out.exists():
        shutil.rmtree(out)
    rng = random.Random(spec["seed"])

    # Topic pool: word + optional qualifier, shuffled so qualifiers are spread.
    pool = [f"{word}{qual}" for word in TOPICS for qual in QUALIFIERS]
    rng.shuffle(pool)

    clusters, pool = plan_clusters(spec, rng, pool)
    dailies = plan_dailies(spec, rng)
    idea_count = max(0, spec["notes"] - sum(len(c.notes) for c in clusters)
                     - len(dailies) - 4)
    ideas = [pool.pop() for _ in range(min(idea_count, len(pool)))]
    ideas += [f"idea-{i}" for i in range(idea_count - len(ideas))]

    # Targets for links that were typed and never written. Named like notes a
    # person meant to make, so they read plausibly in the graph's ghost filter.
    ghost_roles = ["writeup", "notes", "ideas", "page", "follow-up", "review"]
    ghost_names: list[str] = []
    while len(ghost_names) < spec["ghosts"]:
        name = (f"{rng.choice(TOPICS)}-{ghost_roles[len(ghost_names) % len(ghost_roles)]}")
        if name not in ghost_names:
            ghost_names.append(name)

    (out / "assets").mkdir(parents=True)
    assets = ASSET_NAMES[:spec["assets"]]
    for i, name in enumerate(assets):
        colour = ((i * 41) % 190 + 40, (i * 83) % 190 + 30, (i * 29) % 190 + 70)
        (out / "assets" / f"{name}.png").write_bytes(png_bytes(colour))

    tags_of = cluster_tags(clusters, ideas, rng)
    targets, extra = plan_links(rng, clusters, dailies, ideas, ghost_names,
                                spec["assets"])

    groups: dict[str, list[str]] = {}

    def note(folder: str, stem: str, group: str) -> None:
        """Write one note and record which planted group it belongs to."""
        text = body(rng, display_title(stem),
                    targets.get(stem, []), tags_of.get(stem, []),
                    extra.get(stem, ""), group.split("/")[0])
        write_note(out, folder, stem, text)
        groups.setdefault(group, []).append(f"{folder}/{stem}.md")

    for cluster in clusters:
        for stem in cluster.notes:
            note(cluster.folder, stem, cluster.folder)
    for day in dailies:
        note("journal", day, "journal")
    for idea in ideas:
        note("ideas", idea, f"ideas/{idea}")

    # The index notes: modest, real, and the closest thing to a hub here. They
    # name a few clusters each, two notes from a cluster, so one index note
    # touches at most INDEX_CLUSTERS * INDEX_PER_CLUSTER notes.
    index_targets = [s for c in rng.sample(clusters, min(INDEX_CLUSTERS, len(clusters)))
                     for s in c.notes[:INDEX_PER_CLUSTER]]
    write_note(out, ".", "Home", "\n".join([
        "# Home", "",
        f"Vault `examples/graph/{key}`, generated by `tools/graph_vaults.py`.",
        "Shape: small topic folders, sparse bridges, cluster-local tags — the",
        "point is that the layout has to find the groups on its own.", "",
        "## Jumping-off points", "",
        *[f"- [[{t}]]" for t in index_targets],
        "", "The mess that is the point:",
        "- two notes share a stem: [[renovation-budget]] lands on the shallowest",
        "  match; the deeper one needs its full path:",
        "  [[areas/home/renovation/renovation-budget.md]]",
        "- no index note here links more than a handful of others, on purpose",
        "",
    ]))
    groups.setdefault("index", []).append("Home.md")

    book_notes = [n for c in clusters if c.area == "books" for n in c.notes]
    list_targets = rng.sample(book_notes, min(INDEX_CLUSTERS * INDEX_PER_CLUSTER,
                                              len(book_notes))) \
        if book_notes else index_targets[:4]
    write_note(out, ".", "reading-list", "\n".join([
        "# Reading list", "", "In the order I actually got to them.", "",
        *[f"- [ ] [[{t}]]" for t in list_targets], "",
    ]))
    groups.setdefault("index", []).append("reading-list.md")

    # The colliding stems: one shallow, one nested, same file name. `Home` links
    # the bare stem, so the shortest path has to win.
    write_note(out, "books/renovation", "renovation-budget",
               "# Renovation budget\n\nThe shallow one: a bare "
               "`[[renovation-budget]]` should resolve here.\n\n"
               + paragraph(rng, 1) + "\n")
    groups.setdefault("books/renovation", []).append(
        "books/renovation/renovation-budget.md")
    write_note(out, "areas/home/renovation", "renovation-budget",
               "# Renovation budget\n\nThe nested one. Only an exact path "
               "reaches it from elsewhere in the vault.\n\n"
               + paragraph(rng, 1) + "\n")
    groups.setdefault("areas/home/renovation", []).append(
        "areas/home/renovation/renovation-budget.md")

    # Ground truth for scoring the layout's clustering.
    (out / ".fixture.json").write_text(json.dumps(
        {"shape": "organic", "seed": spec["seed"],
         "groups": {g: sorted(p) for g, p in sorted(groups.items())}},
        indent=1, sort_keys=True), encoding="utf-8")
    return out
