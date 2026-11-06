# Editable transclusion — the plan (P4.0b)

Status: **design pass complete 2026-11-06; nothing built.** This is the interaction
design pass the roadmap's design gate asked for ("transclusion cursor handoff"), plus
the architecture pass it did not ask for and needed more.

Reads this plan is grounded in: `crates/core/src/buffer/mod.rs:15-60` (the struct that
owns the state), `crates/app/src/editor/mod.rs:137,229,405` (who holds that struct and
where `dirty` actually lives), `crates/app/src/workspace/tabs.rs:90-95` (a tab owns its
editor), `crates/app/src/workspace/notes.rs:37` (the one-note-per-window rule),
`crates/app/src/workspace/mod.rs:774` (`apply_disk_changes`), `docs/keymap.md` and
`crates/core/src/vim/motions.rs` (what keys are free).

---

## 1. Binding rulings

Not re-litigating these; they are decisions, and each is dated to the round that made it.

1. **Two buffers, one view.** An embed is not spliced into the host's buffer. The host
   buffer stays exactly what its file is. The embed's rows belong to the *other note's*
   buffer and are drawn inside the host's column. Splicing would make the buffer stop
   matching its file, which poisons save, search, the ruler, the index and every
   row-relative motion. (2026-10-29, restated 2026-11-06.)
2. **One level of editing.** Only a level-1 embed is enterable; anything nested inside it
   is read-only. The nesting cap stays binding: 1 editable + 2 read-only = 3 levels,
   deeper renders as a name badge. (2026-10-29; "only one level of editing" 2026-11-06.)
3. **Entering: click, or `ctrl-]` on the embed's own `![[…]]` row.** `ctrl-]` is vim's tag
   jump — "go into the thing under the cursor" — and it is free in Shale. It sits beside
   `gd`, which opens the embedded note *in the tab*: `gd` moves you, `ctrl-]` brings the
   note into you. `gr` is **not** used: it is vim's virtual replace (`gr{char}`, `gR`
   mode) and binding it would show up as a diff in `scripts/vimcrosscheck.py`.
   (2026-11-06.)
