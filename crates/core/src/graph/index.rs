//! The vault's link graph as structure: nodes, edges, and the queries the
//! graph view and its filters need.
//!
//! Built from the in-memory vault index — the same
//! [`crate::markdown::extract_wikilinks`] + [`Vault::resolve_link`] path the
//! backlinks panel uses — so the graph is always consistent with what the
//! editor already knows. When the `.shale` SQLite DB lands (roadmap P1) edge
//! extraction moves to the DB's incremental index; this module keeps the
//! structure and the queries (graph-view-plan.md).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use petgraph::graph::{NodeIndex, UnGraph};

use crate::markdown::extract_wikilinks;
use crate::mdstyle::{style_spans, Style};
use crate::vault::{Vault, VaultEntry};

/// What a graph node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A markdown note in the vault.
    Note,
    /// A non-markdown file an embed or link points at.
    Attachment,
    /// A link target that resolves to no file; rendered dimmer, and dropped
    /// by the "existing files only" filter.
    Ghost,
    /// A `#tag`, linked to every note that carries it.
    Tag,
}

/// Per-node metadata the renderer and the filters read.
#[derive(Debug, Clone)]
pub struct NodeMeta {
    /// What this node is.
    pub kind: NodeKind,
    /// Vault-relative path; `None` for ghosts and tags.
    pub path: Option<PathBuf>,
    /// Display name: the note stem, the tag name without `#`, or the raw
    /// link target for a ghost.
    pub label: String,
}

/// Per-edge metadata.
#[derive(Debug, Clone, Copy)]
pub struct EdgeMeta {
    /// True when links run both ways between the endpoints — the renderer
    /// can draw a single line instead of two arrows.
    pub bidirectional: bool,
}

/// One resolved outgoing link from a note, before it becomes an edge.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Link {
    /// What the target turned out to be.
    kind: NodeKind,
    /// Vault-relative path for notes and attachments; `None` for ghosts.
    path: Option<PathBuf>,
    /// Node label (stem, tag name, or raw ghost target).
    label: String,
}

/// The vault's link graph.
///
/// Notes are nodes even when they link nothing (orphans); attachments, tags
/// and ghosts appear only when something links to them. A→B and B→A collapse
/// into one undirected edge carrying a [`EdgeMeta::bidirectional`] flag, and
/// self-links are ignored. The graph is rebuilt from the per-note link map on
/// every structural change — sub-millisecond at vault scale (≤2k notes) and
/// free of petgraph's index-shifting pitfalls on node removal.
#[derive(Debug, Clone)]
pub struct GraphIndex {
    /// The undirected graph; node payloads are [`NodeMeta`], edge payloads
    /// [`EdgeMeta`].
    graph: UnGraph<NodeMeta, EdgeMeta>,
    /// Every scanned note and its resolved outgoing links. The rebuild
    /// source: no file IO, no re-parsing.
    note_links: HashMap<PathBuf, Vec<Link>>,
    /// Vault-relative path to node, for incremental updates and click maps.
    by_path: HashMap<PathBuf, NodeIndex>,
    /// Tag name (no `#`) to node.
    by_tag: HashMap<String, NodeIndex>,
    /// Raw unresolved link target to its ghost node.
    by_ghost: HashMap<String, NodeIndex>,
}

impl GraphIndex {
    /// Build the graph by reading every note in `notes`. `attachments` are
    /// the vault's non-note files, used to resolve `![[image.png]]`-style
    /// targets to real nodes instead of ghosts.
    pub fn build(vault: &Vault, notes: &[VaultEntry], attachments: &[VaultEntry]) -> Result<Self> {
        let mut note_links = HashMap::new();
        for note in notes {
            let text = vault.read(&note.path)?;
            note_links.insert(
                note.path.clone(),
                scan_note_links(&text, vault, notes, attachments),
            );
        }
        let mut index = Self {
            graph: UnGraph::new_undirected(),
            note_links,
            by_path: HashMap::new(),
            by_tag: HashMap::new(),
            by_ghost: HashMap::new(),
        };
        index.rebuild();
        Ok(index)
    }

