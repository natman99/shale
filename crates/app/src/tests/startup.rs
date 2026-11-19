//! The start with no path named (roadmap P4.20): what the app decides, and the screen
//! it shows when the answer is "ask".
//!
//! The decision is a pure function of the command line and the remembered list, so the
//! table is tested directly. The screen is tested as a window: which rows it offers, what
//! typing does to them, what `enter` costs, and what a refusal leaves on screen.

use std::path::PathBuf;

use shale_core::RecentVaults;

use super::*;
use crate::tests::vaults::vault_dir;
use crate::vault_open::{self, VaultTarget};
use crate::workspace::exit;
use crate::workspace::vaults;

/// Point the recent store at a temp dir and put the globals a run needs in place.
fn setup(cx: &mut TestAppContext, store: &std::path::Path) {
    cx.update(|cx| {
        cx.bind_keys(keybindings());
        cx.set_global(vaults::VaultWindows::new());
        cx.set_global(vaults::RecentStore(store.to_path_buf()));
    });
}

/// Remember `vaults` in the store the screen will read.
fn remember(store: &std::path::Path, vaults: &[std::path::PathBuf]) {
    let mut recent = RecentVaults::default();
    for vault in vaults {
        recent.touch(vault);
    }
    recent.save_to(store).unwrap();
}

// ---------------------------------------------------------------- the decision

#[test]
fn the_command_line_wins_over_the_memory() {
    let (_keep, named) = vault_dir("named");
    let (_keep2, remembered) = vault_dir("remembered");
    let mut recent = RecentVaults::default();
    recent.touch(&remembered);
    assert_eq!(
        vault_open::startup_plan(Some(named.clone()), &recent),
        vault_open::Startup::Vault(named.canonicalize().unwrap()),
        "a path typed into a terminal is the one meant"
    );
}

#[test]
fn with_nothing_named_the_remembered_vault_opens() {
    let (_keep, a) = vault_dir("alpha");
    let mut recent = RecentVaults::default();
    recent.touch(&a);
    assert_eq!(
        vault_open::startup_plan(None, &recent),
        vault_open::Startup::Vault(a.canonicalize().unwrap())
    );
}

/// A path *named* and no good is a terminal answer, not a window: a script needs an exit
/// code rather than a screen it cannot see (EDGE-V4).
#[test]
fn a_named_path_that_is_not_a_vault_is_refused_rather_than_asked() {
    let plan = vault_open::startup_plan(
        Some(PathBuf::from("/definitely/not/a/vault/anywhere")),
        &RecentVaults::default(),
    );
    assert!(
        matches!(plan, vault_open::Startup::Refuse(_)),
        "not `Ask`: a desktop shortcut never names a path, so the asking screen is not for this case: {plan:?}"
    );
}

#[test]
fn a_first_run_asks_with_nothing_to_explain() {
    assert_eq!(
        vault_open::startup_plan(None, &RecentVaults::default()),
        vault_open::Startup::Ask(None)
    );
}

#[test]
fn a_remembered_vault_that_is_gone_asks_and_names_it() {
    let gone = PathBuf::from("/definitely/not/a/vault/anywhere");
    let recent = RecentVaults {
        last: Some(gone.clone()),
        recent: vec![],
    };
    let plan = vault_open::startup_plan(None, &recent);
    let said = match plan {
        vault_open::Startup::Ask(Some(said)) => said,
        other => panic!("a remembered folder that is gone has to ask, with a reason: {other:?}"),
    };
    assert!(
        said.contains("not a vault any more") && said.contains("anywhere"),
        "the screen says which folder it looked for: {said}"
    );
}

// ---------------------------------------------------------------- the screen

