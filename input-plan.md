# Input & defaults sweep — plan

Six rough edges reported 2026-11-10, all in the layer between the keyboard/mouse and the
window: the picker's text field, the editor's arrow and Tab keys, the graph's `escape`, the
theme default, and the mouse's side buttons. None of them was a roadmap item; none had been
fixed before this round (checked against `roadmap.md`, `docs/`, and the commit history).

This file is the binding record for the round: rulings first, then the edge ledger, then the
milestones in the order they land.

## 1. What the code already said before the round

| Ask | State found |
|---|---|
| Selection + clipboard in the picker | `crates/app/src/field.rs` is a caret and a line of text — **no selection model at all**. `caret_selection()` reports an empty range to gpui, so IME and paste cannot replace a selection either. Three owners wire the field keys by hand: `picker.rs`, `prompt.rs`, `graph/panel.rs`. |
| Arrow keys in the editor | Bound in `vim-insert` only (`keys.rs` 348-370). `vim-normal` (284-347) has no arrow bindings, so in normal mode the arrows do nothing. |
| Tab in the editor | No `tab` binding anywhere in the app's keymap, and gpui ships no default one — so nothing ever inserts a tab. |
| `escape` in the graph | Bound to `FocusEditor`, whose handler (`workspace/tree.rs:303`) swaps the pane back to the last note tab when the active tab is a graph. That swap is what reads as "escape closes it". |
| Default theme | **Mostly built already** (§15 of `design-language.md`): `theme = ""` names no palette, `Workspace::follow_desktop_theme` asks the XDG Desktop Portal, `desktop::built_in_for` maps the answer onto a pair, and `follow_desktop` stops the following the first time a theme is chosen by hand. What is missing is the *default*: `Palette::DEFAULT = MOCHA`, `DEFAULT_NAME = "mocha"`, and `Config::starter()` writes `theme = "mocha"` into every new vault — so the following path is never reached. |
| Mouse side buttons | gpui already delivers them: `gpui_linux` maps `BTN_SIDE`/`BTN_BACK → MouseButton::Navigate(Back)` and `BTN_EXTRA`/`BTN_FORWARD → Navigate(Forward)` (wayland `client.rs:2371`), X11 buttons 8/9 likewise. `JumpBack`/`JumpForward` (`ctrl-o`/`ctrl-i`) already exist. Nothing consumes the buttons. |

## 2. Rulings

1. **The picker's operators stay in the field.** `shift`-selection, `ctrl-a`, clipboard keys
   are bound in the field contexts only — `prompt`, `picker`, `popup`, `graph-filter`. The
   note keeps vim's `ctrl-a` (increment) and its own `ctrl-shift-c`/`ctrl-shift-v`.
2. **`ctrl-a` in a field selects all.** `Home`/`End` jump to the ends. (Ruled 2026-11-10.)
3. **A one-line field has no vertical motion of its own.** In the picker, plain `up`/`down`
   move the highlighted row — that is the picker's job and stays. `shift+up`/`shift+down`
   therefore extend the selection to the start/end of the text, the same as `shift+Home`/
   `shift+End`.
4. **Tab inserts a literal tab character** in insert mode; **shift-Tab removes one indent
   level** from the line. (Ruled 2026-11-10.) *Addition, veto-able:* an indent level is one
   leading tab, or up to four leading spaces when the line has no tab — so shift-Tab can undo
   an indent the file spelled with spaces.
5. **`escape` in the graph stays in the graph.** It clears the keyboard's chosen node, and
   closes the filter strip when the strip has the keyboard (already `GraphFilterClose`).
   `ctrl-w l` is the way to the note. (Ruled 2026-11-10.) A second `escape` with nothing
   chosen does nothing — the graph never hands the keyboard away on its own, because that is
   the surprise the item was about. This retires the P3.4 note in `acts.rs` that spelled the
   strip's close as "the first `escape` closes the strip and only a second one leaves the
   tab": the second one now puts down a node.
6. **The default theme follows the desktop; the fallback is dark.** `theme = ""` becomes the
   shipped default and the starter config writes it commented-out-but-empty with the reason
   spelled out. The auto pair is **`dark` / `light`** — Shale's own neutral built-ins, not a
   family's flavours. (Ruled 2026-11-10.) `Palette::DEFAULT`/`DEFAULT_NAME` move to `dark`,
   so "the desktop did not answer" and "the theme file was broken" both land on a dark palette
   nobody has a personal stake in.
