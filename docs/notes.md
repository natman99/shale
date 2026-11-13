# Notes, folders, and links

## A note is a file

There is no database of notes: the `.md` files are the notes, and Shale's index
(`.shale/shale.db`) is a cache that can be deleted at any time. The file tree is
the folder, not a view over an import: folders are real folders, and a note's
name is its filename.

Shale never treats a dot directory as vault content. `.shale/`, `.obsidian/`, `.git/` and
anything else starting with `.` are ignored by the walk and refused by create/rename/delete,
so you cannot delete the vault's machinery by accident.

**Creating** a note: `ctrl-n` (or `n` in the tree) opens a blank tab; `gD` on a
link that resolves to nothing creates the note and follows it; the start block in
a blank tab offers `new note` as a row.

**Deleting** (`shift-d` in the tree, `ctrl-shift-d`) removes the file. It is
permanent; there is no trash. A folder can only be deleted when it is empty. **Renaming** (`r` in the tree, `ctrl-shift-r`) rewrites the wiki-links that
pointed at the old name, in every other note and in the open one when it has no
unsaved edits.

## Naming a note

A tab with no file is not "nothing open": it is a note you are writing. The
first line you type is its name, and `:w` takes it:

```markdown
# Shopping list
milk
bread
```

`:w` writes `Shopping list.md`. The name is cleaned, not copied: markdown block
markers (`#`, `-`, `>`, `1.`) and the characters a filesystem refuses fall out,
the name is capped at 60 characters, and it can never be a hidden file or sit
above the vault. If the name is taken you get `Shopping list-2.md`, because two notes
that open with the same line are the common case and you were not choosing a name.

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
(`r` in the tree, `ctrl-shift-r`), which also rewrites the links that point at the old
name. That difference is the whole reason the two are separate commands.

### Other names the same note answers to

A wiki-link is matched against the vault in this order: the exact path, the path with
`.md` added, and a file whose name matches ignoring case. A fourth name belongs to the
note itself: what its `aliases:` property declares.

```markdown
---
aliases:
  - anon
  - the masked one
---
```

`[[anon]]` opens that note, its backlinks list counts the link, and the link is tinted
as resolved rather than as a dead end. The rule is **additive** — it is consulted only
when the three file rules have found nothing — so a note called `Test.md` keeps its own
`[[Test]]` even if some other note declares `Test` as an alias. Two notes declaring the
same alias tie-break the way a case-insensitive stem tie does: shortest path, then
alphabetical.

Obsidian deliberately writes `[[Real name|alias]]` when you complete an alias, so its
links keep working in other tools. Shale resolves the short form too, which lets a link
stay short and lets it survive a rename: the alias travels with the note, and the next
index pass re-files every link that named it.

## Saving

Autosave is on by default and writes a note:

- after two seconds of not typing (`editor.autosave_after_ms`),
- when the window loses focus,
- before a quit, a vault close, or a signal.

`:w` and `ctrl-s` do the same write now that a blank tab has a name to take. The
tab's dirty dot in the bar is per-tab, and an exit owes **every** tab: `:wq`, a
quit with autosave on, a blur and a window-manager close all write them all.

With autosave **off**, a quit is refused when any tab is dirty, including a note you are
not looking at, and the bar says which one. `:q!` throws the edits away
on purpose; `:wq` writes on purpose.

## Links

Wiki-links resolve against the vault:

```markdown
[[Another Note]]              # by name
[[notes/Another Note]]        # by path
[[another note|Aliased]]      # with an alias
[[Another Note#A heading]]    # a heading anchor (`gd` lands on that heading)
[[Another Note#^goblin]]      # a block anchor (`gd` lands on that block)
```

Resolution tries the exact path, then the exact path with `.md` added, then the
stem case-insensitively. An alias and a `#heading` anchor are cut from the target
before that happens, so both resolve to the note.

Resolved links are tinted, unresolved ones are dimmed — and a note's `aliases:` count
as its name, so a link to one is resolved (see *Naming a note*). `gd` follows the link under
the cursor (`gx` is the older spelling of the same thing), `ctrl`+click is the
mouse door, and `gD` makes the note first when the link resolves to nothing. A
note `gD` creates lands **beside the note you are in**, on the assumption that a link
written from a folder is meant to live in that folder, unless the link spells a path,
which is honoured exactly.

