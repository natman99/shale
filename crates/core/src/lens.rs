//! A lens: a saved query the graph wears (roadmap P4.16).
//!
//! A lens is a `.shq` file kept under `<vault>/.shale/lenses/`. Everything about that
//! location is a ruling, not a detail (EDGE-L1): `.shale/` is in the vault's ignore list,
//! so a lens is out of the file tree, out of quick-open, out of the index, and out of the
//! agent's note tools — while staying plain text the author can open in any editor and
//! edit by hand. The file is the authority: a lens is re-read when it is used, so an edit
//! made outside Shale takes effect on the next answer rather than needing an import.
//!
//! ## The answer's paths, not its table
//!
//! A query answers with a result set; a lens needs a set of note paths out of it. The
//! column is chosen by name where a name is available — `note` is what a saved search
//! compiles (`db::compile_sql` emits `SELECT path AS note`), `path` is what a hand-written
//! query most often calls it — and falls back to the first column, because the file is the
//! authority and a hand-written query may name its column anything. A cell that is not
//! text is not a path, and lands in the count of answers with no node rather than being
//! guessed at.
//!
//! ## A lens has no host
//!
//! A lens is not a note, so asking one always passes `host = None`: a query that declares
//! `relative = "current"` has nothing to be relative to and is refused in
//! [`crate::query_note`]'s own words, which is also EDGE-L13's rule that wearing a lens
//! does not make a graph tab a note.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow};

use crate::db::{Cell, Rows};
use crate::graph::index::GraphIndex;
use crate::query_note;

/// The folder a vault's lenses live in, under `.shale/`.
pub const DIR: &str = "lenses";

/// Where a vault keeps its lenses. The folder appears when the first lens is saved; its
/// absence is "no lenses", not an error.
pub fn dir(root: &Path) -> PathBuf {
    root.join(".shale").join(DIR)
}

/// The lens names in this vault, sorted: the stems of the `.shq` files in the lens
/// folder. A folder inside the lens folder is not a lens, and neither is a file with any
/// other extension or a hidden one.
pub fn list(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir(root)) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some(query_note::EXTENSION) {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                // A hidden file is not a lens: it is an editor's backup or swap file, and
                // listing it would offer a lens `read` then refuses by the same rule.
                if stem.starts_with('.') {
                    continue;
                }
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    names
}

/// Read one lens's text by name.
///
/// The name is a lens's own, not a path: a separator in it is refused rather than
/// resolved, because the one place a lens name comes from is a typed command and a name
/// that walks out of the lens folder would be a second, quieter way to reach any file in
/// the vault. A trailing `.shq` is dropped, so `:lens goblins.shq` names the same lens as
/// `:lens goblins`.
pub fn read(root: &Path, name: &str) -> Result<String> {
    let name = guard_name(name)?;
    let path = dir(root).join(format!("{name}.{}", query_note::EXTENSION));
    if !path.is_file() {
        return Err(anyhow!(
            "there is no lens named `{name}` in {}",
            dir(root).display()
        ));
    }
    std::fs::read_to_string(&path).map_err(|err| anyhow!("{name}: {err}"))
}

/// What asking a lens answers, in the three shapes every lens door gets (the same three
/// a saved query gets, with paths instead of rows).
#[derive(Debug, Clone, PartialEq)]
pub enum LensOutcome {
    /// The lens answered. `paths` are vault-relative, in the answer's own order; a path
    /// the graph has no node for is still in here, and [`unmatched`] is what counts it
    /// (EDGE-L5).
    Answered { name: String, paths: Vec<String> },
    /// An answer of nothing is still an answer: the graph draws nothing and the strip says
    /// so, rather than falling back to the whole vault.
    Empty { name: String },
    /// The lens could not be asked, in one sentence the reader can act on — the same
    /// wording [`query_note::ask`] gives the picker and an embedded table, so a lens
    /// cannot disagree with the editor about the same file.
    Refused { why: String },
}

