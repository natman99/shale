//! The vault index inside the window: what `:wordcount` reports, and how the index
//! keeps up with notes that move under it.
//!
//! These are the first tests that read the index rather than the notes, which makes
//! them the proof that the spine works end to end: the window opens a DB, the
//! startup pass fills it, the watcher and the save paths keep it current, and a
//! command reads a number out of it. The rows themselves — tags, links, events —
//! are `shale_core::db`'s own tests to make.
//!
//! Every other test helper turns the index off, so these open it deliberately with
//! [`indexed_vault`].

use super::*;

/// Type a command line and press enter.
fn type_command(line: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape :");
    cx.simulate_input(line);
    cx.simulate_keystrokes("enter");
}

/// Open one note in the window, so a command that reads the caret's note has one.
fn open_note(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext, name: &str) {
    window
        .update(cx, |ws, _window, cx| ws.open_note(&PathBuf::from(name), cx))
        .unwrap();
}

/// The status line, where the count is reported.
fn status(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> String {
    window.update(cx, |ws, _window, _cx| ws.status()).unwrap()
}

/// Six words, and three more in a folder, so a scoped count differs from the whole.
const WELCOME: &str = "# Welcome\n\nA note about the plan.\n";
const IDEA: &str = "three words here\n";

/// The startup pass reads the vault on a background thread, so a window that has an
/// index has to be waited for before it is asked what the index says.
#[gpui::test]
fn the_command_line_counts_the_vault_from_the_index(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME), ("notes/Idea.md", IDEA)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("wordcount", cx);
    assert_eq!(
        status(&window, cx),
        "9 words in 2 notes",
        "the count comes from the index the startup pass filled"
    );
}

/// `:wordcount notes` answers for one folder. The folder is a prefix of a note's
/// path, not a substring of it, so `notes/` and `notesx/` are different answers.
#[gpui::test]
fn the_count_scopes_to_one_folder(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Welcome.md", WELCOME),
            ("notes/Idea.md", IDEA),
            ("notesx/Elsewhere.md", IDEA),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("wordcount notes", cx);
    assert_eq!(status(&window, cx), "3 words in 1 note under notes/");
    type_command("wc", cx);
    assert_eq!(
        status(&window, cx),
        "12 words in 3 notes",
        "`wc` is the same command under the shorter name, with no folder"
    );
}

/// A note another program wrote is indexed by the same disk check that puts it in
/// the tree — the watcher is the index's event source once the startup pass ends.
#[gpui::test]
fn a_note_written_elsewhere_is_indexed_with_the_tree(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    std::fs::write(dir.path().join("extra.md"), "two more words\n").unwrap();
    window
        .update(cx, |ws, _window, cx| ws.check_vault_now(cx))
        .unwrap();

    type_command("wordcount", cx);
    assert_eq!(
        status(&window, cx),
        "9 words in 2 notes",
        "the new note is in the count without Shale reading the vault again"
    );
}

/// Shale's own save is invisible to the watcher — its own writes are re-stamped out
/// of the way — so the save path has to report the note itself.
#[gpui::test]
fn a_note_shale_saves_is_indexed_at_once(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("Welcome.md"), cx)
        })
        .unwrap();
    // `A` at the end of the first line, so the words land where the count expects
    // them rather than in the middle of the heading's marker.
    cx.simulate_keystrokes("A");
    cx.simulate_input(" extra words here");
    type_command("w", cx);

    type_command("wordcount", cx);
    assert_eq!(
        status(&window, cx),
        "9 words in 1 note",
        "the save reported the note; nothing had to wait for a disk check"
    );
}

/// A note Shale creates is in the index from the moment it exists, and a note it
/// deletes is out of it — otherwise the count quietly grows stale in both
/// directions.
#[gpui::test]
fn creating_a_note_puts_it_in_the_count(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("ctrl-n");
    cx.simulate_input("Fresh");
    cx.simulate_keystrokes("enter i");
    cx.simulate_input("four words right in");
    type_command("w", cx);

    type_command("wordcount", cx);
    assert_eq!(status(&window, cx), "10 words in 2 notes");
}

