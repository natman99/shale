# Getting started

## A vault is a folder

Shale does not import anything and keeps no private copy of your notes. A vault
is a directory of markdown files, and Shale edits those files:

```sh
./target/release/shale ~/vaults/notes
```

Any folder works, including one an Obsidian vault already points at — Shale
ignores `.obsidian/`. With no folder named, Shale opens the vault it opened last;
`:vault` opens the list of vaults the app remembers, and picking a row opens that
vault in a new window.

The first time Shale opens a folder it writes a `.shale/` directory inside it.
That directory holds everything Shale's own — config, session, themes, the index
cache, backups, the agent's pending edits — and it is safe to delete any of it
except the notes themselves. See [state-files.md](state-files.md).

## The window

```
┌───────────┬───────────────────────────────────────────┐
│ file tree │  tab bar  (one chip per tab, dirty dot)   │
│  space e  ├───────────────────────────────────────────┤
│           │                                           │
│  j/k move │              the note                     │
│  h/l fold │      (or the graph, in a graph tab)       │
│  enter    │                                           │
│           ├───────────────────────────────────────────┤
│           │  review strip — only when edits wait      │
├───────────┴───────────────────────────────────────────┤
│ mode · checker count · dirty · path · row:col         │
└───────────────────────────────────────────────────────┘
```

The file tree is the vault's folders and notes. The tab bar belongs to a pane,
and a window can hold more than one pane
([windows-panes-tabs.md](windows-panes-tabs.md)). The status bar's segments come
and go: the mode, the checker's count, a dirty dot, the note's path, and the
cursor's display row and column.

`ctrl-w h` puts the keyboard in the tree, `ctrl-w l` puts it back on the note.
`space e` hides the tree and shows it again.

## Modes, and the first note

The editor is vim-shaped: **normal** mode for moving and operating, **insert**
for typing (`i`, `a`, `o`), **visual** for selecting (`v`, `V`). Normal mode is
where every shortcut in these docs lives.

A new note starts as a **blank tab** — `ctrl-n`, or `n` in the tree, or a blank
tab's start block. Type the title as the first line and save:

```
:w
```

The note takes its name from that first line, cleaned: a leading `#` or `-` or
`>` falls out, characters the filesystem refuses fall out, and the name is capped
at 60 characters. If the name is already taken you get `Idea-2.md` rather than a
question. To choose the name yourself, `:w areas/idea` — a name you type may
place the note in a folder, and it will never overwrite a note that already
exists. See [notes.md](notes.md#naming-a-note).

Autosave is on by default: after two seconds of not typing, when the window loses
focus, and before a quit, the note is written. `:w` still works; it is just
rarely the difference between keeping the note and losing it.

## The three views of a note

| keys | view | what it is |
| --- | --- | --- |
| `space r` | raw | the markdown source, exactly as it is on disk |
| `space l` | live | the default: the syntax of a row you are not on is hidden |
| `space p` | preview | fully rendered, and read-only |

The badge in the status bar names which one you are in. Preview refuses typing on
purpose, which is why the way out is the same two keys as the way in.

## Links

Write a wiki-link and it resolves against the vault:

```markdown
[[Another Note]] and an [[alias|aliased link]]
```

Resolved links are tinted, unresolved ones are dimmed, and `gd` follows the link
under the cursor. If the target does not exist, `gD` creates the note — beside
the note you are in, not at the vault root — and follows it. Renaming a note
rewrites every link that pointed at it.

To find the notes that point *here*, use the graph, or ask an agent
(`backlinks` is an MCP tool); there is no backlinks panel in the window yet.

## Where to go next

- Everything you can press: [keys.md](keys.md)
- Everything you can type after `:`: [commands.md](commands.md)
- Splits, tabs and what survives a restart: [windows-panes-tabs.md](windows-panes-tabs.md)
