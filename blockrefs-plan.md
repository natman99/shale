# P4.9 Block references — plan

`^abc123` gives a note a **named part**: a paragraph, a bullet, a quote, a table.
`[[note#^abc123]]` jumps to it, `![[note#^abc123]]` puts it in this note, and a
hover card over the link shows that block and nothing else.

Status: **plan.** Ruled in brainstorm (2026-10-29, ✅), never had code. This file is
the design pass the roadmap item owes. §1 is what is already settled and will not be
re-litigated; §3 is what this pass needs the user to decide.

---

## 1. Already ruled — not re-litigating

From `brainstorm.md` §"Block references ✅" (lines 736–752), verbatim in substance:

- `^abc123` at the end of a paragraph/bullet; `![[note#^abc123]]` embeds it;
  `[[note#^abc123]]` links to it and the jump scrolls to it.
- **ID generation is random, not derived**: a command appends `^` + 6 random chars,
  lowercase base32 (`a-z2-7`), and the create command checks the index for a collision.
  Content hashes die on every edit; counters collide on merge and agent writes;
  word-slugs go stale on reword. Stable once written.
- `mdstyle` gains a block-anchor span; the transclusion resolver grows from
  note/section to **block granularity**; hover preview on a block ref shows just that block.
- **Graph edges stay note-level** (Obsidian's rule). Block refs enrich the backlinks
  surface with the exact block — that surface is later work, not this item.
- Format-compatible with Obsidian: notes authored there just work.

Downstream items that wait on this (do not build them here):

- **Canvas C3 block cards** (`canvas-plan.md`, P6.10) — a card that shows one block.
- **Cue chips' `for: ^block-ref` override** (GM toolkit, `brainstorm.md:1017`).
- **P6.2**'s block case; the backlinks panel's "which block linked me".

## 2. What the survey found (measured, not assumed)

The anchor machinery is already shaped for this. Four doors exist and all four are
one branch wide from answering about a block:

- `transclude::line_embed` (`crates/core/src/transclude.rs:59`) splits a link at the
  first `#` into target and anchor. `[[note#^abc]]` already arrives with
  `anchor = Some("^abc")` — nothing today understands it, so it falls through to the
  heading matcher and reports `no section "^abc" in note`.
- `transclude::section_rows` / `heading_row` (`transclude.rs:411`, `:443`) are the
  section half, and since `e34d571` both read the one heading scan
  (`mdstyle::object::heading_lines`).
- The jump door: `Workspace::reveal_in_open_note` (`crates/app/src/workspace/notes.rs:377`)
  → `MarkdownEditor::reveal_heading` (`crates/app/src/editor/mod.rs:1388`), and its
  index-fed sibling `reveal_section` (`:1409`) that *checks* a line against the buffer
  instead of believing it (headings-plan ruling 53).
- The hover card: `MarkdownEditor::preview_for` (`:2242`) builds an `EmbedRef` from the
  link and calls `transclude::resolve_from` — so the card shows exactly what an embed
  of the same link would show. Teach the resolver about blocks and the card follows for
  free, title included (`name › ^abc`).
- Embeds: `embeds::paint_body` + the phantom-row machinery from P4.0/P4.3 already draw
  a cut-out range of another note. A block is just a smaller range.

What does **not** exist, priced honestly:

- No block-id storage. `db/schema.rs` has `notes/tags/links/events/marks/edges/props/
  headings`; nothing holds `^ids`. (`marks` is a named-position table with no writer
  anywhere in `crates/` — an empty shell, not a home for this.)
- No completion machinery at all: nothing suggests inside `[[`. Obsidian's
  `^`-triggered block picker and its vault-wide `[[^^block]]` search have no host to
  plug into, so "type `^` and get suggestions" is a separate feature, not a flag.
- No backlinks panel in the app (only `GraphIndex::backlinks_of` and the MCP `backlinks`
  tool). "Backlinks show the exact block" has no surface yet.
- The paragraph text object (`Buffer::paragraph_object`, `crates/core/src/buffer/words.rs:98`)
  is **blank-line** based, not markdown-aware: a list item is part of its list. So the
  existing `ip`/`ap` is *not* the block unit and must not be quietly reused as one.

Obsidian's own rules, read from `refs/obsidian/Internal links.md:96–139` (spec material,
not copied): a block is a paragraph, a blockquote, or a list item; a paragraph's id goes
at the end of its last line after a space; a **structured** block's (list, quote, callout,
table) id goes on its own line with blank lines around it; a list item's id may sit on the
item; **links into specific lines of quotes, callouts and tables are not supported** (the
whole block is the atom); ids are Latin letters, numbers and dashes only; `[[^^block]]`
searches the vault. `[^id]` is a footnote, a separate syntax — a collision hazard to design
around.

