//! The graph filters as a pure view over the [`GraphIndex`]: a visibility
//! bitmask the simulation and the renderer both consult, so a filtered-out
//! node takes part in neither the physics nor the paint (graph-view-plan.md
//! Task 4). Re-running the filter is cheap; re-heating the sim on a filter
//! change is the caller's job.

use std::collections::HashSet;

use petgraph::graph::NodeIndex;

use crate::fuzzy::fuzzy_match;
use crate::graph::index::{GraphIndex, NodeKind};
use crate::vault::rel_text;

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
    /// Local scope (M2b): only the nodes within `depth` hops of one note. This
    /// is the graph read as *a note's neighbourhood* rather than as a vault —
    /// the picture that answers "what is this note connected to". `None` is the
    /// whole vault.
    pub local: Option<LocalScope>,
    /// A lens (roadmap P4.16): only the notes one saved query answered, plus the
    /// tags and ghosts those notes carry. `None` is no lens — EDGE-L9, there is
    /// no "lens off" state apart from "no lens". It **combines** with the other
    /// dimensions rather than replacing them (EDGE-L3): the strip's chips still
    /// cut inside a lens, so the strip never lies about the picture.
    pub lens: Option<LensScope>,
}

/// A lens: the notes one saved query answered, worn as a scope (roadmap P4.16).
///
/// The filter's job is only the mask — who carries one of these paths. Counting an
/// answer that named no node is the app's, not this module's (EDGE-L5), because
/// [`apply`] answers with a bitmask and nothing else. The app does the counting once and
/// carries the result here, so the tab title, the strip and the status line all read one
/// answer rather than each re-deriving a number that could disagree (EDGE-L14).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LensScope {
    /// The lens's name, as its file under `.shale/lenses/` is named. Carried for the
    /// tab title and the filter strip, which both say what the picture is wearing.
    pub name: String,
    /// The answer, as vault-relative paths, in the answer's own order. A path the graph
    /// has no node for simply keeps nothing here; the app counts it (EDGE-L5).
    pub paths: Vec<String>,
    /// How many of `paths` this graph has no node for (EDGE-L5). Counted out loud in the
    /// strip rather than dropped in silence: a picture that quietly disagrees with the
    /// answer is the failure the lens exists to avoid.
    pub unmatched: usize,
    /// Set when the lens stopped answering: the picture is the **last good one** (EDGE-L6)
    /// and this is the sentence for why it is not current — the database's own words, or
    /// the file being gone (EDGE-L11). `apply` ignores it; the strip does not.
    pub stale: Option<String>,
}

/// How far the local view reaches from its centre (M2b). Five is Obsidian's
/// slider ceiling and the honest point where "neighbourhood" stops being a
/// word: at five hops a hub note's scope is most of the vault again.
pub const LOCAL_DEPTH_MAX: usize = 5;

/// The local scope: a centre and how far out it reaches.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalScope {
    /// Vault-relative path of the note the view is centred on.
    pub center: String,
    /// Hops from the centre. `0` is the centre alone.
    pub depth: usize,
}

