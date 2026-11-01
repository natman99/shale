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
        // Likewise for the `space r` view chord: vim's `r` (replace the character
        // under the cursor) is not a Shale command yet, and the engine says so.
        VimR,
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
        // `:q!`. No key is bound to it: the command line is the only way in, and the
        // bang the user types is the whole point of it.
        QuitForce,
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
        // Walking the chain of notes the user came from (`ctrl-o` back, `ctrl-i`
        // forward), the way vim and neovim do.
        JumpBack,
        JumpForward,
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
        // The note's three views: the markdown source, Shale's live preview, and
        // fully rendered preview, which takes no edits (`:raw`, `:live`,
        // `:preview`).
        ViewRaw,
        ViewLive,
        ViewPreview,
        // Focus mode: dim everything but the block the cursor is in (`space d`,
        // `:focus`). A toggle both ways in, because the mode is a preference the
        // user moves in and out of while writing.
        ToggleFocus,
        // Typewriter scroll: keep the cursor's line near the middle of the window
        // (`:typewriter`).
        ToggleTypewriter,
        // Putting the selection, or the last yank, on the **system** clipboard
        // (`space y`, `:yank`). The way out; `ctrl-shift-v` / `:paste` is the way
        // back in, and it fills the vim register too.
        YankClipboard,
        // How many words the vault holds, counted from the index rather than by
        // reading every note again (`:wordcount [folder]`). The palette row counts
        // the whole vault; the command line can scope it to one folder.
        WordCount,
    ]
);

/// Choosing a row of the popup by its number. It carries the row, so it is a
/// struct rather than a name in the list above; the popup turns it back into
/// whichever of its own actions that row stands for.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct ApplyRow(pub usize);
