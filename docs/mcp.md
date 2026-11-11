# Agents: MCP, scopes, and the review queue

Shale is written to be read and written by an agent while a person is looking at
the same notes. There are two halves, and they are separate processes.

## `shale-mcp`: the vault

```sh
cargo build --release -p shale-mcp    # → target/release/shale-mcp
shale-mcp --vault ~/vaults/notes [--profile <name>] [--docs <path>]
```

A Model Context Protocol server over stdio JSON-RPC 2.0 (protocol version
`2025-06-18`). It speaks `initialize`, `notifications/initialized`, `ping`,
`tools/list`, `tools/call`, `resources/list` and `resources/read`. Diagnostics go
to stderr; stdout is the protocol.

**Vault tools**, always available:

| tool | what it does |
| --- | --- |
| `list_notes` | the vault's notes |
| `read_note` | one note's contents |
| `write_note` | replace a note's contents — or a saved query's `.shq` |
| `search_notes` | find lines across the vault |
| `backlinks` | what points at a note |
| `pending_edits` | the proposals waiting for review |
| `retract_pending` | take one of the agent's own proposals back |

**Saved-query tools**, also always available:

| tool | what it does |
| --- | --- |
| `list_queries` | the vault's `.shq` files (`list_notes` walks markdown, so it never shows these) |
| `read_query` | one query's declaration and SQL body |
| `run_query` | ask one, and read back the rows it answers |

There is one write door: a `.shq` is written with `write_note`, so the profile's paths and
the review queue govern it exactly as they govern a note. Running is a different matter and
gets its own verb, because the guarantee there is about the statement, not the path — the
query runs against the vault index on a connection that cannot write, and a body that is
not exactly one `SELECT` is refused before SQLite is asked. A vault with no index says so
rather than answering zero rows.

A query that declares `relative = "current"` asks for the note it is relative to. An agent
has no caret, so `run_query` takes a `note` argument and refuses to guess one: without it,
and for a `note` this profile cannot see, the answer is the same sentence the editor shows
when a relative query has no note under the reader.

The answer arrives as text: the query's name and how many rows it answered, a header line,
then one line per row — `∅` for NULL, `''` for an empty string. Long answers are cut at the
same number the in-file table cuts at, and the last line says how many were left out.
**Live tools**, offered only when a Shale window is up and answering:

| tool | what it answers |
| --- | --- |
| `live_status` | is a window open, on which vault |
| `live_note` | which note is on screen, and in which view |
| `live_selection` | where the cursor and selection are |
| `live_jumps` | the jump chain |

The window writes its loopback port to `.shale/mcp.lock`, but the lockfile is a hint and
not an authority: a pid can be reused and a port taken over. So the server probes the
port and offers the live tools only when a Shale window answers. When nothing answers,
the vault tools keep working and the resources say the live half is unavailable instead of
failing.

**Resources:** `shale://status`, `shale://access` (what this connection may reach), and
`shale://commands`, which serves the generated command table so an agent sees the same
list the docs carry.

## Scopes: profiles

A connection's reach is decided at startup and enforced server-side. A profile is
a table in the vault's config:

```toml
[mcp.profile."editor"]
read  = ["notes/**", "areas/journal/**"]
write = ["notes/**"]
deny  = ["private/**"]
tools = ["read_note", "search_notes", "write_note"]
review = true
```

The patterns are path globs, and the matcher is small on purpose. A pattern with no
wildcards is a prefix with a separator boundary, so `private` covers `private/` and
everything under it but not `private-notes.md`. `*` stays inside one path segment,
`**` crosses segments (`notes/**` needs at least one segment inside it), `?` is one
character, and an empty pattern covers nothing rather than the whole vault. Matching is
case-sensitive, because the vault's paths are.

A profile decides what a connection may read, may write, must never see, and may call.
The allowlist narrows `tools/list` as well as `tools/call`, so a client is never shown a
tool that would only refuse. A connection with **no** `--profile` has the whole vault.

Enforcement happens server-side: a client that claims more gets what the profile says.
Every path also goes through the vault's confinement, so nothing reaches outside the
vault or into a dot directory.

## The review queue

A profile with `review = true` **cannot write a note**. Its `write_note` call becomes a
proposal instead: one TOML file per call in `.shale/pending-edits/`, named
`<epoch>-<profile>-<tool>-<hex>.toml`, carrying unified-diff hunks with three lines of
context. The files are the source of truth, so a corrupt or missing index loses nothing.

The window notices the queue on a one-second beat and shows a **review strip** at the
bottom, one row per hunk: note, profile, line, `−N +M`, and a `stale` or `clash` badge.
Six rows at most, and a longer queue is a sliding window the marker walks. The strip
appears only when something is waiting.

Two layers of keys, so the strip never shadows prose:

- while the strip has the keyboard (`:review`): `j`/`k` move the marker, `y`/`x`
  accept and reject, `o` opens the note, `z a` collapses a proposal (then `y`
  answers all of its hunks), `escape` gives the keyboard back;
- while the note has it: only `]a` / `[a` reach the marker — which is why `y` and
  `x` still mean yank and delete in the note.

Accepting either applies or goes stale. The hunk goes into the *buffer* when the note is
open, as one undo step with the cursor and unsaved edits intact, and onto the *disk* when
it is not. If the note changed underneath, the rest of the proposal is re-anchored by the
line shift the accepted hunk caused and written back to the queue; when the context no
longer matches, that row is badged `stale` instead.

`:q` is refused while proposals are unanswered; `:q!` leaves them queued for the
next window. A window-manager close or a signal is never refused, and the files
survive either way.

## Who owns the vault

The process that opened the vault holds an exclusive `flock` on `.shale/lock` for
its whole life, and that process owns the live MCP port. A second Shale pointed at
the same vault refuses; a second *window* on the same vault shares the one set of
vault services. An agent talking to `shale-mcp` is talking to the vault itself,
with or without a window open.