/// A rename moves the note's rows rather than adding a second copy of them, so the
/// count does not start counting the same note twice.
#[gpui::test]
fn a_rename_moves_the_count_rather_than_adding_to_it(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(cx, &[("Target.md", IDEA), ("a.md", "see [[Target]] now\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    cx.simulate_keystrokes("space f");
    cx.simulate_input("target");
    cx.simulate_keystrokes("enter ctrl-shift-r");
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace backspace");
    cx.simulate_input("Renamed");
    cx.simulate_keystrokes("enter");

    assert!(dir.path().join("Renamed.md").is_file());
    type_command("wordcount", cx);
    assert_eq!(
        status(&window, cx),
        "6 words in 2 notes",
        "the note that moved is still counted once"
    );
}

/// The badge and the command count a word the same way, so the number at the end of
/// the bar and the number the index reports cannot drift apart for a note that is
/// saved. They answer different questions — the badge is *this note, right now*, from
/// the buffer; `:wordcount` is *the vault, as indexed* — and agree where the two
/// meet.
#[gpui::test]
fn the_badge_and_the_command_agree_about_a_saved_note(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Welcome.md", WELCOME)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    window
        .update(cx, |ws, _window, cx| {
            ws.open_note(&PathBuf::from("Welcome.md"), cx)
        })
        .unwrap();
    type_command("wordcount", cx);
    assert_eq!(status(&window, cx), "6 words in 1 note");
    let badge = window
        .update(cx, |ws, _window, cx| ws.word_badge(cx))
        .unwrap();
    assert_eq!(badge, "6 words");
}

/// A vault with `index.enabled = false` has nothing to count, and says so rather
/// than inventing a number — the same posture as a checker that will not start.
#[gpui::test]
fn a_vault_without_an_index_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = workspace(cx, &["Welcome.md"]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("wordcount", cx);
    assert!(
        status(&window, cx).contains("no vault index"),
        "the window says which feature is missing: {}",
        status(&window, cx)
    );
}

/// The window keeps a copy of the cache beside it: opening a vault on a day that
/// has no copy yet makes one, in `.shale/backups/` and nowhere else (roadmap
/// P1.6). The rotation's own arithmetic — same-day skip, keep seven — is
/// `shale_core::db::backup`'s tests to make; this is the proof the window
/// actually runs it.
#[gpui::test]
fn opening_a_vault_takes_todays_index_backup(cx: &mut TestAppContext) {
    let (dir, _window) = indexed_vault(cx, &[("Welcome.md", WELCOME)]);
    let backups = dir.path().join(".shale").join("backups");
    let copies: Vec<_> = std::fs::read_dir(&backups)
        .expect("the window made today's copy")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(copies.len(), 1, "one copy for today: {copies:?}");
    assert!(
        copies[0].starts_with("shale-") && copies[0].ends_with(".db"),
        "named by day: {}",
        copies[0]
    );

    // And the vault itself stayed clean: a backup is state, not a note.
    let stray = std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("shale-")
        })
        .count();
    assert_eq!(stray, 0, "no backup in the vault's root");
}

// ---- `:props`: the properties table, read back through the window (P4.3a M1) ----

const STATBLOCK: &str = "---\ncr: 1/4\ntype: goblin\nhp: 7\naliases:\n  - Goblin\n  - Hobgoblin\n---\n# Goblin\n\nSmall and mean.\n";

/// The command reads the table, not the buffer: one entry per key, a list in
/// brackets, and the kind the indexer worked out named next to each.
#[gpui::test]
fn the_command_line_reads_one_notes_properties(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Goblin.md", STATBLOCK)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Goblin.md");

    type_command("props", cx);
    assert_eq!(
        status(&window, cx),
        "Goblin.md: cr=1/4 (text), type=goblin (text), hp=7 (number), \
         aliases=[Goblin, Hobgoblin] (text)",
        "authored order, and `1/4` stays text until the vault declares `cr` numeric"
    );
}

/// The kind census is vault-wide, so a key that is clean *in this note* still gets said
/// when another note writes it differently. The query that reads `cr` reads both notes,
/// and SQLite reads `CAST('low' AS REAL)` as 0 — a disagreement the author of the clean
/// note cannot see from their own block is exactly what the census is for.
#[gpui::test]
fn the_command_line_names_a_kind_the_vault_holds_elsewhere(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n# Goblin\n"),
            ("Wisp.md", "---\ncr: low\n---\n# Wisp\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Goblin.md");

    type_command("props", cx);
    assert_eq!(
        status(&window, cx),
        "Goblin.md: cr=5 (number, vault also text)",
        "the note's own kind first, and the vault's other kind named beside it"
    );
}

