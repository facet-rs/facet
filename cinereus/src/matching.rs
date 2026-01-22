//! GumTree node matching algorithm.
//!
//! Implements two-phase matching:
//! 1. Top-down: Match identical subtrees by hash
//! 2. Bottom-up: Match remaining nodes by structural similarity

use crate::{debug, trace};

use crate::tree::{Properties, Tree, TreeTypes};
use indextree::NodeId;
use rapidhash::{RapidHashMap as HashMap, RapidHashSet as HashSet};
use rayon::prelude::*;

#[cfg(feature = "matching-stats")]
use core::cell::RefCell;

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
pub fn compute_matching<T: TreeTypes>(
    tree_a: &Tree<T>,
    tree_b: &Tree<T>,
    config: &MatchingConfig,
) -> Matching {
    debug!(
        nodes_a = tree_a.arena.count(),
        nodes_b = tree_b.arena.count(),
        "compute_matching start"
    );
    let mut matching = Matching::new();

    // Phase 1: Top-down matching (identical subtrees by hash)
    top_down_phase(tree_a, tree_b, &mut matching, config);
    debug!(matched = matching.len(), "after top_down_phase");

    // Phase 2: Bottom-up matching (similar nodes by Dice coefficient)
    bottom_up_phase(tree_a, tree_b, &mut matching, config);
    debug!(matched = matching.len(), "after bottom_up_phase");

    matching
}

/// Phase 1: Top-down matching.
///
/// Greedily matches nodes with identical subtree hashes, starting from the roots
/// and working down. When two nodes have the same hash, their entire subtrees
/// are identical and can be matched recursively.
fn top_down_phase<T: TreeTypes>(
    tree_a: &Tree<T>,
    tree_b: &Tree<T>,
    matching: &mut Matching,
    config: &MatchingConfig,
) {
    trace!("top_down_phase start");

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
            trace!(a = usize::from(a_id), a_kind = %a_data.kind, b = usize::from(b_id), "top_down: hash match");
            match_subtrees(tree_a, tree_b, a_id, b_id, matching);
        } else {
            trace!(a = usize::from(a_id), a_kind = %a_data.kind, b = usize::from(b_id), b_kind = %b_data.kind, "top_down: no hash match");
            // Hashes differ - try to match children
            // IMPORTANT: Only consider children of b_id, NOT arbitrary nodes from tree B
            // This prevents cross-level matching that causes spurious operations
            for a_child in tree_a.children(a_id) {
                let a_child_data = tree_a.get(a_child);

                for b_child in tree_b.children(b_id) {
                    if !matching.contains_b(b_child) {
                        let b_child_data = tree_b.get(b_child);
                        // Match by hash (exact isomorphism) or kind (structural match)
                        if a_child_data.hash == b_child_data.hash
                            || a_child_data.kind == b_child_data.kind
                        {
                            candidates.push((a_child, b_child));
                        }
                    }
                }
            }
        }
    }
}

