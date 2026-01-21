//! Edit script simplification.
//!
//! Consolidates redundant operations to produce cleaner diffs:
//! - When a subtree is inserted, don't report individual child inserts
//! - When a subtree is deleted, don't report individual child deletes
//! - When a subtree is moved, don't report individual child moves

use crate::chawathe::EditOp;
use crate::tree::{Properties, Tree};
use core::hash::Hash;
use indextree::NodeId;
use std::collections::HashSet;

/// Simplify an edit script by consolidating subtree operations.
///
/// This removes redundant child operations when a parent operation already
/// covers the entire subtree.
pub fn simplify_edit_script<K, L, P>(
    ops: Vec<EditOp<K, L, P>>,
    tree_a: &Tree<K, L, P>,
    tree_b: &Tree<K, L, P>,
) -> Vec<EditOp<K, L, P>>
where
    K: Clone + Eq + Hash,
    L: Clone + Eq,
    P: Properties,
{
    // Collect all nodes involved in each operation type
    let mut inserted_nodes: HashSet<NodeId> = HashSet::new();
    let mut deleted_nodes: HashSet<NodeId> = HashSet::new();
    let mut moved_nodes_b: HashSet<NodeId> = HashSet::new();

    for op in &ops {
        match op {
            EditOp::Insert { node_b, .. } => {
                inserted_nodes.insert(*node_b);
            }
            EditOp::Delete { node_a } => {
                deleted_nodes.insert(*node_a);
            }
            EditOp::Move { node_b, .. } => {
                moved_nodes_b.insert(*node_b);
            }
            EditOp::Update { .. } | EditOp::UpdateProperty { .. } => {}
        }
    }

    // Find "root" operations - those whose parent is not also in the set
    let root_inserts: HashSet<NodeId> = inserted_nodes
        .iter()
        .filter(|&&node| {
            tree_b
                .parent(node)
                .map(|p| !inserted_nodes.contains(&p))
                .unwrap_or(true)
        })
        .copied()
        .collect();

    let root_deletes: HashSet<NodeId> = deleted_nodes
        .iter()
        .filter(|&&node| {
            tree_a
                .parent(node)
                .map(|p| !deleted_nodes.contains(&p))
                .unwrap_or(true)
        })
        .copied()
        .collect();

    let root_moves: HashSet<NodeId> = moved_nodes_b
        .iter()
        .filter(|&&node| {
            tree_b
                .parent(node)
                .map(|p| !moved_nodes_b.contains(&p))
                .unwrap_or(true)
        })
        .copied()
        .collect();

    // Filter operations to only include roots
    ops.into_iter()
        .filter(|op| match op {
            EditOp::Insert { node_b, .. } => root_inserts.contains(node_b),
            EditOp::Delete { node_a } => root_deletes.contains(node_a),
            EditOp::Move { node_b, .. } => root_moves.contains(node_b),
            EditOp::Update { .. } | EditOp::UpdateProperty { .. } => true, // Always keep updates
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::NodeData;

    #[test]
    fn test_simplify_subtree_insert() {
        // Tree B has a subtree: parent -> child1, child2
        let mut tree_b: Tree<&str, String> = Tree::new(NodeData::new(0, "root"));
        let parent = tree_b.add_child(tree_b.root, NodeData::new(1, "parent"));
        let child1 = tree_b.add_child(parent, NodeData::leaf(2, "leaf", "a".to_string()));
        let child2 = tree_b.add_child(parent, NodeData::leaf(3, "leaf", "b".to_string()));

        // Empty tree A for reference
        let tree_a: Tree<&str, String> = Tree::new(NodeData::new(0, "root"));

        // Simulate raw ops: insert parent, insert child1, insert child2
        let ops: Vec<EditOp<&str, String>> = vec![
            EditOp::Insert {
                node_b: parent,
                parent_b: tree_b.root,
                position: 0,
                kind: "parent",
                label: None,
            },
            EditOp::Insert {
                node_b: child1,
                parent_b: parent,
                position: 0,
                kind: "leaf",
                label: Some("a".to_string()),
            },
            EditOp::Insert {
                node_b: child2,
                parent_b: parent,
                position: 1,
                kind: "leaf",
                label: Some("b".to_string()),
            },
        ];

        let simplified = simplify_edit_script(ops, &tree_a, &tree_b);

        // Should only have the parent insert
        assert_eq!(simplified.len(), 1);
        assert!(matches!(
            &simplified[0],
            EditOp::Insert { node_b, .. } if *node_b == parent
        ));
    }

    #[test]
    fn test_simplify_subtree_delete() {
        // Tree A has a subtree: parent -> child1, child2
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(0, "root"));
        let parent = tree_a.add_child(tree_a.root, NodeData::new(1, "parent"));
        let child1 = tree_a.add_child(parent, NodeData::leaf(2, "leaf", "a".to_string()));
        let child2 = tree_a.add_child(parent, NodeData::leaf(3, "leaf", "b".to_string()));

        // Empty tree B for reference
        let tree_b: Tree<&str, String> = Tree::new(NodeData::new(0, "root"));

        // Simulate raw ops: delete child1, delete child2, delete parent (post-order)
        let ops: Vec<EditOp<&str, String>> = vec![
            EditOp::Delete { node_a: child1 },
            EditOp::Delete { node_a: child2 },
            EditOp::Delete { node_a: parent },
        ];

        let simplified = simplify_edit_script(ops, &tree_a, &tree_b);

        // Should only have the parent delete
        assert_eq!(simplified.len(), 1);
        assert!(matches!(
            &simplified[0],
            EditOp::Delete { node_a } if *node_a == parent
        ));
    }

    #[test]
    fn test_simplify_keeps_independent_ops() {
        let mut tree_a: Tree<&str, String> = Tree::new(NodeData::new(0, "root"));
        let a1 = tree_a.add_child(tree_a.root, NodeData::leaf(1, "leaf", "a".to_string()));
        let a2 = tree_a.add_child(tree_a.root, NodeData::leaf(2, "leaf", "b".to_string()));

        let tree_b: Tree<&str, String> = Tree::new(NodeData::new(0, "root"));

        // Two independent deletes (siblings, not parent-child)
        let ops: Vec<EditOp<&str, String>> =
            vec![EditOp::Delete { node_a: a1 }, EditOp::Delete { node_a: a2 }];

        let simplified = simplify_edit_script(ops, &tree_a, &tree_b);

        // Both should remain since they're independent
        assert_eq!(simplified.len(), 2);
    }
}
