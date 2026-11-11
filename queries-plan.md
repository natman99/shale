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
- **M4 — `![[query-name]]`.** Landed 2026-11-10 (rulings 25-31): the resolver's third
  branch, the answer's table hung under the row that asked, the badge as the marker row,
  and an errored query showing its reason in the block. EDGE-10, 11.
- **M4b — the link a query embed asserts.** Landed 2026-11-10 (rulings 32-34): the index
  files `![[query]]` as `target_kind = 'query'`, `known_names` colours it resolved, `gd`
  and the answer's title row open the `.shq`, and renaming a `.shq` keeps its extension,
  keeps it out of the `notes` table, and re-files the embeds that name it (ruling G,
  EDGE-21). The door and the index moved together, as ruling 30 demanded. What the map
  said was there to change, surveyed 2026-11-10:
  - `db/index.rs` resolves link targets against markdown entries only, so `![[query]]` is
    filed as `target_kind = 'ghost'` today; `target_kind` has three values and no `query`.
  - `known_names`/`note_keys` hold markdown stems and paths (plus aliases), which is why
    the embed paints as unresolved — agreeing with the ghost row, which is the pair ruling
    30 refuses to break.
  - The graph does not read `links` at all: `graph/index.rs` re-scans text, so a query
    embed is a ghost node there until its own entry list learns `.shq`.
  - Renaming a `.shq` is already reachable from the tree (`r`) and `ctrl-shift-r`, and
    already wrong twice: the prompt's `sibling_path` appends `.md` only for `.md`, so a
    typed name loses the extension, and `Db::note_renamed` has no markdown filter, so the
    renamed file gains a `notes` row and its stem starts posing as a note name.
- **M5 — relative queries.** Landed 2026-11-10 (rulings 36-38): the anchor's journey from
  the host note to the runner, decided by the query itself in one place, and the refusal for
  a host-less surface. EDGE-12, 20. EDGE-13 (the anchor in a cache key) waits for a cache.
- **M6 — save a search.** Landed 2026-11-10 (ruling 40): `where_clause` is the one builder
  the search box and the saved file both read, `compile_sql` writes its values as literals,
  and the offer to keep a search is the last row of an operators-only search. Three searches
  cannot be saved and each is refused by name.
- **M7 — MCP.** The agent door: `list_queries`, `read_query`, `run_query`, so "make me a
  dashboard of X" writes a file and reads back what it answers rather than holding a
  session. Writing stays `write_note` (ruling 43), the runner is the same runner on a
  connection that cannot write (ruling 41), a relative query names its host or is refused
  (ruling 42), and the sentences come from one chain both doors read (ruling 44).
  EDGE-18.
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

### Ruled 2026-11-10 (the first reader bug round)

22. **The caret never comes to rest in a row the view refuses to paint.** `G` on a folded
    program asks for the last row of the file, and that row is inside the fold; `5G` can
    land in a closed section the same way. The caret goes to the row the fold is drawn on —
    the only row of it a reader can see, and what vim does with a closed fold. The rule
    already ran when a fold *closed over* the caret (`clamp_cursor_out_of_folds`, in
    `buffer/folds.rs`); the bug was that the other door — a *motion* reaching for a hidden
    row — never asked. One rule, both doors, called from the keymap after the engine moves
    the caret. Without it the status bar names a row the screen does not show and the next
    `j` leaves from somewhere out of sight.
23. **Preview's `G` is a motion of the view, not of the caret.** It goes to the end of what
    the view *shows*. A query note hangs its answer below the program, and the rows of an
    answer belong to no line of the file — so the last row of the file is not the last row
    on screen, and no caret position can reach it. The scroller does (`scroll_to_bottom`);
    the caret still goes to the end of the text, so what `/` repeats and `g f` follows
    afterwards are at the end the reader is looking at. `gg` needed nothing: the top of the
    file and the top of the view are the same row.
24. **A fold decision belongs to the note, not to the view's attention.** Clicking a row of
    an answer replaces the query note in the same tab, and `ctrl-o` brings it back; the
    reader did not ask to be told the program is folded again. The workspace keeps each
    note's fold state while the note is off screen and hands it back when it returns — the
    same shape as ruling 16's fold-inside-an-embed, which lives on the host's editor keyed
    by path. The regions are re-derived from the text, so a note that changed underneath
    still resolves the decisions against the regions it has now.

### Ruled 2026-11-10 (M4: a query embedded in a note)

25. **`![[query]]` resolves in core and answers in the app.** The transclusion resolver
    gains a third branch — note, attachment, saved query — and it hands over a
    `QueryBody`: the file, the name the reader knows the query by, and the program as the
    file wrote it. It does not hand over an *answer*, because a query's content is what
    its program computes and the index to compute it against belongs to the window. One
    resolution, two halves: neither half gets to decide alone what `![[Creature
    ratings]]` names.