    /// The graph itself, for iteration and for the simulation.
    pub fn graph(&self) -> &UnGraph<NodeMeta, EdgeMeta> {
        &self.graph
    }

    /// Node count, for sizing the simulation's position arrays.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// The node for a vault-relative note/attachment path, if it is in the
    /// graph.
    pub fn node_for_path(&self, path: impl AsRef<Path>) -> Option<NodeIndex> {
        self.by_path.get(path.as_ref()).copied()
    }

    /// The node for a tag name (no `#`), if any note carries it.
    pub fn node_for_tag(&self, tag: &str) -> Option<NodeIndex> {
        self.by_tag.get(tag).copied()
    }

    /// The ghost node for a raw link target, if one exists.
    pub fn ghost_for_target(&self, target: &str) -> Option<NodeIndex> {
        self.by_ghost.get(target).copied()
    }

    /// Metadata for a node.
    pub fn meta(&self, node: NodeIndex) -> &NodeMeta {
        &self.graph[node]
    }

    /// Link count of a node — drives the node radius (`4 + 2*sqrt(degree)`).
    /// (petgraph 0.8 has no inherent `degree`; the neighbor list is one
    /// entry per edge, and this graph never holds parallel edges.)
    pub fn degree(&self, node: NodeIndex) -> u32 {
        self.graph.neighbors(node).count() as u32
    }

