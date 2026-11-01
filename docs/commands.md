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
| `note.save` | Save the note | `ctrl-s` | `:w`, `:write` | — |
| `app.quit` | Quit Shale | `ctrl-q` | `:q`, `:quit` | — |
| `app.quit-force` | Quit Shale, discarding the note's unsaved edits | — | `:q!`, `:quit!` | — |
| `note.write-quit` | Write the note, then quit | — | `:wq`, `:x`, `:xit`, `:exit` | — |
| `note.new` | New note | `ctrl-n, n in the tree` | — | — |
| `folder.new` | New folder | `ctrl-shift-n, shift-n in the tree` | — | — |
| `note.rename` | Rename the selected note | `ctrl-shift-r, r in the tree` | — | — |
| `note.delete` | Delete the selected note | `ctrl-shift-d, shift-d in the tree` | — | — |
| `note.open` | Open a note | `space f, g f` | `:e`, `:edit`, `:open` | — |
| `jump.back` | Jump back to the previous note | `ctrl-o` | — | — |
| `jump.forward` | Jump forward to the next note | `ctrl-i` | — | — |
| `vault.search` | Search the vault | `ctrl-shift-f, space s` | — | — |
| `theme.change` | Change the theme | — | `:theme [name]` | `[name]` |
| `checker.on` | Turn the spellchecker on | — | `:harper on`, `:harper 1`, `:spellcheck on`, `:spell on` | — |
| `checker.off` | Turn the spellchecker off | — | `:harper off`, `:harper 0`, `:spellcheck off`, `:spell off` | — |
| `checker.show` | Show the checker's note for the word here | `K` | — | — |
| `view.raw` | Show the note as markdown source | `space r` | `:raw`, `:source`, `:src` | — |
| `view.live` | Show the note with live preview | `space l` | `:live` | — |
| `view.preview` | Preview the note, without editing it | `space p` | `:preview`, `:view` | — |
| `focus.toggle` | Dim everything but the block the cursor is in | `space d` | `:focus [on|off|block|section]`, `:dim [on|off|block|section]` | `[on|off|block|section]` |
| `typewriter.toggle` | Keep the cursor's line near the middle of the window | — | `:typewriter [on|off]` | `[on|off]` |
| `window.left` | Focus the file tree | `ctrl-w h` | — | — |
| `window.right` | Focus the note | `ctrl-w l` | — | — |
| `note.undo` | Undo | `u, ctrl-z` | — | — |
| `note.redo` | Redo | `ctrl-r, ctrl-y, ctrl-shift-z` | — | — |
| `clipboard.copy` | Copy the selection to the clipboard | `ctrl-shift-c` | — | — |
| `clipboard.paste` | Paste from the clipboard | `ctrl-shift-v` | `:paste`, `:p` | — |
| `clipboard.yank` | Copy the selection, or the last yank, to the clipboard | `space y` | `:y`, `:yank`, `:clip`, `:copy` | — |
| `vault.wordcount` | Count the vault's words | — | `:wordcount [folder]`, `:wc [folder]` | `[folder]` |

The palette does not list `app.quit-force`, `note.write-quit`: `note.write-quit` is save and then quit, two
actions rather than one, and `app.quit-force` is the exit that throws unsaved
edits away, which is not a row to offer next to *Quit Shale* in a list you
pick from.