26. **A note beats a query of the same name.** `![[tough]]` is `tough.md` when one exists,
    even if `queries/tough.shq` does too — the same precedence aliases were given
    (2026-11-09). Nothing that already means a note gets quietly retargeted at a query.
27. **An anchor on a query embed is refused in the note branch's own words** — `no section
    "hp" in tough`. A program has no sections; its `#` lines are SQL comments. Two
    surfaces must not disagree about what the same mistake costs.
28. **The program is read through `Sources`, the same door an embedded note's text comes
    through.** A `.shq` open in a tab with unsaved edits answers with the program the
    reader is looking at, not the disk's older one; a resolver that read the file itself
    would quietly disagree with the editor. Pinned in core, where the rule lives — at the
    app level the fixture needs two live tabs, which is heavier than the rule deserves.
29. **The badge row carries the answer's shape, and says `refused` rather than `0 rows`.**
    `tough · 2 rows` is the marker row's sentence (EDGE-10: the badge *is* the marker row),
    produced by one function both surfaces call. A run-time failure is named, not counted:
    a badge reading `0 rows` would let the reader believe the query asked and got an
    answer. An empty answer *does* count zero — that is an answer (ruling 17's shape).
30. **No door on the badge yet, deliberately.** A door aimed at a `.shq` would resolve in
    this one surface and nowhere else: `resolve_target` reaches notes and aliases,
    `known_names` paints link colour from that same list, and the index files no such
    link. A colour is a claim about the index, so the door waits for M4b — where the index
    row, the painting and the rename machinery (ruling G, EDGE-21) move in one step.
31. **A query reached through another note's body is a name, not a second table.**
    `![[a]]` where `a.md` embeds a query: the block carries the query's name at depth 2.
    The answer belongs to the row that asked for it, and a row inside someone else's block
    is not that row.
32. **The badge says the answer's shape; the row above says the name.** The reader's first
    complaint about an embedded query was the name printed twice: the host's own row
    renders `![[Creature ratings]]` as that name, and the block's title row said it again.
    `answer_count` is the one producer of the shape (`2 rows`, `refused`); the query note's
    own marker row wraps it in the name, because there the fold has hidden the whole
    program and the marker is the only thing left saying what the file is. EDGE-10's "the
    badge *is* the marker row" survives as the same *rendering of the answer*, not the same
    sentence.
33. **A query embed is a link everywhere the question is asked, or nowhere.** Ruling 30's
    condition, paid in one step: the index files `![[query]]` as `target_kind = 'query'`
    (after notes and attachments, before the ghost), `known_names` colours it resolved,
    `gd` follows it, and the answer's title row is a door onto the `.shq`. `resolve_target`
    consults saved queries last, after notes and aliases — the same order the resolver and
    the indexer use, so the four surfaces cannot name four different files.
34. **Renaming a `.shq` keeps it a `.shq`.** The rename prompt's `sibling_path` keeps
    whatever extension the file has (a note keeps `.md`, a query keeps `.shq`, a file with
    no extension stays exactly as typed), and `Db::note_renamed` refuses to write a `notes`
    row for a non-markdown path: a notes row would put the stem in the list of names a link
    resolves to, which is a claim the markdown scan never made. `rewrites_for_rename`
    resolves notes first and saved queries last, so the `![[query]]` rows that name the
    file move with it.
35. **The graph is the one surface still out of step.** `graph/index.rs` re-scans text with
    its own entry lists and never reads `links`, so a query embed is still a ghost *node*
    in the graph view while the index, the colour and the door agree it is a link. Left for
    the graph pass rather than patched half-in: teaching the graph a third list is a
    signature change through `build` and `set_note`, and a half-taught graph is exactly the
    disagreement ruling 33 refuses.
36. **The anchor is the note the reader is standing in, and only a note** (M5). `:current`
    binds the host of the surface that ran the query: the active note for `:query`, the note
    holding the embed for a `![[query]]` block. A query note is not a note, so a relative
    query viewed in its own file has no host — the same nothing a graph tab has — and says so
    rather than binding its own path and answering about a file the `links` table has no row
    for. Inside an embed chain the host is unambiguous by construction: a query reached
    inside another note's body is a name at depth+1 (ruling 31), so a query table never hangs
    under anyone else's block (EDGE-20).
37. **The declaration and the body must agree about `:current`.** A body that asks for the
    anchor while the declaration never said `relative = "current"` is refused by name: the
    picker labels a query "relative" from the declaration, so a body whose answer changes
    with where the reader stands while the label says otherwise is hidden state. The other
    direction costs nothing — a declaration claiming a relativity the body never uses runs,
    the same way an unknown `view` is kept rather than refused (ruling D).
38. **A host-less surface refuses in one sentence, written once.** EDGE-12: `QueryNote::
    anchor` returns `Bound` / `NotAsked` / `Refused(reason)`, and every surface — the picker,
    an embedded table, the query's own view, and later the MCP verb — reads its refusal from
    there. The picker draws it as a notice row; the table draws it as a refusal row, which
    opens the fold (ruling 17's shape: a run-time failure reveals its source). SQLite's "the
    query asks for `:current`, and nothing supplies it" stays what it is — the message for a
    body that asks for a parameter nobody ever meant to bind, not the message for standing in
    the wrong place.
39. **A door answers for the cell it is painted in** (the reader's report, 2026-11-10). A
    phantom row's addressable band is the row's whole width when the row *is* the block's
    width — an embedded note's body, a badge — and the grid **cell** when the row is a cell
    inside a table: the space past a table's right edge, and the padding of a neighbouring
    column, belong to the note the answer hangs under. `PhantomLine::door_span` carries the
    cell in character cells from the grid's left edge, so the rule is stated once in the
    painter and holds for a later answer whose second or third column names a note. Same
    class as the hover card's band having to check x, not only y: a hit region is a claim
    about what is on screen. The badge row keeps its whole band by the P4.0a ruling — it is
    the block's title, not a cell in it.
40. **A saved search is the SQL the search box compiled, not a re-spelling of it**
    (P4.3 M6). `db::where_clause` builds the conditions and their values once: the live
    search binds them, `db::compile_sql` writes them into the file, because a `.shq` has no
    way to be handed an argument. The oracle is the only test worth writing for that claim —
    compile a search, run the statement through the runner, compare it note for note with
    what the search selected. Three searches cannot be kept and each is refused **by name**
    rather than compiled into a statement that means something else later: plain text (it
    reads lines, and the index holds no lines), `path:` (Shale's own matcher, not SQLite's
    `LIKE`), and `modified:` with a duration (it counts from the moment it runs, so a file
    would freeze the day it was saved — a date saves fine). The offer appears only where it
    can be taken, as the last row of an operators-only search; `:save-query` states the
    reason before the reader has spent a name on a search that cannot be kept. A name is one
    name — no separators, no folders spliced out of a prompt — and an existing file is never
    overwritten. Found on the way and fixed: the link fragment escapes the `LIKE` wildcards
    in a name without saying which character escapes them, so `link:"100%"` found nothing —
    and this is the statement a saved file keeps, so the mistake would have outlived the
    search box. One gap left open on purpose: creating a file that explains an existing ghost
    link does not re-file that ghost in the index, and the same is true of a created *note*,
    so it is one fix for both doors rather than an M6 special case.
41. **The agent door runs the same runner, on a connection that cannot write** (M7).
    Ruling 15's promise is kept by moving the runner rather than copying it: `db::run_on(conn,
    sql, bindings)` is the whole act, and `Db::run` is `check` + `self.read()` + that call.
    The stdio server never starts a writer — `db::reader(vault_root)` opens the *existing*
    `<vault>/.shale/shale.db` with `query_only` and answers `None` when the file is not there,
    which is EDGE-14's sentence ("there is no vault index to read") rather than zero rows. A
    door that creates an index it never writes to would be claiming a vault has one.
42. **A relative query names its host or is refused** (M7). The MCP door takes an optional
    `note` argument and hands it to `QueryNote::anchor` exactly as the editor hands the note
    under the caret; without it, `anchor(None)` refuses in the sentence ruling 38 wrote
    (EDGE-12). Ruling 36 is untouched — the host is a note, and only a note — the difference
    is that an agent has no caret, so it *names* the note instead of the door guessing one.
    A `note` that is not a note in the vault is refused, not bound.
43. **One write door** (M7). There is no `write_query`: `write_note` writes any text file in
    the vault, `.shq` included, because a second verb is a second profile rule and a second
    review path for one act — and EDGE-18's promise (the review strip governs an agent's
    query write) is already kept by the door that exists. What the query door adds is
    *visibility* (`list_queries`, `read_query`: `list_notes` walks markdown, so an agent
    could not see a query at all) and the run verb. `write_note`'s description says what it
    covers, since an agent decides which verb to reach for from that sentence.
44. **The sentences come from one chain** (M7). `query_note::ask(text, host, run)` parses the
    file, decides the anchor, runs the body, applies the declaration's sort, and answers
    `Answered { name, rows } | Empty { name } | Refused { why }`. The app maps that to picker
    rows; the MCP door maps it to text. EDGE-1 ("no note answers this"), EDGE-2 (the SQLite
    message plus the reader's own text), EDGE-3/4 (a body that is not one question) and
    EDGE-12 (no host) are each one sentence with one author, so the two doors cannot drift
    into disagreeing about the same file.
