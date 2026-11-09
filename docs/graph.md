# The link graph

`space g` (or `:graph`) opens the vault's link graph in a tab of its own. There is
**one graph tab per window**: each tab is a running simulation, and a second view of the
same vault would want the same positions, so asking again moves the frame to the tab that
exists.

The counts row reports the counts **on screen**, not the ones the vault holds: a
filter changes the numbers as well as the picture.

## What the graph is made of

Nodes are notes, attachments, **ghosts** (a link that resolves to nothing) and
tags; an edge is a link. A→b and b→a is one edge, flagged bidirectional. Tag hues
are derived from the palette in OKLCH, so a tag's colour follows the theme rather
than being fixed.

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

## The filter strip

`/` opens it; `escape` closes it, and a second `escape` leaves the tab. Three chips
(`orphans`, `missing`, `tags`, on `alt-o`, `alt-x` and `alt-t`) and a search line that
narrows the picture as it is typed. A filtered node leaves the simulation as well as the
paint, so the layout closes up around what is left.

The strip has its own keyboard context, which is why the graph's `hjkl`, `f`, `+`
and `-` are not in the stack while you are typing a query.

## Keyboard, in the graph

| keys | what |
| --- | --- |
| `hjkl` | pick a node |
| arrows | pan |
| `+` / `-` | zoom |
| `enter` | open the chosen node's note |
| `f` | frame the whole graph |
| `2` / `3` | flat / projected |
| `[` / `]` | one hop out / in (local) |
| `\` | the whole vault |
| `/` | filter strip |
| `escape` | back to the note |
| `:` | the command line, so `:tabnext` and `:only` work here too |

## What it costs

The simulation's beat stops when the layout settles, and a settled graph costs
nothing to look at: measured at rest, the difference between having the graph tab
open and not having it is inside the noise, and idle CPU stays under 1% at a
thousand notes. A whole 5,000-note frame paints in 2.3–2.6 ms.
