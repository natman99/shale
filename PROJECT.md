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

Gui pickers should have a search bar with fuzzy search, but arrow keys + enter can select options manually, or the mouse can.

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

### Graph View

The graph view needs to have filter options to hide and show nodes. It should have the option to color nodes based on tags. I want the ability to sort into "constellations" based on the subfolder the note rests in. Add the option to filter a parent folder to then have the constellations be its children. Add 3d and 2d graph views. Graph should have the option to be static, where each note gets it's position from some sort of stable and fast method, and a dynamic physics based graph like Obsidian has. Make sure to handle duplicate note names and other weird edge cases.

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

## Current status (updated as the project progresses)

Workspace layout:
- `crates/core` (`shale-core`): vault model (path-traversal safe), wiki-link
  extraction/resolution/backlinks, search, TOML config (`shale.toml` in the
  vault), text `Buffer` with undo/redo, the vim mode engine, `mdstyle` (the
  presentation-agnostic markdown style scanner), `mdlayout` (which markdown
  markers live preview hides, and the visible pieces that remain) and `table`
  (the table model: cells, cell lookup, row/column editing). Pure Rust, fully
  unit tested.
- `crates/app` (`shale` binary): GPUI app (Zed's gpui, tag `v1.25.3`) — file
  tree sidebar, custom line-based editor view with **soft wrap** and vim
  normal/insert/visual modes (hjkl, w/b/e, 0 ^ $ — screen-line aware, gg/G,
  counts on motions and operators, d/c/y + motions, f/t/F/T, iw/aw/ip/ap, x, p/P,
  u undo, i/a/I/A/o/O, v/V, `.` repeat, ctrl-z/ctrl-r/ctrl-y, clipboard, mouse
  click to position), **live markdown styling** (headings, emphasis, code,
  quotes, lists/tasks, tables, tags, links; resolved wiki-links tinted blue,
  unresolved dimmed) with **live preview** hiding the markers of a row the cursor
  is not on, and link navigation (`gx` on a link in normal mode, ctrl+click),
  status bar (mode, pending count/operator, dirty flag, path, display row:col),
  ctrl+s save. Split into `main.rs` (workspace, keymap, actions), `editor.rs`
  (the view: buffer + vim + wrap model + input), `line.rs` (the `LineElement`
  that paints one display line, and the run builder), `preview.rs` (the painted
  form of a display line and the buffer <-> painted-offset map), `table_view.rs`
  (the table grid geometry: column widths and cell offsets, in character cells),
  `theme.rs` (the palette and `Style` -> `RunStyle` mapping) and `tree.rs`.
- `crates/mcp` (`shale-mcp` binary): MCP stdio server exposing list/read/
  write/search/backlinks tools.
- `refs/` (**gitignored, not shipped**): local copies of the Vim help files
  (`motion`, `visual`, `change`, `insert`, `editing`, `pattern`, `repeat`,
  `quickref`, `undo`, `options`) and ~57 Obsidian help pages, fetched 2026-10-26
  so behaviour can be checked without network access. Sources and licensing in
  `refs/README.md`: the Obsidian help repo grants **no** license, so those pages
  are specification only — never copy their prose or assets into Shale.

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
  see the table postmortem). Build parallelism is capped in `.cargo/config.toml`
  at the user's request because a full-parallel build stalled the editor.

### Open threads (write findings here as they come up)

