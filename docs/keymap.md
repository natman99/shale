# Shale's keymap

Generated from the bindings built in `crates/app/src/workspace/keys.rs` — do not
edit by hand. A test fails when this file and the keymap disagree;
`SHALE_UPDATE_DOCS=1 cargo test -p shale-app` rewrites it.

The sections are gpui's **contexts**, which is what decides which surface has the
keyboard: the same letter is a motion in a note, a row move in the tree, and a
node move in the graph. **Keys** are what to press, in gpui's spelling (`ctrl-w h`,
`shift-g` for `G`). **Runs** is the command id the action reaches, when it is one of
the registry's commands — `docs/commands.md` is their table.

This table is what **gpui** knows. The note's own vocabulary — counts, the `space`
and `g` leader chords, marks, `z` and `[`/`]` commands, text objects — is not here,
because those keys reach the vim engine, which decides what they mean for the mode
in hand. `docs/keys.md` is the human map of both halves.

### everywhere (no context)

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `ctrl-s` | Save the note | `Save` | `note.save` |
| `ctrl-q` | Quit Shale | `Quit` | `app.quit` |
| `ctrl-n` | New note | `NewNote` | `note.new` |
| `ctrl-shift-n` | New folder | `NewFolder` | `folder.new` |
| `ctrl-shift-r` | Rename the selected note | `RenameNote` | `note.rename` |
| `ctrl-shift-d` | Delete the selected note | `DeleteNote` | `note.delete` |
| `ctrl-o` | Jump back to the previous note | `JumpBack` | `jump.back` |
| `ctrl-i` | Jump forward to the next note | `JumpForward` | `jump.forward` |
| `ctrl-shift-f` | Search the vault | `SearchVault` | `vault.search` |
| `ctrl-t` | Open this note in a new tab | `TabNew` | `tab.new` |
| `ctrl-shift-p` | open the command palette | `CommandPalette` | — |
| `ctrl-]` | step into the embed on this line | `EnterEmbed` | — |
| `ctrl-w h` | hand the keyboard to the pane on the left | `WindowLeft` | `window.left` |
| `ctrl-w l` | hand the keyboard to the pane on the right | `WindowRight` | `window.right` |
| `ctrl-w j` | hand the keyboard to the pane below | `WindowDown` | — |
| `ctrl-w k` | hand the keyboard to the pane above | `WindowUp` | — |
| `ctrl-w s` | split this pane, keeping the top half | `WindowSplitDown` | `window.split` |
| `ctrl-w v` | split this pane, keeping the left half | `WindowSplitRight` | `window.vsplit` |
| `ctrl-w S` | split down, this note in both halves | `WindowSplitDownHere` | — |
| `ctrl-w V` | split right, this note in both halves | `WindowSplitRightHere` | — |
| `ctrl-w q` | close this pane | `WindowClose` | `window.close` |
| `ctrl-w c` | close this pane | `WindowClose` | `window.close` |
| `ctrl-w o` | keep only this pane in the window | `WindowOnly` | `window.only` |
| `ctrl-w =` | even the panes out | `WindowEqualize` | — |
| `ctrl-w >` | grow this pane along its split | `WindowGrow` | — |
| `ctrl-w <` | shrink this pane along its split | `WindowShrink` | — |
| `ctrl-w +` | grow this pane along its split | `WindowGrow` | — |
| `ctrl-w -` | shrink this pane along its split | `WindowShrink` | — |

