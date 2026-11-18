//! The vault's two beats, driven for real: the watcher walk and the
//! pending-queue read (roadmap P2.5a's queue, P3.4's watcher).
//!
//! Every other test reaches these features through the front door —
//! `check_vault_now` for a manual walk, `scan_pending` for a manual read —
//! which tests the folding but not the loops that fold on their own. These
//! two drive the actual loops: a registry window (the beats belong to the
//! vault's services, and only `open_vault_window` starts them), a disk that
//! changes under it, and the change arriving unasked.
//!
//! The beats' timers are the test scheduler's fake clock, so the waiting is
//! `advance_clock` in beat-sized steps: deterministic, no wall-clock sleeps,
//! and the number of steps states the promise — an outside edit shows up
//! within a beat, a proposal within a beat of landing.

use super::*;
use crate::workspace::vaults::{self, OpenOptions};

/// A registry vault with one empty note and the checker, index and live MCP
/// off (the same fixture reasons as `tests::exit`), opened through
/// `open_vault_window` so the vault's services — and with them both beats —
/// are running.
fn registry_vault(
    cx: &mut TestAppContext,
    name: &str,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    WindowHandle<Workspace>,
) {
    let base = tempfile::tempdir().unwrap();
    let dir = base.path().join(name);
    std::fs::create_dir_all(dir.join(".shale")).unwrap();
    std::fs::write(dir.join("a.md"), "").unwrap();
    std::fs::write(
        dir.join(".shale").join("config.toml"),
        "[spellcheck]\nenabled = false\n[index]\nenabled = false\n[mcp]\nlive = false\n",
    )
    .unwrap();
    cx.update(|cx| {
        cx.bind_keys(keybindings());
        cx.set_global(vaults::VaultWindows::new());
        cx.set_global(vaults::RecentStore(base.path().join("recent.json")));
    });
    let window = cx
        .update(|cx| vaults::open_vault_window(&dir, OpenOptions::default(), cx))
        .unwrap();
    (base, dir, window)
}

/// Step the fake clock until `ready` says so. Forty half-second steps cover
/// several beats of both loops (the 750 ms walk floor and the 1 s queue
/// read), so the test fails only when the beat is truly missing.
fn wait_for(
    cx: &mut TestAppContext,
    what: &str,
    mut ready: impl FnMut(&mut TestAppContext) -> bool,
) {
    for _ in 0..40 {
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(500));
        cx.run_until_parked();
        if ready(cx) {
            return;
        }
    }
    panic!("{what} — the beat never answered");
}

/// The watcher's promise, end to end: a note made by another program gets a
/// row without anyone asking — and the beat keeps its clock, because a
/// *second* outside note gets a row too. This is the regression gate for the
/// watcher's unchanged-skip: a walk that compares against the last one must
/// still fold every walk that actually differs.
#[gpui::test]
fn the_watcher_rows_outside_notes_beat_after_beat(cx: &mut TestAppContext) {
    let (_base, vault, window) = registry_vault(cx, "alpha");

    std::fs::write(vault.join("Elsewhere.md"), "made by another program\n").unwrap();
    wait_for(
        cx,
        "the tree never picked up the first outside note",
        |cx| {
            window
                .update(cx, |ws, _window, _cx| {
                    ws.tree_rows()
                        .iter()
                        .any(|row| row == std::path::Path::new("Elsewhere.md"))
                })
                .unwrap_or(false)
        },
    );

    std::fs::write(vault.join("Later.md"), "and then another\n").unwrap();
    wait_for(cx, "the tree stopped after the first outside note", |cx| {
        window
            .update(cx, |ws, _window, _cx| {
                ws.tree_rows()
                    .iter()
                    .any(|row| row == std::path::Path::new("Later.md"))
            })
            .unwrap_or(false)
    });
}

/// The queue beat's promise, end to end: a proposal an agent drops on disk
/// reaches the review strip on its own, within a beat. The regression gate
/// for the queue watch's unchanged-skip: a read that compares against the
/// last one must still fan out every read that differs.
#[gpui::test]
fn the_queue_beat_lands_a_proposal_in_the_strip(cx: &mut TestAppContext) {
    let (_base, vault, window) = registry_vault(cx, "alpha");

    let edit = shale_core::pending::PendingEdit {
        id: "beat-1".to_string(),
        note: PathBuf::from("a.md"),
        profile: "agent".to_string(),
        tool: "write_note".to_string(),
        created_at: 1,
        hunks: vec![shale_core::diff::Hunk {
            old_start: 1,
            old_len: 0,
            new_start: 1,
            new_len: 1,
            final_newline: true,
            text: "@@ -0,0 +1,1 @@\n+a proposed line".to_string(),
        }],
    };
    shale_core::pending::write(&vault, &edit).unwrap();

    wait_for(cx, "the strip never showed the proposal", |cx| {
        window
            .update(cx, |ws, _window, cx| ws.review_active(cx))
            .unwrap_or(false)
    });
}