## 3. Rulings — ALL RESOLVED (2026-11-11)

The user answered all eleven in one reply, taking the recommendation on every one.
The options stay below as the record of what was on the table.

| # | Ruling |
|---|---|
| 1 | **Markdown-aware blocks**, Obsidian's set: paragraph, one list item (with its nested children), whole quote/callout/table, code fence, heading line. One scan in `mdstyle`. |
| 2 | **Obsidian's grammar exactly**: `[A-Za-z0-9-]+`, at the end of the block's last line after whitespace, or alone on its own line with a blank above; case-sensitive. Generated ids stay 6 chars of `a-z2-7`. |
| 3 | **Index it**: schema v5 `blocks(note, id, line)`, `line` = the block's first line, extent re-derived from the text. |
| 4 | An id on a heading names **the heading line**, not its section. |
| 5 | **Whole quote/callout/table** is the atom; a list item includes its sub-list. |
| 6 | **Never hide the anchor** — `dim`, not a marker. Plus the user's addition: *keep the recursion cap*. The standing embed cap (`transclude::MAX_DEPTH = 3`, 1 editable + 2 read-only) bounds block embeds too, and M3 pins it with a test rather than trusting the note in this file. |
| 7 | `:block-id` in the palette, no chord, one grouped edit, refuses a second id on a block. |
| 8 | **Yes to `:blocks`**, riding `:sections`. |
| 9 | Duplicate ids: both exist, resolver tie-breaks shortest path then alphabetical, `:blocks` shows both. |
| 10 | **`ib`/`ab` block text object**, free on top of the scan. |
| 11 | `[[note#Heading#^id]]` is **refused** this phase and documented as refused. |

Eleven questions, each with the recommendation first. Answer by number.

**A. What is a block?**
  1. *(rec)* **Markdown-aware, Obsidian's supported set**: paragraph (all its soft-wrapped
     lines), one list item (with its nested children), one whole blockquote, one whole
     callout, one whole table, one fenced code block, one heading line, one image/embed
     line. One scan in `mdstyle`, the same authority ruling 46 gave headings.
  2. Blank-line paragraphs only (reuse `paragraph_object`). Cheaper, and it breaks
     interop: a note authored in Obsidian with an id on a bullet would not resolve here.
  Cost of (1): a new scan (~150 lines) that must agree with the parser's fence, quote and
  table rules. It is also the scan canvas C3 and the cue chip will read later, so it is
  not single-use.

**B. Anchor grammar and placement.**
  *(rec)* Match Obsidian exactly: token `[A-Za-z0-9-]+`; recognized **only** at the end of
  the block's last line with whitespace before it, or on its own line after the block
  (the structured form); case-sensitive comparison. Generated ids stay 6 chars of `a-z2-7`.
  Consequences worth naming: `2^10` is never an anchor (no whitespace before the caret),
  `[^id]` is never one (caret preceded by `[`), `^foo_bar` is never one (underscore), and
  an id typed in the *middle* of a paragraph is ordinary text, not an anchor.

**C. Index it or scan for it?**
  1. *(rec)* **Schema v5: `blocks(note, id, line)`** + an index on `id`, written in the
     note's own indexing pass with delete-then-insert, exactly like `headings` in v4.
     `line` is the block's **first** line — the extent is re-derived from the text by the
     same scan, per ruling 47 (store what a reader types, recompute what a parser knows).
     Needed by: the create command's collision check (the brainstorm's own promise), a
     vault-wide `:blocks` list, and any `.shq` over the vault's ids.
  2. No table; scan the vault on demand. Saves a migration; costs the collision check a
     full-text walk and leaves `.shq` with nothing to query.
  This is the schema decision that is expensive to retrofit — price it now.

