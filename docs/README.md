# Shale — the docs

Shale is a markdown vault editor for Linux: one folder of `.md` files is the
whole database, the editing is vim-shaped, and an agent on the other end of MCP
is treated as a first-class reader and writer of the same notes. It is written in
Rust against Zed's GPUI, and it is deliberately not a clone of Obsidian — the
places where it differs are documented as decisions, not omissions.

## Running it

```sh
cargo build --release          # → target/release/shale
./target/release/shale ~/vaults/notes
```

With no vault named, Shale opens the vault it opened last. The flags:

| flag | what it does |
| --- | --- |
| `--note <path>` | open this note (vault-relative) at startup |
| `--graph` | open the graph tab at startup |
| `--split` | tile a second pane beside the first |
| `--open-after <secs>` | debug aid: open the note some seconds after the window appears |

Spell and grammar checking come from a language server — `harper-ls` by default.
It is optional: a server that is not installed simply means nothing gets
underlined.

## What is here

| document | what it answers |
| --- | --- |
| [getting-started.md](getting-started.md) | opening a vault, the parts of the window, the first note, where state lives |
| [keys.md](keys.md) | the keys, explained: what each one is for and why it sits where it does |
| [keymap.md](keymap.md) | every binding, by surface — the exhaustive list (**generated** — see below) |
| [commands.md](commands.md) | every command: its id, its keys, its `:` spelling, its argument (**generated** — see below) |
| [notes.md](notes.md) | notes, folders, the tree, saving and naming, wiki-links, search, word counts |
| [windows-panes-tabs.md](windows-panes-tabs.md) | windows, vaults, splits, tabs, and what comes back next launch |
| [graph.md](graph.md) | the link graph: flat, projected 3D, and the local neighbourhood |
| [mcp.md](mcp.md) | the MCP surface: tools, profiles and scopes, the review queue |
| [config.md](config.md) | `<vault>/.shale/config.toml`, themes, fonts |
| [state-files.md](state-files.md) | everything Shale writes to disk, and what is safe to delete |

## The generated files

Two documents are built from the code rather than written:

| document | generated from |
| --- | --- |
| `commands.md` | the command table in `crates/app/src/command.rs` |
| `keymap.md` | the bindings built in `crates/app/src/workspace/keys.rs`, grouped by gpui context |

A test fails when the code and its file disagree, and this rewrites both:

```sh
SHALE_UPDATE_DOCS=1 cargo test -p shale-app
```

That is why the other documents point at these instead of re-listing commands or
keys: a hand-copied list is the thing that goes stale. `keymap.md` covers what
**gpui** binds; the vim engine's own vocabulary (counts, `space` and `g` chords,
marks, text objects) is reached through keys gpui hands to the engine, so it lives
in `keys.md` instead.

## Owed

These are described where they are used, but do not have a page of their own yet:

- **Tables** — the model, and the editing keys (`DD`, `|`, `\`, `alt+hjkl`).
- **Folding, focus mode, typewriter scroll** — the reading-comfort surfaces.
- **The checker popup** — the `K` panel, its offered fixes, and what a code action
  does to the note.
- **The review strip** as a surface — `mcp.md` covers the queue and its semantics;
  the strip's own layout rules (six rows, the sliding window, the clash and stale
  badges) are in `PROJECT.md`.
- **Query notes (`.shq`)**, the time machine, and the canvas — specced in
  `brainstorm.md` and `roadmap.md`, not built or partly built; do not document
  them as if they were.