### while `vim-normal` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `h` | the vim engine takes this key | `VimH` | — |
| `j` | the vim engine takes this key | `VimJ` | — |
| `k` | the vim engine takes this key | `VimK` | — |
| `l` | the vim engine takes this key | `VimL` | — |
| `w` | the vim engine takes this key | `VimW` | — |
| `b` | the vim engine takes this key | `VimB` | — |
| `e` | the vim engine takes this key | `VimE` | — |
| `f` | the vim engine takes this key | `VimF` | — |
| `shift-f` | the vim engine takes this key | `VimUpperF` | — |
| `t` | the vim engine takes this key | `VimT` | — |
| `shift-t` | the vim engine takes this key | `VimUpperT` | — |
| `space` | the vim engine takes this key | `VimSpace` | — |
| `0` | the vim engine takes this key | `VimZero` | — |
| `^` | the vim engine takes this key | `VimCaret` | — |
| `$` | the vim engine takes this key | `VimDollar` | — |
| `g` | the vim engine takes this key | `VimG` | — |
| `shift-g` | the vim engine takes this key | `VimUpperG` | — |
| `d` | the vim engine takes this key | `VimD` | — |
| `c` | the vim engine takes this key | `VimC` | — |
| `y` | the vim engine takes this key | `VimY` | — |
| `p` | the vim engine takes this key | `VimP` | — |
| `shift-p` | the vim engine takes this key | `VimUpperP` | — |
| `x` | the vim engine takes this key | `VimX` | — |
| `i` | the vim engine takes this key | `VimI` | — |
| `shift-i` | the vim engine takes this key | `VimUpperI` | — |
| `a` | the vim engine takes this key | `VimA` | — |
| `shift-a` | the vim engine takes this key | `VimUpperA` | — |
| `o` | the vim engine takes this key | `VimO` | — |
| `shift-o` | the vim engine takes this key | `VimUpperO` | — |
| `v` | the vim engine takes this key | `VimV` | — |
| `shift-v` | the vim engine takes this key | `VimUpperV` | — |
| `shift-h` | the vim engine takes this key | `VimUpperH` | — |
| `shift-j` | the vim engine takes this key | `VimUpperJ` | — |
| `shift-k` | the vim engine takes this key | `VimUpperK` | — |
| `shift-l` | the vim engine takes this key | `VimUpperL` | — |
| `(` | the vim engine takes this key | `VimOpenParen` | — |
| `)` | the vim engine takes this key | `VimCloseParen` | — |
| `{` | the vim engine takes this key | `VimOpenBrace` | — |
| `}` | the vim engine takes this key | `VimCloseBrace` | — |
| `<` | the vim engine takes this key | `VimLess` | — |
| `>` | the vim engine takes this key | `VimGreater` | — |
| `"` | the vim engine takes this key | `VimQuote` | — |
| `'` | the vim engine takes this key | `VimApostrophe` | — |
| `;` | the vim engine takes this key | `VimSemicolon` | — |
| `,` | the vim engine takes this key | `VimComma` | — |
| `escape` | the vim engine takes this key | `VimEscape` | — |
| `1` | the vim engine takes this key | `VimDigit1` | — |
| `2` | the vim engine takes this key | `VimDigit2` | — |
| `3` | the vim engine takes this key | `VimDigit3` | — |
| `4` | the vim engine takes this key | `VimDigit4` | — |
| `5` | the vim engine takes this key | `VimDigit5` | — |
| `6` | the vim engine takes this key | `VimDigit6` | — |
| `7` | the vim engine takes this key | `VimDigit7` | — |
| `8` | the vim engine takes this key | `VimDigit8` | — |
| `9` | the vim engine takes this key | `VimDigit9` | — |
| `ctrl-z` | Undo | `Undo` | `note.undo` |
| `ctrl-r` | Redo | `Redo` | `note.redo` |
| `ctrl-shift-z` | Redo | `Redo` | `note.redo` |
| `ctrl-y` | Redo | `Redo` | `note.redo` |
| `ctrl-shift-c` | Copy the selection, or the last yank, to the clipboard | `CopyText` | `clipboard.copy` |
| `ctrl-shift-v` | Paste from the clipboard | `PasteText` | `clipboard.paste` |
| `u` | the vim engine takes this key | `VimU` | — |
| `/` | search this note | `SearchBuffer` | — |
| `n` | next match in this note | `SearchNext` | — |
| `shift-n` | previous match in this note | `SearchPrev` | — |
| `s` | the vim engine takes this key | `VimS` | — |
| `r` | the vim engine takes this key | `VimR` | — |
| `.` | the vim engine takes this key | `VimDot` | — |
| `enter` | go to the next line | `NextLine` | — |
| `shift-d` | the vim engine takes this key | `VimUpperD` | — |
| `~` | the vim engine takes this key | `VimTilde` | — |
| `shift-s` | the vim engine takes this key | `VimUpperS` | — |
| `shift-c` | the vim engine takes this key | `VimUpperC` | — |
| `shift-x` | the vim engine takes this key | `VimUpperX` | — |
| `shift-y` | the vim engine takes this key | `VimUpperY` | — |
| `|` | the vim engine takes this key | `VimPipe` | — |
| `\` | the vim engine takes this key | `VimBackslash` | — |
| `alt-h` | the vim engine takes this key | `VimAltH` | — |
| `alt-j` | the vim engine takes this key | `VimAltJ` | — |
| `alt-k` | the vim engine takes this key | `VimAltK` | — |
| `alt-l` | the vim engine takes this key | `VimAltL` | — |
| `z` | the vim engine takes this key | `VimZ` | — |
| `shift-r` | the vim engine takes this key | `VimUpperR` | — |
| `shift-m` | the vim engine takes this key | `VimUpperM` | — |
| `shift-e` | the vim engine takes this key | `VimUpperE` | — |
| `shift-w` | the vim engine takes this key | `VimUpperW` | — |
| `shift-b` | the vim engine takes this key | `VimUpperB` | — |
| `:` | the vim engine takes this key | `VimColon` | — |
| `[` | the vim engine takes this key | `VimOpenBracket` | — |
| `]` | the vim engine takes this key | `VimCloseBracket` | — |
| `left` | move the cursor left | `MoveLeft` | — |
| `right` | move the cursor right | `MoveRight` | — |
| `up` | move the cursor up | `MoveUp` | — |
| `down` | move the cursor down | `MoveDown` | — |

### while `vim-visual` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `h` | the vim engine takes this key | `VimH` | — |
| `j` | the vim engine takes this key | `VimJ` | — |
| `k` | the vim engine takes this key | `VimK` | — |
| `l` | the vim engine takes this key | `VimL` | — |
| `w` | the vim engine takes this key | `VimW` | — |
| `b` | the vim engine takes this key | `VimB` | — |
| `e` | the vim engine takes this key | `VimE` | — |
| `f` | the vim engine takes this key | `VimF` | — |
| `shift-f` | the vim engine takes this key | `VimUpperF` | — |
| `t` | the vim engine takes this key | `VimT` | — |
| `shift-t` | the vim engine takes this key | `VimUpperT` | — |
| `space` | the vim engine takes this key | `VimSpace` | — |
| `0` | the vim engine takes this key | `VimZero` | — |
| `^` | the vim engine takes this key | `VimCaret` | — |
| `$` | the vim engine takes this key | `VimDollar` | — |
| `g` | the vim engine takes this key | `VimG` | — |
| `shift-g` | the vim engine takes this key | `VimUpperG` | — |
| `d` | the vim engine takes this key | `VimD` | — |
| `c` | the vim engine takes this key | `VimC` | — |
| `y` | the vim engine takes this key | `VimY` | — |
| `p` | the vim engine takes this key | `VimP` | — |
| `shift-p` | the vim engine takes this key | `VimUpperP` | — |
| `x` | the vim engine takes this key | `VimX` | — |
| `i` | the vim engine takes this key | `VimI` | — |
| `shift-i` | the vim engine takes this key | `VimUpperI` | — |
| `a` | the vim engine takes this key | `VimA` | — |
| `shift-a` | the vim engine takes this key | `VimUpperA` | — |
| `o` | the vim engine takes this key | `VimO` | — |
| `shift-o` | the vim engine takes this key | `VimUpperO` | — |
| `v` | the vim engine takes this key | `VimV` | — |
| `shift-v` | the vim engine takes this key | `VimUpperV` | — |
| `shift-h` | the vim engine takes this key | `VimUpperH` | — |
| `shift-j` | the vim engine takes this key | `VimUpperJ` | — |
| `shift-k` | the vim engine takes this key | `VimUpperK` | — |
| `shift-l` | the vim engine takes this key | `VimUpperL` | — |
| `(` | the vim engine takes this key | `VimOpenParen` | — |
| `)` | the vim engine takes this key | `VimCloseParen` | — |
| `{` | the vim engine takes this key | `VimOpenBrace` | — |
| `}` | the vim engine takes this key | `VimCloseBrace` | — |
| `<` | the vim engine takes this key | `VimLess` | — |
| `>` | the vim engine takes this key | `VimGreater` | — |
| `"` | the vim engine takes this key | `VimQuote` | — |
| `'` | the vim engine takes this key | `VimApostrophe` | — |
| `;` | the vim engine takes this key | `VimSemicolon` | — |
| `,` | the vim engine takes this key | `VimComma` | — |
| `escape` | the vim engine takes this key | `VimEscape` | — |
| `1` | the vim engine takes this key | `VimDigit1` | — |
| `2` | the vim engine takes this key | `VimDigit2` | — |
| `3` | the vim engine takes this key | `VimDigit3` | — |
| `4` | the vim engine takes this key | `VimDigit4` | — |
| `5` | the vim engine takes this key | `VimDigit5` | — |
| `6` | the vim engine takes this key | `VimDigit6` | — |
| `7` | the vim engine takes this key | `VimDigit7` | — |
| `8` | the vim engine takes this key | `VimDigit8` | — |
| `9` | the vim engine takes this key | `VimDigit9` | — |
| `ctrl-z` | Undo | `Undo` | `note.undo` |
| `ctrl-r` | Redo | `Redo` | `note.redo` |
| `ctrl-shift-z` | Redo | `Redo` | `note.redo` |
| `ctrl-y` | Redo | `Redo` | `note.redo` |
| `ctrl-shift-c` | Copy the selection, or the last yank, to the clipboard | `CopyText` | `clipboard.copy` |
| `ctrl-shift-v` | Paste from the clipboard | `PasteText` | `clipboard.paste` |