An anchor is matched case-insensitively against the heading text. A heading that
has gone missing opens the note at the top and the status bar says which heading it
could not find, rather than dropping you somewhere plausible. A `#^id` anchor is a
different kind of question and is matched case-sensitively against a block id, because
the spelling decides which kind it is — Shale never guesses from the text. A block that
has gone missing leaves the note open and says which block it could not find.

**A block can be named.** `:block-id [id]` writes an id on the block the cursor is in:
six characters if you give no name, your name if you do. It lands at the end of the
block's last line, or — for a table, quote, callout or code fence, which have no last
line an id could sit on — on its own line below a blank. It is **one edit**, so one `u`
takes the id and its blank line back together. An id is letters, digits and `-`,
case-sensitive, and it is never hidden: it is drawn dim, because it is text in the note
and other tools should be able to read it. `:block-id` refuses a second id on a block
that already has one (and names the id it has, since that is the one you will want to
paste into a link), refuses an id outside that grammar, and refuses an id another note in
the vault already uses. A heading can carry one too; it names the heading line, not the
section, and the heading still answers its own name.

The block is a text object while it is nameless: `i^` / `a^` act on exactly the span an id
would sit on, so `di^` deletes what `:block-id` labels (see *Keys*).

Backlinks are computed (the index keeps them, and the graph draws them), and an
agent can ask for them with the `backlinks` MCP tool. There is no backlinks panel
in the window yet.

## Embedding another note

A line that is nothing but an embed shows the other note inside this one:

```markdown
![[Another Note]]              # the whole note
![[Another Note#A heading]]    # one section of it
![[Another Note#^goblin]]      # one block of it
```

The body is live, not a screenshot: it re-renders when either note changes, and it
is styled by the source note's own markdown. A section that no longer exists shows
a badge naming what is missing instead of showing nothing; a missing block says
`no block "^goblin" in gift.md`, in the badge's own words. A block is one paragraph,
one list item with its children, one whole quote, callout, table or code fence, or one
heading line — the same atom the id was written on. Embeds nest, with a cap;
past the cap a block shows the name it would have expanded and nothing else.

**A badge is a door, not a caption.** Clicking one opens the note it names, the same door
`gd` opens on the `![[…]]` line, so a block that refused to expand still gets you where you
were going.

**Resting the pointer on a link previews what it points at.** A card opens below the
link with the note's first lines, rendered the way the note renders and the way the
link means: `[[note#Heading]]` previews that section, not the top of the note. If the
note is open in a tab, the card shows it as you have it, not as the disk has it. It
says how many lines it left out, it hangs under the link rather than under the line it is
written on, and it never takes the keyboard: your next keystroke is still meant for the
note. It closes when the pointer leaves the link, and on `escape`, which is there for the case
where you start typing without touching the mouse and need to see the text under the card. Scrolling moves the card with its link and then asks the pointer again what it is
on: still on the card or the link, it stays; somewhere else in the note, it goes. The same
rule runs for the spelling popup, which follows its word the same way.

**The block is editable, one level at a time.** Click inside the body, or put the
cursor on the `![[…]]` line and press `ctrl-]`. `ctrl-o` comes back out, and so does
clicking the host's own text; `escape` never does, because `escape` belongs to
whatever command you were halfway through typing. Only the block the cursor is in
takes edits; the same note embedded twice shows the second copy as a view. `gd` on
the `![[…]]` line is a different door: it opens the embedded note in the tab you are
in, the way it does for any other link, and `ctrl-o` walks back.

**The seam is a buffer boundary, and everything buffer-scoped stops there.** `w`,
`$`, `0`, `gg` and `G` stop at the edge of the block, and `/` searches the note you
are in rather than what is embedded in it. There is one scroll bar, the host's: the block
has no viewport of its own.

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
| `ctrl-shift-f`, `space s` | vault search: every line that holds what you typed, or every note that answers an operator (below); choosing a row opens the note |
| `/`, then `n` / `N` | search the note that is open |
| `:wordcount [folder]` | words in the vault, or in one subtree (needs the index) |
| `:sections [pattern]` | jump to a section: the open note's outline with no pattern, every section in the vault that holds the pattern with one (the vault-wide half needs the index) |
| `:blocks [pattern]` | jump to a block by its id: the open note's ids with no pattern, every id in the vault that holds the pattern with one (the vault-wide half needs the index) |
| `:block-id [id]` | give the block under the cursor an id; one `u` takes it back |
| `space g` | the graph, whose filter strip narrows by `orphans` / `missing` / `tags` and by a search line |

