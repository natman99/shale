# Shale

A markdown vault editor for Linux, written in Rust on GPUI. Windows is not shipped.

One folder of `.md` files is the whole program. Your notes are the source of truth:
Shale keeps an index in SQLite that it can throw away and rebuild, and it does not
rewrite a note behind your back. There is no sync, no mobile app and no plugin system,
and those are decisions rather than things nobody got around to building.

The editing is vim-shaped and the vim is native. The text objects are checked against
real vim: [`scripts/vimcrosscheck.py`](scripts/vimcrosscheck.py) feeds the same cases to
`vim -es` and diffs the two, so the claim can be re-run instead of remembered.

Agents read and write the same vault over MCP. Their edits land as files in a queue and
wait for you to accept them hunk by hunk.

**Not an Obsidian clone.** Same ground (notes, links, a graph), different priorities.
Obsidian has sync, mobile and a plugin ecosystem; Shale will never have any of the three.

| Priority | What you get |
| --- | --- |
| **Agent-native** | Two-mode MCP, scoped profiles, pending-edit review that survives a crash, provenance on every write |
| **Keyboard-first** | Deep vim (markdown text objects, marks, jump chain), one command registry for palette / `:` / macros / MCP |
| **Local & honest** | Notes are the source of truth; the SQLite index is a cache you can delete and it will rebuild itself |
| **Quiet** | Nothing is volunteered - every surface is asked for. Zero frames at rest: what still runs is the vault walk and the MCP queue poll, and the graph's ambient drift is opt-in |
| **Owned renderer** | Live / raw / preview views, the graph, the review overlay, editable embeds - built for the interactions web views cannot provide |

## Status

Built: the editor and the vim engine, the vault model and file tree, tabs and splits,
the graph (flat, projected 3D, local neighbourhood), live / raw / preview views,
transclusion and editable embeds, tables, callouts, themes, and the MCP server with its
profiles and review queue.

Not built: the canvas, the vault-history scrubber, and the TTRPG toolkit. Frontmatter
(properties) indexing is being written now, so the index schema will move under you.

## Quick start

```sh
cargo build --release
./target/release/shale examples/demo-vault
```

Requires a Rust toolchain and a Linux desktop (Wayland or X11). With no vault named,
Shale opens the vault it opened last.

Windows is not shipped. The core test suite has been run under Wine (437 passed, 7
failed, 10 ignored) and the path-separator invariant those failures point at is settled
in [PROJECT.md](PROJECT.md); the app build and its console handling are still owed.

Spell and grammar checking comes from
[`harper-ls`](https://github.com/Automattic/harper), which is optional - a server that is
not installed simply means nothing gets underlined.

Useful flags:

| flag | what it does |
| --- | --- |
| `--note <path>` | open this note (vault-relative) at startup |
| `--graph` | open the graph tab at startup |
| `--split` | tile a second pane beside the first |
| `--open-after <secs>` | debug aid: open the note some seconds after the window appears |

## Agent workflow

1. Point your agent at the `shale-mcp` binary. It connects over stdio, and when a Shale
   window is running it finds it over a loopback port (`.shale/mcp.lock`) and gains the
   live tools: what is open, where the cursor is, the jump chain.
2. Give the agent a **profile** (path globs + tool allowlist). An untrusted profile
   queues writes instead of applying them.
3. Agent edits land as plain-text files in `<vault>/.shale/pending-edits/`, which is why
   they survive a crash or a kill.
4. The review strip appears - accept / reject per hunk. Nothing is applied silently.

Details: [docs/mcp.md](docs/mcp.md).

## Documentation

The user-facing docs live in [`docs/`](docs/) - start with
[`docs/getting-started.md`](docs/getting-started.md).

| Doc | Purpose |
| --- | --- |
| [getting-started.md](docs/getting-started.md) | opening a vault, the parts of the window, the first note, where state lives |
| [keys.md](docs/keys.md) | the keys, explained: what each is for and why it sits where it does |
| [keymap.md](docs/keymap.md) | every gpui binding, by surface (**generated**) |
| [commands.md](docs/commands.md) | every command: id, keys, `:` spelling, argument (**generated**) |
| [notes.md](docs/notes.md) | notes, folders, the tree, saving and naming, wiki-links, search, word counts |
| [windows-panes-tabs.md](docs/windows-panes-tabs.md) | windows, vaults, splits, tabs, and what comes back next launch |
| [graph.md](docs/graph.md) | the link graph: flat, projected 3D, the local neighbourhood |
| [mcp.md](docs/mcp.md) | the MCP surface: tools, profiles and scopes, the review queue |
| [config.md](docs/config.md) | `<vault>/.shale/config.toml`, themes, fonts |
| [state-files.md](docs/state-files.md) | everything Shale writes to disk, and what is safe to delete |

`commands.md` and `keymap.md` are **generated** from the code. A test fails when they
drift, and this rewrites both:

```sh
SHALE_UPDATE_DOCS=1 cargo test -p shale-app
```

Tables, folding, focus mode and typewriter scroll are still owed a page of their own;
they are described where they are used.

The repo's own planning and design documents are at the root: [PROJECT.md](PROJECT.md)
(what the app is and every ruling that binds it), [roadmap.md](roadmap.md) (what is
built and what is next), [brainstorm.md](brainstorm.md) (ideas and their status),
[design-language.md](design-language.md) (the visual grammar),
[graph-view-plan.md](graph-view-plan.md), [canvas-plan.md](canvas-plan.md),
[transclusion-plan.md](transclusion-plan.md) and [properties-plan.md](properties-plan.md)
(feature plans), [PROGRESS.md](PROGRESS.md) (the working log).

## Layout

```
crates/core   vault model, markdown, vim engine, config, index, theme - no UI
crates/app    the `shale` window: the editor, the graph, the pickers, review
crates/mcp    the `shale-mcp` binary: the MCP server an agent connects to
```

## Development

```sh
cargo test -p shale-core -p shale-app
```

The app's tests are headless GPUI: real keystrokes, real windows, and assertions on what
the window says and does rather than on internals. Benchmarks are `#[ignore]` tests run
with `--release --ignored`.

## License

Shale is dual-licensed under **MIT OR Apache-2.0** - see
[LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).

- The palettes Shale ships are upstream's own hex values, reused under their own
  permissive terms (Catppuccin and the rest are MIT).
- The vendored reference material in `refs/` — vim's docs, the Obsidian help pages,
  Catppuccin's style guide — is **not tracked and not distributed**. It was read as
  specification; no prose, icons or code from it is in this codebase.