/// A note with no frontmatter is not an error and not a mystery.
#[gpui::test]
fn a_note_without_frontmatter_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Plain.md", "# Plain\n\nNothing up top.\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Plain.md");

    type_command("props", cx);
    assert_eq!(status(&window, cx), "Plain.md has no properties");
}

// ---- `:keys`: the vault-wide census (P4.3a M5) ----

/// The pane is the vault's vocabulary on one screen: one row per key, the keys that
/// reach furthest first, and each row saying how far it reaches and what its values
/// come as. A key whose values disagree is shown with **both** kinds, brought
/// forward — the pane is where a kind conflict is surfaced (ruling §8.4), not a
/// warning on every save.
#[gpui::test]
fn the_key_pane_lists_every_key_with_its_reach_and_kinds(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\ntype: goblin\n---\n# Goblin\n"),
            (
                "Wisp.md",
                "---\ncr: low\ntype: wisp\nMax HP: 4\n---\n# Wisp\n",
            ),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("keys", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec![r#"prop:cr"#, r#"prop:type"#, r#"prop:"Max HP""#],
        "the two keys every note writes first, ties alphabetical, and a key with a
         space comes back quoted so the search can read it"
    );

    let shown = window
        .update(cx, |ws, _window, cx| ws.picker_shown(cx))
        .unwrap();
    assert_eq!(shown[0].0, "cr");
    assert_eq!(
        shown[0].1, "2 notes · number, text",
        "the disagreement is in the row itself"
    );
    assert_eq!(
        shown[0].2,
        vec![
            (shown[0].1.len() - "number, text".len(), false),
            ("number, text".len(), true)
        ],
        "the kinds are the part brought forward, because that is the warning"
    );
    assert_eq!(
        shown[1].1, "2 notes · text",
        "a clean key gets a caption, not a warning"
    );
    assert!(
        shown[1].2.iter().all(|(_, lit)| !*lit),
        "nothing is brought forward on a key with nothing to flag"
    );
}

/// Choosing a key does not open a second list of notes: it puts `prop:key` in the
/// search field, which is the surface that answers "which notes write this" and can
/// be widened from there.
#[gpui::test]
fn a_key_row_hands_the_key_to_the_search(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n# Goblin\n"),
            ("Wisp.md", "---\ncr: low\n---\n# Wisp\n"),
            ("Plain.md", "# Plain\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("keys", cx);
    cx.simulate_keystrokes("enter");

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["Goblin.md", "Wisp.md", "save prop:cr"],
        "the notes that write `cr`, and not the one that writes nothing; then the offer to \
         keep the search the key row started"
    );
}

/// A vault with no properties at all is an answer, not an empty pane.
#[gpui::test]
fn the_key_pane_says_when_no_note_writes_a_property(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Plain.md", "# Plain\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("keys", cx);
    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["no note in this vault writes a property"],
        "an empty census says so rather than showing a blank pane"
    );
}

/// EDGE-25 again, one surface over: with no index the pane cannot answer, and it
/// says that rather than reporting a vault with no keys.
#[gpui::test]
fn the_key_pane_says_when_there_is_no_index(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Goblin.md", "---\ncr: 5\n---\nbody\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("keys", cx);
    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["there is no vault index to read (index.enabled = false)"],
        "\"zero keys\" and \"I cannot look\" are different answers"
    );
}

