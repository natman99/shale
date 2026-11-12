//! vim's command line: `:` and a word, matched exactly.
//!
//! The command palette fuzzy-matches what you type, which is right for a list you
//! look through and wrong for a command you spell. In a fuzzy list `:w` is whatever
//! sorts best for the letter `w` — and `enter` runs it. So the two have separate
//! entry points (`ctrl-shift-p` for the list, `:` for the line), and this module is
//! the line's whole vocabulary: a word that is not here does nothing, and says so.
//!
//! The names are vim's, because the reflex is vim's. `w` is a real save rather
//! than a no-op: it costs nothing when the note is already clean, and it is what
//! the user types when they do not trust the autosave to have run yet.

use crate::command::Command;
use crate::editor::View;

/// What a typed command line asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ex {
    /// Nothing was typed: `:` then `enter` simply closes.
    Empty,
    /// `w` / `write`: write the open note.
    Write,
    /// `q` / `quit`: close Shale, writing the note first if it is unsaved.
    Quit,
    /// `q!` / `quit!`: close Shale and throw the note's unsaved edits away. The bang
    /// is the user saying they know; without it a quit never loses work.
    QuitForce,
    /// `wq` / `x`: write, then close.
    WriteQuit,
    /// `e` / `open`: the quick-open picker.
    Open,
    /// `harper on|1` / `harper off|0`: the spell and grammar checker.
    Checker(bool),
    /// `focus on|1` / `focus off|0`, or bare `focus` to turn it around: dim
    /// everything but the block the cursor is in. `dim` is the same command under
    /// the name the feature is usually called.
    Focus(Option<bool>),
    /// `focus block` / `focus section`: how far the lit region reaches. The mode
    /// itself is left alone, because asking how wide the light should be is not a
    /// request to turn it on.
    FocusScope(String),
    /// `typewriter on|1` / `typewriter off|0`, or bare `typewriter` to turn it
    /// around: keep the cursor's line near the middle of the window.
    Typewriter(Option<bool>),
    /// `tree on|1` / `tree off|0`, or bare `tree` to turn it around: put the file
    /// tree away, or bring it back. The note takes the width either way.
    Tree(Option<bool>),
    /// `sidebar on|1` / `sidebar off|0`, or bare `sidebar` to turn it around: put the
    /// right-hand strip away, or bring it back. The note takes the width either way.
    Sidebar(Option<bool>),
    /// `sidebar outline|backlinks|links|tags|properties`: show that view. The strip shows
    /// one view at a time, so this is a switch rather than a scroll.
    SidebarView(crate::sidebar::View),
    /// `theme` with nothing after it: the theme picker.
    ThemePicker,
    /// `vault`: the vault switcher — the vaults this app remembers, or a
    /// folder from the file dialog.
    VaultPicker,
    /// `new-window`: another window on the vault this one shows.
    NewWindow,
    /// `theme <name>`: that palette, now.
    Theme(String),
    /// `theme export <name>`: the palette on screen, written to
    /// `.shale/themes/<name>.toml` as a whole file (§15).
    ///
    /// The word `export` is taken by this command, so a vault theme literally
    /// named `export` is reachable by the picker and by `appearance.theme` but
    /// not by name at the `:` line. Naming the subcommand wins over naming a
    /// theme that one; the collision is written here so it is not a surprise.
    ThemeExport(String),
    /// `raw`, `live` or `preview`: how the note is presented, and whether it can
    /// be edited. `:live` is the way back to Shale's default view.
    View(View),
    /// `y` / `yank` / `clip`: the selection, or the last yank, out to the system
    /// clipboard. vim's `:yank` copies the current line into its own register;
    /// Shale's command line has no addresses yet, and the copy a user reaches for
    /// here is the one other programs can see.
    YankClipboard,
    /// `p` / `paste`: the system clipboard in at the caret, and into the vim
    /// register, so `p` and `.` can carry the same text on.
    PasteClipboard,
    /// `wordcount` / `wc [folder]`: how many words the vault — or one folder of it,
    /// as typed — holds. The count comes from the vault index, so it is the index's
    /// answer rather than a fresh read of every note.
    WordCount(String),
    /// `props` / `properties`: the note the caret is in, as the index holds it — one
    /// row per authored value, with the kind the indexer worked out (P4.3a).
    Props,
    /// Every property key the vault writes (`:keys`, P4.3a M5).
    PropertyKeys,
    /// `rename-key [old] [new]`: rename a property key in every note that writes it
    /// (P4.3a M5). What is missing is asked for rather than guessed: no key opens the
    /// key list, no new name opens the prompt. A key with a space in it is reachable
    /// through the list, not typed here — the command line splits on spaces.
    RenameKey(String, String),
    /// `sections [pattern]`: jump to a section (P4.3b M3). No pattern outlines the
    /// note on screen from its buffer; a pattern searches the vault's sections through
    /// the index. The rest of the line is the pattern verbatim, because a section name
    /// can hold a space.
    Sections(String),
    /// `blocks [pattern]`: jump to a block (P4.9 M4). No pattern lists the blocks the
    /// note on screen has ids for, read from its buffer; a pattern searches the vault's
    /// ids through the index. The rest of the line is the pattern verbatim.
    Blocks(String),
    /// `block-id [id]`: give the block under the caret an id (P4.9 M4). With no id one
    /// is generated; a typed id has to fit the grammar and be unused in the vault.
    BlockId(String),
    /// `query`: run a saved query (`:query`, P4.3 M2). The name is not an argument —
    /// the command opens the list of `.shq` files, because the file's *name* is what a
    /// reader remembers and the picker is the surface that matches against it.
    Query,
    /// `save-query <search>`: keep a search as a `.shq` (P4.3 M6). The rest of the line
    /// is the search **verbatim** — `tag:monster prop:cr<=5` is one question with a space
    /// in it, not two arguments, and the name is asked for afterwards because the reader
    /// gives it, not the command line.
    SaveQuery(String),
    /// `review` / `pending`: give the keyboard to the review strip, the list of agent
    /// edits waiting to be accepted or rejected (roadmap P2.5a). The strip appears by
    /// itself whenever there is something in the queue; this is how the user gets to
    /// it from the note, and how the quit prompt tells them to.
    Review,
    /// `tabnew` / `tabedit`: a new tab holding the note in hand, so the same note can
    /// be read in two places at once, each with its own cursor and undo history
    /// (roadmap P3.1).
    TabNew,
    /// `tabnext` / `tabn`: the next tab, wrapping. The command-line spelling of `g t`.
    TabNext,
    /// `tabprev` / `tabp`: the previous tab, wrapping. The spelling of `g T`.
    TabPrev,
    /// `tabclose` / `tabc`: close the tab on screen, its note written first.
    TabClose,
    /// `tabonly` / `tabo`: close every tab but this one.
    TabOnly,
    /// `w <name>` / `write <name>`: give the note in hand a name and write it --
    /// only for a tab that has no name yet. A tab that already has a file refuses
    /// out loud: the command line must not be a way to clone or move a note by
    /// accident (ruled 2026-11-05). Renaming is `:rename`, which rewrites links.
    WriteAs(String),
    /// `tabmove <n>`: move this tab to position `n`, 1-based as typed. The one tab
    /// command with an argument, so it is the one that does not travel as an action.
    TabMove(usize),
    /// `graph` / `g`: the vault's link graph, in a tab of its own. One per window, so
    /// a second `:graph` moves the frame to the graph that is already open (roadmap
    /// P3.4).
    Graph,
    /// `graph2d` / `graph3d`: turn the graph tab between its two spaces (roadmap P3.6).
    /// The space is an argument in all but spelling, so it travels as a payload on one
    /// variant rather than two names that could drift.
    GraphMode(bool),
    /// `graph-local [n]` / `graph-global`: the graph as the neighbourhood of the note on
    /// screen, to depth `n` (M2b). One variant carrying the depth, and `graph-global` is
    /// the same request with the scope switched off.
    GraphLocal(Option<usize>),
    /// The panes (roadmap P3.2). vim's window commands, spelled the way vim does:
    /// `split`/`sp` stacks a second pane under this one, `vsplit`/`vs` puts one
    /// beside it, `close`/`clo` closes this pane (its notes written first), and
    /// `only`/`on` keeps only this one.
    Split,
    VSplit,
    WindowClose,
    WindowOnly,
    /// Anything else, with the word that was typed. Nothing runs: an unknown word
    /// is a typo, not something to guess at.
    Unknown(String),
}

