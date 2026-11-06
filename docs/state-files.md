# What Shale writes to disk

## Inside the vault: `.shale/`

Shale puts all of its own state in one dot directory at the vault root, and never
treats a dot directory as vault content — the file tree ignores it, and
create/rename/delete refuse to touch anything under one.

| path | what it is | safe to delete? |
| --- | --- | --- |
| `config.toml` | the vault's settings ([config.md](config.md)) | yes — a starter is rewritten |
| `session.json` | one slot per window: panes, tabs, cursors, the jump chain, the graph's camera and filter, the window's frame; plus the vault's recent notes at the top level | yes — you lose the arrangement |
| `themes/<name>.toml` | your palettes | yes |
| `shale.db` (+`-wal`, `-shm`) | the index cache: notes, tags, links, word counts, the event log | yes — rebuilt from the notes |
| `backups/shale-<day>.db` | one copy per day of **the index**, newest 7 kept | yes |
| `pending-edits/<epoch>-<profile>-<tool>-<hex>.toml` | the agent's proposals waiting for review | only on purpose — that is the queue |
| `lock` | the `flock` claim: which process owns this vault | only while nothing is running |
| `mcp.lock` | the loopback port a live window answers on, and its token | only while nothing is running |

**The notes are the source of truth.** Everything above is either a cache, an
arrangement, or a queue, and deleting any of it costs convenience rather than
writing.

The one thing worth saying plainly: **`backups/` is not a backup of your notes.**
It is a daily copy of the index cache, which exists so a corrupt `shale.db` is not
an event. Your notes are as safe as whatever you run over the folder yourself —
git, Nextcloud, rsync, whatever you already use.

## Outside the vault: the app's own memory

`$XDG_STATE_HOME/shale/recent.json` (falling back to `~/.local/state/shale/`) holds
the vaults the app knows — newest first, capped at 10. It is what `shale` with no
argument opens, and what `:vault` lists. Deleting it costs you that list and
nothing else.

## How a file gets written

Every file Shale owns — a note, the config, the session, a queue entry — goes
through **one** write discipline: write a dot-prefixed temporary file beside the
target (`.name.shale-tmp`), then rename it over the target. A crash mid-write can
leave a stray temp file; it cannot leave a half-written note.

On `SIGTERM` (and the window manager's close), Shale finishes the work it has in
flight before exiting: the notes are written, the index is flushed and its WAL
checkpointed, the live MCP port and lock are released. A `SIGKILL` is out of
scope — the atomic writes are what make even that leave the notes intact.

## What the running app holds

- **One process per vault claim.** The `flock` on `.shale/lock` is held for the
  process's whole life, and that process owns the live MCP port. Windows on the
  same vault share one index, watcher, backup loop and queue beat.
- **One child process, optional.** `harper-ls` is started for the spellchecker and
  is the largest memory item in the whole app; a vault with `[spellcheck]
  enabled = false` never starts it.
