# Getting started

## A vault is a folder

Shale does not import anything and keeps no private copy of your notes. A vault
is a directory of markdown files, and Shale edits those files:

```sh
./target/release/shale ~/vaults/notes
```

Any folder works, including one an Obsidian vault already points at: `.obsidian/` is
on the ignore list along with `.git`, `.trash` and `.shale`. With no folder named, Shale
opens the vault it opened last; `:vault` opens the list of vaults the app remembers, and
picking a row opens that vault in a new window.

The first time Shale opens a folder it writes a `.shale/` directory inside it. Config,
session, themes, the index cache, backups and the agent's pending edits all live there,
and all of it is safe to delete. See [state-files.md](state-files.md).

## The window

```
┌───────────┬─────────────────────────────┬──────────┐
│ file tree │ tab bar (one chip per tab,  │ sidebar  │
│  space e  │  dirty dot)                 │ space b  │
│           ├─────────────────────────────┤          │
│  j/k move │         the note            │  j/k     │
│  h/l fold │   (or the graph, in a       │  enter   │
│  enter    │    graph tab)               │  [/]     │
│           ├─────────────────────────────┤          │
│           │ review strip — edits wait   │          │
├───────────┴─────────────────────────────┴──────────┤
│ mode · checker count · dirty · path · row:col      │
└────────────────────────────────────────────────────┘
```

The file tree is the vault's folders and notes. The tab bar belongs to a pane,
and a window can hold more than one pane
([windows-panes-tabs.md](windows-panes-tabs.md)). The status bar's segments come
and go: the mode, the checker's count, a dirty dot, the note's path, and the
cursor's display row and column.

`ctrl-w h` puts the keyboard in the tree, `ctrl-w l` puts it back on the note.
`space e` hides the tree and shows it again. The strip on the right is the tree's
mirror — it answers about the note you are looking at rather than about the vault —
and `ctrl-w l` walks into it when it is open
([sidebar.md](sidebar.md)); `space b` hides it and shows it again.

## Modes, and the first note

The editor is vim-shaped: **normal** mode for moving and operating, **insert** for
typing (`i`, `a`, `o`), **visual** for selecting (`v`, `V`). Normal mode is where every
shortcut in these docs lives.

A new note starts as a blank tab: `ctrl-n`, or `n` in the tree, or the start block on a
blank tab. Type the title as the first line and save:

```
:w
```

The note takes its name from that first line, cleaned: a leading `#`, `-` or `>` falls
out, characters the filesystem refuses fall out, and the name is capped at 60 characters.
If the name is taken you get `Idea-2.md` instead of a prompt. To choose the name yourself,
`:w areas/idea`. A name you type can place the note in a folder, and it never overwrites a
note that exists. See [notes.md](notes.md#naming-a-note).

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

Resolved links are tinted, unresolved ones are dimmed, and `gd` follows the link under
the cursor. If the target does not exist, `gD` creates the note and follows it. It lands
beside the note you are standing in rather than at the vault root, and a target that
spells a path out, `[[areas/x/y]]`, is created where it says. Renaming a note rewrites
every link that pointed at it.

To find the notes that point *here*, use the graph, or ask an agent (`backlinks` is an
MCP tool). There is no backlinks panel in the window yet.

## Where to go next

- Everything you can press: [keys.md](keys.md)
- Everything you can type after `:`: [commands.md](commands.md)
- Splits, tabs and what survives a restart: [windows-panes-tabs.md](windows-panes-tabs.md)