### while `vim-insert` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `escape` | the vim engine takes this key | `VimEscape` | — |
| `enter` | break the line here | `Newline` | — |
| `backspace` | delete the character before the cursor | `Backspace` | — |
| `ctrl-backspace` | delete the word before the cursor | `DeleteWordBack` | — |
| `delete` | delete the character under the cursor | `DeleteForward` | — |
| `tab` | insert a tab character | `InsertTab` | — |
| `shift-tab` | remove one indent level from the line | `OutdentLine` | — |
| `left` | move the cursor left | `MoveLeft` | — |
| `right` | move the cursor right | `MoveRight` | — |
| `up` | move the cursor up | `MoveUp` | — |
| `down` | move the cursor down | `MoveDown` | — |
| `ctrl-z` | Undo | `Undo` | `note.undo` |
| `ctrl-r` | Redo | `Redo` | `note.redo` |
| `ctrl-shift-z` | Redo | `Redo` | `note.redo` |
| `ctrl-y` | Redo | `Redo` | `note.redo` |
| `ctrl-shift-c` | Copy the selection, or the last yank, to the clipboard | `CopyText` | `clipboard.copy` |
| `ctrl-shift-v` | Paste from the clipboard | `PasteText` | `clipboard.paste` |

### while `tree` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `j` | move the selection down | `TreeDown` | — |
| `k` | move the selection up | `TreeUp` | — |
| `down` | move the selection down | `TreeDown` | — |
| `up` | move the selection up | `TreeUp` | — |
| `h` | collapse this folder, or step out of it | `TreeCollapse` | — |
| `l` | open this folder, or step into it | `TreeExpand` | — |
| `left` | collapse this folder, or step out of it | `TreeCollapse` | — |
| `right` | open this folder, or step into it | `TreeExpand` | — |
| `enter` | open this note | `TreeOpen` | — |
| `space e` | Show or hide the file tree | `TreeToggle` | `tree.toggle` |
| `escape` | hand the keyboard to the tab on screen | `FocusEditor` | — |
| `n` | New note | `NewNote` | `note.new` |
| `shift-n` | New folder | `NewFolder` | `folder.new` |
| `r` | Rename the selected note | `RenameNote` | `note.rename` |
| `shift-d` | Delete the selected note | `DeleteNote` | `note.delete` |

