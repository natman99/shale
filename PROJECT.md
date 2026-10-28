# Shale

Shale is an obsidian-style markdown editor with extra developer focused features. Shale is built with GPUI in Rust

## Goals

- [ ] "Live preview" (obsidian-style) Markdown editor with vim binding support
- [ ] File tree and interface
- [ ] Wiki-links
- [ ] MCP server
- [ ] Spellcheck and grammar checking using HarperLS. Support for custom language servers.
- [ ] Near-perfect compatibility with Obsidian vaults
- [ ] Image support in notes using Obsidian style
- [ ] Theming support and support for custom themes. Include light and dark themes.
- [ ] Graph view
- [ ] Pdf export notes
- [ ] Tabs
- [ ] Tiling split view (like Obsidian)
- [ ] Command pallette (vim `:` and Vscode's `ctrl+shift+p` to open. Vim version supports vim features like `s/x/x` replacement. Fuzzy search for commands)
- [ ] Note search and full vault search (command pallette, vim `/`)
- [ ] Templates: save standard notes as templates to be copied in to other notes with a command. Also add a gui picker.
- [ ] "Raw" mode (disables syntax highlighting and markdown rendering)
- [ ] "Database" tables (Obsidian bases) see below

- Config files in TOML
- Assure any code you reference or copy from online has a commercial permissible licensing
- Write unit tests
- Regularly commit to a local git repo. You don't need to publish it. Don't override the user's global config, make a local override for a user for yourself.
- Expand this plan and goals as needed.
- Assure **proper** support for manipulating tables with vim bindings. No unexpected skipping or jumping around when manipulating tables. Ask the user for expected behavior when designing this.
- Tree-sitter powered code highlighting for code blocks.
- Full keyboard support if at all possible. 98% of the app should not require mouse usage. Mouse in the settings menu is fine.
- Indexing for quick wiki-link look up and resolving.
- Use GPUI's testing framework for headless testing of the app when possible.
- Use `gd` for follow the wiki link under the cursor, `gD` to create in the base vault directory if it doesn't exist.
- Follow the chain of links, opened files, jumps, and edits, `ctrl+o`/`ctrl+i` should properly navigate along the chain like neovim and vim.
- Preview mode that will support more advanced markdown rendering, and will not have editing.
- Markdown tasks should support mouse clicks to check, and a "partly complete" icon for `[/]` in preview mode. 
- Fuzzy search for the file picker

Gui pickers should have a search bar with fuzzy search, but arrow keys + enter can select options manually, or the mouse can. The same picker logic and UI should be reused for the command pallette and commands as much as possible to provide a consistent experience.

## Non-Goals

- Plugins

### Tech Stack

Keep as much in Rust as possible, no bindings external libraries. Update the tech stack as needed.

- Tokio for async
- GPUI for UI — **from Zed's repo, tag `v1.25.3`** (`gpui` + `gpui_platform`
  with `wayland`/`x11`/`font-kit`). crates.io `gpui` 0.2.2 is an older tree whose
  Wayland backend stalls its frame/present loop, so the window never updates on
  compositors like niri; the `gpui_linux` crate at this tag has the rewritten
  frame-loop state machine and works. Apache-2.0 (commercially fine).
  On Linux the renderer is wgpu (Vulkan), so a Vulkan driver is required.
- Serde for file parsing
- You may want to use GPUI kit, a components library for GPUI. Evaluate if it's useful before using.
  https://gpui-kit.com/
  Note (evaluated): gpui-kit's crates (`gpui-pre`, `gpui-base`) are a **fork of
  gpui**, so they cannot be mixed with Zed's gpui — a component library would
  have to be built on our own elements. Don't re-litigate this without checking
  the dependency tree.

### MCP

Shale has an MCP server with support for reading, writing, and searching vaults. The MCP server should _either_ be a separate binary which shares core functionality with a `core` crate in the project, or if speed will be acceptable use the main binary.

If the app is open it should inform the user when the MCP is used to edit files and what files were changed. Make a toasts feature that will appear in the corner. Toasts always disappear after a few seconds. The timer on that can pause if the app isn't focused.

### Graph View

The graph view needs to have filter options to hide and show nodes. It should have the option to color nodes based on tags. I want the ability to sort into "constellations" based on the subfolder the note rests in. Add the option to filter a parent folder to then have the constellations be its children. Add 3d and 2d graph views. Graph should have the option to be static, where each note gets it's position from some sort of stable and fast method, and a dynamic physics based graph like Obsidian has. Make sure to handle duplicate note names and other weird edge cases.

Add a "heatmap" option which shows the notes that are edited and visited the most. Track these in the local DB.

Example of constellations:
vault /
child 1/
pie/
cake/
child 2/
child 2 1/
child 2 2/
note.md
note2.md
When the constellation filter is set to child 1 it's children, `pie/` and `cake/` should be shown as separate filtered areas.

Also add the ability to have nested constellations shown.

#### Graph animation & idling rules

The full design (with verified gpui file:line references) is in `graph-view-plan.md`; these are the binding rules:

