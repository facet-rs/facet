//! GumTree node matching algorithm.
//!
//! Implements two-phase matching:
//! 1. Top-down: Match identical subtrees by hash
//! 2. Bottom-up: Match remaining nodes by structural similarity

use crate::tree::Tree;
use core::hash::Hash;
use indextree::NodeId;
use rapidhash::{RapidHashMap as HashMap, RapidHashSet as HashSet};
use rayon::prelude::*;

#[cfg(feature = "matching-stats")]
use std::cell::RefCell;

#[cfg(feature = "matching-stats")]
thread_local! {
    static DICE_CALLS: RefCell<usize> = const { RefCell::new(0) };
    static DICE_UNIQUE_A: RefCell<HashSet<NodeId>> = RefCell::new(HashSet::default());
    static DICE_UNIQUE_B: RefCell<HashSet<NodeId>> = RefCell::new(HashSet::default());
}

/// Reset matching statistics (call before compute_matching)
#[cfg(feature = "matching-stats")]
pub fn reset_stats() {
    DICE_CALLS.with(|c| *c.borrow_mut() = 0);
    DICE_UNIQUE_A.with(|s| s.borrow_mut().clear());
    DICE_UNIQUE_B.with(|s| s.borrow_mut().clear());
}

/// Get matching statistics: (total_calls, unique_a_nodes, unique_b_nodes)
#[cfg(feature = "matching-stats")]
pub fn get_stats() -> (usize, usize, usize) {
    let calls = DICE_CALLS.with(|c| *c.borrow());
    let unique_a = DICE_UNIQUE_A.with(|s| s.borrow().len());
    let unique_b = DICE_UNIQUE_B.with(|s| s.borrow().len());
    (calls, unique_a, unique_b)
}

/// A bidirectional mapping between nodes in two trees.
/// Uses Vec for O(1) lookups indexed by NodeId.
#[derive(Debug)]
pub struct Matching {
    /// Map from tree A node to tree B node (indexed by A's NodeId)
    a_to_b: Vec<Option<NodeId>>,
    /// Map from tree B node to tree A node (indexed by B's NodeId)
    b_to_a: Vec<Option<NodeId>>,
    /// All matched pairs (for iteration, since NodeId can't be reconstructed from index)
    pairs: Vec<(NodeId, NodeId)>,
}

impl Default for Matching {
    fn default() -> Self {
        Self::new()
    }
}

impl Matching {
    /// Create a new empty matching.
    pub fn new() -> Self {
        Self {
            a_to_b: Vec::new(),
            b_to_a: Vec::new(),
            pairs: Vec::new(),
        }
    }

    /// Create a new matching with preallocated capacity.
    pub fn with_capacity(max_a: usize, max_b: usize) -> Self {
        Self {
            a_to_b: vec![None; max_a],
            b_to_a: vec![None; max_b],
            pairs: Vec::new(),
        }
    }

    /// Add a match between two nodes.
    #[inline]
    pub fn add(&mut self, a: NodeId, b: NodeId) {
        let a_idx = usize::from(a);
        let b_idx = usize::from(b);

        // Grow vectors if needed
        if a_idx >= self.a_to_b.len() {
            self.a_to_b.resize(a_idx + 1, None);
        }
        if b_idx >= self.b_to_a.len() {
            self.b_to_a.resize(b_idx + 1, None);
        }

        self.a_to_b[a_idx] = Some(b);
        self.b_to_a[b_idx] = Some(a);
        self.pairs.push((a, b));
    }

    /// Check if a node from tree A is matched.
    #[inline(always)]
    pub fn contains_a(&self, a: NodeId) -> bool {
        let idx = usize::from(a);
        self.a_to_b.get(idx).is_some_and(|opt| opt.is_some())
    }

    /// Check if a node from tree B is matched.
    #[inline(always)]
    pub fn contains_b(&self, b: NodeId) -> bool {
        let idx = usize::from(b);
        self.b_to_a.get(idx).is_some_and(|opt| opt.is_some())
    }

