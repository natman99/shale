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

This page is the explanation. The exhaustive list is generated from the keymap itself:
[keymap.md](keymap.md) is built from the bindings in
`crates/app/src/workspace/keys.rs`, grouped by the same contexts, and a test fails when
the two disagree, so if a key is bound, it is in that table. What that table cannot say is
*why* a key sits where it does, or what the vim engine does with a key gpui never sees.
That is this page's job.

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
| mouse back / forward button | the same jump list, browser-style (quiet while a picker or prompt holds the keyboard) |
| `ctrl-s` | save (`:w`) |
| `ctrl-q` | quit (`:q`) |
| `ctrl-w h` / `ctrl-w l` | keyboard to the file tree / back to the note |
| `ctrl-z`, `ctrl-r`, `ctrl-y`, `ctrl-shift-z` | undo, redo |
| `ctrl-shift-c`, `ctrl-shift-v` | system clipboard copy, paste |

## The `ctrl-w` alphabet (panes)

| keys | what |
| --- | --- |
| `ctrl-w h` `j` `k` `l` | step the keyboard between panels and panes (the file tree and the sidebar are stops, like the panes) |
| `ctrl-w s` / `ctrl-w v` | split the pane in hand, stacked / side by side |
| `ctrl-w S` / `ctrl-w V` | the same, **with the note you are looking at** in the new pane |
| `ctrl-w >` / `ctrl-w <` (also `+` / `-`) | the pane in hand takes more / less of the space its split offers |
| `ctrl-w =` | even the panes out |
| `ctrl-w q` (or `ctrl-w c`) | close this pane, writing its notes first |
| `ctrl-w o` | close every pane but this one |

`>` and `<` are not tied to an axis: the pane grows along whatever axis the split
it sits in divides. Moving a *tab* between panes is done by opening the note in the pane
you are standing in; the tab is stolen whole
([windows-panes-tabs.md](windows-panes-tabs.md)).

## The note — normal mode

**Registers.** `"` before an operator or a paste picks the register it works with:
`"ayw` yanks the word into `a`, `"ap` pastes it back. `a`-`z` are yours, `0` holds
the last plain yank, `1`-`9` the recent line deletes, `"-` the most recent small
delete (`dw`, `d$`), `"_` is a black hole that swallows what you delete into it. A
named register also fills the unnamed one, so `"ayw` then a plain `p` still pastes.

**Motions.** `h` `j` `k` `l` (screen-line aware across soft wraps), `w` `b` `e`,
`W` `B` `E` (a WORD is delimited only by blanks, so `**bold**`, `[[a link]]` and
`code.rs` are one step rather than three), `0` `^` `$`, `gg` / `G`, `f` `t` `F` `T`
(and their shifted forms), `;` and `,` to repeat the last find forwards or backwards,
with the same semantics including the way `t` stops short, and `enter` to the next line's
first non-blank — and the arrow keys do the same four, for when the hands are not on
`hjkl`. Counts work on motions and operators: `3w`, `2dd`, `5j`.

**Operators.** `d` (delete), `c` (change), `y` (yank), each with a motion or a
text object; `dd`, `cc`, `yy` for the whole line; `x` to delete the character
under the cursor; `p` and `P` to put the register after or before.

**One-keystroke edits.** `r<char>` replaces one character (or `3r<char>` replaces
three) without touching the register; `~` switches the case of the character under
the cursor and steps right; `s` substitutes (`cl`) and `S` the whole line (`cc`),
both into insert mode; `C` changes to end of line, `D` deletes to end of line; `X`
deletes the character *before* the cursor; `Y` yanks the whole line. A change keeps
its line: `cc` empties a line, `dd` removes it.

**Joining.** `J` pulls the line below up onto the cursor's line, and in visual mode
it joins the lines the selection touches. The line break becomes a space and the
next line's leading whitespace goes with it; a blank line below adds no space, a
line that already ends in whitespace does not get a second one, and after `.`, `?`
or `!` vim's default puts two spaces. `3J` spans three lines.

**Text objects.** `iw` / `aw` (word), `iW` / `aW` (WORD), `ip` / `ap` (paragraph),
`i"` / `a"` and `i'` / `a'` (a quoted string), and the bracket pairs: `i(` `i)` `ib` for
parentheses, `i[` `i]`, `i{` `i}` `iB`, `i<` `i>`. Either character of a pair names it, so
`di(` and `di)` are the same object, and the object works from the bracket itself or from
anything between the two. Examples: `diw`, `ci(`, `da"`, `diW`.
In visual mode the same keys widen the selection instead.

