//! One Shale, one window per vault: the registry that opens vault windows,
//! the claim that keeps other processes out, and the recent list the switcher
//! reads.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use shale_core::RecentVaults;

use super::*;
use crate::workspace::vaults::{self, OpenOptions};

/// A vault directory with one note in it, literally named `name` — the
/// switcher's rows are named for the directory, so a test that types a vault's
/// name has to have a vault whose name is in the path. Returns the temp dir
/// that owns the tree and the vault path inside it.
fn vault_dir(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.md"), format!("# {name}\n")).unwrap();
    // These tests pump the scheduler long enough for a live `harper-ls` to
    // answer, and its reader thread waking the foreground executor trips the
    // test scheduler's determinism check. The checker is off here.
    std::fs::create_dir_all(dir.join(".shale")).unwrap();
    std::fs::write(
        dir.join(".shale").join("config.toml"),
        "[spellcheck]\nenabled = false\n",
    )
    .unwrap();
    (base, dir)
}

/// The globals a vault-opening run needs, with the recent store pointed at
/// the temp dir so the test does not edit anyone's real memory.
fn setup(cx: &mut TestAppContext, store: &std::path::Path) {
    cx.update(|cx| {
        cx.bind_keys(keybindings());
        cx.set_global(vaults::VaultWindows::new(Arc::new(AtomicBool::new(false))));
        cx.set_global(vaults::RecentStore(store.to_path_buf()));
    });
}

#[gpui::test]
fn two_vaults_are_two_windows_in_one_process(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    setup(cx, &store.path().join("recent.json"));
    let (_ka, a) = vault_dir("alpha");
    let (_kb, b) = vault_dir("beta");

    let wa = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    let wb = cx
        .update(|cx| vaults::open_vault_window(&b, OpenOptions::default(), cx))
        .unwrap();
    assert_ne!(wa.window_id(), wb.window_id(), "two vaults, two windows");
    let mut opened = cx.update(|cx| vaults::open_vaults(cx));
    opened.sort();
    let mut want = vec![
        a.canonicalize().unwrap(),
        b.canonicalize().unwrap(),
    ];
    want.sort();
    assert_eq!(opened, want);
}

#[gpui::test]
fn the_same_vault_focuses_its_window_instead_of_opening_a_second(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    setup(cx, &store.path().join("recent.json"));
    let (_ka, a) = vault_dir("alpha");

    let first = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    // The same vault spelled through a `.` — the registry canonicalizes.
    let spelled = a.join(".").join("a.md").parent().unwrap().to_path_buf();
    let again = cx
        .update(|cx| vaults::open_vault_window(&spelled, OpenOptions::default(), cx))
        .unwrap();
    assert_eq!(
        first.window_id(),
        again.window_id(),
        "one vault, one window, whatever the spelling"
    );
    assert_eq!(cx.update(|cx| vaults::open_vaults(cx)).len(), 1);
}

#[gpui::test]
fn opening_a_vault_records_it_for_the_next_start(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("recent.json");
    setup(cx, &store_path);
    let (_ka, a) = vault_dir("alpha");
    let (_kb, b) = vault_dir("beta");

    cx.update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    cx.update(|cx| vaults::open_vault_window(&b, OpenOptions::default(), cx))
        .unwrap();

    let recent = RecentVaults::load_from(&store_path);
    assert_eq!(
        recent.recent,
        vec![
            b.canonicalize().unwrap(),
            a.canonicalize().unwrap()
        ],
        "most recent first"
    );
    assert_eq!(recent.last.as_deref(), Some(b.canonicalize().unwrap().as_path()));
    assert_eq!(
        cx.update(|cx| vaults::recent_vaults(cx)),
        recent.recent,
        "and the switcher reads the same list"
    );
}

#[cfg(unix)]
#[gpui::test]
fn a_vault_claimed_elsewhere_is_refused(cx: &mut TestAppContext) {
    use std::os::unix::io::AsRawFd;
    let store = tempfile::tempdir().unwrap();
    setup(cx, &store.path().join("recent.json"));
    let (_ka, a) = vault_dir("alpha");

    // Another process's claim: the lock file, held.
    std::fs::create_dir_all(a.join(".shale")).unwrap();
    let held = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(a.join(".shale").join("lock"))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0,
        "the test holds the claim now"
    );

    let err = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("already open"),
        "the refusal says why: {err:#}"
    );
    assert!(cx.update(|cx| vaults::open_vaults(cx)).is_empty());
}

#[gpui::test]
fn a_closed_window_frees_its_vault(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    setup(cx, &store.path().join("recent.json"));
    let (_ka, a) = vault_dir("alpha");

    let window = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    assert_eq!(cx.update(|cx| vaults::open_vaults(cx)).len(), 1);
    cx.update(|cx| {
        window.update(cx, |_ws, window, _cx| window.remove_window()).unwrap();
    });
    // The registry notices the release, and the vault can be opened again —
    // by this process, in a fresh window.
    assert!(
        cx.update(|cx| vaults::open_vaults(cx)).is_empty(),
        "the registry forgot the closed window"
    );
    let again = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    assert_ne!(window.window_id(), again.window_id());
}

/// `:vault` lists what the app remembers, and choosing a row opens that vault
/// and closes the window that asked.
#[gpui::test]
fn the_vault_picker_switches_to_a_recent_vault(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("recent.json");
    setup(cx, &store_path);
    let (_ka, a) = vault_dir("alpha");
    let (_kb, b) = vault_dir("beta");
    // Both vaults get opened, so both are remembered; beta's window then
    // closes, so the switch is a real move and not a focus.
    let wa = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    let wb = cx
        .update(|cx| vaults::open_vault_window(&b, OpenOptions::default(), cx))
        .unwrap();
    cx.update(|cx| {
        wb.update(cx, |_ws, window, _cx| window.remove_window())
            .unwrap()
    });
    let mut vcx = VisualTestContext::from_window(wa.into(), cx);

    vcx.simulate_keystrokes(": v a u l t enter");
    assert_eq!(
        wa.update(&mut vcx, |ws, _w, cx| ws.picker_matches(cx)).unwrap(),
        Some(3),
        "the folder row and both vaults"
    );

    vcx.simulate_input("beta");
    vcx.simulate_keystrokes("enter");
    // The switch closed the window this context was riding, so the context
    // goes before the assertions do.
    drop(vcx);

    let opened = cx.update(|cx| vaults::open_vaults(cx));
    assert_eq!(
        opened,
        vec![b.canonicalize().unwrap()],
        "the chosen vault is the one open now"
    );
    assert!(
        wa.read_with(cx, |_ws, _cx| ()).is_err(),
        "and the window that asked closed"
    );
}

/// Picking the vault that is already on screen just keeps it: no second
/// window, and the asking window stays.
#[gpui::test]
fn picking_here_keeps_the_window(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("recent.json");
    setup(cx, &store_path);
    let (_ka, a) = vault_dir("alpha");
    let wa = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    let cx = &mut VisualTestContext::from_window(wa.into(), cx);

    cx.simulate_keystrokes(": v a u l t enter");
    assert_eq!(
        wa.update(cx, |ws, _w, cx| ws.picker_matches(cx)).unwrap(),
        Some(2),
        "the folder row and the vault on screen"
    );
    cx.simulate_input("alpha");
    cx.simulate_keystrokes("enter");

    assert_eq!(cx.update(|_window, cx| vaults::open_vaults(cx)).len(), 1);
    assert!(
        wa.read_with(cx, |_ws, _cx| ()).is_ok(),
        "the window is still there"
    );
}
