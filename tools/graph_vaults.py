#!/usr/bin/env python3
"""Generate and report the link-graph fixture vaults under `examples/graph/`.

Two shapes, three scales each, all deterministic — every fixture has its own
seed, so regenerating is byte-identical and a diff after a sim tweak means
something.

  100 / 1k / 5k              the hub shape: `MOC` hubs with 14-40 outgoing links,
                             explicit star/bridge/leaf link budgets, one root
                             hub, a long chain. The stress shape — huge degrees,
                             one connected component, a dense core with sparse
                             arms. Good for frame cost and filters; it settles
                             into a single blob, so it says nothing about
                             clustering.
  organic-100 / -1k / -5k    a vault shaped like a real one: topic folders of
                             2-9 notes, sparse bridges between specific notes,
                             cluster-local tags, a journal chain, an `ideas/`
                             folder of singletons. The natural-clustering shape,
                             with a `.fixture.json` ground-truth map so layout
                             grouping can be scored rather than admired.

Every run prints the graph each fixture produces, including clustering numbers
(intra-group edge share, the planted partition's modularity, component count).
The vaults are git-ignored: the tool and `examples/graph/README.md` are the
source of truth.

Usage:
  python3 tools/graph_vaults.py                  # regenerate all six
  python3 tools/graph_vaults.py organic-1k 5k    # just these
  python3 tools/graph_vaults.py --stats          # report, write nothing
  python3 tools/graph_vaults.py --list           # fixture names and shapes
"""

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path

from graph_vault_common import ATTACH_SUFFIXES, GRAPH_DIR
import graph_vault_hubs
import graph_vault_organic

#: Fixture name -> the module that writes it. Each module keeps its own spec
#: table, because the knobs are shape-specific.
FIXTURES: dict[str, tuple] = {
    **{name: graph_vault_hubs for name in graph_vault_hubs.SPECS},
    **{name: graph_vault_organic for name in graph_vault_organic.SPECS},
}

LINK_RE = re.compile(r"(!?)\[\[([^\[\]]*)\]\]")
INLINE_CODE_RE = re.compile(r"`[^`]*`")
FENCE_RE = re.compile(r"^\s*(```|~~~)")
TAG_RE = re.compile(r"(?<![A-Za-z0-9_/])#([A-Za-z0-9_][A-Za-z0-9_/-]*)")


def read_links(path: Path) -> list[tuple[str, bool]]:
    """One note's wiki-links as `(target, is_embed)`, with code skipped.

    A deliberate simplification of `markdown::extract_wikilinks` (no nested
    brackets, no blockrefs). These numbers sanity-check the fixtures; they are
    not a second implementation to test shale against — if the two ever
    disagree, shale is right.
    """
    found: list[tuple[str, bool]] = []
    fence = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if FENCE_RE.match(line):
            fence = not fence
            continue
        if fence:
            continue
        for m in LINK_RE.finditer(INLINE_CODE_RE.sub("", line)):
            target = m.group(2).split("|", 1)[0].split("#", 1)[0].strip()
            if target:
                found.append((target, m.group(1) == "!"))
    return list(dict.fromkeys(found))  # de-dupe per note, as `scan_note_links` does


def read_tags(path: Path) -> list[str]:
    """The `#tags` in one note, close enough to `extract_tags` to count nodes.

    Heading lines and code are skipped, matching the style scanner, which never
    styles a `#` that starts a heading.
    """
    found: list[str] = []
    fence = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if FENCE_RE.match(line):
            fence = not fence
            continue
        if fence or line.lstrip().startswith("#"):
            continue
        found += TAG_RE.findall(INLINE_CODE_RE.sub("", line))
    return list(dict.fromkeys(found))


def planted_groups(root: Path, notes: list[str]) -> tuple[dict[str, str], str]:
    """Map each note to its group, and say where the grouping came from.

    The organic fixtures write `.fixture.json` with the planted topic folders.
    The hub fixtures plant nothing, so their groups fall back to the top-level
    folder — an honest stand-in (it is what the graph's folder filter sees) that
    still lets the two shapes be compared on the same footing.
    """
    meta = root / ".fixture.json"
    if meta.is_file():
        data = json.loads(meta.read_text(encoding="utf-8"))
        return ({p: group for group, paths in data["groups"].items() for p in paths},
                ".fixture.json")
    return ({p: p.split("/")[0] for p in notes}, "top-level folder")


