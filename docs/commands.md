# Shale's commands

Generated from the table in `crates/app/src/command.rs` — do not edit by hand. A
test fails when this file and the table disagree; `SHALE_UPDATE_DOCS=1 cargo test
-p shale-app` rewrites it.

Every command has a stable **id**: what the docs, the MCP surface and any later
macro language name it by, so renaming a label never breaks a caller. **Keys** are
what to press, and a key that only works in the file tree says so. **Command line**
is what to type after `:` in normal mode. **Args** is how the argument is spelled,
as you would type it.

| id | what it does | keys | command line | args |
| --- | --- | --- | --- | --- |
| `note.save` | Save the note | `ctrl-s` | `:w [name]`, `:write [name]` | `[name]` |
| `app.quit` | Quit Shale | `ctrl-q` | `:q`, `:quit` | — |
| `app.quit-force` | Quit Shale, discarding the note's unsaved edits | — | `:q!`, `:quit!` | — |
| `note.write-quit` | Write the note, then quit | — | `:wq`, `:x`, `:xit`, `:exit` | — |
| `note.new` | New note | `ctrl-n, n in the tree` | — | — |
| `folder.new` | New folder | `ctrl-shift-n, shift-n in the tree` | — | — |
| `note.add-properties` | Add frontmatter properties to the top of the note | — | — | — |
| `note.rename` | Rename the selected note | `ctrl-shift-r, r in the tree` | — | — |
| `note.delete` | Delete the selected note | `ctrl-shift-d, shift-d in the tree` | — | — |
| `note.open` | Open a note | `space f, g f` | `:e`, `:edit`, `:open` | — |
| `jump.back` | Jump back to the previous note | `ctrl-o` | — | — |
| `jump.forward` | Jump forward to the next note | `ctrl-i` | — | — |
| `vault.search` | Search the vault | `ctrl-shift-f, space s` | — | — |
| `theme.change` | Change the theme | — | `:theme [name]` | `[name]` |
| `vault.switch` | Switch vault | — | `:vault` | — |
| `window.new` | New window on this vault | — | `:new-window` | — |
| `theme.export` | Write the palette on screen to a theme file | — | `:theme export <name>` | `<name>` |
| `checker.on` | Turn the spellchecker on | — | `:harper on`, `:harper 1`, `:spellcheck on`, `:spell on` | — |
| `checker.off` | Turn the spellchecker off | — | `:harper off`, `:harper 0`, `:spellcheck off`, `:spell off` | — |
| `checker.show` | Show the checker's note for the word here | `K` | — | — |
| `view.raw` | Show the note as markdown source | `space r` | `:raw`, `:source`, `:src` | — |
| `view.live` | Show the note with live preview | `space l` | `:live` | — |
| `view.preview` | Preview the note, without editing it | `space p` | `:preview`, `:view` | — |
| `focus.toggle` | Dim everything but the block the cursor is in | `space d` | `:focus [on|off|block|section]`, `:dim [on|off|block|section]` | `[on|off|block|section]` |
| `typewriter.toggle` | Keep the cursor's line near the middle of the window | — | `:typewriter [on|off]` | `[on|off]` |
| `tree.toggle` | Show or hide the file tree | `space e` | `:tree [on|off]` | `[on|off]` |
| `window.left` | Focus the file tree | `ctrl-w h` | — | — |
| `window.right` | Focus the note | `ctrl-w l` | — | — |
| `note.undo` | Undo | `u, ctrl-z` | — | — |
| `note.redo` | Redo | `ctrl-r, ctrl-y, ctrl-shift-z` | — | — |
| `clipboard.copy` | Copy the selection, or the last yank, to the clipboard | `ctrl-shift-c, space y` | `:y`, `:yank`, `:clip`, `:copy` | — |
| `clipboard.paste` | Paste from the clipboard | `ctrl-shift-v` | `:paste`, `:p` | — |
| `vault.props` | Show the open note's properties | — | `:props`, `:properties` | — |
| `vault.keys` | List the vault's property keys | — | `:keys` | — |
| `vault.query` | Run a saved query | — | `:query` | — |
| `vault.save-query` | Save a search as a query | — | `:save-query [search]` | `[search]` |
| `vault.rename-key` | Rename a property key across the vault | — | `:rename-key [old] [new]` | `[old] [new]` |
| `vault.wordcount` | Count the vault's words | — | `:wordcount [folder]`, `:wc [folder]` | `[folder]` |
| `review.focus` | Review the agent edits that are waiting | — | `:review`, `:pending` | — |
| `review.next` | Next pending hunk | `j in the strip, ]a in the note` | — | — |
| `review.prev` | Previous pending hunk | `k in the strip, [a in the note` | — | — |
| `review.accept` | Accept the pending hunk under the marker | `y in the strip` | — | — |
| `review.reject` | Reject the pending hunk under the marker | `x in the strip` | — | — |
| `review.open-note` | Open the note the pending hunk is about | `o in the strip` | — | — |
| `review.collapse` | Collapse or expand the pending proposal under the marker | `za in the strip` | — | — |
| `tab.new` | Open this note in a new tab | `ctrl-t` | `:tabnew`, `:tabedit` | — |
| `tab.next` | Go to the next tab | `gt` | `:tabnext`, `:tabn` | — |
| `tab.prev` | Go to the previous tab | `gT` | `:tabprev`, `:tabp` | — |
| `tab.close` | Close this tab, writing its note first | `the tab's ×` | `:tabclose`, `:tabc` | — |
| `tab.only` | Close every tab but this one | — | `:tabonly`, `:tabo` | — |
| `window.split` | Split this pane, stacked | `ctrl-w s` | `:split`, `:sp` | — |
| `window.vsplit` | Split this pane, side by side | `ctrl-w v` | `:vsplit`, `:vs` | — |
| `window.close` | Close this pane, saving its notes first | `ctrl-w q` | `:close`, `:clo` | — |
| `window.only` | Close every pane but this one | `ctrl-w o` | `:only`, `:on` | — |
| `tab.move` | Move this tab to a position | — | `:tabmove <n>` | `<n>` |
| `graph.open` | Show the vault's link graph | `space g` | `:graph`, `:g` | — |
| `graph.flat` | Graph: flat view | `2 (graph)` | `:graph2d` | — |
| `graph.local` | Graph: this note's neighbourhood | `[ ] (graph)` | `:graph-local [n]`, `:loc [n]` | `[n]` |
| `graph.global` | Graph: the whole vault again | `\ (graph)` | `:graph-global` | — |
| `graph.projected` | Graph: projected 3D view | `3 (graph)` | `:graph3d` | — |

The palette does not list `app.quit-force`, `note.write-quit`, `theme.export`, `review.next`, `review.prev`, `review.accept`, `review.reject`, `review.open-note`, `review.collapse`, `tab.next`, `tab.prev`, `tab.only`, `window.only`, `tab.move`: `note.write-quit` is save and then quit, two
actions rather than one, `app.quit-force` is the exit that throws unsaved
edits away (not a row to offer next to *Quit Shale* in a list you pick
from), and the `review.*` keys other than `review.focus` mean nothing while
the review strip is off screen — which is only when nothing is waiting.
