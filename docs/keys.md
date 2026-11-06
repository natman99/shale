# Keys

Keys are grouped by **what has the keyboard**. The same letter means different
things in the note and in the file tree, and that is deliberate: the tree is a
list, the note is prose, and neither should have to borrow the other's keys.

Three prefixes are worth knowing before the tables:

- **`space` is the leader.** It is also vim's "move right" motion, so the engine
  decides: a lone `space` moves, `space` plus a letter is a chord.
- **`g` starts a two-stroke command** (`gd`, `gD`, `gt`, `gT`, `gf`). Like
  `space`, a lone `g` is held only as long as a chord is still possible.
- **`ctrl-w` is the window alphabet** (panes and panels). A lone `ctrl-w` is
  pending input, and is replayed if the next key does not finish a chord.

This page is the explanation. The exhaustive list is generated from the keymap
itself: [keymap.md](keymap.md) is built from the bindings in
`crates/app/src/workspace/keys.rs`, grouped by the same contexts, and a test fails
when the two disagree — so if a key is bound, it is in that table. What that table
cannot say is *why* a key is where it is, or what the vim engine does with a key
that gpui never sees; that is this page's job.

A key that only works in the file tree says so here, and says so in
[commands.md](commands.md), which is generated from the command table and is the
authority on ids and `:` spellings.

## Anywhere

| keys | what |
| --- | --- |
| `ctrl-shift-p` | command palette (fuzzy; `:` is the exact-match command line, not this) |
| `ctrl-shift-f`, `space s` | search every note for a line |
| `ctrl-shift-r` | rename the selected note (rewrites the links to it) |
| `ctrl-shift-d` | delete the selected note |
| `ctrl-n` | new note |
| `ctrl-shift-n` | new folder |
| `ctrl-t` | new tab holding the note in hand |
| `ctrl-o` / `ctrl-i` | jump back / forward through the notes you came from |
| `ctrl-s` | save (`:w`) |
| `ctrl-q` | quit (`:q`) |
| `ctrl-w h` / `ctrl-w l` | keyboard to the file tree / back to the note |
| `ctrl-z`, `ctrl-r`, `ctrl-y`, `ctrl-shift-z` | undo, redo |
| `ctrl-shift-c`, `ctrl-shift-v` | system clipboard copy, paste |

## The `ctrl-w` alphabet (panes)

| keys | what |
| --- | --- |
| `ctrl-w h` `j` `k` `l` | step the keyboard between panels and panes |
| `ctrl-w s` / `ctrl-w v` | split the pane in hand, stacked / side by side |
| `ctrl-w S` / `ctrl-w V` | the same, **with the note you are looking at** in the new pane |
| `ctrl-w >` / `ctrl-w <` (also `+` / `-`) | the pane in hand takes more / less of the space its split offers |
| `ctrl-w =` | even the panes out |
| `ctrl-w q` (or `ctrl-w c`) | close this pane, writing its notes first |
| `ctrl-w o` | close every pane but this one |

`>` and `<` are not tied to an axis: the pane grows along whatever axis the split
it sits in divides. Moving a *tab* between panes is done by opening the note in
the pane you are standing in — the tab is stolen whole
([windows-panes-tabs.md](windows-panes-tabs.md)).

## The note — normal mode

**Motions.** `h` `j` `k` `l` (screen-line aware across soft wraps), `w` `b` `e`,
`W` `B` `E` (a WORD is delimited only by blanks, so `**bold**`, `[[a link]]` and
`code.rs` are one step rather than three), `0` `^` `$`, `gg` / `G`, `f` `t` `F` `T`
(and their shifted forms), `enter` to the next line's first non-blank. Counts work
on motions and operators: `3w`, `2dd`, `5j`.

**Operators.** `d` (delete), `c` (change), `y` (yank), each with a motion or a
text object; `dd`, `cc`, `yy` for the whole line; `x` to delete the character
under the cursor; `p` and `P` to put the register after or before.

**One-keystroke edits.** `r<char>` replaces one character (or `3r<char>` replaces
three) without touching the register; `~` switches the case of the character under
the cursor and steps right; `s` substitutes (`cl`) and `S` the whole line (`cc`),
both into insert mode; `C` changes to end of line, `D` deletes to end of line; `X`
deletes the character *before* the cursor; `Y` yanks the whole line. A change keeps
its line: `cc` empties a line, `dd` removes it.

**Text objects.** `iw` / `aw` (word), `ip` / `ap` (paragraph): `diw`, `cap`, and
so on. Quotes, brackets and the rest of vim's objects are not there yet, and
neither is repeating a text object in visual mode to extend it.

**Insert.** `i` `a` `I` `A` `o` `O`, `v` / `V` for visual (charwise / linewise;
blockwise `ctrl-v` is not implemented), `u` for undo, `.` to repeat the last
change.

**Folding** (a heading owns the rows below it): `za` toggle, `zc` close, `zo`
open, `zd` remove the fold, `zM` close all, `zR` / `zE` open all. A folded heading
carries a `⋯ n more lines` marker you can click.

