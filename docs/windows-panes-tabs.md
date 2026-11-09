# Windows, vaults, panes, tabs

The containers, from the outside in:

```
process → vault → window → pane → tab → note
```

Each level has its own rule about what happens when the last one of them closes.

## One process, one window per vault

The process that opens a vault **claims** it: an exclusive `flock` on
`.shale/lock`, held for the life of the process. A second Shale pointed at the
same vault finds the door shut and says so, rather than opening a second watcher,
index and MCP channel beside the first.

Inside one process, the claim is a registry, so:

- asking for a vault that is already open **focuses the window that has it**;
- `:vault` opens the switcher over the vaults the app remembers
  (`$XDG_STATE_HOME/shale/`, newest first, capped at 10) and opens the one you pick in a
  **new window**. The asking window stays open with the keyboard in it: a flow that opens
  a window never closes the one that asked;
- `:new-window` opens a second window on the vault you are already in, on purpose.

Windows on one vault share one set of the vault's once-per-vault things: one
index, one file watcher, one backup loop, one live MCP channel, one review-queue
beat. That is why a second window is cheap and why the queue is not polled once
per window.

Each window is one **session slot**, and remembers its own frame on screen: where it was
and how big it was. A split window comes back split, where it was.

## Panes

A window's note area is a tiling tree: a leaf (one pane) or a split (a direction,
a fraction, and two children).

| keys | what |
| --- | --- |
| `ctrl-w s` / `ctrl-w v` | split the pane in hand, stacked / side by side |
| `ctrl-w S` / `ctrl-w V` | split it **with the note you are looking at** |
| `ctrl-w h` `j` `k` `l` | step the keyboard between panes (and to the tree) |
| `ctrl-w >` / `ctrl-w <` | the pane in hand takes more / less of the space its split offers |
| `ctrl-w =` | even them out |
| `ctrl-w q` / `ctrl-w o` | close this pane / close every pane but this one |

`>` and `<` are not tied to an axis: the pane grows along whatever axis the split
it sits in divides. The divider between panes can also be dragged.

**The keyboard is in exactly one pane.** That is what most of the pane rules turn
on: a pane you are not looking at can still be told things (a tab can be moved
into it, a note can be opened in it by clicking its start block), but the keys
always go to the one you chose.

`:q`, `:q!` and `:wq` close **the pane the command was typed in** when there is
more than one; the window goes only with the last pane. Closing the last tab in a pane
closes that pane, with the last pane as the exception: there the tab is cleared, so a
window never quits because a tab closed.

## Tabs

A tab is the unit that holds a note. What belongs to the tab is the *view*:
scroll, folds and the raw/live/preview mode travel with it, which is why moving a
tab moves the reading state with it.

What belongs to the **note** is the text, the caret and the undo history (roadmap
P4.17). A window keeps one buffer per note, so two tabs of one file are two views
of one text: an edit made in one is in the other, `u` undoes the same edit
whichever tab has the keyboard, and the dirty dot lights on every tab showing a
note that differs from its file. Preview mode is per-tab: reading a note in one tab does
not lock the other tab out of editing it.

| keys | what |
| --- | --- |
| `ctrl-t`, `:tabnew` | a new tab holding the note in hand: the same note read twice, one text and two views |
| `gt` / `gT`, `:tabnext` / `:tabprev` | walk the bar, wrapping |
| `:tabclose`, `:tabonly`, `:tabmove <n>` | close, close the others, reorder |
| click a tab / its `×` | switch to it / close it |

A tab bar row is a rounded chip framed by the bar's own colour, and a dirty dot
sits on the tab it belongs to.

**Opening a note that another pane already shows steals the tab** into the pane with the
keyboard, whole: the tab owns its editor, so the cursor, folds, view and undo history
travel with it. The pane it left gets a blank tab rather than closing. (Ruled 2026-11-05:
"found, and brought here" replaces "found, and the keyboard goes there".) The graph tab
is exempt, because there is one per window and it stays where it is.

A tab holds a **kind** of content: a note, or the graph. Because a graph tab has no note,
the parts of the window that need a note say so: `:raw` on the graph answers *no note
open*, the bar's note-shaped segments go empty, and the badge reads `graph`. Opening a
note from the graph never converts the graph tab. The note goes to the tab that already
holds it, else the most recently used note tab, else a new tab. Closing the graph tab
leaves a place to write: the tab becomes an empty note tab under the same id.

## A blank tab

A tab with no file is a note being written, not an error state. It paints a **start
block**: the vault's counts, its recent notes as clickable rows, and the doors as
key-chips. The block steps aside the moment the tab starts being a note, in insert mode or
once there is text in the buffer. A row names its own pane, so clicking a recent in a pane
without the keyboard opens the note *there*.

Saving it is covered in [notes.md](notes.md#naming-a-note): the first line names
the note, and `:w <name>` names it by hand.

## What comes back next launch

`.shale/session.json` holds one slot per window: its panes and the tiling tree
over them (direction, fraction, which pane had the keyboard), each pane's tabs in
order with the view each was in, the open note's cursor and scroll, the tree's
expanded folders, the jump chain, focus mode and typewriter state, the graph's camera and
filter, and the window's frame. The vault's **recent notes** sit at the top level of the
same file, one list for every window on the vault.

The file is plain text a person can edit, and a shape that does not describe
exactly the right number of panes is not a crash: it restores as one pane holding
every tab, with a warning. Deleting it is safe; it costs you the arrangement.

Not remembered, on purpose: the graph's node positions and hand-pinned nodes. The layout
is re-derived from a fixed seed, so the same vault lays out identically every launch; see
[graph.md](graph.md).
