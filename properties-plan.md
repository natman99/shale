# Properties (frontmatter as data) — plan

**Status:** ruled 2026-11-08 (rulings §1 are binding). **Owner: P4.3a** — the indexing
is the first commit of P4.3 `.shq`, not a sibling feature. This closes the orphan the
2026-11-05 audit left behind: `brainstorm.md` ruled properties-as-data as a *"P1 schema
decision"*, Phase 1 closed without it, and the only pointer to it (P1.3's *"frontmatter
… stays empty until P3.3"*) named an item that shipped as the graph core and never
parsed frontmatter. As of this writing there is **no frontmatter parser in `crates/`**
(`grep -rn frontmatter crates/ --include=*.rs` → one doc comment in `db/schema.rs`) and
**no writer for the `edges` table anywhere**.

Why it is cheap now and expensive later: `.shq` bodies are hand-written SQL, so every
query note, graph lens, search operator and GM-toolkit feature is written against the
shape decided here. Adding the columns after those surfaces exist means re-deriving all
of them.

---

## 1. Binding rulings (2026-11-08)

1. **Layout:** one KV table — `props(note, key, ordinal, value, kind)`. Not typed
   columns, not generated per-key views, not a JSON blob. Rationale in §2.
2. **`kind` is per key, vault-wide, inferred-first / override-second** — the rule
   Obsidian itself states (*"once a property type is assigned to a property name, all
   properties with that name across your vault will use the same type"*), and the same
   inferred-first/override-second shape as the edge-type registry.
3. **`tags` and `aliases` are projections.** The props table holds every key *as
   authored*; `tags`, `aliases` and `edges` are derived from props **plus the body**.
   **Body `#tags` keep working exactly as before** — the frontmatter feed is additive,
   never a replacement (`graph::extract_tags` over body style spans stays the body path).
4. **The YAML subset parser is hand-rolled in `shale-core`.** No `serde_yaml`
   (unmaintained, not in the lock tree), no `unsafe-libyaml` (conformance we do not
   want). Non-goals are documented, not accidental.
5. **Nested / unsupported YAML is indexed inert** (`kind='nested'`, raw text) and
   **never rewritten**. This is the Obsidian-plugin compatibility and no-data-loss rule:
   a plugin-authored nested block survives Shale untouched.
6. **JSON frontmatter is read, never rewritten.** Obsidian reads JSON and saves YAML;
   we read and leave it alone. TOML `+++` is deferred, with fence detection kept as one
   chokepoint so it stays a match arm rather than a refactor.
7. **Editing surface:** (a) in-place live preview of the block now; (c) a vault-wide
   properties pane later for key census and rename; (b) a separate focus-island strip is
   **refused** — it is a second caret holder and a borrowed-surface interaction pass.
8. **Sequencing:** rides **P4.3** as its first commit (indexing before any UI).

---

## 2. The layout

```sql
-- schema v3: v2 plus one table. Bump SCHEMA_VERSION; drop and rebuild (house rule).
CREATE TABLE IF NOT EXISTS props (
    note    TEXT    NOT NULL,   -- vault-relative, like events.note. Never absolute.
    key     TEXT    NOT NULL,   -- exactly as authored: case kept, spaces allowed
    ordinal INTEGER,            -- NULL = scalar; 0-based index = one item of a list
    value   TEXT    NOT NULL,   -- the authored text, unquoted, for display and round-trip
    kind    TEXT    NOT NULL    -- text | number | date | datetime | bool | link | nested
);
CREATE INDEX IF NOT EXISTS props_key  ON props (key, kind);
CREATE INDEX IF NOT EXISTS props_note ON props (note);
```

The test query, *every monster with CR ≤ 5 sorted by CR*:

```sql
SELECT note FROM props WHERE key='cr' AND kind='number' AND value+0 <= 5
ORDER BY value+0;
```

`kind` is the whole trick. It answers the type question **in Rust at index time, where a
parser exists**, instead of in SQL, where `CAST('low' AS REAL)` is `0` and a monster
whose `cr:` is prose sorts first without complaining. `ordinal` distinguishes a scalar
from a one-item list and preserves list order, for free.

Rejected, with the reason recorded so it is not re-pitched:

- **Typed columns** (`text/num/date/flag`): honest comparisons, but four columns of
  bookkeeping and no rule for a key that is a number in 40 notes and text in 2.
- **Generated view per key** (`props_cr`): nicest queries; DDL that tracks the vault's
  vocabulary is exactly the shape drop-and-rebuild exists to avoid, plus key/table name
  collisions. Kept available as later sugar (`:props view cr`), not built now.
- **JSON blob on `notes` + `json_extract`**: verified available with zero new deps
  (`libsqlite3-sys-0.39.1/build.rs:178` compiles the bundled SQLite with
  `-DSQLITE_ENABLE_JSON1`), but the blob is opaque to the plain-SQL story and cannot be
  indexed by anyone using the read-only `.shq` connection — so it gets slower precisely
  where the feature is meant to be instant.

### Kind inference (per key, not per value)

Declared in `<vault>/.shale/props.toml` (`cr = number`, `died = date`) wins. A GM
campaign's `session` key is a **declared number** for the same reason `cr` is: as text,
`Session 10` sorts before `Session 2`, which silently breaks every chronological query
(EDGE-17's family). Otherwise
the **majority kind across the vault's values for that key**, ties → `text`. A key whose
values disagree is a typo worth *seeing*, not a value worth silently casting: the census
is a query (`GROUP BY key, kind`), surfaced in `:props`.

Value-level rules, deliberately narrow (YAML 1.2 core, not YAML 1.1):

- `true` / `false` → `bool`. `yes` / `no` / `on` / `off` stay **text** (the YAML 1.1
  Norway problem; Obsidian's own checkbox type is `true`/`false`).
- `YYYY-MM-DD` → `date`; `YYYY-MM-DDTHH:MM:SS` → `datetime`. Lexicographic order is
  numeric order, so SQL comparisons work without a date type.
- Decimal integer/float only → `number`. `007`, `0x1f`, `1e5`, `1/4` stay **text**
  (see EDGE-17).
- `"[[Target]]"` (quoted, per Obsidian) → `link`, and it feeds `edges`.

---

## 3. The parser

`shale-core`, one module (`frontmatter.rs`), scanning **only the block** — it stops at
the closing fence, it never walks the note.

In scope: the `---` fence at byte 0; `key: value` pairs; block lists (`- item`, any
indentation consistent within the list); flow lists (`[a, b]`) read-only; single- and
double-quoted scalars with escapes; comments (`#`) and blank lines; an empty block.

Out of scope, by ruling (documented in the module's doc comment and asserted by a test
each): nested maps and sequences of maps (`kind='nested'`, raw text preserved); anchors
and aliases (`&x` / `*x`); explicit tags (`!!str`); multi-document `---` separators;
multi-line scalars (`|`, `>`); JSON inside a YAML value.

Error posture, inherited whole from P1.3: **one unreadable note costs its own row, not
the pass.** A block that will not parse leaves the note with no props, the pass stands,
and `SyncReport.failed` names it. `index.enabled = false` or a DB that will not open
means props are a missing feature reported in the log, never a reported zero.

---

## 4. Projections: one authored record, three derived tables

`props` is the record of what the author wrote. From it (and from the body) the indexer
fills:

- **`tags`** — body `#tags` exactly as today via `graph::extract_tags`, **plus** the
  `tags:` key's list items (leading `#` stripped either spelling). Additive: a note with
  only body tags must index identically to before, and that is a test, not a comment.
- **`aliases`** — feeds link resolution and the spotlight ledger's mention rollup
  (`brainstorm` "Spotlight ledger" cites `aliases:` as its data source).
- **`edges`** — every `link`-kind value, scalar or list item, one directed row per fact,
  no inverses (ruled). **This is the pass that finally fills `edges`** and therefore
  closes the dangling "until P3.3" pointer.

Two rows for one authored fact (props + projection) is the one deliberate exception to
"one row per fact", and it is the same argument already used for `edges`: the props row
is the authored text the strip must show and rewrite; the projection is a query surface.

---

## 5. EDGE-CASE LEDGER

### Format / interop

1. **`---` at byte 0 only.** A thematic break later in the note is not frontmatter.
2. **Closing fence is `---` or `...`.** An unterminated block is not frontmatter at all
   (it is prose), and says so in the census rather than indexing half of it.
3. **Empty block** (`---` / `---`) → zero props, no error, no empty row.
4. **UTF-8 BOM before the fence** is tolerated; anything else before byte 0 is not
   frontmatter.
5. **CRLF** line endings parse identically.
6. **Keys with spaces are legal and must round-trip** — the vault in hand uses `Max HP`,
   `Armor Class`, `First Downs`, `Damage Taken`. No normalization, no `_` substitution.
7. **Key case is significant.** `Type` and `type` are different keys (Obsidian agrees).
   Nothing lowercases; migrating a vault's vocabulary is a key rename, not an accident.
8. **Duplicate keys in one block** (YAML forbids, Obsidian says each name must be
   unique): index **both** rows and flag the note in the census. Never silently drop one.
9. **Comments and blank lines** inside the block are not props.
10. **`#` inside a quoted value** is part of the value; unquoted, it starts a comment.
    The quote state must be tracked before comment stripping, or `tags: "#draft"`
    indexes as an empty value.
11. **Unquoted value containing `: `** is malformed YAML — refuse the block, do not
    guess the split point.
12. **Flow lists** (`tags: [a, b]`) are read; Obsidian writes block lists, so flow is a
    compatibility read, not an output shape.
13. **Nested values** are inert (§1.5). A nested key still appears in the census and in
    `:props`, badged `nested`, and the strip never edits it.
14. **JSON frontmatter** (`---` `{…}` `---`) is parsed with the existing `serde_json`
    and never rewritten back as YAML — rewriting would silently reformat someone's file.
15. **TOML `+++`** is not parsed. Fence detection lives in one function so adding it is
    one match arm.
16. **A wikilink in a list item** (`allies:\n  - "[[Zhentarim]]"`) produces one `edges`
    row per item, including ghosts (an unresolved ally is a visible plot hole — ruled).
17. **`cr: 1/4` is text in YAML**, so a numeric `cr <= 5` query silently drops CR ¼
    monsters. Domain-specific and guaranteed to bite a D&D vault. Ruled fix: declare
    `cr` as a number key and normalize a bare `n/m` at index time (`kind='number'`,
    `value` = the decimal, the authored text preserved for display).
18. **A value that is only whitespace** (`status:` with nothing) → `kind='text'`, empty
    value, and it is *present*: "has the key" and "has a value" are different queries
    (`has-prop:status` vs `prop:status=draft`).

### Lifecycle

19. **Frontmatter deleted from a note** → its rows go. The model is delete-then-insert
    for the note's whole prop set on every index of that note; there is no per-key diff.
20. **Note rename** re-keys or re-indexes the note's rows — the rename path already
    rewrites links and must not leave props pointing at the old path.
21. **Key rename vault-wide** rides the link-health machinery (P4.6) as a text rewrite.
    It must not touch text *inside* a `nested` value.
22. **Dirty buffer vs disk:** the index sees saved text only. The strip's edits are text
    edits into the buffer, so there is exactly one write path (save → index) and no
    separate props write to keep in sync.
23. **Embedded notes:** an embed's props are the embedded note's, and the strip inside an
    entered block edits the embedded buffer — the caret rule already ruled for P4.0b.
24. **Deleted note** → rows go with it, through the existing delete reporting.
25. **No index** (`index.enabled = false`, DB unopenable) → `:props` says there is no
    index rather than reporting zero. Same posture as `:wordcount` and `harper-ls`.

### Concurrency / performance

26. **Parsing stays outside the write transaction**, on the caller's thread (P1.3's
    rule). The writer thread never touches the disk.
27. **The parser stops at the closing fence.** A 5k-note vault pays for the block's
    head, not the note.
28. **No per-key kind table.** The census is `GROUP BY key, kind` over `props` at
    question time; a second aggregate table would be a cache of a cache with its own
    staleness story.
29. **Both indexes are required** — `key` for queries and the operator, `note` for the
    delete-then-insert. Measure the row count and query cost on the `1k`/`5k` fixture
    vaults with the bench recipe before quoting any number.
30. **Anything cached from props keys on `Buffer::change_id()`** — the house rule for
    per-note caches; a view that adopts another note drops the cache.

### Deletion-under-view / surfaces

31. **A `.shq` open on screen whose key is renamed** refreshes on the vault-change beat;
    the results table is built from the query's own column list, so a vanished key cannot
    crash a stale column.
32. **An agent proposal rewriting the frontmatter under an open strip** — the strip
    re-renders from the buffer, never from a cached prop list; the review path's
    placement rules already decide what is still under review.
33. **Hand-editing a `nested` value in raw mode** may make it parseable; it becomes
    editable on the next frame. Stated so it is not mistaken for a bug either way.
34. **`:raw` must show the YAML exactly as written**, including blocks Shale cannot
    parse — raw mode is the escape hatch and the audit trail.

---

## 6. Free rides (no new subsystems — they fall out of the table)

- **The grammar/spell checker stops flagging YAML as prose.** `PROJECT.md:2558` already
  lists frontmatter as the known-next false-positive case; the props pass is the same
  scan that gives `mdstyle` a frontmatter range. Land it in M1.
- **`prop:key=value`** in vault search (P4.14, already ruled) is one query shape.
- **`edges` becomes non-empty**, which unblocks typed-edge rendering and the graph
  lenses (P4.16) that cite `region:` props.
- **Tag tree, graph hues and `tag:` see frontmatter tags** for the first time.
- **The GM toolkit's "free rides" stop being hypothetical**: statblock queries
  (`type`/`cr`/`hp`/`ac`), `weight:` for random generators, `aliases:` for the spotlight
  ledger, `campaign:`/`session:` for the session spine.

---

## 7. Milestones (split by dependency, each one commit-sized)

- **M1 — parser + schema v3 + `props` table.** No UI beyond `:props` printing a note's
  props from the DB. Includes the frontmatter range for the checker. Exit: the fixture
  table for §3 and §5 passes; a note with broken YAML costs only its own row.
- **M2 — projections:** `tags` (body unchanged + frontmatter added), `aliases`, `edges`.
  Exit: `edges` is non-empty on a vault with a link property; a body-only-tagged note
  indexes identically to before (regression test); ghosts get typed edges.
- **M3 — query surfaces:** `prop:` search operator, the kind census in `:props`, and the
  first `.shq` written against `props` (the statblock query is the natural fixture).
- **M4 — in-place live preview of the block** (display segments / hidden ranges; the
  buffer never changes; `:raw` shows the YAML). Editing a value is a text edit.
- **M5 — the vault-wide properties pane:** key list with counts, kind conflicts, and key
  rename through the link-health machinery.

M1–M3 are the schema decision and belong to P4.3a. M4–M5 are surfaces and can be
scheduled apart from it.

**Explicitly NOT in P4.3a's scope (added 2026-11-08, while it is being implemented):** a
`headings` index. A later round ruled that queries should be able to group things by the
section they sit in (cue/counter chips, the outline pane reading the index), and that
pulls a **`headings(note, line, text, depth)` table — schema v4, roadmap P4.3b — which
lands AFTER P4.3a, never inside it.** P4.3a ships v3 with `props` alone. If the parser
happens to see headings on its way through the note, that is not a licence to add the
table in the same commit.

---

## 8. Open questions (each with a recommendation)

1. **GM key vocabulary.** The vault in hand mixes cases (`Type`, `AC`, `Max HP`).
   *Recommendation:* lowercase single-word keys as the convention (`type`, `cr`, `hp`,
   `ac`, `weight`, `region`, `campaign`, `session`), declared in `.shale/props.toml`;
   existing mixed-case keys keep working (case is significant) and a key rename is the
   migration path, not a forced rewrite.
2. **`cr: 1/4` normalization** (EDGE-17). *Recommendation:* yes — normalize a bare
   `n/m` for keys declared `number`, keeping the authored text for display. Five lines,
   and it is the spelling the domain actually uses.
3. **Keys with spaces in the search grammar.** *Recommendation:* quoting, not escaping:
   `prop:"Max HP"<=5`. No new escape rules anywhere.
4. **Where kind conflicts are shown.** *Recommendation:* the `:props` panel lists keys
   whose values disagree in kind, with counts, and offers the rename. No modal, no
   warning on every save.
5. **JSON/TOML scope.** *Recommendation:* read JSON (already a dependency), defer TOML,
   keep fence detection in one function.
6. **Does the checker's frontmatter range land in M1?** *Recommendation:* yes — it is
   the same scan, and it removes a known false-positive class for free.

---

## 9. Hooks (done in the same session as this file)

- `brainstorm.md` "Properties: frontmatter as data" — owner marked, rulings recorded.
- `brainstorm.md` "Typed edges" — the cost claim *"the parser already reads
  frontmatter"* corrected: it does not, and P4.3a is the pass that reads it.
- `roadmap.md` P4.3 — P4.3a as its first commit; dependency-map edge added; P1.3's
  "until P3.3" pointer annotated as then-vs-now; open decisions 12–13 added.