- **No frame unless dirty.** gpui only draws when the scene is dirty (`should_draw = invalidator.is_dirty() || force_render`, `gpui/src/window.rs:1884`). Never `cx.notify()` from a bare timer. Every continuous animation must be the thing that marks dirty, and must stop marking it when it has nothing to animate.
- **The physics sim sleeps.** It runs at a fixed timestep while the graph tab is open, alpha decays to a settle threshold, then it stops notifying the UI. Vault edits, filter changes, and node drags re-heat it. The sim runs on a background thread and never touches `App`; positions ship to the UI thread via a channel.
- **All continuous animation pauses when the window is not focused.** Use `window.is_active(cx)` + the window activation observers. On deactivate: freeze the ambient-drift clock, stop shipping sim positions to the UI (the sim may keep ticking off-thread), let MCP edit ripples expire without drawing, pause orbit rotation, pause toast timers. On activate: resume. Keep one shared "window active" flag in the workspace that the graph and toasts both subscribe to. (gpui's built-in inactive-window 30 fps throttle is not enough — our animations keep the scene dirty.)
- **Ambient drift is the only opt-in continuous frame source** (`[graph] drift` in config, off on the performance preset). With drift off and the sim settled, Shale must draw zero frames while idle.
- **Platform frame loops differ — don't fight them in app code.** Wayland (`gpui_linux`) parks on a `wl_callback` when idle: true 0% CPU. Windows (`gpui_windows`) has a `VSyncProvider` thread that `DwmFlush()`es and invalidates every visible window each refresh; a settled idle window still takes a ~60 Hz wakeup + dirty-check (sub-1% CPU, no scene rebuild, no present). Minimized windows are skipped.
- **Verify idle behavior** by watching the `FrameLoop::{Ticking, AwaitingCallback, Scheduled, Parked}` log lines in `gpui_linux`'s `wayland/window.rs` plus `top`/`pidstat` (expect 0.0–0.3% with drift off); ask the user to eyeball the graph tab for unwanted motion.

### Database 
Persist metadata about notes to a local sqlite DB. This, along with other vault metadata, should be kept in a `.shale` directory in the vault. Store modification and creation date, tags, names, and whatever else is relevant as the project continues. When Obsidian properties are added, these should be stored as well. Use incremental updating of the DB. When the app is launched, a background thread should go and check for modified files that weren't persisted (due to an external editor, etc) and update the vault in the background. This background thread should then end and further updates will be done as notes are updated.

The DB should track the number of times notes are opened/edited (debounced to avoid overlapping consecutive edits).

The user will be able to write in line SQL queries (heavily sanitized, read only) that will then be used to create in line rendered tables based on vault contents. Query results should be cached when possible. For now, modifying information in these tables is not required. Just list the matching notes and data with the option to go to the note like wikilinks.

These are equivalent to Obsidian bases, which allow dynamically updating views into vault contents based on tags, referenced other notes, referenced by notes, and Obsidian note "properties".

**Note:** The DB remains an extra feature. The plain text notes are the source of truth always. If you need to persist note contents to the DB (for user searching or whatever else) that's okay, but it will never be the primary source of information.

## Current status (updated as the project progresses)

Workspace layout:
- `crates/core` (`shale-core`): vault model (path-traversal safe), wiki-link
  extraction/resolution/backlinks, search, TOML config (`shale.toml` in the
  vault), text `Buffer` with undo/redo, the vim mode engine, `mdstyle` (the
  presentation-agnostic markdown style scanner, split into `mdstyle/mod.rs` for
  the vocabulary and the line walk, `mdstyle/block.rs` for what a line *is* and
  `mdstyle/inline.rs` for emphasis/links/tags inside it), `mdlayout` (which
  markdown markers live preview hides, and the visible pieces that remain),
  `table` (the table model: cells, cell lookup, row/column editing and moving —
  `table/mod.rs` is the model, `table/edit.rs` the splices), `fold` (the foldable
  regions of a note — a heading and the rows it owns — the closed-fold set keyed
  on heading text, and the per-row display effect of that set), `fuzzy` (the
  subsequence matcher and ranker the pickers use: a score, and the byte ranges of
  the candidate the query consumed), `watch` (the
  vault signature — every path stamped with size and mtime, or just existence for
  a folder — and the added/removed/modified diff built from two of them), `rename`
  (rewriting the wiki-links that point at a note when it is renamed) and `theme` (the
  colour palette as data: the `Palette` roles and 20 built-in values — Shale's
  `dark`/`light` plus the palettes of seven families, one module per family
  (`theme/catppuccin.rs`, `tokyo_night.rs`, `gruvbox.rs`, `nord.rs`,
  `dracula.rs`, `solarized.rs`, `one.rs`, `rose_pine.rs`) — the
  `<vault>/.shale/themes/<name>.toml` loader and the callout accents derived from
  them). The hexes are upstream's own: Catppuccin's from `palette.json` v1.8.0 of
  https://github.com/catppuccin/palette with the roles filled by its style guide
  (https://github.com/catppuccin/catppuccin/blob/main/docs/style-guide.md), and
  the rest from each family's palette file — Atom's One is defined in HSL rather
  than hex, so `one.rs` evaluates those definitions instead of trusting a port.
  Every one of those upstreams is **MIT**, so the hexes may live in this
  Apache-2.0 crate. Pure Rust, fully unit tested.
- `crates/app` (`shale` binary): GPUI app (Zed's gpui, tag `v1.25.3`) — file
  tree sidebar, custom line-based editor view with **soft wrap** and vim
  normal/insert/visual modes (hjkl, w/b/e, 0 ^ $ — screen-line aware, gg/G,
  counts on motions and operators, d/c/y + motions, f/t/F/T, iw/aw/ip/ap, x, p/P,
  u undo, i/a/I/A/o/O, v/V, `.` repeat, ctrl-z/ctrl-r/ctrl-y, clipboard, mouse
  click to position), **live markdown styling** (headings, emphasis, code,
  quotes, lists/tasks, tables, tags, links; resolved wiki-links tinted blue,
  unresolved dimmed) with **live preview** hiding the markers of a row the cursor
  is not on, **heading-section folding** (`za`/`zc`/`zo`/`zd`/`zM`/`zR`, or a click
  on the `⋯ n more lines` marker a folded heading carries), and link navigation
  (`gx` on a link in normal mode, ctrl+click),
  status bar (mode, pending count/operator, dirty flag, path, display row:col),
  **theming** (`appearance.theme` selects a built-in palette — Shale's own `dark`
  and `light` plus 18 family palettes: the four Catppuccin flavours (`latte`,
  `frappe`, `macchiato`, `mocha`, also spelled with a `catppuccin-` prefix),
  Tokyo Night ×3, Gruvbox ×2, Nord, Dracula, Solarized ×2, One ×2 and Rosé Pine
  ×3 — default `mocha` — or a `<vault>/.shale/themes/<name>.toml` file, and
  `appearance.font_size` sizes the text),
  ctrl+s save, and a file tree that can create, rename and delete notes and
  folders through a one-line prompt (`prompt.rs`); a rename also rewrites the
  wiki-links that pointed at the old name, in every other note and in the open one
  if it has no unsaved edits. It follows the disk when
  another program changes the vault, and **quick-open** (`ctrl-o`, or `space f` in
  normal mode: a fuzzy list of every note, `j`/`k` to move, `enter` to open) and a
  **command palette** (`ctrl-shift-p`, or `:` in normal mode) — both pickers
  *overlay* the note rather than pushing it down, and the note has margins around
  it. `ctrl-w h`/`ctrl-w l` move the keyboard between the sidebar and the note,
  and **vault search** (`ctrl-shift-f`) lists every line that holds what you type,
  opening the note on that line. Spell and grammar checking come from a language
  server (`harper-ls --stdio` by default, named by `[spellcheck]` in `shale.toml`):
  what it finds is drawn as wavy underlines in the note and counted in the status
  bar, and vim's `K` — or resting the pointer on a marked word — opens a **popup**
  over the note with the checker's complaint and the fixes it offers (one popup at
  a time, anchored to the line it belongs to, not an OS window). Split into `main.rs` (the
  command line and the window it opens), `acts.rs` (every action the keymap
  names — the two namespaces its `actions!` calls generate are what the keymap
  and the tests match on, so they are load-bearing, not just namespaces),
  `field.rs` (the one-line text field the prompt and the pickers share: the text,
  the caret, the utf-16 translation gpui's input handler speaks, and the element
  that paints it and registers the input handler), `prompt.rs`, `picker.rs` (the
  searchable list), `popup.rs` (the checker's panel over the note: its message, its
  rows, and the size the window uses to place it), `workspace/` (`mod.rs` the state,
  `keys.rs` the keymap,
  `tree.rs` the sidebar's keyboard state and its rows, `edits.rs` the prompt and
  the vault edits behind its answers, `open.rs` the pickers' rows and choices
  (a row is either a note or a command), `popups.rs` the window's one popup —
  opening, closing, and applying what the checker offered,
  `commands.rs` the command palette's list
  — every command's label, the keys that run it and the action it dispatches —
  `render.rs` the frame), `tree.rs` (turning the vault's entries
  into rows) and its headless tests in `tests/` (`keys.rs`, `paint.rs`,
  `tree.rs`, `picker.rs`, `popup.rs`), `editor/` (the view: `mod.rs`
  holds the state and accessors, `wrap.rs` the display model, `render.rs` builds
  the frame and the key context, `actions.rs` the key handlers, `input.rs` the
  IME/paste/click surface), `line.rs` (the `LineElement` that paints one display
  line, and the run builder), `preview.rs` (the painted form of a display line and
  the buffer <-> painted-offset map), `image.rs` (resolving an embed target to a
  vault attachment and sizing the picture), `table_view.rs` (the table grid
  geometry: column widths and cell offsets, in character cells), `theme.rs` (the
  text metrics, the monospace family choice and the `Style` -> `RunStyle`
  mapping — the colours themselves are `shale_core::theme::Palette`),
  `lsp/` (the language-server client: `mod.rs` the `Spell` entity — process,
  handshake, one open document, debounced whole-document sync, and the answers to
  the requests the *server* sends the client; `wire.rs` the JSON-RPC framing, where
  `Content-Length` counts bytes; `mapping.rs` the UTF-16-position to buffer-byte
  translation) and
  `tree.rs`.
- `crates/mcp` (`shale-mcp` binary): MCP stdio server exposing list/read/
  write/search/backlinks tools.
- `refs/` (**gitignored, not shipped**): local copies of the Vim help files
  (`motion`, `visual`, `change`, `insert`, `editing`, `pattern`, `repeat`,
  `quickref`, `undo`, `options`) and ~57 Obsidian help pages, fetched 2026-10-26
  so behaviour can be checked without network access. Sources and licensing in
  `refs/README.md`: the Obsidian help repo grants **no** license, so those pages
  are specification only — never copy their prose or assets into Shale.
- `tools/` (**not shipped, Python**): `theme_gen.py` generates the seven
  third-party palette modules in `crates/core/src/theme/` from upstream colour
  names, and runs the same WCAG contrast checks as the crate's readability test
  before writing anything, so a palette that would fail that test is reported
  instead of emitted — including `match_text`, the one role judged against the
  picker's two backgrounds rather than against the note's. Running it and then
  `cargo fmt --all` reproduces those files byte for byte; `git diff` on them is
  the check that tool and crate agree.

### GPUI gotchas (learned the hard way — re-read before touching the UI)

- **Never use crates.io `gpui` 0.2.2.** Its Wayland backend requests a
  `wl_surface` frame callback exactly once (at first configure) and then depends
  on the compositor's callback chain; under niri that chain stalls, so the app
  keeps drawing new frames (render logs prove it) but never presents them — the
  window looks frozen at its first frame and only updates when the compositor
  forces a full redraw (resize, remap). Zed's tag `v1.25.3` has the rewritten
  frame-loop state machine in `gpui_linux` and works. `gpui_platform` /
  `gpui_linux` are **not on crates.io**, so the git pin is load-bearing.
  Debug aid: `--open-after <secs>` opens a note after the window exists, which is
  the repro for that bug.
- **Views must register their input handler during paint.** Typed printable
  characters are not keymap actions; gpui hands them to whichever handler was
  registered with `window.handle_input(&focus, ElementInputHandler::new(bounds,
entity), cx)` while painting the current frame. Forget it and every character
  is silently dropped while bound keys (enter/backspace/dd/x) keep working —
  exactly the symptom we hit. `LineElement::paint` does this once per frame
  (guarded by `MarkdownEditor::input_registered`, reset in `render`).
- **Linux keystrokes: shifted symbols arrive without shift.** gpui's
  `keystroke_from_xkb` maps a shifted symbol to the symbol itself and drops the
  shift modifier (shift is only kept for uppercase letters). So `"shift-4"`
  never matches; bind `"^"` and `"$"` literally, exactly as Zed's
  `assets/keymaps/vim.json` does. `"shift-g"` is correct for `G`. This is why
  `^`/`$` looked "not implemented" — the bindings simply never fired.
- **Counts are actions, not text input.** `1`..`9` are bound to `VimDigit1..9`
  in the `vim-normal` context and fed to the engine as `Key::Char('1'..)`; the
  engine builds the count and applies it to the next motion/operator (counts on
  both sides of an operator multiply, like vim). `0` stays the first-column
  motion and only extends a count that is already in progress. Zed does the same
  thing with a data-carrying `vim::Number(usize)` action; that derive needs
  `Deserialize` + `JsonSchema` (i.e. a `schemars` dependency), so we use ten
  unit actions instead.
- Other API notes for this version: `gpui_platform::application()` instead of
  `Application::new()`; `ShapedLine::paint` takes `TextAlign` + `align_width`;
  `Window::focus` takes `cx`; `TextRun.len` and `ShapedLine::x_for_index` are
  **utf-8 byte** offsets, while the input-handler range methods are utf-16.
- **Overlays, fonts and the two x spaces.** There is no `.absolute()/.left()/.top()`
  on `Styled` in v1.25.3 — something drawn over something else is
  `gpui::anchored().position(window_point).position_mode(AnchoredPositionMode::Window)`.
  `Styled` has `font_family`, **not** `text_font`, and a `TextRun` carries a `Font`
  but no size: the size is whatever text style the element is nested in, so a panel
  sets `font_family`/`text_size` on its root and lets its rows inherit. And a
  `HitLine`'s own x is measured **from the line's left edge** while a mouse event's
  is a **window** coordinate — the editor has margins and a table row starts further
  in again, so mixing them puts the cursor (or a hover) a few characters off. See
  `MarkdownEditor::offset_at_x` (line-local) vs `offset_at_pointer_x` (window).

### Testing

- `crates/core`: pure-Rust unit tests (vault, markdown, search, buffer, vim).
- `crates/app`: headless UI tests via gpui's `test-support` feature
  (`#[gpui::test]` + `VisualTestContext::simulate_keystrokes/simulate_input`),
  which is how input-path regressions get caught without a live window.
- Manual Wayland checks: `grim` for full-screen, `niri msg action
screenshot-window --id <id> --write-to-disk true` for a single window, and
`niri msg --json windows` to find the window id.
- Run the test binaries under `ulimit -v` when working on anything that scans
  text (a scanner bug can allocate until the OOM killer takes the editor down —
  see the table postmortem).
  Current peak RSS of the test binaries: core 6 MB, app 47 MB, mcp 3.5 MB. The
  app binary's *address space* (not RSS) is far larger because of gpui/wgpu
  reservations, and it now needs **4 GB or more** — a 1 GB cap makes it die in an
  80-byte allocation. Build with `cargo test --workspace --no-run`, then run each
  `target/debug/deps/<name>-<hash>` binary under `sh -c 'ulimit -v N; /usr/bin/
  time -v ./binary'`; never wrap `cargo test` itself, and re-derive the hashes
  after a rebuild.
- The headless harness (`tests/mod.rs::workspace`) turns `[spellcheck]` **off**.
  It would otherwise start a real `harper-ls` per window: several hundred
  megabytes of other people's dictionaries across the suite, and a gpui test
  scheduler that drops a still-pending local task from another thread and
  panics. `tests/spell.rs` covers the window's side with a command that cannot
  start, and `lsp/`'s own tests cover the protocol without a server.

### Open threads (write findings here as they come up)

- **Live preview stage 2a — the markers are hidden.**
  `shale_core::mdlayout` answers the two questions the paint needs:
  `hidden_ranges` (which bytes of a row to leave unpainted) and `visible_pieces`
  `visible_pieces` (which remain). `is_marker` hides a heading's `#` (and the space after it, so a
  heading is not indented), a blockquote's `>` (but *not* its space, which is what
  leaves the quote visibly indented), every `Style::Punctuation` — emphasis
  delimiters, a code span's backticks, a link's brackets, an escaped marker's
  backslash — plus `LinkUrl` and the new `LinkTarget`, the `target|` of an aliased
  wiki-link, and a thematic break's dashes (which the line element replaces with a
  rule across its width). Deliberately kept: list bullets and numbers, task
  checkboxes, `#tags` and the ``` fence lines (the only sign of where a block
  starts).
  `mdstyle` had to change to make that expressible: the backticks of a code span
  and the brackets of a wiki-link are now `Punctuation` spans *outside* the
  content, and `[[target|alias]]` reports the alias as the link. So the note reads
  `Welcome` rather than `[[Welcome]]`, and `an aliased link` rather than
  `[[alias|an aliased link]]`.
  **The rule:** a row is active — its markers stay visible — when the cursor is in
  it or the selection touches it; every other row is painted as its reader sees it.
  That is Obsidian's behaviour and it is what keeps the syntax editable. Hiding is
  only a paint decision: the buffer, the undo history and the file keep every
  byte, and `live_preview_hides_markers_off_the_cursor_row` asserts both halves.
  **Two coordinate spaces.** A painted line is no longer one contiguous buffer
  range, so `preview::PieceMap` (`buffer start, buffer end, painted index`) is the
  only way between them. `LineLayout::build` makes the painted string from a
  display line's pieces; `rebase_spans`/`rebase_links` clip the scanner's spans
  onto it (a span across a hidden marker becomes two, in order, so the scanner's
  "later span wins" rule still holds); and every `HitSegment` carries a map, which
  is what keeps `x_for_offset`/`offset_for_x` — clicks, IME placement, the cursor
  and selection quads — exact. A hidden marker's offset resolves to the character
  after it, so clicking where the `**` used to be lands on the word
  (`a_click_lands_on_visible_text`).
  **Wrap model:** `display_lines` wraps the *visible* text and maps each
  `Boundary.ix` back through `offset_for_visible_index`, so a break can never fall
  inside a hidden range; a break exactly at a piece seam belongs to the piece that
  follows, which leaves the hidden gap on the line above. The wrap cache is now
  per row and keyed on whether that row's markers were hidden, so moving the
  cursor re-wraps only the two rows whose state changed rather than the note.
  **Tables:** the grid paint still receives the raw row (it decides what of it to
  draw) but builds each cell from the row's hidden ranges, so a cell shows
  `Welcome`, not `[[Welcome]]` (`a_table_cell_hides_its_markers`). Consequence to
  revisit: `table_view::column_widths` still measures the *raw* content, so a cell
  containing markers gets a slightly wide column; measuring the painted text is part
  of the wide-glyph/display-column rework.
  **Done since (see the stage 2b bullet below): callouts, the fence lines of a
  code block, and inline images.**

- **Live preview stage 2b — callouts, collapsed rows, inline images — done.**
  - **Callouts** (`> [!warning] Watch out`). `mdstyle` gained `CalloutKind`
    (Obsidian's types plus its aliases: `caution`→Warning, `faq`→Question, …,
    and `Other` for anything it does not know) and two styles:
    `Style::Callout { kind }`, one span per line of the box, and
    `Style::CalloutTitle { kind }` for the header. A callout is a blockquote
    whose first line opens with `[!name]` (an optional `-`/`+` fold marker) and
    it runs until the first line that is not part of a quote. `[!` and `]` are
    emitted as `Punctuation`, so live preview hides them like any other syntax,
    but **the type name is kept and painted as the title** — Obsidian capitalises
    it ("Note"), we show the buffer's own bytes, which is what keeps the painted
    text mappable to buffer offsets. Painting: `RowStyle.callout` carries the
    kind, `theme::callout_accent(kind)` picks the colour, and `LineElement` draws
    a full-width `CALLOUT_BG` quad plus a 2 px accent bar on the line's left edge
    (`a_callout_is_painted_as_a_box` checks the box follows the block and that the
    header's brackets come back on the cursor's row).
  - **Collapsed rows.** `Buffer::set_display_segments` now distinguishes a row
    with **no entry** (one unwrapped line) from a row with an **empty vec**, which
    means the UI does not draw it at all: it contributes no display line, so
    `display_line_count`, `display_range`, `display_index_for` and the vertical
    motions step over it, and an offset inside it resolves to the line after
    (`buffer::tests::a_collapsed_row_takes_no_display_line`).
    `MarkdownEditor::collapsed_rows` marks two kinds: the ``` lines of a fenced
    block the cursor is not inside, and a table's `---` delimiter row. Both come
    back the moment the cursor is on them (for a fence, anywhere in its block;
    for the delimiter row, also when the cursor is on the header above it — see
    the delimiter-row note below), which is what lets the user still type a new
    table's delimiter row or edit a fence. Consequence for tables: with the
    delimiter row gone, the rule under the header is painted along the header
    row's bottom edge (`TablePaint::rule_below`), so a table no longer shows an
    empty band under its header. The wrap cache gained `collapsed` and `no_wrap`
    flags so a cached measurement is never reused for a row whose state changed.
  - **Inline images.** `markdown::line_image_embed` recognises a row that is
    *nothing but* an embed — `![[att.png]]`, `![[att.png|300]]`,
    `![[att.png|300x200]]`, `![alt](path =300x200 "title")` — and returns the
    target plus any size suffix. `Vault::list_attachments` (everything that is
    not a `.md`) and `Vault::resolve_attachment` (exact path → case-insensitive
    path → file name or stem, shortest path winning) do the Obsidian-style name
    lookup. `crates/app/src/image.rs` turns those into a per-row `ImagePlan`, and
    `fit_size` sizes the picture: the note's size suffix or the file's own size,
    scaled down to the editor's width and `MAX_HEIGHT` (600 px), never enlarged.
    The editor is given the vault and its attachments with
    `MarkdownEditor::set_vault`; each frame it asks gpui with
    `window.use_asset::<ImgResourceLoader>(Resource::Path(..))` (which decodes in
    the background and redraws when the file arrives) and `LineElement` paints it
    with `window::paint_image`. An image row never soft-wraps, its display line
    is as tall as the picture, and its painted text is empty with an identity
    `PieceMap`, so clicking the picture puts the cursor on the row and the source
    comes back. Verified live: `![[demo.png|300]]` in the demo vault draws the
    320×180 png 300 px wide.
    Still open: an embed inside a paragraph (only whole-row embeds are drawn as
    images), and a `|alias` suffix on an embed is read as "no size".

