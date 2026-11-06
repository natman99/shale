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

Undecided — no license is declared yet, so assume nothing here may be reused. The
third-party palettes Shale ships are upstream's own values, and every upstream it
borrows from is MIT; the license that eventually goes here has to stay compatible
with that.
