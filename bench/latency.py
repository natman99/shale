#!/usr/bin/env python3
"""Keystroke -> frame latency for each app, via ydotool + niri screenshots.

  latency.py --app shale --vault bench/vaults/dnd [--runs 10]

Method: launch the app, wait for settled initial render, focus its window
(niri focus-window), send one keystroke with ydotool, then poll window
screenshots until pixels change. Latency = keystroke-send -> first changed
frame. The display is 3840x1080@120Hz (8.33ms/frame), so the quantisation
floor is ~8ms; report median over runs.

Keystroke choice per app (must produce a visible change, be undoable, and
be safe):
  shale     'x' in a note (deletes a char; we type then backspace)
  obsidian  'x' in the editor
  zennotes  'x' in the editor
  qownnotes 'x' in the editor

Every run operates on a vault COPY under bench/vaults/rt (round-trip dir is
disposable); the app is killed at run end. Requires ydotoold running.
"""

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as B

# ydotool keycodes: x = 45, backspace = 14, escape = 1.
KEY_X, KEY_BS, KEY_ESC = "45", "14", "1"


def ydo(*args: str) -> None:
    subprocess.run(["ydotool", *args], check=True,
                   capture_output=True, timeout=10)


def focus(win_id: int) -> None:
    subprocess.run(["niri", "msg", "action", "focus-window",
                    "--id", str(win_id)], check=True, timeout=10)


def one_run(app: str, vault: Path, tmp: Path) -> dict:
    seed = B.SEEDERS.get(app)
    if seed:
        seed(vault)
    note = "Welcome.md" if app == "shale" else None
    cmd = B.LAUNCHERS[app](vault, note)
    proc = B.launch(cmd)
    rec = {"app": app, "vault": vault.name}
    try:
        t0 = time.monotonic()
        win = B.find_window(app, timeout=30, since=t0, pid=proc.pid)
        if win is None:
            rec["error"] = "window never mapped"
            return rec
        wid = win["id"]
        # Let the initial render settle, then focus.
        time.sleep(3.0)
        focus(wid)
        time.sleep(0.5)

        # Baseline frame.
        a, b = tmp / "a.png", tmp / "b.png"
        if not B.shot(app, wid, a):
            rec["error"] = "baseline shot failed"
            return rec

        # Send the keystroke; t_send is right after ydotool returns.
        ydo("key", KEY_X)
        t_send = time.monotonic()

        # Poll until the window changes.
        changed = None
        deadline = t_send + 5.0
        while time.monotonic() < deadline:
            if B.shot(app, wid, b):
                if not B.pixels_equal(a, b):
                    changed = time.monotonic()
                    break
            time.sleep(0.004)
        if changed is None:
            rec["error"] = "no frame change within 5s"
            return rec
        rec["latency_ms"] = (changed - t_send) * 1000.0

        # Undo the edit so vault copies stay clean-ish (best effort).
        try:
            ydo("key", KEY_BS)
        except Exception:
            pass
    finally:
        B.kill_tree(proc.pid)
        proc.wait()
    return rec


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--app", required=True, choices=list(B.LAUNCHERS))
    ap.add_argument("--vault", required=True, type=Path)
    ap.add_argument("--runs", type=int, default=10)
    args = ap.parse_args()

    B.RESULTS.mkdir(exist_ok=True)
    tmp = B.RESULTS / "shots"
    tmp.mkdir(exist_ok=True)

    out = B.RESULTS / "latency.jsonl"
    lats = []
    for i in range(args.runs):
        rec = one_run(args.app, args.vault, tmp)
        with open(out, "a") as f:
            f.write(json.dumps(rec) + "\n")
        print(json.dumps(rec))
        if "latency_ms" in rec:
            lats.append(rec["latency_ms"])
        time.sleep(2)
    if lats:
        lats.sort()
        print(f"median={lats[len(lats)//2]:.1f}ms n={len(lats)} "
              f"min={lats[0]:.1f} max={lats[-1]:.1f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