### while `sidebar` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `j` | move the selection down | `SidebarDown` | — |
| `k` | move the selection up | `SidebarUp` | — |
| `down` | move the selection down | `SidebarDown` | — |
| `up` | move the selection up | `SidebarUp` | — |
| `h` | fold this section in the note | `SidebarCollapse` | — |
| `l` | unfold this section in the note | `SidebarExpand` | — |
| `left` | fold this section in the note | `SidebarCollapse` | — |
| `right` | unfold this section in the note | `SidebarExpand` | — |
| `enter` | jump the note to this section | `SidebarOpen` | — |
| `[` | show the previous view in the strip | `SidebarPrevView` | — |
| `]` | show the next view in the strip | `SidebarNextView` | — |
| `space b` | Show or hide the right-hand strip | `SidebarToggle` | `sidebar.toggle` |
| `escape` | hand the keyboard to the tab on screen | `FocusEditor` | — |

### while `prompt` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `enter` | accept what is typed | `FieldSubmit` | — |
| `escape` | dismiss without accepting | `FieldCancel` | — |
| `backspace` | delete the character before the caret | `FieldBackspace` | — |
| `delete` | delete the character at the caret | `FieldDelete` | — |
| `ctrl-backspace` | delete the word before the caret | `FieldDeleteWordBack` | — |
| `left` | move the caret left | `FieldLeft` | — |
| `right` | move the caret right | `FieldRight` | — |
| `shift-left` | extend the selection left | `FieldSelectLeft` | — |
| `shift-right` | extend the selection right | `FieldSelectRight` | — |
| `shift-up` | select to the start of the line | `FieldSelectToStart` | — |
| `shift-down` | select to the end of the line | `FieldSelectToEnd` | — |
| `home` | move the caret to the start of the line | `FieldHome` | — |
| `end` | move the caret to the end of the line | `FieldEnd` | — |
| `shift-home` | select to the start of the line | `FieldSelectToStart` | — |
| `shift-end` | select to the end of the line | `FieldSelectToEnd` | — |
| `ctrl-left` | move the caret to the start of the word | `FieldWordLeft` | — |
| `ctrl-right` | move the caret to the end of the word | `FieldWordRight` | — |
| `ctrl-shift-left` | extend the selection to the start of the word | `FieldSelectWordLeft` | — |
| `ctrl-shift-right` | extend the selection to the end of the word | `FieldSelectWordRight` | — |
| `ctrl-a` | select the whole line | `FieldSelectAll` | — |
| `ctrl-c` | copy the selection to the clipboard | `FieldCopy` | — |
| `ctrl-x` | cut the selection to the clipboard | `FieldCut` | — |
| `ctrl-v` | paste the clipboard over the selection | `FieldPaste` | — |

