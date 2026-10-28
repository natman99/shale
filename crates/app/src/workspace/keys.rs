//! The keymap: which keystroke means which action, and in which context.

use gpui::KeyBinding;

use crate::acts::*;

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
        KeyBinding::new("ctrl-b", FocusTree, None),
        // Quick-open: jump to a note by typing part of its name or path.
        KeyBinding::new("ctrl-o", QuickOpen, None),
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
    ]);
    bind!("vim-insert", [
        "escape" => VimEscape,
        "enter" => Newline,
        "backspace" => Backspace,
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
    // The sidebar, while it has the keyboard (`ctrl-b`). `escape` hands the
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
        "left" => FieldLeft,
        "right" => FieldRight,
    ]);
    // A picker: the same field keys, plus the two that move the highlighted row.
    // `j`/`k` are free here because the picker is drawn outside the tree and the
    // editor, so neither of their key contexts is in the focused stack.
    bind!("picker", [
        "enter" => FieldSubmit,
        "escape" => FieldCancel,
        "backspace" => FieldBackspace,
        "delete" => FieldDelete,
        "left" => FieldLeft,
        "right" => FieldRight,
        "j" => PickerDown,
        "k" => PickerUp,
        "down" => PickerDown,
        "up" => PickerUp,
    ]);
    keys
}
