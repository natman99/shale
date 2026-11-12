# The sidebar

`space b` (or `:sidebar`) puts a strip on the **right** of the window, opposite the file
tree. It answers questions about one thing: the note the window is looking at. The tree
says where the notes are; the strip says what is inside this one.

It is chrome, not a pane. It has its own keyboard, its own visibility, and its own place
in the `ctrl-w` path — the same arrangement the file tree has, and for the same reason:
the note is what both strips give room to, and neither is something you split or tab.

## One view at a time

The strip shows **one** view, named in its header along with a count. `[` and `]` step
through the five; `:sidebar <name>` jumps to one directly.

| view | header | `:sidebar` word | built |
| --- | --- | --- | --- |
| Outline | Outline · *n* | `outline` | yes |
| Backlinks | Links to here | `backlinks` | yes |
| Outgoing links | Links from here | `links` | yes |
| Tags | Tags | `tags` | yes |
| Properties | Properties | `props` | yes |

A view that is planned but not built is still selectable: the header names it and the
body says it is not built yet, rather than the key doing nothing or an empty box reading
as "this note has no tags".

## The outline

The outline is the note's own headings, in order, indented by level — the same spans the
gutter folds and `zc` works on, not a second numbering of them. It reads the **buffer**,
not the index, so a heading you have typed but not saved is already a row. Fold a heading
in the note and its children leave the list; fold it here with `h` and the note's gutter
closes too. There is one fold decision and two surfaces that show it.

The rows are built in the frame that draws them, which is the tree's rule: `j`/`k` step
over exactly what you can see, so a folded heading's children are skipped for free.

## Following the caret, and not fighting you

The section your caret is in is marked in the list with the palette's "you are here"
colour — a different fact from the keyboard's selection, and a different visual. When the
caret crosses into another section, the list scrolls **once**, and only if that row is off
screen. A scroll you made yourself is never undone: your scroll does not change the mark,
so there is nothing to scroll for. And while the strip has the keyboard, the caret does
not move the list at all — there the selection moves it, and the two would fight.

Moving the selection in the Outline **scrubs** the note: the note scrolls to that section
without moving its caret. Back out of the strip and the note is exactly where you left it.
This is Outline-only; a row in another view has no position in this note to reveal.

## Jumping

`enter` on a row jumps the note to that heading and the keyboard follows into the note —
the file tree's rule for a file row. The caret lands on the heading.

A heading inside the note already on screen is a **reveal**, not an open: no tab, no
switch, no re-read of the file. A jump that re-opened the note would quietly throw away
unsaved edits, which is the one thing a navigation surface must never do. (Jumping to a
heading in a note that is *not* on screen goes through the same door every other jump
uses, which refuses to overwrite the only view of a note that is ahead of its file.)

## Reshaping the note

The strip is an outliner as well as a table of contents. With the keyboard in it:

| key | |
| --- | --- |
| `shift-k` / `shift-j` | move this section up / down among its siblings |
| `Tab` / `shift-Tab` | one level up / down for this heading **and everything under it** |

A move takes the section whole: everything nested under the heading goes with it, and it
swaps with a sibling — a section under the *same* parent. A move that would lift a section
out from under its heading is refused rather than done; re-parenting is a different act
than a reorder, and the two keys should not be confusable.

A relevel keeps the shape of the subtree. Promoting `## Notes` promotes `### Today` with
it, so `### Today` is still under `Notes` afterwards; the heading that follows at the old
level is not part of the subtree and does not move. The edit is the `#` markers and nothing
else, so a heading's text and its trailing `^block-id` survive — an anchor that stopped
resolving because someone changed a heading's level would break a link in another note.

Both are **one edit**: one `u` in the note puts a whole relevelled subtree back.

The caps refuse. `#` has no level above it and `######` has none below it, and a keystroke
that looked like it worked and did nothing is worse than one that says
"“Notes” is already a top-level heading". A move with no sibling on that side, and a
preview, which is read to be read, say so the same way.

## The links, both directions

**Links to here** lists the notes that link this one; **Links from here** lists what this
note links. They read the vault index, not the buffer — these are notes this window does
not hold — and they answer for whatever is saved, so an unsaved heading is not in them yet.