4. **Exiting: `ctrl-o`.** vim records tag jumps in the jump list, so `ctrl-o` is already
   how a vim user comes back from `ctrl-]` — and Shale's jump chain already lands on the
   exact byte. The embed stack *is* the jump chain; there is no second stack to keep in
   sync. `ctrl-t` (vim's tag-pop) is unavailable — it is new-tab — and does not matter.
   **`<esc>` never exits**: it returns to normal mode *inside* the borrowed text.
   (2026-11-06.)
5. **One scroll axis: the host's.** No nested scroll regions. A long embed is scrolled
   *through*, not inside. The scrollbar and minimap measure the view's extent, embed rows
   included. (2026-11-06: "I don't want layered scroll fields, feels too much like a
   browser and it gets confusing fast.")
6. **The seam is a buffer boundary, and everything buffer-scoped stops there.** `j`/`k`,
   counts, `gg`/`G`, `{`/`}`, `d}`/`ci"`, visual selection, `/` search, `za` folds, the
   ruler, and undo. Phrased so the screen motions fall out correctly: **the view scrolls
   freely; the caret never crosses the seam on its own** — `ctrl-d`/`ctrl-u`/`ctrl-f`/
   `ctrl-b` scroll and the caret clamps at the seam. (2026-11-06: "let's make them stop at
   the boundary, that way `gg` and `G` will also work properly.")
7. **One caret per note per window.** The caret stays inside `Buffer`. A note open by tab
   and shown by embed share text *and* caret. Separate per-view carets are the expensive
   half and are explicitly **not** taken now. (2026-11-06.)
8. **Per-view state moves out of `Buffer`; model state moves into a registry.** The trio
   that is per-view today — `display_segments`, `folds`/`fold_cache`, `read_only` — becomes
   view state. `dirty` moves the other way: it is per-note state and currently sits in the
   editor. (2026-11-06: "move the display segments and folds out and leave the caret in
   the buffer for now.")
9. **Dirty display is transitive; dirty state is per file.** A host tab's dirty dot lights
   when the host *or any embed it renders* is dirty. The status line names which notes.
   (2026-11-06.)
10. **`:w` inside an embed writes host and embeds together.** One keystroke, one meaning,
    regardless of where the caret sits. Closing a host with a dirty embed warns through the
    dirty-tab machinery. The host never *silently* saves the embed. (2026-10-29 + 2026-11-06.)
11. **Undo is buffer-local with a seam indicator** — exhausted local undo reports
    `next undo is in [[morgoth]]` instead of beeping. Undo follows the text, so a tab and
    an embed of one note share one undo stack; the indicator is about crossing *notes*,
    not views. (2026-10-29.)
12. **An embed renders the note as Shale has it in hand, not as the disk has it.** If a tab
    holds unsaved edits to `gift.md`, the embed of `gift.md` shows those edits.
    (2026-11-06; `embeds::Cache` reads disk today and must consult open buffers first.)
13. **One note may appear several times in one host, showing different sections at once.**
    `![[gift#The middle]]` and `![[gift#The end]]` both render — the point is reading two
    parts of one note side by side, which is a posture the app should not need two panes
    for. (2026-11-06, extending the Q2 ruling.)
14. **Edit-ness follows the caret, not the position in the file.** The block the caret is in
    is editable; every other block of that note is a **view** — phantom rows, no caret
    painted, no click-to-edit until you click into it (which moves edit-ness there). Only
    one block per note per view is ever entered. (2026-11-06.)
15. **A view whose section stops resolving shows a badge naming the missing section.** No
    polling: the check happens when the rest of the view re-renders, which is every frame
    already. The entered block is the exception — it exits with a status line instead,
    because a caret cannot be badged out from under itself. (2026-11-06.)
16. **The embed cache keys its bodies on the note's `change_id`.** `Buffer::change_id()`
    (core/src/buffer/mod.rs:151) counts every edit and is already how `fold_cache`
    invalidates; keying the body cache on it is ruling 12 implemented with a number
    instead of a policy — a note edited in a tab, or in another embed block of itself,
    re-renders on the next paint with no timer and no watcher. (2026-11-06.)

---

## 2. What a view is made of

`shale_core::Buffer` (core/src/buffer/mod.rs:15) holds three kinds of thing in one bag,
and `MarkdownEditor` holds it **by value** (`buffer: Buffer`, editor/mod.rs:137):

| kind | fields | right home |
|---|---|---|
| model | `text`, `undo`, `redo` | the note — one per note per window |
| caret | `cursor`, `selection`, `desired_col` | the note, **for now** (ruling 7) |
| view | `display_segments`, `folds`, `fold_cache`, `read_only` | the view — one per (view, note) |

`dirty: bool` sits on the wrong side of that line today: it is at `editor/mod.rs:229`,
per-view, while it describes a note. Two views of one note would disagree about it
immediately.

Measured cost of the moves:

- `display_segments`: 3 reads inside core (all in `buffer/display.rs` + the clear in
  `mod.rs:100`), 1 writer outside (`editor/wrap.rs:420`). Nearly free.
- `folds`/`fold_cache`: 11 reads, all inside `buffer/folds.rs`, which is a thin wrapper
  over `fold::FoldState`. Nearly free — the wrapper moves to the view.
- `read_only`: 2 accessors on Buffer, set from the editor's `View` (editor/mod.rs:405).
  Trivial, and it must move: one note can be Preview in a tab and Live in an embed.
- The registry (`buffer: Entity<Buffer>`): **132 `self.buffer` sites across 6 app files**
  (`mod.rs`, `render.rs`, `wrap.rs`, `input.rs`, `actions.rs`, `overlay.rs`) — and **0 in
  tests**, which go through the public API. Mechanical, wide, and the suite is a net
  rather than something to rewrite.

---

## 3. The registry, and why it is the first commit

Today a note is open once per window: `open_note` (notes.rs:37) finds the note in *any*
pane and **steals** the tab into the pane with the keyboard. So "the same note twice in a
window" cannot happen by tabs. An embed is the first second holder the app would create —
a tab plus an embed, or two embeds of one note — and the invariant forbids it.

Therefore the prerequisite is the feature's first commit, not a later cleanup: building
editable embeds first turns the collision case into a bug in shipped code.

**Design:** a per-window registry, `path → Entity<Buffer>`. Tabs and embeds both borrow
from it. A note is open once per window *whether it got there by tab or by embed*. The
caret stays in the Buffer, so the tab and the embed show the same caret — which is the
same invariant the tab bar already enforces, and better than independent carets: switch
to the tab and you are exactly where you were inside the embed.

Entry policy: an entry lives while a tab holds it **or** it is dirty. A clean, unheld entry
is just a cache and can go.

Cross-window is deliberately untouched: two windows already hold two copies, and
`apply_disk_changes` (workspace/mod.rs:774) folds an outside write in — a clean tab
re-reads, a dirty tab is left alone and last-writer-wins on save. That is existing,
deliberate behaviour and P4.0b does not change it.

---

## 4. Milestones, split by dependency

**M0 — view state out of `Buffer`.** `display_segments`, `folds`/`fold_cache`,
`read_only` → a `ViewState` owned by the editor, keyed per buffer it draws. ~14 core sites
+ the editor. No behaviour change; the test is that `Buffer` stops answering wrap and
fold questions. *Free on its own: it is what lets a note be Preview in one pane and Live
in another.*

**M1 — the registry.** `MarkdownEditor.buffer: Entity<Buffer>` (132 sites), entries keyed
by path per window, `dirty` moves onto the entry, `open_note`'s one-note rule extended to
embeds, `apply_disk_changes` walks the registry instead of the tab list, and **both write
paths** (`write_tab` and `autosave`) cover entries with no tab. This is the wide commit and
it is worth landing alone.

**M2 — the row list spans buffers.** The view's display list becomes
`ViewRow { buffer, row }`; the caret carries which buffer it is in; motions clamp at the
seam (ruling 6); `ensure_cursor_visible` and typewriter scroll work on view rows. Inside an
entered embed, rows are ordinary buffer rows — real cursor, real selection, real marker
hiding — while nested embeds stay phantom blocks.

**M3 — entry and exit.** The hit-test from a phantom row back to the body row under it
(already P4.0a's leftover #1, and the same machinery click-to-open and the hover card
need), `ctrl-]` recording a jump, `ctrl-o` leaving, the seam header naming the note, the
status bar and ruler following the caret's note.

**M4 — the save story.** Transitive dirty dot, `:w` writing host + embeds, the close
warning enumerating dirty embeds, autosave covering them, and a partial write failure
naming which note wrote.

M1 before M3 is not a preference, it is the ordering rule.

---

## 5. Edge-case ledger

**Lifecycle**
1. **Dirty embed with no tab is work with no visible home.** The registry keeps it alive,
   but nothing shows it. → Q1.
2. Host tab switched away while its embed is dirty: same problem one step earlier. → Q1.
3. Host tab closed with a dirty embed: warn (ruled 10).
4. Quit / SIGTERM: `write_all_tabs` must cover registry entries with no tab. Two write
   paths, one rule (the recurring mistake in this repo).
5. Note deleted from disk while an embed buffer is dirty: the buffer stays on screen so
   `ctrl-s` can write it back — the existing rule for tabs, extended to the registry.
6. Rename of an entered note: rekey the registry entry and let rename propagation rewrite
   the host's `![[…]]` text (it already rewrites links).
7. Vault switch: the registry belongs to the window's vault services and goes with it.
8. Session restore: entered-embed state is not persisted; embeds come back read-only. → Q8.

**Two views, one buffer**
9. **The same note embedded twice in one host** — ruled (Q2 + 2026-11-06): both render,
   different sections side by side is the point. The entered block is real rows; the others
   are views (ruling 14). The resolver already allows it: the chain is per-path
   (`chain: &mut Vec<PathBuf>`, push/pop at transclude.rs:113-150), never a global seen-set,
   so sibling embeds of one note both expand today.
9a. **The double-drawn caret is the hazard this ruling creates.** One caret per note
    (ruling 7) plus two blocks of that note = the naive render paints the caret twice.
    Only the entered block paints caret and selection. This needs a test, because a caret
    drawn in a block you are not editing reads as a bug and someone will fix the wrong half.
9b. **Overlapping sections**: a subsection embed and a parent-section embed of one note
    share rows, so typing in the entered block visibly changes the view block. That is one
    text doing what it should; pin it with a test so it never gets "fixed" as a glitch.
9c. **The depth cap counts levels, not occurrences**: two level-1 embeds of one note are
    both level 1, each nesting two more levels along its own chain.
10. Note open in a tab in pane A and entered in an embed in pane B: the same caret appears
    in both panes, live. That is the invariant working, and it needs a test so it never
    reads as a bug.
11. Different pane widths → different wrap segments for the same text; different folds;
    different `View` modes. All covered by M0.
12. The seam header (note name, "editing") is view chrome: not editable, not a buffer row,
    not counted by `G`.

**Editing semantics**
13. `gg`/`G` are buffer-scoped: inside an embed, `G` goes to the end of the *embedded*
    note and the host scrolls to show it.
14. `H`/`L` are screen-scoped and stay view-relative.
15. `/` searches the caret's buffer; `n`/`N` stay in it. The search popup should say which
    note it is searching.
16. `za` inside an embed folds in the embed's note, in that view's fold state.
17. `:q` while inside an embed acts on the tab (the host), and the embed's dirty state
    participates in the warning. → Q7.
18. `:w <name>` and `:rename` while inside an embed: which note? → Q4.
19. A read-only refusal must name the note it refused to write.
20. Partial save failure (one note read-only on disk): the status names what wrote and what
    did not.
21. Registers are shared (they are app state); undo is per buffer. Both correct as-is.

**Entry / exit**
22. `View::Preview` gates entry — a read-only view cannot hand over a caret
    (`set_read_only`, editor/mod.rs:405).
23. Entering an embed whose note is already open in a tab: allowed, and uneventful, because
    of the registry (ruling 7).
24. Entering a broken or missing embed: status line, no entry.
25. Entering a nested (level ≥ 2) embed: not offered (ruling 2).
26. Self-embed: the chain badge already covers it; not enterable.
27. Image embeds (`![[img.png]]`, already built) are never enterable.
28. Clicking host text while inside an embed: → Q3.
29. Tab switch while entered: the embed state drops, the host's caret is where it was.
30. `ctrl-o` after entering: leaves the embed. `ctrl-i` forward still works; the chain is a
    chain.

**The cut moves under you**
31. An entered `![[gift#The middle]]` whose heading is renamed or deleted (by typing in the
    embed, or by an agent write): the section the embed was cut from no longer exists while
    the caret is inside it — **ruled (Q5)**: exit the embed, status line, caret back on the
    host's `![[…]]` row. A *view* block of the same note badges instead (ruling 15); the
    asymmetry is deliberate and the rule behind it is *the caret decides*.
31a. Editing the entered block can delete a heading a *view* block of the same note was cut
    from (a subsection embed inside a parent-section embed). The view badges; nothing else
    happens. No conflict with 31 — different halves of the same rule.
32. The embed cache re-derives references from the host buffer every frame and caches the
    reads; the entered note must bypass the disk cache and render the live buffer
    (ruling 12).

**Agent / MCP**
33. An agent writes a note that is embedded and dirty: the external-change path leaves the
    dirty buffer alone — same rule as tabs, and it must reach registry entries (M1).
34. A pending agent *proposal* on an embedded note: the strip lists it; the inline overlay
    does not paint inside an embed in v1. **Deferred (Q6)** — the overlay composition it
    needs (struck rows and phantom additions *inside* another overlay's block) belongs to
    roadmap **P4.2**, which now carries the pointer so this does not get lost.

**Perf**
35. The view's row list is rebuilt per frame over host + embed rows; the cap keeps the
    embed side small, and only one embed per view is ever editable, so at most one extra
    `ViewState` per view.
36. Nothing here adds a periodic beat. The embed cache stays frame-driven.

---

## 6. Free rides (no rulings needed)

- `gd` on a wikilink *inside* an entered embed: those rows are ordinary buffer rows, so the
  existing link-following works unchanged.
- `ctrl-o` exit: the jump chain exists and already lands on the exact byte.
- The seam rule in the gutter: `line::marks::glyph_x` already owns that column.
- Backlinks, the index, the graph, word counts: they follow saves and are untouched.
- The status bar and the ruler exist; they just need to follow the caret's note.
- `heading_row` (P4.6a) is already the shared heading lookup, so an embed's cut and a
  `gd` jump cannot disagree about which heading an anchor means.

---

## 7. Open questions — **ALL RESOLVED 2026-11-06**

The user answered all eight in one reply; Q2 came back extended, and the extension is now
ruling 13-16. Additions I made beyond what was ruled are marked **(addition)** so they can
be vetoed.

**Q1. A dirty embed with no tab.** ✅ **RESOLVED — materialise a tab** the moment an embed's
buffer goes dirty and no tab holds it. Unsaved work always has a visible home, the dirty dot
has somewhere to light, and the close/quit warning needs no new machinery. It also settles
`:q` from inside an embed (Q7): there is a tab, so closing the host's tab is a plain
dirty-tab close.

**Q2. The same note embedded twice.** ✅ **RESOLVED, extended by the user.** Not "first real,
later a badge" — **both render**, because reading two different sections of one note at once
is a posture worth having without two panes. The first is editable and the second view-only;
a view whose section lookup fails (or whose note was edited elsewhere so the section moved)
shows a **badge naming the missing section**; and there is **no polling** — the check happens
when the rest of the view re-renders. That became rulings 13-16.
**(addition)** Edit-ness follows the caret rather than file position (ruling 14): clicking a
view block enters it and moves edit-ness there, which is what ruling 3 already implies and
what a position-based rule would break the moment you click the second block.
**(addition)** Only the entered block paints the caret and selection (ledger 9a) — with one
caret per note, the naive render draws it twice.
**(addition)** Overlapping sections visibly change while you type in the other block, and
that is pinned as correct rather than fixed (ledger 9b).

**Q3. Clicking host text while inside an embed.** ✅ **RESOLVED — exits.** The caret is in a
buffer; clicking puts it where you clicked.

**Q4. `:rename` / `:w <name>` from inside an embed.** ✅ **RESOLVED** — `:rename` acts on the
caret's note (rename propagation already rewrites the host's embed line); `:w <name>` refuses
inside an embed, consistent with the existing rule that the command line must not clone a
note by accident.

**Q5. The entered embed's section disappears under the caret.** ✅ **RESOLVED — exit with a
status line**, caret back on the host's `![[…]]` row. No conflict with Q2: a *view* block of
the same note badges instead, because there is no caret inside it to rescue. The rule behind
both: **the caret decides.**

**Q6. Agent proposals on an embedded note.** ✅ **RESOLVED — deferred**, and written down in
two places so it cannot be lost: ledger 34 below, and roadmap **P4.2** ("Diff renderer"),
which is where an overlay that composes struck rows and phantom additions inside another
overlay's block belongs. The strip stays authoritative and already lists them meanwhile.

**Q7. `:q` from inside an embed.** ✅ **RESOLVED — closes the host's tab**, with the embed's
dirty state feeding the warning. Depends on Q1: the embed has a tab by then, so this is an
ordinary dirty-tab close.

**Q8. Does the entered embed ride the session file?** ✅ **RESOLVED — no.** Embeds come back
read-only; the session file describes panes and tabs, not views-within-views, and restoring
a caret into a region that has to be re-derived from text that may have changed is exactly
the trap this repo has been bitten by once.

---

## 8. What is settled enough to build

Rulings 1-16, the ledger, and milestones M0-M4 are decided. The build order is M0 (view state
out of `Buffer`) → M1 (the registry, P4.17) → M2 (the row list spans buffers) → M3 (entry and
exit) → M4 (the save story), and M1 lands before any of the visible feature.

Three tests carry the new rulings and should be written with the code that satisfies them:
the caret painted in one block only (9a); a view block badging when the entered block's
typing deletes its heading (31a); and the body cache re-deriving on `change_id` when a note
is edited in a tab while embedded (ruling 12 + 16).