impl Ex {
    /// The registry id this line dispatches, or `None` for `Empty` and `Unknown`
    /// (which run nothing). The argument forms (`:theme gruvbox`,
    /// `:wordcount notes/`, `:focus section`) resolve to the id of the command
    /// they are an argument *of*, which is what lets the parity test in
    /// [`crate::command`] hold for every spelling.
    pub(crate) fn command_id(&self) -> Option<&'static str> {
        Some(match self {
            Ex::Empty | Ex::Unknown(_) => return None,
            Ex::Write | Ex::WriteAs(_) => "note.save",
            Ex::Quit => "app.quit",
            Ex::QuitForce => "app.quit-force",
            Ex::WriteQuit => "note.write-quit",
            Ex::Open => "note.open",
            Ex::Checker(true) => "checker.on",
            Ex::Checker(false) => "checker.off",
            Ex::Focus(_) | Ex::FocusScope(_) => "focus.toggle",
            Ex::Typewriter(_) => "typewriter.toggle",
            Ex::Tree(_) => "tree.toggle",
            // The same shape as the tree's: the bare word and the on/off form are one
            // command; `:sidebar <view>` names a view and keeps its handling in the
            // workspace.
            Ex::Sidebar(None) | Ex::Sidebar(Some(_)) | Ex::SidebarView(_) => "sidebar.toggle",
            Ex::ThemePicker | Ex::Theme(_) => "theme.change",
            Ex::VaultPicker => "vault.switch",
            Ex::NewWindow => "window.new",
            Ex::ThemeExport(_) => "theme.export",
            Ex::View(View::Raw) => "view.raw",
            Ex::View(View::Live) => "view.live",
            Ex::View(View::Preview) => "view.preview",
            Ex::YankClipboard => "clipboard.copy",
            Ex::PasteClipboard => "clipboard.paste",
            Ex::WordCount(_) => "vault.wordcount",
            Ex::Props => "vault.props",
            Ex::PropertyKeys => "vault.keys",
            Ex::RenameKey(..) => "vault.rename-key",
            Ex::Sections(..) => "vault.sections",
            Ex::Blocks(..) => "vault.blocks",
            Ex::BlockId(..) => "note.block-id",
            Ex::Query => "vault.query",
            Ex::SaveQuery(..) => "vault.save-query",
            Ex::Review => "review.focus",
            Ex::TabNew => "tab.new",
            Ex::TabNext => "tab.next",
            Ex::TabPrev => "tab.prev",
            Ex::TabClose => "tab.close",
            Ex::TabOnly => "tab.only",
            Ex::TabMove(_) => "tab.move",
            Ex::Graph => "graph.open",
            Ex::GraphMode(false) => "graph.flat",
            Ex::GraphMode(true) => "graph.projected",
            // The same variant spells both: `0` is the whole vault, which is a different
            // row of the registry with its own words.
            Ex::GraphLocal(Some(0)) => "graph.global",
            Ex::GraphLocal(_) => "graph.local",
            Ex::Split => "window.split",
            Ex::VSplit => "window.vsplit",
            Ex::WindowClose => "window.close",
            Ex::WindowOnly => "window.only",
        })
    }

    /// The registry command, when this line runs one with no argument the action
    /// cannot carry. Those are dispatched through the action — the same call the
    /// palette makes — so `:w`, `ctrl-s` and the palette row are one dispatch
    /// reached three ways. The argument and composite forms (`:theme <name>`,
    /// `:focus section`, `:wq`, …) return `None` and keep their handling in
    /// [`crate::workspace`].
    pub(crate) fn no_arg_command(&self) -> Option<Command> {
        Command::by_id(match self {
            Ex::Write
            | Ex::Quit
            | Ex::QuitForce
            | Ex::Open
            | Ex::Checker(_)
            | Ex::Focus(None)
            | Ex::Typewriter(None)
            | Ex::Tree(None)
            | Ex::Sidebar(None)
            | Ex::ThemePicker
            | Ex::VaultPicker
            | Ex::NewWindow
            | Ex::View(_)
            | Ex::YankClipboard
            | Ex::PasteClipboard
            | Ex::Review
            | Ex::PropertyKeys
            | Ex::Query
            | Ex::TabNew
            | Ex::TabNext
            | Ex::TabPrev
            | Ex::TabClose
            | Ex::TabOnly
            | Ex::Graph
            | Ex::GraphMode(_)
            // Only the bare form: `:graph-local 3` carries an argument and keeps its
            // handling in the workspace.
            | Ex::GraphLocal(None)
            | Ex::Split
            | Ex::VSplit
            | Ex::WindowClose
            | Ex::WindowOnly => self.command_id()?,
            _ => return None,
        })
    }
}

