# The link graph

`space g` (or `:graph`) opens the vault's link graph in a tab of its own. There is
**one graph tab per window**: each tab is a running simulation, and a second view of the
same vault would want the same positions, so asking again moves the frame to the tab that
exists.

The counts row reports the counts **on screen**, not the ones the vault holds: a
filter changes the numbers as well as the picture.

## What the graph is made of

Nodes are notes, attachments, saved queries (`.shq`, drawn with an `ƒ` and the palette's
`graph_query` colour), **ghosts** (a link that resolves to nothing) and tags; an edge is a
link. A→b and b→a is one edge, flagged bidirectional. Tag hues are derived from the palette
in OKLCH, so a tag's colour follows the theme rather than being fixed.

A link written by an **alias** is an edge to the note that declares the alias, not a ghost:
the graph resolves through the same table the link colour, the hover card and `ctrl-]` use,
so a note reached only by the names it declares is still a note people write about. An
alias never takes a link away from a note that already answers it, and because an alias is
declared in one note and written in another, declaring one re-scans the vault rather than
only the note that changed.

A query is a node whether or not anything embeds it — an unasked question is still a
question, and the `orphans` chip hides it the way it hides a note nobody links to. The
embed is what draws the edge: a query's own program is SQL, so a `[[` inside it is not a
link and asserts nothing. Delete the file and a note still embedding it shows a ghost
keyed by the old path, which is what the note's own text says about it.

## The layout is deterministic, and it is not saved

The force simulation is seeded with a fixed constant, so the same vault lays out
byte-identically on every launch, on every machine. The layout is **not** cached
on disk: it is re-derived each time, which is what makes the cache-free claim
true and what makes a hand-pinned node a temporary thing.

So dragging nodes around and `shift`-pinning one lasts until the window closes. Only
*view* decisions ride the session file: the camera, the mode, the filter.

## Flat and projected

| keys | view |
| --- | --- |
| `2` | flat (the default) |
| `3` | projected 3D |
| `f` | frame the whole graph again |

Turning the graph is a **mode**, not a new graph: switching never re-seeds or
rebuilds the layout, because you are turning the same graph, not getting a new
one. Going flat→3D *opens* the third axis at the scale the flat layout already
has; 3D→flat carries the arrangement back. There is no camera glide on the switch: the
turn is the movement.

In the projected view the pointer's gestures are the ones the flat view has: the
wheel **magnifies at the cursor**, `ctrl`+wheel travels, dragging the background
turns or pans the view, and the eye sits outside the cloud rather than inside it.

## The local neighbourhood