/// The palette reaches the pane from the words a person would type. Membership, not
/// ranking: being offered is the contract.
#[gpui::test]
fn the_palette_offers_the_key_pane(cx: &mut TestAppContext) {
    // The palette's rows are the command registry, which does not need an index.
    let (_dir, window) = vault_with(cx, &[("Goblin.md", "---\ncr: 5\n---\nbody\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    for query in ["keys", "property keys", "vault keys"] {
        cx.simulate_keystrokes("ctrl-shift-p");
        cx.simulate_input(query);
        let offered = window
            .update(cx, |ws, _window, cx| ws.picker_rows(cx))
            .unwrap();
        assert!(
            offered
                .iter()
                .any(|row| row == "List the vault's property keys"),
            "{query} offers: {offered:?}"
        );
        cx.simulate_keystrokes("escape");
    }
}

/// A key rename is the one migration a vocabulary cleanup asks for, so it has to
/// reach every note that writes the key — and stop exactly where a note stops being
/// frontmatter. The rules of what gets touched are `shale_core::rename`'s tests to
/// make; what is proved here is that the window's rename actually runs them, writes
/// the files, and says how far it got.
#[gpui::test]
fn a_key_rename_rewrites_every_note_that_writes_the_key(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(
        cx,
        &[
            (
                "Goblin.md",
                "---\ncr: 5\ntype: goblin\n---\n# Goblin\n\nThe card reads cr: 5 in prose.\n",
            ),
            ("Wisp.md", "---\ncr: low\n---\n# Wisp\n"),
            ("Nest.md", "---\ndata:\n  cr: 9\n---\n# Nest\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("rename-key cr challenge", cx);

    assert_eq!(
        std::fs::read_to_string(dir.path().join("Goblin.md")).unwrap(),
        "---\nchallenge: 5\ntype: goblin\n---\n# Goblin\n\nThe card reads cr: 5 in prose.\n",
        "the block's key changes and the prose that merely looks like one does not"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Wisp.md")).unwrap(),
        "---\nchallenge: low\n---\n# Wisp\n",
        "a value the index calls text is still the same value"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Nest.md")).unwrap(),
        "---\ndata:\n  cr: 9\n---\n# Nest\n",
        "a nested map's inner key is indexed but never rewritten"
    );
    assert!(
        status(&window, cx).contains("`cr` is now `challenge` in 2 notes"),
        "the line says how far it reached: {}",
        status(&window, cx)
    );
}

/// The census the reader chose the key from has to agree afterwards: a rename that
/// rewrote the notes and left the index holding the old spelling would put `:keys`
/// out of step with the vault, which is the one thing this pane cannot be.
#[gpui::test]
fn a_key_rename_moves_the_census_with_the_notes(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n# Goblin\n"),
            ("Wisp.md", "---\ncr: low\n---\n# Wisp\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("rename-key cr challenge", cx);

    type_command("keys", cx);
    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["prop:challenge"],
        "the old key is gone from the census, not added to it"
    );
}

/// The pane is the door for a key the reader cannot type at `:` — one with a space in
/// it — and for someone who would rather point at a row than name it. Choosing a row
/// in this mood opens the prompt rather than the search, and the field starts with
/// the old spelling so the answer is a correction rather than a retyping.
#[gpui::test]
fn the_key_pane_can_be_opened_to_rename(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n# Goblin\n"),
            ("Wisp.md", "---\ncr: 3\n---\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("rename-key", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["rename cr"],
        "the same census, and choosing a row renames that key rather than searching it"
    );

    cx.simulate_keystrokes("enter backspace backspace");
    cx.simulate_input("challenge");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Goblin.md")).unwrap(),
        "---\nchallenge: 5\n---\n# Goblin\n",
        "the prompt's answer is the new spelling"
    );
}

/// A rename across the vault is the worst place to overwrite a note the author is
/// mid-edit in, so the open note with unsaved edits is left alone and named — the
/// same rule the file rename keeps, at a larger scale.
#[gpui::test]
fn a_key_rename_leaves_a_dirty_open_note_alone(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n# Goblin\n"),
            ("Wisp.md", "---\ncr: 3\n---\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Goblin.md");
    cx.simulate_keystrokes("i x escape");

    type_command("rename-key cr challenge", cx);

    assert_eq!(
        std::fs::read_to_string(dir.path().join("Goblin.md")).unwrap(),
        "---\ncr: 5\n---\n# Goblin\n",
        "the note with unsaved edits is not written behind the author's back"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Wisp.md")).unwrap(),
        "---\nchallenge: 3\n---\n",
        "every other note is still updated"
    );
    assert!(
        status(&window, cx).contains("unsaved edits"),
        "and the unfinished part is named: {}",
        status(&window, cx)
    );
}

/// The note that must not be written blind is not only the one under the caret: a
/// dirty note sitting in another tab is just as much the author's work, and a
/// vault-wide rewrite reaching it is the same loss with a longer reach.
#[gpui::test]
fn a_key_rename_leaves_a_dirty_note_in_another_tab_alone(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(
        cx,
        &[
            ("Goblin.md", "---\ncr: 5\n---\n# Goblin\n"),
            ("Wisp.md", "---\ncr: 3\n---\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Goblin.md");
    cx.simulate_keystrokes("i x escape");
    // A second tab, and the other note goes there: the dirty note stays open, just
    // not under the caret.
    cx.simulate_keystrokes("ctrl-t");
    open_note(&window, cx, "Wisp.md");

    type_command("rename-key cr challenge", cx);

    assert_eq!(
        std::fs::read_to_string(dir.path().join("Goblin.md")).unwrap(),
        "---\ncr: 5\n---\n# Goblin\n",
        "a note open with unsaved edits is not rewritten because the caret moved on"
    );
    assert!(
        status(&window, cx).contains("unsaved edits"),
        "{}",
        status(&window, cx)
    );
}

/// A key is written plain in the block, so a name that cannot be one is refused
/// rather than written into every note in the vault.
#[gpui::test]
fn a_key_rename_refuses_a_name_that_cannot_be_a_key(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(cx, &[("Goblin.md", "---\ncr: 5\n---\n# Goblin\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("rename-key cr challenge:rating", cx);

    assert_eq!(
        std::fs::read_to_string(dir.path().join("Goblin.md")).unwrap(),
        "---\ncr: 5\n---\n# Goblin\n",
        "nothing was written"
    );
    assert!(
        status(&window, cx).contains("cannot be a property key"),
        "{}",
        status(&window, cx)
    );
}

/// A vault that declares a key keeps that declaration in `.shale/props.toml`, which
/// a rename of the notes does not move — it is a hand-kept file, and the promise made
/// here is about notes. The rename therefore says what it left behind, because a
/// declaration pointing at a key nobody writes means the new key's kind quietly stops
/// being honoured, and that is a bad thing to discover by inference.
#[gpui::test]
fn a_key_rename_names_the_declaration_it_left_behind(cx: &mut TestAppContext) {
    let (dir, window) = indexed_vault(cx, &[("Goblin.md", "---\ncr: 5\n---\n# Goblin\n")]);
    std::fs::create_dir_all(dir.path().join(".shale")).unwrap();
    std::fs::write(dir.path().join(".shale/props.toml"), "cr = \"number\"\n").unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    type_command("rename-key cr challenge", cx);

    assert!(
        status(&window, cx).contains("`.shale/props.toml` still declares `cr`"),
        "{}",
        status(&window, cx)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".shale/props.toml")).unwrap(),
        "cr = \"number\"\n",
        "the declaration file is left for its author to edit"
    );
}

/// The palette reaches the rename too, and the row is findable by the words a reader
/// would actually type.
#[gpui::test]
fn the_palette_offers_the_key_rename(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Goblin.md", "---\ncr: 5\n---\nbody\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    for query in ["rename key", "rename property", "rename prop key"] {
        cx.simulate_keystrokes("ctrl-shift-p");
        cx.simulate_input(query);
        let offered = window
            .update(cx, |ws, _window, cx| ws.picker_rows(cx))
            .unwrap();
        assert!(
            offered
                .iter()
                .any(|row| row == "Rename a property key across the vault"),
            "{query} offers: {offered:?}"
        );
        cx.simulate_keystrokes("escape");
    }
}

/// EDGE-25: with no index, `:props` says there is nothing to read rather than
/// reporting that the note has no properties. "Zero" and "I cannot look" are
/// different answers, and this is the posture `:wordcount` and the spellchecker keep.
#[gpui::test]
fn no_index_is_reported_as_no_index(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Plain.md", "---\ncr: 1\n---\nbody\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Plain.md");

    type_command("props", cx);
    assert_eq!(
        status(&window, cx),
        "there is no vault index to read (index.enabled = false)"
    );
}

/// The two badges the census will lean on: a key the note wrote twice, and a value
/// Shale indexes but does not read. Both are shown rather than smoothed over.
#[gpui::test]
fn a_written_twice_key_and_an_unread_value_say_what_they_are(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[(
            "Odd.md",
            "---\ntags: a\ntags: b\nquest:\n  name: The Lost Mine\n---\nbody\n",
        )],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Odd.md");

    type_command("props", cx);
    let line = status(&window, cx);
    assert!(
        line.contains("tags=a | b (text, written twice)"),
        "the duplicate is shown, not merged: {line}"
    );
    assert!(
        line.contains("quest=name: The Lost Mine (nested, not read)"),
        "the unread value is named as unread: {line}"
    );
}

// ---- `:sections`: the outline, and the vault-wide jump (P4.3b M3) ----

/// The row a section jump renders: which note, which heading, and the line the index
/// named — 0-based, the basis the `headings` table stores.
fn caret_row(window: &WindowHandle<Workspace>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |ws, _window, cx| ws.cursor_position(cx))
        .unwrap()
        .0
}

/// `:sections` with no pattern outlines the note on screen, in reading order, each row
/// saying how deep it is and which line it sits on. The list comes from the **buffer**,
/// so it is the outline of what the reader is looking at rather than what the disk
/// remembers (headings-plan.md ruling 52).
#[gpui::test]
fn sections_outlines_the_note_on_screen(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Lair.md", "# The Fall\nbody\n## The Deep\nmore\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("sections", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["Lair.md#The Fall@0", "Lair.md#The Deep@2"],
        "in reading order, with the line the reveal will use"
    );

    let shown = window
        .update(cx, |ws, _window, cx| ws.picker_shown(cx))
        .unwrap();
    assert_eq!(shown[0].0, "The Fall");
    assert_eq!(
        shown[1].0, "  The Deep",
        "indented by depth, because the point of an outline is which section is inside which"
    );
    assert_eq!(
        shown[1].1, "## · line 3",
        "the markers and the line are the dim half: 1-based, the way a reader counts"
    );
}

/// Choosing a row puts the caret on the heading, not at the top of the note.
#[gpui::test]
fn a_section_row_puts_the_caret_on_its_heading(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[("Lair.md", "# The Fall\nbody\n## The Deep\nmore\nand more\n")],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("sections", cx);
    // `down`, not `j`: the picker is a text field first, so a typed `j` would filter
    // the list to nothing rather than step through it.
    cx.simulate_keystrokes("down enter");

    assert_eq!(
        caret_row(&window, cx),
        2,
        "the second row is `## The Deep`, on buffer row 2"
    );
}

/// A note with nothing to outline is an answer, not an empty pane.
#[gpui::test]
fn a_note_with_no_sections_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Plain.md", "just prose\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Plain.md");
    type_command("sections", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(rows, vec!["Plain.md has no sections"]);
}

/// With a pattern the list is the vault, read from `headings`: the sections in notes
/// nobody has open, which is the question a table can answer and a buffer cannot.
#[gpui::test]
fn sections_with_a_pattern_lists_the_vault_through_the_index(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Lair.md", "# The Fall\nbody\n## The Deep\n"),
            ("Other.md", "# the fall\n"),
            ("Third.md", "# Nothing here\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("sections fall", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["Lair.md#The Fall@0", "Other.md#the fall@0"],
        "case-insensitively, in note order, and the note that has no such section is absent"
    );

    let shown = window
        .update(cx, |ws, _window, cx| ws.picker_shown(cx))
        .unwrap();
    assert_eq!(
        shown[0].1, "Lair.md · line 1",
        "the note is the dim half, because out here the section is not the interesting part"
    );
}

/// A vault with no index says it cannot look rather than reporting no sections — the
/// same posture `:keys` and `:wordcount` keep (EDGE-25).
#[gpui::test]
fn sections_without_an_index_says_it_cannot_look(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Lair.md", "# The Fall\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("sections fall", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["there is no vault index to read (index.enabled = false)"]
    );
}

/// The index answers about the vault; the buffer answers about the note under the
/// reader. A note edited since the index read it has moved its headings, and the jump
/// checks the line against the buffer instead of believing it — so the caret lands on
/// the heading the reader can see. Believing the stale line is what this test catches.
#[gpui::test]
fn a_section_jump_into_a_dirty_note_lands_on_the_heading_the_reader_sees(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(cx, &[("Gift.md", "# Gift\nnot this\n## The middle\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Gift.md");
    // A line above everything, left unsaved: the index still remembers the headings
    // where the file had them.
    cx.simulate_keystrokes("gg O pushed down escape");

    type_command("sections middle", cx);
    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["Gift.md#The middle@2"],
        "the index's line, stale by one"
    );

    cx.simulate_keystrokes("enter");
    assert_eq!(
        caret_row(&window, cx),
        3,
        "the heading where the reader is looking at it, not where the index last read it"
    );
}

// ---- `:block-id` and `:blocks`: the two verbs (P4.9 M4) ----

/// The id Shale generated, read back out of the note: six characters of `a-z2-7`
/// (ruling 2). Returns the whole buffer text too, for the tests that need both.
fn id_in(text: &str) -> String {
    let at = text.find('^').expect("an id was written");
    text[at + 1..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn is_generated(id: &str) -> bool {
    id.len() == 6
        && id
            .chars()
            .all(|c| "abcdefghijklmnopqrstuvwxyz234567".contains(c))
}

/// `:block-id` on a paragraph writes the id at the end of its last line, says what it
/// wrote, and one `u` takes the whole thing back (EDGE-21).
#[gpui::test]
fn block_id_gives_the_block_under_the_caret_an_id(cx: &mut TestAppContext) {
    let original = "the goblin waits\nby the door\n\nnext paragraph\n";
    let (_dir, window) = vault_with(cx, &[("Lair.md", original)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("block-id", cx);

    let text = window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap();
    assert!(
        text.starts_with("the goblin waits\nby the door ^"),
        "the id lands at the end of the block's last line: {text}"
    );
    let id = id_in(&text);
    assert!(is_generated(&id), "{id} is not a generated id");
    assert!(
        status(&window, cx).starts_with("block id ^"),
        "{}",
        status(&window, cx)
    );

    cx.simulate_keystrokes("u");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        original,
        "one undo removes the id and the space before it"
    );
}

/// EDGE-10: a block that already has an id is not given a second one. The sentence
/// names the id that is already there, which is what the reader needs to reuse it.
#[gpui::test]
fn block_id_refuses_a_second_id_on_the_same_block(cx: &mut TestAppContext) {
    let text = "the goblin waits ^goblin\n\nnext\n";
    let (_dir, window) = vault_with(cx, &[("Lair.md", text)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("block-id", cx);

    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        text,
        "nothing was written"
    );
    assert_eq!(
        status(&window, cx),
        "this block already has an id: ^goblin",
        "and the reader is told which one"
    );
}

/// A typed id is used as written, so an author can name a block after what it is
/// instead of after what a generator drew.
#[gpui::test]
fn block_id_takes_the_id_the_reader_types(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Lair.md", "the goblin waits\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("block-id goblin", cx);

    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "the goblin waits ^goblin\n"
    );
    assert_eq!(status(&window, cx), "block id ^goblin (u takes it back)");
}

/// Ruling 2's grammar is a refusal, not a shrug: an id with a character outside
/// `[A-Za-z0-9-]` would not be found by the scan, so writing it would be a lie.
#[gpui::test]
fn block_id_refuses_an_id_outside_the_grammar(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Lair.md", "the goblin waits\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("block-id bad_id", cx);

    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "the goblin waits\n"
    );
    assert_eq!(
        status(&window, cx),
        "^bad_id is not a block id: letters, digits and - only"
    );
}

/// The collision check reaches past the note on screen: an id another note already
/// uses would make `[[elsewhere#^goblin]]` ambiguous, so it is refused. This is the
/// vault-wide half — the buffer alone would let it through.
#[gpui::test]
fn block_id_refuses_an_id_the_vault_already_uses(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Gift.md", "the troll waits ^goblin\n"),
            ("Lair.md", "the goblin waits\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("block-id goblin", cx);

    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "the goblin waits\n",
        "the note was not written"
    );
    assert_eq!(
        status(&window, cx),
        "^goblin is already in use in this vault",
        "{}",
        status(&window, cx)
    );
}

/// A structured block's id stands on its own line below a blank (EDGE-8), and the one
/// undo takes the blank line away with it (EDGE-21).
#[gpui::test]
fn block_id_on_a_table_writes_the_own_line_form_as_one_edit(cx: &mut TestAppContext) {
    let original = "| a | b |\n| - | - |\n| 1 | 2 |\n";
    let (_dir, window) = vault_with(cx, &[("Lair.md", original)]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.simulate_keystrokes("0");
    type_command("block-id tid", cx);

    let text = window
        .update(cx, |ws, _window, cx| ws.buffer_text(cx))
        .unwrap();
    assert_eq!(text, "| a | b |\n| - | - |\n| 1 | 2 |\n\n^tid\n");
    cx.simulate_keystrokes("u");
    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        original,
        "the blank line goes with the id"
    );
}

/// `:blocks` with no pattern lists the note on screen from its **buffer**: the ids the
/// reader can see, in reading order, each row labelled by the block's first line.
#[gpui::test]
fn blocks_lists_the_note_on_screen(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Lair.md", "one ^aaaaaa\n\ntwo ^bbbbbb\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("blocks", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["Lair.md#^aaaaaa", "Lair.md#^bbbbbb"],
        "in reading order"
    );

    let shown = window
        .update(cx, |ws, _window, cx| ws.picker_shown(cx))
        .unwrap();
    assert_eq!(
        shown[0].0, "one ^aaaaaa",
        "the block's first line is the label"
    );
    assert_eq!(
        shown[0].1, "^aaaaaa · paragraph · line 1",
        "the id, the kind, and the line a reader counts from one"
    );
}

/// EDGE-12: the list answers from the buffer, so an id typed a second ago — unsaved,
/// unread by the index — is already in it.
#[gpui::test]
fn blocks_lists_an_id_the_reader_just_typed(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Lair.md", "the goblin waits\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.simulate_keystrokes("A");
    cx.simulate_input(" ^mine");
    cx.simulate_keystrokes("escape");

    type_command("blocks", cx);
    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(rows, vec!["Lair.md#^mine"], "unsaved, and still listed");
}

/// Choosing a row puts the caret on the block's first line — the top of the thing the
/// id names, which is the same row the `blocks` table stores.
#[gpui::test]
fn a_block_row_puts_the_caret_on_the_blocks_first_line(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(
        cx,
        &[(
            "Lair.md",
            "intro\n\nthe goblin waits\nby the door. ^goblin\n",
        )],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    type_command("blocks", cx);
    cx.simulate_keystrokes("enter");

    assert_eq!(
        caret_row(&window, cx),
        2,
        "the top of the block, not the line the id sits on"
    );
}

/// With a pattern the list is the vault, read from `blocks`: ids in notes nobody has
/// open, which is the question a table answers and a buffer cannot.
#[gpui::test]
fn blocks_with_a_pattern_lists_the_vault_through_the_index(cx: &mut TestAppContext) {
    let (_dir, window) = indexed_vault(
        cx,
        &[
            ("Gift.md", "the troll waits ^goblin\n"),
            ("Lair.md", "the goblin waits ^gob222\n"),
            ("Third.md", "nothing ided here\n"),
        ],
    );
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("blocks gob", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["Gift.md#^goblin", "Lair.md#^gob222"],
        "in note order, and the note with no such id is absent"
    );
}

/// A vault with no index says it cannot look, rather than reporting no ids (EDGE-13).
#[gpui::test]
fn blocks_without_an_index_says_it_cannot_look(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Lair.md", "the goblin waits ^goblin\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    type_command("blocks gob", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["there is no vault index to read (index.enabled = false)"],
        "cannot look, and says so"
    );
}

/// A note with no ids is an answer, not an empty pane — and the answer points at the
/// verb that would change it.
#[gpui::test]
fn a_note_with_no_block_ids_says_so(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Plain.md", "just prose\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Plain.md");
    type_command("blocks", cx);

    let rows = window
        .update(cx, |ws, _window, cx| ws.picker_rows(cx))
        .unwrap();
    assert_eq!(
        rows,
        vec!["Plain.md has no block ids yet (`:block-id` adds one)"],
        "{rows:?}"
    );
}

/// The caret decides which block, and a caret on a blank line decides nothing: the
/// command says where to put the caret rather than iding the block above by surprise.
#[gpui::test]
fn block_id_on_a_blank_line_says_where_the_caret_should_be(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("Lair.md", "one\n\nthree\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    open_note(&window, cx, "Lair.md");
    cx.simulate_keystrokes("j");
    type_command("block-id tid", cx);

    assert_eq!(
        window
            .update(cx, |ws, _window, cx| ws.buffer_text(cx))
            .unwrap(),
        "one\n\nthree\n"
    );
    assert_eq!(
        status(&window, cx),
        "put the caret inside a paragraph, list item, heading, table, quote or code block"
    );
}

/// The palette reaches both verbs from the words a person would type. Membership, not
/// ranking, is the contract.
#[gpui::test]
fn the_palette_offers_both_block_verbs(cx: &mut TestAppContext) {
    let (_dir, window) = vault_with(cx, &[("a.md", "one line\n")]);
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    for (query, label) in [
        ("block", "Jump to a block by its id"),
        ("blocks", "Jump to a block by its id"),
        ("block id", "Give the block under the caret an id"),
        ("blockid", "Give the block under the caret an id"),
    ] {
        cx.simulate_keystrokes("ctrl-shift-p");
        cx.simulate_input(query);
        let offered = window
            .update(cx, |ws, _window, cx| ws.picker_rows(cx))
            .unwrap();
        assert!(
            offered.iter().any(|row| row == label),
            "{query} offers: {offered:?}"
        );
        cx.simulate_keystrokes("escape");
    }
}