- **Tables — done: model, editing, and the grid widget.**
  `shale_core::table` holds the model (`table/mod.rs`: parsing, cell lookup,
  offset-to-cell resolution) and the edits (`table/edit.rs`: add, drop and move
  rows and columns, and where the cursor lands afterwards);
  `mdstyle::table_row_flags` and
  `mdstyle::is_table_delimiter_row` are public so the styler and the editor ask
  the same question and can never disagree about which lines are a table. A
  `Table` is `rows` + `columns` + the index of the delimiter row. Each `Cell`
  carries three ranges: the **content** (`start..end`, padding and pipes out),
  the **region** between the pipes (`span_start..span_end`, padding in) and the
  **pipes** (`open_pipe` / `close_pipe`). Navigation uses the pipes: the pipe
  that *closes* a cell belongs to that cell, the pipe that *opens* it belongs to
  the cell on its left, so every offset in a row resolves to exactly one column
  (`every_offset_in_a_table_row_resolves_to_a_cell`). That total ownership is
  what prevents the "weird skipping around" the user has seen in other apps.
  Ragged rows get synthetic empty cells (`Cell::is_synthetic`) so the column
  count never varies between rows. Edits **splice** rather than rewrite: dropping
  a column removes exactly that cell's region plus its closing pipe, inserting
  one adds `  |` (or ` --- |` on the delimiter row, which keeps the row a valid
  delimiter), so a hand-aligned table stays aligned where the user left it.
  Every edit re-positions the cursor through `landing()`, which re-identifies the
  table by the buffer row it starts on and clamps the requested `(row, column)`:
  deleting the last row or column selects the cell that took its place, and if
  the table itself is gone (its delimiter row was deleted) the cursor is left
  where the edit put it rather than jumping to some other table.
  **Postmortem — this cost the user an OOM kill.** The first cell scanner pushed
  a boundary for every `|` but never advanced past it, so *any* line containing a
  pipe looped forever allocating until the kernel killed the editor at 20 GB. The
  fix is one `i += 1`; `table::tests::a_row_of_only_pipes_terminates` is the
  regression test. Run these binaries under `ulimit -v` when touching this area —
  peak RSS afterwards: core 4 MB, app 19 MB, mcp 3 MB. Note that the **app** test
  binary needs more than 600 MB of *address space* (gpui/wgpu reservations, not
  touched pages), so cap it at 1 GB or more; the core binary runs fine at 500 MB.
  **Bindings (implemented in the vim engine + keymap):** `Shift+H`/`Shift+L`
  column left/right, `Shift+J`/`Shift+K` row down/up, `dd` delete row, `DD`
  (`shift-d`) delete column, `Shift+I` empty the cell and insert, `o`/`O` insert
  an empty row below/above, `|` insert a column after, `\` insert a column
  before, and **`alt+h`/`alt+j`/`alt+k`/`alt+l` move a whole row or column**
  (`table::move_row`/`move_column`: the columns or rows between the cursor and
  its destination rotate by one, the delimiter row never moves, and the cursor
  follows what moved). `J`/`K` inside a table step **cell to cell** in the same
  column, which is what the user asked for after other apps skipped around
  (plain `j`/`k` went back to being ordinary line motions later — see the
  delimiter-row note below);
  `A` appends at the cell's end rather than the row's. A find that would land
  outside the cell counts as "not found" (so `f|` works — the closing pipe
  belongs to the cell — and `d3x`-style sweeps stop at the content edge, as do
  `dw`/`d$`). `H/J/K/L` are bound in the visual context too, where they extend
  the selection; `D`, `|`, `\` and `alt+hjkl` are normal-mode only, since `D` in
  visual is the selection operator. **Caveat:** `D` is not `d$` yet (it was
  unbound before), so if we want vim's `D` it needs a different key or a
  table/non-table split. As with every new bound key, the half-typed-command
  path was checked: `f|`, `f\`, `fH` arrive through the pending-find arm, which
  runs before the table match. `alt+hjkl` arrives as the plain letter with the
  alt modifier (xkb leaves a letter's keysym alone under alt, unlike shift),
  mapped to `Key::AltChar(c)`; `keymap_matches_the_linux_keystroke_shape` pins
  that shape. If the user's compositor uses Alt as its modifier key these bindings
  will never reach the app — rebind rather than debug.
  **Painting.** A table row is *not* painted as markdown: `table_view::plans_by_row`
  turns each table into a grid in **character cells** (the widest content per
  column plus a one-cell gutter on each side), and `LineElement` paints each
  cell's content at its column offset with vertical rules on every boundary and a
  shaded header row; the delimiter row is collapsed out of the display model and
  the rule under the header is painted in its place. Consequences worth
  remembering:
  - A table row **never soft-wraps** (`display_lines` gives it one segment), so
    the buffer's display model and the paint stay in step;
    `a_table_is_painted_cell_by_cell` asserts one display line per row and one
    painted piece per cell.
  - Because the painted text is no longer one string, a painted line is recorded
    as `HitSegment`s (one per cell, or one for an ordinary row) and every x <->
    offset question goes through `HitLine::x_for_offset` / `offset_for_x`. The
    cursor and selection quads are computed per cell.
  - Geometry is in character cells, so it is exact for the monospace family the
    editor resolves but not for wide glyphs (CJK/emoji) — the same limitation as
    the tab/display-column work above.
  - A table wider than the window overflows: there is no horizontal scroll yet
    (`HitLine::width` is kept for exactly that).
  - Verified live (screenshot of `examples/demo-vault/Markdown Tour.md`): aligned
    columns, grid rules, shaded header, no visible pipes or dashes, and the
    wiki-link and tag styling still applied inside cells. The cursor landing in a
    cell still needs a check by the user (no key-injection tool is installed).

  `column_widths` counts characters, not display cells, so CJK/emoji columns
  will not line up until the tab/width rework noted above.

- **Table polish round (the user's feedback) — done.**
  - **Character motions step cell to cell.** `h`/`l`/`w`/`b`/`e` inside a cell
    are no longer "undone at the boundary": at a cell's edge they step into the
    neighbouring cell, and they never come to rest on a cell's padding or on a
    pipe (`vim::snap_cell_motion`). Which end of the neighbour they land on
    follows the motion: `l`, `w` and `b` arrive at its first content character,
    `h` and the inclusive `e` at its last; at the edge of the row the cursor
    stays on its own cell's content instead of escaping into the gutter. `0` now
    lands on the cell's first content character like `^` (it used to land on the
    padding), and `$` on its last.
  - **`J`/`K` can leave the table.** They used to be captured as cell-to-cell
    motion and clamped, so the cursor was trapped. `step_cell_rows` walks the
    table's *navigable* rows and, when the count runs past the top or bottom,
    comes to rest on the edge row and hands the leftover steps to the ordinary
    line motion, so the cursor walks out (`j_and_k_walk_out_of_the_table`).
  - **The delimiter row is not a cell.** `Table::cell_at` refuses every offset on
    it, `cell_clamped` snaps a request for it to the nearest row with cells (the
    one below, else the one above), and `navigable_rows` is what the shifted
    vertical motions walk, so `J`/`K` never land on the rule. (Both of those
    changed afterwards: plain `j`/`k` are ordinary line motions again and *can*
    stop on the row once live preview draws it, and `table::clickable_offset`,
    which snapped a click on the painted rule into the cell below it, was removed
    — see the delimiter-row note below.) Consequence: `dd` can never target the
    rule, and if a cursor does end up there the engine treats that line as
    ordinary text.
  - **One table edit = one undo step.** `delete_column`, `insert_column`,
    `move_row` and `move_column` splice every row, so they open a buffer undo
    group; `Buffer::delete_range`/`insert_at` now use `snapshot_for_edit`, which
    joins an open group instead of closing it (`a_table_edit_is_one_undo_step`
    checks undo *and* redo). `dd` and `shift-i` were already single edits.
  - **`---` is painted as a rule.** `mdlayout::is_marker` hides `Style::Rule`, so
    a thematic break's dashes are hidden on any row the cursor is not on, and
    `LineElement` draws a `TABLE_LINE` quad across the full width of the line in
    their place (like the fenced-block background). `RowStyle.rule` carries the
    flag from the spans; the dashes come back on the cursor's own row so they
    stay editable (`a_thematic_break_is_painted_as_a_rule`, and verified live:
    the rule spans the editor's width at the row's vertical centre).
  - **Wiki-links do not span lines** — in Shale and in Obsidian. The scanner is
    line-based (`markdown::tests::multiline_link_ignored` pins it), so a link
    broken across two source lines is plain text. `examples/demo-vault/Markdown
    Tour.md` had exactly that and now keeps each link on one line; if a note ever
    looks like its links stopped resolving, check for a line break inside `[[…]]`
    before debugging the parser.
  - Still to check in a live window (no key-injection tool is installed, so this
    needs the user): cell-to-cell `l`/`h`, `H`/`L` landing on content, `j`/`k`
    escaping the table, the rule line being skipped, single-step undo for `DD`
    and `alt+hjkl`, and whether the user's compositor swallows `alt+letter`.

- **Font family: `font("monospace")` does not work on Linux.** gpui's Linux text
  system (`gpui_wgpu::CosmicTextSystem`) matches a family name *literally*
  against the installed font families, and no family is named `monospace`, so
  `load_family` finds nothing, `TextSystem::resolve_font` falls through to its
  built-in `fallback_font_stack`, and the user's system answered **Adwaita Sans**
  — a proportional UI font. Worse, that fallback path resolves the *fallback*
  font, so the requested `weight` and `style` are dropped: `font(...).bold()` and
  `.italic()` returned the same `FontId` as regular text, which is why bold and
  italic looked identical. Fix: `theme::pick_mono_family` matches the config's
  `appearance.font_family` (empty = auto) against
  `window.text_system().all_font_names()` and falls back to a preference list of
  real monospace families, ordered so families with genuine bold **and** italic
  faces come first (JetBrains Mono, DejaVu Sans Mono, Adwaita Mono, …) — gpui
  never synthesizes a missing face, and Noto Sans Mono (the usual fontconfig
  `monospace`) has **no italic**, so italic emphasis would still be invisible if
  it were picked first. The resolved family is used for the text style *and* the
  line wrappers, and is logged once at debug (`editor font family family=…`);
  if nothing matches, a warning says text will use gpui's fallback. Verified
  live: JetBrains Mono, and bold/italic are now clearly drawn. Note that for a
  monospace family the bold and regular advances are identical (8.4px at 14pt),
  so a width comparison cannot detect bold — check pixels, not metrics.
- **Live preview, stage 1 (syntax styling) — done.** `shale_core::mdstyle::
  style_spans(text)` returns `StyleSpan { start, end, style }` over the whole
  buffer: absolute byte offsets on character boundaries, sorted by `(start,
  end)`, with block spans (heading, quote, fenced code) emitted *before* the
  inline spans inside them so a consumer that applies them in order ends up with
  the inline style winning. It is line-based and deliberately pragmatic (the
  subset Obsidian renders, not full CommonMark); fences and inline code spans
  suppress inline markers; a GFM table is only recognised when a delimiter row
  containing a pipe is present. The scanner knows nothing about painting —
  `theme::RunStyle::for_style` is the only place a `Style` becomes colours and
  fonts, which is also where a TOML theme will hook in.
  Painting (`line.rs::build_runs`): each display line gets a **per-byte**
  `Vec<RunStyle>` (default plain, spans applied in order, then the resolved /
  unresolved wiki-link colour over the top), and adjacent equal styles are
  coalesced into `gpui::TextRun`s. `TextRun.len` is a utf-8 byte count, so a
  multi-byte character just contributes several bytes and needs no special
  handling. **The run lengths must tile the line exactly** — gpui matches runs
  against glyphs by byte count — which `line::tests::runs_tile_the_line`
  asserts. Run *backgrounds* (inline code, `==highlight==`, fenced blocks) need
  a separate `ShapedLine::paint_background` pass before `paint`; `paint` alone
  only draws glyphs, underline and strikethrough.
  **Wrap-model agreement (re-check this before changing anything):** bold and
  italic runs measure wider than the regular face the wrapper uses, so
  `display_lines` picks the wrapper **per row** — a row containing a heading /
  strong / quote span is wrapped with `font("monospace").bold()` (`wide_rows()`
  computes the flags in one forward walk, since spans and rows are both in
  offset order). Over-measuring is safe (the row just breaks earlier);
  under-measuring makes a painted segment overflow the wrap width and puts j/k
  out of step. `tests::styled_markdown_keeps_the_wrap_accurate` holds that
  invariant, and `tests::editing_a_styled_note` covers the style cache (keyed on
  `Buffer::change_id`, like the wrap cache) after edits.
  **Sizes and blocks (added on top of stage 1):** heading rows are painted
  larger (`theme::font_size(level)`, and `line_height(size)` keeps the body
  line's leading ratio, so a display line's height follows its size). Because
  wrapping must agree with the paint, `row_styles()` computes one `RowStyle`
  per buffer row — `wide` (a face wider than regular), `heading` (the largest
  level on the row) and `block` (inside a fenced block) — and the frame builds
  one `LineWrapper` per (level, wide) pair, indexed by `wrapper_index`. A fenced
  block's background is a full-width quad painted by the line element rather
  than a run background, so it reads as a block; `mdstyle` therefore distinguishes
  `Style::CodeBlock` (fenced content) from `Style::Code` (inline spans).
  `tests::headings_are_painted_larger` checks a heading row is both wider and
  taller than a body row.
  Deliberately *not* done yet (stage 2): hiding markers, inline images, callouts,
  and any per-run font size (`shape_line` takes one size per line, which is why
  sizes are per display line, not per run).
- **Soft wrap — done.** `Buffer` holds a _display model_: the UI calls
  `Buffer::set_display_segments(rows_of_absolute_byte_starts)` each frame, and
  every vertical motion (`j`/`k`, `desired_col`) works on flat display-line
  indices, so wrapped rows step like neovim and the core stays unit-testable
  without any UI. The app gets break points from
  `window.text_system().line_wrapper(font, size).wrap_line(&[LineFragment::text(
row)], width, IndentAdjustment::NoIndent)`, whose `Boundary.ix` is a **byte
  offset into the row** (add `line_start(row)` for absolute). Boundaries are
  cached per `(buffer change_id, width)`; `Buffer::change_id()` bumps on every
  mutation (including edits inside an undo group — forgetting that produced
  slices containing `\n` and a `shape_line` debug_assert panic).
  Wrap width comes from `scroll_handle.bounds().size.width`, i.e. the **previous
  frame's** layout: the first frame has 0 width (no wrap) and a resize takes one
  frame to catch up. The scroll container's width is definite (viewport minus
  sidebar), so there is no content-driven feedback loop — verified live: 940px
  transient then a stable 1781.6px in a 2042px viewport.
  **Measurement is exact, verified by test** (`wrap_model_versus_shaped_widths`,
  `wrap_stays_accurate_for_tabs_and_unicode`): `fc-match monospace` is Noto Sans
  Mono (8.4px/char at 14px), `LineWrapper`'s per-character sum equals
  `shape_line().width()` to the last digit, and every painted segment fits the
  wrap width (widest 1915.2px of 1920px) even with tabs, accents, CJK and emoji.
  gpui gives one cell per character, so a **character** column is the column the
  user sees. `display_lines()` now hands the buffer _exactly_ the segments it
  paints (previously the buffer got raw cached segments and the paint re-validated
  them — two index spaces that could desync the cursor's row); the tests assert
  the model's ranges equal the painted ranges.
  Screen vs logical motions (vim's own split): `0`, `^`, `$` (and `d$`, `c$`)
  are **display-line** motions; `A` and `I` stay **row**-wise
  (`Buffer::row_first_non_blank` / `row_last_char_motion`). `desired_col` is a
  character column within the display line, and the ruler reports character
  columns.
  Still open in this area:
  - **j/k is still slightly off in a live window** (user-confirmed after the
    character-column fix). The headless invariants all pass, so the suspect is
    the _width_ the break points are computed from rather than the character
    math: `LineWrapper` sums per-character advances while painting uses shaped
    lines, and the width itself is the previous frame's container bounds. Rework
    later: drive the display model from shaped geometry (an x-offset based
    desired column resolved with `closest_index_for_x`, Zed-style) instead of
    character counts, and take the wrap width from the current frame's layout.
    **Big suspect found since:** the editor was not being drawn in a monospace
    font at all (see the font bullet below), so character columns were not
    visual columns. Re-test j/k now that the family resolves to a real mono font
    before doing the shaped-geometry rework.
  - The buffer row is no longer visible anywhere (the ruler shows the display
    row, as vim does).
  - Tabs render as one cell in gpui, so a tab-indented note looks different from
    vim/neovim; if we want real tab stops the wrap and the column math both need
    to expand them.
- **ctrl+letter keys: the keymap used `+` instead of `-`.** Fixed. gpui's
  keystroke syntax separates modifiers with `-` (`ctrl-z`); `Keystroke::parse(
"ctrl+z")` does _not_ error, it returns one literal key named `"ctrl+z"` that
  can never match what the platform delivers. That silently killed **every**
  ctrl binding: undo/redo, `ctrl-s` save and `ctrl-q` quit. The headless test
  passed for ages because `simulate_keystrokes("ctrl+z")` parses with the same
  parser as the binding, so the typo matched itself. `tests::
