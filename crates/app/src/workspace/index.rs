//! The vault index inside the window (roadmap P1.3, P1.4).
//!
//! The index is a **cache**: the notes are the source of truth, so nothing here is
//! allowed to fail the window. Every call is best-effort and reports itself in the
//! log, and a window with no index (turned off in the config, or a DB that would
//! not open) is a window with a few features missing rather than a broken one — the
//! same posture the spellchecker has.
//!
//! Two paths keep it current. [`Workspace::start_index`] runs the one startup
//! pass that catches what other programs did while Shale was closed; after that
//! the watcher's ticks and Shale's own save, create, rename and delete paths
//! report what changed, which is what keeps the steady state free: nothing is
//! re-read unless its stamp moved. A third keeps a copy rather than the cache:
//! [`Workspace::start_backups`], roadmap P1.6's daily rotation in
//! `.shale/backups/`.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use gpui::{Context, Window};
use shale_core::db::Provenance;
use shale_core::frontmatter::{KeyCensus, Kind, Prop};
use shale_core::watch::VaultChanges;

use super::open::{notice, PickerTarget};
use super::Workspace;
use crate::acts::{PropertyKeys, Props, WordCount};
use crate::picker::PickerItem;

impl Workspace {
    pub(super) fn flush_index(&self) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.flush() {
            tracing::warn!("the vault index was not checkpointed: {err:#}");
        }
    }

    /// Fold one set of disk changes into the index.
    pub(super) fn index_changes(&self, changes: &VaultChanges, provenance: Provenance) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.sync(&self.vault, changes, provenance) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Re-index the note Shale just wrote.
    pub(super) fn index_saved(&self, path: &Path) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.note_saved(&self.vault, path, Provenance::User) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Index the notes a rename rewrote to point at the new name. Shale's own
    /// writes are stamped out of the watcher's view, so nothing else would notice
    /// them.
    pub(super) fn index_rewrites(&self, paths: &[PathBuf]) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        if let Err(err) = db.notes_saved(&self.vault, paths, Provenance::Rename) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Index a note Shale just created.
    pub(super) fn index_created(&self, path: &Path) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.note_created(&self.vault, path, Provenance::User) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Drop a note Shale just deleted.
    pub(super) fn index_deleted(&self, path: &Path) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.note_deleted(path, Provenance::User) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// Move a note's rows, and record the move as one event.
    pub(super) fn index_renamed(&self, from: &Path, to: &Path) {
        let Some(db) = self.db.as_ref() else {
            return;
        };
        if let Err(err) = db.note_renamed(&self.vault, from, to) {
            tracing::warn!("the vault index fell behind: {err:#}");
        }
    }

    /// `:wordcount [folder]`: the first feature that reads the index rather than
    /// the notes, which is what makes it the proof that the spine works.
    ///
    /// The count is the index's, so it says which notes it counted. A note whose
    /// indexing failed is simply not in the number, and a vault with no index says
    /// so instead of inventing one.
    pub(super) fn wordcount(&mut self, folder: &str, cx: &mut Context<Self>) {
        let Some(db) = self.db.as_ref() else {
            self.say(
                "there is no vault index to count (index.enabled = false)",
                cx,
            );
            cx.notify();
            return;
        };
        let folder = folder.trim();
        match db.word_count_total(folder) {
            Ok((words, notes)) => {
                // One note is one note, and the bar is read out loud in the user's
                // head, so "1 notes" would sit wrong there.
                let word_word = if words == 1 { "word" } else { "words" };
                let note_word = if notes == 1 { "note" } else { "notes" };
                let scope = if folder.is_empty() {
                    format!("{words} {word_word} in {notes} {note_word}")
                } else {
                    format!("{words} {word_word} in {notes} {note_word} under {folder}/")
                };
                self.say(scope, cx);
            }
            Err(err) => {
                tracing::warn!("could not count the vault: {err:#}");
                self.say(format!("could not count the vault: {err:#}"), cx);
            }
        }
        cx.notify();
    }

    /// `:props`: what the index holds for the note the caret is in.
    ///
    /// Read from the table rather than re-parsed from the buffer, which is the point
    /// of the table: `:props`, a `.shq` body and the `prop:` operator all ask the same
    /// question, and only one of them has the note open. A vault with no index says so
    /// rather than reporting no properties (EDGE-25) — the same posture `:wordcount`
    /// and the spellchecker keep, because "zero" and "I cannot look" are different
    /// answers.
    pub(super) fn props(&mut self, cx: &mut Context<Self>) {
        let Some(db) = self.db.as_ref() else {
            self.say(
                "there is no vault index to read (index.enabled = false)",
                cx,
            );
            cx.notify();
            return;
        };
        // The note the caret is inside — which inside an entered embed is the embedded
        // note, whose properties are the ones on screen — and otherwise the tab's.
        let note = self
            .caret_note(cx)
            .or_else(|| self.pane().active_tab().path().map(|p| p.to_path_buf()));
        let Some(note) = note else {
            self.say("no note to read properties of", cx);
            cx.notify();
            return;
        };
        match db.note_props(&note) {
            Ok(rows) if rows.is_empty() => {
                self.say(format!("{} has no properties", note.display()), cx);
            }
            Ok(rows) => {
                // The census is vault-wide and the line is one note's, so the read is
                // narrowed to the keys this note actually writes.
                let keys: Vec<&str> = {
                    let mut seen: Vec<&str> = Vec::new();
                    for row in &rows {
                        if !seen.contains(&row.key.as_str()) {
                            seen.push(row.key.as_str());
                        }
                    }
                    seen
                };
                let census = match db.kinds_for_keys(&keys) {
                    Ok(census) => census,
                    Err(err) => {
                        tracing::warn!("could not read the kind census: {err:#}");
                        Default::default()
                    }
                };
                self.say(describe_props(&note, &rows, &census), cx);
            }
            Err(err) => {
                tracing::warn!("could not read the note's properties: {err:#}");
                self.say(format!("could not read the properties: {err:#}"), cx);
            }
        }
        cx.notify();
    }

    /// `:keys`: every property key the vault writes, how far each one reaches, and
    /// every kind its values come in (P4.3a M5).
    ///
    /// The census is the index's, not a re-parse of the notes: the same table `:props`
    /// reads for one note answers for the whole vault here. A key whose values
    /// disagree in kind is shown with **both** kinds rather than smoothed into one
    /// (ruling §8.4) — the disagreement is the thing the reader needs to see, and the
    /// pane is where it is shown rather than a warning on every save.
    pub(super) fn show_property_keys(
        &mut self,
        _: &PropertyKeys,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = "Every property key in the vault";
        let Some(db) = self.db.as_ref() else {
            // "No keys" and "I cannot look" are different answers (EDGE-25).
            self.open_picker(
                label,
                vec![notice(
                    "there is no vault index to read (index.enabled = false)",
                )],
                window,
                cx,
            );
            return;
        };
        let census = match db.key_census() {
            Ok(census) => census,
            Err(err) => {
                tracing::warn!("could not read the key census: {err:#}");
                self.open_picker(
                    label,
                    vec![notice(format!("the vault index could not answer: {err:#}"))],
                    window,
                    cx,
                );
                return;
            }
        };
        if census.is_empty() {
            self.open_picker(
                label,
                vec![notice("no note in this vault writes a property")],
                window,
                cx,
            );
            return;
        }
        let rows = census.iter().map(key_row).collect();
        self.open_picker(label, rows, window, cx);
    }

    /// The palette row's `:wordcount`: the whole vault, because a palette row has
    /// nowhere to type a folder.
    pub(super) fn show_wordcount(
        &mut self,
        _: &WordCount,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.wordcount("", cx);
    }

    /// The palette row's `:props`: the note the caret is in, which is the only note a
    /// palette row can mean.
    pub(super) fn show_props(&mut self, _: &Props, _window: &mut Window, cx: &mut Context<Self>) {
        self.props(cx);
    }
}