    /// Get the match for a node from tree A.
    #[inline(always)]
    pub fn get_b(&self, a: NodeId) -> Option<NodeId> {
        let idx = usize::from(a);
        self.a_to_b.get(idx).copied().flatten()
    }

    /// Get the match for a node from tree B.
    #[inline(always)]
    pub fn get_a(&self, b: NodeId) -> Option<NodeId> {
        let idx = usize::from(b);
        self.b_to_a.get(idx).copied().flatten()
    }

    /// Get all matched pairs.
    pub fn pairs(&self) -> impl Iterator<Item = (NodeId, NodeId)> + '_ {
        self.pairs.iter().copied()
    }

    /// Get the number of matched pairs.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Check if there are no matches.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

/// Configuration for the matching algorithm.
#[derive(Debug, Clone)]
pub struct MatchingConfig {
    /// Minimum Dice coefficient for bottom-up matching.
    /// Nodes with similarity below this threshold won't be matched.
    pub similarity_threshold: f64,

    /// Minimum height for a node to be considered in top-down matching.
    /// Smaller subtrees are left for bottom-up matching.
    pub min_height: usize,
}

impl Default for MatchingConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.5,
            min_height: 1,
        }
    }
}

/// Compute the matching between two trees using the GumTree algorithm.
pub fn compute_matching<K, L>(
    tree_a: &Tree<K, L>,
    tree_b: &Tree<K, L>,
    config: &MatchingConfig,
) -> Matching
where
    K: Clone + Eq + Hash + Send + Sync,
    L: Clone + Send + Sync,
{
    let mut matching = Matching::new();

    // Phase 1: Top-down matching (identical subtrees by hash)
    top_down_phase(tree_a, tree_b, &mut matching, config);

    // Phase 2: Bottom-up matching (similar nodes by Dice coefficient)
    bottom_up_phase(tree_a, tree_b, &mut matching, config);

    matching
}

/// Phase 1: Top-down matching.
///
/// Greedily matches nodes with identical subtree hashes, starting from the roots
/// and working down. When two nodes have the same hash, their entire subtrees
/// are identical and can be matched recursively.
fn top_down_phase<K, L>(
    tree_a: &Tree<K, L>,
    tree_b: &Tree<K, L>,
    matching: &mut Matching,
    config: &MatchingConfig,
) where
    K: Clone + Eq + Hash,
    L: Clone,
{
    // Build hash -> nodes index for tree B
    let mut b_by_hash: HashMap<u64, Vec<NodeId>> = HashMap::default();
    for b_id in tree_b.iter() {
        let hash = tree_b.get(b_id).hash;
        b_by_hash.entry(hash).or_default().push(b_id);
    }

    // Priority queue: process nodes by height (descending)
    // Higher nodes = larger subtrees = more valuable to match first
    let mut candidates: Vec<(NodeId, NodeId)> = vec![(tree_a.root, tree_b.root)];

    // Sort by height descending
    candidates.sort_by(|a, b| {
        let ha = tree_a.height(a.0);
        let hb = tree_a.height(b.0);
        hb.cmp(&ha)
    });

    while let Some((a_id, b_id)) = candidates.pop() {
        // Skip if already matched
        if matching.contains_a(a_id) || matching.contains_b(b_id) {
            continue;
        }

        let a_data = tree_a.get(a_id);
        let b_data = tree_b.get(b_id);

        // Skip small subtrees (leave for bottom-up)
        if tree_a.height(a_id) < config.min_height {
            continue;
        }

        // If hashes match, these subtrees are identical
        if a_data.hash == b_data.hash && a_data.kind == b_data.kind {
            match_subtrees(tree_a, tree_b, a_id, b_id, matching);
        } else {
            // Hashes differ - try to match children
            for a_child in tree_a.children(a_id) {
                let a_child_data = tree_a.get(a_child);

                // Look for B nodes with matching hash
                if let Some(b_candidates) = b_by_hash.get(&a_child_data.hash) {
                    for &b_candidate in b_candidates {
                        if !matching.contains_b(b_candidate) {
                            candidates.push((a_child, b_candidate));
                        }
                    }
                }

                // Also try children of b_id with same kind
                for b_child in tree_b.children(b_id) {
                    if !matching.contains_b(b_child) {
                        let b_child_data = tree_b.get(b_child);
                        if a_child_data.kind == b_child_data.kind {
                            candidates.push((a_child, b_child));
                        }
                    }
                }
            }
        }
    }
}

