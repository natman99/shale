# Config and themes

## The file

Each vault carries its own config at **`<vault>/.shale/config.toml`**. The first
time Shale opens a folder it writes a starter file there: every key present, every
value already the default, each with the comment that explains it. Delete any line
you do not care about and Shale uses its default for it.

An unknown key is an error rather than a shrug — `deny_unknown_fields` — so a typo
in a key name tells you instead of silently doing nothing.

When Shale changes a setting of its own (a theme you pick), it rewrites **that one
line** and leaves the rest of the file, comments included, exactly as it found it.

A vault whose config is still the root-level `shale.toml` that older Shale used has
that file **moved** into `.shale/` — moved rather than also read, because there is
one config location now.

## The keys

| section | key | default | what it does |
| --- | --- | --- | --- |
| `[editor]` | `vim_mode` | `true` | vim-style normal/insert bindings |
| | `line_wrap` | `true` | soft wrap |
| | `show_file_tree` | `true` | the tree is open at startup |
| | `autosave` | `true` | write the note on its own — after a pause, on blur, before a quit |
| | `autosave_after_ms` | `2000` | how long a pause counts as one |
| `[appearance]` | `theme` | `"mocha"` | a built-in palette, or the name of a file in `.shale/themes/<name>.toml` |
| | `font_size` | `14.0` | body text in points; headings and line heights scale from it |
| | `font_family` | `""` | empty means "pick a monospace family that is actually installed" |
| | `live_preview` | `true` | hide the markdown syntax of a row the cursor is not on |
| `[spellcheck]` | `enabled` | `true` | underline what the checker finds |
| | `command` | `"harper-ls"` | the language server to run |
| | `args` | `["--stdio"]` | its arguments |
| `[index]` | `enabled` | `true` | `.shale/shale.db`: notes, tags, links, word counts |
| `[mcp]` | `live` | `true` | the loopback channel a window answers live questions on |
| | `profile."<name>"` | — | an agent connection's scope: `read`, `write`, `deny`, `tools`, `review` |

Two of those deserve a second look:

- **`theme = ""`** names no palette at all: the window opens in the dark default and
  follows the desktop's own dark-or-light setting instead.
- **`[index] enabled = false`** leaves the cache unwritten. The features that read
  it say so rather than guessing — `:wordcount` answers *there is no vault index to
  count (index.enabled = false)*. The graph does not need it: the picture is built
  from the notes.

## Themes

Twenty built-in palettes: Shale's own `dark` and `light`, plus the families it
ships — Catppuccin (`latte`, `frappe`, `macchiato`, `mocha`, also spelled with a
`catppuccin-` prefix), Tokyo Night ×3, Gruvbox ×2, Nord, Dracula, Solarized ×2,
One ×2, Rosé Pine ×3. The default is `mocha`.

Change it while the window is up with `:theme <name>` or the palette's *Change the
theme* — the picker previews the palette its highlighted row names, `escape` puts
the old one back, and a theme you actually choose is written back to
`appearance.theme` so the next launch opens in it.

Your own theme is a file: `.shale/themes/<name>.toml`, named by `appearance.theme`.
The fastest way to start one is to export the palette you are already looking at:

```
:theme export my-theme
```

A theme file is **additive**: it fills the colour roles it names, and every role it
leaves out is computed from the ones it gives. The role names are the fields of
`shale_core::theme::Palette` — the chrome (`ui_fg`, `ui_dim`, `ui_disabled`,
`ui_border`, `ui_hover_bg`, `ui_active_bg`, `ui_active_border`, `ui_tab_bg`,
`popup_border`), the text and markdown roles, the graph kinds, and the diff colours.
`design-language.md` in the repo root is the grammar those roles are defined by.

Contrast is not left to chance: a test walks every built-in palette and checks the
readability floors, which is why a family's signature colour is sometimes *not*
used where it would fail — Catppuccin Latte's highlight falls back to its Mauve
because its Peach is 1.6:1 on a highlighted row.
