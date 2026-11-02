//! What one MCP profile is allowed to touch (roadmap P2.4).
//!
//! A profile is the answer to "let the agent have free rein over my drafts without
//! letting it near my journal". It is a name, some path patterns, and a tool
//! allowlist, written in the vault's config and chosen when the server starts
//! (`shale-mcp --profile writing-help`).
//!
//! Two rules hold this together, both from the binding MCP operational rules:
//!
//! - **Enforcement is server-side only.** A client that claims a scope gets what the
//!   server's profile says, nothing more. Every path still goes through
//!   [`crate::Vault`]'s normalisation and confinement; a profile narrows what is
//!   inside the vault, it does not replace the wall around it.
//! - **`deny` beats `write`,** and beats `read` too. A path any `deny` pattern covers
//!   is not writable, not readable, and not listed — it is not there.
//!
//! The patterns themselves are [`crate::glob`]'s vocabulary: `journal` (that folder
//! and everything in it), `inbox/**`, `*.md`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::glob;
use crate::vault::VaultEntry;

/// One named agent scope, as `[mcp.profile."writing-help"]` spells it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpProfile {
    /// Which paths the agent may see: read, and be told about by `list_notes`,
    /// `search_notes` and `backlinks`. A path no pattern here covers is invisible.
    /// Defaults to the whole vault.
    pub read: Vec<String>,
    /// Which paths the agent may write. Independent of [`Self::read`] on purpose: a
    /// capture agent that may append to `inbox/` but never read it is a thing a user
    /// legitimately wants. Defaults to the whole vault.
    pub write: Vec<String>,
    /// Paths the agent may not touch at all. Checked first, so it wins over both
    /// other lists however broadly they are written.
    pub deny: Vec<String>,
    /// The tools this agent may call. Empty means every tool the server has, which
    /// is what an unprofiled connection gets; naming any tool narrows the list to
    /// the names given.
    pub tools: Vec<String>,
    /// Whether the agent's edits queue for the user's review instead of landing in
    /// the note. **Not honoured yet** — the review path is roadmap P2.5 — so a
    /// profile that asks for it has its writes refused rather than quietly written
    /// straight through.
    pub review: bool,
}

impl Default for McpProfile {
    /// The whole vault, every tool, no review: what `shale-mcp` has always been, and
    /// what a connection with no `--profile` still gets.
    fn default() -> Self {
        Self {
            read: vec!["**".to_string()],
            write: vec!["**".to_string()],
            deny: Vec::new(),
            tools: Vec::new(),
            review: false,
        }
    }
}

impl McpProfile {
    /// May this agent see `path` at all? A `deny` match ends the question.
    pub fn can_read(&self, path: &Path) -> bool {
        !denied(self, path) && self.read.iter().any(|p| glob::covers(p, path))
    }

    /// May this agent write `path`? `deny` wins here too, which is what makes
    /// `write = ["**"] deny = ["journal"]` mean what it says.
    pub fn can_write(&self, path: &Path) -> bool {
        !denied(self, path) && self.write.iter().any(|p| glob::covers(p, path))
    }

    /// Is `name` on this profile's list? An empty list is every tool.
    pub fn allows_tool(&self, name: &str) -> bool {
        self.tools.is_empty() || self.tools.iter().any(|tool| tool == name)
    }

    /// The entries this agent is allowed to know about, in the order given. This is
    /// one place on purpose: `list_notes`, `search_notes` and `backlinks` all ask it,
    /// so a listing cannot leak through a path a read would have refused.
    pub fn visible(&self, entries: &[VaultEntry]) -> Vec<VaultEntry> {
        entries
            .iter()
            .filter(|entry| self.can_read(&entry.path))
            .cloned()
            .collect()
    }
}

/// Does any `deny` pattern cover `path`?
fn denied(profile: &McpProfile, path: &Path) -> bool {
    profile.deny.iter().any(|p| glob::covers(p, path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scope(read: &[&str], write: &[&str], deny: &[&str]) -> McpProfile {
        McpProfile {
            read: read.iter().map(|p| (*p).to_string()).collect(),
            write: write.iter().map(|p| (*p).to_string()).collect(),
            deny: deny.iter().map(|p| (*p).to_string()).collect(),
            tools: Vec::new(),
            review: false,
        }
    }

    fn path(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    fn entry(text: &str) -> VaultEntry {
        VaultEntry { path: path(text) }
    }

    /// No profile at all is today's behaviour: the whole vault, every tool.
    #[test]
    fn the_default_profile_is_the_whole_vault() {
        let profile = McpProfile::default();
        assert!(profile.can_read(&path("journal/february.md")));
        assert!(profile.can_write(&path("journal/february.md")));
        assert!(profile.allows_tool("write_note"));
        assert!(profile.allows_tool("a_tool_invented_next_release"));
    }

    /// The point of the feature: free rein over drafts, nothing near the journal.
    #[test]
    fn a_folder_scope_holds_its_boundary() {
        let profile = scope(&["inbox/", "drafts/"], &["inbox/", "drafts/"], &[]);
        assert!(profile.can_read(&path("inbox/idea.md")));
        assert!(profile.can_read(&path("drafts/deep/idea.md")));
        assert!(!profile.can_read(&path("journal/february.md")));
        assert!(!profile.can_write(&path("journal/february.md")));
        assert!(!profile.can_write(&path("notes/idea.md")));
    }

    /// The rule the operational list calls binding, as a test.
    #[test]
    fn deny_beats_read_and_write() {
        let profile = scope(&["**"], &["**"], &["journal"]);
        assert!(!profile.can_read(&path("journal/february.md")));
        assert!(!profile.can_write(&path("journal/february.md")));
        assert!(profile.can_read(&path("inbox/idea.md")));

        // Denying a single note, not a folder: its neighbours are untouched.
        let profile = scope(&["**"], &["**"], &["secrets.md"]);
        assert!(!profile.can_read(&path("secrets.md")));
        assert!(profile.can_read(&path("secrets-other.md")));
    }

    /// Writing without reading is a real shape (a capture agent), so the two lists
    /// are independent rather than one implying the other.
    #[test]
    fn write_does_not_imply_read() {
        let profile = scope(&[], &["inbox/"], &[]);
        assert!(!profile.can_read(&path("inbox/idea.md")));
        assert!(profile.can_write(&path("inbox/idea.md")));
    }

    #[test]
    fn a_named_tool_list_narrows_to_the_names_given() {
        let profile = McpProfile {
            tools: vec!["read_note".into(), "search_notes".into()],
            ..McpProfile::default()
        };
        assert!(profile.allows_tool("read_note"));
        assert!(profile.allows_tool("search_notes"));
        assert!(!profile.allows_tool("write_note"));
        assert!(!profile.allows_tool("live_note"));
    }

    /// A listing is where a scope leaks if it is forgotten, so it goes through the
    /// same check as a read — including the `deny` half.
    #[test]
    fn a_listing_shows_only_what_the_profile_may_see() {
        let entries = [
            entry("inbox/idea.md"),
            entry("journal/february.md"),
            entry("private/notes.md"),
        ];
        let profile = scope(&["**"], &[], &["private"]);
        let visible = profile.visible(&entries);
        let paths: Vec<PathBuf> = visible.iter().map(|e| e.path.clone()).collect();
        assert_eq!(
            paths,
            vec![path("inbox/idea.md"), path("journal/february.md")]
        );
    }
}