impl Default for GraphFilter {
    fn default() -> Self {
        Self {
            orphans: true,
            existing_only: false,
            tags: true,
            search: None,
            folder: None,
            local: None,
            lens: None,
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
        for (i, v) in visible.iter_mut().enumerate() {
            let meta = index.meta(NodeIndex::new(i));
            match meta.kind {
                NodeKind::Note | NodeKind::Attachment | NodeKind::Query => {
                    let path = meta.path.as_ref().map(rel_text);
                    let inside = match path {
                        Some(p) => p == prefix || p.starts_with(&format!("{prefix}/")),
                        None => false,
                    };
                    *v = inside;
                }
                NodeKind::Tag | NodeKind::Ghost => {
                    // Decided by their neighbours, below.
                    *v = true;
                }
            }
        }
    }

    // A lens is the same shape with a different way of deciding who is in: the paths one
    // saved query answered, rather than everything under one folder (EDGE-L2). It **ands**
    // with folder scope instead of taking over — a lens worn on a folder-scoped graph is
    // the intersection, which is what "combine" means (EDGE-L3).
    if let Some(lens) = &filter.lens {
        let answered: HashSet<&str> = lens.paths.iter().map(String::as_str).collect();
        for (i, v) in visible.iter_mut().enumerate() {
            let meta = index.meta(NodeIndex::new(i));
            match meta.kind {
                NodeKind::Note | NodeKind::Attachment | NodeKind::Query => {
                    *v &= match meta.path.as_ref().map(rel_text) {
                        Some(p) => answered.contains(p.as_str()),
                        None => false,
                    };
                }
                NodeKind::Tag | NodeKind::Ghost => {
                    // Decided by their neighbours, below (EDGE-L4).
                    *v = true;
                }
            }
        }
    }

    // ...then, for either path-shaped scope, tags and ghosts survive only if a visible
    // note links them: a tag is a fact about the notes that carry it, and a ghost is a
    // fact about the note that names it (EDGE-L4).
    // (Two passes: the neighbor check reads `visible` while deciding, so
    // the writes wait until every decision is made.)
    if filter.folder.is_some() || filter.lens.is_some() {
        let mut hide = Vec::new();
        for i in 0..n {
            let node = NodeIndex::new(i);
            if matches!(index.meta(node).kind, NodeKind::Tag | NodeKind::Ghost)
                && !index.neighbors(node).any(|nb| visible[nb.index()])
            {
                hide.push(i);
            }
        }
        for i in hide {
            visible[i] = false;
        }
    }

    // Local scope: a hop count from the centre, over the vault as it is — the
    // BFS walks *through* nodes the kind filters are about to hide, because
    // "two hops away" is a fact about the links, not about what is on screen.
    // The kind and search filters then cut inside that neighbourhood as usual.
    if let Some(local) = &filter.local
        && let Some(center) = index.node_for_path(&local.center)
    {
        let depth = local.depth.min(LOCAL_DEPTH_MAX);
        let mut hops = vec![usize::MAX; n];
        hops[center.index()] = 0;
        let mut frontier = vec![center];
        let mut hop = 0usize;
        while hop < depth && !frontier.is_empty() {
            let mut next = Vec::new();
            for node in &frontier {
                for nb in index.neighbors(*node) {
                    if hops[nb.index()] == usize::MAX {
                        hops[nb.index()] = hop + 1;
                        next.push(nb);
                    }
                }
            }
            frontier = next;
            hop += 1;
        }
        for (i, v) in visible.iter_mut().enumerate() {
            if hops[i] > depth {
                *v = false;
            }
        }
    }
    // A centre the index does not know — a note deleted from under the view,
    // or one never indexed — leaves the scope inert rather than painting an
    // empty canvas. The window clears the mode in that case (it owns the
    // centre); an empty graph is a worse answer than the whole one.

    for (i, v) in visible.iter_mut().enumerate() {
        let node = NodeIndex::new(i);
        let meta = index.meta(node);
        match meta.kind {
            NodeKind::Ghost if filter.existing_only => *v = false,
            NodeKind::Tag if !filter.tags => *v = false,
            // An unembedded query is an orphan the same way an unwritten-about
            // note is: a file that nothing reaches.
            NodeKind::Note | NodeKind::Query if !filter.orphans && index.degree(node) == 0 => {
                *v = false
            }
            _ => {}
        }
    }

    if let Some(query) = &filter.search
        && !query.trim().is_empty()
    {
        for (i, v) in visible.iter_mut().enumerate() {
            if !*v {
                continue;
            }
            let node = NodeIndex::new(i);
            let meta = index.meta(node);
            let label = &meta.label;
            let path = meta.path.as_ref().map(rel_text).unwrap_or_default();
            let matched = fuzzy_match(query, label).is_some()
                || (!path.is_empty() && fuzzy_match(query, &path).is_some());
            if !matched {
                *v = false;
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
        let index = GraphIndex::build(
            &vault,
            &vault.list_markdown()?,
            &vault.list_attachments()?,
            &vault.list_queries()?,
            &[],
        )?;
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

    /// The local view is a hop count, and the hop count is the whole feature: one hop
    /// from `root.md` is its note, its ghost and its tag; two hops adds what those reach;
    /// notes that are not reachable at all never arrive.
    #[test]
    fn local_scope_reaches_the_depth_it_says() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let at = |depth: usize| {
            apply(
                &index,
                &GraphFilter {
                    local: Some(LocalScope {
                        center: "root.md".into(),
                        depth,
                    }),
                    ..Default::default()
                },
            )
        };
        let sorted = |values: &[String]| -> Vec<String> {
            let mut v = values.to_vec();
            v.sort();
            v
        };
        let named = |visible: &[bool]| -> Vec<String> {
            (0..index.node_count())
                .filter(|i| visible[*i])
                .map(|i| index.meta(NodeIndex::new(i)).label.to_string())
                .collect::<Vec<_>>()
        };
        // The labels are the graph's own: a note is its stem, and `[[pie/p]]` is a
        // ghost node beside the resolved link to `child 1/pie/p.md`.
        let one = named(&at(1));
        assert_eq!(
            sorted(&one),
            sorted(&["gone", "p", "pie/p", "root", "top"].map(String::from)),
            "one hop is the note, what it links, and what it tags"
        );
        let two = named(&at(2));
        assert_eq!(
            sorted(&two),
            sorted(&["gone", "p", "pie/p", "root", "sweet", "top", "x"].map(String::from)),
            "the second hop arrives, and nothing unreachable does"
        );
        let zero = named(&at(0));
        assert_eq!(
            zero,
            vec!["root".to_string()],
            "depth 0 is the centre alone"
        );
        Ok(())
    }

    /// A depth past the ceiling is clamped rather than trusted: an unbounded BFS on a
    /// hub note is the whole vault wearing a neighbourhood's clothes.
    #[test]
    fn local_depth_is_clamped() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            local: Some(LocalScope {
                center: "root.md".into(),
                depth: LOCAL_DEPTH_MAX + 40,
            }),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        let cake = index.node_for_path("child 1/cake/c.md").unwrap();
        assert!(
            !visible[cake.index()],
            "the clamp holds, so an unreachable note stays unreachable"
        );
        Ok(())
    }

    /// A centre the index does not know leaves the scope inert rather than empty.
    #[test]
    fn a_missing_centre_does_not_empty_the_graph() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            local: Some(LocalScope {
                center: "deleted.md".into(),
                depth: 1,
            }),
            ..Default::default()
        };
        assert_eq!(count(&apply(&index, &f)), index.node_count());
        Ok(())
    }

