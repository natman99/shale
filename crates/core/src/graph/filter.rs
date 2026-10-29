//! The graph filters as a pure view over the [`GraphIndex`]: a visibility
//! bitmask the simulation and the renderer both consult, so a filtered-out
//! node takes part in neither the physics nor the paint (graph-view-plan.md
//! Task 4). Re-running the filter is cheap; re-heating the sim on a filter
//! change is the caller's job.

use petgraph::graph::NodeIndex;

use crate::fuzzy::fuzzy_match;
use crate::graph::index::{GraphIndex, NodeKind};

/// What to show. Defaults: everything on (Obsidian's default graph).
#[derive(Debug, Clone)]
pub struct GraphFilter {
    /// Show notes that link nothing and are linked by nothing.
    pub orphans: bool,
    /// Hide ghost nodes — links to files that do not exist.
    pub existing_only: bool,
    /// Show `#tag` nodes.
    pub tags: bool,
    /// Fuzzy match against the node label and path; a node that does not
    /// match is hidden (illumination mode, M5f, is a render-side variant of
    /// the same match, not a different filter).
    pub search: Option<String>,
    /// Constellation scope (M3): only notes under this vault-relative folder,
    /// plus the tags and ghosts attached to them. `None` is the whole vault.
    pub folder: Option<String>,
}

impl Default for GraphFilter {
    fn default() -> Self {
        Self {
            orphans: true,
            existing_only: false,
            tags: true,
            search: None,
            folder: None,
        }
    }
}

/// The nodes that pass `filter`, as a bitmask indexed by
/// `NodeIndex::index()`. Edges need no mask: an edge is drawn exactly when
/// both endpoints are visible, which drops ghost edges with the ghosts.
pub fn apply(index: &GraphIndex, filter: &GraphFilter) -> Vec<bool> {
    let n = index.node_count();
    let mut visible = vec![true; n];

    // Folder scope first: notes and attachments by path prefix...
    if let Some(folder) = &filter.folder {
        let prefix = folder.trim_end_matches('/').to_string();
        for i in 0..n {
            let node = NodeIndex::new(i);
            let meta = index.meta(node);
            match meta.kind {
                NodeKind::Note | NodeKind::Attachment => {
                    let path = meta.path.as_ref().map(|p| p.to_string_lossy().into_owned());
                    let inside = match path {
                        Some(p) => p == prefix || p.starts_with(&format!("{prefix}/")),
                        None => false,
                    };
                    visible[i] = inside;
                }
                NodeKind::Tag | NodeKind::Ghost => {
                    // Decided by their neighbours, below.
                    visible[i] = true;
                }
            }
        }
        // ...then tags and ghosts survive only if a visible note links them.
        for i in 0..n {
            let node = NodeIndex::new(i);
            if matches!(index.meta(node).kind, NodeKind::Tag | NodeKind::Ghost)
                && !index.neighbors(node).any(|nb| visible[nb.index()])
            {
                visible[i] = false;
            }
        }
    }

    for i in 0..n {
        let node = NodeIndex::new(i);
        let meta = index.meta(node);
        match meta.kind {
            NodeKind::Ghost if filter.existing_only => visible[i] = false,
            NodeKind::Tag if !filter.tags => visible[i] = false,
            NodeKind::Note if !filter.orphans && index.degree(node) == 0 => visible[i] = false,
            _ => {}
        }
    }

    if let Some(query) = &filter.search {
        if !query.trim().is_empty() {
            for i in 0..n {
                if !visible[i] {
                    continue;
                }
                let node = NodeIndex::new(i);
                let meta = index.meta(node);
                let label = &meta.label;
                let path = meta
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let matched = fuzzy_match(query, label).is_some()
                    || (!path.is_empty() && fuzzy_match(query, &path).is_some());
                if !matched {
                    visible[i] = false;
                }
            }
        }
    }

    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use std::fs;

    fn fixture() -> anyhow::Result<(tempfile::TempDir, Vault, GraphIndex)> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        fs::create_dir_all(root.join("child 1/pie"))?;
        fs::create_dir_all(root.join("child 1/cake"))?;
        fs::create_dir_all(root.join("child 2"))?;
        fs::write(root.join("root.md"), "to [[pie/p]] and [[gone]]. #top")?;
        fs::write(root.join("child 1/pie/p.md"), "back to [[root]] #sweet")?;
        fs::write(root.join("child 1/cake/c.md"), "lonely cake")?;
        fs::write(root.join("child 2/x.md"), "to [[p]]")?;
        fs::write(root.join("lonely.md"), "an orphan note")?;
        let vault = Vault::open(root)?;
        let index = GraphIndex::build(&vault, &vault.list_markdown()?, &vault.list_attachments()?)?;
        Ok((dir, vault, index))
    }

    fn count(visible: &[bool]) -> usize {
        visible.iter().filter(|v| **v).count()
    }

    #[test]
    fn default_shows_everything() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let visible = apply(&index, &GraphFilter::default());
        assert_eq!(count(&visible), index.node_count());
        Ok(())
    }

    #[test]
    fn orphans_toggle_hides_the_lonely_note() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            orphans: false,
            ..Default::default()
        };
        let visible = apply(&index, &f);
        let lonely = index.node_for_path("lonely.md").unwrap();
        assert!(!visible[lonely.index()]);
        let cake = index.node_for_path("child 1/cake/c.md").unwrap();
        assert!(!visible[cake.index()], "cake links nothing either");
        Ok(())
    }

    #[test]
    fn existing_only_drops_ghosts_and_their_edges_follow() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            existing_only: true,
            ..Default::default()
        };
        let visible = apply(&index, &f);
        let ghost = index.ghost_for_target("gone").unwrap();
        assert!(!visible[ghost.index()]);
        // The renderer draws an edge only when both ends are visible, so the
        // root->gone edge is gone with it; assert the ghost had an edge to
        // prove the mechanism has something to drop.
        let root = index.node_for_path("root.md").unwrap();
        assert!(index.graph().find_edge(root, ghost).is_some());
        Ok(())
    }

    #[test]
    fn tags_toggle_hides_tag_nodes() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            tags: false,
            ..Default::default()
        };
        let visible = apply(&index, &f);
        assert!(!visible[index.node_for_tag("top").unwrap().index()]);
        assert!(!visible[index.node_for_tag("sweet").unwrap().index()]);
        Ok(())
    }

    #[test]
    fn search_matches_label_and_path() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            search: Some("cake".into()), // matches the path, not the label
            ..Default::default()
        };
        let visible = apply(&index, &f);
        let cake = index.node_for_path("child 1/cake/c.md").unwrap();
        assert!(
            visible[cake.index()],
            "the label is 'c' but the path contains 'cake'"
        );
        assert!(!visible[index.node_for_path("lonely.md").unwrap().index()]);
        Ok(())
    }

    #[test]
    fn folder_scope_keeps_children_and_their_tags_not_the_rest() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            folder: Some("child 1".into()),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        assert!(visible[index.node_for_path("child 1/pie/p.md").unwrap().index()]);
        assert!(visible[index.node_for_path("child 1/cake/c.md").unwrap().index()]);
        assert!(!visible[index.node_for_path("root.md").unwrap().index()]);
        assert!(!visible[index.node_for_path("child 2/x.md").unwrap().index()]);
        // #sweet is on a child-1 note: kept. #top is on root: dropped.
        assert!(visible[index.node_for_tag("sweet").unwrap().index()]);
        assert!(!visible[index.node_for_tag("top").unwrap().index()]);
        Ok(())
    }
}
