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

## Milestones

- **M1 — the format and the runner.** Landed. No surface.
- **M2 — the door.** Landed. `:query` and the palette's *Run a saved query*, the rows in
  the picker, the tree and quick-open citizenship settled.
- **M3 — the results where the query lives.** The declaration's `sort`/`view` start to
  mean something; rows that name notes behave like links. EDGE-16, 19, 22.
- **M4 — `![[query-name]]`.** The transclusion resolver's third branch (note, attachment,
  query). EDGE-10, 11, 21.
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
6. **`![[query]]` when the query errors:** show the error inside the block (recommended —
   a broken embed hides the reason) or fall back to a ghost.