/// Match two subtrees recursively (when their hashes match).
fn match_subtrees<T: TreeTypes>(
    tree_a: &Tree<T>,
    tree_b: &Tree<T>,
    a_id: NodeId,
    b_id: NodeId,
    matching: &mut Matching,
) {
    // Skip if either node is already matched (can happen if a descendant was
    // matched earlier due to candidate processing order)
    if matching.contains_a(a_id) || matching.contains_b(b_id) {
        return;
    }

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
fn precompute_descendants<T: TreeTypes>(tree: &Tree<T>) -> DescendantMap {
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

/// Check if B is a valid match for A based on ancestry constraints.
///
/// If A's parent is matched to some node P_b, then B must be a descendant of P_b.
/// This prevents matching nodes across incompatible tree locations.
fn ancestry_compatible<T: TreeTypes>(
    a_id: NodeId,
    b_id: NodeId,
    tree_a: &Tree<T>,
    tree_b: &Tree<T>,
    matching: &Matching,
) -> bool {
    // Check if A's parent is matched
    if let Some(a_parent) = tree_a.parent(a_id)
        && let Some(matched_b_parent) = matching.get_b(a_parent)
    {
        // A's parent is matched to matched_b_parent.
        // B must be a descendant of matched_b_parent.
        let is_descendant = tree_b
            .descendants(matched_b_parent)
            .any(|desc| desc == b_id);
        if !is_descendant {
            trace!(
                a = usize::from(a_id),
                b = usize::from(b_id),
                a_parent = usize::from(a_parent),
                matched_b_parent = usize::from(matched_b_parent),
                "ancestry check failed: B not descendant of matched parent"
            );
            return false;
        }
    }

    // Also check the reverse: if B's parent is matched, A must be a descendant
    // of the corresponding node in tree_a
    if let Some(b_parent) = tree_b.parent(b_id)
        && let Some(matched_a_parent) = matching.get_a(b_parent)
    {
        let is_descendant = tree_a
            .descendants(matched_a_parent)
            .any(|desc| desc == a_id);
        if !is_descendant {
            trace!(
                a = usize::from(a_id),
                b = usize::from(b_id),
                b_parent = usize::from(b_parent),
                matched_a_parent = usize::from(matched_a_parent),
                "ancestry check failed: A not descendant of matched parent"
            );
            return false;
        }
    }

    true
}

/// Phase 2: Bottom-up matching.
///
/// Uses a two-pass approach based on GumTree Simple:
/// 1. First pass: Match internal nodes - prefer position+kind when parent is matched,
///    fall back to Dice coefficient for global matches
/// 2. Second pass: Match leaf nodes (now ancestry constraints are established)
///
/// This prevents cross-level matching of leaves that happen to have the same hash.
fn bottom_up_phase<T: TreeTypes>(
    tree_a: &Tree<T>,
    tree_b: &Tree<T>,
    matching: &mut Matching,
    config: &MatchingConfig,
) {
    // Build index for tree B by kind
    let mut b_by_kind: HashMap<T::Kind, Vec<NodeId>> = HashMap::default();
    for b_id in tree_b.iter() {
        if !matching.contains_b(b_id) {
            let b_data = tree_b.get(b_id);
            b_by_kind.entry(b_data.kind.clone()).or_default().push(b_id);
        }
    }

    // Precompute all descendant sets in parallel
    let desc_a = precompute_descendants(tree_a);
    let desc_b = precompute_descendants(tree_b);

    // PASS 1: Match internal nodes (non-leaves)
    // Use BFS order so parents are matched before children, enabling position-based matching
    for a_id in tree_a.iter() {
        if matching.contains_a(a_id) {
            continue;
        }

        let a_data = tree_a.get(a_id);
        let a_pos = tree_a.position(a_id);

        // Check if parent is matched - enables position-based matching
        let parent_a = tree_a.parent(a_id);
        let matched_parent_b = parent_a.and_then(|p| matching.get_b(p));

        if let Some(parent_b) = matched_parent_b {
            // Parent is matched - try position+kind matching among children of parent_b
            // This is the "unique type among children" heuristic from GumTree Simple
            // Note: We don't require b to have children - a node can go from having children to empty
            let candidates: Vec<NodeId> = tree_b
                .children(parent_b)
                .filter(|&b_id| !matching.contains_b(b_id) && tree_b.get(b_id).kind == a_data.kind)
                .collect();

            // Match by position - don't fall back to first candidate as that can cause
            // incorrect matches between semantically different elements (e.g., two divs
            // with different classes).
            // Also check property compatibility (e.g., elements with different tags shouldn't match)
            let best = candidates
                .iter()
                .find(|&&b_id| {
                    tree_b.position(b_id) == a_pos
                        && a_data.properties.similarity(&tree_b.get(b_id).properties)
                            >= config.similarity_threshold
                })
                .copied();

            if let Some(b_id) = best {
                trace!(
                    a = usize::from(a_id),
                    a_kind = %a_data.kind,
                    b = usize::from(b_id),
                    pos = a_pos,
                    "bottom_up pass1: position+kind match"
                );
                matching.add(a_id, b_id);
                continue;
            }
        }

        // No parent match or no position match - fall back to Dice coefficient
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

            // Check ancestry constraint
            if !ancestry_compatible(a_id, b_id, tree_a, tree_b, matching) {
                continue;
            }

            // Check property compatibility (e.g., elements with different tags shouldn't match)
            let prop_sim = a_data.properties.similarity(&tree_b.get(b_id).properties);
            if prop_sim < config.similarity_threshold {
                continue;
            }

            let score = dice_coefficient(a_id, b_id, matching, &desc_a, &desc_b);
            trace!(
                a = usize::from(a_id),
                a_kind = %a_data.kind,
                b = usize::from(b_id),
                b_kind = %tree_b.get(b_id).kind,
                score,
                "bottom_up pass1: dice score"
            );
            if score >= config.similarity_threshold && (best.is_none() || score > best.unwrap().1) {
                best = Some((b_id, score));
            }
        }

        if let Some((b_id, _score)) = best {
            trace!(
                a = usize::from(a_id),
                a_kind = %a_data.kind,
                b = usize::from(b_id),
                _score,
                "bottom_up pass1: dice match"
            );
            matching.add(a_id, b_id);
        } else if parent_a.is_none() {
            // Root node with no Dice match - match by kind alone if there's a unique candidate
            // This handles the case where trees are structurally different but have same root type
            let root_candidates: Vec<_> = b_by_kind
                .get(&a_data.kind)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|&b_id| {
                    !matching.contains_b(b_id)
                        && tree_b.child_count(b_id) > 0
                        && tree_b.parent(b_id).is_none() // Must also be a root
                        && a_data.properties.similarity(&tree_b.get(b_id).properties)
                            >= config.similarity_threshold
                })
                .collect();

            if let Some(&b_id) = root_candidates.first() {
                trace!(
                    a = usize::from(a_id),
                    a_kind = %a_data.kind,
                    b = usize::from(b_id),
                    "bottom_up pass1: root kind match (fallback)"
                );
                matching.add(a_id, b_id);
            }
        }
    }

    // PASS 2: Match leaf nodes
    // Now that internal nodes are matched, ancestry constraints are established
    for a_id in tree_a.iter() {
        if matching.contains_a(a_id) {
            continue;
        }

        // Only process leaves in this pass
        if tree_a.child_count(a_id) != 0 {
            continue;
        }

        let a_data = tree_a.get(a_id);
        let a_pos = tree_a.position(a_id);

        // Get the parent's matched counterpart to constrain the search
        let parent_a = tree_a.parent(a_id);
        let matched_parent_b = parent_a.and_then(|p| matching.get_b(p));

        // If parent is matched, only search among children of the matched parent
        if let Some(parent_b) = matched_parent_b {
            // First try exact hash match at same position
            let candidates: Vec<NodeId> = tree_b
                .children(parent_b)
                .filter(|&b_id| {
                    !matching.contains_b(b_id)
                        && tree_b.child_count(b_id) == 0
                        && tree_b.get(b_id).kind == a_data.kind
                })
                .collect();

            // Prefer same position with same hash, then same hash, then same kind+position+compatible props
            let best = candidates
                .iter()
                .find(|&&b_id| {
                    tree_b.position(b_id) == a_pos && tree_b.get(b_id).hash == a_data.hash
                })
                .or_else(|| {
                    candidates
                        .iter()
                        .find(|&&b_id| tree_b.get(b_id).hash == a_data.hash)
                })
                .or_else(|| {
                    // Position-only match requires compatible properties
                    candidates.iter().find(|&&b_id| {
                        tree_b.position(b_id) == a_pos
                            && a_data.properties.similarity(&tree_b.get(b_id).properties)
                                >= config.similarity_threshold
                    })
                })
                .copied();

            if let Some(b_id) = best {
                trace!(
                    a = usize::from(a_id),
                    a_kind = %a_data.kind,
                    b = usize::from(b_id),
                    "bottom_up pass2: leaf match"
                );
                matching.add(a_id, b_id);
            }
        }
        // If parent isn't matched, don't try global search - this prevents cross-level matching
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
        DICE_UNIQUE_A.with(|s| {
            s.borrow_mut().insert(a_id);
        });
        DICE_UNIQUE_B.with(|s| {
            s.borrow_mut().insert(b_id);
        });
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
    use crate::tree::{NodeData, SimpleTypes};

    type TestTypes = SimpleTypes<&'static str, String>;

    #[test]
    fn test_identical_trees() {
        let mut tree_a: Tree<TestTypes> = Tree::new(NodeData::simple_u64(100, "root"));
        tree_a.add_child(
            tree_a.root,
            NodeData::simple_leaf_u64(1, "leaf", "a".to_string()),
        );
        tree_a.add_child(
            tree_a.root,
            NodeData::simple_leaf_u64(2, "leaf", "b".to_string()),
        );

        let mut tree_b: Tree<TestTypes> = Tree::new(NodeData::simple_u64(100, "root"));
        tree_b.add_child(
            tree_b.root,
            NodeData::simple_leaf_u64(1, "leaf", "a".to_string()),
        );
        tree_b.add_child(
            tree_b.root,
            NodeData::simple_leaf_u64(2, "leaf", "b".to_string()),
        );

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());

        // All nodes should be matched
        assert_eq!(matching.len(), 3);
    }

    #[test]
    fn test_partial_match() {
        // Trees with same structure but one leaf differs
        let mut tree_a: Tree<TestTypes> = Tree::new(NodeData::simple_u64(100, "root"));
        let child1_a = tree_a.add_child(
            tree_a.root,
            NodeData::simple_leaf_u64(1, "leaf", "same".to_string()),
        );
        let _child2_a = tree_a.add_child(
            tree_a.root,
            NodeData::simple_leaf_u64(2, "leaf", "diff_a".to_string()),
        );

        let mut tree_b: Tree<TestTypes> = Tree::new(NodeData::simple_u64(100, "root"));
        let child1_b = tree_b.add_child(
            tree_b.root,
            NodeData::simple_leaf_u64(1, "leaf", "same".to_string()),
        );
        let _child2_b = tree_b.add_child(
            tree_b.root,
            NodeData::simple_leaf_u64(3, "leaf", "diff_b".to_string()),
        );

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());

        // The identical leaf should be matched
        assert!(
            matching.contains_a(child1_a),
            "Identical leaves should match"
        );
        assert_eq!(matching.get_b(child1_a), Some(child1_b));
    }
}