`:sections` answers about headings. With no pattern it lists the sections of the note
on screen, in reading order and indented by depth, and choosing one puts the cursor on
that heading — the outline pane's information without the pane. With a pattern it lists
the vault instead: every section whose text holds what you typed, each row naming the
note it lives in, and choosing one opens that note on the heading. The two halves read
different things on purpose. The open note is answered by its buffer, so a heading you
have just typed is already in the list and a heading you have moved is not jumped to by
a stale line number; the rest of the vault is answered by the index, which is the only
thing that knows about the notes you have not opened.

`:blocks` answers about block ids and splits the same way, for the same reason. With no
pattern it lists the ids in the note on screen, in reading order, each row labelled by
the block's first line and detailed with the id, what kind of block it is, and the line
it starts on; choosing one puts the cursor on the block's first line. With a pattern it
lists the vault: every id that holds what you typed, in any note, including the notes you
have never opened. A note with no ids says so and points at `:block-id`; a vault with no
index says it cannot look, which is a different answer from having nothing to find.

### Asking with operators

A plain query is matched literally, line by line, exactly as it always was.
Operators narrow **which notes** that search runs over, and on their own they ask for
notes rather than lines, so `tag:monster` gives one row per monster.

| operator | asks |
| --- | --- |
| `tag:monster` | the note carries that tag, from body `#tags` or the `tags:` property |
| `path:notes/` | the path matches — a substring if you type no wildcards, otherwise `inbox/**`-style globs |
| `link:Zhentarim` | the note links to that note, by name; a link to a note that does not exist yet still counts |
| `prop:cr<=5` | a property compared: `=` `!=` `<` `<=` `>` `>=`, or the key alone for "writes it at all" |
| `modified:7d` | indexed as changed in the last 7 days, or on/after a date like `modified:2026-11-01` |

Operators **and** together, and text can sit beside them: `tag:monster prop:cr<=5
lair` means the line `lair`, in the low-cr monsters. A value with a space is quoted:
`tag:"big bad"`, `prop:"Max HP">=10`.

A note row shows its name and then **the line that answers**, with the part you
asked for brighter than the rest:

```
goblin   tags: [monster, goblinoid] · notes
goblin   Digs in a [[lair]]. · notes
goblin   cr: 5 · notes
```

A `link:` quotes the line holding the `[[…]]`; a `prop:` quotes the frontmatter
line and forwards the value the *note* wrote rather than the bound you typed, so
`prop:cr<=5` on a note reading `cr: 3` shows the `3`. `path:` and `modified:` have
no line to quote, so they answer with facts about the file: the folder, and
`modified 2026-11-02`. The quote comes out of the note, not out of the index: if a
note has been edited since it was indexed, its row shows no quote at all rather
than a line it no longer holds.

A line longer than the row is cut **around the match**, not at its start, with `…`
marking what was left out — the head of a long sentence is not the part the query
named. Several operators split the row's evidence budget, so `tag:monster link:lair`
shows both quotes with both keys rather than one long quote and a dropped second.

Two things it refuses rather than guesses at:

- **A near-miss operator is said out loud.** `tags:monster` (with an `s`) would
  otherwise search for that literal string and answer "0 of 0 matching", which
  reads as an empty vault rather than a typo.
- **A comparison never casts.** `prop:cr<=5` reads only the notes where `cr` is a
  number, so a note that says `cr: low` is not ranked as a low challenge rating.
  Ask about the prose directly with `prop:cr=low`. Likewise `prop:cr<=low` is
  refused instead of quietly meaning "cr at most 0".

Operators are answered by the vault index (`.shale/shale.db`). With the index off
they say so rather than answering zero; plain text search does not need it.

## Markdown that Shale does something with

- **Headings** own the rows below them, which is what folding is built on (`za`,
  `zc`, `zo`, `zd`, `zM`, `zR`), and what the graph's tag and link extraction reads.
- **Tasks** (`- [ ]`) continue on `enter`: a new row gets the marker. Clicking a
  box to check it is not built yet.
- **Tables** are a model, not text: cells, lookup, row and column editing and
  moving. `DD` deletes a column, `|` and `\` insert one, `alt+hjkl` moves the row
  or column under the cursor, and the ordinary motions snap to cell boundaries.
- **Callouts** (`> [!note]`) get their marker and an accent colour drawn from the
  palette, and `> [!note]-` starts folded. The marker is markup, not prose (ruled
  2026-10-28), and that is what keeps the callout's own words from reading as part of the
  block's text.
- **Dice** — a line that starts with `!p` gets its odds painted after the line:
  `!p d20+3 vs AC16` reads out `40% hits · avg 13.5 · crit 5%`. See
  [Dice readouts](#dice-readouts) for what the words mean.
- **Images** referenced from a note are drawn inline in the rendered views.
- **Tags** (`#tag`) are indexed and drawn in the graph as their own nodes, with
  OKLCH hues derived from the palette.

