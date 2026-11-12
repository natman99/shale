# Sidebar plan — the right-hand pane that answers "what is this note"

**Round 1 ruled 2026-11-11.** The rulings section is binding. The open questions at the
end are not — they are the round 2 list, each with a recommendation.

## What this is

A fixed strip on the **right**, opposite the file tree, that describes one note: its
headings, the links pointing at it, the links it makes, its tags, its properties. It is
not a picker. A picker is a question you asked ("jump to a section"); the sidebar is an
answer that stays up. The four views share one style, one row grammar, and one keyboard.

## Findings (measured against code, 2026-11-11)

| surface | today | what exists under it |
| --- | --- | --- |
| Outline | `:sections` picker only (vault-wide, `MAX_SECTIONS` 200) | `heading_lines` — the one scanner (P4.3b M2); `headings(note, line, text, depth)` in schema v4; `Db::sections_matching`; `section_range` (`mdstyle/object.rs:409`), depth-aware: a section ends at the next heading of the same or a higher level, so deeper headings travel with it; per-note fold memory; `i#`/`da#` |
| Backlinks | **no window surface at all** — `docs/getting-started.md:95` and `docs/notes.md:151` say so out loud; the graph draws them and the `backlinks` MCP tool answers | `links(source, target, target_kind, first_seen_at)` with `links_target`; `Vault::backlinks(path, entries)` (scan-based, what MCP calls); the graph's edge index. **No `Db` reader, and `links` carries no line**, so "open the note *at* the link" is not answerable today |
| Outgoing links | none | the same `links` table read by `source`; the writer already does `SELECT target, target_kind, first_seen_at FROM links WHERE source = ?1` (`db/index.rs:470`) inside its diff — a reader, not a new query |
| Tags | `tag:` search operator only | `tags(note, tag)` (P4.3a M2); `Db::notes_matching` narrows on it (`db/index.rs:1418`); no census reader |
| Properties | `:props` (open note) and `:keys` (vault census) pickers | `Db::note_props(rel)` and `Db::key_census()` already exist; `PickerTarget::PropKey` already turns a chosen key into `prop:key` in the search field — the precedent for "a row that asks the next question" |
| The strip itself | the file tree **is** a sidebar: `tree_focus`, `tree_scroll`, `tree_selection`, `tree_visible`, `space e`/`:tree`, keymap context `"tree"` with `j k h l enter n r shift-d`, `ctrl-w h` in / `escape` out (`workspace/keys.rs:385`) | the whole hosting pattern to mirror: own `FocusHandle`, own keymap context, own visibility flag, session-restored expanded set |
| "The note in hand" | `Pane::note_mru: Option<usize>` + `mark_note_mru` (`tabs.rs:130`), plus a vault-wide `recent` list in `Sessions` | the last-focused-note answer already exists per pane |
| Section move | nothing | `section_range` gives the byte span a move needs; `begin_undo_group`/`end_undo_group` give it one undo step |

**P4.13 (Outline pane) is unblocked by P4.3b closing** — the plan's own 2026-11-08 pointer
said the pane waits on the `headings` table, and that table shipped 2026-11-10.

**P4.15 (computed surfaces) has policy and no consumer.** Its first named widget is the
rolls chip on a `!r` line: there is no `rolls` table in `db/schema.rs` and no `!r` grammar,
so inline dice does not exist. Its second candidate (current-section word count in the
gutter) and the cue chip both ride GM machinery from P7.x. Deferred by ruling 7 below.

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

## Milestones (dependency order)

- **M1 — the shell and the outline view.** `sidebar_visible`, its own `FocusHandle` and
  `"sidebar"` keymap context, `ctrl-w h/j/k/l` in and out (EDGE-S12), `space b` / `:sidebar`,
  the subject rule (EDGE-S1), the header strip naming the view and its count, the heading
  list with nesting, the caret-follow mark and its conditional scroll (EDGE-S11), and
  `enter` jumping into the note (EDGE-S13). The first thing a user can try, and the shape
  everything else hangs on.
- **M2 — the structure acts.** `mdstyle::move_section` and `mdstyle::relevel_section` in
  core — pure text edits, table-tested like every mdstyle act (EDGE-S8…S10) — with the
  strip's `shift+j`/`shift+k` and `Tab`/`shift-Tab` as their first doors. The note's own
  keys are a later commit.
- **M3 — links.** `Db::backlinks(rel)` and `Db::outgoing(rel)` readers, and the schema v6
  column that lets a backlink row land on the line that links.