### while `picker` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `enter` | accept what is typed | `FieldSubmit` | — |
| `escape` | dismiss without accepting | `FieldCancel` | — |
| `down` | move the highlighted row down | `PickerDown` | — |
| `up` | move the highlighted row up | `PickerUp` | — |
| `backspace` | delete the character before the caret | `FieldBackspace` | — |
| `delete` | delete the character at the caret | `FieldDelete` | — |
| `ctrl-backspace` | delete the word before the caret | `FieldDeleteWordBack` | — |
| `left` | move the caret left | `FieldLeft` | — |
| `right` | move the caret right | `FieldRight` | — |
| `shift-left` | extend the selection left | `FieldSelectLeft` | — |
| `shift-right` | extend the selection right | `FieldSelectRight` | — |
| `shift-up` | select to the start of the line | `FieldSelectToStart` | — |
| `shift-down` | select to the end of the line | `FieldSelectToEnd` | — |
| `home` | move the caret to the start of the line | `FieldHome` | — |
| `end` | move the caret to the end of the line | `FieldEnd` | — |
| `shift-home` | select to the start of the line | `FieldSelectToStart` | — |
| `shift-end` | select to the end of the line | `FieldSelectToEnd` | — |
| `ctrl-left` | move the caret to the start of the word | `FieldWordLeft` | — |
| `ctrl-right` | move the caret to the end of the word | `FieldWordRight` | — |
| `ctrl-shift-left` | extend the selection to the start of the word | `FieldSelectWordLeft` | — |
| `ctrl-shift-right` | extend the selection to the end of the word | `FieldSelectWordRight` | — |
| `ctrl-a` | select the whole line | `FieldSelectAll` | — |
| `ctrl-c` | copy the selection to the clipboard | `FieldCopy` | — |
| `ctrl-x` | cut the selection to the clipboard | `FieldCut` | — |
| `ctrl-v` | paste the clipboard over the selection | `FieldPaste` | — |