**Searching the note.** `/` opens the strip, `n` / `N` step through the matches.
The query outlives the strip, so `n` keeps working after `enter` closed it.

**Links.** `gd` follows the wiki-link under the cursor (`gx` does the same, and is
the older spelling); `gD` makes the note first when the link resolves to nothing,
then follows it. `ctrl`+click is the mouse door to the same thing.

**Tabs.** `gt` / `gT` walk the tab bar (wrapping). `gf` is quick-open, like
`space f`.

**Checker.** `K` opens the checker's note for the word under the cursor, with the
fixes it offers. Resting the pointer on a marked word does the same.

**Tables.** A table changes what the ordinary motions mean rather than adding a
mode: inside one, `h` `l` `w` `b` `e` move cell to cell, `0` `^` `$` go to the
cell's ends, and `j` `k` step between rows in the same column (outside a table,
`K` is the checker). `DD` deletes a column, `|` and `\` insert one, and
`alt+hjkl` moves a whole row or column under the cursor.

**Agent review, while the note has the keyboard.** `]a` and `[a` hop the review
marker to the next / previous pending hunk. Those are the only review keys in
prose — which is why `y` and `x` still mean yank and delete.

**Leader chords.**

| keys | what |
| --- | --- |
| `space f` | quick-open a note |
| `space s` | search the vault |
| `space g` | the link graph |
| `space e` | show / hide the file tree |
| `space r` / `space l` / `space p` | raw / live / preview view |
| `space d` | focus mode (dim everything but the block you are in) |
| `space y` | put the selection, or the last yank, on the **system** clipboard |

## The note — insert mode

`escape` out. `enter` is a newline (and continues a list, task or quote — the
marker is copied). `ctrl-backspace` deletes the word before the cursor; vim's own
key for that, `ctrl-w`, is left to the window commands. Arrows move the caret;
`delete` deletes forward.

## The file tree

While the tree has the keyboard (`ctrl-w h`):

| keys | what |
| --- | --- |
| `j` `k`, `up` `down` | move |
| `h` `left` / `l` `right` | collapse / expand a folder |
| `enter` | open the note |
| `n` / `shift-n` | new note / new folder |
| `r` | rename (rewrites the links to the note) |
| `shift-d` | delete |
| `space e` | hide the tree (works from inside it, which is how you get out) |
| `escape` | hand the keyboard back to the note |

## Pickers and prompts

Quick-open, vault search, the palette, the theme picker and the one-line prompt
all behave the same: type to narrow (the match is a subsequence, not a prefix),
`j`/`k` or arrows to move, `enter` to choose, `escape` to cancel. Choosing a row
of the theme picker previews that palette; `escape` puts the old one back.

## The command line

`:` in normal mode (and from the graph tab). One word, matched exactly — a word
Shale does not know runs nothing. `enter` runs it, `escape` closes it. Every
spelling, with its argument, is in [commands.md](commands.md).

## The review strip

Only while the strip has the keyboard (`:review`). These keys are bound *only*
here, which is what keeps `y` and `x` meaning yank and delete in the note.

| keys | what |
| --- | --- |
| `j` / `k` | move the marker |
| `y` / `x` | accept / reject the hunk under the marker |
| `o` | open the note the hunk is about |
| `z a` | collapse or expand the proposal (then `y` answers all of its hunks) |
| `escape` | give the keyboard back to the note |

## The graph tab

The graph has its own context, because every key it wants (`hjkl`, `f`, `enter`)
is a vim motion in the note.

| keys | what |
| --- | --- |
| `hjkl` | pick a node |
| `arrows` | pan |
| `+` / `-` | zoom |
| `enter` | open the chosen node's note |
| `f` | frame the whole graph again |
| `2` / `3` | flat view / projected 3D view |
| `[` / `]` | local neighbourhood, one hop out / in |
| `\` | the whole vault again |
| `/` | the filter strip |
| `escape` | back to the note |
| `:` | the command line, so `:tabnext` and `:only` work from the graph too |
| `space f/s/e/d/g`, `gt`/`gT` | the same chords the note has |

The filter strip takes the keyboard for itself: `escape` or `enter` closes it,
`alt-o` / `alt-x` / `alt-t` toggle the `orphans` / `missing` / `tags` chips, and
typing narrows the picture — a filtered node leaves the simulation as well as the
paint.

## The pointer

- Click in the note positions the caret; `ctrl`+click follows a link.
- Click a tab to switch to it, click its `×` to close it, drag the divider
  between panes to resize them.
- Click a folded heading's `⋯ n more lines` marker to open it.
- In the graph: the wheel magnifies at the cursor (`ctrl`+wheel travels), drag
  pans the view, drag a node moves it (a press only grabs after the pointer has
  travelled a little, and `shift` leaves it pinned), click a node to open it.
- Click a row of the start block, the review strip, or the checker popup.