- **Live preview stage 2a — the markers are hidden.**
  `shale_core::mdlayout` answers the two questions the paint needs:
  `hidden_ranges` (which bytes of a row to leave unpainted) and `visible_pieces`
  (which remain). `is_marker` hides a heading's `#` (and the space after it, so a
  heading is not indented), a blockquote's `>` (but *not* its space, which is what
  leaves the quote visibly indented), every `Style::Punctuation` — emphasis
  delimiters, a code span's backticks, a link's brackets, an escaped marker's
  backslash — plus `LinkUrl` and the new `LinkTarget`, the `target|` of an aliased
  wiki-link. Deliberately kept: list bullets and numbers, task checkboxes, `#tags`,
  thematic breaks and the ``` fence lines (the only sign of where a block starts).
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
  **Still stage 2:** inline images, callouts (`> [!note]`), and hiding the fence
  lines of a code block.

- **Tables — done: model, editing, and the grid widget.**
  `shale_core::table` holds the model; `mdstyle::table_row_flags` and
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
  peak RSS afterwards: core 4 MB, app 22 MB, mcp 3 MB.
  **Bindings (implemented in the vim engine + keymap):** `Shift+H`/`Shift+L`
  column left/right, `Shift+J`/`Shift+K` row down/up, `dd` delete row, `DD`
  (`shift-d`) delete column, `Shift+I` empty the cell and insert, `o`/`O` insert
  an empty row below/above, `|` insert a column after, `\` insert a column
  before. Plain `j`/`k` inside a table step **cell to cell** in the same column,
  which is what the user asked for after other apps skipped around; `A` appends
  at the cell's end rather than the row's. The ordinary motions are clamped to
  the cell: `h l w b e` are undone if they would step outside, `0 ^ $` use the
  cell's edges, a find that would land outside counts as "not found" (so `f|`
  works — the closing pipe belongs to the cell — and `d3x`-style sweeps stop at
  the content edge). `H/J/K/L` are bound in the visual context too, where they
  extend the selection; `D`, `|` and `\` are normal-mode only, since `D` in
  visual is the selection operator. **Caveat:** `D` is not `d$` yet (it was
  unbound before), so if we want vim's `D` it needs a different key or a
  table/non-table split. As with every new bound key, the half-typed-command
  path was checked: `f|`, `f\`, `fH` arrive through the pending-find arm, which
  runs before the table match.
  **Painting.** A table row is *not* painted as markdown: `table_view::plans_by_row`
  turns each table into a grid in **character cells** (the widest content per
  column plus a one-cell gutter on each side), and `LineElement` paints each
  cell's content at its column offset with vertical rules on every boundary and a
  shaded header row; the delimiter row's dashes are hidden and drawn as a
  horizontal rule. Consequences worth remembering:
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
   Re-test ctrl-z / ctrl-r / ctrl-y and `ctrl-s` in a live window.
4. ~~Visual mode~~ and ~~`.` repeat~~ done (see Open threads). Next: the table
   goal, then live preview.
5. ~~Table-aware motions and manipulation (the table goal)~~ Done: the model, the
   row/column edits, the cell-aware motions, the keymap and the grid widget (see
   Open threads). Follow-ups: horizontal scrolling for a table wider than the
   window, and wide-glyph column widths.
6. ~~Live preview mode~~ stage 1 (markdown syntax styling, heading sizes and
   code-block backgrounds) and stage 2a (hiding the markers of a row the cursor is
   not on, including inside table cells) are done — see Open threads. Stage 2
   remaining: inline images and callouts.
7. Theming (light/dark, TOML theme files). `theme.rs` is the palette and
   `appearance.font_family` in `shale.toml` already picks the editor's font;
   `appearance.font_size` is still not wired up (the editor shapes at
   `theme::FONT_SIZE`).
8. Editor bindings round: **list / task / quote continuation on Enter.** Inside
   a `-`/`*`/`+` bullet, a numbered item, a task (`- [ ]`) or a `>` quote,
   pressing Enter should insert the same marker on the new line (Obsidian's
   behaviour), and pressing Enter again on a marker-only line should remove that
   empty marker instead of adding a second one. Needs the marker text inserted in
   insert mode (so it lands in the same undo group and `.` replay), and the
   continuation should follow the line's indent. Task lines should insert an
   empty `- [ ]` regardless of whether the previous one was checked.
9. HarperLS / LSP integration for spellcheck and grammar.
10. Image display in vaults (gpui image elements).
11. File tree: new/rename/delete, file watching.