**D. An id on a heading line.**
  *(rec)* Allowed, and it means **the heading line alone**, not its section. `#Heading`
  already means the section; `#^abc` on a heading is the line. Two anchor kinds, two
  meanings, no overlap.

**E. Granularity inside structured blocks.**
  *(rec)* Obsidian's rule: the **whole** quote/callout/table is the atom — no ids on
  individual rows or quoted lines. A list item *does* include its nested children (an id
  on a parent bullet addresses the item and its sub-list). Interop again, and it keeps
  the scan honest: it never has to decide whether a table row is a block.

**F. Does live preview hide the `^abc123`?**
  1. *(rec)* **No.** Style it `dim` (an existing role — no new palette field), and keep it
     out of `mdlayout::is_marker` so it is never hidden. This is ruling 17's line — *hide
     what computes, never hide what asserts* — and an anchor asserts an identity the reader
     must be able to see and edit. Obsidian hides it in preview; that divergence is
     deliberate and gets a test.
  2. Hide it like a `#` marker. Reads cleaner, hides a claim.

**G. The create command's name, and any key.**
  *(rec)* `:block-id` in the palette ("Add a block id to the block under the caret"), no
  key chord — chords are rationed and this is a rare act. One grouped buffer edit, so one
  `u` takes it away (the `add_properties` shape). If the block already has an id, the
  caret moves to it and the status line says so; never a second id on one block.

**H. A vault-wide `:blocks` picker?**
  1. *(rec)* **Yes**, riding the `:sections` shape from P4.3b M3 (same picker, same
     `PickerTarget` pattern, `Db::blocks_matching`). It is Obsidian's `[[^^block]]`
   search, and it is the surface that makes the v5 table obviously worth its migration.
  2. Defer it; the table then serves only the collision check.

**I. Two notes, same id.**
  *(rec)* Both exist (the vault is the user's), the resolver takes **shortest path, then
  alphabetical** — the tie-break aliases and stems already use — and `:blocks` lists both
  so the clash is visible. The create command avoids *creating* a clash by asking the index.

**J. A block text object (`ib` / `ab`)?**
  1. *(rec)* **Yes**, once the scan exists it is ~free, and it makes the block unit
     something the reader can act on (`dab`, `yab`, `cab`) rather than only link to. `b`
     is unused in operator-pending position today.
  2. Skip it; the scan stays link-only.

**K. `[[note#Heading#^id]]` (a block under a heading).**
  *(rec)* **Refuse this phase**, in the note branch's own words, and write the refusal in
  the docs. Obsidian accepts the chained form; nothing in Shale's plan needs it yet, and
  half-supporting it means two anchor parsers.

## 4. Edge-case ledger

Every row is a case a test or a refusal must answer. Free rides are marked.

- **EDGE-1 — `2^10` in prose.** Not an anchor: the caret is not preceded by whitespace.
- **EDGE-2 — `[^id]` footnote.** Not an anchor: caret preceded by `[`. Footnotes keep
  working untouched.
- **EDGE-3 — `^abc` inside a fenced code block or a code span.** Not an anchor. The
  parser's fence rules (three-or-more, closer matches opener) decide, same as `e34d571`
  made them for headings.
- **EDGE-4 — `^abc` inside a properties block.** Not an anchor; it is a value. A property
  whose text happens to end in `^abc` files no block.
- **EDGE-5 — id mid-paragraph.** Ordinary text. An anchor must sit at the end of the
  block's last line (or on its own line after a structured block).
- **EDGE-6 — multi-line (soft-wrapped) paragraph.** One block; the id goes on the last
  line; the block's rows are all of them.
- **EDGE-7 — list item with a nested sub-list.** The item's block includes its children;
  an id on the parent addresses the whole item. An id on a child addresses the child.
- **EDGE-8 — table.** Whole table only (E). An id after a table row is ordinary text
  unless it stands on its own line after the table.
- **EDGE-9 — blockquote / callout.** Whole block only. A callout's `>` markers stay part
  of the block's rows when embedded.
- **EDGE-10 — two ids on one block.** Both address it; the census lists both; the create
  command refuses to add a second.
- **EDGE-11 — same id in two notes.** Resolver tie-break (I); `:blocks` shows both.
- **EDGE-12 — the id the reader just typed (unsaved).** The buffer answers for the open
  note, not the index — the `OpenFirst` / `reveal_section` rule (ruling 53). A jump and an
  embed must both work before the save.
- **EDGE-13 — the index has not read the note yet.** The jump still works (buffer path);
  `:blocks` and the collision check say what they read, never invent.
- **EDGE-14 — `[[note#^]]` (empty id).** Names nothing — the EDGE-15 rule from
  headings-plan. Refuse, don't match the first idless block.
- **EDGE-15 — `[[#^abc]]` (no target).** The current note. Rides the existing
  empty-target branch in `notes.rs:350`.
- **EDGE-16 — `![[query.shq#^abc]]`.** A program has no blocks. Refuse in the query
  branch's own words, the cousin of ruling 27's `no section "hp" in tough`.
- **EDGE-17 — a block embed whose block contains an embed.** Expands as any embed does,
  inside the standing depth cap (1 editable + 2 read-only).
- **EDGE-18 — a block inside a folded section.** Fold state belongs to the view; the
  resolver reads text. Embedding a folded block works; the fold does not hide it from the
  index.
- **EDGE-19 — note renamed.** `[[note#^abc]]` retargets through the existing rename
  rewrite (the target is resolved before the `#`). Needs a test, not new code — and if the
  test says otherwise, that is a bug in the rewrite, not a block feature.
- **EDGE-20 — note deleted.** `blocks` rows go with the note's record (per-note
  delete-then-insert), and the graph already degrades the link to a ghost.
