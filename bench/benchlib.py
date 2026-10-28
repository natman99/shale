"""Shared helpers for the bench harness: launching, niri window polling,
screenshots, process-tree memory sampling, and per-app launch recipes.

All timing goes through time.monotonic() in ns. All app windows are found
via `niri msg --json windows`; all pixels via `niri msg action
screenshot-window --id <id>` (niri writes the PNG itself — no compositor
screenshot race with grim's output naming).
"""

import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

BENCH = Path(__file__).resolve().parent
VAULTS = BENCH / "vaults"
RESULTS = BENCH / "results"
SHALE = BENCH.parent / "target" / "release" / "shale"

# Window-matching patterns per app (fallback only — the primary match is by
# pid tree, see find_window). Title matching alone is unsafe: a terminal
# window's title contains its cwd ("shale — gen_vault.py") and would be
# screenshot/focused by mistake.
WINDOW_PATTERNS = {
    "shale": re.compile(r"^Shale$"),
    "obsidian": re.compile(r"obsidian", re.I),
    "zennotes": re.compile(r"zennotes", re.I),
    "qownnotes": re.compile(r"qownnotes", re.I),
}


def pid_tree(root_pid: int) -> set[int]:
    """All pids in the process tree of root_pid."""
    pids, stack = set(), [root_pid]
    while stack:
        pid = stack.pop()
        pids.add(pid)
        try:
            kids = open(f"/proc/{pid}/task/{pid}/children").read().split()
            stack.extend(int(k) for k in kids)
        except (FileNotFoundError, PermissionError):
            pass
    return pids


def now_ns() -> int:
    return time.monotonic_ns()


def niri_windows() -> list[dict]:
    """Every window niri knows about, as dicts."""
    out = subprocess.run(
        ["niri", "msg", "--json", "windows"],
        capture_output=True, text=True, timeout=10,
    ).stdout
    return json.loads(out)


def find_window(app: str, timeout: float, since: float,
                pid: int | None = None) -> dict | None:
    """Poll until a window belonging to the launched app appears.

    Primary match: the window's pid is inside the process tree of `pid`
    (the launched process) — immune to title collisions. Fallback: the
    app's title/app_id pattern, only for windows whose pid is NOT another
    tracked terminal (kept simple: fallback requires an exact-ish pattern).
    `since` is a time.monotonic() stamp taken before launch. Returns the
    window dict or None on timeout."""
    pat = WINDOW_PATTERNS[app]
    deadline = since + timeout
    while time.monotonic() < deadline:
        tree = pid_tree(pid) if pid else set()
        for w in niri_windows():
            if pid and w.get("pid") in tree:
                return w
            hay = f"{w.get('app_id', '') or ''}|{w.get('title', '') or ''}"
            # Fallback pattern must match the whole app_id|title blob to
            # avoid substring collisions with terminal titles.
            if pid is None and pat.fullmatch(hay.replace("|", " ").strip()):
                return w
        time.sleep(0.05)
    return None


def shot(app: str, win_id: int, path: Path) -> bool:
    """Screenshot the window into `path`. niri writes the file itself."""
    r = subprocess.run(
        ["niri", "msg", "action", "screenshot-window",
         "--id", str(win_id), "--write-to-disk", "true"],
        capture_output=True, text=True, timeout=10,
    )
    if r.returncode != 0:
        return False
    # niri writes to its configured screenshot-path; the newest PNG in there
    # is ours. Copy it to `path`.
    newest = newest_png()
    if newest is None:
        return False
    shutil.copy(newest, path)
    return True


def newest_png() -> Path | None:
    """Newest screenshot niri wrote. The path template comes from the niri
    config: screenshot-path "~/Pictures/Screenshots/Screenshot from %Y...png"."""
    d = Path.home() / "Pictures" / "Screenshots"
    pngs = sorted(d.glob("Screenshot from *.png"),
                  key=lambda p: p.stat().st_mtime_ns)
    return pngs[-1] if pngs else None


def pixels_equal(a: Path, b: Path) -> bool:
    """Byte-compare two PNGs (niri's encoder is deterministic for identical
    pixels; good enough for change detection)."""
    try:
        return a.read_bytes() == b.read_bytes()
    except FileNotFoundError:
        return False


def kill_tree(pid: int, grace: float = 3.0) -> None:
    """SIGTERM the whole process group, then SIGKILL stragglers."""
    try:
        pgid = os.getpgid(pid)
    except ProcessLookupError:
        return
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            return
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            try:
                os.killpg(pgid, 0)
                time.sleep(0.1)
            except ProcessLookupError:
                return


def pss_tree_kb(root_pid: int) -> int:
    """Sum PSS (kB) over the whole process tree of root_pid. PSS, not RSS:
    Electron/flatpak share huge amounts of memory and RSS double-counts it."""
    total = 0
    stack = [root_pid]
    while stack:
        pid = stack.pop()
        rollup = f"/proc/{pid}/smaps_rollup"
        try:
            for line in open(rollup):
                if line.startswith("Pss:"):
                    total += int(line.split()[1])
                    break
        except (FileNotFoundError, PermissionError):
            pass
        # Children: /proc/<pid>/task/<pid>/children (kernel-provided list).
        try:
            kids = open(f"/proc/{pid}/task/{pid}/children").read().split()
            stack.extend(int(k) for k in kids)
        except (FileNotFoundError, PermissionError):
            pass
    return total


