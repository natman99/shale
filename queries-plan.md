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
- **M2 — the door.** `:query` and the palette's *Run a saved query*: pick a `.shq`, run
  it, show the rows. Where the rows go is §Q1. Carries EDGE-1, 2, 9, 14, 15, 17.
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

## Rulings I need

1. **Where do results render?** (a) the picker for M2 — rows as text, enter opens the
   note a row names — and the in-view results region in M3; (b) the region under the SQL
   from the start; (c) a results pane of its own. **Recommend (a):** the picker already
   renders rows, detail and highlight, and it proves the whole pipeline in one commit;
   the region is the right end state and it is a bigger draw.
2. **Is a `.shq` a note in the tree and in quick-open?** **Recommend:** visible in the
   tree (it is a file the author owns), absent from quick-open's note list, reached by
   its own `:query` picker. A `.shq` in the note list would make `space f` answer two
   different questions.
3. **The declaration fence is `+++`.** Taken in M1, because `---` collides with a note's
   YAML block and with SQL's `--` comment. Veto if you want `---` for consistency with
   notes.
4. **No result cache until a query is measured slow.** Taken in M1 (the key rule the
   cache owes is written in `db/query.rs`). Say so if you want the hash+mtime+anchor
   cache built now.
5. **Do `sort` and `view` do anything in M2, or are they parsed and shown only?**
   **Recommend:** `sort` applies from the start (one `ORDER BY` in the surface, no new
   machinery); `view` is parsed and shown, and only `table` is honoured until a second
   view exists.
6. **`![[query]]` when the query errors:** show the error inside the block (recommended —
   a broken embed hides the reason) or fall back to a ghost.