keymap_matches_the_linux_keystroke_shape` now dispatches the platform's real
  keystroke shape (`key: "z"`, `control: true`, `key_char: None`) straight
  through `Keymap::bindings_for_input`, which is how a keymap mistake gets
  caught without a live window.
  Platform facts worth keeping: `keystroke_from_xkb` maps ctrl+letter to the
  letter (`key_utf32 <= 0x1f` → `+0x40`, lowercased) and sets **no** `key_char`
  for control characters; `prefer_character_input` is always `false` in
  `gpui_linux`, so `Window::input_preference()` is always `KeyBindings` and can
  never be the reason a binding is skipped. A keystroke observer logs
  `action = None` both when no binding matched _and_ when a binding matched but
  no `on_action` listener consumed it — so "no action" does not by itself mean
  "no binding". That observer now logs at **trace** (it fires for every
  keystroke and flooded the console at debug): use `RUST_LOG=shale=trace` when a
  keybinding needs diagnosing, and keep it off for normal runs.
- **Undo granularity** is now: one insert session = one step, `cw`/`o` + typing =
  one step, each normal-mode operator = one step. Vim coalesces slightly
  differently; revisit if it feels wrong in practice.
- **j/k across wraps**: the reported "sometimes wildly off" could not be
  reproduced — the wrap model is provably identical to what is painted (see the
  soft-wrap bullet), and gpui's `scroll_to_item` only scrolls the minimum to keep
  the line visible. The two real defects found on the way were the byte-column
  `desired_col` and the model/paint index spaces, both fixed; the logical `$`/`0`
  (which jumped to the end of the _row_, several rows down) is the most likely
  thing the report was describing. Needs a re-test in a live window.
- **Text objects — done** (`iw`/`aw`, `ip`/`ap`, with `d`/`c`/`y` and counts).
  `Buffer::word_object` / `paragraph_object` return the range; the engine holds a
  half-typed `Pending::TextObject { op, count, around }` so the operator resumes
  when the scope key arrives. `i`/`a` are already bound (`VimI`/`VimA`), so no new
  keymap entries were needed. A count before the scope (`d2aw`) extends the range
  over the following objects. `aw` follows vim's whitespace rule exactly: trailing
  run if there is one, otherwise the leading run (so `daw` on the last word of a
  line eats the space _before_ it), and on whitespace it takes that run plus the
  word after.
- **Visual mode — done** (`v` charwise, `V` linewise). Motions extend the
  selection, operators act on it and drop back to normal, `o` swaps the ends,
  `i`/`a` extend it with a text object, `I`/`A` insert at the near/far line,
  `d`/`x`/`c`/`s`/`y`/`p` all work, escape and `v`/`V` exit or switch form.
  **Selection model**: the buffer keeps `(anchor, head)` where `head` _is_ the
  cursor, and a charwise visual selection is **inclusive** of the character under
  the cursor (vim's model). `Vim::selection_range(&buffer)` (public) turns that
  into the exclusive byte range used by operators, painting and `ctrl-shift-c`;
  the editor paints from it rather than from `buffer.selection()`, which would
  paint one character short. Motions run with the selection **cleared** (they
  measure from the cursor, and `move_left`/`move_right` would otherwise jump to
  the anchor) and are then re-anchored; a text object sets its own selection and
  must not be re-anchored.
  The `vim-visual` **key context needs the motion bindings**: `keybindings()` now
  binds the shared list for `["vim-normal", "vim-visual"]` and keeps `u` (undo) and
  `enter` normal-only, since vim uses `u` in visual for lowercasing. The keymap
  regression test checks the visual context too; the platform's shape for `V` is
  `key: "v"` with `shift: true` (not `key: "V"`).
- **Motion inclusivity follows vim's docs** (vimhelp.org `motion.txt`, worth
  re-reading before "fixing" anything here): `w`/`b`/`0`/`^`/`f`/`t` are
  **exclusive**, `$` and `e` are **inclusive**. So `w` lands on the _start of the
  next word_ — not on the gap between words — which is why `dw` deletes the word
  plus its trailing space and why `vw` reaches one character into the next word.
  (An attempt to make `w` stop on the gap was tried and reverted: it breaks `d2w`
  and contradicts the docs.) `de` was off by one character and is now inclusive.
- **`.` repeat — done.** The engine records the command being typed
  (`Vim::command: Vec<Cmd>` where `Cmd` is `Key(Key)` or `Text(String)`); when a
  command finishes and changed the buffer it becomes `last_command`, and `.`
  replays it through `handle_motion_key` (`Vim::repeat_last`). An insert session
  is part of the recording: the editor calls `Vim::note_inserted` from
  `insert_edit`, so `cw`+typed text and `o`+typed text replay as one change and
  `.` never leaves the engine in insert mode (the replay ignores `EnterInsert`
  and inserts the recorded text itself). Counts are recorded with the command;
  commands that change nothing are not recorded, so `.` after a plain motion is
  `Unhandled`.
  **Binding caveat (this bit us):** a key that is bound in the keymap can also be
  the _argument_ of a half-typed command — `fv` must find a literal `v`, `f.` a
  literal period. Unbound characters arrive through `replace_text_in_range`, but
  bound ones arrive as actions, so the engine has to route them: the Find and
  TextObject arms run before the motion match, the digit branch is skipped while
  `waiting_for_literal()`, `.` repeat is guarded by it, and `handle_visual_key`
  forwards straight to `visual_motion` when an argument is pending. Any new bound
  key needs that path checked.
- **Not implemented yet in vim mode**: `gv`, `C`/`S` in visual, repeating a text
  object in visual to extend it, blockwise `ctrl-v`, the case/replace operators
  (`~`, `u`, `r`) in visual, registers other than the unnamed one, insert-mode
  deletions/arrows inside a `.` replay.
- **Cursor model nits**: `Buffer` cursors may sit at `text.len()` (between
  characters) while vim's normal-mode cursor always sits *on* a character, so
  `w` at the end of a buffer can leave the cursor one position past the last
  character; normal mode should clamp. Related: `dd` on the last line leaves the
  cursor on the empty row the trailing newline leaves behind, where vim steps up
  to the previous line.

- **The table's delimiter row comes back next to the cursor.** The `---` row is
  still collapsed out of the display model (it takes no vertical space at all)
  *unless* the cursor or the selection touches it or the header row above it,
  which is where a cursor in the table usually is. When it is drawn it is painted
  as its own grid row, cells and all, so `:---`, `---:` and `:---:` are visible
  and editable (dim, from `Style::TableRule`), and the rule under the header is
  painted exactly once: along the delimiter row's **top edge** when that row is
  drawn, along the header's **bottom edge** (`TablePaint::rule_below`) when it is
  not. `table_view::plans_by_row` takes the frame's `collapsed` flags to decide
  which, so `render.rs` now computes `collapsed_rows` before the plans.
  - **`j`/`k` versus `J`/`K` inside a table** (the user's choice): the shifted
    forms are the cell-to-cell motions and skip the delimiter row
    (`step_cell_rows` walking `Table::navigable_rows`); plain `j`/`k` are *not*
    table commands at all and fall through to the ordinary line motions, which
    follow the display model — so they step onto the rule row exactly when live
    preview is drawing it, which is how the cursor reaches it to fix alignment.
    `Table::clickable_offset` (which snapped a click on the rule into a
    neighbouring cell) is gone: a collapsed row has no display line to click, and
    a drawn one is real text.
  - **The delimiter row is the floor of a column's width**: `column_widths` no
    longer skips it, so a column can never be narrower than its `:---:` and the
    drawn row cannot overflow into its neighbour.

- **Theming — done (first pass).** Colours are data, not paint sites.
  `shale_core::theme::Palette` is one struct of 0xRRGGBB *roles* (`bg`, `fg`,
  `dim`, `link`, `table_line`, `callout_warning`, `ui_status_bg`, …) with twenty
  built-in values — `Palette::DARK`, `Palette::LIGHT` and the family palettes in
  `theme/{catppuccin,tokyo_night,gruvbox,nord,dracula,solarized,one,rose_pine}.rs`
  — and `Palette::DEFAULT` (Mocha)
  is what a vault that names no theme gets; `callout_accent(kind)`
  reads its colours off the palette, so a theme can retune one callout type
  without touching the rest. `appearance.theme` in `shale.toml` names a built-in
  or a `<vault>/.shale/themes/<name>.toml` file, which sets `base` and overrides
  only the roles it cares about (`bg = "#282828"`); unknown keys and malformed
  colours are errors, and a theme that cannot be found or parsed is logged and
  falls back to `Palette::DEFAULT` rather than stopping the editor.
  `theme::palette_for` does the resolution, `Workspace::new` calls it once and
  shares the `Rc<Palette>` with the editor, which hands it to every
  `LineElement`.
  - **The app's `theme.rs` no longer holds colours.** It owns the parts that
    need gpui: `RunStyle::for_style(style, &palette)` / `RunStyle::plain(&palette)`
    (the single place a markdown `Style` becomes a painted run) and the text
    metrics. `DEFAULT_FONT_SIZE` is the fallback; `font_size(level, base)` scales
    every heading from the base, and `line_height(size)` keeps one ratio
    (`LINE_RATIO`) at every size.
  - **`appearance.font_size` is wired up.** The editor holds `font_size`, uses it
    for the wrappers, the char-cell width, the heading sizes and the scroll
    container's `text_size`, so a bigger base means bigger headings *and*
    correctly measured wrapping. `set_font_size` no-ops when the size is
    unchanged, which keeps the wrap cache valid.
  - **The window chrome is themed too**: the sidebar, its border, the hovered and
    active tree row and the status bar are `ui_*` palette roles, so a light theme
    is light end to end.
  - **Verified live** (screenshots): `theme = "light"` end to end, and a
    `gruvbox.toml` (`base = "dark"` plus `bg`/`fg`/`link`/`ui_bg`/`table_line`/
    `callout_warning`) at `font_size = 18` — the note, sidebar, table grid and
    callout bar all followed, and the headings scaled with the base size.
  - **Catppuccin added (2026-10-27).** The four flavours are built in, their
    hexes verbatim from `palette.json` v1.8.0 of `catppuccin/palette` (**MIT**,
    hence copyable into this Apache-2.0 crate), and every role is filled from
    Catppuccin's own style guide (`docs/style-guide.md` in `catppuccin/catppuccin`):
    Base/Mantle/Crust for the panes, Text for body and headings, Rosewater for the
    cursor, Blue for links and tags, Overlay 1 for the dim markers, Subtext 0 for
    quotes, the Surfaces for code/table/hover backgrounds, Yellow/Peach/Red for the
    warning/failure/danger callouts. `selection` and `highlight_bg` are the guide's
    accent-over-Base opacities composited to a flat colour, because a `Palette` has
    no alpha. Flavours resolve by name and with a `catppuccin-` prefix, and are
    legal `base` values in a theme file. **The default theme is now `mocha`**
    (`Palette::DEFAULT` / `Palette::DEFAULT_NAME`, used by the config default,
    `palette_for`, the editor's initial palette and the load-failure fallback).
    - The built-in contrast test now walks `Palette::BUILT_INS` instead of the two
      originals, and its `luminance` helper is real WCAG relative luminance (the
      sRGB channels linearized before weighting). The old shortcut weighted the raw
      0-1 channels and under-reported mid-tones so badly that Catppuccin Frappé's
      text scored 3.4:1 where it is really 8.1:1; the floors are unchanged.
  - **Seven more families added (2026-10-27).** Fourteen more built-in palettes,
    so twenty in all: `tokyo-night` / `-storm` / `-light`, `gruvbox-dark` /
    `-light`, `nord`, `dracula`, `solarized-dark` / `-light`, `one-dark` /
    `-light`, and `rose-pine` / `-moon` / `-dawn`. Each is its upstream's own
    values, not a port's: Tokyo Night from `lua/tokyonight/colors/` in
    tokyonight.nvim plus the workbench shades of the official VS Code themes
    (the origin repo moved from `enkia/` to `tokyo-night/`), Gruvbox from
    `colors/gruvbox.vim` — including the way it re-binds its own `bg0..bg4` /
    `fg0..fg4` names in light mode and its `Visual`/`LineNr`/`Comment` highlights,
    Nord from `src/nord.scss` and the use its four groups document for
    themselves, Dracula from its published eleven-colour table, Solarized from
    its tone ladder, Rosé Pine from `palette.json` (whose README also gives the
    `rose-pine-moon` / `rose-pine-dawn` spelling), and One by *evaluating* Atom's
    HSL definitions and the LESS shades it derives (`@syntax-gutter`, the
    `lighten(@syntax-bg, 10%)` selection, `@level-3-color`), which is how it lands
    on the expected `#282c34` / `#abb2bf` / `#3e4451` / `#21252b` without anyone
    copying a hex.
    - They are written by `tools/theme_gen.py`, which maps upstream colour
      **names** onto Shale's roles and refuses to emit a role that fails the
      crate's readability floors — all 14 pass, each ratio printed. Where a
      palette genuinely has no colour for a role the tool composites one and says
      so in the generated comment: Nord's quiet text roles (nord3 on nord0 is
      1.7:1), Dracula's chrome (its palette has no third surface), Solarized's
      code span and table header, and `selection`/`highlight_bg` throughout, since
      a `Palette` has no alpha. Atom's LESS `lighten()` cannot lighten a 98%
      background, so One Light's surfaces darken.
    - Names are lowercase ASCII file stems (so `frappe`, not `frappé`,
      and `rose-pine`, not `rosé`); every built-in is a legal `base` in a vault
      theme file, and the two spellings people actually type resolve:
      `catppuccin-` prefix as before, plus `-dark` on a family whose dark variant
      carries the bare name (`tokyo-night-dark`, `rose-pine-dark`). A bare
      `gruvbox` or `one` stays unresolved rather than guessing a mode.
    - Tests: the readability test now walks 20 palettes, and
      `the_other_families_are_built_ins` plus `the_built_in_list_is_a_set` cover
      the names, the aliases, `base`, uniqueness, and that every name is a plain
      file stem.
    - **A picker row's matched characters are a colour, not a box
      (2026-10-27).** Quick-open, the command palette and vault search used to
      wrap every matched character in its own `div` with `highlight_bg` behind
      it. That was ugly twice over: the row is a flex container with `gap_2`, so
      each single-letter box sat eight pixels from the next, and the box was the
      colour `==highlight==` uses in the note. A row is now **one
      `gpui::StyledText`** whose runs come from `picker::runs`: matched stretches
      in `Palette::match_text`, everything else in `Palette::fg`, no background.
      The matcher's adjacent single-character ranges are coalesced first, so a
      matched word is one run and keeps its kerning.
    - **`match_text` is a new `Palette` role** — 29 roles now, filled by
      `theme_gen.py`'s tables for the seven families and by hand for the
      Catppuccin flavours and `dark`/`light` — and a theme file can set it
      (`match_text = "#fab387"`). It is judged against the two surfaces a picker
      draws on rather than against the note: 3.0:1 on `ui_bg` (an ordinary row)
      and 2.5:1 on `ui_active_bg` (the highlighted row, where the rest of the row
      is still body text at full contrast). Both floors are in the crate's
      readability test and in the generator. Each family takes its warm accent —
      Tokyo Night's orange, Gruvbox yellow in dark and orange in light, Nord's
      nord8, Dracula's orange, Solarized's yellow and orange, One's `modified`
      and `orange`, Rosé Pine's gold — except where that colour cannot clear the
      floors: Latte falls back to its Mauve (its Peach is 1.6:1 on a highlighted
      row) and Rosé Pine Dawn to its pine (its gold is 1.5:1 on its own base).
    - **Next: a theme picker.** Not started — the user is specifying what they want
      from it first. Today a theme change means editing `shale.toml` and restarting.
  - **Not done here**: no way to change the theme at runtime (restart to pick one
    up), no per-role preview UI, and the palette has no entry for syntax
    highlighting inside fenced code (which is the tree-sitter goal anyway).

