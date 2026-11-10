//! The generated fixture vaults, checked against the machinery they exist to
//! exercise.
//!
//! `examples/graph/` is git-ignored — `tools/graph_vaults.py` writes it — so this
//! says so and passes on a checkout where the fixtures were never generated,
//! rather than failing a clean tree. What it is for: the fixtures carry
//! frontmatter and saved queries, and both are only worth having if the indexer
//! reads them and the runner answers them at fixture scale. A fixture that stops
//! indexing its properties is a fixture that quietly stops testing them.

use shale_core::db::Db;
use shale_core::query_note::QueryNote;
use shale_core::vault::Vault;

/// One fixture's root, or `None` when it has not been generated. The folder is the
/// test for that: not one of the files this test then insists on, which would turn
/// a missing fixture into a passing test that checked nothing.
fn fixture(name: &str) -> Option<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/graph")
        .join(name);
    root.is_dir().then_some(root)
}

#[test]
fn a_fixture_vault_indexes_its_properties_and_answers_its_queries() {
    let Some(root) = fixture("100") else {
        eprintln!(
            "examples/graph/100 is not generated \
             (python3 tools/graph_vaults.py 100); nothing to check"
        );
        return;
    };
    let vault = Vault::open(&root).expect("the fixture opens as a vault");
    let db = Db::open(&root).expect("the index opens");
    // Whether this run had any work to do depends on whether the index was already
    // warm, so the claim is about what the index *says*, not about what this run
    // wrote. A fixture whose properties never reach the census has stopped testing
    // the properties half of the app, whatever its files look like.
    db.reconcile(&vault).expect("the fixture indexes");

    // The properties block is worth having only if the index reads it. These four
    // are what the generator writes; the census is what the reader sees.
    let census = db.key_census().expect("the census");
    let keys: Vec<&str> = census.iter().map(|row| row.key.as_str()).collect();
    println!("census: {census:?}");
    for key in ["status", "effort", "created", "review"] {
        assert!(
            keys.contains(&key),
            "the fixture's `{key}` never reached the census: {keys:?}"
        );
    }

    for name in ["Busiest notes", "Evergreen notes"] {
        let path = root.join("queries").join(format!("{name}.shq"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        let note = QueryNote::parse(&text).unwrap_or_else(|err| panic!("{name}: {err}"));
        let rows = db
            .run(&note.sql, &[])
            .unwrap_or_else(|err| panic!("{name}: {err}"));
        assert!(!rows.columns.is_empty(), "{name} answered no columns");
        assert!(
            !rows.rows.is_empty(),
            "{name} answered no rows against a fixture that has notes"
        );
        println!(
            "{name}: {} rows × {} columns {:?}",
            rows.rows.len(),
            rows.columns.len(),
            rows.columns
        );
    }
}
