//! Every action the keymap names. gpui's `actions!` macro puts each one in a
//! module, and the name it prints in logs and in the tests is that module path
//! plus the action (`shale_editor::VimJ`, `shale::NewNote`), so the two modules
//! here are load-bearing, not just namespaces.

use gpui::actions;

actions!(
    shale_editor,
    [
        VimH,
        VimJ,
        VimK,
        VimL,
        VimW,
        VimB,
        VimE,
        VimF,
        VimUpperF,
        VimT,
        VimUpperT,
        VimSpace,
        VimZero,
        VimCaret,
        VimDollar,
        VimG,
        VimUpperG,
        VimX,
        // Bound so that the `space s` leader chord can reach the vim engine; `s`
        // on its own is not a Shale command yet.
        VimS,
        VimD,
        VimC,
        VimY,
        VimP,
        VimUpperP,
        VimU,
        VimDigit1,
        VimDigit2,
        VimDigit3,
        VimDigit4,
        VimDigit5,
        VimDigit6,
        VimDigit7,
        VimDigit8,
        VimDigit9,
        VimI,
        VimUpperI,
        VimA,
        VimUpperA,
        VimO,
        VimUpperO,
        VimV,
        VimUpperV,
        VimUpperH,
        VimUpperJ,
        VimUpperK,
        VimUpperL,
        VimUpperD,
        VimPipe,
        VimBackslash,
        VimAltH,
        VimAltJ,
        VimAltK,
        VimAltL,
        VimZ,
        VimUpperR,
        VimUpperM,
        VimUpperE,
        VimColon,
        VimDot,
        VimEscape,
        Newline,
        NextLine,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        Backspace,
        DeleteForward,
        // `ctrl-backspace` in insert mode: delete the word before the cursor, the
        // same pair normal-mode `db` makes.
        DeleteWordBack,
        Undo,
        Redo,
        CopyText,
        PasteText,
    ]
);
actions!(
    shale,
    [
        Save,
        Quit,
        NewNote,
        NewFolder,
        RenameNote,
        DeleteNote,
        FocusTree,
        FocusEditor,
        TreeUp,
        TreeDown,
        TreeCollapse,
        TreeExpand,
        TreeOpen,
        QuickOpen,
        // Searching every note's text (`ctrl-shift-f`).
        SearchVault,
        // Searching the note that is open (`/`), and stepping through its matches
        // (`n` and `N`). The query lives in the window, not the field, so `n` keeps
        // working after the field has closed.
        SearchBuffer,
        SearchNext,
        SearchPrev,
        CommandPalette,
        // vim's `:`: a command line, where a word names one command exactly. It is
        // kept apart from the palette precisely because the palette fuzzy-matches a
        // typed letter, and `:w` must not become "the first thing that matches w".
        RunCommandLine,
        // Pick a palette and see it straight away (`theme`, or the palette).
        ChangeTheme,
        // Starting and stopping the spell and grammar checker (`harper on|off`).
        CheckerOn,
        CheckerOff,
        // Moving the keyboard between the window's panels (`ctrl-w h`/`ctrl-w l`).
        // A split view will grow these into stepping sideways between panes.
        WindowLeft,
        WindowRight,
        // The keys a one-line field takes that are not printable text. The prompt
        // and every picker bind these in their own context, because the field is
        // the same widget in both.
        FieldSubmit,
        FieldCancel,
        FieldBackspace,
        FieldDelete,
        // `ctrl-backspace` in a field: the word before the caret. Bound in the
        // prompt and picker contexts, which are the two that hold a field.
        FieldDeleteWordBack,
        FieldLeft,
        FieldRight,
        // Moving the highlighted row of a picker.
        PickerUp,
        PickerDown,
        // The checker's note about the word under the cursor, in a popup over the
        // note (vim's `K`, and the pointer resting on a marked word).
        ShowNote,
    ]
);

/// Choosing a row of the popup by its number. It carries the row, so it is a
/// struct rather than a name in the list above; the popup turns it back into
/// whichever of its own actions that row stands for.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct ApplyRow(pub usize);