- **Folding (heading sections) — done (2026-10-27).** `shale_core::fold` answers
  the two questions the view needs: `fold_regions(text)` (a heading owns every row
  below it until the next heading of its own level or a shallower one, so regions
  nest; a `#` inside a fenced block is not a heading because the region scan goes
  through `mdstyle`'s spans) and `fold_view(regions, state, line_count)`, which
  turns the closed regions into per-row `collapsed` flags plus a `marker` count on
  the row that opens each fold.
  - **Folds live on the `Buffer`, next to `display_segments`, because both are
    view state.** The vim engine runs the commands (`za` toggle, `zc` close, `zo`
    open, `zd` remove, `zM` close all, `zR`/`zE` open all — `Pending::Fold` in the
    engine, and the command letter arrives through the action that key is already
    bound to, so only `shift-r`/`shift-m`/`shift-e` needed new bindings) and the
    display model reads the result. `Buffer::fold_view` is ORed into the rows live
    preview already hides, so a folded row takes no display line and `j`/`k`, the
    ruler, `ensure_cursor_visible` and the wrap cache all follow for free. That is
    the whole integration, and the buffer's text never changes.
  - **A fold is keyed on its heading's text, not its row or offset** (`FoldKey
    { text, ordinal }`, the ordinal telling repeated headings apart). Rows and
    offsets both move when the user types; the heading's text does not, so a fold
    survives edits anywhere else in the note and disappears if the heading itself
    is renamed or deleted. `set_text` (opening another note) clears them.
  - **A closed fold never hides the cursor.** `Buffer::clamp_cursor_out_of_folds`
    walks the cursor up to the heading that opens the innermost fold covering it,
    then repeats outward, so `zM` from the middle of a note lands on the outermost
    heading. Without that, `display_index_for(cursor)` would point at a row that
    has no display line at all.
  - **The marker is painted, not spliced into the text.** `LineElement { fold:
    Some(n) }` shapes `⋯ n more lines` as its own `ShapedLine` in prepaint and
    paints it after the row's text in `palette.dim`, because the marker has no byte
    in the buffer and must not enter the `PieceMap` that maps clicks and the IME
    back to offsets. It goes on the row's *last* display line, so a wrapped heading
    gets one marker. `HitLine::fold_marker` records where it was drawn and a click
    inside it toggles the fold (`input::on_click` checks it before the text
    hit-test) — the only pointer affordance, since there is no fold gutter.
  - **`zd` and `zE` coincide with `zo` and `zR`** here: a fold is a closed region
    derived from the headings, not a stored definition, so opening it *is* removing
    it. Remember that if fold state is ever saved and restored per note.
  - Verified live: `# Welcome` in the demo vault folded to
    `Welcome ⋯ 9 more lines` with the rest of the note gone. `za`/`j`/`k`/`zM`/
    `zR` and the marker click are covered headlessly (`tests/keys.rs`,
    `tests/paint.rs::a_folded_heading_is_marked_and_clickable`, which clicks
    through `simulate_click`).
  - **Not done here**: folding fenced blocks, callouts and tables (the region model
    is not heading-specific — `level` is the only field that knows about headings —
    so those are additive), fold state per note on disk, `zz`/`zt`/`zb` scroll
    commands, and `zn`/`zi` (enable/disable folding).

- **List, task and quote continuation on Enter — done (2026-10-27).**
  `shale_core::continuation` answers one question about a byte offset:
  `enter_action(text, offset, in_code)` returns `Continue { text }` (a newline
  plus the marker the new row should open with), `Clear { start, end }` (the row
  holds *nothing but* its marker, so delete it and leave the row blank — this is
  how a list ends without typing a second empty item) or `Newline`.
  - **The marker shapes are the scanner's.** `mdstyle::block::list_marker_end`
    and `task_checkbox` became `pub(crate)` and are re-exported from `mdstyle`,
    so live preview hides exactly the markers this continues and the two can
    never disagree about what a marker is. `row_marker` reads the row's indent,
    then any number of `>` markers (each with its optional single space), then a
    bullet / number and an optional task checkbox.
  - **Rules:** the indent is preserved, so a nested item continues at the same
    depth; a task always continues as `- [ ] ` whatever the row being continued
    held; a numbered item counts up and keeps its zero padding (`08.` → `09. `,
    `3)` → `4) `); a quote keeps its `>` markers, and a list inside a quote keeps
    both. `in_code` — the UI says the row is inside a fenced block — suppresses
    all of it, so a `-` in code stays a `-`.
  - **The app applies it in insert mode** (`editor/actions.rs::on_newline`): the
    marker text goes in through `buffer.insert` + `insert_edit`, so it joins the
    insert session's undo group and `.` replays it as ordinary typed text.
    `Clear` uses `buffer.delete_range` + `insert_edit("")`. `row_is_code` asks
    the style scanner for the row's spans instead of tracking fences a second
    way.
  - **Known limits.** A marker *removal* cannot be replayed by `.`
    (`note_inserted("")` records nothing), so `.` after a list-ending Enter
    repeats nothing; a nested item keeps the source row's indent rather than
    deepening; a callout's body continues as a plain `> ` and does not keep the
    callout header (Obsidian behaves the same way).
  - Tests: ten in `continuation.rs`, plus
    `enter_continues_list_task_and_quote_markers` and
    `enter_on_a_marker_only_row_ends_the_list` in `tests/keys.rs`. Undo finding
    from writing them: an insert session really is one group, so
    `i - one <enter> two <esc> u` empties the buffer rather than leaving `- one`;
    the test that checks the marker is not its own step uses `A <enter> <esc> u`.
    Needs a live check by the user (no key-injection tool is installed).

- **File tree: create, rename, delete — done (2026-10-27).**
  `shale_core::Vault` gained the checked edits: `create_note` (adds `.md`, never
  overwrites), `create_dir`, `rename`, `delete` and `delete_dir`. Every name goes
  through `checked_rel` (rejects `..`, absolute paths and `FORBIDDEN_NAME_CHARS`)
  and `guard_unprotected`, which refuses anything under a dot directory so
  `.shale` and `.git` stay untouched. `delete_dir` refuses a non-empty folder,
  and `list_dirs` exists so an empty folder still gets a row.
  **The prompt is a sibling of the tree, not a child.** gpui enables a binding
  when its context appears *anywhere* in the focused element's stack
  (`Keymap::binding_enabled`), so rendering the prompt inside `tree-scroll` would
  let the tree's `j`/`k` swallow the letters being typed. `prompt.rs` is a
  `Prompt` entity (`EntityInputHandler` + `Render` + `Focusable`) whose field is
  a custom `PromptField` element that registers an `ElementInputHandler` during
  `paint` — that is how typed characters arrive without any key binding. (An
  `Element` impl also needs `impl IntoElement` or `.child()` will not take it.)
  **Bindings.** Global `ctrl-n` new note, `ctrl-shift-n` new folder,
  `ctrl-shift-r` rename, `ctrl-shift-d` delete, `ctrl-b` focus the tree. In the
  tree context: `j/k/up/down`, `h/l/left/right`, `enter`/`space` open, `escape`
  to the editor, `n`, `shift-n`, `r`, `shift-d`. In the prompt context only
  `enter`/`escape`/`backspace`/`delete`/`left`/`right`, because printables come
  from the input handler and binding them would open a second path to the same
  text.
  **Decisions worth remembering.** A delete happens only when the answer is
  exactly `yes` (case-insensitive); anything else closes the prompt with "delete
  cancelled". An *invalid* name leaves the prompt open with the text in it so it
  can be fixed: `apply_prompt` returns `Result<()>` and `answer_prompt` handles
  the declined delete, because `?` only works in a `Result`-returning fn.
  Closing a prompt returns focus to whatever held it (`prompt_return_focus`), so
  a rename started in the tree keeps you in the tree. Rename prefills the file's
  *stem* and `sibling_path` puts the `.md` back. New notes and folders land in
  `target_dir()` — the selected folder, else the folder of the selected note,
  else the folder of the open note, else the vault root. Every folder starts
  expanded.
  **Not done here.** Renaming or deleting does **not** rewrite the wiki-links
  that point at the note (Obsidian does rewrite them); that needs a decision
  about aliases and embeds first. File watching is still missing, so an edit
  made outside Shale is invisible until restart.
  **Test findings.** `WindowHandle::update` plus `window.dispatch_keystroke` in
  the same closure panics ("already being updated"), so a live keystroke sequence
  cannot be driven that way. And because focus returns to the editor when a
  prompt closes, a tree-context key typed afterwards lands in vim instead — that
  is what broke one of these tests. New tests: six in `vault.rs`, ten in
  `tests/tree.rs`, which exercise the whole path (`ctrl-n`, type the name through
  the input handler, Enter, then read what reached the disk). Live check: the
  prompt renders and the user's own keystrokes landed in the field.
- **File watching — done (2026-10-27).** `Vault::signature` walks the vault once
  and stamps every path it finds: `Stamp::File { len, mtime }` for a file,
  `Stamp::Dir` for a folder. `watch::VaultChanges::between` compares two
  signatures into `added` / `removed` / `modified` (in path order, because a
  signature is a `BTreeMap`). A folder's stamp is its *existence*, never its
  mtime: adding a child rewrites the folder's mtime, and the child is in the
  signature anyway, so stamping the folder too would report every edit twice.
  **Polling, not inotify.** `workspace::start_watcher` re-walks the vault every
  750 ms on a background thread (`background_executor().spawn`, so the walk never
  touches a keystroke) and hands the new signature to
  `Workspace::apply_disk_changes`. `shale-core` stays dependency-free, and the
  same pass is what the planned vault database will use to decide which notes to
  re-index. The watcher holds only a `WeakEntity`, so closing the window ends the
  loop. `Rc<Vault>` cannot cross a thread, so the watcher carries a `Vault` copy.
  **What a change does.** The tree always follows the disk (`refresh_entries`).
  The open note is re-read only when the buffer is clean; with unsaved edits the
  buffer wins and the status bar says `changed on disk; ctrl-s will overwrite it`
  (or `deleted from disk; your edits are still here`, so `ctrl-s` can write the
  note back). A clean note deleted elsewhere is cleared, like our own delete.
  **Shale re-stamps after its own edits** (`resign`, called from `save` and from
  `apply_prompt`), or every save would come back as somebody else's edit and
  reload the note out from under the cursor.
  **Tests.** Six in `watch.rs` (add / edit / delete / rename / folder-mtime-ignored
  / no-change), five in `tests/tree.rs` through `check_vault_now`, which is what
  the timer calls. Live check: a note created in a terminal appeared in the tree
  and vanished again when it was removed, without restarting.
  **Follow-ups:** the watcher re-walks the whole vault each tick, which is fine
  for a few thousand notes and worth replacing with a real watcher (or the
  database's incremental pass) for a big one; an external edit to a *dirty* note
  is reported but never merged; and a rename elsewhere looks like delete + add,
  so an open note that was renamed out is not followed to its new path.
- **Fuzzy matching and quick-open — done (2026-10-27).** `shale_core::fuzzy`
  matches a query as a case-insensitive subsequence and scores it: start of the
  candidate +12, start of a word (after space `/` `-` `_` `.` `(` `[` `:`) +9, a
  run of adjacent matches +4 each, +1 per character, −1 per character skipped
  inside the span, a length penalty of one point per 8 characters the candidate is
  longer than the query, and +40 for an exact match. `fuzzy_match` returns the
  byte ranges it consumed so a picker can highlight them; `ranked` returns
  `(match, index)` pairs best-first with a stable order for ties. Accents are not
  folded (`cafe` does not find `café`). The scan is a small dynamic program
  (`best_alignment`): it looks for the *best* placement of the query, not the
  first one. The greedy version it replaces could report **no match** for a
  two-word query — `new note` against `New note ctrl-n…` spent its `n` on the
  word start of "note" and then found no `w` anywhere after it. O(query ×
  candidate) with a parents matrix for the walk-back; the length penalty and the
  exact-match bonus are applied outside the DP, so the constants did not change.
  **The field is one widget now.** `crates/app/src/field.rs` holds `Field` (text,
  caret, character-wise editing, the utf-16 translation) and `FieldElement` (the
  box, the text, the caret, and the `ElementInputHandler` registration that has to
  happen during paint). `Prompt` and `Picker` are both `FieldOwner`s, and the
  prompt's key actions are `Field*` actions bound in *both* the `prompt` and the
  `picker` contexts, because they mean the same thing in both.
  **Quick-open (`ctrl-o`).** One row per note: the name leads, the folder it lives
  in is drawn dimmer after it, and the query is ranked against the **whole path**,
  so `no/an` reaches `notes/Another Note.md`. `j`/`k` (and the arrows) move the
  highlighted row, `enter` opens it, escape closes the picker and gives the
  keyboard back to whatever had it. The picker is drawn as a *sibling* of the tree
  and the editor for the same reason the prompt is: gpui enables a context's
  bindings whenever that context is in the focused element's stack, so nested
  inside either one, their `j`/`k` would swallow the letters being typed — which
  is also why `j`/`k` are free to bind inside the picker.
  **Two ranges, not one.** Ranking matches the wide `search` text, but the
  highlight has to land in the `label`, so the label is matched separately and a
  query that only matched the folder part highlights nothing in the name. (The
  first version applied the search-text ranges to the label and panicked on a
  slice boundary — the ranges of one string are not ranges of another.)
  **The list is a window, not a scroller:** 12 rows are drawn from an offset that
  slides to keep the selection visible, so there is no scroll state to own. Rows
  are not clickable yet, and the footer says "n of m matching".
  **Tests:** twelve in `fuzzy.rs`, five in `tests/picker.rs` (open, type, `j`/`k`,
  backspace, escape — including that `j`/`k` in the picker never move the vim
  cursor), and the keymap-shape test now asserts the `picker` context answers
  enter/escape/j/k/arrows and *nothing* for plain letters. Live check: the picker
  renders with the matched characters of the name shaded and the folder dimmed.
- **Window chords, `space f`, the command palette, and an overlay picker — done
  (2026-10-27).**
  **Focus chords.** `ctrl-b` is gone; `ctrl-w h` / `ctrl-w l` move the keyboard to
  the sidebar / the note, which is the shape the planned split view wants. gpui
  parses a multi-stroke binding natively — `KeyBinding::load` splits the keystroke
  string on whitespace — so the chord is literally `"ctrl-w h"` in the `actions!`
  keymap, and `ctrl-w` on its own stays pending (the keymap-shape test asserts the
  prefix arms nothing and the pairs resolve). Both handlers return early while a
  prompt or a picker is open, so a chord typed in the middle of a note name never
  steals the keyboard.
  **`space f` is a vim-engine chord, not a keymap binding.** `space` is already a
  complete motion, and gpui's `dispatch_key` returns as soon as a prefix is armed
  (`if pending { return pending }`), so binding `space` *and* `space f` would make
  a lone space feel dead. Instead `Vim::after_space` is set by a plain space motion
  and the next `f` in normal mode answers `Outcome::QuickOpen`, which the editor
  turns into `window.dispatch_action(QuickOpen.boxed_clone(), cx)`. A count, a
  command-line prefix or a visual selection disarms it, and `enter_insert` /
  `enter_normal` clear it.
  **Command palette (`ctrl-shift-p`, or `:` in normal mode).**
  `workspace/commands.rs` is the list: a `Command` enum, its label, the keys that
  run it, and the `Box<dyn Action>` it dispatches. `open.rs` now carries a
  `PickerTarget` per row — a note path or a command — so one picker serves both.
  A command row closes the picker first and *then* dispatches, at the focused node,
  which is how an editor-only action like `Undo` reaches the editor; gpui's
  `dispatch_action` defers (`cx.defer`), and the next `window.update` in a headless
  test flushes it. `:` works because Linux delivers shift+`;` as the keysym `:`
  with shift dropped (see the GPUI gotchas), so it is bound as `":"` in
  `vim-normal`.
  **What the `ctrl-shift-*` keys are for** (they are in the palette's search text,
  so typing `ctrl-shift-d` in the palette finds Delete): `-n` new folder, `-r`
  rename, `-d` delete, `-c` copy, `-v` paste, `-z` redo, `-p` the palette itself.
  The trace lines that used to show `key="r" control=true` with no action **predate
  the `ctrl+z` → `ctrl-z` keymap fix**, so they proved nothing about the
  compositor. Per `gpui_linux`'s `keystroke_from_xkb`, ctrl+shift+letter should
  arrive as the lowercase key with control *and* shift, which is what these
  bindings ask for — still worth one live re-check with `RUST_LOG=shale=trace`.
  **Layout: the note gets margins, the picker floats.** The middle column is
  `.relative()` with `.px_4().pt_3().pb_2()`, and the picker is painted *after* the
  editor as `.absolute().top_2().left_4().right_4()`. Before that, opening a picker
  inserted a row above the text, which re-wrapped and shifted every line — the
  "jerking" the user reported. `Workspace::note_origin` (test-only) reads the
  painted first line's top and left from the hit-test records, so a test can assert
  that an open picker moves nothing. Careful: `x_for_offset` is **line-relative**
  (always 0 at the line start, which is why the first version of that test failed
  with "got 0px"); `hit_line_x` is the window-coordinate edge. An overlay has to be
  **opaque** or it is unreadable: the picker paints `ui_bg` behind itself, which is
  the cost of not pushing the note down (found by the user, from a screenshot).
  **Tests:** app 79 (was 71) — six new in `tests/picker.rs` (the palette lists all
  13 rows, `:` opens it, choosing a row runs its command end to end from the prompt
  to a file on disk, a row can open the note picker, `space f` opens quick-open,
  the window chords move focus), one paint test for the margins and the overlay,
  two new fuzzy tests, two new vim tests. Peak RSS unchanged: app 42 MB, core
  5.5 MB, mcp 3.4 MB.
- **Vault search (`ctrl-shift-f`) — done (2026-10-27).** The third picker, and the
  one that forced the widget to change shape.
  **Rows that only exist once you type.** Quick-open and the palette can build
  their rows up front; a search has to read every note, so `Picker` gained a
  second constructor, `sourced`, which is handed a `RowSource<T>` — a closure from
  the query to the rows — and calls it on every keystroke. `rerank` for a sourced
  picker *keeps the source's order*: a search already decided what is worth
  showing and in what order, and re-ranking the hits as a fuzzy subsequence would
  only muddy that. The highlight is different too: a hit is there because the query
  is *inside* the line, so `substring_ranges` marks that substring rather than a
  fuzzy alignment.
  **The payload travels with the row.** A sourced picker changes the number and
  order of its rows on every keystroke, which is exactly when a parallel
  `picker_targets` array goes wrong, so `Picker` is now generic — `Picker<T>` —
  `items` is `Vec<(PickerItem, T)>`, and `chosen()` returns the payload itself.
  `PickerEvent::Choose` no longer carries an index; the workspace reads `chosen()`
  off the entity. `PickerTarget` grew `Hit { path, line }` beside `Note(path)` and
  `Command(command)`. The generic needs `T: Clone + 'static` — gpui's `Render` and
  `Focusable` supertraits require `'static`, which is the first compile error this
  refactor produced.
  **Choosing a hit moves the cursor.** `Workspace::open_note_at` opens the note and
  then `MarkdownEditor::reveal_line(row, cx)`, which is `Buffer::goto_line` plus the
  existing `ensure_cursor_visible`, so the line the row was showing is the line on
  screen. The search reports 1-based lines and the buffer is 0-based, hence the one
  `saturating_sub(1)` at that seam.
  **What a row looks like:** the matched line leads (trimmed, and clipped to 90
  characters with an ellipsis so a long paragraph cannot stretch the overlay), and
  `path:line` follows it dimmer. `MAX_SEARCH_HITS` is 200, so a keystroke stops
  rather than walking a huge vault to the end.
  **Tests:** app 82 (was 79) — three in `tests/picker.rs` (the rows are the hits
  with their line numbers and `j` walks them; choosing one opens the note with the
  cursor on that line rather than at the top; a query nothing matches leaves
  `enter` inert), plus the `ctrl-shift-f` row in the keymap-shape test. Those tests
  need notes with text in them, so `vault_with` writes real files and folds them in
  through `check_vault_now`, the same path the disk watcher uses. Peak RSS 43 MB.
  **Follow-ups:** every keystroke re-reads the vault, which is fine for a demo
  vault and will want the planned database (or a cached index) for a real one; the
  search options (`whole_word`, `case_sensitive`) exist in `shale_core::search` but
  have no UI; the cursor lands at the start of the line rather than on the match;
  and a hit in a note that is open with unsaved edits opens the disk copy, which is
  the same unsaved-edits question the watcher already refuses to merge.
- **Renaming a note rewrites the links to it — done (2026-10-27).** The last piece
  of the file-tree round, and the one Obsidian compatibility actually turns on.
  **Finding the links means resolving them.** Nothing in a vault records which
  notes link to which, so `shale_core::rename::rewrites_for_rename` reads every
  note, extracts its wiki-links and asks `Vault::resolve_link` what each one means
  — the same walk vault search makes. That is why the scan runs against the entries
  from **before** the rename: once the file has moved, the old name resolves to
  nothing and there is nothing to match.
  **Only the name is rewritten.** `WikiLink` grew `stem_start`/`stem_end`, the byte
  range of the target's last path component as written, and `stem_range` cuts it in
  order: stop at the `|` that starts an alias, then at the `#` that starts a
  heading anchor, then keep only what follows the last `/`, then trim the padding a
  writer left inside the brackets. So `[[folder/Target#A heading|read this]]`
  becomes `[[folder/Renamed#A heading|read this]]` — folder, anchor and alias
  untouched, and an embed keeps its `!`. A link to a heading in the same note
  (`[[#heading]]`) has no name, so its range is empty rather than pointing at
  something a splice would overwrite. Getting the anchor cut wrong was the first
  attempt's bug: it spliced over `Target#A heading` and silently dropped people's
  anchors.
  **Resolution decides what counts, with Obsidian's rules.** Two notes named
  `Target.md` in different folders mean a bare `[[Target]]` points at the shortest
  path and `[[notes/Target]]` points at the other, so renaming one rewrites some of
  them and not others, which is exactly right and only falls out of resolving
  rather than string-comparing. Case is folded the same way: `[[target]]` follows
  the rename and takes the new name as the file spells it. Links inside fenced code
  and inline code spans are skipped, because `extract_wikilinks` already skips them.
  **The open note is the one file Shale must not write blind.** With unsaved edits
  the disk copy is not what the user is looking at, so that note is skipped and the
  status names it (`… ; the open note has unsaved edits, so its links were left
  alone`) instead of letting them find out on the next save. When the buffer is
  clean it is updated in place as well as on disk, or the next save would put the
  old links straight back — at the cost of a `set_text`, which parks the cursor at
  the top of the note.
  **Tests:** core 215 (was 209) — two in `markdown.rs` for the stem range, four in
  `rename.rs` (every link form and the code span, the two-notes-same-name case, the
  case-folded link, a rename with no name in it). App 84 (was 82) — a rename through
  the prompt updating another note's file, and the dirty-open-note case. The test
  helper `vault_with` (notes with real text in them, folded in through
  `check_vault_now`) moved to `tests/mod.rs`, since search and tree both need it.
  **Follow-ups:** a *folder* rename leaves the links that spell the folder out
  (`[[folder/Note]]`) alone — a different splice, over the prefix rather than the
  name; markdown links (`[text](Target.md)`) are not rewritten yet; and the scan is
  a full vault read, which the planned database will make incremental.
- **Spellcheck and grammar, from a language server (2026-10-27).** Shale is the
  **client**: `crates/app/src/lsp` starts a server (`harper-ls --stdio` by
  default, `command`/`args`/`enabled` from `[spellcheck]` in `shale.toml`), speaks
  JSON-RPC over its stdin/stdout, and draws what comes back as wavy underlines.
  One server per window, one document open at a time, whole-document sync — which
  is what Harper advertises (`textDocumentSync.change = 1`), and the honest size
  for a single-note editor.
  **Two facts about harper-ls 2.8 are load-bearing.** It sends the client
  `workspace/configuration` (twice) and `client/registerCapability` *before* it
  will process a `didOpen`, so a client that treats server→client requests as
  noise sees no diagnostics, ever — `decide` answers every request, even with
  `null`. And grammar rules stay off unless `initialize` carries
  `initializationOptions.grammarOpacity = "all"`: with the defaults you get spell
  checking only, which is half the feature. With it, `she don't` comes back as
  `PronounVerbAgreement` and a heading in lowercase as `UseTitleCase`.
  **Positions are UTF-16 code units per line; the buffer counts bytes.** An
  accented letter is one unit and two bytes, an emoji two and four, so
  `mapping::offset_of` walks the line in code units and clamps a character past
  the line end rather than running into the next line. A range that cannot be
  placed is *dropped*: a squiggle on the wrong word is worse than none. The
  framing has the same trap — `Content-Length` counts **bytes**, so a framer that
  counts characters desynchronises the stream the first time a note holds an
  accent, which in a markdown vault is the ordinary case.
  **The ranges are buffer offsets all the way to the paint.** `PieceMap::
  rebase_marks` clips and re-bases them onto what a line actually paints, exactly
  as the markdown spans already were, so a note under a heading marker live
  preview hid contributes nothing and a table cell sees only its own. `build_runs`
  applies them last, over any styling the bytes already had, and `RunStyle.spell`
  turns into a wavy `UnderlineStyle`; where a markdown underline and a checker's
  note meet, the squiggle wins. The colour is `Palette::callout_danger` — the
  error red every built-in already has — rather than a new role, which would mean
  twenty palettes and the generator; a `spell` role is the refinement to make when
  a theme asks for one.
  **Debounce, and who tells whom.** `Spell::changed` bumps a revision counter and
  asks 400 ms after the last keystroke, dropping a request a later edit overtook,
  so typing a word is one request rather than one per character. The editor emits
  `EditorEvent::Changed` from `mark_dirty`, which every edit path already passes
  through, so the editor knows nothing about the checker; gpui queues an `emit`
  rather than running handlers inside the update, which is what makes it safe for
  the workspace to read the buffer back from its own handler. Opening a note,
  deleting one and renaming one re-open/close the document, since its URI *is* its
  path.
  **A missing server is a missing feature, not a broken editor.** `Spell::start`
  fails, the window logs one line and carries on with `spell: None`; nothing in
  the editor path assumes otherwise. `Drop` sends `exit` and kills the process — a
  `std::process::Child` that is merely dropped keeps running, which would leave a
  checker behind per window.
  **Verified live** (screenshots of a throwaway vault): `Thier` and `wrogn`
  squiggled, `don't` squiggled from the grammar rule, a whole lowercase heading
  squiggled from `UseTitleCase` (Harper's own range, checked against the server
  directly — not a mapping bug), and the status bar read `4 spellcheck notes`.
  **Tests:** core 216 (was 215) — the `[spellcheck]` defaults and a partial
  section over them. App 112 (was 84 before this round, +19 in `lsp/` and +2 in
  `tests/spell.rs`): the framer (byte-vs-character `Content-Length`, a reader that
  hands over one byte at a time, a bad frame skipped), the mapping (UTF-16 units,
  the clamp, the backwards range, an unplaceable diagnostic), `decide` (a server
  request answered, the handshake recognised), the URI's escapes, and the window's
  two: a server that will not start leaves the editor alone, and the ranges
  survive a painted frame. `line.rs` gained a marks case in `runs_tile_the_line`
  (gpui asserts runs tile the text exactly, so a wrong range panics the frame
  rather than underlining the wrong word) and `a_checker_range_is_a_wavy_underline`.
  **Follow-ups:** ~~no `textDocument/codeAction`~~ and ~~no hover showing *what*
  the complaint is~~ are both done — see the popup bullet below. Still open: no
  diagnostics outside the note
  (nothing else is a document); `isolateEnglish` is off, so code spans and
  frontmatter are checked as prose; and a palette role of its own for the colour.
- **The popup: code actions and hover (2026-10-28).** The checker now answers the
  two questions it could not before: *what is the complaint* and *what can be done
  about it*. Both arrive in one **panel drawn inside the window** — not an OS
  window — over the note, from `crates/app/src/popup.rs` (the widget) and
  `crates/app/src/workspace/popups.rs` (the window's rules).
  **Two ways to ask.** vim's `K` (the engine returns the new `Outcome::Hover`, and
  the editor dispatches `ShowNote` rather than knowing anything about checkers; the
  window binds `K` in the command palette too). Inside a table `K` stays the row
  motion, because the table path answers the key before the lookup can claim it —
  `upper_k_in_a_table_moves_a_row_instead`. And the pointer: each display line
  reports where the pointer is (`EditorEvent::Hovered { offset, line }`, worked out
  from the text that was painted there), and the window opens a panel only when
  that offset is inside a marked range and the editor is in normal mode. Insert
  mode is typing, so the pointer never takes the keyboard then.
  **The way down to a row is not a way away from the word.** A live try found the
  panel vanishing as soon as the pointer moved to click a fix: it hangs *below* the
  word, so the short trip from the word to one of its rows crosses text the checker
  never marked, and leaving a marked word closed the panel. Now, when nothing is
  marked under the pointer, the panel is kept if the pointer is at the height the
  panel was drawn at — `popup_placed`, the bounds the render recorded, rather than a
  guess below the line, because near the bottom of the window `snap_to_window`
  draws it above the line instead. Only the height is compared, not the column: a
  marked word near the right end of a long line sits above the space *beside* the
  panel, and walking straight down from it is just as much the way to a row.
  Moving sideways along the line still dismisses it.
  **Only one popup, and it always closes.** The window holds one
  `Option<Entity<Popup>>`; opening another replaces it. A prompt or a picker closes
  it on the way in (they are modal in a way a panel is not), and no panel opens
  while one of them is up. Every exit goes through `close_popup`, which hands the
  keyboard back to whatever had it — `escape`, a chosen row, the pointer leaving the
  word it was opened over (but not on its way down to a row), the checker
  withdrawing its note, another note opening.
  The one path with no window to refocus with (the file watcher) leaves the focus
  in `pending_focus` and the next frame does it through `window.defer`, which is
  why "closing always returns the keyboard" stays true rather than being true
  except when a disk change happens to break it.
  **Anchored to the line, not to a point.** The panel's place is recomputed each
  frame from the display line it belongs to (`popup_place` → `hit_line_bounds`), so
  it follows the note as it scrolls, and returns `None` — no panel — when that line
  has scrolled out of the viewport (`MarkdownEditor::line_is_visible`), instead of
  floating over the sidebar. `popup::panel_size(rows, size)` and the render are
  built from the same constants, so the window's idea of where the panel is cannot
  drift from where it is drawn. gpui v1.25.3 has no `.absolute()/.left()/.top()` on
  `Styled`; overlays are `gpui::anchored().position(point)` with
  `AnchoredPositionMode::Window`, which also keeps the panel inside the window
  (`snap_to_window`).
  **The notes live on the window.** `Workspace::checker_notes` keeps the
  `Diagnostic`s the server published (it used to keep only their count), so `K` and
  a resting pointer can find the note under the cursor **without asking the server
  anything** — which is what makes the whole panel testable headlessly, with no
  `harper-ls` in sight (`set_checker_notes` in tests). It also means the answer
  arrives for a word the server already described, so `K` works while the server is
  still thinking.
  **Harper's seven offers, and the shapes they come in.** `textDocument/codeAction`
  for a spelling diagnostic returns **seven** items in **two different shapes**:
  three `CodeAction`s (`title`, `edit.changes`, and a `command` that is an
  *object*) and four bare `Command`s (`title`, and a `command` that is a *string*
  with the arguments one level shallower) — `HarperIgnoreLint`,
  `HarperAddToUserDict`, `HarperAddToWSDict`, `HarperAddToFileDict`.
  `mapping::command_of` tells them apart by exactly that. An edit whose URI is not
  the open document is dropped, as is an item with no title or nothing to do.
  `context.diagnostics` is sent **empty on purpose**: Harper re-lints the range
  itself and ignores what the client echoes (read from its `generate_code_actions`).
  Applying a row writes the edits into the buffer as **one undo group**
  (`MarkdownEditor::apply_edits`, last edit first, `mark_dirty` so the checker is
  told through the usual path) and *then* runs the action's command, which is the
  spec's order. Note what that means on disk: `HarperRecordLint` makes Harper write
  its own stats file, and `HarperAddTo*` writes a dictionary — the user asked for
  it by choosing the row, but it is the server writing outside the vault.
  **`textDocument/hover` is not implemented by Harper 2.8** — probed live, it
  answers `-32601 Method not found`. So the panel's body comes from the
  diagnostics we already hold rather than a hover request. A hover request is worth
  sending only to a server that advertises `hoverProvider`; that is the follow-up.
  **A bug the logs could not show:** `receive` dropped the answered id from
  `pending` *before* `decide` looked, so every code-action answer looked like a
  request we never sent and was discarded silently — the panel opened with its
  message and no rows, and nothing was logged. `decide` reads an answer *against*
  the outstanding request, so the id has to be settled afterwards; pinned by
  `an_answer_is_read_before_its_request_is_settled`.
  **gpui notes for this round.** `Styled` has `font_family`, not `text_font`; a
  `TextRun` carries a `Font` but **no size** — the size comes from the surrounding
  text style, which is why the panel sets `font_family`/`text_size` once and lets
  its rows inherit. A row list built with `.id(...)` is `Vec<Stateful<Div>>` and
  goes in with `.children(rows)`. `HitLine::offset_for_x` is measured **from the
  line's left edge**, while a mouse event's position is a **window** coordinate:
  `offset_at_pointer_x` subtracts the line's left edge, and the two spaces are named
  separately so the next person does not mix them. In tests,
  `VisualTestContext::simulate_mouse_move(point, button, modifiers)` drives the real
  hover path, and `Entity::read_with(cx, f)` takes a closure (there is no bare
  `read_with(cx)`).
  **Verified live** (throwaway vault, screenshot): the panel over `Thier` rendered
  opaque with its border, the message clipped to one line with `SpellCheck` dimmed
  after it, and all seven offers listed with the first highlighted; the status bar
  still read `2 spellcheck notes`. Applying a row was **not** verified live — there
  is no key-injection tool allowed here — but `choosing_an_offer_rewrites_the_note`
  covers it headlessly, including that one `u` undoes the whole fix.
  **Tests:** core 218 (was 216) — `K` outside a table asks for the note and moves
  nothing, `K` inside one moves a row. App 135 (was 112): `lsp/` gained the fix
  mapping (a quickfix carrying its edit and its command, a bare command carrying
  neither edit, an edit to another document dropped, positions round-tripping
  through bytes, the ordering test above) and `tests/popup.rs` has sixteen: `K`
  opens the right note, `K` on a plain word opens nothing, a second question
  replaces the panel, `escape` closes it *and* returns the keyboard (proven by the
  next `x` deleting a character), a picker closes it, a chosen offer rewrites the
  note as one undo step, an offer with no edit still closes the panel, the pointer
  opens and closes it, hovering while typing says nothing, a `K`-asked panel
  outlives the pointer, the pointer can walk from the word down onto the panel —
  including from a word beside the panel rather than above it — a withdrawn note
  takes the panel with it, another note ends
  it (and the deferred refocus gives the keyboard back), and the panel follows its
  line off screen.
- **More theme colours to add (the roles everything currently borrows).** Four
  things are drawn with a role that was chosen for something else, and each wants
  its own `Palette` role when a theme asks for it:
  - the checker's wavy underline uses `callout_danger`;
  - the popup's background, border and highlighted row use `ui_bg`, `ui_border`
    and `ui_active_bg` — the same three the pickers use, so a theme cannot make a
    panel read differently from a picker, and the popup's *code* (`SpellCheck`) is
    dimmed with `dim`, the note's own dim;
  - the status bar's background is `ui_status_bg`, but its text is `fg`;
  - a matched picker row's text is `match_text`, judged against both picker
    backgrounds by the readability test.
  Adding a role is not one line: it means **20 built-in palettes**
  (`crates/core/src/theme/`, one module per family), the `ThemeFile` override
  struct, `tools/theme_gen.py`'s `FLOORS` so the generated palettes clear the same
  WCAG checks the readability test applies, and the readability test itself. Do all
  four in one pass, run the generator and `cargo fmt --all`, and check `git diff`
  on the generated files is empty.
- **Module sizes.** `editor.rs` and `main.rs` both passed the 1000-line mark in
  `AGENTS.md`, so they are now directories: `editor/{mod,wrap,render,actions,input}.rs`
  (a submodule can reach `MarkdownEditor`'s private state because they are all
  descendants of `editor`; anything one sibling calls on another is `pub(super)`)
  and `tests/{mod,keys,paint}.rs`. `mdstyle.rs` passed it too and is now
  `mdstyle/{mod,block,inline}.rs` (450 / 398 / 277): `mod.rs` holds `Style`,
  `CalloutKind`, `StyleSpan`, the line walk and the tests, `block.rs` classifies
  each line, `inline.rs` styles the text inside it. `main.rs` passed it again
  with the file tree, and is now `main.rs` (122) + `acts.rs` + `workspace/`.
  Largest files now: `vim.rs` 2333, `buffer.rs` 1786, `line.rs` 973, `tests/paint.rs`
  806, `core/theme/mod.rs` 794, `vault.rs` 750, `workspace/mod.rs` 734,
  `lsp/mod.rs` 671, `editor/mod.rs` 614, `table/edit.rs` 640, `table/mod.rs` 590,
  `picker.rs` 587, `editor/render.rs` 564,
  `tests/tree.rs` 515, `tests/popup.rs` 483, `lsp/mapping.rs` 454,
  `tests/motions.rs` 442, `markdown.rs` 416, `editor/wrap.rs` 395,
  `workspace/edits.rs` 360, `tests/picker.rs` 346, `mdstyle/block.rs` 403,
  `mdstyle/mod.rs` 454, `field.rs` 335, `fold.rs` 332, `prompt.rs` 256,
  `fuzzy.rs` 324, `workspace/popups.rs` 285, `popup.rs` 276, `rename.rs` 195, and the other
  `workspace/` submodules at 355/209/189/187/118. `tests/keys.rs`
  reached the rule's line and is now three: `keys.rs` (344: the keystroke shapes,
  what each key resolves to per context, the chords), `motions.rs` (442: vim
  motions, operators, text objects, visual mode, `.` and the undo chords driven
  through the keymap) and `blocks.rs` (222: folding and the Enter marker
  continuation). `vim.rs` and
  and
  `buffer.rs` are still
  over the rule — split them the next time either one is touched; `line.rs` (the
  element, the run builder and the table grid paint) grew to 973 with the
  checker's underlines and is the next to cross it, so the split there is probably
  `line.rs` (the element) and `runs.rs` (the run builder and the style map).

System dependency (Linux): `libxkbcommon-x11-devel` (plus fontconfig, freetype,
x11, xcb, wayland dev packages) and a working Vulkan driver (the Linux renderer
is wgpu).

Next steps, roughly in order:

1. ~~Wiki-link rendering/highlighting + navigation~~ Done: resolved links tinted,
   unresolved dimmed; `gx` / ctrl+click follow the link under the cursor.
   Still TODO here: creating a note from an unresolved link, and link preview.
2. ~~Soft wrap~~ Done (see Open threads). Follow-ups there: screen-line `$`/`0`,
   display columns for tabs/multi-byte text.
3. ~~Resolve the ctrl+z / redo keystroke question~~ Done (keymap `+` → `-`).
   Re-test ctrl-z / ctrl-r / ctrl-y and `ctrl-s` in a live window. The
   `ctrl-shift-*` bindings are listed in the command palette now, so a dead one is
   worth a live check with `RUST_LOG=shale=trace` (the old trace lines that looked
   like proof of a swallowed chord predate the `ctrl-z` fix).
4. ~~Visual mode~~ and ~~`.` repeat~~ done (see Open threads). Next: the table
   goal, then live preview.
5. ~~Table-aware motions and manipulation (the table goal)~~ Done: the model, the
   row/column edits, the cell-aware motions, the keymap and the grid widget, plus
   the user's polish round (cell-to-cell `h`/`l`/`w`/`b`/`e`, `J`/`K` escaping
   the table while plain `j`/`k` can stop on the rule row, one undo step per
   table edit, `alt+hjkl` row/column moves) — see Open threads. Follow-ups:
   horizontal scrolling for a table wider than the window, and wide-glyph column
   widths.
