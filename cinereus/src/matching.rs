//! GumTree node matching algorithm.
//!
//! Implements two-phase matching:
//! 1. Top-down: Match identical subtrees by hash
//! 2. Bottom-up: Match remaining nodes by structural similarity

use crate::tree::Tree;
use core::hash::Hash;
use indextree::NodeId;
use std::collections::{HashMap, HashSet};

/// A bidirectional mapping between nodes in two trees.
#[derive(Debug, Default)]
pub struct Matching {
    /// Map from tree A node to tree B node
    a_to_b: HashMap<NodeId, NodeId>,
    /// Map from tree B node to tree A node
    b_to_a: HashMap<NodeId, NodeId>,
}

impl Matching {
    /// Create a new empty matching.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a match between two nodes.
    pub fn add(&mut self, a: NodeId, b: NodeId) {
        self.a_to_b.insert(a, b);
        self.b_to_a.insert(b, a);
    }

    /// Check if a node from tree A is matched.
    pub fn contains_a(&self, a: NodeId) -> bool {
        self.a_to_b.contains_key(&a)
    }

    /// Check if a node from tree B is matched.
    pub fn contains_b(&self, b: NodeId) -> bool {
        self.b_to_a.contains_key(&b)
    }

    /// Get the match for a node from tree A.
    pub fn get_b(&self, a: NodeId) -> Option<NodeId> {
        self.a_to_b.get(&a).copied()
    }

    /// Get the match for a node from tree B.
    pub fn get_a(&self, b: NodeId) -> Option<NodeId> {
        self.b_to_a.get(&b).copied()
    }

    /// Get all matched pairs.
    pub fn pairs(&self) -> impl Iterator<Item = (NodeId, NodeId)> + '_ {
        self.a_to_b.iter().map(|(&a, &b)| (a, b))
    }

    /// Get the number of matched pairs.
    pub fn len(&self) -> usize {
        self.a_to_b.len()
    }

    /// Check if there are no matches.
    pub fn is_empty(&self) -> bool {
        self.a_to_b.is_empty()
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
    K: Clone + Eq + Hash,
    L: Clone,
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
    let mut b_by_hash: HashMap<u64, Vec<NodeId>> = HashMap::new();
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

/// Phase 2: Bottom-up matching.
///
/// For unmatched nodes, find candidates with the same kind and compute
/// similarity using the Dice coefficient on matched descendants.
fn bottom_up_phase<K, L>(
    tree_a: &Tree<K, L>,
    tree_b: &Tree<K, L>,
    matching: &mut Matching,
    config: &MatchingConfig,
) where
    K: Clone + Eq + Hash,
    L: Clone,
{
    // Build kind -> unmatched nodes index for tree B
    let mut b_by_kind: HashMap<K, Vec<NodeId>> = HashMap::new();
    for b_id in tree_b.iter() {
        if !matching.contains_b(b_id) {
            let kind = tree_b.get(b_id).kind.clone();
            b_by_kind.entry(kind).or_default().push(b_id);
        }
    }

    // Process tree A in post-order (children before parents)
    for a_id in tree_a.post_order() {
        if matching.contains_a(a_id) {
            continue;
        }

        let a_data = tree_a.get(a_id);

        // Find candidates with same kind
        let candidates = b_by_kind.get(&a_data.kind).cloned().unwrap_or_default();

        // Score candidates by Dice coefficient
        let mut best: Option<(NodeId, f64)> = None;
        for b_id in candidates {
            if matching.contains_b(b_id) {
                continue;
            }

            let score = dice_coefficient(tree_a, tree_b, a_id, b_id, matching);
            if score >= config.similarity_threshold && (best.is_none() || score > best.unwrap().1) {
                best = Some((b_id, score));
            }
        }

        if let Some((b_id, _)) = best {
            matching.add(a_id, b_id);
        }
    }
}

/// Compute the Dice coefficient between two nodes based on matched descendants.
///
/// dice(A, B) = 2 × |matched_descendants| / (|descendants_A| + |descendants_B|)
fn dice_coefficient<K, L>(
    tree_a: &Tree<K, L>,
    tree_b: &Tree<K, L>,
    a_id: NodeId,
    b_id: NodeId,
    matching: &Matching,
) -> f64
where
    K: Clone + Eq + Hash,
    L: Clone,
{
    let desc_a: HashSet<_> = tree_a.descendants(a_id).collect();
    let desc_b: HashSet<_> = tree_b.descendants(b_id).collect();

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
