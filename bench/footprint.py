#!/usr/bin/env python3
"""Per-vault metadata bloat + install footprint.

  footprint.py

Measures, after each app has opened the same vault copy (cold_start does
this; footprint just inspects the leftovers):
  - size of .obsidian/ .shale/ .zennotes/ notes.sqlite inside the vault
  - install footprint per app (flatpak, /opt, rpm file lists)
Writes bench/results/footprint.json. Read-only w.r.t. the apps; only
reads sizes.
"""

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as B

VAULT_DIRS = {
    "obsidian": ".obsidian",
    "shale": ".shale",
    "zennotes": ".zennotes",
    "qownnotes": "notes.sqlite",  # QOwnNotes drops its DB in the notes folder
}


def du(path: Path) -> int:
    """Bytes of a file or dir tree (0 if missing)."""
    if not path.exists():
        return 0
    if path.is_file():
        return path.stat().st_size
    total = 0
    for p in path.rglob("*"):
        if p.is_file():
            try:
                total += p.stat().st_size
            except OSError:
                pass
    return total


def sh(cmd: list[str]) -> str:
    return subprocess.run(cmd, capture_output=True, text=True,
                          timeout=30).stdout.strip()


def main() -> int:
    B.RESULTS.mkdir(exist_ok=True)
    vaults = {}
    for v in sorted(B.VAULTS.iterdir()):
        if not v.is_dir():
            continue
        vaults[v.name] = {
            app: du(v / sub) for app, sub in VAULT_DIRS.items()
        }
    install = {
        "obsidian_flatpak": sh(["du", "-sh",
                                "/var/lib/flatpak/app/md.obsidian.Obsidian"]),
        "zennotes_opt": sh(["du", "-sh", "/opt/ZenNotes"]),
        "qownnotes_rpm": sh(["rpm", "-q", "--queryformat",
                             "%{SIZE}", "qownnotes"]),
        "shale_release": sh(["du", "-sh", str(B.SHALE)]),
    }
    out = {"vault_metadata_bytes": vaults, "install": install,
           "machine": B.machine_info()}
    p = B.RESULTS / "footprint.json"
    p.write_text(json.dumps(out, indent=2))
    print(json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