6. ~~Live preview mode~~ stage 1 (markdown syntax styling, heading sizes and
   code-block backgrounds), stage 2a (hiding the markers of a row the cursor is
   not on, including inside table cells) and stage 2b (callouts, collapsing a
   code block's fence lines and a table's delimiter row out of the display model,
   and whole-row image embeds) are done — see Open threads. What is left here:
   images embedded inside a paragraph, horizontal scrolling for a table wider
   than the window, wide-glyph column widths, and the shaped-geometry display
   columns that would also settle j/k accuracy.
7. ~~Folding (header sections)~~ Done — see Open threads. `shale_core::fold` holds
   the regions and the closed-fold set, `Buffer::fold_view` turns them into the
   rows the display model drops, and `za`/`zc`/`zo`/`zd`/`zM`/`zR` plus a click on
   the marker drive it. Follow-ups: fold fenced blocks, callouts and tables too;
   keep fold state per note across sessions; `zz`/`zt`/`zb` scroll commands.
8. ~~Theming (light/dark, TOML theme files)~~ Done — see Open threads. The
   palette is `shale_core::theme::Palette`, `appearance.theme` selects it, and
   `appearance.font_size` is wired through the editor. Follow-ups: switching
   themes without a restart, and whatever roles a tree-sitter code highlighter
   will need.