### Dice readouts

A line whose first word is `!p` asks Shale to work out the odds of the dice that follow it,
and paints the answer at the end of that line. Nothing is written into the note: the
readout is drawn, and taking it away is deleting the line that asked.

```markdown
!p d20+3 vs AC16          40% hits · avg 13.5 · crit 5%
!p d20+3 vs DC 16         40% passes · avg 13.5 · crit 5%
!p 2d20kh1+3 vs AC16      64% hits · avg 16.8 · crit 9.8%
!p 2d8+4                  avg 13 · 6–20
```

- **Meeting the number is enough.** `vs AC16` means 16 or better, because an armour class
  means you hit it. `DC` is an alias for `AC` — the arithmetic is one rule with two
  vocabularies, and only the word in the readout moves.
- **A natural 20 lands and a natural 1 does not**, when the roll is one twenty-sided die
  showing one die — which includes advantage and disadvantage (`2d20kh1`). That is also the
  only case with a crit chance, and the crit range is 20 alone. A roll that cannot reach its
  number still hits on a 20.
- **No target, no probability**: the line gets the average and the range instead.
- **A refusal says so.** A line that does not parse, or a roll too wide to work out, puts its
  refusal where the number would have been, rather than going quiet — a blank cannot be told
  apart from a line the app never noticed.
- **A code fence is quoted text**, so a `!p` inside one is not a request.

This is not a probability calculator. Improved-crit ranges, exploding dice and rerolls are
refused rather than invented; there are websites for that.

## Properties (frontmatter)

A note that opens with a `---` block has properties, and Shale reads them into the
vault index: one row per value, with the kind worked out from the value itself. `hp: 7` is
a number, `died: 2024-01-31` a date, `ally: "[[Zhentarim]]"` a link.
`:props` prints what the index holds for the note your cursor is in.

You can see which case you are in. The block sits on a band of its own, with a hairline
along its top and bottom edges, in the palette's `frontmatter_block_bg`: the same claim
a fenced code block's background makes, that these rows are one thing. Nothing inside
the block is hidden — the dashes, the quotes and the brackets are all still there,
because they are the data, not syntax. A block's fences keep the dashes the file holds —
they are not drawn as a rule across the page — and the fences and the keys are drawn in
the palette's `frontmatter` colour. A block the parser refuses, and a `---` at the top
of a note that never closes, are drawn in the colour of an error, because in both cases
the note has **no** properties at all. A `---` at the top followed by prose is still a
thematic break: the line after the fence decides whether someone meant a block.

To see the vault's whole vocabulary at once, `:keys` (the palette's **List the vault's
property keys**) lists every key any note writes: how many notes write it, how many
values it holds, and every kind those values come as. A key whose values disagree —
`cr: 5` in one note and `cr: low` in another — is shown with **both** kinds brought
forward, because the disagreement is the thing worth seeing, and this is where it is
said rather than a warning on every save. Choosing a key puts `prop:key` in the search
field: the search is what lists the notes writing it, and the question can be widened
from there.

The same pane reached from **Rename a property key across the vault** (`:rename-key`)
renames a key everywhere it is written. With both names typed — `:rename-key cr
challenge` — it runs at once; with one it asks for the other, and with none it opens
the key list first, which is the only way to rename a key with a space in it. The
rewrite touches the key and nothing else on the line: the value, its quotes and a
trailing comment stay byte for byte. It stops at the block, so a statblock that says
`cr: 5` in prose is left alone, and it stops at nesting, so an inner key of a map
under `data:` stays where its author put it. A note that is open with unsaved edits is
never rewritten behind your back — it is skipped and named in the status line, which
also says how many notes did change and, if the vault declares the key in
`.shale/props.toml`, that the declaration still names the old spelling.

To start a block on a note that has none, the palette's **Add frontmatter
properties** writes one at the top with `aliases:` left empty, and puts the caret
after the colon so the first thing you type is the alias. The field is empty on
purpose — the command starts the properties rather than guessing at them — and an
unfilled `aliases:` is a key the note writes without being an alias for anything. It
is the field worth starting with: a note answers to its aliases as a link target, so
that line is what makes `[[the green one]]` reach the note you call by a nickname.
It is one undo step, and `u` takes the whole block away. On a
note that already has a block it moves the caret into that one rather than
writing a second, because two blocks at the top is a note whose properties are a
coin flip. Inside an embed it writes the embedded note, the same rule `:w` and
`:rename` follow: the caret decides.