/// One status line for a note's properties: each key once, a list in brackets, the
/// kind the indexer worked out named, and a marker on a key the note wrote twice.
///
/// A summary rather than a table — the table is the vault-wide pane (M5). A key whose
/// values disagree in kind names both kinds, because that disagreement is the typo
/// worth *seeing* rather than something to smooth over.
///
/// `census` is the vault-wide kind set for each key (the kind census, ruling §1.3). A
/// key that is clean *in this note* but not across the vault gets said anyway: the
/// author looking at `cr=5 (number)` cannot see that another note writes `cr: low`, and
/// a `cr <= 5` query is about to be wrong about that note in a way nobody chose.
fn describe_props(note: &Path, rows: &[Prop], census: &BTreeMap<String, Vec<Kind>>) -> String {
    // First-appearance order, so the line reads in the order the block was written.
    let mut order: Vec<&str> = Vec::new();
    let mut groups: BTreeMap<&str, Vec<&Prop>> = BTreeMap::new();
    for row in rows {
        let key = row.key.as_str();
        if !order.contains(&key) {
            order.push(key);
        }
        groups.entry(key).or_default().push(row);
    }

    let mut parts: Vec<String> = Vec::new();
    for key in order {
        let group = &groups[key];
        let kinds: Vec<&str> = {
            let mut seen: Vec<&str> = Vec::new();
            for row in group {
                let kind = row.kind.as_str();
                if !seen.contains(&kind) {
                    seen.push(kind);
                }
            }
            seen
        };
        let listed = group.iter().any(|row| row.ordinal.is_some());
        // A duplicate key restarts its ordinals, so more rows than distinct ordinals
        // — or two scalars — is the signature the plan rules in (EDGE-8).
        let distinct: HashSet<Option<usize>> = group.iter().map(|row| row.ordinal).collect();
        let duplicate = distinct.len() < group.len();
        let values = if listed {
            format!(
                "[{}]",
                group
                    .iter()
                    .map(|row| shown(&row.value))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            group
                .iter()
                .map(|row| shown(&row.value))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        let kind = kinds.join("+");
        // Kinds the vault holds for this key that this note does not itself write.
        let elsewhere: Vec<&str> = census
            .get(key)
            .map(|vault| {
                vault
                    .iter()
                    .map(|k| k.as_str())
                    .filter(|k| !kinds.contains(k))
                    .collect()
            })
            .unwrap_or_default();
        let badge = if duplicate {
            format!("{kind}, written twice")
        } else if row_is_nested(group) {
            format!("{kind}, not read")
        } else {
            kind
        };
        // Said even when this note's own block is clean, because the query that reads
        // this key reads every note that writes it.
        let badge = if elsewhere.is_empty() {
            badge
        } else {
            format!("{badge}, vault also {}", elsewhere.join("/"))
        };
        parts.push(format!("{key}={values} ({badge})"));
    }

    // The status bar is one line, so a long block is cut at a key boundary rather
    // than mid-value: a half-shown value invites a guess, and a "+3 more" does not.
    const ROOM: usize = 180;
    let line = parts.join(", ");
    if line.len() <= ROOM {
        return format!("{}: {line}", note.display());
    }
    let mut used = 0;
    let mut shown_count = 0;
    for part in &parts {
        if used + part.len() + 2 > ROOM {
            break;
        }
        used += part.len() + 2;
        shown_count += 1;
    }
    format!(
        "{}: {} … +{} more",
        note.display(),
        parts[..shown_count].join(", "),
        parts.len() - shown_count
    )
}

/// Whether a key's value is structure Shale indexes but does not read.
fn row_is_nested(group: &[&Prop]) -> bool {
    group.iter().any(|row| row.kind == Kind::Nested)
}

/// One census key as the pane draws it: the key leads, and the dim text after it says
/// how far it reaches and what its values come as. The kinds are brought forward only
/// when they disagree — that is the one case where the detail is a warning rather than
/// a caption, and it is the reason the pane exists (ruling §8.4).
///
/// One producer for the row and for the test that reads it back, so rewording the
/// detail cannot quietly make the test answer a different question.
pub(crate) fn key_row(census: &KeyCensus) -> (PickerItem, PickerTarget) {
    let reach = format!(
        "{} {}",
        census.notes,
        if census.notes == 1 { "note" } else { "notes" }
    );
    // A key written twice in one note is one note and two values. The pane is
    // scanning a vocabulary, so notes is the number that leads; the difference is
    // only said when it is real.
    let reach = if census.values == census.notes {
        reach
    } else {
        format!("{reach}, {} values", census.values)
    };
    let kinds = census
        .kinds
        .iter()
        .map(|kind| kind.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let detail = format!("{reach} · {kinds}");
    let detail_ranges = if census.kinds.len() > 1 {
        vec![(detail.len() - kinds.len(), detail.len())]
    } else {
        Vec::new()
    };
    (
        PickerItem {
            label: census.key.clone().into(),
            detail: detail.into(),
            detail_ranges,
            search: census.key.clone().into(),
        },
        PickerTarget::PropKey(census.key.clone()),
    )
}

/// A value as it fits on one line: a nested block collapses, and anything long gets
/// cut, because the full text is one `:raw` away.
fn shown(value: &str) -> String {
    let flat = value.replace('\n', " ");
    let flat = flat.trim();
    if flat.chars().count() <= 28 {
        return flat.to_string();
    }
    let cut: String = flat.chars().take(27).collect();
    format!("{cut}…")
}