9. ~~Editor bindings round: list / task / quote continuation on Enter~~ Done —
   see the continuation bullet in Open threads. `shale_core::continuation` decides
   what Enter does, the insert-mode handler applies it so the marker joins the
   undo group and `.`. Follow-ups: replaying a marker *removal* with `.`, deeper
   nesting of lists, and continuing a callout's body.
10. ~~HarperLS / LSP integration for spellcheck and grammar~~ Done for the note —
    see the spellcheck and popup bullets in Open threads. `crates/app/src/lsp` is
    the client, `[spellcheck]` in `shale.toml` names the server, what it finds is
    drawn as wavy underlines and counted in the status bar, and its `codeAction`s
    and its complaint reach the user through the popup (`K`, or the pointer on a
    marked word). Left here: asking a server that advertises `hoverProvider` (Harper
    does not implement hover), the colour wanting a palette role of its own, and
    "support for custom language servers" is only half true — the config takes any
    command, but diagnostics and code actions are all Shale consumes. A popup that
    can list more rows than fit (Harper's seven just fit) and a grammar fix applied
    live are untested; `HarperRecordLint` writing Harper's own stats file is the
    first time a row touches anything outside the vault.
11. ~~Image display in vaults (gpui image elements)~~ mostly done: a row that is
    nothing but an embed is drawn as the picture (stage 2b). What is left is an
    image embedded *inside* a paragraph, and an `|alias` suffix on an embed is
    currently read as "no size".
12. ~~File tree: new/rename/delete, file watching~~ and ~~rewriting the wiki-links
    that point at a renamed note~~ Done (see Open threads). `enter`/`space` on a
    note row now opens it *and* moves the keyboard to the note (a folder row keeps
    the sidebar's keyboard). Left here: following an
    open note when something else renames it, rewriting links on a *folder*
    rename, and markdown links (`[text](Target.md)`) as well as wiki-links.
13. **Database tables (Obsidian bases)** — the user's goal, see the Database
    section above. Nothing built yet, but the shape of it is already useful:
    a query result is a table that is *not* buffer text, and the grid paint
    (`table_view::plans_by_row` + `LineElement`) is the widget it needs. Keep
    that geometry independent of `shale_core::table`'s buffer offsets so a
    rendered-from-a-query grid can reuse it, and keep the note files the source
    of truth (the DB in `<vault>/.shale` is a cache, updated incrementally in the
    background at startup and then on edits). `Vault::signature` is the
    incremental pass the user asked for: it stamps every path with size and mtime
    without reading a note, and `watch::VaultChanges::between` says which notes to
    re-index. Open/edit counts (the user's addition) are a separate counter table,
    written on open and debounced on edit, not something the signature can tell.
14. ~~Pickers, reused~~ Quick-open, the **command palette** (`ctrl-shift-p`, `:` in
    normal mode, `space f`) and **vault search** (`ctrl-shift-f`) are all done —
    see Open threads. What is left there is polish: clickable rows, a scrolling
    list once a vault outgrows the 12-row window, search options in the UI, a
    prefix that switches what the picker searches, and jumping to the matched
    column rather than the start of the line.
