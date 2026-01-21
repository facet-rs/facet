//! Chawathe edit script generation algorithm.
//!
//! Generates a minimal edit script (INSERT, DELETE, UPDATE, MOVE) from a node matching.
//! Based on "Change Detection in Hierarchically Structured Information" (Chawathe et al., 1996).
//!
//! The algorithm has 5 phases:
//! 1. UPDATE: Change labels of matched nodes where values differ
//! 2. ALIGN: Reorder children to match destination order
//! 3. INSERT: Add nodes that exist only in the destination tree
//! 4. MOVE: Relocate nodes to new parents
//! 5. DELETE: Remove nodes that exist only in the source tree

#[cfg(feature = "tracing")]
use tracing::debug;

#[cfg(not(feature = "tracing"))]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

use crate::matching::Matching;
use crate::tree::Tree;
use core::hash::Hash;
use indextree::NodeId;

/// An edit operation in the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp<K, L> {
    /// Update a node's label (value changed but structure same).
    Update {
        /// The node in tree A that was updated
        node_a: NodeId,
        /// The corresponding node in tree B
        node_b: NodeId,
        /// Old label
        old_label: Option<L>,
        /// New label
        new_label: Option<L>,
    },

    /// Insert a new node.
    Insert {
        /// The new node in tree B
        node_b: NodeId,
        /// Parent in tree B
        parent_b: NodeId,
        /// Position among siblings (0-indexed)
        position: usize,
        /// The node's kind
        kind: K,
        /// The node's label
        label: Option<L>,
    },

    /// Delete a node.
    Delete {
        /// The node in tree A being deleted
        node_a: NodeId,
    },

    /// Move a node to a new location.
    Move {
        /// The node in tree A
        node_a: NodeId,
        /// The corresponding node in tree B
        node_b: NodeId,
        /// New parent in tree B
        new_parent_b: NodeId,
        /// New position among siblings
        new_position: usize,
    },
}