    /// The neighbours of a node, for hover highlighting and the local graph.
    pub fn neighbors(&self, node: NodeIndex) -> impl Iterator<Item = NodeIndex> + '_ {
        self.graph.neighbors(node)
    }

    /// Notes that link to `path` (the backlinks panel's question, answered
    /// from the graph instead of by rescanning the vault).
    pub fn backlinks_of(&self, path: impl AsRef<Path>) -> Vec<NodeIndex> {
        let Some(node) = self.node_for_path(path) else {
            return Vec::new();
        };
        self.graph
            .neighbors(node)
            .filter(|n| self.graph[*n].kind == NodeKind::Note)
            .collect()
    }

    /// Record `text` as the current content of note `path` (creating the
    /// note node if it is new) and rebuild the structure.
    ///
    /// The caller resolves targets against `notes`/`attachments` — the same
    /// lists [`Self::build`] was given — so a rename that adds or removes a
    /// file must pass the updated lists. Re-heating the simulation is the
    /// caller's job.
    pub fn set_note(
        &mut self,
        path: impl AsRef<Path>,
        text: &str,
        vault: &Vault,
        notes: &[VaultEntry],
        attachments: &[VaultEntry],
    ) {
        self.note_links.insert(
            path.as_ref().to_path_buf(),
            scan_note_links(text, vault, notes, attachments),
        );
        self.rebuild();
    }

    /// Drop a deleted note (and the ghosts/tags that only it linked to) and
    /// rebuild the structure.
    pub fn remove_note(&mut self, path: impl AsRef<Path>) {
        self.note_links.remove(path.as_ref());
        self.rebuild();
    }

    /// Rebuild the arena from `note_links`. Node indexes are only stable
    /// until the next rebuild; callers that cache indexes (the sim, the
    /// hit-test grid) must re-read them after any structural change.
    fn rebuild(&mut self) {
        self.graph = UnGraph::new_undirected();
        self.by_path.clear();
        self.by_tag.clear();
        self.by_ghost.clear();

        // Notes first, in sorted path order so the arena — and therefore the
        // simulation's initial phyllotaxis placement — is deterministic.
        let mut paths: Vec<&PathBuf> = self.note_links.keys().collect();
        paths.sort();
        for path in paths {
            let meta = NodeMeta {
                kind: NodeKind::Note,
                path: Some(path.clone()),
                label: path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string(),
            };
            let node = self.graph.add_node(meta);
            self.by_path.insert(path.clone(), node);
        }

        // Targets, deduped per kind, then edges. Directed pairs are kept so
        // the bidirectional flag can be set once all links are in.
        let mut directed: Vec<(NodeIndex, NodeIndex)> = Vec::new();
        for (path, links) in &self.note_links {
            let source = self.by_path[path];
            for link in links {
                let target = match link.kind {
                    NodeKind::Note | NodeKind::Attachment => {
                        match self.by_path.get(link.path.as_ref().unwrap()) {
                            Some(node) => *node,
                            None => {
                                // An attachment is not in note_links, so it
                                // joins the arena on first reference; a note
                                // path that is gone (deleted or renamed after
                                // this note was last scanned) degrades to a
                                // ghost keyed by the old path, exactly as it
                                // renders in the editor.
                                let path = link.path.as_ref().unwrap();
                                if link.kind == NodeKind::Attachment {
                                    let node = self.graph.add_node(NodeMeta {
                                        kind: NodeKind::Attachment,
                                        path: Some(path.clone()),
                                        label: path
                                            .file_name()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_string(),
                                    });
                                    self.by_path.insert(path.clone(), node);
                                    node
                                } else {
                                    let key = path.to_string_lossy().into_owned();
                                    match self.by_ghost.get(&key) {
                                        Some(node) => *node,
                                        None => {
                                            let node = self.graph.add_node(NodeMeta {
                                                kind: NodeKind::Ghost,
                                                path: None,
                                                label: key.clone(),
                                            });
                                            self.by_ghost.insert(key, node);
                                            node
                                        }
                                    }
                                }
                            }
                        }
                    }
                    NodeKind::Tag => match self.by_tag.get(&link.label) {
                        Some(node) => *node,
                        None => {
                            let node = self.graph.add_node(NodeMeta {
                                kind: NodeKind::Tag,
                                path: None,
                                label: link.label.clone(),
                            });
                            self.by_tag.insert(link.label.clone(), node);
                            node
                        }
                    },
                    NodeKind::Ghost => match self.by_ghost.get(&link.label) {
                        Some(node) => *node,
                        None => {
                            let node = self.graph.add_node(NodeMeta {
                                kind: NodeKind::Ghost,
                                path: None,
                                label: link.label.clone(),
                            });
                            self.by_ghost.insert(link.label.clone(), node);
                            node
                        }
                    },
                };
                if target == source {
                    continue; // self-links are structure-free
                }
                directed.push((source, target));
            }
        }

        let mut seen: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();
        for (a, b) in directed {
            let pair = (a, b);
            if !seen.insert(pair) {
                continue; // the same link written twice in one note
            }
            // The reverse link may have arrived first and already added the
            // edge (the graph is undirected, so find_edge sees it either way):
            // mark that edge bidirectional instead of adding a parallel one.
            if let Some(edge) = self.graph.find_edge(a, b) {
                self.graph[edge].bidirectional = true;
                continue;
            }
            self.graph.add_edge(
                a,
                b,
                EdgeMeta {
                    bidirectional: seen.contains(&(b, a)),
                },
            );
        }
    }
}

/// Resolve every link target in one note's `text` to a [`Link`], deduped.
///
/// Wiki-links go through [`Vault::resolve_link`] (notes), then
/// [`Vault::resolve_attachment`] (embeds and `[[file.png]]` links), and what
/// resolves to neither becomes a ghost keyed by its raw target. Tags come
/// from the style scanner, so fences and inline code are already skipped.
fn scan_note_links(
    text: &str,
    vault: &Vault,
    notes: &[VaultEntry],
    attachments: &[VaultEntry],
) -> Vec<Link> {
    let mut links: Vec<Link> = Vec::new();
    let mut seen: HashSet<(NodeKind, Option<PathBuf>, String)> = HashSet::new();

    for wikilink in extract_wikilinks(text) {
        let target = wikilink.target.trim();
        if target.is_empty() {
            continue;
        }
        let link = if let Some(entry) = vault.resolve_link(target, notes).unwrap_or(None) {
            Link {
                kind: NodeKind::Note,
                path: Some(entry.path),
                label: String::new(),
            }
        } else if let Some(entry) = vault.resolve_attachment(target, attachments) {
            Link {
                kind: NodeKind::Attachment,
                path: Some(entry.path),
                label: String::new(),
            }
        } else {
            Link {
                kind: NodeKind::Ghost,
                path: None,
                label: target.to_string(),
            }
        };
        // Notes and attachments are keyed by path (one node per file);
        // ghosts by their raw target string.
        let key = (link.kind, link.path.clone(), link.label.clone());
        if seen.insert(key) {
            links.push(link);
        }
    }

    for tag in extract_tags(text) {
        let link = Link {
            kind: NodeKind::Tag,
            path: None,
            label: tag,
        };
        let key = (link.kind, link.path.clone(), link.label.clone());
        if seen.insert(key) {
            links.push(link);
        }
    }

    links
}