- **EDGE-21 — undo.** `:block-id` is one grouped edit: one `u` removes ` ^abc123`, and the
  caret lands where the reader can keep typing.
- **EDGE-22 — live preview.** The anchor paints dim and is never in `hidden_ranges`; the
  caret can select and edit it. Falsify by adding `BlockAnchor` to `is_marker` and watching
  the test go red.
- **EDGE-23 — a heading whose text starts with `^`.** The anchor *kind* is decided by the
  link's spelling, never by what the note happens to contain: `#^foo` always asks for block
  id `foo`. So a heading written `## ^foo` has no anchor spelling that reaches it — the
  caret position is claimed by the block form. The resolver never guesses, and the docs say
  so in one sentence.
- **EDGE-24 — perf.** The scan runs per note in the indexing pass, next to the heading
  scan. Two passes until the 5k fixture says otherwise (ruling 51). Measure before fusing.
- **EDGE-25 — `blocks` and `headings` cleanup symmetry.** One note removal empties both;
  a test names both tables.
- **EDGE-26 — a block id that is also a heading's text.** No interaction: different anchor
  kinds, different tables.
- **EDGE-27 — a heading that carries an id** (`## The Fall ^fall`). The id is not part of
  the heading's name: `#The Fall` still lands, because the anchor matcher cuts a trailing
  id off the title before comparing. Not cutting it would make adding an id silently break
  every heading link to that heading. A heading *named* `^bare` keeps its caret — the
  documented consequence of EDGE-23 (the spelling decides, never the content).
- **EDGE-28 — an embed line that carries its own id** (`![[a]] ^x`). Still an embed: the id
  is cut before the line's shape is checked. `line_embed` used to demand the line be *only*
  the embed, so labelling a working transclusion would have silently un-embedded it. The id
  names the embed line as a block, which is what a `#^x` link elsewhere reaches.
- **EDGE-29 — a command's palette reachability is its label, not its name.** `:blocks`
  labelled "Jump to a block" cannot be found by typing `blocks`: the matcher needs an `s`
  after the `block`, and the row that answers instead is typewriter mode's "Dim everything
  but the block the cursor is in". The label now carries the noun the command takes. The
  check is a palette test that types the command's own word and asserts membership, not
  ranking — the same contract `:sections` keeps.

## 5. Free rides (say so, don't spend rulings)

Given A–E and the v5 table:

- `.shq` over the vault's ids: `SELECT note, line FROM blocks WHERE id = 'x'` — the
  read-only door already answers any table.
