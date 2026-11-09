# Shale

A markdown vault editor for Linux, written in Rust against
[GPUI](https://github.com/zed-industries/zed) (tag `v1.25.3`). A folder of `.md`
files is the whole database; the editing is vim-shaped; and an agent on the other
end of MCP is a first-class reader and writer of the same notes, with its edits
reviewable before they land.

## Build and run

```sh
cargo build --release
./target/release/shale ~/vaults/notes     # no argument: the vault it opened last
```

Requires a Rust toolchain and a Linux desktop (Wayland or X11). Spell and grammar
checking comes from [`harper-ls`](https://github.com/Automattic/harper), which is optional — a
server that is not installed simply means nothing gets underlined.

Try it against the fixture vault:

```sh
./target/release/shale examples/demo-vault
```

## Documentation

The user-facing docs live in [`docs/`](docs/) — start with
[`docs/getting-started.md`](docs/getting-started.md). The command table in
`docs/commands.md` is **generated** from `crates/app/src/command.rs`; a test fails
when the two disagree, and `SHALE_UPDATE_DOCS=1 cargo test -p shale-app` rewrites
the file.

The repo's own planning and design documents are at the root: `PROJECT.md` (what
the app is and every ruling that binds it), `roadmap.md` (what is built and what
is next), `brainstorm.md` (ideas and their status), `design-language.md` (the
visual grammar), `graph-view-plan.md` and `canvas-plan.md` (feature plans),
`PROGRESS.md` (the working log).

## Layout

```
crates/core   the vault model, markdown, the vim engine, config, index, theming — no UI
crates/app    the `shale` binary: the window, the editor view, the graph, the pickers
crates/mcp    the `shale-mcp` binary: the MCP server an agent talks to
```

## Tests

```sh
cargo test -p shale-core -p shale-app
```

The app's tests are headless GPUI: they drive real keystrokes and real windows and
assert on what the window says and does, rather than on internals. Benchmarks are
`#[ignore]` tests run with `--release --ignored`.

## License

Shale is dual-licensed under **MIT OR Apache-2.0** — the same pairing Rust itself
uses, so anything that can consume Rust code can consume this. The texts are the
canonical ones: `LICENSE-APACHE` is byte-identical to `rust-lang/rust`'s, and
`LICENSE-MIT` is the SPDX MIT text with the holder line set to "The Shale
Contributors".

Two boundaries, stated rather than assumed:

- The palettes Shale ships are upstream's own hex values, reused under their own
  permissive terms (Catppuccin and the rest are MIT).
- The vendored reference material in `refs/` — vim's docs, the Obsidian help pages,
  Catppuccin's style guide — is **not tracked and not distributed**. It was read as
  specification; no prose, icons or code from it is in this codebase.
