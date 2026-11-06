//! The keymap: which keystroke means which action, and in which context.
//!
//! ## The keymap is also the docs
//!
//! Every binding goes through [`Builder::bind`], which pushes the gpui binding *and*
//! records a row describing it. `docs/keymap.md` is generated from those rows, so the
//! exhaustive table cannot fall behind the keymap the way a hand-written list did.
//! `SHALE_UPDATE_DOCS=1 cargo test -p shale-app` rewrites it; the plain test fails when
//! the file and the keymap disagree.
//!
//! What that table covers is what **gpui** knows. The note's own vocabulary — counts,
//! the `space` and `g` leader chords, marks, `z` and `[`/`]` commands, text objects — is
//! not in the keymap: those keys reach the vim engine, which decides what they mean for
//! the mode in hand (see the `space s` / `space r` notes below). `docs/keys.md` is the
//! human map of both halves.

use std::collections::HashMap;

use gpui::{Action, KeyBinding};
use shale_core::graph::geom::ZOOM_STEP;

use crate::acts::*;
use crate::command::Command;
use crate::popup;

/// One binding, as the generated docs see it.
///
/// Recorded by [`Builder::bind`] alongside the binding itself, which is the whole
/// point: there is no second list of keys for the docs to fall out of date against.
#[derive(Clone, Debug)]
pub(crate) struct KeymapRow {
    /// The gpui context that owns the binding; empty for a global binding. The
    /// context decides *which surface* has the keyboard, so the docs group by it
    /// rather than by the key.
    pub context: &'static str,
    /// The keystrokes, exactly as bound — `ctrl-w h`, `g t`, `+`.
    pub keys: &'static str,
    /// The action's gpui name, module-qualified (`shale_editor::VimH`). The docs
    /// print the bare name.
    pub action: String,
    /// The command id this action runs, when it is one of the registry's commands.
    /// This is the join that makes the table useful: a key that is a command says
    /// which, and the command's label comes from the registry.
    pub command: &'static str,
    /// Human wording, for the bindings whose action name does not read (`GraphPan`,
    /// `ApplyRow`). Empty means: use the command's label, or the action name.
    ///
    /// Owned rather than `&'static` because a few rows build their wording from the
    /// action's own data — popup row 3 says \"take popup row 3\".
    pub note: String,
}

pub(crate) fn keybindings() -> Vec<KeyBinding> {
    build().0
}

/// Every binding, in the order the keymap builds them.
#[allow(dead_code)] // the generated docs and their test are the consumer
pub(crate) fn keymap_rows() -> Vec<KeymapRow> {
    build().1
}

/// Pushes a gpui binding and records its doc row in one step.
struct Builder {
    bindings: Vec<KeyBinding>,
    rows: Vec<KeymapRow>,
    /// action name -> command id, read from the command registry. A binding whose
    /// action is a command's action gets that id in the docs, and with it the label,
    /// so the keymap and `docs/commands.md` describe the same command.
    ids: HashMap<String, &'static str>,
}

impl Builder {
    fn new() -> Self {
        let ids = Command::every()
            .iter()
            .filter_map(|command| Some((command.action_name()?.to_string(), command.id())))
            .collect();
        Builder {
            bindings: Vec::new(),
            rows: Vec::new(),
            ids,
        }
    }

    /// Bind one keystroke sequence, and say what it is for.
    ///
    /// `note` is for the docs only, and only where the action name is not enough to
    /// read the row; leave it empty for a command, whose label the registry supplies.
    fn bind<A: Action>(
        &mut self,
        context: Option<&'static str>,
        keys: &'static str,
        action: A,
        note: impl Into<String>,
    ) {
        let name = action.name().to_string();
        let command = self.ids.get(&name).copied().unwrap_or("");
        self.bindings.push(KeyBinding::new(keys, action, context));
        self.rows.push(KeymapRow {
            context: context.unwrap_or(""),
            keys,
            action: name,
            command,
            note: note.into(),
        });
    }
}

macro_rules! bind {
    ($b:expr, $ctx:expr, [ $( $key:literal => $action:expr ),+ $(,)? ]) => {
        $( $b.bind($ctx, $key, $action, ""); )+
    };
}

/// The same, with wording for the docs. Needed where one action answers several keys
/// (`WindowClose` for both `q` and `c`) or where the action name is not a sentence.
macro_rules! bind_named {
    ($b:expr, $ctx:expr, [ $( $key:literal => $action:expr => $note:expr ),+ $(,)? ]) => {
        $( $b.bind($ctx, $key, $action, $note); )+
    };
}

