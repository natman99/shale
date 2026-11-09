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
    let mut want = vec![a.canonicalize().unwrap(), b.canonicalize().unwrap()];
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
        vec![b.canonicalize().unwrap(), a.canonicalize().unwrap()],
        "most recent first"
    );
    assert_eq!(
        recent.last.as_deref(),
        Some(b.canonicalize().unwrap().as_path())
    );
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
        window
            .update(cx, |_ws, window, _cx| window.remove_window())
            .unwrap();
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
        wa.update(&mut vcx, |ws, _w, cx| ws.picker_matches(cx))
            .unwrap(),
        Some(3),
        "the folder row and both vaults"
    );

    vcx.simulate_input("beta");
    vcx.simulate_keystrokes("enter");
    // The asking window stays open — closing it is the user's call — so
    // the new vault's window simply opens beside it.
    drop(vcx);

    let mut opened = cx.update(|cx| vaults::open_vaults(cx));
    opened.sort();
    let mut both = vec![a.canonicalize().unwrap(), b.canonicalize().unwrap()];
    both.sort();
    assert_eq!(opened, both, "both vaults have windows now");
    assert!(
        wa.read_with(cx, |_ws, _cx| ()).is_ok(),
        "the window that asked stays"
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

/// The per-window half of the session file. Two windows on one vault each
/// keep their own room — different notes, different slots — and a save from
/// one never writes over the other. Close both, reopen, and each window wakes
/// up where the window that held its slot left it.
#[gpui::test]
async fn two_windows_remember_their_own_rooms(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    setup(cx, &store.path().join("recent.json"));
    let (_dir, vault) = vault_dir("per_window_sessions");
    std::fs::write(vault.join("alpha.md"), "# Alpha\n").unwrap();
    std::fs::write(vault.join("beta.md"), "# Beta\n").unwrap();

    let a = cx
        .update(|cx| vaults::open_vault_window(&vault, OpenOptions::default(), cx))
        .unwrap();
    // The second window comes in the way a second window really arrives:
    // `:new-window`, sharing the vault's services.
    let b = cx
        .update(|cx| vaults::open_extra_window(&vault, cx))
        .unwrap();
    a.update(cx, |ws, _window, cx| {
        ws.open_note(&PathBuf::from("alpha.md"), cx)
    })
    .unwrap();
    b.update(cx, |ws, _window, cx| {
        ws.open_note(&PathBuf::from("beta.md"), cx)
    })
    .unwrap();
    // The windows have to be painted once before their sessions carry a
    // frame: render is where the bounds are noticed.
    for handle in [a, b] {
        let mut vcx = VisualTestContext::from_window(handle.into(), cx);
        for _ in 0..2 {
            vcx.update(|window, cx| window.draw(cx).clear(cx));
        }
    }
    a.update(cx, |ws, _window, cx| ws.save_session(cx)).unwrap();
    b.update(cx, |ws, _window, cx| ws.save_session(cx)).unwrap();

    // One file, two slots, and the frame each window sat in rides along.
    let slot_of = |w: gpui::WindowHandle<crate::workspace::Workspace>, cx: &mut TestAppContext| {
        w.update(cx, |ws, _window, _cx| ws.session_slot()).unwrap()
    };
    let (sa, sb) = (slot_of(a, cx), slot_of(b, cx));
    assert_ne!(sa, sb, "two windows, two slots");
    let sessions = shale_core::Sessions::load(&vault).unwrap();
    assert_eq!(sessions.slots.len(), sb + 1);
    assert_eq!(
        sessions.slots[sa].tabs[0].path,
        Some(PathBuf::from("alpha.md"))
    );
    assert_eq!(
        sessions.slots[sb].tabs[0].path,
        Some(PathBuf::from("beta.md"))
    );
    let geom = sessions.slots[sa].window.clone().expect("a frame is saved");
    assert_eq!((geom.w, geom.h), (1200., 800.));

    // Close both, reopen: slot 0 remembers alpha, slot 1 remembers beta.
    a.update(cx, |_ws, window, _cx| window.remove_window())
        .unwrap();
    b.update(cx, |_ws, window, _cx| window.remove_window())
        .unwrap();
    let a2 = cx
        .update(|cx| vaults::open_vault_window(&vault, OpenOptions::default(), cx))
        .unwrap();
    let b2 = cx
        .update(|cx| vaults::open_extra_window(&vault, cx))
        .unwrap();
    let room = |w: gpui::WindowHandle<crate::workspace::Workspace>, cx: &mut TestAppContext| {
        w.update(cx, |ws, _window, _cx| {
            ws.pane()
                .tabs
                .iter()
                .map(|t| t.path().map(|p| p.to_path_buf()))
                .collect::<Vec<_>>()
        })
        .unwrap()
    };
    assert_eq!(room(a2, cx), vec![Some(PathBuf::from("alpha.md"))]);
    assert_eq!(room(b2, cx), vec![Some(PathBuf::from("beta.md"))]);
}

/// Two windows on one vault: two frames, one set of services. The claim
/// stays single, the watcher walks once, the MCP channel answers once —
/// which is the whole reason a second *window* is allowed where a second
/// *process* is refused.
#[gpui::test]
fn two_windows_on_one_vault_share_one_services(cx: &mut TestAppContext) {
    let store = tempfile::tempdir().unwrap();
    let store_path = store.path().join("recent.json");
    setup(cx, &store_path);
    let (_ka, a) = vault_dir("alpha");
    let root = a.canonicalize().unwrap();

    let w1 = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    let w2 = cx.update(|cx| vaults::open_extra_window(&a, cx)).unwrap();
    assert_ne!(w1.window_id(), w2.window_id(), "two frames");
    let (wins, services) = cx.read(|cx| {
        let g = cx.global::<vaults::VaultWindows>();
        (g.window_count(&root), g.services_up(&root))
    });
    assert_eq!(
        (wins, services),
        (2, true),
        "one vault, one set of services"
    );

    // Closing one leaves the other fully alive: the services and the claim
    // go only with the last window.
    cx.update(|cx| {
        w1.update(cx, |_ws, window, _| window.remove_window())
            .unwrap()
    });
    let (wins, services) = cx.read(|cx| {
        let g = cx.global::<vaults::VaultWindows>();
        (g.window_count(&root), g.services_up(&root))
    });
    assert_eq!(
        (wins, services),
        (1, true),
        "the survivor keeps the vault open"
    );
    w2.read_with(cx, |_ws, _cx| ()).unwrap();

    // And the last one out takes the claim with it: the vault opens fresh.
    cx.update(|cx| {
        w2.update(cx, |_ws, window, _| window.remove_window())
            .unwrap()
    });
    let (wins, services) = cx.read(|cx| {
        let g = cx.global::<vaults::VaultWindows>();
        (g.window_count(&root), g.services_up(&root))
    });
    assert_eq!((wins, services), (0, false), "the vault is fully released");
    let w3 = cx
        .update(|cx| vaults::open_vault_window(&a, OpenOptions::default(), cx))
        .unwrap();
    w3.read_with(cx, |_ws, _cx| ()).unwrap();
}
