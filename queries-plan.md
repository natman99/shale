# `.shq` query notes — plan (roadmap P4.3)

The "Obsidian bases" goal: a saved question that lives in the vault as a plain text
file, answers from the index, and can be embedded anywhere. Brainstorm ruled the feature
✅; this file is the build plan, the edge ledger, and the six decisions the build needs.

## Already ruled — not re-litigating

- **A `.shq` is a file in the vault**, plain text and git-able, editable in the normal
  editor. No special folder is *required* of the author (`queries/` is where the app
  writes them, not the only place they work) — ruled here, see §Q2.
- **The body is SQL.** The GM-toolkit rulings ask for aggregates and joins a shorthand
  cannot reach — "the party ledger is a saved `.shq` over those tables", per-session
  rollups, average damage — so the file holds a statement, not a mini-language.
- **The vocabulary is shared with vault search** (P4.14: one grammar, one parser).
  `tag:`, `prop:`, `path:` name the same columns in the search bar and in a saved query,
  so a search can be saved and a saved query is readable by someone who already knows
  the search. What is *not* shared is the syntax: the grammar is the front door that
  generates SQL (§Q1's collision, resolved below).
- **Relative queries**: `relative = "current"` in the declaration, `:current` bound at
  run time, no new index (it is `links` keyed by source, joined to `props`), and the
  anchor path joins the cache key. Refused for graph lenses (a graph tab has no host).
- **A graph lens IS a saved `.shq`** (P4.16): rows resolve to note paths, ghosts allowed,
  edges are the induced subgraph. That is a consumer of this item, not part of it.
- **Read-only is enforced at the connection level**, not by sanitizing the text.
- **Computed surfaces are app-offered and region-anchored; data tables in notes are
  `.shq`; modes are rationed.** A query view must not become a mode with its own keys
  unless it earns one.

## Landed — M2 (2026-11-09)

`:query` and **Run a saved query**: the `.shq` list labelled by declared name, and the
answer drawn in the picker — columns once, then one row per answer. A row whose first cell
names a note in the vault is that note; one that names none stays a row. The declaration's
`sort` is honoured (applied to the answer, not spliced into the statement). The citizenship
ruling is in: `.shq` rows are in the tree and open in the editor, and are not in quick-open.
The binding rules were finished here: one `scan` finds both the statement break and the
`:name` parameters (two walkers would disagree about where a quote ends), an unasked-for
binding is dropped, and an unsupplied one is refused by name. EDGE-1, 2, 3, 6, 8, 9, 14,
16 and the palette door are covered by tests; the demo vault holds two query notes, both
run against a copy of the vault to check the demo answers.

## Landed — M1 (2026-11-09, `8f93336`)

`query_note::QueryNote::parse` (the file: optional TOML declaration between `+++` fences,
then the body) and `Db::run` (one statement, read-only connection, typed cells, named
bindings). `Vault::list_queries` and the `.shq`-out-of-attachments rule are the file's
vault citizenship. The rules and their reasons are in those two modules' doc comments;
each was falsified individually.

## The collision the roadmap carries

brainstorm.md says **SQL body**; the P4.14 entry says **"one grammar, shared with
`.shq`"** and "the same grammar inside a `.shq` body". Read literally, those ask for two
languages in one file. The resolution this plan takes: **the grammar is the front door,
SQL is the file.** Vault search compiles the operator grammar into the same SQL a hand
written `.shq` would hold; `:save-query` writes that SQL into a new `.shq`. One grammar,
one meaning for `prop:cr<=5` everywhere, and the file stays inspectable by a reader and
writable by an agent. §Q1 asks you to confirm or redirect this.

## The M3 view — ruled 2026-11-09

The file renders as **editable SQL with a live table under the body**, which is what
brainstorm asked for; the shape of it was the debate, and these are the rulings.

- **The mechanism is a fold, not a concealment system.** The declaration and the SQL body
  are folded by default and the fold's *rendering* is the table rather than an ellipsis.
  Live preview hides spans inside a row (`is_marker`); hiding a whole multi-line region is
  a fold, and folds already own the hard parts — per-view state keyed by path, a marker
  row, and every motion, ruler and scroll that agrees with them. A second way for text to
  be invisible would be a second set of bugs to find twice (display index ≠ buffer row; a
  row that paints nothing still has to own a byte).
- **The declaration folds too**, so the file reads as a titled table.
- **Three modes, one rule each.** Raw: the source, no table — the file as git sees it.
  Live: the table, the SQL folded, and *editing opens the fold*. Preview: the titled table
  only.
- **No pagination.** Preview already scrolls rather than moving the caret (`j`/`k` step,
  `gg`/`G` jump the ends, wheel and scrollbar work — `View::Preview`'s doc: "a note that
  cannot be read from the top is no use"). A large answer is a layout question, answered
  by virtualizing the region — draw the rows intersecting the viewport, height = row count
  × row height so the scroll extent stays honest — not by a page key and page state to
  keep in step with the index.
- **The marker row carries the answer's shape**: `▸ Creature ratings · 2 rows`. Folded, the
  file still says what it computed.
- **A refusal or an empty answer reveals the source.** A folded region with no table is a
  blank page, which would turn EDGE-1 and EDGE-2 into the silence M2 refused.
- **The line that keeps this from colliding with the properties-block ruling**
  (`545fb25`: nothing is hidden inside a properties block, because the brackets *are* the
  data): **hide what computes, never hide what asserts.** The SQL is a program; the answer
  is the data.
- **`/` searches the body, never a row** (EDGE-23): a hit is a caret position and phantom
  rows own no bytes. The picker is the searchable view of a query; the region is the
  readable one — which is a reason M2's picker stays rather than becoming scaffolding.

## Perf checks owed

Not decisions — measurements the rulings above deliberately deferred.

- **A query embedded in a query, or in itself** (EDGE-10): the embed chain's nesting cap
  bounds it, but the *cost* of a nested run inside one frame is unmeasured. Bench before
  raising the cap.
- **An uncapped read** (EDGE-9/K): the surface draws 200 rows and says how many it cut; the
  core reads whatever the query answers. A 100k-row answer is a memory question, not a
  display one. No cap until a real query makes it one — a cap is a wrong answer the reader
  cannot see.
- **A large region** (EDGE-19): layout cost of a tall phantom region, and whether a slow
  query needs to run off the frame or be cached. The cache's key rule is already written in
  `db/query.rs` (the anchor path belongs in the key); the choice between cache and
  off-frame run waits for the measurement.

## Edge ledger

Built ones name their test; the rest name the milestone that owes one.

| # | Case | Answer |
| --- | --- | --- |
| EDGE-1 | A query that answers nothing | "no note answers this", not an error and not a blank table (M2) |
| EDGE-2 | SQL that fails | the SQLite message and the reader's own text; the file stays editable (M2) |
| EDGE-3 | A body that is not a question (`UPDATE`, `PRAGMA`, empty) | refused out loud — built |
| EDGE-4 | Two statements in one body | refused, quoting the text after the `;` — built |
| EDGE-5 | `;` inside a string, a quoted identifier or a comment | not a statement break — built |
| EDGE-6 | A declaration that opens `+++` and never closes it | refused — built |
| EDGE-7 | A declaration key this version does not know | kept out of the struct, not out of the file — built |
| EDGE-8 | NULL vs empty string vs 0 | three different cells — built |
| EDGE-9 | A result with 10k rows | core reads what the query answers; the surface caps what it draws (M2) |
| EDGE-10 | A `.shq` embedded in a `.shq`, or in itself | the embed chain's nesting cap already bounds it; no query-specific rule (M4) |
| EDGE-11 | `![[query]]` where the query errors | the block shows the error, not a ghost (M4) |
| EDGE-12 | A relative query with no host note (graph tab) | refused out loud (M5) |
| EDGE-13 | A relative query whose result is cached | the anchor path is in the key (M5, or whenever a cache lands) |
| EDGE-14 | Index off (`index.enabled = false`) | "there is no vault index to read", never zero rows — the EDGE-25 posture (M2) |
| EDGE-15 | A `.shq` in the tree, in quick-open, in `:keys`-style lists | §Q2 (M2) |
| EDGE-16 | Rows naming notes that do not exist | allowed; painted as ghosts, which is the lens ruling's "visible plot hole" (M3) |
| EDGE-17 | SQL naming a table that does not exist yet (`headings`, P4.3b) | SQLite's message, verbatim; no invented fallback (M2) |
| EDGE-18 | An agent writes a `.shq` with a syntax error | the same as EDGE-2, and the review strip's accept/reject governs the write (M7) |
| EDGE-19 | A query that takes seconds | the surface must not block the frame; measure, then decide where it runs (M3) |
| EDGE-20 | `:current` inside an embed chain | the host note, which the resolver already knows (M5) |
| EDGE-21 | A `.shq` renamed while embedded | the rename machinery re-files the embed target, as a note rename does (M4) |
| EDGE-22 | The same query asked twice in one window | one run per (file, index change), not one per frame (M3) |
| EDGE-23 | `/` in a query note | matches the body, never a row — a hit is a caret position and phantom rows own no bytes. The picker is the searchable view (M3) |
| EDGE-24 | A fold key in preview mode | `za` is swallowed there ("preview scrolls, it does not move"), so the marker row is the affordance and a leader chord is the keyboard route (M3) |

*(EDGE-25 is the roadmap's index-off posture, not this ledger's — the numbers coincide.)*

## Milestones

- **M1 — the format and the runner.** Landed. No surface.
- **M2 — the door.** Landed. `:query` and the palette's *Run a saved query*, the rows in
  the picker, the tree and quick-open citizenship settled.
- **M3 — the results where the query lives.** **Landed** (`acb0f55` fold, `da616ad` table).
  The declaration and body are one fold that starts shut; the table hangs under the last
  visible line of the program, whether the fold is shut or open; the marker row is the
  table's title and carries the answer's shape (`Creature ratings · 2 rows`). Rows that
  name a note are doors, the same rule the picker follows. EDGE-16, 22, 23, 24 landed;
  EDGE-19 (a query that takes seconds) waits for the perf review with the virtualization
  the surface cap has made unnecessary so far — see ruling 17.
- **M4 — `![[query-name]]`.** The transclusion resolver's third branch (note, attachment,
  query), and the same table with the same marker row. An errored query shows its error in
  the block (ruling 6). EDGE-10, 11, 21.
- **M5 — relative queries.** `:current` is already bound; what is left is the anchor's
  journey from the host note to the runner, and the refusal for a host-less surface.
  EDGE-12, 13, 20.
- **M6 — save a search.** `:save-query` writes the SQL the search compiled into a new
  `.shq`. This is where "one grammar" pays for itself.
- **M7 — MCP.** Create/edit/run query notes, so "make me a dashboard of X" writes a
  file rather than a session. EDGE-18.
- **Not here:** the graph lens (P4.16) and the GM screen (P7.4) consume this item.

## Rulings

1. **Where do results render?** — **taken (a)**: the picker for M2, the in-view results
   region for M3. `result_rows` is the one producer of what a row says, so the region
   draws the same rows differently rather than re-deciding them.
2. **Is a `.shq` a note in the tree and in quick-open?** — **taken**: a tree row (and it
   opens in the editor like any file the author owns), not a quick-open row, reached by its
   own `:query` picker.
3. **The declaration fence is `+++`.** Taken in M1, because `---` collides with a note's
   YAML block and with SQL's `--` comment. No veto, so it stands.
4. **No result cache until a query is measured slow.** Taken in M1 (the key rule the
   cache owes is written in `db/query.rs`). Say so if you want the hash+mtime+anchor
   cache built now.
5. **Do `sort` and `view` do anything in M2, or are they parsed and shown only?**
   **Recommend:** `sort` applies from the start (one `ORDER BY` in the surface, no new
   machinery); `view` is parsed and shown, and only `table` is honoured until a second
   view exists.
6. **`![[query]]` when the query errors** — **taken**: the error is shown inside the block.
   No hidden errors and state is the philosophy; a broken embed that hides its reason is the
   exact failure this item keeps refusing.

### Ruled 2026-11-09 (the M3 view)

7. **Unfolding the SQL in preview**, where `za` is swallowed — **taken: both.** The marker
   row is clickable (already how a phantom region is entered) and a leader chord is the
   keyboard route, so reading your own query never costs you the keyboard.
8. **SQL highlighting** — **taken, split.** The fences and `--` comments are the region's
   shape and get spans now; keywords wait for **P6.6 (tree-sitter)**, which is the planned
   home for language highlighting rather than a second hand-written scanner.
9. **No result cache until a query is measured slow** — stands.
10. **`view` vocabulary** — only `table` is honoured; an unknown spelling is kept and says
    nothing on every render. `list`, `kv` and the graph lens arrive as consumers.
11. **A query embedded in a query, or in itself** — the chain's nesting cap bounds it; the
    cost is a perf check, not a rule.
12. **Renaming a `.shq` while it is embedded** — decided at M4, and the decision is whether
    a query embed is indexed as a link. If it is not, renames silently strand embeds, which
    is the bug the note-rename pass already fixed.
13. **Relative queries (M5)** — `relative = "current"` is refused for a host-less surface;
    `:current` inside an embed chain is the host note (EDGE-20).
14. **`:save-query` (M6)** — the file holds the SQL the search compiled (one grammar, one
    meaning), written to `queries/` with a name asked for and never overwritten silently.
15. **MCP can run queries, not only write them (M7)** — and the read-only guarantee has to
    stay as strong through that door as through the editor's: the same `Db::run`, the same
    read-only connection, no second runner invented for agents.
16. **The runner stays uncapped** — the surface caps what it draws; a perf review is owed
    before any cap exists.
17. **What opens the fold, and what does not** — landed as two gates, one per kind of
    wrong. A body the runner would refuse on shape (two statements, an unterminated quote)
    gets **no fold at all**: `fold::query_region` asks `db::check_question`, the same
    question the runner asks, so the two doors cannot disagree about what is worth folding.
    A body that is well-shaped but fails at run time keeps its fold **and opens it**: the
    answer has an error, and a folded region with no table is a blank page. An answer of
    **no rows** keeps the fold shut — the table draws `the query answers nothing` under the
    marker, which is a reason the reader can read without opening anything. This refines
    the 2026-11-09 ruling ("errored or empty unfolds"): empty is not an error, and unfolding
    a program that answered correctly hides nothing useful.
18. **Virtualization is deferred, not dropped** — the region is capped at `MAX_ROWS` (200)
    rows plus a line admitting how many were left out, so the phantom region a frame lays
    out is bounded and comparable to an embed block. Virtualizing it (draw only the rows
    that intersect the viewport, height = rows × row height) is a perf-review item next to
    EDGE-19, not a correctness one. Say so if the 200-row frame measures slow.
19. **Prose wraps; a table row does not.** A block has no viewport of its own — the host
    scrolls — so a row longer than the pane simply leaves the frame with no way back. The
    sentences an answer says about itself (a refusal, the empty-answer line, the admission
    about rows left out) are prose and break at the pane's width, measured with the note's
    own wrappers so the break lands where the paint has glyphs. A **data** row keeps its
    columns whole: breaking one mid-cell leaves the continuation lines with nothing to
    align to, and the per-cell clip already bounds each cell. `PhantomKind::Caption` is the
    kind that marks a sentence rather than a cell.
20. **The answer wears the grid a typed table wears.** Horizontal and vertical rules, the
    table's own palette roles (`table_line`, `table_header_bg`) rather than new ones, a
    shaded header, a one-cell gutter inside the outer edges and a rule in the middle of the
    gap between columns — so a rule never sits against a value. One deliberate divergence:
    a computed answer draws a rule between **every** row, where a markdown table draws only
    under its header. A markdown table's rows are lines of a file the reader can already
    count; an answer's rows are results, and the grid is what says so. Prose rows — a
    refusal, the admission about rows left out — wear no grid: they are sentences *about*
    the table, not cells in it.
21. **A database message is flattened where a surface draws it** — `db::one_line`. SQLite
    quotes the statement it refused, newlines included, and gpui refuses a text run that
    holds a newline. The core keeps the whole message (the MCP tool still reports it); the
    table row and the picker row each flatten it. Cells flatten the same way: a cell may
    hold a newline, a table row may not.
