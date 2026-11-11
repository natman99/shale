# P4.3b — the headings index (schema v4)

Roadmap P4.3b, ruled 2026-11-08 and deliberately sequenced **after** P4.3a rather than
folded into it. Schema v4 adds `headings(note, line, text, depth)`, filled in the same
indexer pass that fills `tags`, `links` and `props`. It is derived, not authored, so it
cannot lie, and it is dropped and rebuilt like the rest of the index.

What it is for: a **query** grouping things by the section they sit in (cue and counter
chips, P7.10; scene cards), the outline pane (P4.13) reading notes it has never opened,
block cards (Canvas C3), `#heading` anchors, and block refs (P4.9). The alternative was
already refused in the same round — stamping a heading anchor into a chip's text puts
derived data in the buffer and turns a heading rename into a rewrite hazard.

## Already binding — not re-litigated

- **The DB is a cache.** `db/schema.rs`: an index at an older `user_version` is dropped and
  refilled from the notes; only a *newer* one is refused. v4 therefore means a new
  `SCHEMA_V4` constant and `SCHEMA_VERSION = 4`, not an `ALTER TABLE`.
- **Derived, not authored** (roadmap 1515, brainstorm 1058). Nothing in the buffer says
  "this is a section"; the index reads what the parser already reads.
- **One pass, one owner.** The rows are written by the per-note indexer (`db/index.rs::
  record` → `write_record`) and cleaned by `drop_note`. No separate walk, no second writer.
- **P4.3a is closed and stays closed.** This is its own schema bump.

## What the survey found

Three scanners answer "what headings does this note have", and they do not agree:

| Scanner | Used by | Rule |
|---|---|---|
| `transclude::heading_level` / `heading_text` / `heading_row` / `section_rows` | `gd` on `[[note#Section]]`, `![[note#Section]]` embeds, `editor::reveal_heading` | line-based; `#`s then a **space**; a `#` in a fenced block is skipped by a hand-rolled fence toggle; a `>` prefix means "not a heading" |
| `mdstyle::object::headings` | the `da#` text object, `section_range` | the parser's spans; `#`s then space **or tab**, or nothing at all (`#` alone is a heading); a heading inside a blockquote is styled |
| `fold::heading_rows` | section folding, `dim.rs` | the same spans, over row starts |

So `#` alone, `#\tText`, and `> ## Title` are sections to the fold and the text object and
are not sections to an anchor. The index has to pick one, and picking one is the item's
real work — the table itself is four columns and a delete.

## Rulings

46. **One heading scanner, and it is the parser's.** `mdstyle` is the authority: it is what
    paints the heading, what the fold folds, and what `da#` selects. `transclude`'s
    line-based scanner is deleted and `heading_row`/`section_rows` are re-pointed at the
    span scan, so the index, the fold, the outline, the text object and `gd` cannot give
    five answers to one written anchor. The anchor *matching* rule (trim, case-insensitive,
    first match wins) stays in `transclude`; only the "is this line a heading, and what is
    its text" half moves.

47. **`text` is what the file holds.** The row carries the heading's text after the markers,
    trimmed at both ends, inline markdown intact: `The *Fall* of Zhent`, not `The Fall`.
    Concealment is a view decision and the index stores the file's claim — the same reason
    a properties block keeps its quotes and brackets. Anchor matching stays
    case-insensitive against that text, so `[[note#The Fall]]` does not name
    `## The *Fall*` — which is what it does today, and a change there is question B, not a
    side effect of this table.

48. **`line` is the 0-based line index** — the basis `events.line_start` uses and
    `heading_row` returns, so a row can drive a reveal with no translation. The column
    comment says so, because an off-by-one here is invisible until someone jumps to the
    wrong section.

49. **`depth` is the marker count, 1 to 6.** Seven hashes is prose in both scanners today
    and stays prose.

50. **Written in the note's own pass, dropped with the note.** Delete-then-insert per note
    like `tags` and `props`; `drop_note` cleans it; a rename is a drop plus a record. No
    incremental heading edit — the note is the unit.

