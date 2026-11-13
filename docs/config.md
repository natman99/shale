# Config and themes

## The file

Each vault carries its own config at **`<vault>/.shale/config.toml`**. The first
time Shale opens a folder it writes a starter file there: every key present, every
value already the default, each with the comment that explains it. Delete any line
you do not care about and Shale uses its default for it.

An unknown key is an error, not a warning: `deny_unknown_fields` is on, so a typo in a
key name fails loudly instead of doing nothing.

When Shale changes a setting of its own, like the theme you pick, it rewrites that one
line and leaves the rest of the file, comments included, as it found it.

If a vault still has the root-level `shale.toml` that older builds used, Shale moves it
into `.shale/` rather than reading both places. There is one config location.

## The keys

| section | key | default | what it does |
| --- | --- | --- | --- |
| `[editor]` | `vim_mode` | `true` | vim-style normal/insert bindings |
| | `line_wrap` | `true` | soft wrap |
| | `show_file_tree` | `true` | the tree is open at startup |
| | `autosave` | `true` | write the note on its own — after a pause, on blur, before a quit |
| | `autosave_after_ms` | `2000` | how long a pause counts as one |
| `[appearance]` | `theme` | `""` — ask the desktop | a built-in palette, or the name of a file in `.shale/themes/<name>.toml` |
| | `font_size` | `14.0` | body text in points; headings and line heights scale from it |
| | `font_family` | `""` | empty means "pick a monospace family that is actually installed" |
| | `live_preview` | `true` | hide the markdown syntax of a row the cursor is not on |
| `[links]` | `hover_aliased` | `true` | the hover card over a link whose shown text is not the note's name (`[[masked\|anon]]`, or an `aliases:` name) |
| `[spellcheck]` | `enabled` | `true` | underline what the checker finds |
| | `command` | `"harper-ls"` | the language server to run |
| | `args` | `["--stdio"]` | its arguments |
| `[index]` | `enabled` | `true` | `.shale/shale.db`: notes, tags, links, word counts |
| `[mcp]` | `live` | `true` | the loopback channel a window answers live questions on |
| | `profile."<name>"` | — | an agent connection's scope: `read`, `write`, `deny`, `tools`, `review` |

Two of them change what other features do:

- **`theme = ""`** is the default and names no palette at all: the window opens in Shale's
  dark and then follows the desktop's own dark-or-light setting — `dark` for a dark desktop,
  `light` for a light one. Naming a theme by hand stops the following.
- **`[index] enabled = false`** leaves the cache unwritten. The features that read it
  say so instead of guessing: `:wordcount` answers *there is no vault index to count
  (index.enabled = false)*. The graph is unaffected, because `GraphIndex::build` works
  from the vault walk rather than the index.

## Themes

Twenty built-in palettes. Shale's own are `dark` and `light`; the rest are ports of
popular families: Catppuccin (`latte`, `frappe`, `macchiato`, `mocha`, also spelled with
a `catppuccin-` prefix), Tokyo Night ×3, Gruvbox ×2, Nord, Dracula, Solarized ×2, One ×2,
and Rosé Pine ×3. The default is `mocha`.

Change it while the window is up with `:theme <name>` or the palette's *Change the
theme*. The picker previews the palette its highlighted row names, `escape` puts the old
one back, and a theme you choose is written to `appearance.theme` so the next launch
opens in it.

Your own theme is a file: `.shale/themes/<name>.toml`, named by `appearance.theme`.
The fastest way to start one is to export the palette you are already looking at:

```
:theme export my-theme
```

A theme file is **additive**: it fills the colour roles it names, and every role it
leaves out is computed from the ones it gives. The role names are the fields of
`shale_core::theme::Palette`: the chrome (`ui_fg`, `ui_dim`, `ui_disabled`,
`ui_border`, `ui_hover_bg`, `ui_active_bg`, `ui_active_border`, `ui_tab_bg`,
`popup_border`), the text and markdown roles, the graph kinds, and the diff colours.
`design-language.md` in the repo root is the grammar those roles are defined by.

A test walks every built-in palette against the readability floors, which is why a
family's signature colour is sometimes not used where it would fail: Catppuccin Latte's
highlight falls back to Mauve because Peach sits at 1.6:1 on a highlighted row.
