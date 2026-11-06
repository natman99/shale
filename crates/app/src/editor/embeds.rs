//! The `![[transclusions]]` in a note, resolved for the frame that paints them
//! (roadmap P4.0a).
//!
//! The resolution itself is `shale_core::transclude`'s — pure, tested, no gpui. What
//! lives here is the question of *when*: the host's text is re-scanned every frame
//! (cheap, and it must be, because the line the user just typed is an embed the frame
//! has to show), but the bodies come from other files, and re-reading those every frame
//! would put disk I/O in the paint path.
//!
//! So the bodies are cached, keyed by the reference the author wrote — target and
//! anchor together, because `![[a#One]]` and `![[a#Two]]` are different rows of the same
//! file. The cache is dropped whole when the vault reports a change. That is coarser
//! than invalidating one entry per changed file, and deliberately so: the watcher
//! already knows which paths moved, but a note's embeds are a handful of small reads,
//! and a stale embed body — a note showing another note as it was before it was edited
//! — is the kind of wrong that costs more trust than a few syscalls.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use shale_core::transclude::{self, Body};
use shale_core::vault::{Vault, VaultEntry};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    target: String,
    anchor: Option<String>,
}

/// The bodies this note's embeds resolve to, held across frames.
pub(crate) struct Cache {
    /// The vault's markdown notes, which the window already holds. Resolution is a
    /// table lookup against these; rebuilding the table per embed would cost more than
    /// reading the note.
    entries: Vec<VaultEntry>,
    /// The note holding the embeds, so the resolver's chain catches a self-embed.
    host: PathBuf,
    bodies: HashMap<Key, Body>,
}

impl Cache {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            host: PathBuf::new(),
            bodies: HashMap::new(),
        }
    }

    /// The vault's note list, refreshed whenever the window re-indexes.
    pub(crate) fn set_entries(&mut self, entries: Vec<VaultEntry>) {
        if entries != self.entries {
            self.entries = entries;
            // A note that appeared or went away can change what a target resolves to,
            // so the bodies keyed by the old list are no longer answers.
            self.bodies.clear();
        }
    }

    /// Which note this editor is showing. Its embeds are resolved relative to it, and
    /// it is the first link in the chain that stops an embed loop.
    ///
    /// Changing it drops the bodies: the host is an input to every answer, not just a
    /// label. A tab that moves from `a.md` to `b.md` must not keep showing `b`'s
    /// self-embed expanded because `a` was on screen when it was resolved.
    pub(crate) fn set_host(&mut self, host: &Path) {
        if host != self.host {
            self.host = host.to_path_buf();
            self.bodies.clear();
        }
    }

    /// Drop every body. Called when the vault reports a change, which is the coarse
    /// answer described in the module comment.
    pub(crate) fn invalidate(&mut self) {
        self.bodies.clear();
    }

    /// The embeds in `text`, in buffer-row order, with the body each one contributes.
    ///
    /// `Body::Nothing` rows are left out: an attachment embed is the image renderer's
    /// row, and a target that resolves to no note stays as the text the author wrote.
    pub(crate) fn resolve(&mut self, vault: &Vault, text: &str) -> Vec<(usize, Body)> {
        let mut out = Vec::new();
        for (row, line) in text.lines().enumerate() {
            let Some(embed) = transclude::line_embed(line) else {
                continue;
            };
            let key = Key {
                target: embed.target.clone(),
                anchor: embed.anchor.clone(),
            };
            let body = self
                .bodies
                .entry(key)
                .or_insert_with(|| transclude::resolve(vault, &self.entries, &self.host, &embed))
                .clone();
            if body != Body::Nothing {
                out.push((row, body));
            }
        }
        out
    }
}