    /// Local is a scope, not a mode: the kind filters still cut inside it.
    #[test]
    fn local_scope_composes_with_the_kind_filters() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            tags: false,
            existing_only: true,
            local: Some(LocalScope {
                center: "root.md".into(),
                depth: 1,
            }),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        let root = index.node_for_path("root.md").unwrap();
        let p = index.node_for_path("child 1/pie/p.md").unwrap();
        let top = index.node_for_tag("top").unwrap();
        let gone = index.ghost_for_target("gone").unwrap();
        assert!(visible[root.index()] && visible[p.index()]);
        assert!(!visible[top.index()] && !visible[gone.index()]);
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

    /// A lens is a set of paths, and the mask is exactly that set: the notes it answered
    /// are on screen and every other note is not, whatever their degree or folder.
    #[test]
    fn a_lens_keeps_exactly_the_notes_it_answered() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            lens: Some(LensScope {
                name: "goblins".into(),
                paths: vec!["root.md".into(), "child 2/x.md".into()],
                ..LensScope::default()
            }),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        for path in ["root.md", "child 2/x.md"] {
            assert!(
                visible[index.node_for_path(path).unwrap().index()],
                "{path} was answered"
            );
        }
        for path in ["child 1/pie/p.md", "child 1/cake/c.md", "lonely.md"] {
            assert!(
                !visible[index.node_for_path(path).unwrap().index()],
                "{path} was not answered"
            );
        }
        Ok(())
    }

    /// EDGE-L4: a tag is a fact about the notes that carry it and a ghost is a fact about
    /// the note that names it, so both follow the lens rather than the vault.
    #[test]
    fn a_lens_keeps_the_tags_and_ghosts_its_notes_carry() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            lens: Some(LensScope {
                name: "goblins".into(),
                paths: vec!["root.md".into()],
                ..LensScope::default()
            }),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        // #top is on root: kept. #sweet is on p, which the lens did not answer: dropped.
        assert!(visible[index.node_for_tag("top").unwrap().index()]);
        assert!(!visible[index.node_for_tag("sweet").unwrap().index()]);
        // root names both ghosts, so both are drawn (ruled 2026-11-08).
        assert!(index.node_for_path("child 1/pie/p.md").is_some());
        assert!(visible[index.ghost_for_target("gone").unwrap().index()]);
        assert!(visible[index.ghost_for_target("pie/p").unwrap().index()]);
        Ok(())
    }

    /// The renderer draws an edge only when both ends are visible, so a lens takes the
    /// edges with it for free — but only the ones whose far end is off screen. Asserting
    /// the graph still holds both edges is what makes this a fact rather than a hope.
    #[test]
    fn a_lens_leaves_an_edge_only_where_both_ends_survive() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            lens: Some(LensScope {
                name: "goblins".into(),
                paths: vec!["root.md".into()],
                ..LensScope::default()
            }),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        let root = index.node_for_path("root.md").unwrap();
        let gone = index.ghost_for_target("gone").unwrap();
        let p = index.node_for_path("child 1/pie/p.md").unwrap();
        assert!(index.graph().find_edge(root, gone).is_some());
        assert!(
            visible[root.index()] && visible[gone.index()],
            "root->gone stands"
        );
        // p links root, and that edge is still in the graph; p is off the answer, so the
        // renderer will not draw it.
        assert!(index.graph().find_edge(p, root).is_some());
        assert!(!visible[p.index()], "p is not in the answer");
        Ok(())
    }

    /// EDGE-L3: a lens combines with the strip's dimensions rather than switching them
    /// off. The chips still cut inside the answer, and a second scope still intersects it.
    #[test]
    fn a_lens_combines_with_the_chips_and_the_other_scopes() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let answered = |paths: &[&str]| LensScope {
            name: "goblins".into(),
            paths: paths.iter().map(|p| (*p).to_string()).collect(),
            ..LensScope::default()
        };

        // A chip still cuts inside a lens: root is answered and names `[[gone]]`, but
        // "existing only" still drops the ghost. And the chip does not let the lens grow:
        // x.md is untouched by the chip, and the lens is what keeps it off screen.
        let f = GraphFilter {
            lens: Some(answered(&["root.md"])),
            existing_only: true,
            ..Default::default()
        };
        let visible = apply(&index, &f);
        assert!(visible[index.node_for_path("root.md").unwrap().index()]);
        assert!(!visible[index.ghost_for_target("gone").unwrap().index()]);
        assert!(
            !visible[index.node_for_path("child 2/x.md").unwrap().index()],
            "the chip is not what hides this note; the lens is"
        );

        // A lens worn on a folder scope is the intersection, not a takeover: the folder
        // hides root, the lens hides the child-1 note the answer did not name.
        let f = GraphFilter {
            lens: Some(answered(&["root.md", "child 1/pie/p.md"])),
            folder: Some("child 1".into()),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        assert!(visible[index.node_for_path("child 1/pie/p.md").unwrap().index()]);
        assert!(
            !visible[index.node_for_path("root.md").unwrap().index()],
            "answered, but outside the folder"
        );
        assert!(
            !visible[index.node_for_path("child 1/cake/c.md").unwrap().index()],
            "inside the folder, but not answered"
        );

        // And a search narrows inside a lens the same way it narrows anywhere else.
        let f = GraphFilter {
            lens: Some(answered(&["root.md", "child 2/x.md"])),
            search: Some("x".into()),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        assert!(visible[index.node_for_path("child 2/x.md").unwrap().index()]);
        assert!(
            !visible[index.node_for_path("root.md").unwrap().index()],
            "answered, but not matching the search"
        );
        Ok(())
    }

    /// An empty answer is an answer. Unlike a local scope whose centre the index does not
    /// know — which leaves the graph alone, because the window owns that centre — a lens
    /// that came back with nothing has nothing to draw, and the strip says so.
    #[test]
    fn an_empty_lens_keeps_nothing_rather_than_everything() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            lens: Some(LensScope {
                name: "nothing".into(),
                paths: Vec::new(),
                ..LensScope::default()
            }),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        assert_eq!(count(&visible), 0, "no answer, no picture");
        assert_ne!(index.node_count(), 0, "the fixture is not itself empty");
        Ok(())
    }

    /// EDGE-L5: an answer can name a path the graph has no node for. The mask keeps
    /// nothing for it and does not treat it as an error; counting it is the app's job,
    /// because `apply` answers with a bitmask and nothing else.
    #[test]
    fn a_lens_path_with_no_node_keeps_the_rest() -> anyhow::Result<()> {
        let (_dir, _vault, index) = fixture()?;
        let f = GraphFilter {
            lens: Some(LensScope {
                name: "half-gone".into(),
                paths: vec!["root.md".into(), "deleted.md".into()],
                ..LensScope::default()
            }),
            ..Default::default()
        };
        let visible = apply(&index, &f);
        assert!(visible[index.node_for_path("root.md").unwrap().index()]);
        assert_eq!(
            count(&visible),
            // root, #top, and the two ghosts root names.
            4,
            "the path with no node contributes nothing"
        );
        Ok(())
    }
}
