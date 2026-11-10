#!/usr/bin/env python3
"""Shared parts of the graph fixture generators: where things go, prose, PNGs.

Kept separate so `graph_vault_hubs.py` (the hub-shaped fixtures) and
`graph_vault_organic.py` (the realistic-shape fixtures) can both use them, and
so the hub fixtures' RNG stream stays untouched — `SENTENCES` in particular is
drawn from with `rng.sample`, so its order is part of what makes regeneration
byte-identical.
"""

import random
import struct
import zlib
import hashlib
from datetime import date, timedelta
from pathlib import Path

#: Where the fixture vaults live. Committed only as this tool plus `README.md`;
#: the vaults themselves are generated and git-ignored.
GRAPH_DIR = Path(__file__).resolve().parent.parent / "examples" / "graph"

#: File types the vault walker treats as attachments, so the reporter can tell
#: an attachment node from a ghost. Mirrors what `Vault::list_attachments` keeps.
ATTACH_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".pdf"}

#: Filler prose. These vaults are read by eye as well as by the graph, so the
#: bodies are real English sentences rather than word salad; a note that looks
#: like a note is what makes a 5k-note scroll-through legible.
SENTENCES = [
    "Records what was decided and what is still open.",
    "Carries the working notes from the last sit-down.",
    "Half-formed; the shape of it is not settled yet.",
    "Checked against the notes in the neighbouring folder.",
    "Nothing here is binding until the review passes.",
    "Split out of a longer note that had grown too wide.",
    "The thread kept drifting, so it waits here for now.",
    "Worth a link to the hub so the graph shows the join.",
    "Left deliberately vague; the detail lives in the chain.",
    "Renamed twice, so a stale link here would be no surprise.",
    "The figure below is the only part worth trusting.",
    "A short note, on purpose, so it stays readable.",
    "Superseded by the newer note, but the links still matter.",
    "One idea per note, which is why there are so many of these.",
]


#: The lifecycle vocabulary the fixtures declare. Small on purpose: a census with
#: four values is readable, and `prop:status=evergreen` is a search a person can
#: remember.
STATUSES = ("seedling", "growing", "evergreen", "unknown")

#: The frontmatter every generated note carries, so the fixtures exercise the
#: properties index, the `prop:` operators, the key census and saved queries over
#: `props` — none of which the graph fixtures had before frontmatter existed.
#:
#: Values are hashed from the title, **not** drawn from the generator's RNG. The
#: RNG stream is what decides which note links to which; a fixture whose edges
#: moved because a property was added would make `README.md`'s numbers lie, and
#: "a diff after a sim tweak means something" would stop being true.
def frontmatter(title: str) -> str:
    """A properties block for one note, stable across runs and machines."""
    digest = hashlib.sha256(title.encode("utf-8")).digest()
    status = STATUSES[digest[0] % len(STATUSES)]
    effort = 1 + digest[1] % 9
    day = digest[2] + (digest[3] << 8)
    created = date(2022, 1, 1) + timedelta(days=day % 1200)
    review = "true" if digest[4] % 3 == 0 else "false"
    return (
        "---\n"
        f"status: {status}\n"
        f"effort: {effort}\n"
        f"created: {created.isoformat()}\n"
        f"review: {review}\n"
        "---\n\n"
    )


#: Saved queries, written into every fixture so the table has something to draw at
#: fixture scale. `.shq` is Shale's own format: a `+++` fence holding a TOML
#: declaration, then one SQL statement over the vault's index.
QUERIES = {
    "Busiest notes": """+++
name = "Busiest notes"
sort = "-links_out"
+++
-- Which notes reach the most others, busiest first. The row is the note: choosing
-- it opens it. The answer is longer than the table draws, so the table says so and
-- counts what it left out.
SELECT source AS note, COUNT(*) AS links_out
FROM links
WHERE target_kind = 'note'
GROUP BY source
ORDER BY links_out DESC, note
""",
    "Evergreen notes": """+++
name = "Evergreen notes"
sort = "-effort"
+++
-- The join the index makes easy: one row per note that calls itself evergreen,
-- with the effort it claims. `props` holds one row per (note, key), so the two
-- facts arrive as two rows of the same table.
SELECT a.note, CAST(a.value AS REAL) AS effort, b.value AS created
FROM props a JOIN props b ON a.note = b.note
WHERE a.key = 'effort' AND b.key = 'status' AND b.value = 'evergreen'
ORDER BY effort DESC, a.note
""",
}


def write_queries(root: Path) -> None:
    """Put the fixture's saved queries on disk, so a table can be seen at scale."""
    folder = root / "queries"
    folder.mkdir(parents=True, exist_ok=True)
    for name, text in QUERIES.items():
        (folder / f"{name}.shq").write_text(text, encoding="utf-8")


def paragraph(rng: random.Random, k: int) -> str:
    """A paragraph of `k` distinct sentences from the bank."""
    return " ".join(rng.sample(SENTENCES, k))


def png_bytes(rgb: tuple[int, int, int]) -> bytes:
    """A valid 8x8 solid-colour PNG, built with no image library.

    The fixtures need *real* image files: if only the names existed, every embed
    would degrade to a ghost and the attachment-node path would never run.
    """
    width = height = 8
    raw = b"".join(b"\x00" + bytes(rgb) * width for _ in range(height))

    def chunk(kind: bytes, data: bytes) -> bytes:
        crc = zlib.crc32(kind + data) & 0xFFFF_FFFF
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", crc)

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw))
            + chunk(b"IEND", b""))


def write_note(root: Path, folder: str, title: str, text: str) -> None:
    """Put one note on disk at `folder`/`title.md`, making the folder first.

    Every note arrives with a properties block, because a fixture vault that has
    none exercises none of the properties machinery.
    """
    d = root / folder if folder != "." else root
    d.mkdir(parents=True, exist_ok=True)
    (d / f"{title}.md").write_text(frontmatter(title) + text, encoding="utf-8")
