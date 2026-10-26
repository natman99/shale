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

- Config files in TOML
- Assure any code you reference or copy from online has a commercial permissible licensing 
- Write unit tests 
- Regularly commit to a local git repo. You don't need to publish it. Don't override the user's global config, make a local override for a user for yourself.
- Expand this plan and goals as needed.
- Assure **proper** support for manipulating tables with vim bindings. No unexpected around when manipulating tables. 
- Tree-sitter powered code highlighting for code blocks.
- Full keyboard support if at all possible. 98% of the app should not require mouse usage. Mouse in the settings menu is fine.
- Indexing for quick wiki-link look up and resolving.

## Non-Goals
- Plugins

### Tech Stack
Keep as much in Rust as possible, no bindings external libraries. Update the tech stack as needed.
- Tokio for async
- GPUI for UI
- Serde for file parsing
- You may want to use GPUI kit, a components library for GPUI. Evaluate if it's useful before using.
https://gpui-kit.com/

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
- `crates/app` (`shale` binary): GPUI 0.2.2 app — file tree sidebar, custom
  line-based editor view with vim normal/insert modes (hjkl, w/b/e, 0 ^ $,
  gg/G, dd, x, i/a/I/A/o/O, undo/redo, clipboard, mouse click to position),
  status bar (mode, dirty flag, path, row:col), ctrl+s save.
- `crates/mcp` (`shale-mcp` binary): MCP stdio server exposing list/read/
  write/search/backlinks tools.
- `vendor/xattr`: patched xattr 0.2.3 (glibc ENOATTR build fix) via
  `[patch.crates-io]`; see `vendor/xattr/PATCH.md`.

Known platform issue (important): crates.io `gpui` 0.2.2 has **no published
Wayland fix** for its frame/present path. Its Wayland backend requests a
`wl_surface` frame callback exactly once (at first configure) and then relies on
the compositor's callback chain; on niri that chain stalls, so the app draws new
frames (render logs confirm) but never presents them — the window keeps showing
the first frame. Forcing the X11 backend (`env -u WAYLAND_DISPLAY shale ...`, i.e.
XWayland under niri) renders and updates correctly. Current Zed works on Wayland
because it builds a newer, unpublished gpui split (`gpui` + `gpui_linux` /
`gpui_platform`) whose Wayland window has a full frame-loop state machine
(`FrameLoop`, `PresentationState`, `schedule_frame_retry`). `gpui_platform` is not
on crates.io; latest published `gpui` is 0.2.2. Options: depend on Zed's git
(tag `v1.25.3` has the fixed stack, Apache-2.0), stay on 0.2.2 + X11, or port the
frame-loop rework into a vendored gpui.

System dependency (Linux): `libxkbcommon-x11-devel` (plus fontconfig,
freetype, x11, xcb, wayland dev packages).

Next steps, roughly in order:
1. Wiki-link rendering/highlighting + navigation (jump/preview) in the editor.
   Done: resolved links are tinted, unresolved links dimmed; `gx` in normal mode
   and ctrl+click follow a link under the cursor (unresolved targets report a
   status message; creating them is still TODO).
2. Live preview mode (rendered markdown toggle) — pulldown-cmark for parsing.
3. Visual mode, more vim operators (c/d + motions, counts), table-aware
   motions for the table-manipulation goal.
4. Theming (light/dark, TOML theme files).
5. HarperLS / LSP integration for spellcheck and grammar.
6. Image display in vaults (gpui image elements).
7. File tree: new/rename/delete, file watching.