/// Generate an edit script from a matching between two trees.
///
/// The edit script transforms tree A into tree B using INSERT, DELETE, UPDATE, and MOVE operations.
pub fn generate_edit_script<K, L>(
    tree_a: &Tree<K, L>,
    tree_b: &Tree<K, L>,
    matching: &Matching,
) -> Vec<EditOp<K, L>>
where
    K: Clone + Eq + Hash,
    L: Clone + Eq,
{
    debug!(matched_pairs = matching.len(), "generate_edit_script start");
    let mut ops = Vec::new();

    // Phase 1: UPDATE - matched nodes where hash differs (content changed)
    // Note: We compare hashes, not labels, since labels may contain paths
    // which differ even when content is identical
    for (a_id, b_id) in matching.pairs() {
        let a_data = tree_a.get(a_id);
        let b_data = tree_b.get(b_id);

        if a_data.hash != b_data.hash {
            debug!(a = usize::from(a_id), b = usize::from(b_id), a_hash = a_data.hash, b_hash = b_data.hash, "emit UPDATE");
            ops.push(EditOp::Update {
                node_a: a_id,
                node_b: b_id,
                old_label: a_data.label.clone(),
                new_label: b_data.label.clone(),
            });
        }
    }

    // Phase 2 & 3: INSERT - nodes in B that are not matched
    // Process in breadth-first order so parents are inserted before children
    for b_id in tree_b.iter() {
        if !matching.contains_b(b_id) {
            let b_data = tree_b.get(b_id);
            let parent_b = tree_b.parent(b_id);

            if let Some(parent_b) = parent_b {
                let pos = tree_b.position(b_id);
                debug!(b = usize::from(b_id), parent = usize::from(parent_b), pos, "emit INSERT");
                ops.push(EditOp::Insert {
                    node_b: b_id,
                    parent_b,
                    position: pos,
                    kind: b_data.kind.clone(),
                    label: b_data.label.clone(),
                });
            }
            // Root insertion is a special case - usually trees have matching roots
        }
    }

    // Phase 4: MOVE - matched nodes where parent or position changed
    for (a_id, b_id) in matching.pairs() {
        // Skip root
        let Some(parent_a) = tree_a.parent(a_id) else {
            continue;
        };
        let Some(parent_b) = tree_b.parent(b_id) else {
            continue;
        };

        // Check if parent changed
        let parent_match = matching.get_b(parent_a);
        let parent_changed = parent_match != Some(parent_b);

        // Check if position among siblings changed
        let pos_a = tree_a.position(a_id);
        let pos_b = tree_b.position(b_id);
        let position_changed = pos_a != pos_b;

        if parent_changed || position_changed {
            debug!(a = usize::from(a_id), b = usize::from(b_id), parent_changed, pos_a, pos_b, "emit MOVE");
            ops.push(EditOp::Move {
                node_a: a_id,
                node_b: b_id,
                new_parent_b: parent_b,
                new_position: pos_b,
            });
        }
    }

    // Phase 5: DELETE - nodes in A that are not matched
    // Process in post-order so children are deleted before parents
    for a_id in tree_a.post_order() {
        if !matching.contains_a(a_id) {
            debug!(a = usize::from(a_id), "emit DELETE");
            ops.push(EditOp::Delete { node_a: a_id });
        }
    }

    debug!(total_ops = ops.len(), "generate_edit_script done");
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::MatchingConfig;
    use crate::matching::compute_matching;
    use crate::tree::NodeData;

    #[test]
    fn test_no_changes() {
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "a".to_string()));

        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_b.add_child(tree_b.root, NodeData::leaf(1, "leaf", "a".to_string()));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        assert!(ops.is_empty(), "Identical trees should have no edits");
    }

    #[test]
    fn test_update() {
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "old".to_string()));

        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_b.add_child(tree_b.root, NodeData::leaf(2, "leaf", "new".to_string()));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        // Should have an update operation
        let updates: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Update { .. }))
            .collect();
        assert!(!updates.is_empty(), "Should have update operation");
    }

    #[test]
    fn test_insert() {
        let tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));

        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_b.add_child(tree_b.root, NodeData::leaf(1, "leaf", "new".to_string()));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        let inserts: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Insert { .. }))
            .collect();
        assert_eq!(inserts.len(), 1, "Should have one insert operation");
    }

    #[test]
    fn test_delete() {
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));
        tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "old".to_string()));

        let tree_b: Tree<&str, String> = Tree::new(NodeData::new(100, "root"));

        let matching = compute_matching(&tree_a, &tree_b, &MatchingConfig::default());
        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        let deletes: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, EditOp::Delete { .. }))
            .collect();
        assert_eq!(deletes.len(), 1, "Should have one delete operation");
    }

    #[test]
    fn test_swap_two_siblings() {
        // Tree A: root -> [child_a at pos 0, child_b at pos 1]
        // Tree B: root -> [child_b at pos 0, child_a at pos 1]
        // This tests the swap scenario to understand Move semantics

        // Root hashes must differ (otherwise top-down recursively matches children BY POSITION).
        // With min_height=0, leaves are included in top-down and matched by hash.
        let mut tree_a: Tree<&str, &str> = Tree::new(NodeData::new(100, "root"));
        let child_a = tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "A"));
        let child_b = tree_a.add_child(tree_a.root, NodeData::leaf(2, "leaf", "B"));

        let mut tree_b: Tree<&str, &str> = Tree::new(NodeData::new(200, "root")); // Different root hash!
        // Swap order: B first, then A
        let child_b2 = tree_b.add_child(tree_b.root, NodeData::leaf(2, "leaf", "B"));
        let child_a2 = tree_b.add_child(tree_b.root, NodeData::leaf(1, "leaf", "A"));

        let config = MatchingConfig {
            min_height: 0, // Include leaves in top-down matching
            ..Default::default()
        };
        let matching = compute_matching(&tree_a, &tree_b, &config);

        // Debug: print tree structure and matching
        debug!(?tree_a.root, "tree_a root");
        debug!(?child_a, hash = tree_a.get(child_a).hash, pos = tree_a.position(child_a), "tree_a child_a");
        debug!(?child_b, hash = tree_a.get(child_b).hash, pos = tree_a.position(child_b), "tree_a child_b");
        debug!(?tree_b.root, "tree_b root");
        debug!(?child_b2, hash = tree_b.get(child_b2).hash, pos = tree_b.position(child_b2), "tree_b child_b2");
        debug!(?child_a2, hash = tree_b.get(child_a2).hash, pos = tree_b.position(child_a2), "tree_b child_a2");
        for (a, b) in matching.pairs() {
            debug!(?a, ?b, "matching pair");
        }

        // Verify matching is correct
        assert_eq!(matching.get_b(child_a), Some(child_a2), "child_a should match child_a2");
        assert_eq!(matching.get_b(child_b), Some(child_b2), "child_b should match child_b2");

        // Verify positions in original trees
        assert_eq!(tree_a.position(child_a), 0, "child_a at pos 0 in tree_a");
        assert_eq!(tree_a.position(child_b), 1, "child_b at pos 1 in tree_a");
        assert_eq!(tree_b.position(child_a2), 1, "child_a2 at pos 1 in tree_b");
        assert_eq!(tree_b.position(child_b2), 0, "child_b2 at pos 0 in tree_b");

        let ops = generate_edit_script(&tree_a, &tree_b, &matching);

        for op in &ops {
            debug!(?op, "edit script op");
        }

        // Filter move operations
        let moves: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                EditOp::Move { node_a, node_b, new_parent_b, new_position } => {
                    Some((*node_a, *node_b, *new_parent_b, *new_position))
                }
                _ => None,
            })
            .collect();

        // Key question: What does cinereus emit for a swap?
        // - Move for child_a: was at pos 0, should be at pos 1
        // - Move for child_b: was at pos 1, should be at pos 0
        //
        // The new_position field comes from tree_b.position(b_id), which is the
        // FINAL position in the target tree, not an intermediate position.

        assert_eq!(moves.len(), 2, "Should have two move operations for a swap");

        // Find move for child_a (hash 1)
        let move_a = moves.iter().find(|(a, _, _, _)| *a == child_a);
        assert!(move_a.is_some(), "Should have move for child_a");
        let (_, _, _, new_pos_a) = move_a.unwrap();
        assert_eq!(*new_pos_a, 1, "child_a should move to position 1");

        // Find move for child_b (hash 2)
        let move_b = moves.iter().find(|(a, _, _, _)| *a == child_b);
        assert!(move_b.is_some(), "Should have move for child_b");
        let (_, _, _, new_pos_b) = move_b.unwrap();
        assert_eq!(*new_pos_b, 0, "child_b should move to position 0");
    }
}
