# bench/

Benchmark harness for Shale vs Obsidian / ZenNotes / QOwnNotes.
Plan and rationale: [PLAN.md](PLAN.md). Nothing here writes outside `bench/`
(except each app's own isolated profile under `bench/profiles/`).

## Quickstart

```sh
# one-time: corpora (dnd copy + synthetic medium/large)
python3 bench/gen_vault.py dnd
python3 bench/gen_vault.py medium

# cold start / memory (per app; kills the app after every run)
python3 bench/cold_start.py --app shale     --vault bench/vaults/dnd
python3 bench/cold_start.py --app obsidian  --vault bench/vaults/dnd
python3 bench/cold_start.py --app zennotes  --vault bench/vaults/dnd
python3 bench/cold_start.py --app qownnotes --vault bench/vaults/dnd

# keystroke->frame latency (needs ydotoold running, see below)
ydotoold --socket-path /tmp/.ydotool_socket --socket-permode 0666 &
export YDOTOOL_SOCKET=/tmp/.ydotool_socket
python3 bench/latency.py --app shale --vault bench/vaults/dnd

# metadata bloat + install footprint (read-only)
python3 bench/footprint.py
```

Results append to `bench/results/*.jsonl`; screenshots to `bench/results/shots/`.

## How it works

- **Window discovery**: `niri msg --json windows`, matched by **pid tree** of
  the launched process (title matching alone is unsafe — terminals print
  "shale" in their title bar from cwd).
- **Screenshots**: `niri msg action screenshot-window --id <id>`; niri writes
  the PNG to `~/Pictures/Screenshots/` and we copy the newest one.
- **Memory**: PSS summed over the whole process tree (`smaps_rollup`) —
  Electron/flatpak share pages, RSS double-counts them.
- **Isolation**: each app gets its own profile dir under `bench/profiles/`
  (Obsidian `--user-data-dir`, ZenNotes `--user-data-dir`, QOwnNotes
  `--session bench`); the user's real configs are never touched. Vaults are
  copies under `bench/vaults/` — never the real Nextcloud vaults.
- Every launched app is killed (whole process group) at run end.

## Verified on this machine (2026-10-28)

- shale (release): window found, shot, PSS sampled, killed clean.
  First warm-up numbers: mapped 0.74s, first-paint 0.83s, settled 1.27s,
  PSS ~171 MB (dnd vault).
- obsidian: needs the vault **pre-registered** in its profile (`seed_obsidian`
  does this; bare `obsidian://open?path=` shows a "Vault not found" dialog).
  Runs under an isolated `--user-data-dir`. Note: `flatpak run md.obsidian.Obsidian
  --help` does NOT exit — it spawns a full instance. Never probe Obsidian with
  `--help`.
- zennotes: `--user-data-dir` honoured; vault seeded via
  `zennotes.config.json` shape copied from the real config.
- qownnotes: `--session bench` writes only `QOwnNotes-bench.conf`; seeded
  `notesPath=` points at the bench vault.
- ydotool is installed; `ydotoold` must be started manually (daemon needs
  /dev/uinput, which has an ACL for the user).

## Gotchas

- Two `cargo build`es at once (default `target/` + a second CARGO_TARGET_DIR)
  starved the machine hard enough to wedge the desktop session. **Only ever
  build in the default `target/`, and check the other agent isn't building.**
- `pkill -f obsidian` matches your own shell — use `pkill -f '[/]app/obsidian'`.
- Obsidian's zygote children outlive the flatpak parent's process group
  sometimes; `kill_tree` handles the group, but verify with
  `pgrep -cf '[/]app/obsidian'` after runs.

## Status

- [x] harness scripts (cold_start, latency, footprint, gen_vault, benchlib)
- [x] corpora: dnd (152 notes), medium (2001); large not generated yet
- [x] shale release build exists (`target/release/shale`)
- [ ] Tier 1 runs (interrupted after the shale warm-up; rerun everything)
- [ ] Tier 3 latency runs (ydotoold not left running across reboots)
- [ ] Tier 4 feature matrix
- [ ] Tier 2 criterion benches (touches crates/core/Cargo.toml — wait for
      the other agent)
