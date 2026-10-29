//! The keymap: which keystroke means which action, and in which context.

use gpui::KeyBinding;

use crate::acts::*;
use crate::popup;

pub(crate) fn keybindings() -> Vec<KeyBinding> {
    // gpui's keystroke syntax separates modifiers with `-`, not `+`: `ctrl-z`.
    // A `+` is not a separator, so "ctrl+z" parses as one literal key named
    // "ctrl+z" that can never match anything the platform delivers.
    let mut keys = vec![
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-q", Quit, None),
        // The vault edits are available from anywhere, because they act on the
        // tree's selection or, failing that, the note that is open.
        KeyBinding::new("ctrl-n", NewNote, None),
        KeyBinding::new("ctrl-shift-n", NewFolder, None),
        KeyBinding::new("ctrl-shift-r", RenameNote, None),
        KeyBinding::new("ctrl-shift-d", DeleteNote, None),
        // Quick-open: jump to a note by typing part of its name or path.
        KeyBinding::new("ctrl-o", QuickOpen, None),
        // Vault search: every line that contains what is typed. The picker's rows
        // are the hits, and choosing one opens the note on that line.
        KeyBinding::new("ctrl-shift-f", SearchVault, None),
        // The command palette. `ctrl-shift-p` is the shape the user asked for;
        // vim's `:` is bound in the note's own context (see below).
        KeyBinding::new("ctrl-shift-p", CommandPalette, None),
        // Moving the keyboard between panels, vim-window style. gpui matches a
        // two-stroke binding natively: `ctrl-w` alone is kept as pending input
        // and replayed if the next key does not finish the chord.
        KeyBinding::new("ctrl-w h", WindowLeft, None),
        KeyBinding::new("ctrl-w l", WindowRight, None),
    ];
    macro_rules! bind {
        ($ctx:expr, [ $( $key:literal => $action:ident ),* $(,)? ]) => {
            $( keys.push(KeyBinding::new($key, $action, Some($ctx))); )*
        };
    }
    // Motions and operators are bound in both normal and visual mode; the vim
    // engine decides what each key means for the current mode.
    for context in ["vim-normal", "vim-visual"] {
        bind!(context, [
            "h" => VimH,
            "j" => VimJ,
            "k" => VimK,
            "l" => VimL,
            "w" => VimW,
            "b" => VimB,
            "e" => VimE,
            "f" => VimF,
            "shift-f" => VimUpperF,
            "t" => VimT,
            "shift-t" => VimUpperT,
            "space" => VimSpace,
            "0" => VimZero,
            "^" => VimCaret,
            "$" => VimDollar,
            "g" => VimG,
            "shift-g" => VimUpperG,
            "d" => VimD,
            "c" => VimC,
            "y" => VimY,
            "p" => VimP,
            "shift-p" => VimUpperP,
            "x" => VimX,
            "i" => VimI,
            "shift-i" => VimUpperI,
            "a" => VimA,
            "shift-a" => VimUpperA,
            "o" => VimO,
            "shift-o" => VimUpperO,
            "v" => VimV,
            "shift-v" => VimUpperV,
            // Table cell motions (see `shale_core::table`): the shifted forms
            // jump columns and rows. They are bound in visual mode too, where
            // they extend the selection.
            "shift-h" => VimUpperH,
            "shift-j" => VimUpperJ,
            "shift-k" => VimUpperK,
            "shift-l" => VimUpperL,
            "escape" => VimEscape,
            "1" => VimDigit1,
            "2" => VimDigit2,
            "3" => VimDigit3,
            "4" => VimDigit4,
            "5" => VimDigit5,
            "6" => VimDigit6,
            "7" => VimDigit7,
            "8" => VimDigit8,
            "9" => VimDigit9,
            "ctrl-z" => Undo,
            "ctrl-r" => Redo,
            "ctrl-shift-z" => Redo,
            "ctrl-y" => Redo,
            "ctrl-shift-c" => CopyText,
            "ctrl-shift-v" => PasteText,
        ]);
    }
    // Normal mode only: `u` is undo here (vim uses it in visual for
    // lowercasing), and `enter` is the next-line motion rather than a newline.
    // The table edits (`DD` deletes a column, `|` and `\` insert one) are
    // normal-mode commands; in visual mode `D` is the selection operator.
    // `alt+hjkl` moves a whole row or column of the table under the cursor.
    bind!("vim-normal", [
        "u" => VimU,
        // `/` searches the note that is open; `n` and `N` step through what it
        // found. The query lives in the window rather than in the field, so they
        // keep working after `enter` has closed the field. All three are free here:
        // `n` names a new note only in the tree, and the global new-note key is
        // `ctrl-n`.
        "/" => SearchBuffer,
        "n" => SearchNext,
        "shift-n" => SearchPrev,
        // Bound only so the `space s` leader chord reaches the vim engine, which
        // owns the chord because `space` is also the right motion. `s` by itself
        // is not a Shale command yet, and the engine says so.
        "s" => VimS,
        "." => VimDot,
        "enter" => NextLine,
        "shift-d" => VimUpperD,
        "|" => VimPipe,
        "\\" => VimBackslash,
        "alt-h" => VimAltH,
        "alt-j" => VimAltJ,
        "alt-k" => VimAltK,
        "alt-l" => VimAltL,
        // Folding (`za` toggle, `zc` close, `zo` open, `zd` remove, `zM` close
        // all, `zR`/`zE` open all). `z` opens the command and the second key is
        // read by the vim engine; `a`, `c`, `o` and `d` are already bound, so
        // only the shifted command letters need their own actions.
        "z" => VimZ,
        "shift-r" => VimUpperR,
        "shift-m" => VimUpperM,
        "shift-e" => VimUpperE,
        // vim's command line: one word, matched exactly (`:w`, `:q`, `:harper
        // off`). The fuzzy command palette is `ctrl-shift-p` above, and keeping the
        // two apart is what stops `:w` matching a command that merely contains a
        // `w`. Linux delivers shift+`;` as the `:` keysym with shift dropped, so
        // this is bindable directly.
        ":" => VimColon,
    ]);
    bind!("vim-insert", [
        "escape" => VimEscape,
        "enter" => Newline,
        "backspace" => Backspace,
        // `ctrl-backspace` deletes the word before the cursor, the same boundary
        // `b` moves over. vim's own key for this is `ctrl-w`, which Shale leaves to
        // the window commands (`ctrl-w h`/`ctrl-w l`).
        "ctrl-backspace" => DeleteWordBack,
        "delete" => DeleteForward,
        "left" => MoveLeft,
        "right" => MoveRight,
        "up" => MoveUp,
        "down" => MoveDown,
        "ctrl-z" => Undo,
        "ctrl-r" => Redo,
        "ctrl-shift-z" => Redo,
        "ctrl-y" => Redo,
        "ctrl-shift-c" => CopyText,
        "ctrl-shift-v" => PasteText,
    ]);
    // The sidebar, while it has the keyboard (`ctrl-w h`). `escape` hands the
    // keyboard back to the note. The same four vault edits are bound here so a
    // folder or note can be created, renamed or deleted without leaving the tree.
    bind!("tree", [
        "j" => TreeDown,
        "k" => TreeUp,
        "down" => TreeDown,
        "up" => TreeUp,
        "h" => TreeCollapse,
        "l" => TreeExpand,
        "left" => TreeCollapse,
        "right" => TreeExpand,
        "enter" => TreeOpen,
        "space" => TreeOpen,
        "escape" => FocusEditor,
        "n" => NewNote,
        "shift-n" => NewFolder,
        "r" => RenameNote,
        "shift-d" => DeleteNote,
    ]);
    // The one-line prompt. Printable characters never reach a keybinding — they
    // arrive through the input handler — so only the keys that are not text are
    // bound here.
    bind!("prompt", [
        "enter" => FieldSubmit,
        "escape" => FieldCancel,
        "backspace" => FieldBackspace,
        "delete" => FieldDelete,
        "ctrl-backspace" => FieldDeleteWordBack,
        "left" => FieldLeft,
        "right" => FieldRight,
    ]);
    // A picker: the same field keys, plus the two that move the highlighted row.
    // The arrows, and deliberately not `j`/`k`: the picker is a text field, and a
    // query may contain any letter at all. (`j`/`k` still work in the popup, which
    // has no field to type into.)
    bind!("picker", [
        "enter" => FieldSubmit,
        "escape" => FieldCancel,
        "backspace" => FieldBackspace,
        "delete" => FieldDelete,
        "ctrl-backspace" => FieldDeleteWordBack,
        "left" => FieldLeft,
        "right" => FieldRight,
        "down" => PickerDown,
        "up" => PickerUp,
    ]);
    // The popup. It is a sibling of the note and the tree, so — as with a picker —
    // `j`/`k` are free here, and `escape` closes it rather than leaving a vim
    // command half-typed. The rows are also reachable by number, which is the
    // fastest way to take the first offer; those bindings carry the row, so they
    // are pushed one by one rather than through `bind!`.
    bind!(popup::CONTEXT, [
        "escape" => FieldCancel,
        "enter" => FieldSubmit,
        "j" => PickerDown,
        "k" => PickerUp,
        "down" => PickerDown,
        "up" => PickerUp,
    ]);
    for (row, key) in ["1", "2", "3", "4", "5", "6", "7", "8", "9"]
        .into_iter()
        .enumerate()
    {
        keys.push(KeyBinding::new(key, ApplyRow(row), Some(popup::CONTEXT)));
    }
    keys
}
