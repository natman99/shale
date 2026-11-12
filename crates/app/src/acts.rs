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
        // The one-keystroke edits the engine grew: `~` switches case, `S`/`C`/`D`
        // (`D` above) are the line-edge changes, `X` deletes backwards, `Y` yanks a
        // line. Each is vim's shorthand for an operator and a motion, and the engine
        // spells it that way.
        VimTilde,
        VimUpperS,
        VimUpperC,
        VimUpperX,
        VimUpperY,
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
        // WORD motions (`W`, `B`): a WORD is delimited only by blanks, so it walks
        // over Markdown markup in one step. `E` shares the action above with the `zE`
        // fold command — the engine decides which one a given `E` is.
        VimUpperW,
        VimUpperB,
        // `]` and `[` are bound so the review-marker chords `]a`/`[a` reach the vim
        // engine, which owns the pair. They are bound as single keys rather than as a
        // two-stroke chord because gpui holds the first key of a chord as pending
        // input, and a literal `[` has to reach the engine as the argument of `f`, `t`
        // or an operator.
        VimOpenBracket,
        VimCloseBracket,
        // The rest of the bracket pairs, and the quotes: these are the *object*
        // characters (`di(`, `da{`, and the quote objects), bound so the pair the
        // cursor sits in reaches the engine. `[` and `]` above do double duty as the
        // review-marker chords; these have no other use in normal mode.
        VimOpenParen,
        VimCloseParen,
        VimOpenBrace,
        VimCloseBrace,
        VimLess,
        VimGreater,
        VimQuote,
        VimApostrophe,
        // `;` and `,` repeat the last `f`/`t`/`F`/`T`, forwards and backwards.
        VimSemicolon,
        VimComma,
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
        // `tab` in insert mode writes a tab character — Shale indents notes with
        // tabs, so the key types the file's own indentation — and `shift-tab`
        // takes one indent level back off the line.
        InsertTab,
        OutdentLine,
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
        // The right-hand strip: show it, hide it, or put the keyboard in it
        // (`space b`, `:sidebar`, `ctrl-w l`). One action for the toggle, because the
        // way out is the same two keys as the way in (`space e`'s rule).
        SidebarToggle,
        FocusSidebar,
        SidebarUp,
        SidebarDown,
        SidebarOpen,
        SidebarCollapse,
        SidebarExpand,
        // `[` and `]` inside the strip: the other view (sidebar-plan.md ruling 16).
        SidebarPrevView,
        SidebarNextView,
        QuickOpen,
        // Walking the chain of notes the user came from (`ctrl-o` back, `ctrl-i`
        // forward), the way vim and neovim do.
        JumpBack,
        JumpForward,
        // Stepping into the embed under the caret (`ctrl-]`, vim's tag jump), or
        // clicking one (roadmap P4.0b). `ctrl-o` walks back out again.
        EnterEmbed,
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
        // Pick another vault: the list this app remembers, or a folder from the
        // file dialog. The chosen vault opens in this process beside this
        // window (`:vault`, or the palette).
        ChangeVault,
        // Another window on the vault this one is showing (`:new-window`).
        // The vault's services — index, watcher, MCP — are shared, which is
        // what makes a second window safe where a second process is refused.
        NewWindow,
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
        // Resizing the pane in hand within the split it sits in (`ctrl-w >`/`<`).
        WindowGrow,
        WindowShrink,
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
        // Selection in a field: shift extends the caret, `ctrl-a` takes the whole
        // line, `home`/`end` are its ends. Bound in every context that holds a
        // field — the prompt, the pickers, the graph's filter.
        FieldSelectLeft,
        FieldSelectRight,
        FieldWordLeft,
        FieldWordRight,
        FieldSelectWordLeft,
        FieldSelectWordRight,
        FieldSelectToStart,
        FieldSelectToEnd,
        FieldSelectAll,
        FieldHome,
        FieldEnd,
        // The clipboard in a field. The note editor yanks and pastes with vim's
        // keys; a field takes the keys everybody else reaches for, and both write
        // the same system clipboard.
        FieldCopy,
        FieldCut,
        FieldPaste,
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
        // What the index knows about the note the caret is in: its frontmatter
        // properties, read from the `props` table rather than re-parsed from the
        // buffer (`:props`, P4.3a).
        Props,
        // Every property key the vault writes, with how far each reaches and every
        // kind its values come as (`:keys`, P4.3a M5). Choosing a key puts `prop:key`
        // in the search field rather than opening a second list: the search is the
        // surface that answers "which notes write this", and it can be widened there.
        PropertyKeys,
        // Rename a property key across the vault (`:rename-key`, P4.3a M5). The
        // palette row opens the same key list `:keys` shows, in the mood where
        // choosing a key means renaming it; the command line can skip both steps
        // with `:rename-key cr challenge`.
        RenamePropertyKey,
        // Jump to a section (`:sections`, P4.3b M3). The palette row can only mean the
        // note on screen, so it outlines that note; the vault-wide list needs the
        // pattern only the command line can carry.
        Sections,
        // Jump to a block (`:blocks`, P4.9 M4). Like `:sections`: the palette row can
        // only mean the note on screen, and the vault-wide list needs the pattern only
        // the command line can carry.
        Blocks,
        // Give the block under the caret an id (`:block-id`, P4.9 M4). No chord: this is
        // a command reached for by name, when a link needs somewhere to land.
        AddBlockId,
        // Run a saved query: choose a `.shq`, run it against the index, read the rows
        // (`:query`, P4.3 M2).
        RunQuery,
        // Keep the search that is on screen as a `.shq` (`:save-query`, P4.3 M6). The
        // palette row can only say "I need a search": the search arrives from the row at
        // the end of a search picker, or as the argument of the command.
        SaveQuery,
        // Write a properties block at the top of the note, or step into the one it
        // already has. Palette-only: `:props` reads them and `:properties` is its
        // spelling, so a second `:` word for the same noun would be a coin flip.
        // One undo step, because the block arrives through the buffer's grouped-edit
        // path rather than as a rewrite of the file.
        AddProperties,
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
        // `escape` in the graph tab: put down the chosen node and stay in the graph.
        // It is deliberately *not* `note.focus-editor`, which is what it used to be —
        // that handler used to move the pane back to the last note tab, so escape read
        // as 'the graph closed'. The graph has no door of its own any more: leaving it
        // is tab paging, the same as for every other tab.
        GraphClearSelection,
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

/// The local graph's depth (M2b): how many hops from the note on screen the picture
/// reaches. One action carrying the step rather than two names, because `[` and `]` are
/// the same gesture in opposite directions, and stepping from *off* turns the view on at
/// the default depth.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct GraphLocalStep(pub isize);

/// The local graph's scope as a request: `Some(n)` sets the depth, `None` toggles it.
/// The palette row and the strip's chip are the same ask with a different amount of
/// conviction.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct GraphLocal(pub Option<usize>);

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

/// Which space the graph tab shows: the flat graph or the projected one (roadmap
/// P3.6). One action with the space as its payload rather than two names, because the
/// tab's `2`/`3` keys and the `:graph2d` / `:graph3d` commands are the same request
/// with a different value.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = shale, no_json)]
pub(crate) struct GraphMode(pub crate::graph::Dim);

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
