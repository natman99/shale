# Sidebar — the spec and the plan

**Ruled 2026-11-11 (rounds 1 and 2). Roadmap P4.13.** The rulings and the edge ledger are
binding. The one open item is at the end.

## What this is

A fixed strip on the **right**, opposite the file tree, that describes one note: its
headings, the links pointing at it, the links it makes, its tags, its properties. It is
not a picker. A picker is a question you asked ("jump to a section"); the sidebar is an
answer that stays up. **One view at a time** — the view is chosen, not stacked — and the
strip is a first-class stop in the window's focus grammar, reachable with `ctrl-w h/l`
exactly as the file tree is.

## The shape

```
┌──────────────────────────────┬───────────────────┐
│ the pane tree                │ Outline · 7       │ ← header strip: view name + count
│                              │                   │   (and the pin mark when pinned)
│   …notes…                    │ ▸ Goblin Tactics  │ ← the selection: standard focus highlight
│                              │   ▾ Ambush        │ ← the current section: coloured text,
│                              │ ## Wisp           │   follows the caret
└──────────────────────────────┴───────────────────┘
        ctrl-w l →             ← ctrl-w h
```

The header strip is chrome, not a row: `j` starts on the first real row and you cannot get
stuck on the header. It carries the view's name, its count, and the pin mark.

## The five views