### while `popup` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `escape` | dismiss without accepting | `FieldCancel` | — |
| `enter` | accept what is typed | `FieldSubmit` | — |
| `j` | move the highlighted row down | `PickerDown` | — |
| `k` | move the highlighted row up | `PickerUp` | — |
| `down` | move the highlighted row down | `PickerDown` | — |
| `up` | move the highlighted row up | `PickerUp` | — |
| `1` | take popup row 1 | `ApplyRow` | — |
| `2` | take popup row 2 | `ApplyRow` | — |
| `3` | take popup row 3 | `ApplyRow` | — |
| `4` | take popup row 4 | `ApplyRow` | — |
| `5` | take popup row 5 | `ApplyRow` | — |
| `6` | take popup row 6 | `ApplyRow` | — |
| `7` | take popup row 7 | `ApplyRow` | — |
| `8` | take popup row 8 | `ApplyRow` | — |
| `9` | take popup row 9 | `ApplyRow` | — |

### while `review` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `j` | Next pending hunk | `ReviewNext` | `review.next` |
| `k` | Previous pending hunk | `ReviewPrev` | `review.prev` |
| `y` | Accept the pending hunk under the marker | `ReviewAccept` | `review.accept` |
| `x` | Reject the pending hunk under the marker | `ReviewReject` | `review.reject` |
| `o` | Open the note the pending hunk is about | `ReviewOpenNote` | `review.open-note` |
| `z a` | Collapse or expand the pending proposal under the marker | `ReviewCollapse` | `review.collapse` |
| `escape` | hand the keyboard to the tab on screen | `FocusEditor` | — |

