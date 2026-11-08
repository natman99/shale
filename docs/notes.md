# Notes, folders, and links

## A note is a file

There is no database of notes — the `.md` files are the notes, and Shale's index
(`.shale/shale.db`) is a cache that can be deleted at any time. The file tree is
the folder, not a view over an import: folders are real folders, and a note's
name is its filename.

Shale never treats a dot directory as vault content. `.shale/`, `.obsidian/`,
`.git/` and anything else starting with `.` are ignored by the walk and refused by
create/rename/delete — you cannot delete the vault's machinery by accident.

**Creating** a note: `ctrl-n` (or `n` in the tree) opens a blank tab; `gD` on a
link that resolves to nothing creates the note and follows it; the start block in
a blank tab offers `new note` as a row.

**Deleting** (`shift-d` in the tree, `ctrl-shift-d`) removes the file. It is
permanent — there is no trash — and a folder can only be deleted when it is
empty. **Renaming** (`r` in the tree, `ctrl-shift-r`) rewrites the wiki-links that
pointed at the old name, in every other note and in the open one when it has no
unsaved edits.

## Naming a note

A tab with no file is not "nothing open" — it is a note you are writing. The
first line you type is its name, and `:w` takes it:

```markdown
# Shopping list
milk
bread
```

`:w` writes `Shopping list.md`. The name is cleaned, not copied: markdown block
markers (`#`, `-`, `>`, `1.`) and the characters a filesystem refuses fall out,
the name is capped at 60 characters, and it can never be a hidden file or sit
above the vault. If the name is already taken you get `Shopping list-2.md` —
because two notes that open with the same line are the common case, and you were
not choosing a name.

To choose one, `:w <name>`:

```
:w areas/2026 plans
```

A name you type is treated differently from a name that was inferred, in two ways:

- it may place the note in a folder (`areas/2026 plans.md`), and
- if it collides with a note that exists, it is **refused** rather than quietly
  suffixed. A typed name is a specific request, and `plans-2.md` is not what you
  asked for.

And `:w <name>` on a note that **already has a file** refuses out loud:

```
a.md already has a name: :w writes it, :rename moves it
```

The command line will not clone or move a note by accident. Moving is `:rename`
(`r` in the tree, `ctrl-shift-r`), which also rewrites the links that point at the
old name — that difference is the whole reason the two are separate commands.

## Saving

Autosave is on by default and writes a note:

- after two seconds of not typing (`editor.autosave_after_ms`),
- when the window loses focus,
- before a quit, a vault close, or a signal.

`:w` and `ctrl-s` do the same write now that a blank tab has a name to take. The
tab's dirty dot in the bar is per-tab, and an exit owes **every** tab: `:wq`, a
quit with autosave on, a blur and a window-manager close all write them all.

With autosave **off**, a quit is refused when any tab is dirty — including a note
you are not looking at — and the bar says which one. `:q!` throws the edits away
on purpose; `:wq` writes on purpose.

## Links

Wiki-links resolve against the vault:

```markdown
[[Another Note]]              # by name
[[notes/Another Note]]        # by path
[[another note|Aliased]]      # with an alias
[[Another Note#A heading]]    # a heading anchor (`gd` lands on that heading)
```

Resolution tries the exact path, then the exact path with `.md` added, then the
stem case-insensitively. An alias and a `#heading` anchor are cut from the target
before that happens, so both resolve to the note.

Resolved links are tinted, unresolved ones are dimmed. `gd` follows the link under
the cursor (`gx` is the older spelling of the same thing), `ctrl`+click is the
mouse door, and `gD` makes the note first when the link resolves to nothing. A
note `gD` creates lands **beside the note you are in** — a link written from a
folder is meant to live in that folder — unless the link spells a path, which is
honoured exactly.

An anchor is matched case-insensitively against the heading text. A heading that
has gone missing opens the note at the top and the status bar says which heading it
could not find, rather than dropping you somewhere plausible.

Backlinks are computed (the index keeps them, and the graph draws them), and an
agent can ask for them with the `backlinks` MCP tool. There is no backlinks panel
in the window yet.

## Embedding another note

A line that is nothing but an embed shows the other note inside this one:

```markdown
![[Another Note]]              # the whole note
![[Another Note#A heading]]    # one section of it
```

The body is live, not a screenshot: it re-renders when either note changes, and it
is styled by the source note's own markdown. A section that no longer exists shows
a badge naming what is missing instead of showing nothing. Embeds nest, with a cap;
past the cap a block shows the name it would have expanded and nothing else.

