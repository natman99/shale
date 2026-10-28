//! File-tree construction from flat vault entries.

use std::collections::BTreeMap;
use std::path::PathBuf;

use shale_core::vault::VaultEntry;

/// The key context the sidebar's bindings live in, active while it has the
/// keyboard (see `Workspace::focus_tree`).
pub(crate) const CONTEXT: &str = "tree";

#[derive(Debug)]
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_file: bool,
    /// Directories first, then files, each alphabetically.
    pub children: Vec<TreeNode>,
}

#[derive(Default)]
struct DirNode {
    dirs: BTreeMap<String, DirNode>,
    files: Vec<String>,
}

/// Build the sidebar tree from the vault's files and its folders.
///
/// `dirs` is what makes a folder that holds no notes still show up: the tree is
/// otherwise implied by the paths of the files under it.
pub fn build_tree(entries: &[VaultEntry], dirs: &[VaultEntry]) -> Vec<TreeNode> {
    let mut root = DirNode::default();
    for entry in entries {
        let mut node = &mut root;
        let comps: Vec<_> = entry.path.components().collect();
        for comp in &comps[..comps.len() - 1] {
            let name = comp.as_os_str().to_string_lossy().to_string();
            node = node.dirs.entry(name).or_default();
        }
        if let Some(name) = comps.last() {
            node.files
                .push(name.as_os_str().to_string_lossy().to_string());
        }
    }
    // A folder is a `dirs` entry whether or not anything is in it yet.
    for entry in dirs {
        let mut node = &mut root;
        for comp in entry.path.components() {
            let name = comp.as_os_str().to_string_lossy().to_string();
            node = node.dirs.entry(name).or_default();
        }
    }

    fn build(dir: &DirNode, path: &PathBuf) -> Vec<TreeNode> {
        let mut out = Vec::new();
        for (name, sub) in &dir.dirs {
            let sub_path = path.join(name);
            let mut children = build(sub, &sub_path);
            children.sort_by(|a, b| a.is_file.cmp(&b.is_file).then(a.name.cmp(&b.name)));
            out.push(TreeNode {
                name: name.clone(),
                path: sub_path.clone(),
                is_file: false,
                children,
            });
        }
        for name in &dir.files {
            out.push(TreeNode {
                name: name.clone(),
                path: path.join(name),
                is_file: true,
                children: Vec::new(),
            });
        }
        out
    }

    let mut out = build(&root, &PathBuf::new());
    out.sort_by(|a, b| a.is_file.cmp(&b.is_file).then(a.name.cmp(&b.name)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(p: &str) -> VaultEntry {
        VaultEntry {
            path: PathBuf::from(p),
        }
    }

    #[test]
    fn nested_tree() {
        let tree = build_tree(
            &[
                entry("root.md"),
                entry("notes/a.md"),
                entry("notes/deep/b.md"),
                entry("zeta.md"),
            ],
            &[],
        );
        assert_eq!(tree.len(), 3); // notes/, root.md, zeta.md (dirs first)
        assert!(!tree[0].is_file);
        assert_eq!(tree[0].name, "notes");
        assert_eq!(tree[0].children.len(), 2); // deep/ then a.md
        assert!(!tree[0].children[0].is_file);
        assert_eq!(tree[0].children[0].name, "deep");
        assert_eq!(tree[1].name, "root.md");
        assert_eq!(tree[2].name, "zeta.md");
    }

    #[test]
    fn a_folder_with_nothing_in_it_is_still_a_row() {
        let tree = build_tree(
            &[entry("notes/a.md")],
            &[entry("inbox"), entry("inbox/deep")],
        );
        assert_eq!(tree.len(), 2, "inbox/ then notes/");
        assert_eq!(tree[0].name, "inbox");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].name, "deep");
        assert!(tree[0].children[0].children.is_empty());
    }
}
