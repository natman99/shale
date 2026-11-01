//! The command registry: one table of every command the app can run.
//!
//! Before this, the command palette (`workspace::commands`) and the `:` command
//! line (`ex`) each carried their own idea of the command set, and nothing
//! checked they agreed. This module is the one source of truth: every command
//! has a stable **id**, a human **label**, the **keys** that reach it, the **ex**
//! words that spell it, how its **argument** is spelled, and the gpui **action**
//! that runs it. The palette and the command line are two front-ends over the
//! same rows, so a command cannot be renamed in one and missed in the other, and
//! the generated docs (roadmap P2.2) and the MCP surface (P2.3) read the ids from
//! here rather than each keeping a third copy.
//!
//! ## Why the action is the handler
//!
//! The keymap already names an action per command, and the palette dispatches
//! that action — which is what keeps the palette honest to the keys. The command
//! line now dispatches the *same* action for every command that takes no
//! argument, so `:w`, `ctrl-s`, and the palette row all reach one handler through
//! one id. The commands that carry an argument (`:theme gruvbox`,
//! `:wordcount notes/`, `:focus section`, `:wq`) keep their argument handling in
//! `ex`, because a gpui action for a no-argument command cannot carry one — but
//! they still resolve to a registry id, which is what the parity test checks.

use gpui::Action;

use crate::acts::*;

/// One command, as every front-end sees it.
///
/// `Copy` so a palette row can hold one by value: every field is a static
/// reference, a bool, or a function pointer.
#[derive(Clone, Copy)]
pub(crate) struct Command {
    /// The stable id. Dotted `area.verb` so it reads in the docs and the MCP
    /// surface, and so renaming the label never changes what an agent calls.
    id: &'static str,
    /// The palette row's text, and the docs' name for it.
    label: &'static str,
    /// The keys that reach it, shown after the label in the palette. A command with
    /// several ways in lists them all, and says so when one only works in the file
    /// tree. Empty when the command line is the only way in — the `:` spelling comes
    /// from [`Command::ex`], and [`Command::hint`] puts the two together, because a
    /// command written out in both places is a command that can disagree with itself.
    keys: &'static str,
    /// The full command-line invocations that spell this command, first
    /// canonical. Empty for a command `:` cannot reach. The parity test feeds
    /// each through [`crate::ex::parse`] and checks it resolves back to this id.
    ex: &'static [&'static str],
    /// How the command's argument is spelled, for the docs and the palette hint.
    /// Empty when the command takes none.
    args: &'static str,
    /// Whether the palette lists it. Every command is in the docs; a command line
    /// composite like `:wq` is real but has no palette row.
    in_palette: bool,
    /// The action that runs the no-argument form. `None` only for a command-line
    /// composite that is not one action (`:wq`); the palette never lists those.
    action: Option<fn() -> Box<dyn Action>>,
}

impl Command {
    /// The palette's rows, in the order it lists them before anything is typed.
    pub(crate) fn all() -> impl Iterator<Item = Command> {
        ALL.iter().copied().filter(|command| command.in_palette)
    }

