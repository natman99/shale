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
    SearchVault,
    ShowNote,
    WindowLeft,
    WindowRight,
    Undo,
    Redo,
    CopyText,
    PasteText,
}

/// Every command, in the order the palette lists them before anything is typed.
const ALL: [Command; 15] = [
    Command::Save,
    Command::Quit,
    Command::NewNote,
    Command::NewFolder,
    Command::RenameNote,
    Command::DeleteNote,
    Command::QuickOpen,
    Command::SearchVault,
    Command::ShowNote,
    Command::WindowLeft,
    Command::WindowRight,
    Command::Undo,
    Command::Redo,
    Command::CopyText,
    Command::PasteText,
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
            Command::SearchVault => "Search the vault",
            Command::ShowNote => "Show the checker's note for the word here",
            Command::WindowLeft => "Focus the file tree",
            Command::WindowRight => "Focus the note",
            Command::Undo => "Undo",
            Command::Redo => "Redo",
            Command::CopyText => "Copy to the clipboard",
            Command::PasteText => "Paste from the clipboard",
        }
    }

    /// The keys that reach this action, shown dimmer after the label. A command
    /// with several ways in lists them all, so the palette stays honest about
    /// what the user can press instead.
    pub(crate) fn keys(self) -> &'static str {
        match self {
            Command::Save => "ctrl-s",
            Command::Quit => "ctrl-q",
            Command::NewNote => "ctrl-n, n in the tree",
            Command::NewFolder => "ctrl-shift-n, shift-n in the tree",
            Command::RenameNote => "ctrl-shift-r, r in the tree",
            Command::DeleteNote => "ctrl-shift-d, shift-d in the tree",
            Command::QuickOpen => "ctrl-o, space f",
            Command::SearchVault => "ctrl-shift-f",
            Command::ShowNote => "K",
            Command::WindowLeft => "ctrl-w h",
            Command::WindowRight => "ctrl-w l",
            Command::Undo => "u, ctrl-z",
            Command::Redo => "ctrl-r, ctrl-y",
            Command::CopyText => "ctrl-shift-c",
            Command::PasteText => "ctrl-shift-v",
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
            Command::SearchVault => Box::new(SearchVault),
            Command::ShowNote => Box::new(ShowNote),
            Command::WindowLeft => Box::new(WindowLeft),
            Command::WindowRight => Box::new(WindowRight),
            Command::Undo => Box::new(Undo),
            Command::Redo => Box::new(Redo),
            Command::CopyText => Box::new(CopyText),
            Command::PasteText => Box::new(PasteText),
        }
    }
}