/// One note or many, so every surface that counts an answer reads like a sentence.
/// The status line, the picker's rows and the filter strip all say the same count in the
/// same words (EDGE-L14: one producer, so no two of them can disagree).
pub fn notes_word(count: usize) -> &'static str {
    if count == 1 { "note" } else { "notes" }
}

/// The name a lens answers to, checked: one name, never a path, never a hidden file.
///
/// Separators are refused **before** the shape guard: `../secret` is a path trying to walk
/// out of the lens folder, and that is the sentence worth reading — the shape guard would
/// also stop it, by accident of where the dots sit, and a refusal that lands by accident is
/// a refusal the next edit can lose. A trailing `.shq` is forgiven, so `:lens goblins.shq`
/// names the same lens as `:lens goblins`.
fn guard_name(name: &str) -> Result<&str> {
    let name = name
        .trim()
        .strip_suffix(&format!(".{}", query_note::EXTENSION))
        .unwrap_or(name.trim());
    if name.contains('/') || name.contains('\\') {
        return Err(anyhow!("a lens name is one name, not a path: `{name}`"));
    }
    if name.is_empty() || name == "." || name == ".." || name.starts_with('.') {
        return Err(anyhow!("`{name}` is not a lens name"));
    }
    Ok(name)
}

/// Write a lens, and return the path written (M5's save door).
///
/// The folder appears with the first lens; its absence is "no lenses", not an error. An
/// existing lens is **not** overwritten: the file a reader wrote — and may have edited by
/// hand, since it is the authority (EDGE-L1) — outranks the picture someone just saved
/// over it. The name is checked by the same guard [`read`] uses, so a lens cannot be saved
/// somewhere the lens folder does not own.
pub fn write(root: &Path, name: &str, text: &str) -> Result<PathBuf> {
    let name = guard_name(name)?;
    let here = dir(root);
    std::fs::create_dir_all(&here)
        .with_context(|| format!("creating the lens folder {}", here.display()))?;
    let path = here.join(format!("{name}.{}", query_note::EXTENSION));
    if path.exists() {
        return Err(anyhow!(
            "{} already holds a lens — Shale will not overwrite it",
            path.display()
        ));
    }
    crate::atomic::write_file(&path, text)
        .with_context(|| format!("writing lens {}", path.display()))?;
    Ok(path)
}

/// What each lens file on disk looks like right now: name → stamp (EDGE-L7).
///
/// The vault's own signature walk skips `.shale/` on purpose — a lens is not a note
/// (EDGE-L1) — so a lens edited by hand in another editor would otherwise never be
/// noticed, and a lens worn by a graph would keep answering with a program that no longer
/// exists. This is the small answer: one directory, a handful of files, no recursive walk.
/// The same file filter as [`list`], so a swap file is not a lens here either.
pub fn stamps(root: &Path) -> BTreeMap<String, crate::watch::Stamp> {
    let mut out = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(dir(root)) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some(query_note::EXTENSION) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem.starts_with('.') {
                continue;
            }
            if let Ok(stamp) = crate::watch::Stamp::for_file(&entry) {
                out.insert(stem.to_string(), stamp);
            }
        }
    }
    out
}

/// Ask a lens what it answers, on a connection the surface already holds.
///
/// `host` is always `None` here: a lens is not a note (EDGE-L13). `stem` is the lens's
/// name, which the declaration's `name` may overrule, exactly as it does for a query note.
pub fn answer(text: &str, stem: &str, conn: &rusqlite::Connection) -> LensOutcome {
    match query_note::ask(text, stem, None, conn) {
        query_note::Outcome::Refused { why } => LensOutcome::Refused { why },
        query_note::Outcome::Empty { name } => LensOutcome::Empty { name },
        query_note::Outcome::Answered { name, rows } => LensOutcome::Answered {
            name,
            paths: paths_from(&rows),
        },
    }
}

/// The note paths an answer carries, in its own order.
///
/// The column is chosen by name where the answer has one (`note`, then `path`) and falls
/// back to the first column. A cell that is not text — a count, a NULL — is not a path and
/// is left out here so that [`unmatched`] can count it against the graph rather than the
/// reader wondering where a row went.
pub fn paths_from(rows: &Rows) -> Vec<String> {
    let column = rows
        .column("note")
        .or_else(|| rows.column("path"))
        .unwrap_or(0);
    rows.rows
        .iter()
        .filter_map(|row| match row.get(column) {
            Some(Cell::Text(text)) if !text.trim().is_empty() => Some(text.trim().to_string()),
            _ => None,
        })
        .collect()
}