**A badge is a door, not a caption.** Clicking one opens the note it names — the same
door `gd` on the `![[…]]` line opens — so a block that refused to expand still gets you
where you were going.

**Resting the pointer on a link previews what it points at.** A card opens below the
link with the note's first lines — rendered the way the note renders, and the way the
link means: `[[note#Heading]]` previews that section, not the top of the note. If the
note is open in a tab, the card shows it as you have it, not as the disk has it. It
says how many lines it left out, it closes when the pointer leaves, and it never takes
the keyboard: your next keystroke is still meant for the note.

**The block is editable, one level at a time.** Click inside the body, or put the
cursor on the `![[…]]` line and press `ctrl-]`. `ctrl-o` comes back out, and so does
clicking the host's own text; `escape` never does, because `escape` belongs to
whatever command you were halfway through typing. Only the block the cursor is in
takes edits — the same note embedded twice shows the second copy as a view. `gd` on
the `![[…]]` line is a different door: it opens the embedded note in the tab you are
in, the way it does for any other link, and `ctrl-o` walks back.

**The seam is a buffer boundary, and everything buffer-scoped stops there.** `w`,
`$`, `0`, `gg` and `G` stop at the edge of the block, and `/` searches the note you
are in rather than what is embedded in it. There is one scroll bar — the host's; the
block has no viewport of its own.

Editing an embedded note is editing that note. Its tab's dirty dot lights even when
the cursor is three rows inside the host, `:w` writes host and embed together, and a
note you have typed into that has no tab of its own gets one. `:rename` follows the
cursor, so it renames the note you are actually standing in, and `:w newname` refuses
inside an embed because it would clone one. Raw mode draws the `![[…]]` line and no
body, so `:raw` steps you out of the block instead of leaving the cursor on text that
has stopped being drawn.

## Finding things

| door | what it does |
| --- | --- |
| `space f`, `gf`, `:e` | quick-open: a fuzzy list of every note; the match is a subsequence of the name or path |
| `ctrl-shift-f`, `space s` | vault search: every line that holds what you typed; choosing a row opens the note on that line |
| `/`, then `n` / `N` | search the note that is open |
| `:wordcount [folder]` | words in the vault, or in one subtree (needs the index) |
| `space g` | the graph, whose filter strip narrows by `orphans` / `missing` / `tags` and by a search line |

## Markdown that Shale does something with

- **Headings** own the rows below them, which is what folding is built on (`za`,
  `zc`, `zo`, `zd`, `zM`, `zR`), and what the graph's tag and link extraction reads.
- **Tasks** (`- [ ]`) continue on `enter`: a new row gets the marker. Clicking a
  box to check it is not built yet.
- **Tables** are a model, not text: cells, lookup, row and column editing and
  moving. `DD` deletes a column, `|` and `\` insert one, `alt+hjkl` moves the row
  or column under the cursor, and the ordinary motions snap to cell boundaries.
- **Callouts** (`> [!note]`) get their marker and an accent colour drawn from the
  palette, and `> [!note]-` starts folded. The marker is markup, not prose — that
  was ruled on 2026-10-28, and it is what keeps the callout's own words from
  reading as part of the block's text.
- **Images** referenced from a note are drawn inline in the rendered views.
- **Tags** (`#tag`) are indexed and drawn in the graph as their own nodes, with
  OKLCH hues derived from the palette.

## Spelling and grammar

A language server (`harper-ls --stdio` by default, named by `[spellcheck]` in
`config.toml`) reads the note. What it finds is drawn as wavy underlines and
counted in the status bar. `K` — or resting the pointer on a marked word — opens
a popup over the note with the complaint and the fixes it offers; one popup at a
time, anchored to its line, ended by `escape`, by choosing a row, or by the
pointer walking off either edge. `:harper off` / `:harper on` switch it without a
restart. A server that is not installed simply means nothing gets underlined.

## Reading with less noise

- `space l` — **live**: the syntax of a row you are not on is hidden.
- `space p` — **preview**: fully rendered, read-only.
- `space r` — **raw**: the file exactly as it is on disk.
- `space d` — **focus mode**: everything but the block the caret is in fades to a
  floor (a three-step ramp, so the rest of the note goes quiet rather than away).
  `:focus block` or `:focus section` decides how much is lit.
- `:typewriter on` — keeps the caret's line near the middle of the window.

Focus mode and typewriter are state you chose in a window, not config keys: they
wait in the session file for the next one.