fn build() -> (Vec<KeyBinding>, Vec<KeymapRow>) {
    // gpui's keystroke syntax separates modifiers with `-`, not `+`: `ctrl-z`.
    // A `+` is not a separator, so "ctrl+z" parses as one literal key named
    // "ctrl+z" that can never match anything the platform delivers.
    let mut b = Builder::new();
    let global: Option<&'static str> = None;
    bind!(b, global, [
        "ctrl-s" => Save,
        "ctrl-q" => Quit,
        // The vault edits are available from anywhere, because they act on the
        // tree's selection or, failing that, the note that is open.
        "ctrl-n" => NewNote,
        "ctrl-shift-n" => NewFolder,
        "ctrl-shift-r" => RenameNote,
        "ctrl-shift-d" => DeleteNote,
        // Quick-open: jump to a note by typing part of its name or path. It lives
        // on the vim chords `space f` and `g f` and on `:e`, which frees `ctrl-o`
        // for the jump chain — the key the Goals promise asks for, and the one vim
        // users already reach for.
        "ctrl-o" => JumpBack,
        "ctrl-i" => JumpForward,
        // Vault search: every line that contains what is typed. The picker's rows
        // are the hits, and choosing one opens the note on that line.
        "ctrl-shift-f" => SearchVault,
        // The command palette is bound here; its wording waits below, because the
        // palette is the command list itself and so is not a row of it.
        // A new tab holding the note in hand (roadmap P3.1). Obsidian's key for the
        // same thing, and free here: vim's `ctrl-t` is the popup-stack jump, which
        // Shale has no popup stack for. Tab *pages* stay outside the `ctrl-w` prefix,
        // which belongs to the window commands a split view will want.
        "ctrl-t" => TabNew,
    ]);
    // The palette needs its own wording rather than a label from the command table:
    // `ctrl-shift-p` opens the table, it is not one of its rows. vim's `:` is bound
    // in the note's own context (see below).
    bind_named!(b, global, [
        "ctrl-shift-p" => CommandPalette => "open the command palette",
    ]);
    // Moving the keyboard between panels, vim-window style. gpui matches a
    // two-stroke binding natively: `ctrl-w` alone is kept as pending input
    // and replayed if the next key does not finish the chord.
    //
    // The split alphabet (roadmap P3.2), vim's shapes: `s`/`v` split the pane in
    // hand, `S`/`V` split it *with the current buffer*, `h/j/k/l` step between
    // panes, `q` closes one, `o` keeps only it, `=` evens them out. The rest of
    // vim's alphabet (`x`/`r`/`>`,`<`) waits for P3.2c; the prefix itself is already
    // reserved (see above).
    //
    // Spelled with wording because the action names do not tell the keys apart:
    // `WindowLeft` hands over the *keyboard* rather than moving a pane, and `q` and
    // `c` are the same close.
    bind_named!(b, global, [
        "ctrl-w h" => WindowLeft => "hand the keyboard to the pane on the left",
        "ctrl-w l" => WindowRight => "hand the keyboard to the pane on the right",
        "ctrl-w j" => WindowDown => "hand the keyboard to the pane below",
        "ctrl-w k" => WindowUp => "hand the keyboard to the pane above",
        "ctrl-w s" => WindowSplitDown => "split this pane, keeping the top half",
        "ctrl-w v" => WindowSplitRight => "split this pane, keeping the left half",
        "ctrl-w S" => WindowSplitDownHere => "split down, this note in both halves",
        "ctrl-w V" => WindowSplitRightHere => "split right, this note in both halves",
        "ctrl-w q" => WindowClose => "close this pane",
        "ctrl-w c" => WindowClose => "close this pane",
        "ctrl-w o" => WindowOnly => "keep only this pane in the window",
        "ctrl-w =" => WindowEqualize => "even the panes out",
        // The pane in hand takes more or less of the space its split offers. `+`/`-`
        // are the same moves for a hand that is already on the letter row, and — like
        // vim's — they are not tied to an axis: the pane grows along whatever axis the
        // split it sits in divides.
        "ctrl-w >" => WindowGrow => "grow this pane along its split",
        "ctrl-w <" => WindowShrink => "shrink this pane along its split",
        "ctrl-w +" => WindowGrow => "grow this pane along its split",
        "ctrl-w -" => WindowShrink => "shrink this pane along its split",
    ]);
    // Motions and operators are bound in both normal and visual mode; the vim
    // engine decides what each key means for the current mode.
    for context in [Some("vim-normal"), Some("vim-visual")] {
        bind!(b, context, [
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
    bind!(b, Some("vim-normal"), [
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
    bind!(b, Some("vim-insert"), [
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
    bind!(b, Some("tree"), [
        "j" => TreeDown,
        "k" => TreeUp,
        "down" => TreeDown,
        "up" => TreeUp,
        "h" => TreeCollapse,
        "l" => TreeExpand,
        "left" => TreeCollapse,
        "right" => TreeExpand,
        "enter" => TreeOpen,
        // `space e` puts the tree away from anywhere, including from inside it, which
        // is how nvim-tree's and NERDTree's `<leader>e` work. gpui holds a keystroke
        // pending whenever a longer binding shares its prefix, so a `space` bound to
        // something else here would sit for the pending timeout before it fired: a
        // lone `space` would feel like a key that had not worked. `enter` opens a row,
        // and the leader is left whole for the toggle.
        "space e" => TreeToggle,
        "escape" => FocusEditor,
        "n" => NewNote,
        "shift-n" => NewFolder,
        "r" => RenameNote,
        "shift-d" => DeleteNote,
    ]);
    // The one-line prompt. Printable characters never reach a keybinding — they
    // arrive through the input handler — so only the keys that are not text are
    // bound here.
    bind!(b, Some("prompt"), [
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
    bind!(b, Some("picker"), [
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
    bind!(b, Some(popup::CONTEXT), [
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
        b.bind(
            Some(popup::CONTEXT),
            key,
            ApplyRow(row),
            format!("take popup row {}", row + 1),
        );
    }
    // The review strip, while it has the keyboard (`:review`). These are the bare
    // half of the two-layer scheme the design pass ruled: the strip is a list widget,
    // not prose, so `j`/`k`/`y`/`x` are free here — and they are bound *only* here, so
    // the same letters keep meaning yank and delete in the note. `escape` hands the
    // keyboard back to the note rather than leaving the strip focused with nowhere to
    // go; `z a` is the collapse chord, spelled with a space because gpui reads a
    // whitespace-separated pair as two keystrokes.
    bind!(b, Some("review"), [
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
    bind!(b, Some(crate::graph::CONTEXT), [
        // vim's command line, from the graph too: the ex bar is the workspace's,
        // not the note's, so `:tabnext`, `:only` and friends are the way out of
        // a graph tab that `escape` alone does not give.
        ":" => RunCommandLine,
        // The leader chords the graph shares with the note. They dispatch the
        // same actions the note's engine dispatches — the window owns all of
        // them — so the hands do not change when the view does. The engine is
        // not involved here: gpui spells the two-stroke directly, which is
        // safe because nothing in this context binds a lone `space` or `g`.
        "space f" => QuickOpen,
        "space s" => SearchVault,
        "space e" => TreeToggle,
        "space d" => ToggleFocus,
        "space g" => OpenGraph,
        // Tab pages, the same `g t`/`g T` the note's engine reaches. Tab pages
        // live outside the `ctrl-w` prefix, and the graph is one of them.
        "g t" => TabNext,
        "g T" => TabPrev,
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
    bind!(b, Some(crate::graph::FILTER_CONTEXT), [
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
    let graph = Some(crate::graph::CONTEXT);
    // `2` and `3` turn the graph between its two spaces (roadmap P3.6). They are pushed
    // one by one like the zoom steps, because the action carries the space. Bare digits
    // because nothing else in this context wants them: the graph has no counts, no
    // registers and no windows of its own, and the filter strip — the only place a digit
    // could be typed as text — has a context of its own.
    for (key, dim) in [("2", crate::graph::Dim::Two), ("3", crate::graph::Dim::Three)] {
        b.bind(
            graph,
            key,
            GraphMode(dim),
            "switch the graph between its two spaces",
        );
    }
    for (key, factor) in [("+", ZOOM_STEP), ("-", 1.0 / ZOOM_STEP)] {
        b.bind(graph, key, GraphZoom(factor), "magnify at the cursor");
    }
    // `[` and `]` narrow and widen the local graph (M2b). Brackets because they are the
    // shape of a scope, they are free in this context, and they are not `+`/`-`, which
    // already mean the camera: a scope gesture and a zoom gesture that share keys would
    // make the user ask which one the graph heard.
    for (key, delta) in [("[", -1isize), ("]", 1isize)] {
        b.bind(
            graph,
            key,
            GraphLocalStep(delta),
            "narrow or widen the local graph",
        );
    }
    // `\` widens all the way out: the local view off, the whole vault back on screen.
    b.bind(graph, "\\", GraphLocal(Some(0)), "show the whole vault again");
    // The arrows move the view, and their shift forms move it three times as far, which
    // is the spec's "`Shift` = 3× speed".
    // The shift forms are spelled out rather than built with `format!`, because a
    // binding's key text is `&'static str` and a built string would have to be leaked.
    for (key, shift_key, dx, dy) in [
        ("left", "shift-left", -1.0, 0.0),
        ("right", "shift-right", 1.0, 0.0),
        ("up", "shift-up", 0.0, -1.0),
        ("down", "shift-down", 0.0, 1.0),
    ] {
        b.bind(
            graph,
            key,
            GraphPan([dx * crate::graph::PAN_STEP, dy * crate::graph::PAN_STEP]),
            "move the view",
        );
        b.bind(
            graph,
            shift_key,
            GraphPan([
                dx * crate::graph::PAN_STEP * crate::graph::PAN_FAST,
                dy * crate::graph::PAN_STEP * crate::graph::PAN_FAST,
            ]),
            "move the view, three times as far",
        );
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
        b.bind(graph, key, GraphSelect([dx, dy]), "move the chosen node");
    }
    (b.bindings, b.rows)
}

/// Wording for the surface keys that are not registry commands — the text field,
/// the picker, the tree, the plain editing keys insert mode uses. They are the same
/// action in several contexts (`FieldSubmit` answers `enter` in the prompt, the
/// picker and the popup), so the wording belongs to the action once rather than to
/// each binding.
fn action_note(name: &str) -> Option<&'static str> {
    Some(match name {
        "SearchBuffer" => "search this note",
        "SearchNext" => "next match in this note",
        "SearchPrev" => "previous match in this note",
        "NextLine" => "go to the next line",
        "Newline" => "break the line here",
        "Backspace" => "delete the character before the cursor",
        "DeleteWordBack" => "delete the word before the cursor",
        "DeleteForward" => "delete the character under the cursor",
        "MoveLeft" => "move the cursor left",
        "MoveRight" => "move the cursor right",
        "MoveUp" => "move the cursor up",
        "MoveDown" => "move the cursor down",
        "TreeDown" => "move the selection down",
        "TreeUp" => "move the selection up",
        "TreeCollapse" => "collapse this folder, or step out of it",
        "TreeExpand" => "open this folder, or step into it",
        "TreeOpen" => "open this note",
        "FocusEditor" => "hand the keyboard to the note",
        "FieldSubmit" => "accept what is typed",
        "FieldCancel" => "dismiss without accepting",
        "FieldBackspace" => "delete the character before the caret",
        "FieldDelete" => "delete the character at the caret",
        "FieldDeleteWordBack" => "delete the word before the caret",
        "FieldLeft" => "move the caret left",
        "FieldRight" => "move the caret right",
        "PickerDown" => "move the highlighted row down",
        "PickerUp" => "move the highlighted row up",
        "RunCommandLine" => "type a `:` command",
        "GraphOpen" => "open the chosen node's note",
        "GraphFit" => "frame the whole graph",
        "GraphFilterOpen" => "open the filter strip",
        "GraphFilterClose" => "close the filter strip",
        "GraphFilterOrphans" => "show or hide unlinked notes",
        "GraphFilterMissing" => "show or hide links to notes that do not exist",
        "GraphFilterTags" => "show or hide tags",
        _ => return None,
    })
}

/// The action name as the docs print it: gpui qualifies it with the module
/// (`shale_editor::VimH`), and the module is noise next to the key.
fn short_name(action: &str) -> &str {
    action.rsplit("::").next().unwrap_or(action)
}

/// `docs/keymap.md`, generated from the rows the keymap built.
#[allow(dead_code)] // the generated docs and their test are the consumer
pub(crate) fn keymap_docs() -> String {
    let rows = keymap_rows();

    // The contexts in the order the keymap builds them, so the doc follows the file.
    // Grouping by context rather than by key is the point: the same letter means
    // different things in different surfaces, and which surface has the keyboard is
    // what decides.
    let mut contexts: Vec<&'static str> = Vec::new();
    for row in &rows {
        if !contexts.contains(&row.context) {
            contexts.push(row.context);
        }
    }

    let label = |row: &KeymapRow| -> String {
        if !row.note.is_empty() {
            return row.note.clone();
        }
        if let Some(command) = Command::by_id(row.command) {
            return command.label().to_string();
        }
        if let Some(note) = action_note(short_name(&row.action)) {
            return note.to_string();
        }
        // Neither a command nor annotated. A `Vim…` action is the note's engine
        // taking the key; anything else is a binding the file should annotate, and
        // `every_binding_is_a_command_or_says_what_it_is` fails on those.
        if short_name(&row.action).starts_with("Vim") {
            "the vim engine takes this key".to_string()
        } else {
            format!("`{}`", short_name(&row.action))
        }
    };

    let heading = |context: &str| -> String {
        if context.is_empty() {
            "### everywhere (no context)".to_string()
        } else {
            format!("### while `{context}` has the keyboard")
        }
    };

    let mut out = String::from(
        "# Shale's keymap\n\
         \n\
         Generated from the bindings built in `crates/app/src/workspace/keys.rs` — do not\n\
         edit by hand. A test fails when this file and the keymap disagree;\n\
         `SHALE_UPDATE_DOCS=1 cargo test -p shale-app` rewrites it.\n\
         \n\
         The sections are gpui's **contexts**, which is what decides which surface has the\n\
         keyboard: the same letter is a motion in a note, a row move in the tree, and a\n\
         node move in the graph. **Keys** are what to press, in gpui's spelling (`ctrl-w h`,\n\
         `shift-g` for `G`). **Runs** is the command id the action reaches, when it is one of\n\
         the registry's commands — `docs/commands.md` is their table.\n\
         \n\
         This table is what **gpui** knows. The note's own vocabulary — counts, the `space`\n\
         and `g` leader chords, marks, `z` and `[`/`]` commands, text objects — is not here,\n\
         because those keys reach the vim engine, which decides what they mean for the mode\n\
         in hand. `docs/keys.md` is the human map of both halves.\n\
         \n",
    );
    for context in contexts {
        out.push_str(&format!("{}\n\n", heading(context)));
        out.push_str("| keys | what it does | action | runs |\n| --- | --- | --- | --- |\n");
        for row in rows.iter().filter(|row| row.context == context) {
            out.push_str(&format!(
                "| `{}` | {} | `{}` | {} |\n",
                row.keys,
                label(row),
                short_name(&row.action),
                if row.command.is_empty() {
                    "—".to_string()
                } else {
                    format!("`{}`", row.command)
                },
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod docs_tests {
    use super::*;

    /// The generated keymap doc is the keymap. Same contract as `docs/commands.md`:
    /// a failing test means the file has fallen behind the code, and
    /// `SHALE_UPDATE_DOCS=1` rewrites it instead of failing.
    #[test]
    fn the_keymap_docs_are_the_keymap() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/keymap.md");
        let want = keymap_docs();
        if std::env::var_os("SHALE_UPDATE_DOCS").is_some() {
            std::fs::write(path, &want).expect("writing docs/keymap.md");
            return;
        }
        let have = std::fs::read_to_string(path).unwrap_or_default();
        assert_eq!(
            have, want,
            "docs/keymap.md is stale: run `SHALE_UPDATE_DOCS=1 cargo test -p shale-app`"
        );
    }

    /// Every binding either reaches a registry command, carries wording, or is a key
    /// the vim engine takes for itself. The third group is honest about being the
    /// engine's: `Vim…` actions are how the note's key vocabulary reaches
    /// `shale_core::vim`, and what they mean depends on the mode and the count in
    /// hand, which the keymap cannot say. Anything else that reaches neither a
    /// command nor wording is a key the docs would have to guess at.
    #[test]
    fn every_binding_is_a_command_or_says_what_it_is() {
        let missing: Vec<String> = keymap_rows()
            .iter()
            .filter(|row| {
                row.command.is_empty()
                    && row.note.is_empty()
                    && action_note(short_name(&row.action)).is_none()
                    && !short_name(&row.action).starts_with("Vim")
            })
            .map(|row| format!("`{}` in `{}` -> `{}`", row.keys, row.context, row.action))
            .collect();
        assert!(
            missing.is_empty(),
            "bindings with no command id and no wording for the docs:\n  {}",
            missing.join("\n  ")
        );
    }

    /// The same key must not be bound to two different actions in one context: gpui
    /// takes the first, so the second would be a key that silently does nothing.
    #[test]
    fn no_key_is_bound_to_two_actions() {
        let mut seen: HashMap<(&str, &str), String> = HashMap::new();
        for row in keymap_rows() {
            if let Some(other) = seen.insert((row.context, row.keys), row.action.clone()) {
                assert_eq!(
                    other, row.action,
                    "`{}` in `{}` is bound to both `{other}` and `{}`",
                    row.keys, row.context, row.action
                );
            }
        }
    }
}