/// Read one command line. Case-insensitive, like vim's command abbreviations.
pub(crate) fn parse(line: &str) -> Ex {
    let line = line.trim();
    if line.is_empty() {
        return Ex::Empty;
    }
    let (word, rest) = match line.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (line, ""),
    };
    let word = word.to_ascii_lowercase();
    match word.as_str() {
        // The bare word writes the note in hand; anything after it names the note
        // first, which is the only way to write a tab that has no file yet under a
        // name the user chose.
        "w" | "write" if rest.is_empty() => Ex::Write,
        "w" | "write" => Ex::WriteAs(rest.to_string()),
        "q" | "quit" => Ex::Quit,
        // The bang is part of the word, not a separate argument: vim reads `q!` as
        // one command, and so does this.
        "q!" | "quit!" => Ex::QuitForce,
        "wq" | "x" | "xit" | "exit" => Ex::WriteQuit,
        "e" | "edit" | "open" => Ex::Open,
        "harper" | "spellcheck" | "spell" => match rest.to_ascii_lowercase().as_str() {
            "on" | "1" | "true" | "yes" => Ex::Checker(true),
            "off" | "0" | "false" | "no" => Ex::Checker(false),
            _ => Ex::Unknown(format!("harper {rest}").trim().to_string()),
        },
        "vault" if rest.is_empty() => Ex::VaultPicker,
        "new-window" if rest.is_empty() => Ex::NewWindow,
        "theme" => {
            // `export` is a subcommand, not a theme name. A vault theme actually
            // called `export` is still a picker row and still a valid
            // `appearance.theme`; only the `:` line gives the word to the command.
            if rest.is_empty() {
                Ex::ThemePicker
            } else {
                let (head, tail) = match rest.split_once(char::is_whitespace) {
                    Some((head, tail)) => (head, tail.trim()),
                    None => (rest, ""),
                };
                if head.eq_ignore_ascii_case("export") {
                    // No name is not a typo for a theme called "export": it is the
                    // command missing its argument, and `export_theme` says so in
                    // the same voice as the rest of the bar.
                    Ex::ThemeExport(tail.to_string())
                } else {
                    Ex::Theme(rest.to_string())
                }
            }
        }
        // Both are a preference the user moves in and out of while writing, so a
        // bare word turns it around rather than asking which half was meant — the
        // opposite of `harper`, which starts and stops a process and would be
        // annoying to do by accident. A word that is neither yes nor no is still a
        // typo, and says which.
        "focus" | "dim" => match bool_word(rest) {
            Some(on) => Ex::Focus(Some(on)),
            None if rest.is_empty() => Ex::Focus(None),
            // A scope word is the other thing `:focus` can be asked for. Spelled
            // here rather than through `dim::Scope::from_word` so a typo stays a
            // typo: `:focus blockk` runs nothing, while a scope that exists but is
            // not implemented yet (`sentence`) is accepted and answered with the
            // default.
            None if matches!(
                rest.to_ascii_lowercase().as_str(),
                "block" | "section" | "sentence"
            ) =>
            {
                Ex::FocusScope(rest.to_ascii_lowercase())
            }
            None => Ex::Unknown(format!("{word} {rest}")),
        },
        "typewriter" => match bool_word(rest) {
            Some(on) => Ex::Typewriter(Some(on)),
            None if rest.is_empty() => Ex::Typewriter(None),
            None => Ex::Unknown(format!("{word} {rest}")),
        },
        // The sidebar. Bare `tree` is the toggle, because that is what `space e`
        // does and the two doors have to agree. `:e` is `:edit`, so the word here
        // is the whole command: no abbreviation of `tree` is taken, because `tr`
        // would sit too close to words this parser cannot tell apart yet.
        "tree" => match bool_word(rest) {
            Some(on) => Ex::Tree(Some(on)),
            None if rest.is_empty() => Ex::Tree(None),
            None => Ex::Unknown(format!("{word} {rest}")),
        },
        // The right-hand strip. Bare `sidebar` is the toggle, the same shape `:tree`
        // takes; a view word switches what the strip shows. `:sidebar sideways` is
        // refused rather than guessed at, because a wrong view is a confusing answer to
        // a command that was spelled with intent.
        "sidebar" => match rest {
            "" => Ex::Sidebar(None),
            rest => match bool_word(rest) {
                Some(on) => Ex::Sidebar(Some(on)),
                None => match crate::sidebar::View::from_word(rest) {
                    Some(view) => Ex::SidebarView(view),
                    None => Ex::Unknown(format!("{word} {rest}")),
                },
            },
        },
        // The three views. `source` and `src` are there because that is the word
        // other markdown editors use for the same view, and `view` because the
        // user reaching for a view is asking for the one they can read in.
        "raw" | "source" | "src" => Ex::View(View::Raw),
        "live" => Ex::View(View::Live),
        "preview" | "view" => Ex::View(View::Preview),
        // The clipboard, both directions. `y` and `p` are the letters the reflex
        // already knows; `clip` and `paste` spell it out for the rest.
        // `space y` asks for the outward copy; the action is the one `ctrl-shift-c`
        // is bound to, so the chord, the key and `:y` are one command.
        "y" | "yank" | "clip" | "copy" => Ex::YankClipboard,
        "p" | "paste" => Ex::PasteClipboard,
        // The folder is whatever follows the word, so `:wordcount notes/daily` is
        // one argument; an unknown word after it is not guessed at.
        "wordcount" | "wc" => Ex::WordCount(rest.to_string()),
        // The note's own properties. `properties` is spelled out because `:props`
        // next to `:wordcount` is not self-explanatory the first time.
        "props" | "properties" => Ex::Props,
        // Every property key in the vault, with how far each reaches. `keys` is the
        // word because the pane is a list of keys, not of properties: `:props` names
        // one note's, and the two words have to stay two words.
        "keys" => Ex::PropertyKeys,
        // A saved question, run rather than read: the picker lists the vault's `.shq`
        // files and the answer comes back as rows.
        "sections" => Ex::Sections(rest.to_string()),
        // One block, by its id. The same two halves as `:sections`: the note on screen
        // from its buffer, or the vault from the index.
        "blocks" => Ex::Blocks(rest.to_string()),
        // The id is the whole rest of the line, because an id can hold a `-`.
        "block-id" => Ex::BlockId(rest.to_string()),
        "query" => Ex::Query,
        "save-query" => Ex::SaveQuery(rest.to_string()),
        "rename-key" => {
            // Two names, split on the space between them. A third word is not
            // silently dropped: it is the reader typing a folder or a quote, and
            // running something they did not mean is worse than running nothing.
            let mut names = rest.split_whitespace();
            let old = names.next().unwrap_or("").to_string();
            let new = names.next().unwrap_or("").to_string();
            if names.next().is_some() {
                Ex::Unknown(format!("rename-key {rest}"))
            } else {
                Ex::RenameKey(old, new)
            }
        }
        // The review strip. `pending` is there because that is the word the queue is
        // named by on disk, and the quit prompt says "agent edits are waiting".
        "review" | "pending" => Ex::Review,
        // The graph. `g` is the letter the chord already uses (`space g`), and the
        // command line is exact, so it cannot collide with anything else.
        "graph" | "g" => Ex::Graph,
        "graph2d" => Ex::GraphMode(false),
        "graph3d" => Ex::GraphMode(true),
        // The local graph. A bare `:graph-local` turns it on at the default depth; a
        // number sets the depth, and `0` is the whole vault again — the same shape as
        // the strip's chip and the `[`/`]` keys, so the hands and the command line agree.
        "graph-local" | "loc" => match rest {
            "" => Ex::GraphLocal(None),
            "0" | "off" | "all" => Ex::GraphLocal(Some(0)),
            rest => match rest.parse::<usize>() {
                Ok(n) if n > 0 => Ex::GraphLocal(Some(n)),
                _ => Ex::Unknown(format!("graph-local {rest}").trim().to_string()),
            },
        },
        "graph-global" => Ex::GraphLocal(Some(0)),
        // The tabs (roadmap P3.1). vim's spellings, abbreviated the way vim allows,
        // and `:tabmove` is the only one that takes a number.
        "tabnew" | "tabedit" => Ex::TabNew,
        "tabnext" | "tabn" => Ex::TabNext,
        "tabprev" | "tabp" => Ex::TabPrev,
        "tabclose" | "tabc" => Ex::TabClose,
        "tabonly" | "tabo" => Ex::TabOnly,
        // The panes (roadmap P3.2). `:on` is vim's abbreviation for `:only`;
        // the word only ever appears as an argument elsewhere (`:checker on`),
        // and the command word is read before any argument, so they cannot
        // collide.
        "split" | "sp" => Ex::Split,
        "vsplit" | "vs" => Ex::VSplit,
        "close" | "clo" => Ex::WindowClose,
        "only" | "on" => Ex::WindowOnly,
        // A number that is not a number is a typo, and runs nothing — the same
        // posture `:theme` takes with a name it cannot find. With no number at all,
        // vim moves the tab to the end, and that is what this does too.
        "tabmove" => match rest {
            "" => Ex::TabMove(usize::MAX),
            _ => match rest.parse::<usize>() {
                Ok(n) if n > 0 => Ex::TabMove(n),
                _ => Ex::Unknown(format!("tabmove {rest}")),
            },
        },
        _ => Ex::Unknown(word),
    }
}

