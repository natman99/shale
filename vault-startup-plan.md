# Vault startup plan — opening Shale with no path (roadmap P4.20)

The blocker on using Shale as an everyday app: a `.desktop` entry or a Windows shortcut
cannot name a vault without hard-coding a path, and `shale` with no argument either opens
the last vault **or dies in the terminal**. Two confirmed cases today (both verified by
running the release binary):

- no remembered vault → `Error: no vault: name one on the command line…`, no window;
- the remembered vault is held by another Shale → `Error: this vault is already open in
  another Shale (pid N) — close that one first`, no window.

The memory itself already exists and works (`RecentVaults`: `last` + 10 recents under
`$XDG_STATE_HOME/shale/recent.json`), and `:vault` already has a picker with an "Open a
folder…" file dialog. This item is the startup path, not new machinery.

## Rulings (user, 2026-11-18)

1. **A small picker window**, not a vault-less app window: recent vaults + "Open a
   folder…". The Workspace keeps assuming it has a vault.
2. **A double-launch opens the picker** instead of dying, with the reason on the screen.
3. **"New vault…" is in the same round**: the picker creates a folder and opens it.

Addition beyond what was ruled, flagged here so it can be vetoed: a path **named on the
command line** keeps today's behaviour — a terminal error and exit 1, no window. A CLI
invocation gets a CLI answer, and a script must not block on a window. The picker is for
the no-path case, which is the desktop case.

## The shape

`crates/app/src/vault_open.rs` — one view, one window, no vault behind it.

- Rows, in order: the recent vaults (most recent first, gone folders filtered out), then
  `Open a folder…`, then `New vault…`. The switcher (`:vault`) leads with its action row
  because its list is the whole point of the screen; here the list *is* the answer most of
  the time, so it leads.
- The rows are built by the same function the switcher uses, so the two cannot drift.
- A reason line under the field: why the picker is on screen (the last vault is gone, the
  last vault is held elsewhere), and afterwards whatever a choice refused.
- `enter` on a vault opens it and closes this window. `escape` closes it, and with no vault
  window left the run ends.
- `New vault…` swaps the field for a prompt: a folder path, `~` expanded, a relative name
  resolved under `$HOME`, parents created.

## EDGE ledger

- **EDGE-V1** With no remembered vault the picker opens with no reason line. The app is
  usable from zero: a folder dialog or a new folder, no terminal involved.
- **EDGE-V2** A remembered vault whose folder is gone opens the picker with the reason
  naming the path it expected. `startup_vault` already refuses a non-directory.
- **EDGE-V3** A remembered vault held by another Shale opens the picker with that sentence
  and no window on it. The claim is still taken before the GUI exists, so the refusal is
  decided where `return` still means something.
- **EDGE-V4** A path named on the command line that is missing, is not a directory, or is
  claimed elsewhere stays a terminal error with exit 1. Scripts get an exit code, not a
  window they wait on.
- **EDGE-V5** A cancelled folder dialog, or a desktop with no working portal, says nothing
  and leaves the picker open — the same silence `prompt_for_vault` keeps.
- **EDGE-V6** Any directory is a vault: `Vault::open` makes the `.shale/` it needs, so
  "Open a folder…" never refuses a folder for not being a vault yet.
- **EDGE-V7** Choosing a recent that another process holds updates the reason line and
  keeps the picker open. The keyboard stays where the failure is.
- **EDGE-V8** Choosing a recent this process already shows focuses that window (`open_vault_window`
  already does) and closes the picker — its job was to find the vault.
- **EDGE-V9** `escape` with nothing chosen closes the picker; with no vault window the run
  ends on its own, the way the last workspace window ending ends it.
- **EDGE-V10** `New vault…` on a name that is an existing **file** is refused and the
  prompt stays open. On an existing **directory** it opens it — that is a vault, or becomes
  one. Missing parents are created.
- **EDGE-V11** An empty `New vault…` field does nothing. There is no vault named "".
- **EDGE-V12** `New vault…` refuses a path that resolves to the filesystem root: opening
  `/` as a vault would index the machine.
- **EDGE-V13** With an empty recent list the two action rows still filter ("open" finds
  Open a folder…).
- **EDGE-V14** The picker window is small and centered. It reads and writes no session
  window memory — there is no vault to keep a seat in.
- **EDGE-V15** `--note`, `--graph` and `--split` belong to a vault window. With no vault
  opened they are dropped rather than carried into a window for a vault picked later.
- **EDGE-V16** A termination signal closes the picker like any other window: the registry
  holds a handle to it, and the watcher walks the registry.
- **EDGE-V17** There is exactly one picker per run. Nothing re-opens it from inside itself.
- **EDGE-V18** A recent store that will not save is a warning, not a failure — the picker
  simply shows fewer rows next time.
- **EDGE-V19** Gone folders are filtered out of the picker's rows but stay in the file: a
  drive that comes back comes back with its vault.

## Not here

- A vault-less Workspace window (an empty tree, an empty tab bar). Ruled out in favour of
  the small window; the Workspace assumes a vault everywhere it touches.
- Focusing another **process's** window for a claimed vault. That needs cross-process
  signalling; the vault's live MCP port is the obvious carrier, and it is its own item.
- Managing the recent list (forgetting a vault on purpose). Stale entries are filtered from
  the view; removing one is a later verb.
- A first-run tour or a "what is a vault" screen. `docs/getting-started.md` carries that.

## Milestones

- **M1** — the decision and the window: `startup_plan` (named path / remembered path /
  picker-with-a-reason), `main` wiring that no longer exits, the picker view with recents
  and "Open a folder…", the reason line, escape ending the run. Tests: the decision table,
  the rows, enter opening a vault and closing the picker, a foreign claim saying why,
  escape ending the run.
- **M2** — "New vault…": the prompt, the path rules (EDGE-V10/11/12), creating and opening.
  Tests for each refusal and for the created folder.
- **M3** — the desktop story: `assets/shale.desktop` to `Exec=shale`, `docs/getting-started.md`
  (no path needed; where the memory lives; how to start fresh), roadmap and PROGRESS.