/// The paths an answer named that this graph has no node for (EDGE-L5).
///
/// The graph decides, not the answer: a `.shq` can name a path the current node set
/// excludes — a note the index has not folded in yet, an odd column, a file that was
/// deleted after the query was written. Those are counted and said, never dropped in
/// silence, because a picture that quietly disagrees with the answer is the failure this
/// item exists to avoid.
pub fn unmatched(index: &GraphIndex, paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| index.node_for_path(path).is_none())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::vault::Vault;

    /// A vault with a lens folder and an indexed note set, small enough to read.
    fn fixture(lenses: &[(&str, &str)]) -> anyhow::Result<(tempfile::TempDir, Vault, Db)> {
        // Not named `dir`: the module's own `dir` is what the tests call to find the
        // lens folder, and a local of the same name would hide it.
        let tmp = tempfile::tempdir()?;
        let root = tmp.path();
        std::fs::create_dir_all(root.join("notes"))?;
        std::fs::write(root.join("root.md"), "to [[notes/deep]] and [[gone]]. #top")?;
        std::fs::write(root.join("notes/deep.md"), "deep, and #top too")?;
        std::fs::write(root.join("loose.md"), "nothing links me")?;
        let vault = Vault::open(root)?;
        let db = Db::open(root)?;
        db.reconcile(&vault)?;
        let lens_dir = dir(root);
        if !lenses.is_empty() {
            std::fs::create_dir_all(&lens_dir)?;
        }
        for (name, text) in lenses {
            std::fs::write(
                lens_dir.join(format!("{name}.{}", query_note::EXTENSION)),
                text,
            )?;
        }
        Ok((tmp, vault, db))
    }

    /// The lens folder is `.shale/lenses`, which the vault ignores: a lens is not a note,
    /// is not in the tree, and is not in the index (EDGE-L1).
    #[test]
    fn a_lens_lives_where_the_vault_will_not_look() -> anyhow::Result<()> {
        let (_dir, vault, _db) = fixture(&[("all", "SELECT path AS note FROM notes")])?;
        assert!(dir(vault.root()).ends_with(".shale/lenses"));
        let listed = vault.list_markdown()?;
        assert!(
            listed.iter().all(|entry| !rel(&entry.path)),
            "the vault's own listing never reaches into .shale: {listed:?}"
        );
        Ok(())
    }

    fn rel(path: &std::path::Path) -> bool {
        path.components().any(|c| c.as_os_str() == ".shale")
    }

    /// Listing is by stem, sorted, and only `.shq` files: a stray folder or a note left in
    /// the lens folder is not a lens.
    #[test]
    fn listing_sorts_stems_and_skips_anything_else() -> anyhow::Result<()> {
        let (_dir, vault, _db) = fixture(&[("zulu", "SELECT 1"), ("alpha", "SELECT 1")])?;
        let lens_dir = dir(vault.root());
        std::fs::write(lens_dir.join("notes.md"), "# not a lens")?;
        std::fs::create_dir_all(lens_dir.join("sub"))?;
        std::fs::write(lens_dir.join("sub/inner.shq"), "SELECT 1")?;
        std::fs::write(lens_dir.join(".hidden.shq"), "SELECT 1")?;
        assert_eq!(
            list(vault.root()),
            vec!["alpha".to_string(), "zulu".to_string()]
        );
        Ok(())
    }

    /// A vault that has never saved a lens has no lenses rather than an error, because the
    /// folder appears with the first one.
    #[test]
    fn a_vault_with_no_lens_folder_has_no_lenses() -> anyhow::Result<()> {
        let (_dir, vault, _db) = fixture(&[])?;
        assert!(list(vault.root()).is_empty(), "no folder, no lenses");
        assert!(
            read(vault.root(), "any").is_err(),
            "asking for one still says there is none"
        );
        Ok(())
    }

    /// A lens name is one name. A separator is refused rather than resolved, so a typed
    /// `:lens` cannot be a quiet way to read any file in the vault.
    #[test]
    fn a_lens_name_is_not_a_path() -> anyhow::Result<()> {
        let (_dir, vault, _db) = fixture(&[("all", "SELECT 1")])?;
        // A separator is refused as a path, in the sentence that says so — not by the
        // folder happening not to hold that file, which is a different refusal.
        for bad in ["../secret", "notes/deep", "a\\b"] {
            let err = read(vault.root(), bad).unwrap_err().to_string();
            assert!(
                err.contains("not a path"),
                "`{bad}` should be refused as a path, got: {err}"
            );
        }
        // A name that cannot name a file at all is refused as a name.
        for bad in ["..", ".", ".hidden", ""] {
            let err = read(vault.root(), bad).unwrap_err().to_string();
            assert!(
                err.contains("not a lens name"),
                "`{bad}` should be refused as a name, got: {err}"
            );
        }
        Ok(())
    }

    /// A missing lens says so by name, which is what `:lens <name>` needs to be able to
    /// report without changing anything.
    #[test]
    fn a_missing_lens_is_named() -> anyhow::Result<()> {
        let (_dir, vault, _db) = fixture(&[("all", "SELECT 1")])?;
        let err = read(vault.root(), "goblins").unwrap_err().to_string();
        assert!(err.contains("goblins"), "{err}");
        assert!(
            err.contains("no lens named"),
            "the sentence should say there is no such lens, not what the filesystem said: {err}"
        );
        Ok(())
    }

    /// The name may be typed with its extension and means the same lens.
    #[test]
    fn a_lens_name_may_spell_its_extension() -> anyhow::Result<()> {
        let (_dir, vault, _db) = fixture(&[("all", "SELECT path AS note FROM notes")])?;
        let text = read(vault.root(), "all.shq")?;
        assert!(text.contains("SELECT"), "{text}");
        Ok(())
    }

    /// A saved search compiles to `SELECT path AS note`, and that is the column a lens
    /// reads; the answer's order is kept.
    #[test]
    fn a_lens_reads_the_note_column() -> anyhow::Result<()> {
        let (_dir, vault, db) =
            fixture(&[("all", "SELECT path AS note FROM notes ORDER BY path")])?;
        let text = read(vault.root(), "all")?;
        let conn = db.read()?;
        let outcome = answer(&text, "all", &conn);
        match outcome {
            LensOutcome::Answered { name, paths } => {
                assert_eq!(name, "all");
                assert_eq!(
                    paths,
                    vec![
                        "loose.md".to_string(),
                        "notes/deep.md".to_string(),
                        "root.md".to_string()
                    ]
                );
            }
            other => panic!("expected an answer, got {other:?}"),
        }
        Ok(())
    }

    /// A hand-written query may call the column anything, so the first column is the
    /// fallback rather than a refusal naming a column the author never meant to write.
    #[test]
    fn a_lens_falls_back_to_the_first_column() -> anyhow::Result<()> {
        let (_dir, vault, db) =
            fixture(&[("odd", "SELECT path AS whatever FROM notes ORDER BY path")])?;
        let text = read(vault.root(), "odd")?;
        let conn = db.read()?;
        match answer(&text, "odd", &conn) {
            LensOutcome::Answered { paths, .. } => {
                assert_eq!(paths.len(), 3, "the first column is the path column");
            }
            other => panic!("expected an answer, got {other:?}"),
        }
        Ok(())
    }

    /// A saved search puts its path column first, but a hand-written query that reports a
    /// count beside the note puts it second — and the name is still what finds it. Without
    /// this, "prefer the named column" and "take the first column" are the same code.
    #[test]
    fn a_lens_reads_the_note_column_wherever_it_sits() -> anyhow::Result<()> {
        let (_dir, vault, db) = fixture(&[(
            "wide",
            "SELECT 1 AS links, path AS note FROM notes ORDER BY path",
        )])?;
        let text = read(vault.root(), "wide")?;
        let conn = db.read()?;
        match answer(&text, "wide", &conn) {
            LensOutcome::Answered { paths, .. } => {
                assert_eq!(paths.len(), 3, "the named column, not the first one");
                assert!(
                    paths.iter().all(|p| p.ends_with(".md")),
                    "no row contributed its `links` count: {paths:?}"
                );
            }
            other => panic!("expected an answer, got {other:?}"),
        }
        Ok(())
    }

    /// An answer of nothing is an answer: the lens is empty, not broken, and the graph will
    /// draw nothing rather than the whole vault.
    #[test]
    fn a_lens_answering_nothing_is_empty_not_broken() -> anyhow::Result<()> {
        let (_dir, vault, db) = fixture(&[(
            "none",
            "SELECT path AS note FROM notes WHERE path = 'no-such-note.md'",
        )])?;
        let text = read(vault.root(), "none")?;
        let conn = db.read()?;
        assert_eq!(
            answer(&text, "none", &conn),
            LensOutcome::Empty {
                name: "none".to_string()
            }
        );
        Ok(())
    }

    /// A lens is not a note, so a query that needs a host is refused — the same sentence
    /// the picker gives for the same file (EDGE-L13).
    #[test]
    fn a_relative_query_refuses_to_be_a_lens() -> anyhow::Result<()> {
        let (_dir, vault, db) = fixture(&[(
            "relative",
            "+++\nrelative = \"current\"\n+++\nSELECT path AS note FROM notes WHERE path != :current",
        )])?;
        let text = read(vault.root(), "relative")?;
        let conn = db.read()?;
        match answer(&text, "relative", &conn) {
            LensOutcome::Refused { why } => {
                assert!(why.contains("relative"), "{why}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        Ok(())
    }

    /// A lens file edited into a broken query is refused with the database's own sentence
    /// in front of the reader's own text (EDGE-2's rule, reached through `ask`).
    #[test]
    fn a_broken_lens_is_refused_not_empty() -> anyhow::Result<()> {
        let (_dir, vault, db) = fixture(&[("broken", "SELECT path AS note FROM nope")])?;
        let text = read(vault.root(), "broken")?;
        let conn = db.read()?;
        match answer(&text, "broken", &conn) {
            LensOutcome::Refused { why } => {
                assert!(why.contains("could not be answered"), "{why}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        Ok(())
    }

    /// EDGE-L5: an answer can name a path the graph has no node for. The graph decides,
    /// and the ones it cannot name are listed rather than dropped.
    #[test]
    fn an_answer_can_name_a_path_the_graph_has_no_node_for() -> anyhow::Result<()> {
        let (_dir, vault, db) = fixture(&[(
            "mixed",
            "SELECT path AS note FROM notes UNION ALL SELECT 'deleted.md' ORDER BY note",
        )])?;
        let text = read(vault.root(), "mixed")?;
        let conn = db.read()?;
        let paths = match answer(&text, "mixed", &conn) {
            LensOutcome::Answered { paths, .. } => paths,
            other => panic!("expected an answer, got {other:?}"),
        };
        let index = crate::graph::index::GraphIndex::build(
            &vault,
            &vault.list_markdown()?,
            &vault.list_attachments()?,
            &vault.list_queries()?,
            &[],
        )?;
        assert_eq!(unmatched(&index, &paths), vec!["deleted.md".to_string()]);
        Ok(())
    }

    /// A cell that is not text is not a path. It is left out of the paths and shows up as
    /// unmatched, so a query that answers with counts cannot silently become a lens of
    /// nothing.
    #[test]
    fn a_column_of_numbers_is_not_a_set_of_paths() -> anyhow::Result<()> {
        let rows = Rows {
            columns: vec!["note".to_string()],
            rows: vec![
                vec![Cell::Text("root.md".to_string())],
                vec![Cell::Integer(7)],
                vec![Cell::Null],
            ],
        };
        assert_eq!(paths_from(&rows), vec!["root.md".to_string()]);
        Ok(())
    }
}