def launch(cmd: list[str]) -> subprocess.Popen:
    """Start a command in its own process group (so kill_tree works)."""
    return subprocess.Popen(cmd, start_new_session=True,
                            stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)


# ---------------------------------------------------------------- launches

def cmd_shale(vault: Path, note: str | None = None) -> list[str]:
    c = [str(SHALE), str(vault)]
    if note:
        c += ["--note", note]
    return c


def cmd_obsidian(vault: Path, note: str | None = None) -> list[str]:
    # Isolated profile (bench/profiles/obsidian) so the user's real vault
    # registry is never touched; seed_obsidian() registers the bench vault.
    # Obsidian 1.12+ honours obsidian://open?path=<vault dir>.
    prof = BENCH / "profiles" / "obsidian"
    target = vault / note if note else vault
    return ["flatpak", "run", "md.obsidian.Obsidian",
            f"--user-data-dir={prof}", f"obsidian://open?path={target}"]


def seed_obsidian(vault: Path) -> None:
    """Register the bench vault in the isolated Obsidian profile so the
    open?path URI resolves (Obsidian refuses vaults it hasn't been shown)."""
    import json
    import secrets
    prof = BENCH / "profiles" / "obsidian"
    prof.mkdir(parents=True, exist_ok=True)
    cfg = prof / "obsidian.json"
    data = json.loads(cfg.read_text()) if cfg.exists() else {"vaults": {}}
    data.setdefault("vaults", {})
    if not any(v.get("path") == str(vault) for v in data["vaults"].values()):
        data["vaults"][secrets.token_hex(8)] = {
            "path": str(vault), "ts": int(time.time() * 1000)}
    cfg.write_text(json.dumps(data, indent=2))


def cmd_zennotes(vault: Path, note: str | None = None) -> list[str]:
    # Electron: isolated profile so the real ~/.config/ZenNotes is untouched.
    # The vault root lives in the profile config — seed_zennotes() writes it.
    prof = BENCH / "profiles" / "zennotes"
    return ["/opt/ZenNotes/ZenNotes", f"--user-data-dir={prof}"]


def seed_zennotes(vault: Path) -> None:
    """Write a ZenNotes profile whose vaultRoot is the bench vault.
    Mirrors ~/.config/ZenNotes/zennotes.config.json (verified shape)."""
    import json
    prof = BENCH / "profiles" / "zennotes"
    prof.mkdir(parents=True, exist_ok=True)
    cfg = prof / "zennotes.config.json"
    data = {}
    if cfg.exists():
        data = json.loads(cfg.read_text())
    data["workspaceMode"] = "local"
    data["vaultRoot"] = str(vault)
    data["localVaults"] = [{"root": str(vault), "name": vault.name,
                            "lastOpenedAt": int(time.time() * 1000)}]
    cfg.write_text(json.dumps(data, indent=2))


def cmd_qownnotes(vault: Path, note: str | None = None) -> list[str]:
    # --session bench writes ~/.config/PBE/QOwnNotes-bench.conf only.
    return ["/usr/bin/QOwnNotes", "--session", "bench"]


def seed_qownnotes(vault: Path) -> None:
    """Point the bench QOwnNotes session at the bench vault."""
    conf = Path.home() / ".config" / "PBE" / "QOwnNotes-bench.conf"
    conf.parent.mkdir(parents=True, exist_ok=True)
    lines = []
    if conf.exists():
        lines = conf.read_text().splitlines()
    lines = [l for l in lines if not l.startswith("notesPath=")]
    lines.insert(0, f"notesPath={vault}")
    conf.write_text("\n".join(lines) + "\n")


LAUNCHERS = {
    "shale": cmd_shale,
    "obsidian": cmd_obsidian,
    "zennotes": cmd_zennotes,
    "qownnotes": cmd_qownnotes,
}
SEEDERS = {
    "obsidian": seed_obsidian,
    "zennotes": seed_zennotes,
    "qownnotes": seed_qownnotes,
}


def machine_info() -> dict:
    """Environment stamp recorded with every results file."""
    def sh(cmd):
        try:
            return subprocess.run(cmd, capture_output=True, text=True,
                                  timeout=10).stdout.strip()
        except Exception:
            return "unknown"
    cpu = "unknown"
    try:
        for line in open("/proc/cpuinfo"):
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    mem = "unknown"
    try:
        for line in open("/proc/meminfo"):
            if line.startswith("MemTotal"):
                mem = line.strip()
                break
    except OSError:
        pass
    return {
        "cpu": cpu,
        "mem": mem,
        "compositor": sh(["niri", "--version"]),
        "kernel": sh(["uname", "-r"]),
        "shale_rev": sh(["git", "-C", str(BENCH.parent), "rev-parse", "HEAD"]),
    }