    /// Every command, palette-listed or not — what the docs and the MCP surface
    /// enumerate.
    ///
    /// `allow(dead_code)`: the running app only ever needs the palette's rows. The
    /// generated docs (roadmap P2.2) and the MCP resources (P2.3) are the consumers
    /// of the whole table, and this is where they read it from.
    #[allow(dead_code)]
    pub(crate) fn every() -> &'static [Command] {
        ALL
    }

    pub(crate) fn id(self) -> &'static str {
        self.id
    }

    pub(crate) fn label(self) -> &'static str {
        self.label
    }

    /// The keys, exactly as the table spells them — empty when the command line is
    /// the only way in. The palette searches these along with the label; for what it
    /// shows after the label, use [`Command::hint`].
    pub(crate) fn keys(self) -> &'static str {
        self.keys
    }

    /// The command-line words that spell this command, each one a whole
    /// invocation. The docs table lists them all. `allow(dead_code)`: the palette
    /// shows only the first (through [`Command::hint`]) and deliberately does not
    /// search them, so only the docs and the MCP surface read the whole list.
    #[allow(dead_code)]
    pub(crate) fn ex(self) -> &'static [&'static str] {
        self.ex
    }

    /// How the command's argument is spelled. `allow(dead_code)`: the command line
    /// is what parses arguments, and it does so in `ex`; this is for the docs and
    /// the MCP surface, which have to tell an agent what to type.
    #[allow(dead_code)]
    pub(crate) fn args(self) -> &'static str {
        self.args
    }

    /// The action to dispatch at whatever has the keyboard. Only ever called for
    /// a command that has one: the palette's rows and the command line's
    /// no-argument commands, never a composite.
    pub(crate) fn action(self) -> Box<dyn Action> {
        (self
            .action
            .expect("a palette or no-argument command always has an action"))()
    }

    /// The command an id names, if any. The command line and (roadmap P2.3) the
    /// MCP surface look commands up this way.
    pub(crate) fn by_id(id: &str) -> Option<Command> {
        ALL.iter().copied().find(|command| command.id == id)
    }

    /// What the palette shows after the label: the keys, then how the command line
    /// spells the same command. Derived from the two fields rather than written out
    /// in both, so a command cannot say one thing in the palette and another in the
    /// docs.
    pub(crate) fn hint(self) -> String {
        let mut parts = Vec::new();
        if !self.keys.is_empty() {
            parts.push(self.keys.to_string());
        }
        if let Some(word) = self.ex.first() {
            let mut line = word.to_string();
            if !self.args.is_empty() {
                line.push(' ');
                line.push_str(self.args);
            }
            parts.push(format!(":{line}"));
        }
        parts.join(", ")
    }

    /// The command-line spellings, each with the argument it takes: what the docs'
    /// table lists, and what an agent has to type.
    #[allow(dead_code)]
    pub(crate) fn ex_lines(self) -> Vec<String> {
        self.ex
            .iter()
            .map(|word| {
                if self.args.is_empty() {
                    format!(":{word}")
                } else {
                    format!(":{word} {}", self.args)
                }
            })
            .collect()
    }

    /// The repo's command reference (`docs/commands.md`), rendered from the table.
    ///
    /// Generated, and checked by `the_docs_are_the_registry`, which rewrites the
    /// file instead of asserting when `SHALE_UPDATE_DOCS=1` is set. The file is the
    /// artifact roadmap P2.3 serves as an MCP resource, so the app's table and what
    /// an agent reads are the same text. `allow(dead_code)`: the running app does
    /// not render docs; `:help` (the same pipeline, later) will.
    #[allow(dead_code)]
    pub(crate) fn docs() -> String {
        let dash = |text: &str| {
            if text.is_empty() {
                "—".to_string()
            } else {
                format!("`{text}`")
            }
        };
        let mut out = String::from(
            "# Shale's commands\n\
             \n\
             Generated from the table in `crates/app/src/command.rs` — do not edit by hand. A\n\
             test fails when this file and the table disagree; `SHALE_UPDATE_DOCS=1 cargo test\n\
             -p shale-app` rewrites it.\n\
             \n\
             Every command has a stable **id**: what the docs, the MCP surface and any later\n\
             macro language name it by, so renaming a label never breaks a caller. **Keys** are\n\
             what to press, and a key that only works in the file tree says so. **Command line**\n\
             is what to type after `:` in normal mode. **Args** is how the argument is spelled,\n\
             as you would type it.\n\
             \n\
             | id | what it does | keys | command line | args |\n\
             | --- | --- | --- | --- | --- |\n",
        );
        for command in ALL {
            let lines = command.ex_lines();
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                command.id,
                command.label,
                dash(command.keys),
                if lines.is_empty() {
                    "—".to_string()
                } else {
                    lines
                        .iter()
                        .map(|line| format!("`{line}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
                dash(command.args),
            ));
        }
        let hidden: Vec<String> = ALL
            .iter()
            .filter(|command| !command.in_palette)
            .map(|command| format!("`{}`", command.id))
            .collect();
        out.push_str(&format!(
            "\n\
             The palette does not list {}: `note.write-quit` is save and then quit, two\n\
             actions rather than one, and `app.quit-force` is the exit that throws unsaved\n\
             edits away, which is not a row to offer next to *Quit Shale* in a list you\n\
             pick from.\n",
            hidden.join(", "),
        ));
        out
    }
}