def clustering(root: Path, notes: list[str],
               directed: set[tuple[str, str]]) -> None:
    """Print the numbers that say whether a vault is cluster-shaped at all.

    Only note-to-note edges count: tags, ghosts, and attachments sit on top of
    the note structure, and folding them in would report the tag layer rather
    than the vault. The layout sees all of it, which is why the edge table above
    breaks the same edges out by kind.

    Modularity is Newman's Q for the planted partition: how much denser each
    group is internally than its degrees would give at random. Around 0.3 is
    weak structure; 0.7+ means the vault falls into distinct clumps by itself,
    which is what a force layout has any hope of showing.
    """
    pairs = {(a, b) if a < b else (b, a) for a, b in directed}
    m = len(pairs)
    if not m:
        return
    groups, source = planted_groups(root, notes)

    inside = sum(1 for a, b in pairs if groups.get(a) == groups.get(b))
    inner: Counter[str] = Counter()
    deg: Counter[str] = Counter()
    for a, b in pairs:
        ga, gb = groups.get(a, a), groups.get(b, b)
        deg[ga] += 1
        deg[gb] += 1
        if ga == gb:
            inner[ga] += 1
    q = sum(inner[g] / m - (deg[g] / (2 * m)) ** 2 for g in deg)

    # Connected components over the same edges: a vault with real groups falls
    # into many, a hairball into one.
    parent = {n: n for a, b in pairs for n in (a, b)}

    def find(x: str) -> str:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for a, b in pairs:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb
    sizes: Counter[str] = Counter(find(n) for n in parent)
    linked = len(parent)
    biggest = sizes.most_common(1)[0][1]

    per_group = sorted(Counter(groups.get(n, n) for n in notes).values())
    # How many notes reach outside their own group at all. This is the number
    # that decides whether groups stay separable on screen: every bridging note
    # is a spring pulling two clumps together, and the hub fixtures have one per
    # note.
    cross = {a for a, b in directed if groups.get(a) != groups.get(b)}
    print(f"  clustering, note-to-note edges only (groups from {source})")
    print(f"    groups {len(per_group)}  size min {per_group[0]} "
          f"median {per_group[len(per_group) // 2]} max {per_group[-1]}")
    print(f"    notes linking outside their group {len(cross)} of {len(notes)} "
          f"({100 * len(cross) // len(notes)}%)")
    print(f"    edges inside a group {inside} of {m} ({100 * inside // m}%)")
    print(f"    modularity of the planted partition Q {q:.3f}")
    print(f"    components {len(sizes)}  largest holds {biggest} of {linked} "
          f"linked notes ({100 * biggest // max(1, linked)}%)")