51. **Markdown notes only.** A `.shq` has no sections (its `#` lines are SQL comments —
    ruling 27's reasoning, and the reason `![[query#anchor]]` is refused), and an
    attachment has no text to scan.

52. **The index is for notes that are not open.** An unsaved heading is not in the index, so
    every surface that reveals a heading *in the note under the reader* keeps reading the
    buffer (`reveal_heading` does). The index answers about the vault. This goes in the
    schema doc, because it is the first thing a consumer will get wrong.

## Edge ledger

- **EDGE-1 — a `#` inside a fenced block is not a heading.** Both scanners already agree;
  the span scanner gets it for free (it never styles a fenced line as a heading), and the
  re-pointed matcher inherits it. The parser's fence rules turn out to be **stricter** than
  the toggle the matcher deleted: `mdstyle` needs three or more of one character to open, and
  its closer must be the same character and at least as long, while `transclude::fence`
  toggled on any line starting with ` ``` ` or `~~~`. A note that mixed the two styles used
  to have its headings hidden past a stray tilde; now it does not, and both the fold and the
  painter have been holding this rule all along. Pinned in M2.
- **EDGE-2 — `#` alone, and `#\tText`.** **Closed by M2.** Checked against the code, and the
  parser really does differ here: `mdstyle` styles both (a bare `#` as a heading with empty
  text, a tab after the markers as whitespace), while `transclude::heading_level` insisted on
  a space and refused both. The fold was not the odd one out — it walked the style spans
  itself, with its own merge rule and its own line search, so it already agreed with the
  painter; that made it a *fourth* copy of the same walk rather than a disagreeing scanner.
  Now all three read `mdstyle::object::heading_lines`, and the change a reader could notice
  is that `[[note#Tabbed]]` jumps to `#\tTabbed` and that heading folds.
- **EDGE-3 — `> ## Title` inside a blockquote.** The survey guessed the parser styled it. It
  does not: `mdstyle` styles no heading inside a blockquote, and `heading_level` refuses the
  line for its own reason (it wants a `#` at the line's start). Both say *not a section*, so
  the index writes no row and the ruling is only that this stays true — quoting somebody's
  heading is quoting, not sectioning. Pinned by a test anyway: a survey claim that turns out
  backwards is worth more than one that was never checked.
- **EDGE-4 — four-space-indented `#`.** CommonMark calls that a code block; both Shale
  scanners trim and call it a heading. Out of scope here and named as a known divergence:
  the fold and the outline already treat it as a heading, so the index agreeing with them
  is the honest answer. Fixing it is a parser change with fold consequences, not a schema
  one.
- **EDGE-5 — setext headings (`Title` then `===`).** Parsed by nothing in Shale today: not
  styled, not folded, not an anchor target. Out of scope, and the tell is written down —
  *a setext underline is not a section boundary in Shale*. Adding it is a parser item.
- **EDGE-6 — duplicate headings in one note.** Several rows, in line order; the primary key
  is `(note, line)`, not `(note, text)`. The matcher keeps "first wins", which is what
  `heading_row` does today.
- **EDGE-7 — closing hashes (`## Title ##`).** The parser's heading-text span runs to the
  end of the line, so `text` is `Title ##` and an anchor has to be written with them. Same
  as today. Question B asks whether to normalize.
- **EDGE-8 — a `#` inside a properties block.** The block's lines are fences and `key:`
  pairs; the parser styles no heading there. A `#` in a *value* is text.
- **EDGE-9 — a heading inside an embedded body.** The index knows the file that holds the
  heading, not the note embedding it. `![[note#anchor]]` resolves against the file, so the
  row and the embed agree; a query that wants "sections of the note I am in" is a relative
  query (ruling 36) over this table.
- **EDGE-10 — a dirty buffer.** Ruling 52. The symptom of getting this wrong is a jump to a
  stale section after an edit that moved a heading.
- **EDGE-11 — renaming a heading.** The rows follow on the next save. What does **not**
  follow is the `[[other#Old Heading]]` links that named it — that is P4.6's report, and it
  needs the anchor stored on the `links` row (question I). Deliberately not in v4.
- **EDGE-12 — a deleted note.** `drop_note` removes its rows, and the rows of nothing else:
  a heading is not a link target, so no other table points at it.
- **EDGE-13 — a renamed note.** Drop plus record; the rows carry the new path.
- **EDGE-14 — cost.** `record()` already pays one `style_spans` pass for `extract_tags`.
  Headings are a second pass over the same text unless the spans are computed once and fed
  to both. Measure on the 5k fixture before restructuring; the indexer is a background
  walk, not a frame budget.
- **EDGE-15 — an empty heading, and an anchor with no text.** A bare `#` is a section and the
  index lists it with `text = ''`: the census says what the note has. An anchor naming the
  empty string (`[[note#]]`) matches **nothing** and says so. The plan's first draft had the
  empty anchor finding the first nameless heading; that is reversed here on purpose — an empty
  anchor is a typo rather than a request, and a jump that silently lands on an unnamed section
  is the kind of surprise this repo refuses. Pinned by a test in M2.
- **EDGE-16 — tabs and trailing whitespace.** `text` is trimmed at both ends; a tab after
  the markers is not part of the text.
- **EDGE-18 — a closing `##` is part of the name.** `## Deep *Fall* ##` styles its text as
  `Deep *Fall* ##`, and `transclude::heading_text` keeps the trailing hashes too, so the
  index and the anchor matcher agree that the hashes belong to the name. CommonMark strips
  them; neither of our surfaces does. Left alone — that is a parser question with two
  surfaces to move together, not an index question — and written down because a query that
  groups by section will show it to whoever types the anchor.
- **EDGE-17 — size.** 5k notes at ~6 headings is 30k rows, small next to `links` on the same
  fixture. The index on `text` is what a vault-wide anchor lookup reads; it is added with
  the table, not later.

## Milestones

- **M1 — the table and the writer. Closed.** `SCHEMA_V4`, `SCHEMA_VERSION = 4`,
  `Record.headings`, the delete-then-insert in `write_record`, the `drop_note` cleanup, and
  the tests: a note's rows in line order, a fenced `#` producing nothing, a blockquoted
  heading producing nothing (the survey had that one backwards — EDGE-3), a bare `#`
  producing a nameless row, a deleted note losing its rows, and a v3 fixture proving the
  rebuild reaches v4 by being dropped. Rulings 47-52 land here; ruling 46 only half lands,
  because the writer starts from a new `mdstyle::object::heading_spans` (the parser's spans,
  with the text range and depth the old `headings()` threw away) and `transclude` still has
  its own line scanner until M2. The table also answers through `Db::run`, which is the door
  a saved query uses — the rows are not a shell even before M3.
- **M2 — one scanner. Closed.** `heading_row` and `section_rows` re-pointed at
  `mdstyle::object::heading_lines` (the span scan in line numbers), and what is deleted is
  worth listing: `heading_level`, `heading_text`, the hand-rolled fence toggle in both
  anchor functions, `transclude::fence` itself, and `fold::heading_rows` — a fourth copy of
  the same span walk, with its own merge rule for a marker run. The anchor *matching* rule
  (trim, case-insensitive, first match wins) stayed where it was, as ruling 46 said; only
  "is this line a heading, and which part is its name" moved. No existing anchor test was
  re-recorded: the ones that pinned fence behaviour and section slicing passed unchanged,
  which is the result this milestone was supposed to produce. Two new ones pin what
  genuinely changed (EDGE-2), and one pins the fence rule the parser turns out to hold: a
  closing fence must use the same character as its opener and be at least as long, where the
  deleted toggle closed on any `~~~` line after a ` ``` ` one (EDGE-1).
- **M3 — a reader.** The table earns its rows by answering something. Question G decides
  which door: a section-grouped `.shq` (the roadmap's stated purpose, and free the moment
  the table exists), a `:sections` picker that jumps, or both.
- **Not here.** The outline pane is P4.13 (it reads the buffer for the open note and this
  table for the rest); the chips are P7.10; anchor renames are P4.6's report.

## Open questions — one recommendation each

A, C, E, F, H and I are taken as recommended. B, D and G were settled afterwards, each with
its answer written where the question was.

**A. Which scanner wins.** Recommendation: `mdstyle`, per ruling 46, and delete the line
scanner rather than keeping it as a fast path. Cost: `transclude`'s matcher moves from lines
to byte offsets, and its tests get re-read.

**B. Does an anchor match the *plain* text of a heading?** Today `[[note#The Fall]]` does
not match `## The *Fall* of Zhent`; you have to write the asterisks. Obsidian matches the
rendered text. **Closed (user, 2026-11-10): leave it — the anchor is written as written.**
No plain-text column: `[[note#The Fall]]` does not name `## The *Fall* of Zhent`, and a
reader who wants that jump writes the asterisks, which is what the file says. Options: (1) leave it — `text` is the file's claim and the anchor is written
as written; (2) store a second column (`plain`, inline markers removed) and match either,
which is a second truth per heading but makes formatted headings linkable; (3) normalize
`text` itself, which loses the file's claim. Recommendation: **(1) for v4**, and note that
the door is a column, not a rewrite — reopen it when a real vault has formatted headings
worth the mismatch.

**C. Is `line` enough, or does the row need a byte offset?** Recommendation: `line` only
(ruling 48). Every consumer either reveals a row or groups by range; a byte offset is a
second coordinate that goes stale on its own.

**D. Blockquoted headings: sections or not?** Closed by the code rather than by a vote:
**not sections**. The survey read the parser as styling `> ## Title`; it styles nothing
inside a blockquote, and `heading_level` refuses the line for its own reason — so the two
surfaces already agree and there is no divergence to settle (EDGE-3). The index writes no
row, and quoting somebody's heading stays quoting.

**E. Setext headings.** Recommendation: out of scope, with the divergence written in the
schema doc (EDGE-5). Adding them is a parser change that moves folds and `da#` too.

**F. Duplicate headings.** Recommendation: keep every row; the matcher keeps first-wins.
Refusing duplicates would be an opinion about someone's note structure.

**G. What reads the table first.** Options: (1) nothing but `.shq` and the agent door — the
table is the feature, and the roadmap's purpose is query-side grouping; (2) a `:sections`
command — a picker of headings (the open note's, or the whole vault with an argument) that
jumps to the one you pick, which is the outline pane's information without the pane;
(3) make `:query`'s picker offer a heading as a note-with-reveal. Recommendation: **(2) in
M3, with (1) tested in M1.** A table nobody reads is exactly the "`CREATE TABLE` is not a
feature" failure this repo has been bitten by, and `:sections` is a day of work that also
proves the row shape is the shape a consumer wants. **Closed (user, 2026-11-10): (2) and
(1) both** — a `:sections` picker that jumps, and a `.shq` that groups by section. M1 got a
head start on (1): the rows are already readable through `Db::run`, which is what a saved
query runs on.

**H. One `style_spans` pass or two.** Recommendation: two for now, measure on the 5k
fixture, and merge them only if the walk shows the cost (EDGE-14). Restructuring the
indexer's scan before measuring is the perf mistake this repo already has a skill about.

**I. Store the anchor a link uses (`links.anchor`) in v4 too?** It is the missing half of
heading-rename propagation (P4.6's report). Recommendation: **no** — one table per schema
bump keeps the decision still, and the rename report needs a *report* surface anyway. The
door stays open: a column added in a later bump, filled from the same `WikiLink` the link
row already comes from.

## Hooks owed

- roadmap P4.3b: mark the milestones as they land, and add the "three scanners were
  disagreeing" finding to the item — it is the reason M2 exists.
- roadmap P4.6 (the rename report): point at EDGE-11 and question I so the anchor column is
  not rediscovered as a surprise.
- roadmap P4.13 (outline pane): the pointer already says the pane reads this table for
  notes it has not opened; add ruling 52 (the open note stays on the buffer).
- `docs/`: a line in `docs/notes.md` (or `docs/state-files.md`) that sections are indexed,
  once M1 lands, and the `:sections` keys once M3 does.