7. **The side buttons are the jump list**, exactly as `ctrl-o`/`ctrl-i`: back button →
   `JumpBack`, forward → `JumpForward`. Bound on the window's root, so they answer wherever
   the pointer is. *Addition, veto-able:* they are ignored while a picker, prompt or popup
   holds the keyboard — jumping under an open picker would move the note out from under the
   rows being read.
8. **Copy with no selection copies the whole field text** (a field is one line; the whole
   line is the thing you meant). Cut and paste with no selection behave as at the caret.

## 3. Edge ledger

- **EDGE-1 — the picker's arrows are the row, not the caret.** Covered by ruling 3. The
  binding table must not put `shift+up` in the same row as `up`.
- **EDGE-2 — a selection is a claim about the text, so every edit has to honour it.**
  `insert`, `backspace`, `delete_forward`, `delete_word_back` and the IME path
  (`replace_utf16`) all delete the selection first. A selection that only the painting knows
  about is a lie.
- **EDGE-3 — gpui's input handler must see the real selection.** `caret_selection()` becomes
  the actual range, `reversed` set when the anchor is ahead of the caret, so an IME
  composition replaces what is selected instead of inserting at one end of it.
- **EDGE-4 — the caret is the near end.** The model stores `anchor: Option<usize>` and keeps
  `caret` as the moving end, so `shift+left` after `shift+right` shrinks instead of flipping.
  A selection whose ends are equal is no selection: the anchor is cleared, not kept.
- **EDGE-5 — non-ASCII.** Every motion steps by *character*, not byte, as the existing
  `step_caret` does. A test pins a multi-byte query (`«goblin»`) so a byte-stepping
  regression cannot land silently.
- **EDGE-6 — a paste carrying newlines flattens to its first line** (existing rule for the
  field); the clipboard path must reuse `replace_utf16`'s flattening rather than write its own.
- **EDGE-7 — the clipboard is gpui's**, the same door the note's copy/paste use
  (`cx.write_to_clipboard` / `cx.read_from_clipboard`), so Wayland's clipboard manager sees
  one format.
