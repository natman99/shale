# Vocabulary run — the palette's words (roadmap P4.21, proposed)

The `:` words and the status-line errors are in good shape. The command palette is not: 56
rows, and the same verb appears twice (`Save` / `Write`), the same object appears twice
(`hunk` / `proposal`), the same panel appears twice (`strip` / `sidebar`), and the same
point in the text appears twice (`caret` / `cursor`). Some rows describe a mechanism
instead of an action. This is a wording pass over one table — no behaviour, no keys, no
`:` words.

## Rules (ruled with the user, 2026-11-19)

1. **Parity.** Opposite and similar actions get complementary wording; genuinely different
   actions get completely different wording. `Turn the spellchecker on/off` is the model;
   `Graph: the whole vault again` beside `Graph: flat view` is the failure.
2. **Directness.** The row names the action and its object. No scenery, no mechanism:
   `Keep the cursor's line near the middle of the window` (52 chars) is a description of an
   effect; the command is a scroll.
3. **One thing, one noun.** A concept has exactly one word in every user-visible surface —
   labels, the keymap's "what it does" column, and the key hints (`j in the strip`).
   Drift found: `hunk`/`proposal`, `strip`/`sidebar`, `caret`/`cursor`, `Save`/`Write`.
4. **Reachable by the word people type.** Palette matching is subsequence matching over the
   label, so a rename must keep the noun the reader would type (`frontmatter`, `lens`,
   `graph`, `pin`) inside the label. Verified per row, not assumed.
5. **One tail grammar.** A qualifier arrives as the same clause: `…, saving first` /
   `…, discarding unsaved edits`. A toggle is `Show or hide X`. A state is `Graph: <state>`.
   Same punctuation, so two rows can be compared by eye.
6. **A state says the state; a motion says the motion.** Nothing that *sets* a view says
   "again" or "back".
7. **Length.** Aim ≤ 40 characters; allow ~50 where the object genuinely needs qualifying
   (the review-hunk family). The palette is a narrow column and the longest row sets the
   width every row is measured against.

### Rulings

- **Save** wins the whole save family (`:w` keeps its spelling; the row says *Save*).
- **Caret**, never cursor, in labels and keymap wording.
- **search** is the act, **query** is the saved `.shq` file (`Search the vault`,
  `Run a saved query`, `Save a search as a query` — that last row is the split stated).