| view | a row is | `enter` on a row | where the rows come from | the header's count |
| --- | --- | --- | --- | --- |
| **Outline** | a heading, nested by level | jump the note to that heading; the keyboard follows into the note | the **live buffer** (`mdstyle`'s heading scanner) | headings in the note |
| **Links to here** | a note that links this one | open that note *at the line that links* | the index (`links` by `target`) | backlinks |
| **Links from here** | a target this note links to (note, attachment or ghost) | open the target | the **live buffer**'s links, resolved through the vault | links out |
| **Tags** | a tag this note carries | ask `tag:x` in vault search | the **live buffer** (inline tags) plus frontmatter | tags |
| **Properties** | a `key: value` of this note | `key:` search, unless the value is a link-shaped note, which opens | the **live buffer** plus frontmatter | properties |

The buffer-vs-index split is EDGE-S5 and it is the whole honesty of the surface: anything
that describes *this* note reads the text in hand, because an unsaved heading or tag is
real and the index does not know it yet. Anything that describes *other* notes reads the
index and therefore lands when the note is written.

## The rules

### Hosting, focus, `ctrl-w`

- The strip has its own `FocusHandle`, its own keymap context (`"sidebar"`), its own
  visibility flag (`sidebar_visible`), and a fixed width matching the tree's. Resizing
  waits on the tree's own width work.
- `ctrl-w l` from the **rightmost pane** focuses the strip — the same fallback shape the
  tree has today, where `ctrl-w h` moves left and, with nothing left, hands the keyboard to
  the tree (`workspace/mod.rs:846-858`).
- `ctrl-w h` from the strip goes to **the pane that last had the keyboard** — the seat, not
  a fixed edge. The tree's rule (leftmost pane) is unified to the seat in the same commit:
  one rule, and it is what "go back the way you came" means. **No wrapping** — from the
  tree, `ctrl-w h` stays in the tree; from the strip, `ctrl-w l` stays in the strip.
- `ctrl-w j`/`k` from the strip stay in the strip (EDGE-S12): it spans the window's height,
  so there is nothing above or below it.
- `escape` in the strip means what it means in the tree: hand the keyboard back to the note.

### Subject and `:pin`

The strip describes a **note**, not a tab (EDGE-S1): the focused note, or when the strip
itself has the keyboard, the note the pane last had (`Pane::note_mru`, `tabs.rs:130`). A
tab holding no note — the blank tab, the graph tab — leaves the subject where it was.
`:pin` is **one verb, two subjects**: on a tab it means "do not replace me", in the strip
"do not follow the tab switch". Same mark, same command, and the header shows it.

### Following the caret

The current section is marked in colour **always** (reusing the tree's "you are here"
treatment — the exact palette role is matched at implementation, per `design-language.md`).
The list **scrolls only when the mark itself changed** — the caret crossed into a different
section — and only if that row is off-screen. Keying the scroll to the mark *changing*
rather than to it being off-screen is what stops it undoing a scroll the user made
themselves (EDGE-S11). While the strip has the keyboard, the caret does not move the list at
all; the selection does, and the selection scrubs the note (below).

Two marks, two visuals, never confused: **current section** = coloured text; **selection** =
the standard focus highlight from `design-language.md` §4.

### Scrubbing

While the strip has the keyboard, moving the selection in the **Outline** view reveals that
heading in the note — scroll only, caret untouched. It is the mirror of following the caret:
the note's caret drives the strip when the note has the keyboard, the strip's selection
drives the note when the strip has it. **Outline only** — a backlink row has no position in
*this* note to reveal. `enter` is the commit that also moves the caret. The point, in the
user's words: the caret does not stick to the view, so backing out of the strip leaves the
note exactly where it was.

### Jumps: never a duplicate, never a re-read

The rule, stated against the code that already enforces most of it:

- `open_note` **finds** a tab that already holds the note rather than opening it again —
  in this pane (`show_tab`) or another one (`steal_tab`, which moves the tab whole into the
  pane with the keyboard) — and returns without touching the buffer when the tab holding it
  is the one on screen (`notes.rs:279-290`, `panes.rs:475`).
- The window holds **one buffer per note** (P4.17): `note_entry` reads from disk the first
  time and borrows from the registry after, so a second view of a note cannot fork its text
  (`notes.rs:320-326`).
- A tab is never repurposed out from under unsaved work: `load_target` reuses the tab on
  screen **unless** it is the only view holding a dirty note, in which case the arriving
  note gets a new tab (`notes.rs:396-416`, `views_of` at `panes.rs:488`). That is the fix
  for the data-loss bug closed at `60d6a29`.
- **A heading jump inside the note that is already on screen is not an `open_note` at all.**
  It is a reveal: move the caret to the heading's line in the editor that is already there.
  No tab is created, no tab is switched, nothing is read from disk. If the subject note is
  *not* on screen, the jump is an ordinary `open_note` (find-or-open, never duplicate)
  followed by the reveal.
- The jumplist records it (EDGE-S14).

### Structure edits: move and relevel

Both are **core acts returning a text edit** — `mdstyle::move_section` and
`mdstyle::relevel_section` — table-tested like every mdstyle act, with the strip as their
first door and the note's own keys a later commit (ruling J: one act, two doors).

- **Move** (`shift+j`/`shift+k`): swap with the next sibling at the same level under the
  same parent; refuse rather than skip to a different parent. Deeper headings travel with
  the section — `section_range` (`mdstyle/object.rs:409`) is already depth-aware.
- **Relevel** (`Tab`/`shift-Tab`): add or remove a `#` from the heading **and every heading
  in its subtree**, so the tree keeps its shape and only its depth moves. The children are
  whatever the scan found *before* the change (EDGE-S8).
- Caps are refused, not clamped (EDGE-S9). Only the marker run changes (EDGE-S10). One
  grouped edit, so one `u` restores the whole subtree, and the note goes dirty like any
  edit; a read-only surface refuses (EDGE-S4).

### Counts, ghosts, staleness

The header carries the view's count; a tag row carries how many other notes carry that tag;
the outline carries none. A ghost link is shown and marked in the outgoing list (EDGE-S7) —
a hole in the picture is a fact about the note. There is **no staleness badge** on
index-backed rows (ruling K): the header's count is the honesty, and the index lands within
the watcher's beat. With the index off, an index-backed view says there is no vault index
rather than showing nothing (EDGE-S6, the posture `:props` already keeps).

### Keys

| key | where | what |
| --- | --- | --- |
| `space b`, `:sidebar` | anywhere | show / hide the strip |
| `:sidebar outline\|links\|tags\|props` | anywhere | show the strip on that view; the keyboard stays where it is |
| `[` / `]` | strip | previous / next view |
| `j` `k` | strip | move the selection |
| `h` `l` | strip (outline) | collapse / expand a heading's children |
| `enter` | strip | act on the row (see the view table) |
| `shift+j` `shift+k` | strip (outline) | move the section |
| `Tab` `shift-Tab` | strip (outline) | relevel the section |
| `escape`, `ctrl-w h` | strip | back to the pane the strip came from |
| `ctrl-w h/j/k/l` | anywhere | tree ↔ panes ↔ strip, no wrapping |
| `:pin` | strip, tab | stick it |

`space` chords for the individual views are expected later and are not claimed here.

### Session

Per window, beside `tree_visible` (`workspace/session.rs`): `sidebar_visible`, the active
view, the per-view collapsed set, and the pinned subject. Restoring a session restores the
strip to the note the restored tab holds.

## Findings (measured against code, 2026-11-11)

| surface | today | what exists under it |
| --- | --- | --- |
| Outline | `:sections` picker only (vault-wide, `MAX_SECTIONS` 200) | `heading_lines` — the one scanner (P4.3b M2); `headings(note, line, text, depth)` in schema v4; `Db::sections_matching`; `section_range` (`mdstyle/object.rs:409`), depth-aware: a section ends at the next heading of the same or a higher level, so deeper headings travel with it; per-note fold memory; `i#`/`da#` |
| Backlinks | **no window surface at all** — `docs/getting-started.md:95` and `docs/notes.md:151` say so out loud; the graph draws them and the `backlinks` MCP tool answers | `links(source, target, target_kind, first_seen_at, last_seen_at)` with `links_target`; `Vault::backlinks(path, entries)` (`vault.rs:577`, scan-based, what MCP calls). **No `Db` reader, and `links` carries no line**, so "open the note *at* the link" is not answerable today |
| Outgoing links | none | the same `links` table read by `source`; the writer already does `SELECT target, target_kind, first_seen_at FROM links WHERE source = ?1` (`db/index.rs:470`) inside its diff — a reader, not a new query |
| Tags | `tag:` search operator only | `tags(note, tag)` (P4.3a M2); `Db::notes_matching` narrows on it (`db/index.rs:1418`); no census reader |
| Properties | `:props` (open note) and `:keys` (vault census) pickers | `Db::note_props(rel)` (`db/index.rs:859`) and `Db::key_census()` (`:822`) already exist; `PickerTarget::PropKey` already turns a chosen key into `prop:key` in the search field — the precedent for "a row that asks the next question" |
| The strip itself | the file tree **is** a sidebar: `tree_focus`, `tree_scroll`, `tree_selection`, `tree_visible`, `space e`/`:tree`, keymap context `"tree"` with `j k h l enter n r shift-d`, `ctrl-w h` in / `escape` out (`workspace/keys.rs:405`) | the whole hosting pattern to mirror: own `FocusHandle`, own keymap context, own visibility flag, session-restored expanded set |
| Focus grammar | `ctrl-w h` = pane left, else the tree; `ctrl-w l` from the tree = leftmost pane; from a pane = pane right; rightmost keeps the keyboard (`mod.rs:846-858`) | the fallback slot the strip takes at the right edge, and the seat rule to unify both edges on |
| "The note in hand" | `Pane::note_mru: Option<usize>` + `mark_note_mru` (`tabs.rs:130`), plus a vault-wide `recent` list in `Sessions` | the last-focused-note answer already exists per pane |
| Section move | nothing | `section_range` gives the byte span a move needs; `begin_undo_group`/`end_undo_group` give it one undo step |
| Jumplist | `record_jump` **refuses a same-note jump** (`jumps.rs:274-276`); `step_jump` already reuses a tab that holds the destination and reveals its offset (`jumps.rs:318-329`) | heading-to-heading motion is invisible to `ctrl-o` today; the chain machinery for same-note entries already works |
| Schema change cost | **no migration framework**: an older index is dropped and rebuilt, a newer one refused (`db/schema.rs:490-521`) | "schema v6" is a `SCHEMA_VERSION` bump, the new `CREATE TABLE`, and the superseded-version list — no migration code |

**P4.13 (Outline pane) is unblocked by P4.3b closing** — the plan's own 2026-11-08 pointer
said the pane waits on the `headings` table, and that table shipped 2026-11-10.

**P4.15 (computed surfaces) has policy and no consumer.** Its first named widget is the
rolls chip on a `!r` line: there is no `rolls` table in `db/schema.rs` and no `!r` grammar,
so inline dice does not exist. Its second candidate (current-section word count in the
gutter) and the cue chip both ride GM machinery from P7.x. Deferred by ruling 8 below.

## Binding rulings (round 1, 2026-11-11)

1. **Position: a right-side sidebar, opposite the file tree.** Not a tab, not a pane.
2. **Subject: the currently focused note; when the sidebar itself has the keyboard, the
   note it was focused on** — i.e. the pane's `note_mru`, not "whatever is on screen".
3. **`:pin` sticks the sidebar to one note** (it stops following tab switches).
4. **Keyboard-navigable, mirroring the tree's bindings.** `shift+j`/`shift+k` **move a
   heading**, and moving a heading takes its deeper sub-headings with it.
5. **Block ids are not in the outline view.** `:blocks` stays the block inventory.
6. **Current-section marking on both caret moves and scrolling.**
7. **The outline mirrors the note's fold state** — a collapsed heading collapses its
   children in the sidebar too.
8. **The sidebar is built first**; P4.15's first computed widget is deferred.
9. **Block-level backlinks are refused for now** and parked (see "Parked, look again").
10. **A block embed does not get a gutter mark** naming the block it took. The `![[…#^id]]`
    line already states it.

**Round 2 (2026-11-11):**

11. **One view at a time.** The strip shows a single view — outline, links to here, links
    from here, tags, or properties — not a stack. The view is *chosen*; the header strip
    names it and counts what it holds.
12. **`Tab` / `shift-Tab` relevel a heading** (add or remove a `#`), taking its subtree
    with it. The outliner idiom, and free in the strip's own keymap context.
13. **The strip joins `ctrl-w h/j/k/l` the way the file tree does** — one focus grammar
    for the window: tree, panes, sidebar.
14. **The outline follows the caret**: the current section is marked with coloured text and
    scrolled into view when it is off-screen, and the list's own selection carries the
    standard focus highlight. The two marks are different things and look different.
15. **Relevel is a structure edit with the same discipline as the move**: one grouped edit,
    one `u` restores the whole subtree, the marker run is the only thing that changes, and
    the caps are refused rather than clamped.
16. **`[` / `]` choose the view** inside the strip, with `:sidebar <name>` for the direct
    jump and `:sidebar` alone staying the toggle. `space` chords are expected later and are
    left unclaimed.
17. **Both edges use the seat.** `ctrl-w h` from the strip and `ctrl-w l` from the tree both
    land on the pane that last had the keyboard, unified in one commit. No wrapping.
18. **Scrubbing is Outline-only**, and it never moves the caret; `enter` is what moves it.
19. **A heading jump never duplicates a tab and never re-reads the note.** A same-note jump
    is a pure reveal in the editor already on screen; a cross-note jump goes through
    `open_note`'s find-or-open, and `load_target` still protects unsaved work.
20. **The header strip is chrome**, not a walkable row.
21. **Row meanings and counts as proposed** (round-1 C and D): outline row → jump, backlink
    row → open the linking note at the linking line, outgoing row → open the target, tag row
    → `tag:x`, property row → `key:` or open a link-shaped value. Counts in the header, and
    on tag rows.
22. **No staleness badge** on index-backed rows.

## Edge ledger (settled so far)

- **EDGE-S1.** The sidebar's subject is a *note*, not a tab. A tab that holds no note
  (the blank tab, the graph tab) leaves the subject at the last note the pane had —
  the same `note_mru` the local graph already leans on.
- **EDGE-S2.** A heading move never re-parents and never changes a heading's level.
  A move that would put `### B` outside its `## A` is refused in one sentence rather
  than silently rewriting the note's structure.
- **EDGE-S3.** A heading inside a code fence, inside a blockquote, or spelled setext is
  not a section (P4.3b's rulings: one scanner, blockquoted headings are not sections,
  setext unparsed), so it is not a row and cannot be moved.
- **EDGE-S4.** A move is one grouped edit, so one `u` puts the section back, and the note
  becomes dirty like any other edit. A read-only surface (preview mode, an embedded note,
  a `.shq`'s program) refuses.
- **EDGE-S5.** Sections that describe the open note read the **buffer**, not the index —
  an unsaved heading, tag or property is real and the index does not know it yet (the
  P4.13 pointer's rule, generalised). Sections that describe *other* notes (backlinks,
  outgoing links) read the index and therefore land when the note is written.
- **EDGE-S6.** With the index off, an index-backed section says there is no vault index
  rather than showing nothing (the EDGE-25 posture `:props` already keeps).
- **EDGE-S7.** A ghost link in the outgoing list is shown and marked. A hole in the
  picture is a fact about the note, not a reason to hide a row (P4.16's posture for
  ghosts in a lens).
- **EDGE-S8.** Relevel shifts the subtree the scan found **before** the change. Demoting
  `## A` while a `### E` follows it would otherwise make `E` a sibling and quietly drop it
  out of A's section; shifting every heading in the pre-change extent by the same delta
  keeps the tree identical and moves only its depth.
- **EDGE-S9.** The caps are refused, not clamped: a `#` heading cannot promote (there is no
  level above one), and a `######` cannot demote (a seventh `#` is text, not a heading).
  One sentence each.
- **EDGE-S10.** Relevel rewrites the marker run and nothing else on the line, so
  `## The Fall ^fall` keeps its id and still answers `#The Fall` (EDGE-27). `#heading`
  anchors elsewhere match by text, not level, so a relevel breaks no link and moves no edge.
- **EDGE-S11.** Following the caret never fights the user. The mark follows the caret
  always; the **scroll** fires only when the mark *changed* — the caret crossed into a
  different section — and only if that row is off-screen. A scroll the user made themselves
  is never undone, because nothing about their scroll changes the mark. While the strip has
  the keyboard, the caret does not move the list at all; the selection does.
- **EDGE-S12.** `ctrl-w j`/`k` from the strip stay in the strip: it spans the window's
  height, so there is nothing above or below it — the same answer the file tree gives.
- **EDGE-S13.** A sidebar jump is a jump like any other: it goes through `load_target`, so
  it never reads a note over unsaved work, and it reuses a tab that already holds the note.
  The strip's own scrolling never moves the caret, and revealing a heading is not a caret
  move — the caret only moves on `enter`.
- **EDGE-S14.** A same-note heading jump **is** a stop on the jumplist. `record_jump`
  refuses when the note is unchanged (`jumps.rs:274-276`) because a chain of the same note
  at the same place is noise; the position is the difference, so the guard becomes "same
  note *and* same position". `step_jump` already handles a destination the screen holds: it
  finds the tab, does not copy it, and reveals the offset (`jumps.rs:318-329`).

## Milestones

Each milestone ends with the full gate green (`cargo fmt --all`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`), a falsification pass over its own
new tests, and the docs it owes. `docs/sidebar.md` is owed by M1 — until then the surface is
not documented in `docs/`, per `docs/README.md`'s rule about not describing what is not
built.

### M1 — the shell and the Outline view

The strip exists and is worth using before a single index query is written.

- `sidebar_visible`, `sidebar_width`, a `FocusHandle`, a `"sidebar"` keymap context, and the
  render slot to the right of the pane tree at the tree's width.
- `space b` / `:sidebar` toggle; `:sidebar outline` shows it on a view; `[` / `]` cycle the
  five names — the four unbuilt views answer "not built yet" in the header until their
  milestone, so the verb is complete before the views are.
- `ctrl-w l` from the rightmost pane into the strip, `ctrl-w h`/`escape` back to the seat,
  and the tree's `ctrl-w l` unified onto the seat. `ctrl-w j`/`k` stay put.
- The Outline view: rows from the live buffer's heading scan, nested by level, honouring the
  note's folds (ruling 7); the header strip with the name and count.
- The caret mark and its conditional scroll (EDGE-S11), and Outline-only scrubbing (ruling 18).
- `enter` reveals the heading in the note and follows the keyboard in — a pure reveal when
  the note is on screen, `open_note` + reveal when it is not (ruling 19).
- Session fields land here, not later: a strip that forgets its width and its view on every
  restart is a strip nobody leaves on.

**Tests.** The strip opens, closes, and survives a session round-trip; `ctrl-w l` from the
rightmost pane focuses it and `ctrl-w h` returns to the same pane, and the tree's `ctrl-w l`
now returns to the seat rather than the leftmost pane; the outline lists a nested note in
order and depth; a heading typed but unsaved is a row immediately (EDGE-S5); `enter` on a
heading in the note on screen adds **no tab** and does not re-read the file — assert the
buffer identity, not just the caret; the mark follows the caret across a section boundary and
the list scrolls once, and a manual scroll afterwards is not undone (EDGE-S11); scrubbing
moves the editor's scroll and not its caret.

**Falsification.** Route the same-note branch through `open_note` → the no-duplicate test
must go red. Fire the scroll on "mark off-screen" instead of "mark changed" → the anti-fight
test must go red. Read the outline from the index instead of the buffer → the unsaved-heading
test must go red.

### M2 — the structure acts

- `mdstyle::move_section` and `mdstyle::relevel_section` in core: pure text edits returning
  the edit and a refusal reason, table-tested across fence, blockquote, setext, list, table,
  code fence, trailing `^block-id`, and the level caps.
- The strip's doors: `shift+j`/`shift+k`, `Tab`/`shift-Tab`, one grouped edit each.
- The note's own keys are **not** in this commit; the act is what is unified (ruling J).

**Tests.** Move swaps siblings and refuses across parents (EDGE-S2); a move takes the deeper
headings with it; relevel shifts the whole subtree by one and keeps the shape (EDGE-S8); `#`
and `######` refuse (EDGE-S9); `## The Fall ^fall` keeps its id and still resolves through
`#The Fall` (EDGE-S10); one `u` undoes a relevel of a five-heading subtree; a read-only
surface refuses.

**Falsification.** Compute the subtree *after* the level change → EDGE-S8's test goes red.
Clamp instead of refuse at the caps → the cap tests go red. Rewrite the whole heading line
instead of the marker run → the block-id test goes red.

### M3 — links, both directions

- Schema **v6**: `links` gains `line` (the line the link sits on). The cost is a
  `SCHEMA_VERSION` bump, the new `CREATE TABLE`, and the superseded-version list — the index
  is dropped and rebuilt, never migrated (`db/schema.rs:490-521`).
- `Db::backlinks(rel)` and `Db::outgoing(rel)` readers — the first production readers the
  `links` table has by `target` outside the writer's own diff.
- The two views, with the header count; a backlink row opens its note **at the line**; a
  ghost row is shown and marked (EDGE-S7); index-off says so (EDGE-S6).

**Tests.** A backlink row lands on the linking line, not the top of the note; a note that
links the same target twice is one row or two, decided at implementation and written into the
docs either way (see Parked); a ghost target is present and marked; with the index off the
view says there is no index.

**Falsification.** Drop `line` from the reader → the jump-to-line test goes red. Filter
ghosts out → the ghost test goes red.

### M4 — tags and properties

- The tag census reader (`tag` → how many other notes carry it) and the tag view; a row asks
  `tag:x`.
- The properties view over `Db::note_props` plus the buffer's frontmatter; a link-shaped
  value opens its note, anything else asks `key:`.

### M5 — `:pin`, polish, docs

- `:pin` as one verb with two subjects, the mark in the header, and the pinned subject
  surviving a tab switch.
- `docs/sidebar.md`: the shape, the five views, the focus grammar, and the buffer-vs-index
  rule. `keymap.md` and `commands.md` regenerate themselves.

## Not here

- Block-level backlinks (parked below), block ids in the outline (ruling 5), a gutter mark
  on block embeds (ruling 10).
- P4.15's computed widgets — deferred by ruling 8, and every named one waits on GM machinery
  anyway.
- The **canvas outline view** (P5.6). `roadmap.md:1668` already warns: do not build one and
  call it both.
- Outliner *editing* (P6.1) — bullets, not headings. The heading move here is a motion,
  not an outliner.
- Vault-wide browsers (`:keys`, `:sections`, `:blocks`, `:search`) stay pickers. The strip
  answers about one note; a browser answers about the vault.
- Resizing the strip (ruling G) and per-view `space` chords (ruling 16) — both later.

## Parked, look again

- **Block-level backlinks** — "which notes embed this block". Refused 2026-11-11 (ruling 9)
  because `links` rows are note-level and the anchor is dropped for resolution, so it is a
  schema change plus a re-index, not a view. Reopen when someone wants block provenance in
  the window; the sidebar's links section is where it would appear.
- **Two links to the same note in one note.** The `links` primary key says one row, which
  loses the second link's position; the line column makes that loss visible. Decide in M3
  with the row grammar in front of us, and say which in the docs.

## Open

**R. Does a same-note heading jump go on the jumplist?** It does not today — `record_jump`
refuses any jump whose note is unchanged (`jumps.rs:274-276`). The spec says yes
(EDGE-S14): the guard becomes "same note *and* same position", which keeps the noise case out
and puts heading hops on the chain. This changes existing behaviour, so it is called out
rather than buried: `ctrl-o` walking between sections of one note is what vim does and what
the question assumed.
