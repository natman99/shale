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

**M0 — view state out of `Buffer`.** ✅ **done 2026-11-06** (`8b742f3`, `85f718e`).
`display_segments` and `folds`/`fold_cache` now live in `buffer::ViewSlot`, one per
`MarkdownEditor`, and every display-aware buffer method names the slot it is answering
for — the compiler, not a convention, is what stops two views of one note answering each
other's questions. The vim engine takes the slot beside the buffer; its test harness types
keys against an unwrapped slot unless a test builds one on purpose (`type_keys_in`).
A test pins the point of the refactor: two slots on one buffer, folded and wrapped
independently.

Two things the milestone text got wrong, learned by doing it:

- **`read_only` waits for M1.** *(Done with M1, as required — see the M1 note below.)* It is per-view in the same way folds are, but the flag only
  becomes *meaningful* per-view when a buffer is first shared, and that is M1's change.
  Moving it means threading a view through ~180 call sites of the guarded edit methods
  (`insert`, `delete_*`, `undo`, …); doing it in the same commit as the registry keeps the
  churn where the sharing becomes real. **M1 must not ship without it**: a Preview tab and
  a Live tab of one note would fight over one flag.
- **The seam was already leaking.** `set_cursor` used to set `desired_col` from the *buffer*
  column, so a `t`/`f`/`T`/`F` find landing mid-row on a wrapped row reset the vertical aim
  to a column the user never aimed at, and the next `j` landed somewhere else. Fixed: the
  aim is latched by the first vertical motion, from the display column the caret actually
  landed on. Worth remembering as the pattern for P4.0b — anywhere a *buffer* quantity is
  used to answer a *display* question is a bug waiting for a narrow pane.