- **Scope: labels + keymap wording only.** Keys and `:` words are muscle memory and stay.
- **Sidebar** in everything a reader sees. The panel is `:sidebar`, `docs/sidebar.md` and
  the `sidebar` key context; *strip* was the code's habit leaking into the palette and the
  key hints (`j in the strip`). The region inside it that holds pending hunks is **the
  review view**, in step with the sidebar's own "view" wording (`show the previous view in
  the sidebar`) — nothing reader-visible is called a strip after this pass.
- **Hunk** for the thing under the marker. *Agent edits* stays only where the whole queue is
  meant (`Review the pending hunks` names the view; the queue is the agent's edits).

## The table

Rows not listed stay exactly as they are (`New note`, `Rename the selected note`,
`Undo`, `Split this pane, stacked`, `Close every tab but this one`, …).

| id | now | proposed | why |
|---|---|---|---|
| `note.write-all` | Write every dirty note | **Save every dirty note** | rule 3, Save ruling |
| `note.save-close` | Write the note, then close this tab | **Save, then close this tab** | rule 1: pairs are `X, then Y` |
| `note.write-quit` | Write every note, then quit Shale | **Save every note, then quit Shale** | rule 3 |
| `tab.close-write` | Close this tab, writing its note first | **Close this tab, saving first** | rule 5: one tail clause |
| `window.close` | Close this pane, saving its notes first | **Close this pane, saving first** | rule 5, parity with the tab row |
| `tab.close-discard` | Close this tab, discarding its unsaved edits | **Close this tab, discarding unsaved edits** | rule 5, parity with Quit |
| `tab.pin` | Pin this tab: a jump opens a new tab instead | **Pin or unpin this tab** | rule 5 (toggle shape); the explanation belongs in docs |
| `tab.move` | Move this tab to a position | **Move this tab** | rule 2; the `args` field already spells the position |
| `view.raw` | Show the note as markdown source | **Show the note as source** | rule 1: one shape for the three modes |
| `view.live` | Show the note with live preview | **Show the note with live preview** | the shape's model |
| `view.preview` | Preview the note, without editing it | **Show the note read-only** | rule 1, rule 2 |
| `graph.flat` | Graph: flat view | **Graph: flat** | rule 6 |
| `graph.local` | Graph: this note's neighbourhood | **Graph: this note** | rule 7 |
| `graph.global` | Graph: the whole vault again | **Graph: the whole vault** | rule 6: a state is not a return |
| `graph.projected` | Graph: projected 3D view | **Graph: 3D** | rules 6, 7 |
| `focus.toggle` | Dim everything but the block the cursor is in | **Dim everything but this block** | rules 2, 3 (caret) |
| `typewriter.toggle` | Keep the cursor's line near the middle of the window | **Centre the view on the caret** | rule 2 |
| `sidebar.toggle` | Show or hide the right-hand strip | **Show or hide the sidebar** | rule 3 (open question) |
| `sidebar.focus` | Focus the right-hand strip | **Focus the sidebar** | rule 3 |
| `checker.show` | Show the checker's note for the word here, or its card | **Show the checker's note for this word** | rule 2: the card is a mechanism |
| `clipboard.copy` | Copy the selection, or the last yank, to the clipboard | **Copy to clipboard** | rule 2; parity with Paste |
| `clipboard.paste` | Paste from the clipboard | **Paste from clipboard** | rule 1 |
| `note.add-properties` | Add frontmatter properties to the top of the note | **Add frontmatter properties to the note** | rule 7; keeps `frontmatter` reachable (rule 4) |
| `vault.props` | Show the open note's properties | **Show this note's properties** | rule 1: "this note" is the app's phrase |
| `review.focus` | Review the agent edits that are waiting | **Review the pending hunks** | rule 3 (open question), rule 2 |
| `review.toggle` | Collapse or expand the pending proposal under the marker | **Collapse or expand the pending hunk under the marker** | rule 3, rule 1 with Accept/Reject |
| `vault.switch` | Switch vault | *(keep)* | different action from `Change the theme`; the `:` word is `:vault` |

### Keymap wording (`action_note()` in `workspace/keys.rs`)

Same pass, same rules, one table of ~120 rows:

- **10 `cursor` → `caret`** — and the drift is inside one family: `Backspace` says "the
  character before the cursor" while `FieldBackspace` says "the character before the caret";
  `MoveLeft` says cursor, `FieldLeft` says caret.
- **`strip` → `sidebar`** in `SidebarPrevView`, `SidebarNextView`, `PinSidebar`, and the
  review rows' key hints (`j in the strip` → `j in the sidebar`) — the panel is `:sidebar`,
  `docs/sidebar.md`, and `sidebar.toggle`.
- Parity pairs that are already right stay: `collapse this folder, or step out of it` /
  `open this folder, or step into it`.

## Nouns settled (ruled 2026-11-19)

**sidebar** for the panel in every reader-visible surface (internal names like `sidebar.rs`
and `SidebarDown` already agree, so they are untouched), and **hunk** for the thing under the
marker — `proposal` leaves the vocabulary entirely, and *agent edits* survives only as a
description of the whole queue, never as the object of a verb.

## How it gets pinned

- **A vocabulary lint test** over `Command::every()` and `action_note()`: a banned-word list
  (`cursor`, `proposal`, `Write the note`, `strip`) that fails the build, so the drift cannot
  come back one label at a time.
- **Reachability, asserted not assumed**: for each renamed row, the palette's offered rows
  for the query a reader would type (`save`, `frontmatter`, `lens`, `pin`, `graph`, `3d`)
  still contains it — the existing `Workspace::picker_rows` oracle, extended to the renamed
  set.
- **The generated docs follow in the same commit**: `SHALE_UPDATE_DOCS=1 cargo test -p
  shale-app` unfiltered, so `docs/commands.md` and `docs/keymap.md` are regenerated rather
  than hand-edited, and `the_docs_are_the_registry` proves it.
- **Tests that name labels** get updated in the same edit (`tests/picker.rs`,
  `tests/sidebar.rs`, `tests/graph/filter.rs`, and the label strings in `workspace/*.rs`
  comments that quote a row).
- **Falsification**: the banned-word lint must go red when a banned word is put back; each
  renamed row's reachability assertion must go red when the label loses its noun.

## Milestones — all three shipped (2026-11-19)

- **M1** `035cd05` — the table applied: 33 rows of `command.rs` labels, key hints and
  `action_note()` wording, docs regenerated, the three tests that named a label updated.
  The reachability rule bit here: `Show the note read-only` made the mode unfindable by the
  word people type for it, and the test that types `preview` did not fail — it quietly ran
  the *live* preview. The three view modes now share one shape and keep their nouns: `as
  source`, `as live preview`, `as read-only preview`.
- **M2** `3c382b0` — `tests/words.rs`: the banned-word list, the 40-character budget, and 19
  reachability queries. Eleven mutations, all red. Two honest greens on the way: the lint
  reads the generated docs, so a table change reaches it through the docs test that rewrites
  them; and the budget covers palette rows, so the first length mutation went green on
  `tab.next`, which is a docs row.
- **M3** `e4e8367` — the lint extended to `README.md` and every hand-written `docs/*.md`,
  which found 100 lines in 11 files still using a retired word (`docs/sidebar.md` said
  "strip" nineteen times), and the sweep of them. Ruled on the way: **search bar** for what
  `/` opens in a note, and the graph's **filter strip** keeps its name because `graph.md`
  defines it. Four falsifications, all red.

Two findings that were not vocabulary:

- The graph's `+`/`-` note said "magnify at the cursor". Renaming it to the caret would have
  been wrong twice: the graph has no caret, and the keyboard zoom anchors at the canvas
  centre because there is no pointer to anchor at. The wheel, which does have a pointer,
  says **pointer** in the prose. A vocabulary sweep has to check what a word refers to, not
  just which word it is.
- `keys.md` and `mcp.md` described `za` collapsing a *proposal* and `y` answering "all of
  its hunks" — a unit the app does not have. The registry's unit is the hunk under the
  marker. The docs had invented a level of the model.
