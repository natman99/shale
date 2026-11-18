//! Breakdown benches for the hot paths that have no graph: the editor's
//! per-paint styling, the vault-wide text search, the picker's fuzzy
//! ranking, and the block map. Everything here is a path a keystroke can
//! take; the graph benches cover the rest.
//!
//! Run them:
//! `cargo test --release -p shale-core bench_paths -- --ignored --nocapture`

#[cfg(test)]
mod benches {
    use std::time::Instant;

    /// A deterministic markdown note with every construct the styler walks:
    /// headings, emphasis, wikilinks, tags, highlights, code, and lists.
    /// `words` is approximate — the note is built from a repeating block.
    fn synthetic_note(words: usize) -> String {
        let block = "## A section heading with some words\n\n\
                     The quick brown [[Some Note]] jumps over the #lazydog, and ==highlights== \
                     sit beside `inline code` and **bold words** in a paragraph that keeps going.\n\n\
                     - a list item with [[Another Note]] inside it\n\
                     - [ ] a task with *emphasis* and a link [to a page](elsewhere.md)\n\n\
                     ```rust\nlet x = 1 + 2;\nprintln!(\"{x}\");\n```\n\n\
                     A paragraph of ordinary prose to give the styler plain runs to walk \
                     through, with numbers 1234 and symbols &*() mixed in for good measure.\n\n";
        let mut out = String::new();
        while out.split_whitespace().count() < words {
            out.push_str(block);
        }
        out
    }

    /// Where the editor's per-paint time goes as the note grows:
    /// `style_spans` runs over the whole buffer text on every frame the
    /// buffer is dirty (`editor/mod.rs`), so this number is typing latency.
    #[test]
    #[ignore]
    fn bench_style_spans() {
        for words in [500usize, 2000, 8000] {
            let text = synthetic_note(words);
            let reps = 200;
            let t0 = Instant::now();
            let mut total_spans = 0usize;
            for _ in 0..reps {
                total_spans += crate::mdstyle::style_spans(&text).len();
            }
            println!(
                "style_spans  words={words:>5}  {:>8.2?}/call  (spans={})",
                t0.elapsed() / reps,
                total_spans / reps as usize
            );
        }
    }

    /// The block map over the same notes — the other whole-text pass the
    /// editor takes (block anchors, `:id`, block-aware edits).
    #[test]
    #[ignore]
    fn bench_block_map() {
        for words in [500usize, 2000, 8000] {
            let text = synthetic_note(words);
            let reps = 200;
            let t0 = Instant::now();
            let mut total = 0usize;
            for _ in 0..reps {
                total += crate::mdstyle::block_map(&text).blocks.len();
            }
            println!(
                "block_map    words={words:>5}  {:>8.2?}/call  (blocks={})",
                t0.elapsed() / reps,
                total / reps as usize
            );
        }
    }

    /// Vault-wide search over a synthetic 1k-note vault: every query reads
    /// every note from disk and scans it line by line. This is the picker's
    /// `:search` cost per keystroke (debounce aside), so it is measured
    /// with a warm page cache — the disk is not the point, the re-read is.
    #[test]
    #[ignore]
    fn bench_vault_search() {
        use crate::{Vault, search::SearchOptions};
        let dir = tempfile::tempdir().unwrap();
        let notes = 1000usize;
        for i in 0..notes {
            let text = synthetic_note(120);
            let text = text.replace("ordinary prose", &format!("prose seed-{i}"));
            std::fs::write(dir.path().join(format!("note-{i:04}.md")), text).unwrap();
        }
        let vault = Vault::open(dir.path()).unwrap();
        let entries = vault.list_markdown().unwrap();
        let opts = SearchOptions::default();

        for query in ["seed-742", "quick brown"] {
            let reps = 20;
            let t0 = Instant::now();
            let mut hits = 0usize;
            for _ in 0..reps {
                hits = crate::search::search(&vault, &entries, query, &opts)
                    .unwrap()
                    .len();
            }
            println!(
                "search       notes={notes}  q={query:<12} {:>8.2?}/query  (hits={hits})",
                t0.elapsed() / reps
            );
        }

        // The same queries through the note cache: the first query pays
        // the reads, every later keystroke pays only the stats and scan.
        let mut cache = crate::search::NoteCache::default();
        for query in ["seed-742", "quick brown"] {
            let t0 = Instant::now();
            let cold = cache.search(&vault, &entries, query, &opts).unwrap().len();
            let cold_t = t0.elapsed();
            let reps = 50;
            let t0 = Instant::now();
            let mut hits = 0usize;
            for _ in 0..reps {
                hits = cache.search(&vault, &entries, query, &opts).unwrap().len();
            }
            println!(
                "search cache notes={notes}  q={query:<12} cold {:>8.2?}  warm {:>8.2?}/query  (hits={hits})",
                cold_t,
                t0.elapsed() / reps
            );
            assert_eq!(cold, hits);
        }
    }

    /// The picker's fuzzy ranking over note-name lists of realistic and
    /// large size — this runs per keystroke in every picker.
    #[test]
    #[ignore]
    fn bench_fuzzy_ranked() {
        for n in [1000usize, 10_000] {
            let names: Vec<String> = (0..n)
                .map(|i| format!("areas/reading/{i:05}-the-quick-brown-note.md"))
                .collect();
            let reps = 200;
            let t0 = Instant::now();
            let mut got = 0usize;
            for _ in 0..reps {
                got = crate::fuzzy::ranked("qkbrown", &names, 12, |s| s.as_str()).len();
            }
            println!(
                "fuzzy ranked n={n:>6}  {:>8.2?}/call  (rows={got})",
                t0.elapsed() / reps
            );
        }
    }
}