**M1 — the registry. ✅ landed 2026-11-06.** Landed as `crates/app/src/note.rs`
(`Note`: the shared `Rc<RefCell<Buffer>>` plus the entry's `dirty`) and
`Workspace::note_entry` (the per-window `path → Weak<Note>` map). `Entity<Buffer>`
became `Rc<RefCell<Buffer>>`: the vim engine takes `&mut Buffer` and `&mut ViewSlot`
in one call, and inside an `Entity::update` closure the window's other state is
unreachable — `RefCell` is already house style here. Handles are `Weak`, so the last
view to let go retires the note with no unregister step. `read_only` moved to
`ViewSlot` with this milestone, as required: preview is per view, and a flag on the
shared text let one reading tab lock out another tab editing the same note.
`apply_disk_changes` and the vault-wide rewrite now update the entry once and notify
every view of it. What M1 did *not* take: the write paths for entries no tab holds
(M4), and the caret, which stays with the buffer for now.

The plan's original M1 wording, kept for the parts still owed: `MarkdownEditor.buffer: Entity<Buffer>` (132 sites), entries keyed
by path per window, `dirty` moves onto the entry, `open_note`'s one-note rule extended to
embeds, `apply_disk_changes` walks the registry instead of the tab list, and **both write
paths** (`write_tab` and `autosave`) cover entries with no tab. This is the wide commit and
it is worth landing alone.

**M2 — the row list spans buffers.** The view's display list becomes
`ViewRow { buffer, row }`; the caret carries which buffer it is in; motions clamp at the
seam (ruling 6); `ensure_cursor_visible` and typewriter scroll work on view rows. Inside an
entered embed, rows are ordinary buffer rows — real cursor, real selection, real marker
hiding — while nested embeds stay phantom blocks.

**M2a landed.** Every embed row now carries an `Origin { path, row }`
(`transclude::Origin`), which turns a painted row into an address: something to click,
to put a caret in, and to check for still existing. The resolver reads note text through
a `Sources` trait, and the app's implementation answers from the window's registry
before it reads the file — so an embed of a note open in another tab shows that tab's
unsaved edits (ruling 12). The registry's `stamp()` hashes every open buffer's
`change_id`; when it moves, cached bodies are thrown out. That is ruling 16 with no
polling and no freshness policy: an edit anywhere in the window, a re-resolve on the
next paint, nothing in between.

Still owed in M2: the display list itself. `HitLine` is the address record and
`LineLayout::build` is the shaping pass an embed row can reuse, but neither knows which
buffer a row belongs to — that tag, and the caret carrying it, is what M2b adds, along
with motions clamping at the seam.

**M2b, first half landed.** A phantom row now records its own on-screen band as it
paints (`line::phantom::PhantomHit { bounds, origin }`, collected in a cell the editor
owns and clears each frame), and `MarkdownEditor::origin_at(point)` answers a point with
the note and row it was drawn for. The note's own rows answer `None`, and the bands are
pinned to sit between the asking row and the next one — a hit record that lands in the
wrong band sends a click to the wrong place in the note. Nothing consumes it yet: the
click that enters an embed is M3, and so is the display list that has to make the
entered block's rows real enough to hold a caret.

**M3 — entry and exit.** The hit-test from a phantom row back to the body row under it
(already P4.0a's leftover #1, and the same machinery click-to-open and the hover card
need), `ctrl-]` recording a jump, `ctrl-o` leaving, the seam header naming the note, the
status bar and ruler following the caret's note.

**How M3's rows get built (measured 2026-11-07, before writing any of it).** The entered
block's rows have to be real enough to hold a caret, and there are two ways to do that.

*(a) the plan's wording — the display list spans buffers.* `ViewSlot.segments` becomes a
list of `(buffer, row)` pairs and every display-index-shaped thing (motions, wrap, folds,
`display_index_for`, the ruler, scroll-to-item) learns the tag. One index space, which is
what makes `ensure_cursor_visible` and typewriter scroll fall out. Cost: the editor holds
129 `self.buffer` sites (`actions` 38, `mod` 32, `wrap` 26, `render` 14, `input` 13,
`overlay` 6), and each has to be read once to decide whether it means "the caret's
buffer" or "the note this tab is showing" — the split is real and it is the whole job.

*(b) the active buffer.* `self.buffer` becomes the caret's buffer and `self.note.buffer`
stays the tab's note; the host keeps painting its own rows and the entered block is
painted from the other buffer. Cheaper, and the seam comes free (every motion is
buffer-scoped, so it already stops at the edge). What it breaks is the index space: the
scroller's items are the host's line divs, so a caret on an embed row has no item to
scroll to, and `bounds_for_item`, `reveal_line` and typewriter scroll all ask for one.

**Recommendation: (b) for the caret, and *no change at all* to the scroller's item
space.** The host's whole display model keeps answering for the host alone: `j`/`k`,
folds, the ruler and search are ruled to stop at the seam, so a display list that already
contained the embed's rows would make them *wrong* and every buffer-scoped motion would
need a new clamp. Under (b) they are right by construction.

The scroller question looks like it needs (a), and does not. Measured against gpui
(v1.25.3, `elements/div.rs:1990-2000`): `ScrollHandle::child_bounds` is filled from the
tracked div's **direct children, positionally**, and `scroll_to_item(i)` /
`bounds_for_item(i)` index that array. The editor's scroller has exactly one direct child
per display line (`render.rs:759/762`, `div().id(("line", index))`), which is why display
index and child index coincide today — and why promoting an embed row to a direct child
would shift every later index and break `j`/`k` centring, `ensure_cursor_visible` and the
ruler at once. Phantom rows already sit *inside* the wrapper, so they add height to
`child_bounds[index]` without adding an index. That is the shape to keep: **the scroll
target of a caret inside an entered embed is the host line the embed hangs off.** Which
is not a compromise — the ruling is "the host scrolls; the embed never has its own
viewport", so scrolling to the anchor row is exactly the behaviour.

What (b) does need is a second record beside `HitLine`, because `HitLine` cannot carry
it: `start`/`end` are host byte offsets, and both `x_for_offset` and `offset_for_x`
clamp to them (`line/mod.rs:146-186`). The seam's rows get their own hit record — the
`PhantomHit { bounds, origin }` that landed in M2b is its first form — and the entered
block's rows extend it with the byte range *in the other buffer* that the record maps
between. `BlockLine.origin` already carries the address down to the paint; what is
missing is the painted-text-to-buffer-offset map for those rows, which
`embeds::paint_body` already builds internally (`PaintedRow`'s spans are rebased through
a `PieceMap` over the joined body) and simply does not return.

Two ordering facts the map settled, both easy to get wrong:
- **A click in a phantom band is not currently inert.** The y-scan in `input.rs:26-35`
  falls through to "the last line whose top is at or above y", so clicking an embed body
  today moves the host caret to the row above the embed and writes a host byte
  (`input.rs:74-75`). `origin_at` therefore has to be consulted *before* that scan, not
  as a fallback after it.
- **The caret cannot reach a phantom row by accident.** `cursor_col` is set only when
  `cursor_display == index` (`render.rs:600`), and `cursor_display` comes from the host
  buffer's display model. So "only the entered block paints a caret" (ruling 14) is not a
  rule to defend — it is what happens unless the entered block's rows are explicitly
  handed a `cursor_col`. The work is the affirmative half.
- **One input handler per frame, claimed by the first `LineElement` that paints**
  (`line/mod.rs:652-667`), bound to that element's bounds and to the editor entity. The
  entered block's rows are `LineElement`s too, so they must not claim it a second time —
  the flag is reset per frame at `render.rs:55`, and the first claimant is already the
  host's first line.

**M3a landed (entry, exit, the seam).** `ctrl-]` on a `![[…]]` line, and a click inside
a body, move the caret into the embedded note; `ctrl-o` walks out, taking the key before
the jump chain while an embed holds it. The status bar carries `in ![[gift]]` as a state
segment, and dirty follows the caret rather than the tab. The seam is enforced in one
place — the caret is clamped to the section after each keystroke — so `j`, `}`, `gg`, `G`
and a search's `n` all stop at the section's edges without any of them knowing an edge
exists. Five tests in `tests/embeds.rs`.

Two rulings this surfaced that the design pass had not reached:

- **Ruling 19: the view slot follows the caret, and the host's is parked.** The editor
  has one `slot` — display segments, folds, preview mode — and those are offsets into
  *one* buffer. Applying the host's segments to the embedded note makes `j` land on the
  wrong row and the ruler report nonsense. So entering swaps the editor's slot for the
  embedded note's (laid out one display line per row, which is how the block is painted)
  and parks the host's inside `Entered`, where the paint path reaches it through
  `host_slot()`. Three paint sites changed instead of the 25 caret sites.
- **Ruling 20: a note the caret has typed into does not go back to being a picture.**
  Notes are kept alive by the view showing them — the registry holds a `Weak`. An embed
  breaks that chain: the caret typed into a note no tab shows, then left, and the last
  strong reference went with it, taking the unsaved edits. The window now holds dirty
  embeds it has no tab for (`held_embeds`) and says so in the status line. Ruling 18's
  tab materialisation belongs on the save path, which is M4 — and M4 owns releasing the
  hold, because a written note can be let go.

**M3b landed (the block is painted from its own buffer).** An entered embed's rows are
`LineElement`s over the embedded note's buffer — the same element a note paints its own
rows with — so they carry the caret, the selection, markdown runs, and a click that lands
on the character under the pointer instead of the start of a row. A read-only block stays
a phantom row: the difference between the two is the caret, and the caret decides.

Two rulings this surfaced:

- **Ruling 21: the entered block's rows are hit records in a space of their own.** They go
  into `embed_hits`, not the host's `hit_lines`, because the host reads those records'
  `start..end` as bytes of *itself* — a foreign range would answer a click with a byte from
  the wrong note. `embed_offset_at` is the door back in.
- **Ruling 22: the scroll target for a caret inside a block is the host line the block
  hangs off.** The block's rows are not items the scroller knows; they live inside that
  line's wrapper. Asking with the caret's row in the *other* buffer scrolls to the host's
  line of the same number, which is a different line. The frame records the anchor, since
  it is the only thing that knows which host row a body hangs under — and it records it
  every frame, because an anchor that outlives its block is the same bug wearing a hat.

**M4 landed (the save story, and the command line).** The dot on a tab is transitive: a
note embedded in it that the caret has typed into is unsaved work in that tab. `:w` and
the autosave beat both write it, each note to its own file. The view keeps the notes it
has typed into (`touched`) — one list feeding the dot, the write path and the strong
reference that keeps them alive, which replaces ruling 20's window-level `held_embeds`.
`:rename` follows the caret (Q4), `:w <name>` refuses inside an embed and names the embed,
`:q` from inside one is a plain dirty-tab close (Q7), clicking the host's own text exits
(Q3), and a section that stops existing under the caret gives the caret back with a line
saying so (Q5). A dirty embed with no tab gains one (Q1), added without being shown: the
caret is in the middle of a word in the host, and the tab bar is reporting an edit rather
than asking the user to go somewhere.

- **Ruling 23: the transitive dot and the file's own dirty flag are different questions.**
  Rewriting links refuses to touch the open note, and a disk change re-reads a buffer,
  when *that file's* text is ahead of *that file* — an embed's unsaved edits say nothing
  about the file they are embedded in. `own_text_is_dirty` answers that one; the dot, the
  close paths and the quit check keep asking the transitive one. Getting these two mixed
  is how a rename started rewriting the wrong file.

**The save story is visible where the save story is.** An embedded note is not a tab, so
the tab's `●` has nowhere to light for it: the host's tab lights transitively, but it
cannot say which block on the screen the unwritten words are in. A dot now hangs in the
gutter on the first row of each note's run inside a block — per run, not per block,
because a nested embed's rows are a different note's text and the outer block's top row is
not where its unsaved words are.

That made four things wanting the same 16 px column, so the column got a vocabulary
instead of a fourth field: `GutterMark::Glyph` / `GutterMark::Bar` and one `paint_gutter`,
with the callout's kind glyph and a proposal's rule moved onto it. The column holds one
glyph and any number of rules, so a row's list is in precedence order and the caller
chooses it — a callout's kind glyph outranks the dirty dot, because the kind cannot be
carried by colour alone while unsaved work is still reported by the tab.

**The entered block is a note, not a picture of one.** A wiki-link inside the block being
edited is followed by `gd`, `gD` and ctrl-click, and coloured like a link; the spell and
grammar checker is handed the embedded note while the caret is inside it and the host back
when the caret leaves, and its notes underline bytes of whichever buffer it is holding.
Both are the `buffer` / `note.buffer` split applied one record type later: an offset is a
byte of one note or of the other, and answering it out of the wrong list is a silent
nothing (a `gd` that does nothing) or a squiggle on the wrong note.

- **Ruling: `/` does not search embedded text.** Search is buffer-scoped, and the seam is
  a buffer boundary — so a word on screen can be invisible to search, and that is the
  intended shape rather than an oversight.
- **Ruling: the checker follows the caret, not the tab.** It holds one document; the
  document it holds is the one being typed into.

- **Ruling: a paint record reads what was painted.** `HitLine`'s `callout_mark`,
  `gutter_dot` and `marked` are all derived from what the frame drew, because "the row was
  handed these ranges" and "the row drew these marks" stopped being the same claim the
  moment two buffers' records could reach one screen.

- **Ruling: a row number carried between the editor and the window names its line space.**
  `LineRef::Display` / `LineRef::Embed`. The two hit lists — `hit_lines` and `embed_hits` —
  both number from zero and share nothing else, so a bare number is not ambiguous in the
  code, only at the seam where one hands it to the other. Found as a freeze: `K` inside an
  entered embed anchored its panel on the host's line of the same number, and past the end
  of the host's display list the answer was "no such line" — so the panel was never
  rendered while still holding the keyboard, and no key in any context reached anything.
  Shallower in, the same bug was quieter: a panel over the wrong word.

- **Ruling: a panel that cannot be drawn does not keep the keyboard.** The render drops it
  and `drop_popup` falls back to the note on screen when it has no record of who held the
  focus. This is the belt, not the fix: it turns a mis-anchored panel from a frozen window
  into a missing one. The trap is general — anything that focuses an entity which may not
  be in the tree kills the whole window, not just that entity.

- **The block being edited reports the pointer.** Its rows hang below the host line's box,
  so no handler covered them and hovering a marked word inside the block said nothing. One
  handler per block, resolving the row from the bands this frame painted rather than from
  where the pointer is guessed to be.

**Still owed before P4.0b closes:** the entered block's rows do not soft-wrap, and a
folded heading's disclosure glyph is the one gutter tenant not yet on `GutterMark` — it is
a shaped glyph rather than a path and it records a click range, so it is painted where that
range is recorded.

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