/// Match two subtrees recursively (when their hashes match).
fn match_subtrees<K, L>(
    tree_a: &Tree<K, L>,
    tree_b: &Tree<K, L>,
    a_id: NodeId,
    b_id: NodeId,
    matching: &mut Matching,
) where
    K: Clone + Eq + Hash,
    L: Clone,
{
    matching.add(a_id, b_id);

    // Match children in order (they should be identical if hashes match)
    let a_children: Vec<_> = tree_a.children(a_id).collect();
    let b_children: Vec<_> = tree_b.children(b_id).collect();

    for (a_child, b_child) in a_children.into_iter().zip(b_children.into_iter()) {
        match_subtrees(tree_a, tree_b, a_child, b_child, matching);
    }
}

/// Precomputed descendant sets for all nodes in a tree.
/// Uses Vec indexed by node arena index for O(1) access with no hashing.
struct DescendantMap {
    /// Descendant sets indexed by arena index. None for indices that don't exist.
    data: Vec<Option<HashSet<NodeId>>>,
}

impl DescendantMap {
    #[inline(always)]
    fn get(&self, node_id: NodeId) -> Option<&HashSet<NodeId>> {
        let idx = usize::from(node_id);
        self.data.get(idx).and_then(|opt| opt.as_ref())
    }
}

/// Precompute all descendant sets in parallel.
fn precompute_descendants<K, L>(tree: &Tree<K, L>) -> DescendantMap
where
    K: Clone + Eq + Hash + Send + Sync,
    L: Clone + Send + Sync,
{
    let nodes: Vec<NodeId> = tree.iter().collect();

    // Find max index to size the vec
    let max_idx = nodes.iter().map(|&id| usize::from(id)).max().unwrap_or(0);

    // Compute descendants in parallel
    let computed: Vec<(usize, HashSet<NodeId>)> = nodes
        .into_par_iter()
        .map(|node_id| {
            let idx = usize::from(node_id);
            let descendants: HashSet<NodeId> = tree.descendants(node_id).collect();
            (idx, descendants)
        })
        .collect();

    // Build the vec
    let mut data = vec![None; max_idx + 1];
    for (idx, descendants) in computed {
        data[idx] = Some(descendants);
    }

    DescendantMap { data }
}

