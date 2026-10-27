# Shale
Shale is an obsidian-style markdown editor with extra developer focused features. Shale is built with GPUI in Rust

## Goals
- [ ] "Live preview" (obsidian-style) Markdown editor with vim binding support
- [ ] File tree and interface
- [ ] Wiki-links
- [ ] MCP server
- [ ] Spellcheck and grammar checking using HarperLS. Support for custom language servers.
- [ ] Near-perfect compatibility with Obsidian vaults 
- [ ] Image support in vaults
- [ ] Theming support and support for custom themes. Include light and dark themes.
- [ ] Graph view
- [ ] Pdf export notes

- Config files in TOML
- Assure any code you reference or copy from online has a commercial permissible licensing 
- Write unit tests 
- Regularly commit to a local git repo. You don't need to publish it. Don't override the user's global config, make a local override for a user for yourself.
- Expand this plan and goals as needed.
- Assure **proper** support for manipulating tables with vim bindings. No unexpected skipping or jumping around when manipulating tables. Ask the user for expected behavior when designing this behavior.
- Tree-sitter powered code highlighting for code blocks.
- Full keyboard support if at all possible. 98% of the app should not require mouse usage. Mouse in the settings menu is fine.
- Indexing for quick wiki-link look up and resolving.
- Use GPUI's testing framework for headless testing of the app when possible.

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
Shale has an MCP server with support for reading, writing, and searching vaults. The MCP server should *either* be a separate binary which shares core functionality with a `common` crate in the project, or if speed will be acceptable use the main binary.

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
  vault), text `Buffer` with undo/redo, and the vim mode engine. Pure Rust,
  fully unit tested.
- `crates/app` (`shale` binary): GPUI app (Zed's gpui, tag `v1.25.3`) — file
  tree sidebar, custom line-based editor view with **soft wrap** and vim
  normal/insert modes (hjkl, w/b/e, 0 ^ $ — screen-line aware, gg/G, counts on
  motions and operators, d/c/y + motions, x, p/P, u undo, i/a/I/A/o/O,
  ctrl-z/ctrl-r/ctrl-y, clipboard, mouse click to position), wiki-link styling (resolved tinted blue,
  unresolved dimmed) and navigation (`gx` on a link in normal mode, ctrl+click),
  status bar (mode, pending count/operator, dirty flag, path, display row:col),
  ctrl+s save.
- `crates/mcp` (`shale-mcp` binary): MCP stdio server exposing list/read/
  write/search/backlinks tools.

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

### Open threads (write findings here as they come up)
- **Soft wrap — done.** `Buffer` holds a *display model*: the UI calls
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
  user sees. `display_lines()` now hands the buffer *exactly* the segments it
  paints (previously the buffer got raw cached segments and the paint re-validated
  them — two index spaces that could desync the cursor's row); the tests assert
  the model's ranges equal the painted ranges.
  Screen vs logical motions (vim's own split): `0`, `^`, `$` (and `d$`, `c$`)
  are **display-line** motions; `A` and `I` stay **row**-wise
  (`Buffer::row_first_non_blank` / `row_last_char_motion`). `desired_col` is a
  character column within the display line, and the ruler reports character
  columns.
  Still open in this area:
  - The buffer row is no longer visible anywhere (the ruler shows the display
    row, as vim does).
  - Tabs render as one cell in gpui, so a tab-indented note looks different from
    vim/neovim; if we want real tab stops the wrap and the column math both need
    to expand them.
- **ctrl+letter keys: the keymap used `+` instead of `-`.** Fixed. gpui's
  keystroke syntax separates modifiers with `-` (`ctrl-z`); `Keystroke::parse(
  "ctrl+z")` does *not* error, it returns one literal key named `"ctrl+z"` that
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
  `action = None` both when no binding matched *and* when a binding matched but
  no `on_action` listener consumed it — so "no action" does not by itself mean
  "no binding".
- **Undo granularity** is now: one insert session = one step, `cw`/`o` + typing =
  one step, each normal-mode operator = one step. Vim coalesces slightly
  differently; revisit if it feels wrong in practice.
- **j/k across wraps**: the reported "sometimes wildly off" could not be
  reproduced — the wrap model is provably identical to what is painted (see the
  soft-wrap bullet), and gpui's `scroll_to_item` only scrolls the minimum to keep
  the line visible. The two real defects found on the way were the byte-column
  `desired_col` and the model/paint index spaces, both fixed; the logical `$`/`0`
  (which jumped to the end of the *row*, several rows down) is the most likely
  thing the report was describing. Needs a re-test in a live window.
- **Not implemented yet in vim mode**: visual mode (`v`/`V`) and selection
  operators, `.` repeat, text objects (`iw`, `aw`, `ap`), `f`/`F`/`t`/`T` find
  motions, `0`-column vs display-column subtleties, registers other than the
  unnamed one.
- **Cursor model nits**: `Buffer` cursors may sit at `text.len()` (between
  characters) while vim's normal-mode cursor always sits *on* a character, so
  `w` at the end of a buffer can leave the cursor one position past the last
  character; normal mode should clamp.

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
4. Visual mode (`v`/`V`) and selection operators; `.` repeat; text objects
   (`iw`, `ap`); `f`/`t` find motions.
5. Table-aware motions and manipulation (the table goal): column motions on the
   pipe and dash characters, adding/removing rows and columns.
6. Live preview mode (rendered markdown toggle) — pulldown-cmark for parsing.
7. Theming (light/dark, TOML theme files).
8. HarperLS / LSP integration for spellcheck and grammar.
9. Image display in vaults (gpui image elements).
10. File tree: new/rename/delete, file watching.