/// The yes and no a command line's second word spells, or `None` for anything
/// else — including nothing at all, which is how a bare command says "turn it
/// around".
fn bool_word(rest: &str) -> Option<bool> {
    match rest.to_ascii_lowercase().as_str() {
        "on" | "1" | "true" | "yes" => Some(true),
        "off" | "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vim_aliases_are_the_short_ones() {
        // The point of these tests: `:w` is `write`, and never whatever a fuzzy
        // list happens to rank first for the letter `w`.
        assert_eq!(parse("w"), Ex::Write);
        assert_eq!(parse("write"), Ex::Write);
        assert_eq!(parse("q"), Ex::Quit);
        assert_eq!(parse("quit"), Ex::Quit);
        assert_eq!(parse("q!"), Ex::QuitForce);
        assert_eq!(parse("quit!"), Ex::QuitForce);
        assert_eq!(parse("wq"), Ex::WriteQuit);
        assert_eq!(parse("x"), Ex::WriteQuit);
        assert_eq!(parse("e"), Ex::Open);
        assert_eq!(parse("open"), Ex::Open);
    }

    #[test]
    fn the_checker_takes_both_spellings_of_yes_and_no() {
        for on in ["harper on", "harper 1", "harper ON", "spellcheck yes"] {
            assert_eq!(parse(on), Ex::Checker(true), "{on}");
        }
        for off in ["harper off", "harper 0", "harper Off", "spellcheck no"] {
            assert_eq!(parse(off), Ex::Checker(false), "{off}");
        }
        // `harper` on its own is not a guess about which half was meant.
        assert_eq!(parse("harper"), Ex::Unknown("harper".into()));
    }

    #[test]
    fn theme_names_a_palette_and_bare_theme_asks_for_the_picker() {
        assert_eq!(parse("theme"), Ex::ThemePicker);
        assert_eq!(parse("vault"), Ex::VaultPicker);
        assert_eq!(parse("new-window"), Ex::NewWindow);
        assert_eq!(
            parse("theme   gruvbox-dark  "),
            Ex::Theme("gruvbox-dark".into())
        );
        // `theme export <name>` is the subcommand (§15), and only the exact word
        // at the head of the argument is the subcommand: a theme whose name
        // merely starts with it is still a theme.
        assert_eq!(parse("theme export mine"), Ex::ThemeExport("mine".into()));
        assert_eq!(parse("THEME Export warm"), Ex::ThemeExport("warm".into()));
        assert_eq!(parse("theme export"), Ex::ThemeExport(String::new()));
        assert_eq!(parse("theme exporter"), Ex::Theme("exporter".into()));
        assert_eq!(
            parse("theme export my theme"),
            Ex::ThemeExport("my theme".into()),
            "the name is the rest of the line, not one word"
        );
    }

    #[test]
    fn the_three_views_are_named_not_guessed() {
        for raw in ["raw", "source", "src", "RAW"] {
            assert_eq!(parse(raw), Ex::View(View::Raw), "{raw}");
        }
        assert_eq!(parse("live"), Ex::View(View::Live));
        for preview in ["preview", "view", "Preview"] {
            assert_eq!(parse(preview), Ex::View(View::Preview), "{preview}");
        }
    }

    #[test]
    fn the_clipboard_goes_both_ways() {
        for yank in ["y", "yank", "clip", "copy", "YANK"] {
            assert_eq!(parse(yank), Ex::YankClipboard, "{yank}");
        }
        for paste in ["p", "paste", "Paste"] {
            assert_eq!(parse(paste), Ex::PasteClipboard, "{paste}");
        }
    }

    #[test]
    fn focus_and_typewriter_take_yes_and_no_and_a_bare_word_toggles() {
        for on in ["focus on", "focus 1", "dim ON", "typewriter yes"] {
            let want = if on.starts_with("typewriter") {
                Ex::Typewriter(Some(true))
            } else {
                Ex::Focus(Some(true))
            };
            assert_eq!(parse(on), want, "{on}");
        }
        for off in ["focus off", "focus 0", "dim Off", "typewriter no"] {
            let want = if off.starts_with("typewriter") {
                Ex::Typewriter(Some(false))
            } else {
                Ex::Focus(Some(false))
            };
            assert_eq!(parse(off), want, "{off}");
        }
        // A bare word is a toggle, not a guess.
        assert_eq!(parse("focus"), Ex::Focus(None));
        assert_eq!(parse("dim"), Ex::Focus(None));
        assert_eq!(parse("typewriter"), Ex::Typewriter(None));
        // A scope word is the other thing `:focus` answers, and it is not a
        // toggle: asking how wide the light is does not turn it on.
        assert_eq!(parse("focus block"), Ex::FocusScope("block".into()));
        assert_eq!(parse("dim SECTION"), Ex::FocusScope("section".into()));
        // Named on the roadmap, not built yet: accepted, and the window answers
        // with the default rather than pretending otherwise.
        assert_eq!(parse("focus sentence"), Ex::FocusScope("sentence".into()));
        // A word that is neither yes nor no nor a scope is a typo, and says which.
        assert_eq!(
            parse("focus sideways"),
            Ex::Unknown("focus sideways".into())
        );
    }

    /// The sidebar answers the same three ways as the other writing preferences:
    /// yes, no, or a bare word that turns it around.
    #[test]
    fn the_file_tree_takes_yes_and_no_and_a_bare_word_toggles() {
        for on in ["tree on", "tree 1", "TREE yes"] {
            assert_eq!(parse(on), Ex::Tree(Some(true)), "{on}");
        }
        for off in ["tree off", "tree 0", "Tree NO"] {
            assert_eq!(parse(off), Ex::Tree(Some(false)), "{off}");
        }
        assert_eq!(parse("tree"), Ex::Tree(None));
        assert_eq!(parse("tree sideways"), Ex::Unknown("tree sideways".into()));
    }

    #[test]
    fn an_unknown_word_is_named_rather_than_guessed() {
        assert_eq!(parse("xyzzy"), Ex::Unknown("xyzzy".into()));
        assert_eq!(parse(""), Ex::Empty);
        assert_eq!(parse("   "), Ex::Empty);
    }

    /// The review strip is reachable by word as well as by key, which is what the
    /// quit prompt points at.
    #[test]
    fn review_opens_the_strip() {
        for word in ["review", "pending", "REVIEW"] {
            assert_eq!(parse(word), Ex::Review, "{word}");
        }
    }

    /// The other half of the id parity test in `command`: every word `parse` knows
    /// that carries no argument must be a row in the registry, because the command
    /// line runs those as actions rather than by matching on the word. A variant
    /// left out of `no_arg_command` would reach an `unreachable!` at runtime, and
    /// this is what keeps that from happening.
    #[test]
    fn every_command_with_no_argument_has_an_id() {
        for command in [
            Ex::Write,
            Ex::Quit,
            Ex::QuitForce,
            Ex::Open,
            Ex::Checker(true),
            Ex::PropertyKeys,
            Ex::Checker(false),
            Ex::Focus(None),
            Ex::Typewriter(None),
            Ex::Tree(None),
            Ex::ThemePicker,
            Ex::View(View::Raw),
            Ex::View(View::Live),
            Ex::View(View::Preview),
            Ex::YankClipboard,
            Ex::PasteClipboard,
            Ex::Review,
        ] {
            assert!(
                command.no_arg_command().is_some(),
                "{command:?} takes no argument but has no command id"
            );
        }
    }

    /// And what carries an argument stays in the command line, where the value can
    /// be acted on.
    #[test]
    fn a_command_with_an_argument_is_not_dispatched_by_id() {
        for command in [
            Ex::Empty,
            Ex::WriteQuit,
            Ex::Focus(Some(true)),
            Ex::Typewriter(Some(false)),
            Ex::Tree(Some(false)),
            Ex::Theme("catppuccin-mocha".into()),
            Ex::FocusScope("heading".into()),
            Ex::WordCount("notes".into()),
            Ex::SaveQuery("tag:monster".into()),
            Ex::Unknown("xyzzy".into()),
        ] {
            assert!(
                command.no_arg_command().is_none(),
                "{command:?} carries an argument and must not be dispatched by id"
            );
        }
    }
}
