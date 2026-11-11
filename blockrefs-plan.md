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
- **M2 — schema v5.** `blocks(note, id, line)` + index; written in the note's record pass
  with per-note delete; `note_removed` clears it; `Db::block_exists(id)` and
  `Db::blocks_matching(want, limit)`; a v4 vault gains the table by rebuild (the v3→v4
  test is the template). Falsifications: no per-note delete → red; scan returns nothing →
  red; `line` off by one → red.
- **M3 — the resolver and the two doors.** `Anchor::{Heading, Block}` decided by the link's
  spelling; `block_rows`; embed draws the block through the existing phantom rows; refusal
  sentence `no block "^abc" in <note>`; hover card follows; `reveal_block` in the editor and
  the anchor-kind branch in `reveal_in_open_note` (buffer-first, per ruling 53).
- **M4 — the two verbs.** `:block-id` (grouped edit, collision-checked, refuses a second
  id) and `:blocks` (vault-wide, riding `:sections`).
- **M5 — docs and the demo vault.** `docs/notes.md` prose + the `:block-id`/`:blocks`
  palette rows (regenerate `docs/commands.md` unfiltered), a block ref in the demo tour,
  roadmap/brainstorm writeback in the same commit.

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
