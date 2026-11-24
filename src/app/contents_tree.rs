//! Display the document's depth-first outline as a collapsible tree.
//!
//! Source indexes are stable for this outline, including duplicate titles and
//! destinations. The caller supplies the entries and keeps their navigation
//! targets. Rebuild the tree when the book or active outline changes.
//!
//! Reference: KOReader's current-path reveal, title filtering and branch
//! controls at commit 9192014d8bd82a91dc1012473be0f238dedfdb54:
//! https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readertoc.lua#L770-L1182
//! https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/util.lua#L1535-L1559

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Branch {
    Leaf,
    Collapsed,
    Expanded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct VisibleRow {
    pub(super) source_index: usize,
    pub(super) depth: usize,
    pub(super) branch: Branch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SearchOutcome {
    EmptyQuery,
    NoMatches,
    Applied,
}

struct Node {
    parent: Option<usize>,
    depth: usize,
    end: usize,
}

pub(super) struct ContentsTree {
    nodes: Vec<Node>,
    // Within a search, None excludes the entire subtree rooted at that entry.
    branches: Vec<Option<Branch>>,
    query: Option<String>,
}

impl ContentsTree {
    pub(super) fn new(depths: impl IntoIterator<Item = usize>) -> Self {
        let mut nodes: Vec<Node> = Vec::new();
        let mut ancestors: Vec<(usize, usize)> = Vec::new();
        for (index, source_depth) in depths.into_iter().enumerate() {
            while let Some(&(closed, depth)) = ancestors.last()
                && depth >= source_depth
            {
                ancestors.pop();
                nodes[closed].end = index;
            }
            nodes.push(Node {
                parent: ancestors.last().map(|&(parent, _)| parent),
                depth: ancestors.len(),
                end: 0,
            });
            ancestors.push((index, source_depth));
        }
        let end = nodes.len();
        for (index, _) in ancestors {
            nodes[index].end = end;
        }

        let branches = Self::collapsed_branches(&nodes);
        Self {
            nodes,
            branches,
            query: None,
        }
    }

    fn collapsed_branches(nodes: &[Node]) -> Vec<Option<Branch>> {
        nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                Some(if node.end > index + 1 {
                    Branch::Collapsed
                } else {
                    Branch::Leaf
                })
            })
            .collect()
    }

    /// Iterate the visible rows in source order, skipping closed or excluded
    /// subtrees. Search results use the same source indexes as the full tree.
    pub(super) fn visible_rows(&self) -> impl Iterator<Item = VisibleRow> + '_ {
        let mut next = 0;
        std::iter::from_fn(move || {
            while let Some(node) = self.nodes.get(next) {
                let source_index = next;
                let branch = self.branches[source_index];
                next = if branch == Some(Branch::Expanded) {
                    source_index + 1
                } else {
                    node.end
                };
                if let Some(branch) = branch {
                    return Some(VisibleRow {
                        source_index,
                        depth: node.depth,
                        branch,
                    });
                }
            }
            None
        })
    }

    /// Open the paths to the current heading and any other headings on its
    /// page. The caller resolves that set against the active layout.
    pub(super) fn reveal_current(&mut self, indexes: impl IntoIterator<Item = usize>) {
        if self.query.is_some() {
            return;
        }
        for index in indexes {
            let mut parent = self.nodes.get(index).and_then(|node| node.parent);
            while let Some(index) = parent {
                if self.branches[index] == Some(Branch::Expanded) {
                    break;
                }
                self.branches[index] = Some(Branch::Expanded);
                parent = self.nodes[index].parent;
            }
        }
    }

    /// Toggle a visible branch. Closing a branch also closes its descendants,
    /// so opening it again initially reveals its direct children.
    pub(super) fn toggle(&mut self, source_index: usize) -> bool {
        let Some(node) = self.nodes.get(source_index) else {
            return false;
        };
        let mut parent = node.parent;
        while let Some(index) = parent {
            if self.branches[index] != Some(Branch::Expanded) {
                return false;
            }
            parent = self.nodes[index].parent;
        }
        match self.branches[source_index] {
            Some(Branch::Collapsed) => self.branches[source_index] = Some(Branch::Expanded),
            Some(Branch::Expanded) => {
                for branch in &mut self.branches[source_index..node.end] {
                    if *branch == Some(Branch::Expanded) {
                        *branch = Some(Branch::Collapsed);
                    }
                }
            }
            Some(Branch::Leaf) | None => return false,
        }
        true
    }

    /// Show title matches and their ancestors. A query of exactly "*" opens
    /// the full tree. Empty or unmatched queries leave the current view intact.
    pub(super) fn search<'a>(
        &mut self,
        query: &str,
        mut source_title: impl FnMut(usize) -> &'a str,
    ) -> SearchOutcome {
        if query.is_empty() {
            return SearchOutcome::EmptyQuery;
        }
        let match_all = query == "*";
        let literal = lowercase(query);
        let mut branches = vec![None; self.nodes.len()];
        let mut found = false;
        for (index, node) in self.nodes.iter().enumerate().rev() {
            if match_all || lowercase(source_title(index)).contains(&literal) {
                branches[index].get_or_insert(Branch::Leaf);
                found = true;
            }
            if branches[index].is_some()
                && let Some(parent) = node.parent
            {
                branches[parent] = Some(Branch::Expanded);
            }
        }
        if !found {
            return SearchOutcome::NoMatches;
        }
        self.branches = branches;
        self.query = Some(query.to_owned());
        SearchOutcome::Applied
    }

    pub(super) fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// Restore the full collapsed outline. The caller can then reveal the
    /// current reading path again, as on initial open.
    pub(super) fn clear_search(&mut self) -> bool {
        if self.query.take().is_none() {
            return false;
        }
        self.branches = Self::collapsed_branches(&self.nodes);
        true
    }
}