/// The whole command set. The palette order is the order these appear in, so the
/// rows a user sees before typing are the first `in_palette` entries here.
const ALL: &[Command] = &[
    Command {
        id: "note.save",
        label: "Save the note",
        keys: "ctrl-s",
        ex: &["w", "write"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(Save)),
    },
    Command {
        id: "app.quit",
        label: "Quit Shale",
        keys: "ctrl-q",
        ex: &["q", "quit"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(Quit)),
    },
    // The bang version: the same exit without the write. Not in the palette — a list
    // you choose from should not offer "throw the work away" next to "Quit Shale" —
    // so the command line is the only way in, and the bang the user types is what
    // says they meant it.
    Command {
        id: "app.quit-force",
        label: "Quit Shale, discarding the note's unsaved edits",
        keys: "",
        ex: &["q!", "quit!"],
        args: "",
        in_palette: false,
        action: Some(|| Box::new(QuitForce)),
    },
    // A command-line composite, not one action, and no palette row: it is here so
    // the docs list it and the parity test covers `:wq`.
    Command {
        id: "note.write-quit",
        label: "Write the note, then quit",
        keys: "",
        ex: &["wq", "x", "xit", "exit"],
        args: "",
        in_palette: false,
        action: None,
    },
    Command {
        id: "note.new",
        label: "New note",
        keys: "ctrl-n, n in the tree",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(NewNote)),
    },
    Command {
        id: "folder.new",
        label: "New folder",
        keys: "ctrl-shift-n, shift-n in the tree",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(NewFolder)),
    },
    Command {
        id: "note.rename",
        label: "Rename the selected note",
        keys: "ctrl-shift-r, r in the tree",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(RenameNote)),
    },
    Command {
        id: "note.delete",
        label: "Delete the selected note",
        keys: "ctrl-shift-d, shift-d in the tree",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(DeleteNote)),
    },
    Command {
        id: "note.open",
        label: "Open a note",
        keys: "space f, g f",
        ex: &["e", "edit", "open"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(QuickOpen)),
    },
    Command {
        id: "jump.back",
        label: "Jump back to the previous note",
        keys: "ctrl-o",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(JumpBack)),
    },
    Command {
        id: "jump.forward",
        label: "Jump forward to the next note",
        keys: "ctrl-i",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(JumpForward)),
    },
    Command {
        id: "vault.search",
        label: "Search the vault",
        keys: "ctrl-shift-f, space s",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(SearchVault)),
    },
    Command {
        id: "theme.change",
        label: "Change the theme",
        keys: "",
        // Bare `:theme` opens the picker; `:theme <name>` is the argument form,
        // spelled by the `args` field and handled in `ex`.
        ex: &["theme"],
        args: "[name]",
        in_palette: true,
        action: Some(|| Box::new(ChangeTheme)),
    },
    Command {
        id: "checker.on",
        label: "Turn the spellchecker on",
        keys: "",
        ex: &["harper on", "harper 1", "spellcheck on", "spell on"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(CheckerOn)),
    },
    Command {
        id: "checker.off",
        label: "Turn the spellchecker off",
        keys: "",
        ex: &["harper off", "harper 0", "spellcheck off", "spell off"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(CheckerOff)),
    },
    Command {
        id: "checker.show",
        label: "Show the checker's note for the word here",
        keys: "K",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(ShowNote)),
    },
    Command {
        id: "view.raw",
        label: "Show the note as markdown source",
        keys: "space r",
        ex: &["raw", "source", "src"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(ViewRaw)),
    },
    Command {
        id: "view.live",
        label: "Show the note with live preview",
        keys: "space l",
        ex: &["live"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(ViewLive)),
    },
    Command {
        id: "view.preview",
        label: "Preview the note, without editing it",
        keys: "space p",
        ex: &["preview", "view"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(ViewPreview)),
    },
    Command {
        id: "focus.toggle",
        label: "Dim everything but the block the cursor is in",
        keys: "space d",
        // Bare `:focus` is the toggle the action runs; `on|off` and the scope
        // words are argument forms handled in `ex`, spelled here for the docs.
        ex: &["focus", "dim"],
        args: "[on|off|block|section]",
        in_palette: true,
        action: Some(|| Box::new(ToggleFocus)),
    },
    Command {
        id: "typewriter.toggle",
        label: "Keep the cursor's line near the middle of the window",
        keys: "",
        ex: &["typewriter"],
        args: "[on|off]",
        in_palette: true,
        action: Some(|| Box::new(ToggleTypewriter)),
    },
    Command {
        id: "window.left",
        label: "Focus the file tree",
        keys: "ctrl-w h",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(WindowLeft)),
    },
    Command {
        id: "window.right",
        label: "Focus the note",
        keys: "ctrl-w l",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(WindowRight)),
    },
    Command {
        id: "note.undo",
        label: "Undo",
        keys: "u, ctrl-z",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(Undo)),
    },
    Command {
        id: "note.redo",
        label: "Redo",
        keys: "ctrl-r, ctrl-y, ctrl-shift-z",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(Redo)),
    },
    Command {
        id: "clipboard.copy",
        label: "Copy the selection to the clipboard",
        keys: "ctrl-shift-c",
        ex: &[],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(CopyText)),
    },
    Command {
        id: "clipboard.paste",
        label: "Paste from the clipboard",
        keys: "ctrl-shift-v",
        ex: &["paste", "p"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(PasteText)),
    },
    Command {
        id: "clipboard.yank",
        label: "Copy the selection, or the last yank, to the clipboard",
        keys: "space y",
        ex: &["y", "yank", "clip", "copy"],
        args: "",
        in_palette: true,
        action: Some(|| Box::new(YankClipboard)),
    },
    Command {
        id: "vault.wordcount",
        label: "Count the vault's words",
        keys: "",
        ex: &["wordcount", "wc"],
        args: "[folder]",
        in_palette: true,
        action: Some(|| Box::new(WordCount)),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ex;

    /// The heart of P2.1: the palette and the command line dispatch the same
    /// command ids. Every `ex` word the registry lists must parse back to the
    /// command it is listed under — so the two vocabularies cannot drift apart.
    #[test]
    fn every_ex_word_resolves_to_the_command_that_lists_it() {
        for command in Command::every() {
            for word in command.ex {
                let parsed = ex::parse(word);
                assert_eq!(
                    parsed.command_id(),
                    Some(command.id),
                    "`:{word}` should dispatch {}, the command that lists it",
                    command.id
                );
            }
        }
    }

    /// The other direction: every command the command line understands belongs
    /// to the registry, so `ex` cannot grow a command the docs and MCP never see.
    #[test]
    fn every_parsed_command_has_a_registry_id() {
        // One representative line per `Ex` variant that runs something.
        let lines = [
            "w",
            "q",
            "wq",
            "e",
            "harper on",
            "harper off",
            "focus",
            "focus on",
            "focus section",
            "typewriter",
            "typewriter off",
            "theme",
            "theme gruvbox-dark",
            "raw",
            "live",
            "preview",
            "yank",
            "paste",
            "wordcount notes",
        ];
        for line in lines {
            let parsed = ex::parse(line);
            let id = parsed
                .command_id()
                .unwrap_or_else(|| panic!("`:{line}` parsed to a command with no id"));
            assert!(
                Command::by_id(id).is_some(),
                "`:{line}` resolved to {id}, which is not in the registry"
            );
        }
    }

    /// The docs are generated, which only helps if something checks them. Reading
    /// the file back and comparing it to the table is the whole freshness
    /// guarantee; `SHALE_UPDATE_DOCS=1` rewrites the file instead of failing, which
    /// is what you run after editing the table.
    #[test]
    fn the_docs_are_the_registry() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/commands.md");
        let want = Command::docs();
        if std::env::var_os("SHALE_UPDATE_DOCS").is_some() {
            std::fs::write(path, &want).expect("could not write docs/commands.md");
            return;
        }
        let got = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{path}: {err}"));
        assert_eq!(
            got, want,
            "docs/commands.md is stale: run `SHALE_UPDATE_DOCS=1 cargo test -p shale-app`"
        );
    }

    /// Ids are the contract agents and docs rely on, so they must be unique.
    #[test]
    fn command_ids_are_unique() {
        let all = Command::every();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.id, b.id, "two commands share the id {}", a.id);
            }
        }
    }

    /// Every palette row can be dispatched: the palette calls `action()` on each.
    #[test]
    fn palette_rows_all_have_an_action() {
        for command in Command::all() {
            assert!(
                command.action.is_some(),
                "the palette lists {}, which has no action",
                command.id
            );
        }
    }
}