#[gpui::test]
fn the_screen_lists_the_remembered_vaults_then_the_folder(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);
    let (_k, alpha) = vault_dir("alpha");
    let (_k, beta) = vault_dir("beta");
    remember(&store, &[alpha.clone(), beta.clone()]);

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let targets = window
        .update(&mut vcx, |view, _window, cx| view.targets(cx))
        .unwrap();
    assert_eq!(
        targets,
        vec![
            VaultTarget::Recent(beta.canonicalize().unwrap()),
            VaultTarget::Recent(alpha.canonicalize().unwrap()),
            VaultTarget::OpenFolder,
            VaultTarget::NewVault,
        ],
        "most recent first, then the two ways to a vault this app has never seen"
    );
}

#[gpui::test]
fn typing_narrows_the_screen_to_the_vault_being_named(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);
    let (_k, alpha) = vault_dir("alpha");
    let (_k, beta) = vault_dir("beta");
    remember(&store, &[alpha.clone(), beta.clone()]);

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("alph");
    let targets = window
        .update(&mut vcx, |view, _window, cx| view.targets(cx))
        .unwrap();
    assert_eq!(
        targets,
        vec![VaultTarget::Recent(alpha.canonicalize().unwrap())],
        "the folder's name is what the row is searched by"
    );
}

/// `enter` on a remembered vault: the vault window is up and the screen is gone. The
/// screen's whole job is to get the reader to a vault.
#[gpui::test]
fn choosing_a_vault_opens_it_and_retires_the_screen(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);
    let (_k, alpha) = vault_dir("alpha");
    remember(&store, std::slice::from_ref(&alpha));

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    let opened = cx.update(|cx| vaults::open_vaults(cx));
    assert_eq!(
        opened,
        vec![alpha.canonicalize().unwrap()],
        "the vault the row named is now a window"
    );
    assert!(
        !cx.update(|cx| vaults::has_startup_window(cx)),
        "and the screen that asked is gone"
    );
}

/// A vault another Shale holds: the screen stays, and says why, in the colour it keeps
/// for a broken thing. The keyboard stays where the failure happened.
#[gpui::test]
fn a_vault_held_by_another_shale_is_said_and_the_screen_stays(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);
    let (_k, alpha) = vault_dir("alpha");
    remember(&store, std::slice::from_ref(&alpha));

    // The claim a *different* process would hold. `flock` is per open file description,
    // so a second claim of the same vault in this process conflicts exactly the way a
    // second Shale's would — and the handle has to stay alive for the test to mean it.
    let held = vaults::claim_vault(&alpha.canonicalize().unwrap()).unwrap();
    assert!(held.is_some(), "the vault was free to claim");

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    let drawn = window
        .update(&mut vcx, |view, _window, _cx| view.drawn())
        .unwrap();
    let (said, danger) = drawn.expect("the screen drew a line about the refusal");
    assert!(
        said.contains("already open in another Shale"),
        "the sentence names the real reason: {said}"
    );
    assert!(danger, "and it is drawn as a refusal, not as a note");
    assert!(
        cx.update(|cx| vaults::has_startup_window(cx)),
        "the screen is still there to choose another vault"
    );
    assert!(cx.update(|cx| vaults::open_vaults(cx)).is_empty());
}

/// `escape` with nothing chosen: the screen closes, and a run with no window in it ends.
#[gpui::test]
fn escape_closes_the_screen(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);
    let (_k, alpha) = vault_dir("alpha");
    remember(&store, std::slice::from_ref(&alpha));

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();

    assert!(
        !cx.update(|cx| vaults::has_startup_window(cx)),
        "the screen let go of the registry as it went"
    );
    assert!(cx.update(|cx| vaults::open_vaults(cx)).is_empty());
}

/// A `kill` that lands while the app is sitting on the startup screen has to take that
/// window too. It is not a window on a vault, so it is not in the per-vault list the
/// watcher walks; the registry keeps a second handle for exactly this (EDGE-V16).
#[gpui::test]
fn a_signal_closes_the_screen(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let (_termination, watcher) = exit::Termination::new();
    cx.update(|cx| exit::spawn_watcher(watcher, cx));
    vcx.run_until_parked();
    assert!(
        window
            .update(&mut vcx, |view, _window, _cx| view.drawn())
            .is_ok(),
        "the screen is on screen before the signal"
    );

    _termination.wake();
    vcx.run_until_parked();

    assert!(
        window
            .update(&mut vcx, |view, _window, _cx| view.drawn())
            .is_err(),
        "a signal ends a run that is sitting on a question: the window is still there"
    );
    assert!(
        !cx.update(|cx| vaults::has_startup_window(cx)),
        "and the registry let go of the handle"
    );
}