The markdown constructs have their own objects: `iL` / `aL` (a link), `i#` / `a#` (a
section, from its heading to the next one of the same level or higher), `i*` / `a*` (a
callout box), `` i` `` / `` a` `` (a fenced block, or an inline span when the cursor is not
in one), `iq` / `aq` (a quoted span), `it` / `at` (a table cell), and `i^` / `a^` — **one
block**: a paragraph, one list item with its children, a whole quote, callout, table or
code fence, or one heading line. That is the same span a `^id` names, so `:block-id` writes
an id on exactly what `di^` would take. `^` and not `b` because `ib` / `ab` are already
vim's parentheses: an object that shadows a motion the hand already knows costs more than an
unfamiliar scope letter.

A count before the object reaches further, and vim counts *units* (object, gap, object)
rather than objects: `d2iw` takes the word and the space after it but not
the next word, while `d2aw` takes both words and the space between them. Brackets
inside a code span or a fenced block are text, not brackets, so `di(` in prose will
not reach into a code block. Repeating a text object in visual mode to extend it is
not there yet.

**Insert.** `i` `a` `I` `A` `o` `O`, `v` / `V` for visual (charwise / linewise;
blockwise `ctrl-v` is not implemented), `gv` to bring back the selection that was
last active, in the mode it was in and with the cursor where it was, `u` for undo, and `.`
to repeat the last change.

**Folding** (a heading owns the rows below it): `za` toggle, `zc` close, `zo`
open, `zd` remove the fold, `zM` close all, `zR` / `zE` open all. A folded heading
carries a `⋯ n more lines` marker you can click.

**Searching the note.** `/` opens the strip, `n` / `N` step through the matches.
The query outlives the strip, so `n` keeps working after `enter` closed it.

**Links.** `gd` follows the wiki-link under the cursor (`gx` does the same, and is
the older spelling); `gD` makes the note first when the link resolves to nothing,
then follows it. `ctrl`+click is the mouse door to the same thing.

A link that names a section, `[[morgoth#The Fall]]`, lands the caret **on that heading**
rather than at the top of the note. The heading is matched on its text, ignoring case and
any `#`, and a `#` inside a code block never counts; `[[#The Fall]]` with no target does
the same search in the note you are already in. A heading that is not there opens the note
anyway and says `no heading "The Fall" in morgoth` in the status line: the note is almost
certainly the right destination, and refusing the jump would leave you with nothing for a
link one rename away from working.

**Embeds.** `ctrl-]` on a `![[…]]` line steps into the block, which edits the other
note where it stands; clicking inside the body is the same door. `ctrl-o` steps back
out, and so does clicking the host's own text. `escape` never does, because it belongs to
whatever command you were halfway through typing. The seam is a buffer
boundary: motions stop at it, `/` does not search inside a block, and there is one
scroll bar, the host's. `gd` on the `![[…]]` line is a different door on purpose: it
opens the embedded note in this tab, the way it does for any other link.

**Tabs.** `gt` / `gT` walk the tab bar (wrapping). `gf` is quick-open, like
`space f`.

**Checker.** `K` opens the checker's note for the word under the cursor, with the
fixes it offers. Resting the pointer on a marked word does the same.