`[` and `]` narrow the graph to the note on screen and step its hop distance out
and back (default 2 hops, up to 5); `\` is the whole vault again. Local is a
**filter dimension**, not a second view mode: the same graph, with everything
outside the neighbourhood removed from the simulation as well as the paint.

The centre is the most recently shown note in the pane with the keyboard, the note you
were reading when the vault became a neighbourhood, and it is marked. The centre is never
stored in the session: a remembered centre could be a note you deleted on the way out.
The **depth** is stored.

## A lens

A **lens** is a saved query the graph wears: a `.shq` file under `.shale/lenses/` whose
note column names the notes the picture is allowed to show ([notes.md](notes.md) covers
what a saved query is). `:lens <name>` puts one on, `:lens none` takes it off, and
`:lens` with nothing typed opens a picker that lists each lens with how many notes it
answers **now** — a lens whose query will not run shows `—` and can still be chosen,
because the picture is not the place to find out. The palette's "Choose a lens for the
graph" row opens the same picker.

Like local scope, a lens is a **filter dimension**: what it did not answer leaves the
simulation as well as the paint, and the layout closes up around what is left. A lens and
a local scope **and** each other — a lens worn on a neighbourhood is the notes in both —
and the strip's chips and search still cut inside the answer.

While a lens is on, the **status bar says so**: a segment beside the view badge reads
`lens <name>`, in the colour the bar reserves for states that are not the default. It is
there because a lens that narrows the picture quietly is a lens the reader forgets is on,
and the strip's filter line only answers when the strip is focused. It appears on the graph
tab and not on a note tab, because there it would read as a property of the note.

A lens is also a file Shale will write for you. With the filter strip open, `alt-s` keeps
the question in the line as a lens: the operators compile to the SQL a hand-written `.shq`
would hold — the same translation keep-a-search makes — and the prompt asks for a name,
starting from one made from the question. `:lens save <name>` is the same door with the
name already given. A saved lens goes on right away, and a lens that already exists is
never written over: the file a reader wrote outranks the picture just saved. The strip's
chips are **not** encoded into the file — they stay dimensions, so a lens never freezes an
`orphans` choice into a query its author will later read.

While a lens is on it keeps up with the vault: the query re-runs when the index moves or
when the file changes under an editor's hands, and the layout is only disturbed when the
answer names a different set of notes. A lens that cannot be answered — SQL that fails, or
a query with no note column — says so and leaves the last good picture standing; an answer
that names notes the graph has no node for counts them out loud rather than dropping them.

The strip carries a row for it — `lens <name> · 3 notes`, or `lens <name> · not answering:
<why>` — because the status line's sentence scrolls away and the picture's silence does
not. The tab's title carries it too: a graph tab reads `graph · <name>`.

The demo vault ships one to read — `:lens Tagged notes` narrows the picture to the three
notes that write a tag — at
`examples/demo-vault/.shale/lenses/Tagged notes.shq`.

## The camera moves, and then it stops

Every camera move the app makes for you (a filter change, a hop, a node click) drives a
critically damped spring rather than a jump: the eye travels and settles, and the loop
that animates it **stops** once it has, so a settled graph paints zero frames. `f` is the
exception and stays an instant snap: it is a recovery move, and an animation on top of it
would be motion for its own sake.

Two rules the camera obeys, both learned from watching it:

- **One spring, not two.** In the projected view the eye's distance is the sprung
  quantity, and zoom and pan are derived from the fit at the distance the eye has
  actually reached. Two independent multipliers make a node's drawn size fight
  itself, which is what "the dots pulsate" was.
- **The framing may lag a shrinking; it may never lag a growth.** Smoothing exists
  to absorb jitter, and a step you asked for is not jitter.

## Labels

Labels are placed with a budget and a collision test rather than for every node: 300 in
the flat view, 120 in the projected one, and only where a label fits without landing on
another. The node under the pointer is always named. The labels chosen last frame keep
their slots while things move; greedy selection without memory trades the same contested
slot back and forth every tick, and that reads as flicker.

## The pointer

A press is not a grab: nothing is picked up until the pointer has travelled 6 px. Below
that, a press is a click, and a click opens the node. Dragging a node begins by pinning it
**at its own position**, so a shaky hand cannot throw the layout across the screen.
`shift` leaves a node pinned where you put it, for as long as the window lives.

A pointer that **stops** on a node for half a second raises a card: the note's name, the
first lines of it with the markers hidden, and how many lines the card left out. The wait
is the whole difference from the card a wikilink raises in a note. Prose holds few links,
so the editor answers the moment the pointer finds one; a graph is dots packed together,
and sweeping across one is ordinary, so without the wait a card would flash at every dot
on the way past. Pressing ends the wait, and so does moving off the dot: the card belongs
to the pointer and never takes the keyboard. It is placed against the dot each frame, so a
view that zooms or pans under a still pointer takes the card with it.

A note reached only by the names it declares is otherwise indistinguishable — same dot,
same label — so the card puts `also known as: …` above the lines when the note declares
aliases. That row is the graph's half of the alias rule: the edge already lands on the
right note, and now the dot can say what it was reached by.

Ghosts and tags get no card. Neither is a note, so neither has lines to preview.

## The filter strip

`/` opens it; `escape` closes it and leaves the keyboard where it was — on the strip a
moment ago, on the graph after that. Leaving the *tab* is tab paging — `g t`/`g T`,
`:tabnext` — exactly as for any other tab. Three chips
(`orphans`, `missing`, `tags`, on `alt-o`, `alt-x` and `alt-t`) and a search line that
narrows the picture as it is typed. A filtered node leaves the simulation as well as the
paint, so the layout closes up around what is left.

The strip has its own keyboard context, which is why the graph's `hjkl`, `f`, `+`
and `-` are not in the stack while you are typing a query.

## Keyboard, in the graph

| keys | what |
| --- | --- |
| `hjkl` or `ctrl`+`shift`+arrows | pick a node |
| arrows | pan |
| `shift`+arrows | pan three times as far |
| `+` / `-` | zoom |
| `enter` | open the chosen node's note |
| `f` | frame the whole graph |
| `2` / `3` | flat / projected |
| `[` / `]` | one hop out / in (local) |
| `\` | the whole vault |
| `/` | filter strip |
| `escape` | put down the chosen node; the tab stays (`g T` pages away) |
| `:` | the command line, so `:tabnext` and `:only` work here too |

## What it costs

The simulation's beat stops when the layout settles, and a settled graph costs
nothing to look at: measured at rest, the difference between having the graph tab
open and not having it is inside the noise, and idle CPU stays under 1% at a
thousand notes. A whole 5,000-note frame paints in 2.3–2.6 ms.