// ---------------------------------------------------------------- a new vault

/// A bare name lands under the home folder — the place "make me a vault called Dnd"
/// means — and a path spelled out is honored as spelled.
#[test]
fn a_bare_vault_name_lands_under_the_home_folder() {
    let home = std::path::Path::new("/home/reader");
    assert_eq!(
        vault_open::vault_path("Dnd", Some(home)).unwrap(),
        home.join("Dnd")
    );
    assert_eq!(
        vault_open::vault_path("~/Dnd", Some(home)).unwrap(),
        home.join("Dnd"),
        "the tilde is typed out of habit and means the same place"
    );
    // "absolute" is spelled differently on each platform, and the rule is about the
    // spelling: a path that names its own root is honored as named.
    #[cfg(windows)]
    let spelled = r"C:\srv\notes";
    #[cfg(not(windows))]
    let spelled = "/srv/notes";
    assert_eq!(
        vault_open::vault_path(spelled, Some(home)).unwrap(),
        std::path::PathBuf::from(spelled),
        "a spelled-out path is not rewritten"
    );
    assert!(
        vault_open::vault_path("..", Some(home)).is_err(),
        "a vault cannot be named for the folder it would live in"
    );
    assert!(
        vault_open::vault_path("Dnd", None).is_err(),
        "with nowhere to put it, a bare name has to be said rather than guessed"
    );
}

/// A path spelled out as keystrokes. `simulate_keystrokes` reads a bare letter as
/// lowercase, so a capital has to be asked for with `shift`; the temp folders this test
/// types into have capitals in their random names.
fn typed_path(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_uppercase() {
                format!(" shift-{c}")
            } else {
                format!(" {c}")
            }
        })
        .collect::<String>()
}

/// Reach the row, name the folder, and the app makes it and opens it: the shortcut that
/// hard-codes nothing can still start a reader with nothing on disk yet.
#[gpui::test]
fn a_new_vault_is_made_and_opened(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);
    let base = tempfile::tempdir().unwrap();
    let target = base.path().join("goblins");
    // The vault's own state folder, and the checker off inside it: opening a real vault
    // window starts `harper-ls`, whose reader thread wakes the test scheduler and makes
    // the run non-deterministic (the same fixture reason as `tests::vaults`). `.shale`
    // alone does not make a folder "already have things in it".
    std::fs::create_dir_all(target.join(".shale")).unwrap();
    std::fs::write(
        target.join(".shale").join("config.toml"),
        "[spellcheck]\nenabled = false\n",
    )
    .unwrap();

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("vault");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    vcx.simulate_keystrokes(&typed_path(&target));
    vcx.run_until_parked();
    let typed = window
        .update(&mut vcx, |view, _window, cx| view.prompt_text(cx))
        .unwrap();
    assert_eq!(
        typed,
        target.to_string_lossy().to_string(),
        "what the field holds"
    );
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    assert!(target.is_dir(), "the folder was made: {}", target.display());
    assert_eq!(
        cx.update(|cx| vaults::open_vaults(cx)),
        vec![target.canonicalize().unwrap()],
        "and opened"
    );
    assert!(!cx.update(|cx| vaults::has_startup_window(cx)));
}

/// The folder is made where the name points, parents and all, and an empty folder is
/// accepted as already made (the reader may have made it themselves).
#[test]
fn a_new_vault_folder_is_made_where_the_name_points() {
    let base = tempfile::tempdir().unwrap();
    let target = base.path().join("deep/goblins");
    vault_open::make_folder(&target).unwrap();
    assert!(target.is_dir(), "the parents came with it");
    vault_open::make_folder(&target).unwrap();
    assert!(target.is_dir(), "an empty folder is not an argument");
}