**Tables.** A table changes what the ordinary motions mean rather than adding a
mode: inside one, `h` `l` `w` `b` `e` move cell to cell, `0` `^` `$` go to the
cell's ends, and `j` `k` step between rows in the same column (outside a table,
`K` is the checker). `DD` deletes a column, `|` and `\` insert one, and
`alt+hjkl` moves a whole row or column under the cursor.

**Agent review, while the note has the keyboard.** A queued proposal is painted
in the note it belongs to: the lines it would take away are struck through in the
palette's red, and the lines it would bring sit between the note's own lines in
green. Over *those* lines three keys belong to the proposal rather than to vim:
`y` accepts that hunk, `x` rejects it, and `za` folds its additions to a
`−N +M` count. Over every other line they are still yank, delete-char and fold,
and a count or a pending operator means you are spelling a vim command: `2y` and `dy`
stay vim's even over a struck line. The additions are not in the buffer: `j`
and `k` step past them, search cannot match them, and the ruler never counts them.
`]a` and `[a` hop the review marker to the next / previous pending hunk from
anywhere in the note, and the note's `y` names the hunk under its own cursor
rather than the marked one.

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

`escape` out. `enter` is a newline, and it continues a list, task or quote by copying the
marker. `ctrl-backspace` deletes the word before the cursor; vim's own
key for that, `ctrl-w`, is left to the window commands. Arrows move the caret;
`delete` deletes forward. `tab` writes a tab character (Shale indents notes with tabs)
and `shift-tab` takes one indent level back off the line — a tab, or up to four spaces
when the line has no tab.

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

## The sidebar

The strip on the right (`space b`) answers about the note on screen. While it has the
keyboard (`ctrl-w l` from the note, when it is open):

| keys | what |
| --- | --- |
| `j` `k`, `up` `down` | move the selection |
| `h` `left` / `l` `right` | close / open the section under it — the note's own fold |
| `enter` | jump the note to that heading — or, on a link row, open the other note at the line that links — and follow the keyboard into it |
| `shift-k` `shift-j` | move this section up / down among its siblings, subtree included |
| `tab` `shift-tab` | one level up / down for this heading and everything under it |
| `[` `]` | which view the strip is showing |
| `space b` | hide the strip (works from inside it) |
| `escape` | hand the keyboard back to the note |

The keys are the tree's on purpose: the two strips are one family. Moving the selection
in the Outline scrolls the note to that section **without moving its caret**, so backing
out leaves the note where it was. The structure keys belong to the Outline and refuse out
loud — a `#` cannot be promoted, a section cannot be moved out from under its parent, and a
row in a link view is a place in a list of *other* notes, not a line in this one — each
refusal one line in the status bar. Details in [sidebar.md](sidebar.md).

## Pickers and prompts

Quick-open, vault search, the palette, the theme picker and the one-line prompt
all behave the same: type to narrow (the match is a subsequence, not a prefix),
arrows to move the highlighted row, `enter` to choose, `escape` to cancel. Choosing
a row of the theme picker previews that palette; `escape` puts the old one back.
The arrows and not `j`/`k`, because the surface is a text field first: a query may
contain any letter at all. (`j`/`k` do move rows in the popup, which has no field.)

The line you type into is a field with a selection, and it answers the same keys
wherever it appears — the prompt, every picker, and the graph's filter strip:

| key | what it does |
| --- | --- |
| `shift-left` / `shift-right` | extend the selection by one character |
| `ctrl-left` / `ctrl-right` | the caret a word at a time |
| `ctrl-shift-left` / `ctrl-shift-right` | extend the selection a word at a time |
| `home` / `end` | the caret to the start / end of the line |
| `shift-home` / `shift-end`, `shift-up` / `shift-down` | select to the start / end |
| `ctrl-a` | select the whole line |
| `ctrl-c` / `ctrl-x` / `ctrl-v` | copy / cut / paste, on the system clipboard |

A word is a run of one kind — letters and digits, punctuation, or blanks — and the word
step uses the same boundary `ctrl-backspace` deletes from, so the caret can be parked on
exactly the thing the key would take away. In the graph tab itself `ctrl`+`shift`+arrows
belong to the canvas (they move the chosen node); inside its filter strip they are back to
being field keys. `ctrl-a`
is the deliberate departure from that editor — it selects everything, and `home` is
start-of-line. Typing, `backspace`, `delete` or a paste replaces what is selected; a
plain arrow leaves the selection at the end it was heading for rather than stepping.

## The command line

`:` in normal mode (and from the graph tab). One word, matched exactly: a word Shale does
not know runs nothing. `enter` runs it, `escape` closes it. Every
spelling, with its argument, is in [commands.md](commands.md).

## The review strip

Only while the strip has the keyboard (`:review`). The bindings live *only* here;
the note reaches the same commands through its own `y` / `x` / `za` over a line a
proposal speaks for (above), and there the hunk under the cursor is the one that
is meant.

| keys | what |
| --- | --- |
| `j` / `k` | move the marker |
| `y` / `x` | accept / reject the hunk under the marker |
| `o` | open the note the hunk is about |
| `z a` | collapse or expand the proposal (then `y` answers all of its hunks) |
| `escape` | give the keyboard back to the note |

## The graph tab

The graph has its own context, because every key it wants (`hjkl`, `f`, `enter`)
is a vim motion in the note. It is otherwise a tab like any other: it has no way out of
its own, and `ctrl-w l` means the pane on the right here as it does everywhere else.

| keys | what |
| --- | --- |
| `hjkl` or `ctrl`+`shift`+arrows | pick a node |
| `arrows` | pan (`shift`+arrows three times as far) |
| `+` / `-` | zoom |
| `enter` | open the chosen node's note |
| `f` | frame the whole graph again |
| `2` / `3` | flat view / projected 3D view |
| `[` / `]` | local neighbourhood, one hop out / in |
| `\` | the whole vault again |
| `/` | the filter strip |
| `escape` | put down the chosen node; the tab stays |
| `:` | the command line, so `:tabnext` and `:only` work from the graph too |
| `space f/s/e/d/g`, `gt`/`gT` | the same chords the note has |

The filter strip takes the keyboard for itself: `escape` or `enter` closes it,
`alt-o` / `alt-x` / `alt-t` toggle the `orphans` / `missing` / `tags` chips, and typing
narrows the picture, with a filtered node leaving the simulation as well as the paint.

## The pointer

- Click in the note positions the caret; `ctrl`+click follows a link.
- Click a tab to switch to it, click its `×` to close it, drag the divider
  between panes to resize them.
- Click a folded heading's `⋯ n more lines` marker to open it.
- In the graph: the wheel magnifies at the cursor (`ctrl`+wheel travels), drag
  pans the view, drag a node moves it (a press only grabs after the pointer has
  travelled a little, and `shift` leaves it pinned), click a node to open it.
- Click a row of the start block, the review strip, or the checker popup.