- Hover card showing just the block — falls out of `preview_for` + the resolver branch.
- `:blocks` picker (H) riding `:sections`.
- `ib`/`ab` text object (J) riding the scan.
- Canvas C3 and the cue chip's `for:` later read the same scan and table — no new storage.

## 6. Milestones

Split by dependency; each closes with the workspace gate green and its falsifications run.

- **M1 — the scan and the span (core only). CLOSED (2026-11-11).**
  `mdstyle::blockmap::block_map(text) -> BlockMap { blocks, anchors }` — `Block` carries
  kind, first/last line, byte range and indent; `BlockAnchor` carries the id, the token's
  byte range, its line, and which block it names. `Style::BlockAnchor` is painted `dim` and
  kept out of `mdlayout::is_marker`. No schema, no resolver.
  Two deviations from the wording above, both deliberate: the scan lives in its own module,
  `mdstyle/blockmap.rs` (with the span scan in one file, `block.rs` would have passed the
  repo's 1000-line refactor line), and it returns one map rather than two functions, because
  an anchor means nothing except next to the block it attaches to.
  15 tests. Six falsifications, each turning at least one red: the whitespace-before-the-caret
  rule, the deepest-block-wins rule, "the block must end on the anchor's line", the blank line
  above an own-line id, structured blocks taking only the own-line form, and the marker set.
  Two guards were **deleted** rather than kept, because breaking them showed they decided
  nothing once the block-kind rule and the walk's own skipping were in place: marking fence
  lines as code, and a properties-block check inside the anchor pass.
- **M2 — schema v5. CLOSED (2026-11-11).** `SCHEMA_VERSION` is 5: `blocks(note, id, line)`
  with `blocks_id` (the collision check, the "where is this id" question) and `blocks_note`
  (the per-note delete-then-insert). Written in the note's record pass, cleared by
  `note_removed`, and `SCHEMA_V4` is `#[cfg(test)]` so a test can hand Shale a v4 index and
  watch it rebuild — the v3→v4 test is the template, and it passes.
  Five deviations from the wording above, each deliberate:
  1. **No primary key.** The plan wrote `blocks(note, id, line)` as though `(note, id)` were
     a key. It is not: the same id can name two blocks in one note (a typo, or two blocks
     the author meant alike), and the same id can live in two notes (ruling 9). A key would
     have made the second insert an error — a failed index over a note the author legally
     wrote. Both rows exist; the resolver's tie-break decides which one a link reaches.
  2. **`line` is the block's first line**, not the line the token sits on. A jump wants the
     top of the thing it names, and an id written on its own line below a table would
     otherwise land the reader *under* it. The extent is not stored: the scan that found the
     id can produce the range again (ruling 47's rule, applied to blocks).
  3. The readers are `Db::blocks_matching(want, limit)` and `Db::block_id_taken(id)`, not
     `block_exists`. The check is **exact**, not a prefix: `ab12` is not `ab12cd`, and a
     false "taken" would make `:block-id` redraw an id it could have used.
  4. The writer calls `mdstyle::block_map` rather than matching a caret token itself, so the
     index cannot hold an id the editor would decline — inside a fence, inside a properties
     block, or mid-paragraph file no row.
  5. A sort in the row builder was **deleted**: the scan walks the note line by line, so its
     anchors are already in line order, and every reader orders by `line` anyway.
  Also fixed on the way: `db/tests.rs`'s table enumeration had been missing `headings` since
  v4 landed — v5 adds both it and `blocks`, so the list is nine.
  8 tests, and a saved query grouping the vault by id proves the payoff of ruling 3: the
  table is reachable from `.shq` through the same `Db::run` path, with no second answer to
  keep in step.
  Falsifications, each turning at least one test red: store the anchor's own line; drop the
  per-note delete; drop the delete on removal; make the collision check a prefix match; stop
  quoting the reader's wildcards in the blocks reader; replace the block map with a naive
  caret-token scan (two tests red). The first round had two **green** mutations, and both
  were real findings: `id LIKE ?1` is `id = ?1` when the pattern has no wildcards, so the
  "prefix" mutation changed nothing until it became `LIKE ?1 || '%'`; and the sort was dead
  code, which is why it is gone rather than tested.
- **M3 — the resolver and the two doors. CLOSED (2026-11-11).** `Anchor::{Heading, Block}`
  is decided by the link's spelling (`#^x` block, `#x` section, `#^` an empty block ask,
  bare `#` no anchor at all); `transclude::block_rows` cuts the block the id names; the
  embed draws it through the existing phantom rows; the refusal sentence is
  `no block "^abc" in …`, the same words the embed badge uses; the hover card follows for
  free; `MarkdownEditor::reveal_block` plus the anchor-kind branch in `reveal_in_open_note`
  (buffer-first, per ruling 53, so an id typed a second ago jumps before it is saved). The
  recursion cap is **pinned by test**: a chain of block embeds stops at `MAX_DEPTH` like
  every other embed, and a block that embeds its own note is cut by the chain at the second
  mention rather than spending the budget on one note.
  Six deviations from the wording above, each deliberate:
  1. `Anchor` lives in `markdown.rs`, not `transclude.rs`. `WikiLink.anchor` and
     `EmbedRef.anchor` are the same question and `split_anchor` is one splitter; two types
     would let `gd` and `![[…]]` drift apart on what `#^x` means.
  2. The embed cache key now holds the anchor **with its kind**. `![[a#^b]]` and `![[a#b]]`
     are different bodies; a `String` key hands one embed the other's rows.
  3. EDGE-27 (new row): `mdstyle::without_block_anchor` is applied in `find_heading`, so
     `## The Fall ^fall` still answers `#The Fall`.
  4. EDGE-28 (new row): **an embed line can carry its own id.** Found by the cap fixture,
     not by a ruling — `![[a]] ^x` was not an embed at all, so adding an id silently
     un-embedded a working transclusion. `line_embed` cuts a trailing id first.
  5. The chained `[[note#H#^id]]` (ruling 11: refuse) is refused by **reading it literally**
     — the anchor reads as a heading named `H#^id`, which no note has — rather than by an
     error path. One code path, and the sentence tells the reader what Shale thought they
     wrote.
  6. The app's missing-block sentence names the note by **path** (`gift.md`) and the badge
     by **stem** (`gift`), the same split the heading surfaces already have
     (`no heading “X” in gift.md` vs `no section "X" in gift`). The block wording itself is
     identical in both.
  11 new tests (7 core, 4 app). Falsifications, each turning at least one red: `list_owner`
  returning `None` (three red: EDGE-7's parent bullet, the own-line id inside a list item,
  the sub-list embed); `block_rows` never answering in the resolver; the heading strip
  turned off (core and app); the cache key flattened to the anchor's text; the card title
  losing its caret; `reveal_block` swapped for `reveal_heading`; the id cut removed from
  `line_embed`. **Two came back green at first and both were fixture faults, not code
  faults**: the cache-key test used `^atom` against `#Atom`, and two different strings never
  collide however flat the key; and the heading-jump test put the heading on the note's
  first line, where a *failed* jump leaves the caret too. A falsification is only as good as
  the fixture's ability to tell the failure from the success. A third green was a mutation
  that never applied — the mutation script now asserts its search string matched.
- **M4 — the two verbs. CLOSED (2026-11-11).** `:block-id [id]` (`note.block-id`,
  `AddBlockId`, no chord per ruling 7) and `:blocks [pattern]` (`vault.blocks`, `Blocks`,
  riding `:sections`). The labour is split the way the two questions are: the **window**
  owns the vault-wide collision check (`Db::block_id_taken`) and the sentences; the
  **editor** (`editor/blockid.rs`) owns the block under the caret and where an id for it
  would land. The outcome is a closed enum — `BlockId::{Inserted, HasId, NoBlock, BadId,
  Taken, NoRoom, ReadOnly}` — so every refusal has its own words rather than one failure
  with a reason bolted on. Core gained the writer's side of the same scan: `is_id_byte` is
  the **one** statement of the id byte set (ruling 2's grammar shared by `anchor_token` and
  `valid_block_id` — two statements of a grammar drift), `new_block_id` draws 6 chars of
  `a-z2-7` and retries while the vault or the buffer holds the draw, `block_at` finds the
  innermost block under a byte (`list_owner`'s tie-break, so a caret inside a nested item
  names the item and not its parent), and `with_block_id` returns a `BlockIdWrite { at,
  insert, caret }`.
  Four things the wording above did not know:
  1. **The write is checked by the scan, not by placement arithmetic.** Each candidate
     spelling for the block's kind is applied to a copy and re-read by `block_map`; the
     first whose read-back names *this* block wins. A placement guess the scan reads as a
     different block would hand the author a link that lies.
  2. **One insert, so one `u`** (EDGE-21): a structured block's id arrives with its blank
     line in the same edit, and the table test takes both back at once.
  3. EDGE-29 (new row): **a command's palette reachability is its label, not its name.**
  4. `:blocks` is two halves like `:sections`: no pattern reads the **buffer** (an id typed
     a second ago is listed before it is saved, EDGE-12), a pattern reads `blocks` (notes
     nobody has open answer too). No index with a pattern says there is no vault index to
     read; no index with no pattern still answers (EDGE-13).
  A note with no ids is a row, not an empty pane — the row names the note and points at
  `:block-id` — and a caret on a blank line idles with a sentence about where the caret
  should be rather than iding the block above by surprise. Rows label with the block's
  first line and detail with `^id`, the kind, and the line a reader counts from one.
  21 new tests (7 core, 14 app). Falsifications, each turning at least one red: the grammar
  validator accepting anything; the vault-wide check removed; the already-has-an-id read
  removed; the buffer path never taken; a row that opens without revealing; the own-line
  spelling dropped; the glued spelling dropped. **One came back green and was a harness
  fault**: `if !pattern.is_empty() {` appears twice in `index.rs` — the `sections` branch
  and the `blocks` branch — and the mutation replaced the first. A mutation must anchor on
  text unique to the branch it means, not on the first match.
- **M5 — docs and the demo vault. CLOSED (2026-11-11).** `docs/notes.md` prose (the link
  grammar, the embed's missing-block sentence, `:blocks` in Finding things), the
  `:block-id`/`:blocks` palette rows in `docs/commands.md` (regenerated unfiltered), and a
  real block ref in the demo tour: `Welcome.md`'s first paragraph is `^start`, linked and
  embedded from `Markdown Tour.md`. The demo content was checked through the real resolver
  rather than trusted — the id reads as a Paragraph anchor, the embed line parses as
  `Block("start")`, and the block's rows are the paragraph's two lines.
- **M6 — the block text object (ruling 10). CLOSED (2026-11-11).** `i^` / `a^`, **not** the
  `ib` / `ab` the ruling named: `Object::from_char` already answers `b` and `B` as vim's
  historical letters for parens and braces, so the ruling's letters would have shadowed a
  motion the hand already knows for an object nobody has typed yet. `^` is the mark a block
  id is written with, so the object and the anchor are named by one character and are one
  atom — pinned by a test that asserts the object's range *is* the block the anchor names,
  both read from the same scan. `inner` is the block as written, indent and markers
  included: `di^` on a bullet takes the bullet, because half an item is not an item. `a^`
  adds the block's line break the way `ap` adds a paragraph's gap. A caret on a blank line
  cancels the operator rather than iding the block above by surprise. 7 core tests and 2
  keystroke cases (`di^`, `da^`) — the keystroke cases matter because `^` is also a vim
  motion, and the scope letter had to be shown reaching a pending operator rather than
  becoming one.

## 7. Not here

- `^`-triggered completion inside `[[` — no completion machinery exists; separate item.
- Backlinks showing the linked-against block — no backlinks surface exists yet.
- Canvas C3 block cards, cue-chip `for:`, P6.2's block case.
- Block-level graph edges (ruled out: edges stay note-level).
- `links.anchor` column — ruling 52 stands; a block ref files the same note-level edge.

## 8. Hooks owed (same commit as the first implementation commit)

- `roadmap.md` P4.9: point at this file, split into M1–M5, note the v5 schema bump in the
  phase's schema line.
- `brainstorm.md` §Block references: append the rulings from §3 (A–K) with dates.
- `references/index-and-schema.md` (skill): the `blocks` table and its writer rules.
