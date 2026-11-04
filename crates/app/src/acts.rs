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
        // `]` and `[` are bound so the review-marker chords `]a`/`[a` reach the vim
        // engine, which owns the pair. They are bound as single keys rather than as a
        // two-stroke chord because gpui holds the first key of a chord as pending
        // input, and a literal `[` has to reach the engine as the argument of `f`, `t`
        // or an operator.
        VimOpenBracket,
        VimCloseBracket,
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
        // The one outward copy (`ctrl-shift-c`, `space y`, `:y`/`:yank`/`:clip`/
        // `:copy`): the selection when there is one, otherwise the last yank. The
        // window answers it, because only the window can talk to the system
        // clipboard; `ctrl-shift-v` / `:paste` is the way back in.
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
        // Put the file tree away, or bring it back (`space e`, `:tree`). The note takes
        // the width the tree had; nvim-tree and NERDTree answer `<leader>e` the same way.
        TreeToggle,
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
        // The split alphabet (roadmap P3.2), all under the `ctrl-w` prefix the
        // keymap already reserves for window commands.
        WindowSplitDown,
        WindowSplitRight,
        WindowSplitDownHere,
        WindowSplitRightHere,
        WindowClose,
        WindowOnly,
        WindowUp,
        WindowDown,
        WindowEqualize,
        // The tabs (roadmap P3.1). Tab pages live outside the `ctrl-w` prefix, which
        // is reserved for the window commands a split view will want: these are
        // `g t`/`g T` in the note, `:tabnext`/`:tabprev`/`:tabclose`/`:tabonly` on the
        // command line, and the tab bar under the pointer.
        TabNext,
        TabPrev,
        TabNew,
        TabClose,
        TabOnly,
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
        // How many words the vault holds, counted from the index rather than by
        // reading every note again (`:wordcount [folder]`). The palette row counts
        // the whole vault; the command line can scope it to one folder.
        WordCount,
        // Agent edit review (roadmap P2.5a). The strip is a focusable widget with a
        // key context of its own, so its bare `j`/`k`/`y`/`x`/`o`/`za` reach these
        // only while it has the keyboard; `]a`/`[a` in the note reach the same two
        // hopping commands, and the palette reaches `review.focus`. Nothing here
        // shadows a vim key in prose, which is the whole point of the two layers.
        ReviewFocus,
        ReviewNext,
        ReviewPrev,
        ReviewAccept,
        ReviewReject,
        ReviewOpenNote,
        ReviewCollapse,
        // The vault's graph (`space g`, `:graph`). One tab per window, so the action
        // means "show the graph": a second press moves the frame to the tab that
        // already holds it rather than opening another (roadmap P3.4).
        OpenGraph,
        // The graph tab's keyboard (`graph` context, roadmap P3.4 Task 7). `enter` and
        // `f` are plain names; the camera and selection keys below carry their step,
        // because the direction is decided in the keymap and one action per direction
        // would be eight names for two gestures.
        GraphOpen,
        GraphFit,
        // The graph tab's filter strip (`/` in the graph, roadmap P3.4 Task 8). `escape`
        // and `enter` are one action: the filter took effect as it was typed, so there is
        // nothing for `enter` to accept that `escape` would be undoing. Closing the strip
        // is its own action rather than `note.focus-editor` because the first `escape`
        // closes the strip and only a second one leaves the tab.
        GraphFilterOpen,
        GraphFilterClose,
        // The three chips, also reachable from the keyboard as `alt-o`/`alt-x`/`alt-t`.
        // They are `alt`-modified because the strip holds a text field, and every bare
        // letter this context receives is a letter of the query.
        GraphFilterOrphans,
        GraphFilterMissing,
        GraphFilterTags,
    ]
);

/// A zoom step for the graph's camera: the factor to scale by, so `+` is
/// [`shale_core::graph::geom::ZOOM_STEP`] and `-` its reciprocal.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct GraphZoom(pub f32);

/// A pan of the graph's camera, in screen px. `shift-<arrow>` is the same action with
/// a longer step, which is why the step is a field rather than eight actions.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct GraphPan(pub [f32; 2]);

/// Moving the graph's keyboard selection (`h`/`j`/`k`/`l`), as a direction the core
/// cone search points along.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct GraphSelect(pub [f32; 2]);

/// Choosing a row of the popup by its number. It carries the row, so it is a
/// struct rather than a name in the list above; the popup turns it back into
/// whichever of its own actions that row stands for.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct ApplyRow(pub usize);
