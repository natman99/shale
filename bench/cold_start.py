#!/usr/bin/env python3
"""Cold-start benchmark: launch each app against a vault copy and time
window-mapped, first-paint, and pixels-settled; sample process-tree PSS.

  cold_start.py --app shale --vault bench/vaults/dnd [--runs 5] [--warmup 1]

Checkpoints per run (all ns, monotonic, t0 = just before exec):
  mapped_ns   niri reports the app's window
  paint_ns    first screenshot that differs from the previous poll (non-blank)
  settled_ns  two consecutive screenshots identical (initial render burst over)
  pss_*_kb    process-tree PSS at settled, +1s, +3s

Results append to bench/results/cold_start.jsonl. Every launched app is
killed (whole process group) at run end — nothing is left running.
"""

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchlib as B


def one_run(app: str, vault: Path, tmp: Path) -> dict:
    seed = B.SEEDERS.get(app)
    if seed:
        seed(vault)
    cmd = B.LAUNCHERS[app](vault)
    t0 = time.monotonic()
    proc = B.launch(cmd)
    rec = {"app": app, "vault": vault.name, "pid": proc.pid, "cmd": cmd}
    try:
        win = B.find_window(app, timeout=30, since=t0, pid=proc.pid)
        if win is None:
            rec["error"] = "window never mapped"
            return rec
        rec["mapped_ns"] = int((time.monotonic() - t0) * 1e9)
        wid = win["id"]

        # First paint: poll screenshots until two consecutive differ.
        a, b = tmp / "a.png", tmp / "b.png"
        prev = None
        paint = None
        deadline = t0 + 30
        while time.monotonic() < deadline:
            if B.shot(app, wid, b):
                if prev is not None and not B.pixels_equal(a, b):
                    paint = time.monotonic()
                    break
                a, b, prev = b, a, a
            time.sleep(0.05)
        if paint is None:
            rec["error"] = "no pixel change observed"
            return rec
        rec["paint_ns"] = int((paint - t0) * 1e9)

        # Settled: two consecutive screenshots identical after paint.
        settled = None
        while time.monotonic() < t0 + 60:
            time.sleep(0.2)
            if B.shot(app, wid, b):
                if B.pixels_equal(a, b):
                    settled = time.monotonic()
                    break
                a, b = b, a
        if settled is None:
            rec["error"] = "never settled within 60s"
            settled = t0 + 60
        rec["settled_ns"] = int((settled - t0) * 1e9)

        rec["pss_settled_kb"] = B.pss_tree_kb(proc.pid)
        time.sleep(1.0)
        rec["pss_1s_kb"] = B.pss_tree_kb(proc.pid)
        time.sleep(2.0)
        rec["pss_3s_kb"] = B.pss_tree_kb(proc.pid)
    finally:
        B.kill_tree(proc.pid)
        proc.wait()
    return rec


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--app", required=True, choices=list(B.LAUNCHERS))
    ap.add_argument("--vault", required=True, type=Path)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--warmup", type=int, default=1)
    args = ap.parse_args()

    B.RESULTS.mkdir(exist_ok=True)
    tmp = B.RESULTS / "shots"
    tmp.mkdir(exist_ok=True)

    out = B.RESULTS / "cold_start.jsonl"
    for i in range(args.warmup + args.runs):
        rec = one_run(args.app, args.vault, tmp)
        rec["warmup"] = i < args.warmup
        rec["machine"] = B.machine_info() if i == 0 else None
        with open(out, "a") as f:
            f.write(json.dumps(rec) + "\n")
        print(json.dumps(rec))
        time.sleep(2)  # let the compositor settle between runs
    return 0


if __name__ == "__main__":
    sys.exit(main())
