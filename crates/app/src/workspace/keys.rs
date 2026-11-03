//! The keymap: which keystroke means which action, and in which context.

use gpui::KeyBinding;
use shale_core::graph::geom::ZOOM_STEP;

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
        // Quick-open: jump to a note by typing part of its name or path. It lives
        // on the vim chords `space f` and `g f` and on `:e`, which frees `ctrl-o`
        // for the jump chain — the key the Goals promise asks for, and the one vim
        // users already reach for.
        KeyBinding::new("ctrl-o", JumpBack, None),
        KeyBinding::new("ctrl-i", JumpForward, None),
        // Vault search: every line that contains what is typed. The picker's rows
        // are the hits, and choosing one opens the note on that line.
        KeyBinding::new("ctrl-shift-f", SearchVault, None),
        // The command palette. `ctrl-shift-p` is the shape the user asked for;
        // vim's `:` is bound in the note's own context (see below).
        KeyBinding::new("ctrl-shift-p", CommandPalette, None),
        // A new tab holding the note in hand (roadmap P3.1). Obsidian's key for the
        // same thing, and free here: vim's `ctrl-t` is the popup-stack jump, which
        // Shale has no popup stack for. Tab *pages* stay outside the `ctrl-w` prefix,
        // which belongs to the window commands a split view will want.
        KeyBinding::new("ctrl-t", TabNew, None),
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
        // The same for `space r`, the raw view: `r` is vim's replace-the-character,
        // which Shale does not have yet, so on its own this reaches an engine that
        // reports it unhandled.
        "r" => VimR,
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
        // Agent edit review (roadmap P2.5a): the two keys that hop the marker while
        // the *note* has the keyboard. `]a`/`[a` are vim's next/previous-of-a-kind
        // shape, and they are the only review keys in prose — which is what keeps `y`
        // and `x` meaning yank and delete. The pair is owned by the vim engine rather
        // than spelled as a two-stroke chord, because gpui holds the first key of a
        // chord as pending input and a literal `[` must still reach `f`/`t`/operators.
        "[" => VimOpenBracket,
        "]" => VimCloseBracket,
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
    // The review strip, while it has the keyboard (`:review`). These are the bare
    // half of the two-layer scheme the design pass ruled: the strip is a list widget,
    // not prose, so `j`/`k`/`y`/`x` are free here — and they are bound *only* here, so
    // the same letters keep meaning yank and delete in the note. `escape` hands the
    // keyboard back to the note rather than leaving the strip focused with nowhere to
    // go; `z a` is the collapse chord, spelled with a space because gpui reads a
    // whitespace-separated pair as two keystrokes.
    bind!("review", [
        "j" => ReviewNext,
        "k" => ReviewPrev,
        "y" => ReviewAccept,
        "x" => ReviewReject,
        "o" => ReviewOpenNote,
        "z a" => ReviewCollapse,
        "escape" => FocusEditor,
    ]);
    // The graph tab (roadmap P3.4). Its own context for the same reason the popup and
    // the review strip have one: the keys the graph will want (`h`/`j`/`k`/`l`, `f`,
    // `enter`) are all vim motions in the note, so they can only be bound where the
    // note is not what has the keyboard. Task 5 gives it the way out; the pan, zoom
    // and selection keys land with Task 7.
    //
    // `escape` is `note.focus-editor` rather than a graph-specific action because it is
    // the same request `ctrl-w l` makes: put the keyboard on the note. With the graph on
    // screen that means moving the frame back to the note tab the user was last on.
    bind!(crate::graph::CONTEXT, [
        "escape" => FocusEditor,
        // `enter` opens the chosen node's note, and `f` frames the whole graph again.
        "enter" => GraphOpen,
        "f" => GraphFit,
        // `/` opens the filter strip, which then takes the keyboard for itself.
        "/" => GraphFilterOpen,
    ]);
    // The filter strip. It is a sibling of the canvas rather than inside its context, so
    // the graph's `hjkl`, `f`, `+` and `-` are not in the stack while a query is being
    // typed — which is the whole reason the strip has a context of its own. Printable
    // letters never reach a keybinding, so what is bound here is the editing and the
    // three toggles, and the toggles carry `alt` for the same reason.
    bind!(crate::graph::FILTER_CONTEXT, [
        "escape" => GraphFilterClose,
        "enter" => GraphFilterClose,
        "backspace" => FieldBackspace,
        "delete" => FieldDelete,
        "ctrl-backspace" => FieldDeleteWordBack,
        "left" => FieldLeft,
        "right" => FieldRight,
        "alt-o" => GraphFilterOrphans,
        "alt-x" => GraphFilterMissing,
        "alt-t" => GraphFilterTags,
    ]);
    // The camera and selection keys carry their step, so they are pushed one by one as
    // the popup's row numbers are. `+` and `-` are literal keys — shift+`=` reaches us
    // as `+`, the same way `$` does in the note.
    let graph = crate::graph::CONTEXT;
    for (key, factor) in [("+", ZOOM_STEP), ("-", 1.0 / ZOOM_STEP)] {
        keys.push(KeyBinding::new(key, GraphZoom(factor), Some(graph)));
    }
    // The arrows move the view, and their shift forms move it three times as far, which
    // is the spec's "`Shift` = 3× speed".
    for (key, dx, dy) in [
        ("left", -1.0, 0.0),
        ("right", 1.0, 0.0),
        ("up", 0.0, -1.0),
        ("down", 0.0, 1.0),
    ] {
        keys.push(KeyBinding::new(
            key,
            GraphPan([dx * crate::graph::PAN_STEP, dy * crate::graph::PAN_STEP]),
            Some(graph),
        ));
        keys.push(KeyBinding::new(
            &format!("shift-{key}"),
            GraphPan([
                dx * crate::graph::PAN_STEP * crate::graph::PAN_FAST,
                dy * crate::graph::PAN_STEP * crate::graph::PAN_FAST,
            ]),
            Some(graph),
        ));
    }
    // `hjkl` picks the node instead of moving the view: the arrows are the camera, these
    // are the thing `enter` opens. They are free here for the reason the whole context
    // exists — in the note they are motions.
    for (key, dx, dy) in [
        ("h", -1.0, 0.0),
        ("l", 1.0, 0.0),
        ("k", 0.0, -1.0),
        ("j", 0.0, 1.0),
    ] {
        keys.push(KeyBinding::new(key, GraphSelect([dx, dy]), Some(graph)));
    }
    keys
}
