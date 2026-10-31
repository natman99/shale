//! The command palette: what it offers, and what choosing a row means.
//!
//! Every row names an action the window already handles, so running a command
//! from the palette is the same dispatch its key performs — the palette cannot
//! drift from the keymap. Each row also carries the keys that reach the same
//! action, which makes the palette a map of the bindings as well as a launcher.

use gpui::Action;

use crate::acts::*;

/// A command the palette can run.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Save,
    Quit,
    NewNote,
    NewFolder,
    RenameNote,
    DeleteNote,
    QuickOpen,
    JumpBack,
    JumpForward,
    SearchVault,
    ChangeTheme,
    CheckerOn,
    CheckerOff,
    ShowNote,
    ViewRaw,
    ViewLive,
    ViewPreview,
    ToggleFocus,
    ToggleTypewriter,
    WindowLeft,
    WindowRight,
    Undo,
    Redo,
    CopyText,
    PasteText,
    YankClipboard,
}

/// Every command, in the order the palette lists them before anything is typed.
const ALL: [Command; 26] = [
    Command::Save,
    Command::Quit,
    Command::NewNote,
    Command::NewFolder,
    Command::RenameNote,
    Command::DeleteNote,
    Command::QuickOpen,
    Command::JumpBack,
    Command::JumpForward,
    Command::SearchVault,
    Command::ChangeTheme,
    Command::CheckerOn,
    Command::CheckerOff,
    Command::ShowNote,
    Command::ViewRaw,
    Command::ViewLive,
    Command::ViewPreview,
    Command::ToggleFocus,
    Command::ToggleTypewriter,
    Command::WindowLeft,
    Command::WindowRight,
    Command::Undo,
    Command::Redo,
    Command::CopyText,
    Command::PasteText,
    Command::YankClipboard,
];

impl Command {
    /// Every command, for building the palette's rows.
    pub(crate) fn all() -> &'static [Command] {
        &ALL
    }

    /// The row's text.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Command::Save => "Save the note",
            Command::Quit => "Quit Shale",
            Command::NewNote => "New note",
            Command::NewFolder => "New folder",
            Command::RenameNote => "Rename the selected note",
            Command::DeleteNote => "Delete the selected note",
            Command::QuickOpen => "Open a note",
            Command::JumpBack => "Jump back to the previous note",
            Command::JumpForward => "Jump forward to the next note",
            Command::SearchVault => "Search the vault",
            Command::ChangeTheme => "Change the theme",
            Command::CheckerOn => "Turn the spellchecker on",
            Command::CheckerOff => "Turn the spellchecker off",
            Command::ShowNote => "Show the checker's note for the word here",
            Command::ViewRaw => "Show the note as markdown source",
            Command::ViewLive => "Show the note with live preview",
            Command::ViewPreview => "Preview the note, without editing it",
            Command::ToggleFocus => "Dim everything but the block the cursor is in",
            Command::ToggleTypewriter => "Keep the cursor's line near the middle of the window",
            Command::WindowLeft => "Focus the file tree",
            Command::WindowRight => "Focus the note",
            Command::Undo => "Undo",
            Command::Redo => "Redo",
            Command::CopyText => "Copy the selection to the clipboard",
            Command::PasteText => "Paste from the clipboard",
            Command::YankClipboard => "Copy the selection, or the last yank, to the clipboard",
        }
    }

    /// The keys that reach this action, shown dimmer after the label. A command
    /// with several ways in lists them all, so the palette stays honest about
    /// what the user can press instead.
    pub(crate) fn keys(self) -> &'static str {
        match self {
            Command::Save => "ctrl-s, :w",
            Command::Quit => "ctrl-q, :q",
            Command::NewNote => "ctrl-n, n in the tree",
            Command::NewFolder => "ctrl-shift-n, shift-n in the tree",
            Command::RenameNote => "ctrl-shift-r, r in the tree",
            Command::DeleteNote => "ctrl-shift-d, shift-d in the tree",
            Command::QuickOpen => "space f, g f, :e",
            Command::JumpBack => "ctrl-o",
            Command::JumpForward => "ctrl-i",
            Command::SearchVault => "ctrl-shift-f, space s",
            Command::ChangeTheme => ":theme",
            Command::CheckerOn => ":harper on",
            Command::CheckerOff => ":harper off",
            Command::ShowNote => "K",
            Command::ViewRaw => "space r, :raw",
            Command::ViewLive => "space l, :live",
            Command::ViewPreview => "space p, :preview",
            Command::ToggleFocus => "space d, :focus on|off, :focus block|section",
            Command::ToggleTypewriter => ":typewriter",
            Command::WindowLeft => "ctrl-w h",
            Command::WindowRight => "ctrl-w l",
            Command::Undo => "u, ctrl-z",
            Command::Redo => "ctrl-r, ctrl-y",
            Command::CopyText => "ctrl-shift-c",
            Command::PasteText => "ctrl-shift-v, :paste",
            Command::YankClipboard => "space y, :yank",
        }
    }

    /// The action to dispatch at whatever has the keyboard. The editor's own
    /// actions (`Undo`, `PasteText`, …) only reach it while it is focused, which
    /// is exactly what the user was working on when they opened the palette.
    pub(crate) fn action(self) -> Box<dyn Action> {
        match self {
            Command::Save => Box::new(Save),
            Command::Quit => Box::new(Quit),
            Command::NewNote => Box::new(NewNote),
            Command::NewFolder => Box::new(NewFolder),
            Command::RenameNote => Box::new(RenameNote),
            Command::DeleteNote => Box::new(DeleteNote),
            Command::QuickOpen => Box::new(QuickOpen),
            Command::JumpBack => Box::new(JumpBack),
            Command::JumpForward => Box::new(JumpForward),
            Command::SearchVault => Box::new(SearchVault),
            Command::ChangeTheme => Box::new(ChangeTheme),
            Command::CheckerOn => Box::new(CheckerOn),
            Command::CheckerOff => Box::new(CheckerOff),
            Command::ShowNote => Box::new(ShowNote),
            Command::ViewRaw => Box::new(ViewRaw),
            Command::ViewLive => Box::new(ViewLive),
            Command::ViewPreview => Box::new(ViewPreview),
            Command::ToggleFocus => Box::new(ToggleFocus),
            Command::ToggleTypewriter => Box::new(ToggleTypewriter),
            Command::WindowLeft => Box::new(WindowLeft),
            Command::WindowRight => Box::new(WindowRight),
            Command::Undo => Box::new(Undo),
            Command::Redo => Box::new(Redo),
            Command::CopyText => Box::new(CopyText),
            Command::PasteText => Box::new(PasteText),
            Command::YankClipboard => Box::new(YankClipboard),
        }
    }
}