def stats(key: str) -> None:
    """Print what graph a fixture produces: degrees, orphans, ghosts, tags.

    Resolution follows `Vault::resolve_link` / `resolve_attachment` — exact path
    first, then case-insensitive stem or name, shortest path winning a tie — so
    an ambiguous link here lands where shale would land too.
    """
    root = GRAPH_DIR / key
    if not root.is_dir():
        print(f"examples/graph/{key}: not generated yet")
        return
    print(f"examples/graph/{key}")
    notes = [p for p in sorted(root.rglob("*.md")) if ".shale" not in p.parts]
    rel_notes = {str(p.relative_to(root)): p for p in notes}
    lower_notes = {k.lower(): v for k, v in rel_notes.items()}
    stems: dict[str, list[Path]] = {}
    for p in notes:
        stems.setdefault(p.stem.lower(), []).append(p)
    attach = [p for p in sorted(root.rglob("*"))
              if p.suffix.lower() in ATTACH_SUFFIXES and ".shale" not in p.parts]
    rel_attach = {str(p.relative_to(root)): p for p in attach}
    lower_attach = {k.lower(): v for k, v in rel_attach.items()}
    by_name, by_stem = {}, {}
    for p in attach:
        by_name.setdefault(p.name.lower(), []).append(p)
        by_stem.setdefault(p.stem.lower(), []).append(p)

    def shortest(paths: list[Path]) -> str:
        """Shortest path wins an ambiguous match, as `resolve_link` does."""
        best = min(paths, key=lambda p: len(p.relative_to(root).parts))
        return str(best.relative_to(root))

    def resolve(target: str) -> tuple[str, str]:
        """Classify a link target as note / attachment / ghost."""
        rel = target.replace("\\", "/")
        if rel in rel_notes:
            return "note", rel
        if rel + ".md" in rel_notes:
            return "note", rel + ".md"
        if rel.lower() in lower_notes:
            return "note", str(lower_notes[rel.lower()].relative_to(root))
        if rel.lower() in lower_attach:
            return "attachment", str(lower_attach[rel.lower()].relative_to(root))
        hits = stems.get(rel.lower()) or stems.get(rel.lower().removesuffix(".md"))
        if hits:
            return "note", shortest(hits)
        # An attachment is addressed by file name (`map-core.png`) or by stem
        # (`map-core`); both are accepted, shortest path wins a tie.
        name = by_name.get(rel.lower()) or by_stem.get(rel.lower())
        if name:
            return "attachment", shortest(name)
        return "ghost", target

    def add_edge(a: str, b: str) -> None:
        """Record one undirected edge, collapsing `A->B` and `B->A`."""
        e = frozenset((a, b))
        if e in edges:
            return
        edges.add(e)
        adj[a] += 1
        adj[b] += 1

    out_degree: dict[str, int] = {}
    edges: set[frozenset[str]] = set()
    edge_kinds: Counter[str] = Counter()  # edges by the kind of the far endpoint
    adj: Counter[str] = Counter()      # undirected degree, as the graph counts it
    incoming: Counter[str] = Counter()  # links pointing in, i.e. backlink-ish
    directed: set[tuple[str, str]] = set()  # note->note links, both directions kept
    ghosts: Counter[str] = Counter()
    tags: Counter[str] = Counter()

    for p in notes:
        path_key = str(p.relative_to(root))
        adj[path_key] += 0
        reached: set[str] = set()
        for target, _embed in read_links(p):
            kind, target_key = resolve(target)
            other = {"note": target_key, "attachment": f"asset:{target_key}",
                     "ghost": f"ghost:{target_key}"}[kind]
            if kind == "note" and other == path_key:
                continue  # self-links produce no edge
            add_edge(path_key, other)
            if kind == "note":
                directed.add((path_key, other))
            incoming[other] += 1
            reached.add(other)
            if kind == "ghost":
                ghosts[target_key] += 1
        for tag in read_tags(p):
            tags[tag] += 1
            add_edge(path_key, f"tag:{tag}")
            incoming[f"tag:{tag}"] += 1
            reached.add(f"tag:{tag}")
        out_degree[path_key] = len(reached)

    for e in edges:
        a, b = sorted(e)
        kind = "note" if not (a.startswith(("tag:", "ghost:", "asset:"))
                             or b.startswith(("tag:", "ghost:", "asset:"))) else (
            "tag" if (a.startswith("tag:") or b.startswith("tag:")) else
            "ghost" if (a.startswith("ghost:") or b.startswith("ghost:")) else "asset")
        edge_kinds[kind] += 1

    out = sorted(out_degree.values())
    total = sorted(adj.values())  # every node: notes, ghosts, attachments, tags
    linked = sum(1 for d in out_degree.values() if d)
    orphans = [n for n in rel_notes if not adj[n]]
    # Note edges that exist in both directions: `EdgeMeta::bidirectional`, which
    # M4's arrow display depends on.
    both = sum(1 for a, b in directed if (b, a) in directed and a < b)

    def pct(arr: list[int], q: float) -> int:
        return arr[min(len(arr) - 1, int(len(arr) * q))] if arr else 0

    print(f"examples/graph/{key}")
    print(f"  notes {len(notes)}  with at least one outgoing edge {linked}")
    print(f"  undirected edges {len(edges)}  "
          + "  ".join(f"{k}:{edge_kinds[k]}" for k in ("note", "tag", "ghost", "asset")))
    print(f"  note edges linked both ways {both} of {edge_kinds['note']}")
    print(f"  outgoing per note  min {out[0]}  median {pct(out, 0.5)}  "
          f"p90 {pct(out, 0.9)}  max {out[-1]}")
    print(f"  degree of a node   min {total[0]}  median {pct(total, 0.5)}  "
          f"p90 {pct(total, 0.9)}  max {total[-1]}")
    print(f"  notes with exactly 1 link {sum(1 for d in out if d == 1)}"
          f"   with 10 or more {sum(1 for d in out if d >= 10)}")
    print(f"  true orphans (no edge either way) {len(orphans)}")
    print(f"  ghost nodes {len(ghosts)}  "
          f"busiest {ghosts.most_common(1)[0] if ghosts else '-'}")
    print(f"  tag nodes {len(tags)}: " + " ".join(
        f"#{t}={n}" for t, n in tags.most_common()))
    print(f"  attachment nodes {len(attach)}")
    hub = max(adj.items(), key=lambda kv: kv[1], default=("-", 0))
    quiet = min(((adj[n], n) for n in rel_notes), default=(0, "-"))
    print(f"  busiest node {hub[0]}  degree {hub[1]}   "
          f"quietest note {quiet[1]}  degree {quiet[0]}")
    top_notes = sorted(rel_notes, key=lambda n: -adj[n])[:3]
    print("  busiest notes: " + "  ".join(f"{n}={adj[n]}" for n in top_notes))
    clustering(root, list(rel_notes), directed)
    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> int:
    """Generate the fixtures, then report the graph each one produces."""
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("vaults", nargs="*", choices=sorted(FIXTURES),
                    help="which fixtures (default: all six)")
    ap.add_argument("--stats", action="store_true",
                    help="only report on what is on disk; write nothing")
    ap.add_argument("--list", action="store_true",
                    help="name the fixtures and the module that writes each")
    args = ap.parse_args()

    if args.list:
        for name, module in sorted(FIXTURES.items()):
            shape = module.__name__.removeprefix("graph_vault_")
            print(f"{name:12} {shape}")
        return 0

    keys = args.vaults or sorted(FIXTURES)
    GRAPH_DIR.mkdir(parents=True, exist_ok=True)
    for key in keys:
        if not args.stats:
            out = FIXTURES[key].generate(key)
            print(f"wrote {out}  ({sum(1 for _ in out.rglob('*.md'))} notes)")
        stats(key)
    return 0


if __name__ == "__main__":
    sys.exit(main())