- **EDGE-8 — both clipboard spellings work in a field**: `ctrl-c`/`ctrl-x`/`ctrl-v` and the
  `ctrl-shift-` pair the note already uses. A field has no `ctrl-c` meaning to protect (the
  editor's `ctrl-c` is a vim thing and lives in another context).
- **EDGE-9 — three owners, one wiring.** `picker`, `prompt` and the graph's filter strip each
  attach the field's actions by hand today; twelve more keys × three owners is where drift
  starts. One macro in `field.rs` attaches the whole set, and each owner calls it once.
- **EDGE-10 — Tab is not focus traversal here.** Nothing in the app binds `tab`, and gpui
  ships no default binding for it, so insert-mode Tab is free to mean a character. In normal
  mode `tab` stays unbound (it is a motion in vim, and inventing one here would be a second
  opinion about what Tab means).
- **EDGE-11 — shift-Tab at the first column of an empty line** removes the line's whole
  indent and never runs past the first non-blank; on a line with no indent it does nothing.
- **EDGE-12 — Tab and shift-Tab are buffer edits**, so they ride the buffer's undo: one
  `u` after typing three tabs takes them one at a time, exactly as typing letters does.
- **EDGE-13 — arrows in normal mode are motions, not the engine.** They dispatch the same
  actions insert mode already uses (`MoveUp`/`MoveDown`/`MoveLeft`/`MoveRight`), which are
  display-aware: they step between *display* rows of a wrapped or folded line, the way `j`/`k`
  do. They must not be swallowed by a pending count (`3` then `down` is not a thing; the count
  is dropped, matching how the engine treats an unmapped key).
- **EDGE-14 — arrows keep their other meanings everywhere else**: the picker moves rows, the
  tree moves rows, the graph pans the camera. Only `vim-normal` gains them.
- **EDGE-15 — `escape` in the graph must not clear the filter strip's *text***, only close
  the strip (already the case); and clearing the chosen node must not clear the pointer's
  hover, which is the pointer's business, not the keyboard's.
- **EDGE-16 — the graph's node index is slot-order**, so clearing selection is `selected =
  None`, and the next `h`/`j`/`k`/`l` starts from the centre again (the existing rule for
  "nothing chosen yet").
- **EDGE-17 — the theme default is a config default, not a loader change.** `theme = ""`
  already means "ask the desktop"; the fix is that the shipped default and the starter
  template stop naming mocha. A vault that *did* choose a theme keeps it — nothing here
  re-reads a named theme.
- **EDGE-18 — the fallback chain stays honest**: named theme → (missing: report and use
  `Palette::DEFAULT`) and empty → desktop portal → (`None`: `Palette::DEFAULT`). Both ends
  land on `dark` now, and the status line's name follows whichever it landed on.
- **EDGE-19 — the desktop answer arrives late on purpose** (a portal round trip; the window
  opens in the fallback and settles). A theme chosen by hand before the answer wins — the
  existing `follow_desktop` guard — and the pair change must not disturb that.
- **EDGE-20 — the side buttons must not double as a click.** A `Navigate` button press is a
  distinct `MouseButton`, so the root's left-click handlers never see it; and the buttons are
  read on the root div, where a picker's own click handling sits above it in the tree.
- **EDGE-21 — the jump list is what it already is.** The buttons call the same verbs as
  `ctrl-o`/`ctrl-i`, so per-pane scope, the graph tab's exemption, and the "opening a note
  pushes a jump" rule are unchanged — no new list, no new semantics.
- **EDGE-22 — a mouse-down takes the keyboard, whatever the button.** gpui's focusable
  elements transfer focus on a mouse-down for *any* button (`div.rs:2776`), so a side-button
  press over the note pulled the keyboard out of an open picker and left the field stranded
  with no way to cancel it. The handler is therefore `capture_any_mouse_down` on the window's
  root — capture runs root-first, so it answers before anything deeper sees the press — and
  it calls `cx.stop_propagation()` in both arms, answered and refused. Falsified by removing
  the stop: the picker test goes red.

## 4. Milestones (dependency order, one commit each)

- **M1 — arrows in normal mode.** **Landed** (`489ac63`). Four bindings in `vim-normal`, a
  keystroke test, and the Linux keysym shape pinned in `keymap_matches_the_linux_keystroke_shape`.
- **M2 — Tab / shift-Tab in insert mode.** **Landed** (`489ac63`, same commit as M1 — both
  live in the binding table, so splitting them would have meant splitting a file by hunk).
  `InsertTab`/`OutdentLine`, `indent_cut` in core with its own test, wording in the keymap's
  `action_note` table (the docs gate refuses a binding that says nothing).
- **M3 — side buttons → jump list.** **Landed.** `capture_any_mouse_down` on the window's
  root (see EDGE-22), the modal-field guard, two tests: the buttons walk the chain and land on
  the byte the link was read at; and a picker keeps its keyboard, its rows and its focus.
- **M4 — graph `escape`.** **Landed.** `GraphClearSelection`, the view's `clear_selection`
  (the graph handles its own actions; no workspace hop), and the binding swapped. The old
  behaviour was load-bearing in four tests and two doc lines — `escape` was how you got from
  the graph to the note to type a command — so those moved to `ctrl-w l` in the same commit.
  The falsification is the old binding put back: the new test goes red on the selection
  assertion, not on the tab assertion, which is the half that actually changed.
- **M5 — theme default.** `Palette::DEFAULT`/`DEFAULT_NAME` → `dark`; `desktop::built_in_for`
  → `dark`/`light`; `Config::starter()` default and the starter template's `theme` line; docs
  (`docs/config.md`, `docs/keys.md` untouched, `design-language.md` §15's auto-pair line
  amended in the same commit).
- **M6 — field selection + clipboard.** The `Field` model (anchor, selection-aware edits,
  utf-16 selection for gpui), the painted selection band, the shared key macro, the three
  owners' wiring, the bindings in all four field contexts, and tests: keyboard selection,
  selection-replacing edits, copy/cut/paste round trip, non-ASCII, and the picker keeping its
  row navigation while the field selects.

M1-M5 are independent of each other; M6 is the one that touches three surfaces at once, so it
goes last and gets the most tests.