- **M4 — tags and properties.** The note's tags with their census, each row asking `tag:x`;
  `Db::note_props` rows, where a link-shaped value opens its note.
- **M5 — `:pin`, session, docs.** One verb with two subjects, the session fields, and the
  prose in `docs/`.

## Not here

- Block-level backlinks (parked below), block ids in the outline (ruling 5), a gutter mark
  on block embeds (ruling 10).
- P4.15's computed widgets — deferred by ruling 8, and every named one waits on GM
  machinery anyway.
- The **canvas outline view** (P5.6). `roadmap.md:1668` already warns: do not build one and
  call it both.
- Outliner *editing* (P6.1) — bullets, not headings. The heading move here is a motion,
  not an outliner.
- Vault-wide browsers (`:keys`, `:sections`, `:blocks`, `:search`) stay pickers. The
  sidebar answers about one note.

## Parked, look again

- **Block-level backlinks** — "which notes embed this block". Refused 2026-11-11 (ruling 9)
  because `links` rows are note-level and the anchor is dropped for resolution, so it is a
  schema change plus a re-index, not a view. Reopen when someone wants block provenance in
  the window; the sidebar's links section is where it would appear.

## Round 2 status (2026-11-11)

**Settled by the round:**

- **A/B — one view at a time** (ruling 11). Not a stack, not a scroll-through of five
  sections. The header strip names the view and its count.
- **C — row meanings** as proposed, including `links.line` (schema v6) so a backlink row
  lands on the line that links.
- **D — counts** live in the header strip (the view's own count) and on tag rows (how many
  other notes carry the tag).
- **E — `space b` and `:sidebar`** toggle the strip.
- **F — one `:pin` verb**, two subjects.
- **G — fixed width** matching the tree; resizing waits on the tree's own width work.
- **H — session fields**: `sidebar_visible`, the active view, the per-view collapsed state,
  and the pinned subject.
- **I — move neighbours**: swap with the next sibling at the same level under the same
  parent; refuse rather than skip to a different parent.
- **I (relevel) — `Tab`/`shift-Tab`**, subtree shifts together, caps refused (EDGE-S8…S10).
- **J — one core act, two doors**: `mdstyle::move_section` and `mdstyle::relevel_section`
  return the text edit; the strip is the first door, the note's keys are a later commit.
- **K — no staleness badge**; the header's count is the honesty.
- **L — follow the caret** with the anti-fight rule (EDGE-S11).
- **M — `enter` on an outline row jumps the note to that heading and the keyboard follows
  into the note**, which is exactly what the file tree does today (`tree.rs:186`:
  `open_note` then `focus_editor`), so "mirror the tree bindings" gives it for free. The
  jump is a jump: it goes through `load_target` (the unsaved-work rule from `60d6a29`) and
  lands on the heading's line, where a trailing `^block-id` still resolves (EDGE-27). What
  one-view costs is a way to *choose* the view, which is question N.

**Still open:**

**N. How is the view chosen?**
(a) `[` / `]` cycle through the views inside the strip, plus `:sidebar <name>` for the
direct jump (`:sidebar` alone stays the toggle); (b) digits `1`–`5` for direct selection,
with the header showing the number beside each name; (c) `space`-leader chords in the
strip's context (`space o` outline, `space l` links, `space t` tags, `space p` properties),
matching the `space e` tree habit.
**Recommend (a)**, and (b) as an easy addition later — cycling is one pair of keys and the
command line is where a name is typed.

**O. Where does `ctrl-w h` from the strip land?** The tree currently goes to the **leftmost
pane** (`mod.rs:846`), not to the pane that last had the keyboard. For the strip, the
symmetric answer is "the pane that last had the keyboard" (the seat). Recommend: **the seat
for the strip**, and unify the tree to the same rule in the same commit — one rule, and it
is what "go back the way you came" means.

**P. Is the header strip a row in the list or chrome above it?** Recommend **chrome**: `j`
starts on the first real row, and the header is not something you can get stuck on. It
carries the view name, its count, and the pin mark.

**Q. Does the list selection scrub the note?** While the strip has the keyboard, moving the
selection in the Outline view *reveals* that heading in the note — scroll only, the caret
untouched. It is the mirror of ruling L: the note's caret drives the strip when the note has
the keyboard, and the strip's selection drives the note when the strip has it. Recommend
**yes for the Outline view only** (a links row has no position in this note to reveal).