/// Phase 2: Bottom-up matching.
///
/// For unmatched nodes, find candidates with the same kind and compute
/// similarity using the Dice coefficient on matched descendants.
/// For leaf nodes (no children), we match by hash since Dice is not meaningful.
fn bottom_up_phase<K, L>(
    tree_a: &Tree<K, L>,
    tree_b: &Tree<K, L>,
    matching: &mut Matching,
    config: &MatchingConfig,
) where
    K: Clone + Eq + Hash + Send + Sync,
    L: Clone + Send + Sync,
{
    // Build indices for tree B: by kind and by (kind, hash) for leaves
    let mut b_by_kind: HashMap<K, Vec<NodeId>> = HashMap::default();
    let mut b_by_kind_hash: HashMap<(K, u64), Vec<NodeId>> = HashMap::default();

    for b_id in tree_b.iter() {
        if !matching.contains_b(b_id) {
            let b_data = tree_b.get(b_id);
            let kind = b_data.kind.clone();
            b_by_kind.entry(kind.clone()).or_default().push(b_id);

            // For leaves, also index by (kind, hash)
            if tree_b.child_count(b_id) == 0 {
                b_by_kind_hash
                    .entry((kind, b_data.hash))
                    .or_default()
                    .push(b_id);
            }
        }
    }

    // Precompute all descendant sets in parallel
    let desc_a = precompute_descendants(tree_a);
    let desc_b = precompute_descendants(tree_b);

    // Process tree A in post-order (children before parents)
    for a_id in tree_a.post_order() {
        if matching.contains_a(a_id) {
            continue;
        }

        let a_data = tree_a.get(a_id);
        let is_leaf = tree_a.child_count(a_id) == 0;

        if is_leaf {
            // For leaves, match by exact hash (same kind AND same hash)
            let key = (a_data.kind.clone(), a_data.hash);
            if let Some(candidates) = b_by_kind_hash.get(&key) {
                for &b_id in candidates {
                    if !matching.contains_b(b_id) {
                        matching.add(a_id, b_id);
                        break; // Take the first available match
                    }
                }
            }
        } else {
            // For internal nodes, use Dice coefficient
            let candidates = b_by_kind.get(&a_data.kind).cloned().unwrap_or_default();

            let mut best: Option<(NodeId, f64)> = None;
            for b_id in candidates {
                if matching.contains_b(b_id) {
                    continue;
                }

                // Skip leaves when looking for internal node matches
                if tree_b.child_count(b_id) == 0 {
                    continue;
                }

                let score = dice_coefficient(a_id, b_id, matching, &desc_a, &desc_b);
                if score >= config.similarity_threshold
                    && (best.is_none() || score > best.unwrap().1)
                {
                    best = Some((b_id, score));
                }
            }

            if let Some((b_id, _)) = best {
                matching.add(a_id, b_id);
            }
        }
    }
}

/// Compute the Dice coefficient between two nodes based on matched descendants.
///
/// dice(A, B) = 2 × |matched_descendants| / (|descendants_A| + |descendants_B|)
fn dice_coefficient(
    a_id: NodeId,
    b_id: NodeId,
    matching: &Matching,
    desc_a_map: &DescendantMap,
    desc_b_map: &DescendantMap,
) -> f64 {
    #[cfg(feature = "matching-stats")]
    {
        DICE_CALLS.with(|c| *c.borrow_mut() += 1);
        DICE_UNIQUE_A.with(|s| { s.borrow_mut().insert(a_id); });
        DICE_UNIQUE_B.with(|s| { s.borrow_mut().insert(b_id); });
    }

    let empty = HashSet::default();
    let desc_a = desc_a_map.get(a_id).unwrap_or(&empty);
    let desc_b = desc_b_map.get(b_id).unwrap_or(&empty);

    let common = desc_a
        .iter()
        .filter(|&&a| {
            matching
                .get_b(a)
                .map(|b| desc_b.contains(&b))
                .unwrap_or(false)
        })
        .count();

    if desc_a.is_empty() && desc_b.is_empty() {
        1.0 // Both are leaves with no descendants
    } else {
        2.0 * common as f64 / (desc_a.len() + desc_b.len()) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::NodeData;

    #[test]
    fn test_identical_trees() {
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "a".to_string()));
        tree_a.add_child(tree_a.root, NodeData::leaf(2, "leaf", "b".to_string()));

        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_b.add_child(tree_b.root, NodeData::leaf(1, "leaf", "a".to_string()));
        tree_b.add_child(tree_b.root, NodeData::leaf(2, "leaf", "b".to_string()));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());

        // All nodes should be matched
        assert_eq!(matching.len(), 3);
    }

    #[test]
    fn test_partial_match() {
        // Trees with same structure but one leaf differs
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        let child1_a = tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "same".to_string()));
        let _child2_a =
            tree_a.add_child(tree_a.root, NodeData::leaf(2, "leaf", "diff_a".to_string()));

        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        let child1_b = tree_b.add_child(tree_b.root, NodeData::leaf(1, "leaf", "same".to_string()));
        let _child2_b =
            tree_b.add_child(tree_b.root, NodeData::leaf(3, "leaf", "diff_b".to_string()));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());

        // The identical leaf should be matched
        assert!(
            matching.contains_a(child1_a),
            "Identical leaves should match"
        );
        assert_eq!(matching.get_b(child1_a), Some(child1_b));
    }
}