fn lowercase(text: &str) -> String {
    // KOReader's title search lowercases individual Unicode characters.
    text.chars()
        .map(|character| character.to_lowercase().next().unwrap_or(character))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(tree: &ContentsTree) -> Vec<(usize, usize, Branch)> {
        tree.visible_rows()
            .map(|row| (row.source_index, row.depth, row.branch))
            .collect()
    }

    #[test]
    fn opens_collapsed_and_reveals_every_path_on_the_current_page() {
        use Branch::*;
        let mut tree = ContentsTree::new([0, 1, 2, 1, 0, 1, 0]);
        assert_eq!(
            rows(&tree),
            [(0, 0, Collapsed), (4, 0, Collapsed), (6, 0, Leaf)]
        );
        tree.reveal_current([2, 5, usize::MAX]);
        assert_eq!(
            rows(&tree),
            [
                (0, 0, Expanded),
                (1, 1, Expanded),
                (2, 2, Leaf),
                (3, 1, Leaf),
                (4, 0, Expanded),
                (5, 1, Leaf),
                (6, 0, Leaf),
            ]
        );
    }

    #[test]
    fn closing_a_branch_resets_its_descendants_and_rejects_hidden_toggles() {
        use Branch::*;
        let mut tree = ContentsTree::new([0, 1, 2, 0]);
        assert!(!tree.toggle(1));
        assert!(!tree.toggle(99));
        tree.reveal_current([2]);
        assert!(tree.toggle(0));
        assert_eq!(rows(&tree), [(0, 0, Collapsed), (3, 0, Leaf)]);
        assert!(tree.toggle(0));
        assert_eq!(
            rows(&tree),
            [(0, 0, Expanded), (1, 1, Collapsed), (3, 0, Leaf)]
        );
    }

    #[test]
    fn title_filter_preserves_duplicate_matches_and_only_their_ancestors() {
        use Branch::*;
        let titles = [
            "Part A",
            "Chapter",
            "ÉTUDE",
            "Ignored",
            "Part B",
            "Étude",
            "Another title",
            "Étude",
            "Nonmatching child",
        ];
        let mut tree = ContentsTree::new([0, 1, 2, 1, 0, 1, 1, 0, 1]);
        assert_eq!(
            tree.search("étude", |index| titles[index]),
            SearchOutcome::Applied
        );
        assert_eq!(tree.query(), Some("étude"));
        assert_eq!(
            rows(&tree),
            [
                (0, 0, Expanded),
                (1, 1, Expanded),
                (2, 2, Leaf),
                (4, 0, Expanded),
                (5, 1, Leaf),
                (7, 0, Leaf),
            ]
        );
        assert!(!tree.toggle(7)); // Its only child is not a search result.
        assert!(tree.toggle(0));
        tree.reveal_current([2]); // Reading location does not expand a filtered view.
        assert_eq!(rows(&tree)[0], (0, 0, Collapsed));
        assert!(tree.toggle(0));
        assert_eq!(rows(&tree)[1], (1, 1, Collapsed));
        assert!(tree.toggle(1));
        assert_eq!(rows(&tree)[2], (2, 2, Leaf));
    }

    #[test]
    fn wildcard_clear_and_empty_results_do_not_change_the_reading_path() {
        use Branch::*;
        let titles = ["Part A", "a*b", "Part B", "ab"];
        let mut tree = ContentsTree::new([0, 1, 0, 1]);
        assert_eq!(
            tree.search("*", |index| titles[index]),
            SearchOutcome::Applied
        );
        assert_eq!(tree.visible_rows().count(), 4);
        assert_eq!(
            tree.search("a*b", |index| titles[index]),
            SearchOutcome::Applied
        );
        assert_eq!(rows(&tree), [(0, 0, Expanded), (1, 1, Leaf)]);
        assert_eq!(
            tree.search("missing", |index| titles[index]),
            SearchOutcome::NoMatches
        );
        assert_eq!(
            tree.search("", |index| titles[index]),
            SearchOutcome::EmptyQuery
        );
        assert_eq!(tree.query(), Some("a*b"));
        assert_eq!(rows(&tree), [(0, 0, Expanded), (1, 1, Leaf)]);
        assert!(tree.clear_search());
        assert!(!tree.clear_search());
        assert_eq!(tree.query(), None);
        assert_eq!(rows(&tree), [(0, 0, Collapsed), (2, 0, Collapsed)]);
        tree.reveal_current([3]);
        assert_eq!(
            rows(&tree),
            [(0, 0, Collapsed), (2, 0, Expanded), (3, 1, Leaf)]
        );
    }

    #[test]
    fn tolerates_skipped_source_depths_and_empty_outlines() {
        use Branch::*;
        let mut tree = ContentsTree::new([8, usize::MAX, 2, 4, 3]);
        tree.reveal_current([1, 3]);
        assert_eq!(
            rows(&tree),
            [
                (0, 0, Expanded),
                (1, 1, Leaf),
                (2, 0, Expanded),
                (3, 1, Leaf),
                (4, 1, Leaf)
            ]
        );
        let mut empty = ContentsTree::new([]);
        assert_eq!(
            empty.search("*", |_| unreachable!()),
            SearchOutcome::NoMatches
        );
        assert!(!empty.toggle(0));
        assert_eq!(empty.visible_rows().count(), 0);
    }

    #[test]
    fn iterates_a_deep_outline_without_recursive_traversal() {
        let count = 10_000;
        let mut tree = ContentsTree::new(0..count);
        tree.reveal_current([count - 1]);
        assert_eq!(tree.visible_rows().count(), count);
        assert!(tree.toggle(0));
        assert_eq!(tree.visible_rows().count(), 1);
    }
}