What Shale reads: `key: value` pairs, block lists (`- item`) and flow lists
(`[a, b]`), quoted values, comments, and JSON frontmatter. What it does not read:
nested maps, block scalars (`|`, `>`), anchors and explicit tags. Those are still
indexed, as `nested`, and show up in `:props` badged "not read". **Shale never rewrites
the block**, so whatever a plugin wrote survives untouched.

Two rules worth knowing:

- **A kind is inferred from the value, not guessed at.** `yes`, `no`, `007`, `0x1f`
  and `1/4` are text, because a query that reads them as something else is wrong
  quietly rather than loudly. To make a key numeric vault-wide, declare it in
  `.shale/props.toml` (`cr = "number"`); a declared-numeric key reads `cr: 1/4` as
  0.25 for queries, and the note keeps the text you wrote. Declare `session` numeric
  for the same reason: as text, `Session 10` sorts before `Session 2`, and every
  chronological query inherits that mistake. A declaration never *casts* a value that
  cannot carry it: `session: tonight` stays text, and `:props` shows the key holding
  two kinds, because a disagreement you can see beats a wrong answer you cannot.
- **The block is not prose.** The spell and grammar checker ignores it, so a key you
  spell your own way is not a typo.
- **A property is a link when the whole value is one `[[…]]`, and it is painted as one
  only then.** It becomes an edge to the note it names, quoted or not. A heading anchor
  names a section of a note and not part of its name, so `region: "[[Waterdeep#Government]]"`
  is an edge to Waterdeep while the property keeps the anchor you typed. A value with two
  links in it — `group: [[Federation]] [[Empire]]` — is text, not one broken link: write a
  list, one link per item, which is also the only shape strict YAML and therefore Obsidian
  reads. In prose live preview hides a link's brackets, an aliased link's target and a
  markdown link's URL; inside the block nothing is hidden, because the brackets are the
  whole difference between a link and a string and the value on screen should be the value
  in the file.

Keys keep their case and their spaces: `Max HP` and `max hp` are different keys, and
nothing renames them for you. A key the note writes twice keeps both rows, and
`:props` says so.

## Saved queries

A question worth asking twice is worth keeping, and Shale keeps it the way it keeps
everything else: a plain text file you can read, edit and commit. A `.shq` holds one SQL
statement, and optionally a declaration between `+++` fences naming it and sorting its
answer:

```
+++
name = "Who is still missing"
sort = "-cr"
+++
SELECT note, value AS cr FROM props WHERE key = 'cr'
```

`:query` — or **Run a saved query** in the palette — lists the vault's `.shq` files and
answers in the same surface: the columns once, then one row per answer. A row whose first
cell names a note in the vault *is* that note, so a saved query is a door and not a
printout; a row that names no note stays a row, which is how a query over a
half-populated campaign shows its plot holes instead of offering dead clicks. An answer of
nothing says "no note answers this" and a query that fails says why in the database's own
words, because "zero" and "I could not look" are different answers. A long answer is cut
at 200 rows and says how much was cut.

The statement runs against the vault index on a read-only connection, so it cannot write —
the connection refuses, which is a different kind of promise than filtering what you are
allowed to type. A body that is not a question (`UPDATE`, `PRAGMA`, or a second statement
after a `;`) is refused before it runs: not because it could do damage, but because a note
that quietly runs two statements answers a different question than the one you wrote.

Query notes are files the author owns: they appear in the file tree and open in the editor
like any other file. They are not in quick-open, which stays a list of notes, and they are
not attachments — `![[my-query]]` does not resolve to one yet, and will mean the query
when query embedding lands.

## Spelling and grammar

A language server (`harper-ls --stdio` by default, named by `[spellcheck]` in
`config.toml`) reads the note. What it finds is drawn as wavy underlines and
counted in the status bar. `K`, or resting the pointer on a marked word, opens a popup
over the note with the complaint and the fixes it offers. One popup at a time, anchored to
its line, ended by `escape`, by choosing a row, or by the pointer walking away from it, off
its top or bottom or out past its side. The
way down from the marked word stays inside that area even when the popup is
clamped to the text column and the word is beside it. `:harper off` / `:harper on` switch it without a
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
