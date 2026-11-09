# What Shale writes to disk

## Inside the vault: `.shale/`

Shale keeps its own state in one dot directory at the vault root, and never treats a
dot directory as vault content: the file tree ignores it, and create/rename/delete
refuse to touch anything under one.

| path | what it is | safe to delete? |
| --- | --- | --- |
| `config.toml` | the vault's settings ([config.md](config.md)) | yes — a starter is rewritten |
| `props.toml` | optional: the key types you declare, `cr = "number"` | no — that is your vocabulary, not a cache |
| `session.json` | one slot per window: panes, tabs, cursors, the jump chain, the graph's camera and filter, the window's frame; plus the vault's recent notes at the top level | yes — you lose the arrangement |
| `themes/<name>.toml` | your palettes | yes |
| `shale.db` (+`-wal`, `-shm`) | the index cache: notes, tags, links, properties, word counts, the event log | yes — rebuilt from the notes |
| `backups/shale-<day>.db` | one copy per day of **the index**, newest 7 kept | yes |
| `pending-edits/<epoch>-<profile>-<tool>-<hex>.toml` | the agent's proposals waiting for review | only on purpose — that is the queue |
| `lock` | the `flock` claim: which process owns this vault | only while nothing is running |
| `mcp.lock` | the loopback port a live window answers on, and its token | only while nothing is running |

**The notes are the source of truth.** Everything above is a cache, a saved
arrangement, or a queue, so deleting any of it costs convenience and not writing.

**`backups/` is not a backup of your notes.** It is a daily copy of the index cache,
kept so that a corrupt `shale.db` is not an event. Your notes are only as safe as
whatever you run over the folder yourself: git, Nextcloud, rsync, whatever you already
use.

## Outside the vault: the app's own memory

`$XDG_STATE_HOME/shale/recent.json` (falling back to `~/.local/state/shale/`) holds
the vaults the app knows — newest first, capped at 10. It is what `shale` with no
argument opens, and what `:vault` lists. Deleting it costs you that list and
nothing else.

## How a file gets written

Every file Shale owns (a note, the config, the session, a queue entry) goes through
one write path: a dot-prefixed temp file beside the target, `.name.shale-tmp`, renamed
over the target. A crash mid-write can leave a stray temp file; it cannot leave a
half-written note.

On `SIGTERM`, and when the window manager closes the window, Shale finishes the work
it has in flight before exiting: the notes are written, the index is flushed and its WAL
checkpointed, the live MCP port and lock are released. A `SIGKILL` is out of scope: the
atomic writes are what keep the notes intact even then.

## What the running app holds

- The `flock` on `.shale/lock` is held for the whole life of the process, and that
  process owns the live MCP port. Windows opened on the same vault share one index,
  watcher, backup loop and queue beat.
- The language server (`harper-ls` unless the config names another) is the only child
  process Shale starts, and it is the largest memory item in the app. A vault with
  `[spellcheck] enabled = false` never starts it.