/// The `#tags` in `text`, without the leading `#`, in order of first
/// appearance and deduped.
///
/// The tag spans come from [`style_spans`], which already skips code fences,
/// inline code, and `#`-prefixed headings, and already accepts the Obsidian
/// tag grammar (internal hyphens, underscores, and `/` for nested tags).
pub fn extract_tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for span in style_spans(text) {
        if span.style != Style::Tag {
            continue;
        }
        let name = &text[span.start..span.end].trim_start_matches('#');
        if name.is_empty() {
            continue;
        }
        if seen.insert((*name).to_string()) {
            out.push(name.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// The vault fixture from `vault.rs`'s tests, restated here so the graph
    /// tests do not reach into another module's private helper.
    fn fixture() -> Result<(tempfile::TempDir, Vault)> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("notes"))?;
        fs::create_dir_all(root.join("assets"))?;
        fs::write(
            root.join("Welcome.md"),
            "Link to [[Another Note]]. #greeting",
        )?;
        fs::write(
            root.join("notes/Another Note.md"),
            "Back to [[welcome]] and ![[Diagram.png]] and [[nowhere]].",
        )?;
        fs::write(root.join("assets/Diagram.png"), "not really a png")?;
        Ok((dir, Vault::open(root)?))
    }

    fn build(vault: &Vault) -> Result<GraphIndex> {
        GraphIndex::build(vault, &vault.list_markdown()?, &vault.list_attachments()?)
    }

    #[test]
    fn links_resolve_and_collapse_both_ways() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let index = build(&vault)?;

        let welcome = index.node_for_path("Welcome.md").unwrap();
        let another = index.node_for_path("notes/Another Note.md").unwrap();
        let edge = index.graph().find_edge(welcome, another).expect("one edge");
        assert!(
            index.graph()[edge].bidirectional,
            "Welcome links Another Note and back via [[welcome]]"
        );
        assert_eq!(index.degree(welcome), 2, "one note + one tag");
        Ok(())
    }

    #[test]
    fn embeds_become_attachment_nodes_and_ghosts_stay_dim() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let index = build(&vault)?;

        let diagram = index
            .node_for_path("assets/Diagram.png")
            .expect("the embed resolves to the attachment");
        assert_eq!(index.meta(diagram).kind, NodeKind::Attachment);

        let nowhere = index.ghost_for_target("nowhere").expect("unresolved link");
        assert_eq!(index.meta(nowhere).kind, NodeKind::Ghost);
        assert_eq!(index.meta(nowhere).label, "nowhere");
        Ok(())
    }

    #[test]
    fn tags_link_to_their_notes() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let index = build(&vault)?;

        let tag = index.node_for_tag("greeting").expect("the tag node");
        assert_eq!(index.meta(tag).kind, NodeKind::Tag);
        let welcome = index.node_for_path("Welcome.md").unwrap();
        assert!(index.graph().find_edge(welcome, tag).is_some());
        Ok(())
    }

    #[test]
    fn duplicate_stems_resolve_to_the_shortest_path() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        fs::create_dir_all(root.join("deep/deeper"))?;
        fs::write(root.join("Note.md"), "I am the short one.")?;
        fs::write(root.join("deep/deeper/Note.md"), "I am deep.")?;
        fs::write(root.join("Index.md"), "Pointing at [[Note]].")?;
        let vault = Vault::open(root)?;
        let index = build(&vault)?;

        let short = index.node_for_path("Note.md").unwrap();
        let deep = index.node_for_path("deep/deeper/Note.md").unwrap();
        let source = index.node_for_path("Index.md").unwrap();
        assert!(index.graph().find_edge(source, short).is_some());
        assert!(index.graph().find_edge(source, deep).is_none());
        Ok(())
    }

    #[test]
    fn self_links_are_ignored() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        fs::write(root.join("Solo.md"), "Myself: [[Solo]] and [[solo]].")?;
        let vault = Vault::open(root)?;
        let index = build(&vault)?;
        let solo = index.node_for_path("Solo.md").unwrap();
        assert_eq!(
            index.degree(solo),
            0,
            "a note linking only itself is an orphan"
        );
        Ok(())
    }

    #[test]
    fn set_note_moves_edges_without_rescanning_files() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let notes = vault.list_markdown()?;
        let attachments = vault.list_attachments()?;
        let mut index = GraphIndex::build(&vault, &notes, &attachments)?;

        // Sever Welcome's link to Another Note; the ghost it gains must show.
        index.set_note(
            "Welcome.md",
            "Now pointing at [[gone]].",
            &vault,
            &notes,
            &attachments,
        );
        let welcome = index.node_for_path("Welcome.md").unwrap();
        let another = index.node_for_path("notes/Another Note.md").unwrap();
        // The undirected edge survives — Another Note still links back — but
        // it is one-way now.
        let edge = index
            .graph()
            .find_edge(welcome, another)
            .expect("the reverse link keeps the edge");
        assert!(
            !index.graph()[edge].bidirectional,
            "Welcome's own link is gone, so the edge is one-way"
        );
        assert!(index.ghost_for_target("gone").is_some());
        assert!(index.node_for_tag("greeting").is_none(), "the tag left too");
        Ok(())
    }

    #[test]
    fn remove_note_drops_it_and_its_orphaned_ghosts() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let mut index = build(&vault)?;
        index.remove_note("notes/Another Note.md");
        assert!(index.node_for_path("notes/Another Note.md").is_none());
        assert!(
            index.ghost_for_target("nowhere").is_none(),
            "the ghost only that note linked to is gone"
        );
        assert!(
            index.node_for_path("assets/Diagram.png").is_none(),
            "the attachment too"
        );
        Ok(())
    }

    #[test]
    fn a_link_to_a_deleted_note_degrades_to_a_ghost() -> Result<()> {
        let (_dir, vault) = fixture()?;
        let notes = vault.list_markdown()?;
        let attachments = vault.list_attachments()?;
        let mut index = GraphIndex::build(&vault, &notes, &attachments)?;

        // Another Note is gone from the vault but Welcome's scan still points
        // at it: the edge survives, aimed at a ghost, not a dangling index.
        index
            .note_links
            .remove(std::path::Path::new("notes/Another Note.md"));
        index.rebuild();
        let welcome = index.node_for_path("Welcome.md").unwrap();
        let ghosts: Vec<_> = index
            .graph()
            .neighbors(welcome)
            .filter(|n| index.meta(*n).kind == NodeKind::Ghost)
            .collect();
        assert_eq!(ghosts.len(), 1, "the stale link became a ghost");
        assert_eq!(index.meta(ghosts[0]).label, "notes/Another Note.md");
        Ok(())
    }

    #[test]
    fn tag_extraction_follows_the_style_scanner() {
        let text =
            "# heading, not a tag\na #tag and #nested/path and `#inline`\n```\n#fenced\n```\n";
        let tags = extract_tags(text);
        assert_eq!(tags, vec!["tag", "nested/path"]);
    }
}
