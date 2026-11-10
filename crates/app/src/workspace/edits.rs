//! The one-line prompt and the vault edits behind its answers.
//!
//! A prompt is the only place a name is typed, so it is also the only place a
//! note or folder can be named: the answer is what touches the disk, through the
//! checked edits in [`shale_core::Vault`].

use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context as _, Result};
use gpui::{prelude::*, Context, Entity, Focusable, Window};
use shale_core::rename::Rewrite;
use shale_core::vault::rel_text;

use crate::acts::*;
use crate::ex;
use crate::prompt;

use super::known_names;
use super::tabs::TabContent;
use super::Workspace;

impl Workspace {
    /// Open the one-line prompt. This is the only place a name is typed, so it is
    /// also the only place a note or folder can be named.
    pub(super) fn open_prompt(
        &mut self,
        kind: prompt::PromptKind,
        label: &str,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The field is drawn in the note's font so a typed name looks like the text it
        // will become.
        let (family, size) = self.note_font(cx);
        let palette = Rc::clone(&self.palette);
        let prompt = cx
            .new(|cx| prompt::Prompt::new(kind.clone(), label, initial, palette, family, size, cx));
        // A prompt owns the keyboard, so a popup cannot stay up beside it, and a card
        // cannot float over the thing being renamed.
        self.close_popup(window, cx);
        self.close_card(cx);
        self._prompt_events = Some(cx.subscribe_in(
            &prompt,
            window,
            |ws, prompt, event, window, cx| match event {
                prompt::PromptEvent::Submit(text) => {
                    ws.answer_prompt(prompt, text.as_str(), window, cx)
                }
                prompt::PromptEvent::Cancel => ws.cancel_prompt(prompt, window, cx),
                // The search answers as it is typed; the prompts that name
                // something ignore this and wait for an answer.
                prompt::PromptEvent::Changed(text) => ws.prompt_changed(prompt, text.as_str(), cx),
            },
        ));
        self.prompt = Some((kind, prompt.clone()));
        self.prompt_return_focus = window.focused(cx);
        let focus = prompt.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// `:` — vim's command line: one word, matched exactly.
    ///
    /// It is kept apart from the command palette on purpose. The palette
    /// fuzzy-matches what is typed, which is what makes it good to look through
    /// and dangerous to spell at: `:w` typed out of vim habit would become
    /// whatever ranks best for the letter `w`, and `enter` would run it.
    pub(super) fn command_line(
        &mut self,
        _: &RunCommandLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_prompt(prompt::PromptKind::Command, ":", "", window, cx);
    }

    /// Close the prompt and hand the keyboard back to whatever had it before.
    pub(super) fn close_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompt.is_none() {
            return;
        }
        self.prompt = None;
        let focus = match self.prompt_return_focus.take() {
            Some(focus) => focus,
            None => self.active_focus(cx),
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    /// What the answer means. A delete answered anything but "yes" is a finished
    /// answer rather than one to fix; a name the vault cannot hold leaves the
    /// prompt open with what was typed, so the user fixes it instead of starting
    /// over.
    fn answer_prompt(
        &mut self,
        prompt: &Entity<prompt::Prompt>,
        answer: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = prompt.read(cx).kind();
        // A search is not an answer about a name either: `enter` only closes the
        // field and hands the matches to `n`/`N`. Its query is used exactly as
        // typed, spaces included, because searching for `foo ` is a real question.
        if kind == prompt::PromptKind::Search {
            self.search_submitted(answer, window, cx);
            return;
        }
        let answer = answer.trim();
        // A command line is a word, not a name for something on disk, and it is
        // finished either way: an unknown command is an answer, not something to
        // correct in place.
        if kind == prompt::PromptKind::Command {
            self.answer_command(answer, window, cx);
            return;
        }
        if kind == prompt::PromptKind::Delete && !answer.eq_ignore_ascii_case("yes") {
            self.say("delete cancelled", cx);
            self.close_prompt(window, cx);
            return;
        }
        match self.apply_prompt(kind, answer, cx) {
            Ok(()) => self.close_prompt(window, cx),
            Err(err) => {
                tracing::warn!("the prompt was refused: {err:#}");
                self.say(format!("{err:#}"), cx);
                cx.notify();
            }
        }
    }

    /// What a command line meant. The line closes before the command runs, so the
    /// keyboard is back where it was and a command that opens a picker takes it
    /// from there, exactly as if the command's key had been typed.
    ///
    /// A command that takes no argument is dispatched as the very action its key
    /// binding and its palette row dispatch, so the three doors cannot drift apart;
    /// `command::ALL` says which word means which id, and `ex::tests` says that every
    /// word `parse` knows without an argument is in that list. What is left in the
    /// match below is what only the command line can ask for: an argument.
    fn answer_command(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) {
        let command = ex::parse(line);
        self.close_prompt(window, cx);
        if let Some(command) = command.no_arg_command() {
            // The id is what the docs, the MCP surface and an agent all name, so a
            // command run from the command line is logged by id: that is the line to
            // read when a key, a palette row and a word disagree.
            tracing::debug!(id = command.id(), "command from the command line");
            window.dispatch_action(command.action(), cx);
            cx.notify();
            return;
        }
        match command {
            ex::Ex::Empty => {}
            ex::Ex::WriteQuit => {
                // Not one action: `:wq` is a write that does not care whether the
                // note was dirty, then the exit — and the exit has to be the
                // window's own, which flushes the database and writes the session
                // on the way out.
                self.write_quit(window, cx);
            }
            // `:w <name>`: name the note in hand, then write it. Only a tab with no
            // file takes the name -- see [`Workspace::write_note_as`].
            ex::Ex::WriteAs(name) => self.write_note_as(&name, cx),
            ex::Ex::Focus(Some(on)) => self.set_focus(on, cx),
            ex::Ex::FocusScope(word) => self.set_focus_scope(&word, cx),
            ex::Ex::Typewriter(Some(on)) => self.set_typewriter(on, cx),
            ex::Ex::Tree(Some(on)) => self.set_tree_visible(on, window, cx),
            ex::Ex::Theme(name) => match self.set_theme(&name, cx) {
                Ok(()) => {
                    // A hand-written theme that loads but paints something too
                    // faint to read is reported on the same line as the switch:
                    // the palette is kept, and the user learns what is wrong
                    // with it without opening a log.
                    let note = self.readability_suffix();
                    match self.persist_theme(&name) {
                        Ok(()) => self.say(format!("theme: {name}{note}"), cx),
                        Err(err) => {
                            tracing::warn!("could not save the theme: {err:#}");
                            self.say(format!("theme: {name}, not saved: {err:#}"), cx);
                        }
                    }
                }
                Err(err) => self.say(format!("{err:#}"), cx),
            },
            ex::Ex::WordCount(folder) => self.wordcount(&folder, cx),
            ex::Ex::RenameKey(old, new) => self.rename_key_command(&old, &new, window, cx),
            ex::Ex::Props => self.props(cx),
            ex::Ex::ThemeExport(name) => match self.export_theme(&name) {
                Ok(path) => self.say(format!("wrote {}", rel_text(&path)), cx),
                Err(err) => self.say(format!("{err:#}"), cx),
            },
            // The one tab command with an argument. Typed 1-based, like every number
            // in vim's command line.
            ex::Ex::TabMove(to) => self.move_tab(to - 1, cx),
            // `:graph-local [n]` / `:graph-global`. `Some(0)` is the whole vault: the
            // command line spells zero the way the strip's chip does by clicking it off.
            ex::Ex::GraphLocal(depth) => match depth {
                // `0` and `:graph-global` are the whole vault; a bare `:graph-local` is
                // the default depth, not a toggle, because the command line is a
                // statement of what the user wants rather than a shrug.
                Some(0) => self.graph_local_synced(None, cx),
                Some(n) => self.graph_local_synced(Some(n), cx),
                None => self.graph_local_synced(Some(crate::graph::DEFAULT_LOCAL_DEPTH), cx),
            },
            ex::Ex::Unknown(word) => self.say(format!("unknown command: {word}"), cx),
            // Every one of these carries no argument, so `no_arg_command` above
            // already dispatched it as an action.
            ex::Ex::Write
            | ex::Ex::Quit
            | ex::Ex::QuitForce
            | ex::Ex::Open
            | ex::Ex::Checker(_)
            | ex::Ex::Focus(None)
            | ex::Ex::Typewriter(None)
            | ex::Ex::Tree(None)
            | ex::Ex::ThemePicker
            | ex::Ex::VaultPicker
            | ex::Ex::NewWindow
            | ex::Ex::View(_)
            | ex::Ex::YankClipboard
            | ex::Ex::PasteClipboard
            | ex::Ex::Review
            | ex::Ex::PropertyKeys
            | ex::Ex::Query
            | ex::Ex::TabNew
            | ex::Ex::TabNext
            | ex::Ex::TabPrev
            | ex::Ex::TabClose
            | ex::Ex::TabOnly
            | ex::Ex::Graph
            | ex::Ex::GraphMode(_)
            | ex::Ex::Split
            | ex::Ex::VSplit
            | ex::Ex::WindowClose
            | ex::Ex::WindowOnly => {
                unreachable!("a command with no argument is dispatched by id")
            }
        }
        cx.notify();
    }

    /// The one outward copy: `ctrl-shift-c`, `space y`, and `:y`/`:yank`/`:clip`/
    /// `:copy` all come through here, because they are one command. The vim register
    /// is the engine's memory of what was yanked; the clipboard is the door other
    /// programs see, and only the window can open it — which is also why the status
    /// bar says what went out, or that there was nothing to send. A plain `y` needs
    /// no command at all: the editor mirrors its register to the clipboard (see
    /// `mirror_register`); this is the copy that *reports*.
    pub(crate) fn copy_text(&mut self, _: &CopyText, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor().cloned() else {
            self.say("no note open", cx);
            cx.notify();
            return;
        };
        match editor.read(cx).clipboard_candidate() {
            Some(text) => {
                let chars = text.chars().count();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                self.say(format!("copied {chars} chars to the clipboard"), cx);
            }
            None => self.say("nothing to copy", cx),
        }
        cx.notify();
    }

    /// The vault edits behind each kind of answer, kept apart from the
    /// bookkeeping so each one can refuse with a message the status bar shows.
    fn apply_prompt(
        &mut self,
        kind: prompt::PromptKind,
        answer: &str,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        match kind {
            prompt::PromptKind::NewNote => {
                let created = self.vault.create_note(self.target_dir().join(answer))?;
                self.refresh_entries(cx);
                self.index_created(&created);
                self.refresh_link_names(cx);
                self.tree_selection = Some(created.clone());
                self.open_note(&created, cx);
                self.say(format!("created {}", rel_text(&created)), cx);
            }
            prompt::PromptKind::NewFolder => {
                let dir = self.target_dir().join(answer);
                self.vault.create_dir(&dir)?;
                self.refresh_entries(cx);
                self.expanded.insert(dir.clone());
                self.tree_selection = Some(dir.clone());
                self.say(format!("created {}/", rel_text(&dir)), cx);
            }
            prompt::PromptKind::RenameKey(old) => {
                self.rename_property_key(&old, answer, cx)?;
            }
            prompt::PromptKind::Rename => {
                // Re-derived here, not only when the prompt opened: the answer is a
                // different moment, and the caret may have moved between them.
                let from = self.rename_target(cx)?;
                let to = sibling_path(&from, answer);
                // The links have to be found against the vault as it was *before*
                // the rename: resolving a target is what decides which notes point
                // at this one, and once the file has moved the old name resolves
                // to nothing. A folder rename is left for later — the links that
                // would need it spell the folder out, which is a different splice.
                let rewrites = if self.is_dir(&from) {
                    Vec::new()
                } else {
                    shale_core::rename::rewrites_for_rename(&self.vault, &self.entries, &from, &to)?
                };
                let renamed = self.vault.rename(&from, &to)?;
                let report = self.apply_rewrites(Some((&from, &renamed)), &rewrites, cx)?;
                self.refresh_entries(cx);
                // The note itself moves in one step, and the notes the rename
                // rewrote to point at it are re-indexed alongside it: Shale's own
                // writes are stamped out of the watcher's view, so nothing else
                // would ever notice them.
                self.index_renamed(&from, &renamed);
                self.index_rewrites(&report.written);
                self.refresh_link_names(cx);
                // The graph moves with the file: the old path is gone, and the new one
                // has to be re-scanned for the links inside it.
                self.graph_note_removed(&from);
                if let Ok(text) = self.vault.read(&renamed) {
                    self.graph_note_saved(&renamed, &text);
                }
                self.refresh_graph(cx);
                // Every tab that was showing the old name now shows the new one.
                // More than one can be, which is what a tab holding a note while
                // another tab holds the same note looks like from here.
                for pane in self.panes.iter_mut() {
                    for tab in pane.tabs.iter_mut() {
                        if let TabContent::Note { path, .. } = &mut tab.content {
                            if path.as_deref() == Some(from.as_path()) {
                                *path = Some(renamed.clone());
                            }
                        }
                    }
                }
                // The registry moves with the file, so an embed that now spells the
                // new name finds the buffer the caret is editing rather than re-reading
                // the disk copy — which, with unsaved edits in the block, is a version
                // the author has already edited past.
                self.notes.rename(&from, &renamed);
                for pane in self.panes.iter() {
                    for tab in pane.tabs.iter() {
                        if let Some(editor) = tab.editor() {
                            editor.update(cx, |editor, _cx| editor.embed_renamed(&from, &renamed));
                        }
                    }
                }
                // ...and a tab renamed out from under its proposals has to be told, or
                // it keeps painting a proposal that now belongs to no tab at all.
                self.sync_overlay(cx);
                // The jump chain named the old path, and a chain that points at a
                // note that no longer resolves is a dead end the user has to step
                // over by hand.
                self.jumps.rename(&from, &renamed);
                self.tree_selection = Some(renamed.clone());
                self.say(rename_status(&renamed, &report), cx);
            }
            // A command line is not a name for something on disk, and
            // `answer_prompt` sends it off to `answer_command` before this is
            // reached, so there is nothing to do here.
            prompt::PromptKind::Command => {}
            // The same for a search: `answer_prompt` keeps the query and its
            // matches in [`Workspace::search`] instead.
            prompt::PromptKind::Search => {}
            prompt::PromptKind::Delete => {
                let target = self.prompt_target()?;
                if self.is_dir(&target) {
                    self.vault.delete_dir(&target)?;
                } else {
                    self.vault.delete(&target)?;
                }
                // Every tab that was showing it becomes an empty tab: the note is
                // gone, and a tab holding a note that no longer exists is a lie.
                let mut cleared = false;
                for index in 0..self.pane().tabs.len() {
                    if self.pane().tabs[index].path() == Some(target.as_path()) {
                        self.clear_tab(index, cx);
                        cleared = true;
                    }
                }
                if cleared {
                    self.checker_closed(cx);
                    // The note is gone, and with it every offset the search had.
                    self.clear_search(cx);
                }
                // And every jump that named it.
                self.jumps.forget(&target);
                self.index_deleted(&target);
                self.refresh_link_names(cx);
                self.graph_note_removed(&target);
                self.refresh_graph(cx);
                self.tree_selection = None;
                self.refresh_entries(cx);
                self.say(format!("deleted {}", rel_text(&target)), cx);
            }
        }
        // The edit that just happened was ours, so the watcher's next pass has
        // nothing to report.
        self.resign();
        Ok(())
    }

    /// The row a rename or a delete acts on: the tree's selection if there is
    /// one, otherwise the note that is open.
    /// What a rename acts on. Ruling Q4: the caret decides — inside an embed, the
    /// note being edited is the one being renamed, and the propagation that rewrites
    /// the host's `![[…]]` line is already in place.
    fn rename_target(&self, cx: &mut Context<Self>) -> Result<PathBuf> {
        self.caret_note(cx)
            .or_else(|| self.prompt_target().ok())
            .context("nothing is selected")
    }

    fn prompt_target(&self) -> Result<PathBuf> {
        self.tree_selection
            .clone()
            .or_else(|| self.note())
            .context("nothing is selected")
    }

    /// Write the rewritten notes back and keep the open note in step.
    ///
    /// The open note is the one file Shale must not write blind: with unsaved
    /// edits, what is on disk is not what the user is looking at, and overwriting
    /// it would lose their work — so that note is left alone and named in the
    /// status. When the buffer is clean it is updated in place as well as on disk,
    /// or the next save would put the old links straight back.
    fn apply_rewrites(
        &mut self,
        moved: Option<(&Path, &Path)>,
        rewrites: &[Rewrite],
        cx: &mut Context<Self>,
    ) -> Result<RewriteReport> {
        let open = self.note();
        // Own text, not the transitive dot: an embed with unsaved edits says nothing
        // about whether *this* file is ahead of its copy on disk, and this is the check
        // that decides whether Shale may rewrite the note it has open.
        let dirty = self
            .editor()
            .is_some_and(|editor| editor.read_with(cx, |editor, _cx| editor.own_text_is_dirty()));
        let mut report = RewriteReport::default();
        for rewrite in rewrites {
            // The renamed note is the one file that has already moved.
            let path = match moved {
                Some((from, renamed)) if rewrite.path == from => renamed.to_path_buf(),
                _ => rewrite.path.clone(),
            };
            // `active` is still the old path here — the caller updates it after.
            let is_open = open.as_deref() == Some(rewrite.path.as_path());
            // Any tab or split in this window can hold the note, not only the one
            // with the caret, and a rewrite of a note the author has edited without
            // saving would throw that edit away. The buffer registry is what knows
            // the difference, so the check runs against it rather than against the
            // active editor alone.
            let dirty_here = is_open && dirty
                || self
                    .notes
                    .get(&rewrite.path)
                    .is_some_and(|note| note.dirty.get());
            if dirty_here {
                report.skipped.push(rewrite.path.clone());
                continue;
            }
            self.vault.write(&path, &rewrite.text)?;
            report.files += 1;
            report.changed += rewrite.changed;
            report.written.push(path.clone());
            if is_open {
                let text = rewrite.text.clone();
                // The note's buffer is shared, so the rewrite lands in it once and
                // every view of the note is told that what it measured against the
                // old text is stale (roadmap P4.17).
                if let Some(note) = self.notes.get(&path) {
                    note.buffer.borrow_mut().set_text(text.clone());
                    note.dirty.set(false);
                    self.reload_views_of(&path, cx);
                } else if let Some(editor) = self.editor().cloned() {
                    editor.update(cx, |editor, cx| editor.set_text(text.clone(), cx));
                }
                // The checker's document moved with the note: its URI is the
                // note's path, so the old one is dropped and the new one opened
                // with what the buffer now holds.
                self.checker_opened(&path, &text, cx);
            }
        }
        Ok(report)
    }

    /// `:rename-key [old] [new]`. Each half that is missing is asked for, in the
    /// order the reader would want to be asked: which key, then what to call it.
    /// Nothing is renamed until both names are in hand.
    pub(super) fn rename_key_command(
        &mut self,
        old: &str,
        new: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if old.is_empty() {
            // No key named: the census pane, in the mood where choosing a key means
            // renaming it.
            self.show_keys(true, window, cx);
            return;
        }
        if new.is_empty() {
            self.ask_key_rename(old, window, cx);
            return;
        }
        if let Err(err) = self.rename_property_key(old, new, cx) {
            self.say(format!("{err:#}"), cx);
        }
    }

    /// Ask for the new spelling of a key the reader has already chosen. The field
    /// starts with the old spelling, so the answer is a correction rather than a
    /// retyping, and the old key rides in the prompt's kind because the answer means
    /// nothing without it.
    pub(super) fn ask_key_rename(
        &mut self,
        old: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_prompt(
            prompt::PromptKind::RenameKey(old.to_string()),
            &format!("rename `{old}` to"),
            old,
            window,
            cx,
        );
    }

    /// Rename a property key across the vault (roadmap P4.3a M5) — the migration
    /// path a vocabulary cleanup asks for, rather than a tool deciding that the vault
    /// should be lowercase.
    ///
    /// What the rewrite touches is core's business and written there
    /// ([`shale_core::rename::rewrites_for_key_rename`]): the key span and nothing
    /// else, top-level lines of a note's block only, blocks that parse only. What is
    /// decided here is what a write across many notes has to get right on its own: a
    /// note open with unsaved edits is left alone rather than overwritten, every note
    /// that did change is told to the index (Shale's own writes are stamped out of the
    /// watcher's view, so nothing else would ever notice them), the link names are
    /// re-read in case the key was `aliases`, and the status line says how far the
    /// rename reached — including the notes it refused to touch.
    pub(super) fn rename_property_key(
        &mut self,
        old: &str,
        new: &str,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        // A key is written plain in the block, so a colon would end it where the
        // author did not mean it to, and a key with room around it is not the key the
        // census listed.
        if new.is_empty() || new.contains(':') || new.trim() != new {
            anyhow::bail!("`{new}` cannot be a property key: no empty, no `:`, no room around it");
        }
        if new == old {
            self.say(format!("`{old}` is already spelled `{new}`"), cx);
            return Ok(());
        }
        let rewrites =
            shale_core::rename::rewrites_for_key_rename(&self.vault, &self.entries, old, new)?;
        if rewrites.is_empty() {
            // Not "renamed nothing": the pane the reader came from lists only keys
            // some note writes, so an empty answer means the index is ahead of the
            // notes and the reader should know which of the two it is looking at.
            self.say(format!("no note writes `{old}`"), cx);
            return Ok(());
        }
        let report = self.apply_rewrites(None, &rewrites, cx)?;
        self.index_rewrites(&report.written);
        // The key being renamed may be `aliases`, and the resolver reads aliases out
        // of the index, so the link names are re-read exactly as a save re-reads them.
        self.refresh_link_names(cx);
        // A link-shaped value moves with the key that held it: the edges drawn from
        // `group: [[Test]]` now come from `party:`, and the graph is the surface that
        // would otherwise keep showing the old ones.
        self.refresh_graph(cx);
        let mut status = key_rename_status(old, new, &report);
        // Declarations live in `.shale/props.toml`, a hand-kept file with comments in
        // it, and this rename does not rewrite it: the promise made here is about
        // notes. So the line names what the rename just left pointing at nothing,
        // rather than letting the reader find out when the new key's kind quietly
        // stops being honoured.
        let decls = shale_core::db::Decls::read(&self.vault);
        if decls.declares(old) && !decls.declares(new) {
            status = format!("{status}; `.shale/props.toml` still declares `{old}`");
        }
        self.say(status, cx);
        Ok(())
    }

    /// Re-read the aliases the index holds and hand the editor the link names again.
    ///
    /// This belongs **after** an index write rather than with the disk refresh: the
    /// watcher lists the files before it updates the index, so an alias added a moment
    /// ago is not in the database yet when [`Workspace::refresh_entries`] looks. Reading
    /// it only there would leave `[[anon]]` painted as a dead end in a vault where the
    /// index, the graph and `gd` all reach the note — the colour disagreeing with the
    /// resolution, which is the one disagreement this feature cannot have.
    pub(super) fn refresh_link_names(&mut self, cx: &mut Context<Self>) {
        let aliases = self
            .db
            .as_ref()
            .and_then(|db| db.aliases().ok())
            .unwrap_or_default();
        if aliases == self.aliases {
            return;
        }
        self.aliases = aliases;
        self.known = known_names(&self.entries, &self.queries, &self.aliases);
        for tab in self.pane().tabs.iter() {
            let Some(editor) = tab.editor() else {
                continue;
            };
            editor.update(cx, |editor, cx| {
                editor.set_known_links(self.known.clone(), cx)
            });
        }
        cx.notify();
    }

    /// Re-read the vault's listings after the disk moved — one of Shale's own
    /// edits or a change made outside it — and hand the editor the new
    /// attachments and link targets so wiki-links resolve against what is on disk
    /// now.
    pub(super) fn refresh_entries(&mut self, cx: &mut Context<Self>) {
        self.entries = self.vault.list_markdown().unwrap_or_default();
        self.dirs = self.vault.list_dirs().unwrap_or_default();
        // The aliases come from the index rather than from the files, because the index
        // is what already read them, and it has been updated before this runs.
        self.aliases = self
            .db
            .as_ref()
            .and_then(|db| db.aliases().ok())
            .unwrap_or_default();
        self.known = known_names(&self.entries, &self.queries, &self.aliases);
        self.attachments = self.vault.list_attachments().unwrap_or_default();
        // The saved queries ride the same refresh, so a `.shq` an agent just wrote is a
        // tree row and a `:query` candidate without a restart.
        self.queries = self.vault.list_queries().unwrap_or_default();
        self.tree_files = super::tree_files(&self.entries, &self.queries);
        let vault = Rc::clone(&self.vault);
        // Every tab, not only the one on screen: a wiki-link in a tab nobody is
        // looking at has to resolve against the disk the same way the moment that
        // tab comes forward. The two fields are kept on the window for the same
        // reason, so a tab opened later starts with what is on disk now.
        for tab in self.pane().tabs.iter() {
            let Some(editor) = tab.editor() else {
                continue;
            };
            let path = tab.path().map(|p| p.to_path_buf());
            let notes = self.entries.clone();
            editor.update(cx, |editor, cx| {
                editor.set_known_links(self.known.clone(), cx);
                editor.set_vault(Rc::clone(&vault), self.attachments.clone(), cx);
                editor.set_notes(Rc::clone(&self.notes), cx);
                // The disk moved, so every embed body this note is showing is a guess
                // about another file (roadmap P4.0a). `set_embed_context` refreshes the
                // note list; `invalidate_embeds` drops the bodies the old list answered.
                editor.set_embed_context(path.as_deref(), notes, self.queries.clone(), cx);
                editor.invalidate_embeds(cx);
                // The index just moved, so a query note's table has to ask again:
                // handing over the same `Db` clears the cached answer rather than
                // opening a second door to read the index through.
                editor.set_query_db(self.db.clone(), cx);
            });
        }
        cx.notify();
    }

    pub(super) fn new_note(&mut self, _: &NewNote, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.target_dir();
        let label = if dir.as_os_str().is_empty() {
            "New note in the vault".to_string()
        } else {
            format!("New note in {}/", rel_text(&dir))
        };
        self.open_prompt(prompt::PromptKind::NewNote, &label, "", window, cx);
    }

    pub(super) fn new_folder(
        &mut self,
        _: &NewFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dir = self.target_dir();
        let label = if dir.as_os_str().is_empty() {
            "New folder in the vault".to_string()
        } else {
            format!("New folder in {}/", rel_text(&dir))
        };
        self.open_prompt(prompt::PromptKind::NewFolder, &label, "", window, cx);
    }

    pub(super) fn rename_note(
        &mut self,
        _: &RenameNote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Ruling Q4: the caret decides. Inside an embed, `:rename` renames the note
        // being edited, not the note it is embedded in — the same rule `:w` follows,
        // and the propagation that rewrites the host's `![[…]]` line is already here.
        let target = match self.rename_target(cx) {
            Ok(target) => target,
            Err(err) => {
                self.say(format!("{err:#}"), cx);
                cx.notify();
                return;
            }
        };
        // A note is renamed by its stem, so the `.md` the user never types does
        // not have to be edited.
        let initial = if self.is_dir(&target) {
            target
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            target
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        self.open_prompt(
            prompt::PromptKind::Rename,
            &format!("Rename {} to", rel_text(&target)),
            &initial,
            window,
            cx,
        );
    }

    pub(super) fn delete_note(
        &mut self,
        _: &DeleteNote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match self.prompt_target() {
            Ok(target) => target,
            Err(err) => {
                self.say(format!("{err:#}"), cx);
                cx.notify();
                return;
            }
        };
        self.open_prompt(
            prompt::PromptKind::Delete,
            &format!("Delete {}? Type yes to confirm.", rel_text(&target)),
            "",
            window,
            cx,
        );
    }
}

/// What the link rewrites that follow a rename did.
#[derive(Default)]
struct RewriteReport {
    /// Notes whose text was written back.
    files: usize,
    /// What changed inside them: links for a file rename, keys for a key rename.
    changed: usize,
    /// Where those writes went, so the index can be told. Vault-relative, and the
    /// renamed note appears under its new name.
    written: Vec<PathBuf>,
    /// Notes left alone because they are open somewhere in this window with
    /// unsaved edits. Named rather than counted: the author has to know *which*
    /// note still holds the old spelling.
    skipped: Vec<PathBuf>,
}

/// What a rename did. Links that could not be updated because the note holding
/// them has unsaved edits are named here rather than left for the user to find on
/// their next save.
fn rename_status(renamed: &Path, report: &RewriteReport) -> String {
    let mut status = format!("renamed to {}", rel_text(renamed));
    if report.changed > 0 {
        status = format!(
            "{status}; {} links in {} updated",
            report.changed, report.files
        );
    }
    if let Some(note) = skipped_note(report) {
        status = format!("{status}; {note}");
    }
    status
}

/// The half a rename says when it refused to touch something. Both renames have to
/// be able to say it, and a rewrite that quietly skipped a note would be the worst
/// kind of success.
fn skipped_note(report: &RewriteReport) -> Option<String> {
    if report.skipped.is_empty() {
        return None;
    }
    Some(format!(
        "{} open with unsaved edits, so {} left alone",
        report.skipped.len(),
        if report.skipped.len() == 1 {
            "it was"
        } else {
            "they were"
        }
    ))
}

/// What a key rename did: how far it reached, and what it refused to touch. The
/// voice is the file rename's, because from here they are the same act — one name
/// changed everywhere it was written.
fn key_rename_status(old: &str, new: &str, report: &RewriteReport) -> String {
    let mut status = format!(
        "`{old}` is now `{new}` in {} {}",
        report.files,
        if report.files == 1 { "note" } else { "notes" }
    );
    if report.changed > report.files {
        status = format!("{status}, {} lines", report.changed);
    }
    if let Some(note) = skipped_note(report) {
        status = format!("{status}; {note}");
    }
    status
}

/// The path `target` would have with `name` as its last component: the same
/// folder, a new name. A note keeps its `.md`, which the user does not type.
fn sibling_path(target: &Path, name: &str) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    let named = if target.extension().is_some_and(|e| e == "md") && !name.ends_with(".md") {
        format!("{name}.md")
    } else {
        name.to_string()
    };
    parent.join(named)
}