#[test]
fn a_folder_that_is_not_empty_is_not_made_into_a_vault() {
    let base = tempfile::tempdir().unwrap();
    let occupied = base.path().join("photos");
    std::fs::create_dir_all(&occupied).unwrap();
    std::fs::write(occupied.join("a.jpg"), "not a note").unwrap();
    let why = vault_open::make_folder(&occupied).unwrap_err();
    assert!(why.contains("already has things in it"), "{why}");

    // Shale's own state does not count as "things": a vault opened once and emptied is
    // exactly the folder the reader is asking to open again.
    let emptied = base.path().join("emptied");
    std::fs::create_dir_all(emptied.join(".shale")).unwrap();
    vault_open::make_folder(&emptied).unwrap();

    let file = base.path().join("afile.md");
    std::fs::write(&file, "# note").unwrap();
    let why = vault_open::make_folder(&file).unwrap_err();
    assert!(why.contains("is a file, not a folder"), "{why}");
}

/// `escape` backs out of the name without leaving the screen: the field goes, the list
/// comes back, and a second `escape` is the one that closes the window. Two questions,
/// two escapes.
#[gpui::test]
fn backing_out_of_the_name_returns_to_the_list(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("vault");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert!(
        window
            .update(&mut vcx, |view, _, _| view.asking_name())
            .unwrap()
    );

    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert!(
        !window
            .update(&mut vcx, |view, _, _| view.asking_name())
            .unwrap(),
        "the name field is gone"
    );
    assert!(
        cx.update(|cx| vaults::has_startup_window(cx)),
        "the screen is not"
    );

    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert!(!cx.update(|cx| vaults::has_startup_window(cx)));
}

/// A folder that already has things in it is someone's folder: "new vault" does not
/// quietly vault it. The field stays with the name in it.
#[gpui::test]
fn a_folder_with_things_in_it_is_not_made_into_a_vault(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);
    let base = tempfile::tempdir().unwrap();
    let target = base.path().join("photos");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("a.jpg"), "not a note").unwrap();

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("vault");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.simulate_keystrokes(&typed_path(&target));
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    let (said, danger) = window
        .update(&mut vcx, |view, _window, _cx| view.drawn())
        .unwrap()
        .expect("the refusal is on the screen");
    assert!(said.contains("already has things in it"), "{said}");
    assert!(danger);
    assert!(
        cx.update(|cx| vaults::open_vaults(cx)).is_empty(),
        "and nothing was opened"
    );
}

/// An empty answer is not a folder name. The field stays open rather than the screen
/// falling back to a guess.
#[gpui::test]
fn a_new_vault_needs_a_name(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);

    let window = cx
        .update(|cx| vault_open::open_startup_window(None, cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("vault");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    let (said, danger) = window
        .update(&mut vcx, |view, _window, _cx| view.drawn())
        .unwrap()
        .expect("the screen says what is missing");
    assert!(said.contains("needs a name"), "{said}");
    assert!(danger);
    assert!(
        window
            .update(&mut vcx, |view, _window, _cx| view.asking_name())
            .unwrap(),
        "the field is still open to be answered"
    );
    assert!(cx.update(|cx| vaults::open_vaults(cx)).is_empty());
}

/// The reason the screen opened is on the screen: a reader who launched from a shortcut
/// and got a question instead of their vault needs to know which one it looked for.
#[gpui::test]
fn the_reason_the_screen_opened_is_drawn_on_it(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store = store.path().join("recent.json");
    setup(cx, &store);
    let window = cx
        .update(|cx| vault_open::open_startup_window(Some("/mnt/gone/dnd".to_string()), cx))
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let (said, danger) = window
        .update(&mut vcx, |view, _window, _cx| view.drawn())
        .unwrap()
        .expect("the reason is drawn");
    assert!(said.contains("/mnt/gone/dnd"), "the folder named: {said}");
    assert!(
        !danger,
        "an explanation of where the reader is is not an error: {said}"
    );
}