A backlink row knows the **line** the link sits on, and `enter` opens the note there rather
than at its top: the row exists to show you that line. An outgoing row opens its target at
the top, because the line the index holds for that direction sits in the note you came from,
and using it would throw away the fact the row exists to show.

A link to a name that is not a note is still a row, in the palette's quiet colour, with the
name exactly as it was typed. That is a fact about the note — a hole in the picture is part
of the picture — and `enter` on it says `"Nowhere" is not a note in this vault` instead of
inventing a file from a typo. Attachments and `.shq` queries get rows like any other target;
they are the author's own files, not missing ones.

With `index.enabled = false` these two views have nothing to answer with, and they say so
rather than showing an empty list, which would read as "nothing links here" — a claim about
the vault that the setting never made. The Outline is unaffected: it reads the buffer.

## The tags

The Tags view lists the tags this note carries — the `#tags` in its body **and** the
`tags:`/`tag:` keys in its frontmatter, in the order the note carries them. They come from
the **buffer**, so a tag you typed a second ago is a row before the index has heard of it.

Each row carries a number: how many **other** notes carry that tag. That one number is the
vault's answer, not the note's, so it comes from the index — and with the index off the rows
carry no number at all rather than a screenful of zeros, because "nobody else writes this"
and "nothing was asked" are different sentences. A tag nobody else writes shows a real `0`.

`enter` asks `tag:x` of the vault search: the tag is this note's, but "who else?" is a vault
question, and the search is the surface that answers those and can be widened from there.
This is the same hand-off the `:keys` picker makes for a property key.

## The properties

The Properties view lists this note's frontmatter, as authored and in authored order —
`cr: 6`, `ally: [[Goblin]]` — read from the **buffer**, so an edit you have not saved is
already in the list.

A value written as a wiki-link is a link the author wrote, so `enter` opens its note,
resolved by the same rules `gd` uses (file rules, then aliases, then saved queries), which
is what stops the strip and a wiki-link meaning two different files. A value that resolves
to nothing keeps its row and is marked, and `enter` says which half is missing rather than
making a file. Anything that is not written as a link is a string — `name: Goblin` in a
vault that happens to have a `Goblin.md` is a string — and `enter` asks `prop:name` of the
search instead.

## The keyboard

The strip's keys are the tree's, because the two strips are one family:

| key | |
| --- | --- |
| `j` / `k` | move the selection |
| `h` / `l` | close / open the section under it |
| `enter` | do what the row is about: jump to a heading, open the other note at the line that links, ask `tag:x` or `prop:key`, or open the note a property names |
| `shift-k` / `shift-j` | move this section up / down |
| `Tab` / `shift-Tab` | one level up / down for it and its subtree |
| `[` / `]` | which view is showing (a view with rows opens on its first one) |
| `p` | stick the strip to this note, or let it follow the tabs again |
| `escape` | back to the note |
| `space b` | put the strip away |

`ctrl-w h/j/k/l` reaches it like any other surface. Both edges are unified on the
**seat** — the pane that last had the keyboard — so going back the way you came lands you
where you came from, and there is no wrapping around the window. `ctrl-w j`/`k` from the
strip has nowhere to go: it spans the window's height.

The full binding list is generated in [keymap.md](keymap.md) under the `sidebar` context;
the commands are in [commands.md](commands.md).

## Pinning it to one note

`p` (with the keyboard in the strip) sticks the strip to the note it is describing. The
header grows a `◆` and the strip stops following the tabs: switch notes, close the tab, open
three others — the strip keeps answering about the pinned note, reading its file when it is
not on screen. `p` again lets it follow.

That is the whole of it: a way to hold one note's outline, links or tags open in the corner
while you work through other notes. The pin rides the session, so a window that opens with
one pinned opens pinned. A pin to a note that has since been deleted is kept rather than
quietly dropped — the strip reports the read failure the same way a restored tab would.

## What comes back

Whether the strip was open, which view it was on, and what it was pinned to ride the session
file — they are decisions you made, not facts about the notes. A session written before the strip existed
restores a window without one, and the first `space b` opens it.

## Not yet

Resizing, and the other half of the pin the plan sketched: `:pin` on a **tab**, meaning "do
not replace me" — a jump that would reuse this tab opens a new one instead. The strip's half
is built (`p`); the tab's half changes how every jump behaves, so it waits for its own
ruling. Both are on the roadmap under P4.13.
