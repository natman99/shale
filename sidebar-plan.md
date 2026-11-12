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

## Milestones (dependency order)

- **M1 — the shell.** `sidebar_visible`, its own `FocusHandle` and `"sidebar"` keymap
  context, `ctrl-w l` in / `escape` out, the subject rule (EDGE-S1), `:pin`, session
  fields. Ships with the **outline** view only.
- **M2 — links.** `Db::backlinks(rel)` and `Db::outgoing(rel)` readers; the schema bump
  that lets a backlink row land on the line that links (open question C).
- **M3 — tags.** The note's tags, each row asking `tag:x`.
- **M4 — properties.** `Db::note_props` rows; a link-shaped value opens its note.
- **M5 — the move.** `shift+j`/`shift+k` on the outline: one core act returning the text
  edit, the sidebar as its first door.

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

## Open questions (round 2 — recommendation attached to each)

**A. One list or four views?**
(a) one continuous list whose rows belong to collapsible sections (`Outline`, `Links to
here`, `Links from here`, `Tags`, `Properties`), `j`/`k` walking across section
boundaries and `h`/`l` collapsing a section — the tree's grammar exactly, and no mode;
(b) one view at a time behind a command.
**Recommend (a).** A mode is the thing this repo rations, and the tree already teaches
`h`/`l` over a list of sections.

**B. Section order and defaults.** Recommend fixed order Outline → Links to here → Links
from here → Tags → Properties, all open, each collapsed state saved per window.

**C. What a row is, and what `enter` does.**
- Outline row → jump the note to that heading.
- Backlink row → open the linking note. **Sub-question:** land on the line that holds the
  link? That needs `links.line` (schema v6) — one column, written by the same per-note
  writer that already records the link. **Recommend yes**: a backlink list that opens the
  top of a 400-line note is half a feature, and `PickerTarget::Hit` already carries
  path+line.
- Outgoing row → open the target (a ghost row is marked and `enter` offers the create
  that P6.x's unresolved-link→create plans).
- Tag row → put `tag:x` in the search field (the `PropKey` precedent).
- Property row → `prop:key` in the search field; a whole-`[[…]]` value opens its note
  instead (the frontmatter ruling on what is link-shaped).

**D. Row counts.** Recommend: tags show how many *other* notes carry them (one
`GROUP BY`), the links sections show their count in the header, the outline shows none.

**E. The toggle key and the command.** `space e` is the tree's. Recommend `space b` and
`:sidebar` for the toggle, with no per-section commands — the sections are not doors,
they are what is there.

**F. `:pin` shape.** One verb, two subjects: in a note tab it means "do not replace me"
(P3.2's unbuilt note pinning), in the sidebar it means "do not follow the tab switch".
Same mark glyph. **Recommend one verb**; two verbs for one idea is how they drift.

**G. Width.** The tree is fixed-width today. Recommend the sidebar match it (fixed,
same measure) and leave resizing to whatever config work the tree's width eventually
gets.

**H. Session fields.** `sidebar_visible`, the per-section collapsed set, and the pinned
subject. **Recommend yes** — the session file already stores the tree's expanded folders,
and adding fields to `shale_core::Session` breaking the app's test literals is the
designed consequence.

**I. Move neighbours.** `shift+j` swaps a section with the next **sibling at the same
level under the same parent**, skipping nothing: if the next section at that level belongs
to a different parent, refuse (EDGE-S2). Alternative: move it to the end of the parent's
children. **Recommend refuse** — the skip is a guess about intent, and a moved section in
the wrong place costs the author a scan to find it.

**J. Does the buffer get the same motion?** Recommend: build it as one core act
(`mdstyle::move_section` returning the edit), sidebar door in M5, buffer door (`]#`/`[#`)
as a later commit — the same labour split that made `:block-id` and `#^id` share a scan.

**K. Stale index, honest or silent?** Backlinks land only after the note is written and
re-indexed. Recommend: no staleness badge (the watcher indexes on save, so the lag is one
beat), but the section header counts what it shows, so a missing row is visible as a
count.