### while `graph` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `:` | type a `:` command | `RunCommandLine` | — |
| `space f` | Open a note | `QuickOpen` | `note.open` |
| `space s` | Search the vault | `SearchVault` | `vault.search` |
| `space e` | Show or hide the file tree | `TreeToggle` | `tree.toggle` |
| `space d` | Dim everything but the block the cursor is in | `ToggleFocus` | `focus.toggle` |
| `space g` | Show the vault's link graph | `OpenGraph` | `graph.open` |
| `g t` | Go to the next tab | `TabNext` | `tab.next` |
| `g T` | Go to the previous tab | `TabPrev` | `tab.prev` |
| `escape` | put down the chosen node | `GraphClearSelection` | — |
| `enter` | open the chosen node's note | `GraphOpen` | — |
| `f` | frame the whole graph | `GraphFit` | — |
| `/` | open the filter strip | `GraphFilterOpen` | — |
| `2` | switch the graph between its two spaces | `GraphMode` | `graph.projected` |
| `3` | switch the graph between its two spaces | `GraphMode` | `graph.projected` |
| `+` | magnify at the cursor | `GraphZoom` | — |
| `-` | magnify at the cursor | `GraphZoom` | — |
| `[` | narrow or widen the local graph | `GraphLocalStep` | — |
| `]` | narrow or widen the local graph | `GraphLocalStep` | — |
| `\` | show the whole vault again | `GraphLocal` | `graph.global` |
| `left` | move the view | `GraphPan` | — |
| `shift-left` | move the view, three times as far | `GraphPan` | — |
| `ctrl-shift-left` | move the chosen node | `GraphSelect` | — |
| `right` | move the view | `GraphPan` | — |
| `shift-right` | move the view, three times as far | `GraphPan` | — |
| `ctrl-shift-right` | move the chosen node | `GraphSelect` | — |
| `up` | move the view | `GraphPan` | — |
| `shift-up` | move the view, three times as far | `GraphPan` | — |
| `ctrl-shift-up` | move the chosen node | `GraphSelect` | — |
| `down` | move the view | `GraphPan` | — |
| `shift-down` | move the view, three times as far | `GraphPan` | — |
| `ctrl-shift-down` | move the chosen node | `GraphSelect` | — |
| `h` | move the chosen node | `GraphSelect` | — |
| `l` | move the chosen node | `GraphSelect` | — |
| `k` | move the chosen node | `GraphSelect` | — |
| `j` | move the chosen node | `GraphSelect` | — |

### while `graph-filter` has the keyboard

| keys | what it does | action | runs |
| --- | --- | --- | --- |
| `escape` | close the filter strip | `GraphFilterClose` | — |
| `enter` | close the filter strip | `GraphFilterClose` | — |
| `alt-o` | show or hide unlinked notes | `GraphFilterOrphans` | — |
| `alt-x` | show or hide links to notes that do not exist | `GraphFilterMissing` | — |
| `alt-t` | show or hide tags | `GraphFilterTags` | — |
| `backspace` | delete the character before the caret | `FieldBackspace` | — |
| `delete` | delete the character at the caret | `FieldDelete` | — |
| `ctrl-backspace` | delete the word before the caret | `FieldDeleteWordBack` | — |
| `left` | move the caret left | `FieldLeft` | — |
| `right` | move the caret right | `FieldRight` | — |
| `shift-left` | extend the selection left | `FieldSelectLeft` | — |
| `shift-right` | extend the selection right | `FieldSelectRight` | — |
| `shift-up` | select to the start of the line | `FieldSelectToStart` | — |
| `shift-down` | select to the end of the line | `FieldSelectToEnd` | — |
| `home` | move the caret to the start of the line | `FieldHome` | — |
| `end` | move the caret to the end of the line | `FieldEnd` | — |
| `shift-home` | select to the start of the line | `FieldSelectToStart` | — |
| `shift-end` | select to the end of the line | `FieldSelectToEnd` | — |
| `ctrl-left` | move the caret to the start of the word | `FieldWordLeft` | — |
| `ctrl-right` | move the caret to the end of the word | `FieldWordRight` | — |
| `ctrl-shift-left` | extend the selection to the start of the word | `FieldSelectWordLeft` | — |
| `ctrl-shift-right` | extend the selection to the end of the word | `FieldSelectWordRight` | — |
| `ctrl-a` | select the whole line | `FieldSelectAll` | — |
| `ctrl-c` | copy the selection to the clipboard | `FieldCopy` | — |
| `ctrl-x` | cut the selection to the clipboard | `FieldCut` | — |
| `ctrl-v` | paste the clipboard over the selection | `FieldPaste` | — |

