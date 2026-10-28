//! The one-line prompt and the vault edits behind its answers.
//!
//! A prompt is the only place a name is typed, so it is also the only place a
//! note or folder can be named: the answer is what touches the disk, through the
//! checked edits in [`shale_core::Vault`].

use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context as _, Result};
use gpui::{prelude::*, Action, Context, Entity, Focusable, Window};
use shale_core::rename::LinkRewrite;

use crate::acts::*;
use crate::ex;
use crate::prompt;

use super::note_keys;
use super::Workspace;

impl Workspace {
    /// Open the one-line prompt. This is the only place a name is typed, so it is
    /// also the only place a note or folder can be named.
    fn open_prompt(
        &mut self,
        kind: prompt::PromptKind,
        label: &str,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let family = self.editor.read(cx).font_family();
        let size = self.editor.read(cx).font_size();
        let palette = Rc::clone(&self.palette);
        let prompt =
            cx.new(|cx| prompt::Prompt::new(kind, label, initial, palette, family, size, cx));
        // A prompt owns the keyboard, so a popup cannot stay up beside it.
        self.close_popup(window, cx);
        self._prompt_events = Some(cx.subscribe_in(
            &prompt,
            window,
            |ws, prompt, event, window, cx| match event {
                prompt::PromptEvent::Submit(text) => {
                    ws.answer_prompt(prompt, text.as_str(), window, cx)
                }
                prompt::PromptEvent::Cancel => ws.close_prompt(window, cx),
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
    fn close_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompt.is_none() {
            return;
        }
        self.prompt = None;
        let focus = match self.prompt_return_focus.take() {
            Some(focus) => focus,
            None => self.editor.read(cx).focus_handle(cx),
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
        let answer = answer.trim();
        // A command line is a word, not a name for something on disk, and it is
        // finished either way: an unknown command is an answer, not something to
        // correct in place.
        if kind == prompt::PromptKind::Command {
            self.answer_command(answer, window, cx);
            return;
        }
        if kind == prompt::PromptKind::Delete && !answer.eq_ignore_ascii_case("yes") {
            self.status = "delete cancelled".into();
            self.close_prompt(window, cx);
            return;
        }
        match self.apply_prompt(kind, answer, cx) {
            Ok(()) => self.close_prompt(window, cx),
            Err(err) => {
                tracing::warn!("the prompt was refused: {err:#}");
                self.status = format!("{err:#}").into();
                cx.notify();
            }
        }
    }

    /// What a command line meant. The line closes before the command runs, so the
    /// keyboard is back where it was and a command that opens a picker takes it
    /// from there, exactly as if the command's key had been typed.
    fn answer_command(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) {
        let command = ex::parse(line);
        self.close_prompt(window, cx);
        match command {
            ex::Ex::Empty => {}
            ex::Ex::Write => self.save(&Save, window, cx),
            ex::Ex::Quit => self.quit(&Quit, window, cx),
            ex::Ex::WriteQuit => {
                self.save(&Save, window, cx);
                self.quit(&Quit, window, cx);
            }
            ex::Ex::Open => window.dispatch_action(QuickOpen.boxed_clone(), cx),
            ex::Ex::Checker(on) => self.set_checker(on, cx),
            ex::Ex::ThemePicker => window.dispatch_action(ChangeTheme.boxed_clone(), cx),
            ex::Ex::Theme(name) => match self.set_theme(&name, cx) {
                Ok(()) => self.status = format!("theme: {name}").into(),
                Err(err) => self.status = format!("{err:#}").into(),
            },
            ex::Ex::Unknown(word) => self.status = format!("unknown command: {word}").into(),
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
                self.tree_selection = Some(created.clone());
                self.open_note(&created, cx);
                self.status = format!("created {}", created.display()).into();
            }
            prompt::PromptKind::NewFolder => {
                let dir = self.target_dir().join(answer);
                self.vault.create_dir(&dir)?;
                self.refresh_entries(cx);
                self.expanded.insert(dir.clone());
                self.tree_selection = Some(dir.clone());
                self.status = format!("created {}/", dir.display()).into();
            }
            prompt::PromptKind::Rename => {
                let from = self.prompt_target()?;
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
                let report = self.apply_rewrites(&from, &renamed, &rewrites, cx)?;
                self.refresh_entries(cx);
                if self.active.as_deref() == Some(from.as_path()) {
                    self.active = Some(renamed.clone());
                }
                self.tree_selection = Some(renamed.clone());
                self.status = rename_status(&renamed, &report).into();
            }
            // A command line is not a name for something on disk, and
            // `answer_prompt` sends it off to `answer_command` before this is
            // reached, so there is nothing to do here.
            prompt::PromptKind::Command => {}
            prompt::PromptKind::Delete => {
                let target = self.prompt_target()?;
                if self.is_dir(&target) {
                    self.vault.delete_dir(&target)?;
                } else {
                    self.vault.delete(&target)?;
                }
                if self.active.as_deref() == Some(target.as_path()) {
                    self.active = None;
                    self.editor
                        .update(cx, |editor, cx| editor.set_text(String::new(), cx));
                    self.checker_closed(cx);
                }
                self.tree_selection = None;
                self.refresh_entries(cx);
                self.status = format!("deleted {}", target.display()).into();
            }
        }
        // The edit that just happened was ours, so the watcher's next pass has
        // nothing to report.
        self.resign();
        Ok(())
    }

    /// The row a rename or a delete acts on: the tree's selection if there is
    /// one, otherwise the note that is open.
    fn prompt_target(&self) -> Result<PathBuf> {
        self.tree_selection
            .clone()
            .or_else(|| self.active.clone())
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
        from: &Path,
        renamed: &Path,
        rewrites: &[LinkRewrite],
        cx: &mut Context<Self>,
    ) -> Result<RewriteReport> {
        let open = self.active.clone();
        let dirty = self.editor.read_with(cx, |editor, _cx| editor.is_dirty());
        let mut report = RewriteReport::default();
        for rewrite in rewrites {
            // The renamed note is the one file that has already moved.
            let path = if rewrite.path == from {
                renamed.to_path_buf()
            } else {
                rewrite.path.clone()
            };
            // `active` is still the old path here — the caller updates it after.
            let is_open = open.as_deref() == Some(rewrite.path.as_path());
            if is_open && dirty {
                report.skipped_open = true;
                continue;
            }
            self.vault.write(&path, &rewrite.text)?;
            report.files += 1;
            report.links += rewrite.changed;
            if is_open {
                let text = rewrite.text.clone();
                self.editor
                    .update(cx, |editor, cx| editor.set_text(text.clone(), cx));
                // The checker's document moved with the note: its URI is the
                // note's path, so the old one is dropped and the new one opened
                // with what the buffer now holds.
                self.checker_opened(&path, &text, cx);
            }
        }
        Ok(report)
    }

    /// Re-read the vault's listings after the disk moved — one of Shale's own
    /// edits or a change made outside it — and hand the editor the new
    /// attachments and link targets so wiki-links resolve against what is on disk
    /// now.
    pub(super) fn refresh_entries(&mut self, cx: &mut Context<Self>) {
        self.entries = self.vault.list_markdown().unwrap_or_default();
        self.dirs = self.vault.list_dirs().unwrap_or_default();
        let known = note_keys(&self.entries);
        let attachments = self.vault.list_attachments().unwrap_or_default();
        let vault = Rc::clone(&self.vault);
        self.editor.update(cx, |editor, cx| {
            editor.set_known_links(known, cx);
            editor.set_vault(vault, attachments, cx);
        });
        cx.notify();
    }

    pub(super) fn new_note(&mut self, _: &NewNote, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.target_dir();
        let label = if dir.as_os_str().is_empty() {
            "New note in the vault".to_string()
        } else {
            format!("New note in {}/", dir.display())
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
            format!("New folder in {}/", dir.display())
        };
        self.open_prompt(prompt::PromptKind::NewFolder, &label, "", window, cx);
    }

    pub(super) fn rename_note(
        &mut self,
        _: &RenameNote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match self.prompt_target() {
            Ok(target) => target,
            Err(err) => {
                self.status = format!("{err:#}").into();
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
            &format!("Rename {} to", target.display()),
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
                self.status = format!("{err:#}").into();
                cx.notify();
                return;
            }
        };
        self.open_prompt(
            prompt::PromptKind::Delete,
            &format!("Delete {}? Type yes to confirm.", target.display()),
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
    /// Links rewritten across them.
    links: usize,
    /// The open note holds unsaved edits, so its links were left alone.
    skipped_open: bool,
}

/// What a rename did. Links that could not be updated because the note holding
/// them has unsaved edits are named here rather than left for the user to find on
/// their next save.
fn rename_status(renamed: &Path, report: &RewriteReport) -> String {
    let mut status = format!("renamed to {}", renamed.display());
    if report.links > 0 {
        status = format!(
            "{status}; {} links in {} updated",
            report.links, report.files
        );
    }
    if report.skipped_open {
        status = format!("{status}; the open note has unsaved edits, so its links were left alone");
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
