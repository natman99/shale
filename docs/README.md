# Shale — the docs

The map of these pages, what each one answers, the build commands and the startup flags
all live in the [README](../README.md). This page is for whoever maintains the docs.

## The generated files

Two pages are built from the code rather than written:

| document | generated from |
| --- | --- |
| `commands.md` | the command table in `crates/app/src/command.rs` |
| `keymap.md` | the bindings built in `crates/app/src/workspace/keys.rs`, grouped by gpui context |

A test fails when the code and its file disagree, and the README carries the command that
rewrites both.

That is why the other pages point at these two instead of re-listing commands or keys: a
hand-copied list is the thing that goes stale. `keymap.md` covers what **gpui** binds;
the vim engine's own vocabulary (counts, `space` and `g` chords, marks, text objects) is
reached through keys gpui hands to the engine, so it lives in `keys.md` instead.

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
