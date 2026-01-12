//! Tree representation for diffing.
//!
//! Uses `indextree` as the arena backend for efficient tree storage.

use core::hash::Hash;
use indextree::{Arena, NodeId};

/// Data stored in each tree node.
///
/// This is the minimal information needed for the GumTree/Chawathe algorithms:
/// - A structural hash for fast equality checking
/// - A "kind" for type-based matching (nodes of different kinds don't match)
/// - An optional label for leaf nodes (the actual value)
#[derive(Debug, Clone)]
pub struct NodeData<K, L> {
    /// Structural hash of this node and all its descendants (Merkle-tree style).
    /// Two nodes with the same hash are structurally identical.
    pub hash: u64,

    /// The kind/type of this node.
    /// Used during matching: only nodes of the same kind can match.
    pub kind: K,

    /// Optional label for leaf nodes.
    /// For internal nodes, this might be None or a type name.
    /// For leaf nodes, this is the actual value (as a string or comparable form).
    pub label: Option<L>,
}

impl<K, L> NodeData<K, L> {
    /// Create a new node with the given hash and kind.
    pub const fn new(hash: u64, kind: K) -> Self {
        Self {
            hash,
            kind,
            label: None,
        }
    }

    /// Create a new leaf node with a label.
    pub const fn leaf(hash: u64, kind: K, label: L) -> Self {
        Self {
            hash,
            kind,
            label: Some(label),
        }
    }
}

/// A tree structure for diffing.
///
/// Wraps an `indextree::Arena` with a designated root node.
#[derive(Debug)]
pub struct Tree<K, L> {
    /// The arena storing all nodes.
    pub arena: Arena<NodeData<K, L>>,
    /// The root node ID.
    pub root: NodeId,
}

impl<K, L> Tree<K, L>
where
    K: Clone + Eq + Hash,
    L: Clone,
{
    /// Create a new tree with a single root node.
    pub fn new(root_data: NodeData<K, L>) -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(root_data);
        Self { arena, root }
    }

    /// Add a child node to a parent.
    pub fn add_child(&mut self, parent: NodeId, data: NodeData<K, L>) -> NodeId {
        let child = self.arena.new_node(data);
        parent.append(child, &mut self.arena);
        child
    }

    /// Get the data for a node.
    pub fn get(&self, id: NodeId) -> &NodeData<K, L> {
        self.arena.get(id).expect("invalid node id").get()
    }

    /// Get the parent of a node.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.arena.get(id).and_then(|n| n.parent())
    }

    /// Get the children of a node.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        id.children(&self.arena)
    }

    /// Get the number of children of a node.
    pub fn child_count(&self, id: NodeId) -> usize {
        id.children(&self.arena).count()
    }

    /// Get the position of a node among its siblings (0-indexed).
    pub fn position(&self, id: NodeId) -> usize {
        if let Some(parent) = self.parent(id) {
            parent
                .children(&self.arena)
                .position(|c| c == id)
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// Iterate all nodes in the tree.
    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.root.descendants(&self.arena)
    }

    /// Iterate nodes in post-order (children before parents).
    /// This is needed for bottom-up matching.
    pub fn post_order(&self) -> impl Iterator<Item = NodeId> + '_ {
        PostOrderIter::new(self.root, &self.arena)
    }

    /// Get all descendants of a node (including the node itself).
    pub fn descendants(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        id.descendants(&self.arena)
    }

    /// Get the height of a node (distance to furthest leaf).
    pub fn height(&self, id: NodeId) -> usize {
        let children: Vec<_> = self.children(id).collect();
        if children.is_empty() {
            0
        } else {
            1 + children.iter().map(|&c| self.height(c)).max().unwrap_or(0)
        }
    }
}

/// Post-order iterator over tree nodes.
struct PostOrderIter<'a, K, L> {
    arena: &'a Arena<NodeData<K, L>>,
    stack: Vec<(NodeId, bool)>, // (node_id, children_visited)
}

impl<'a, K, L> PostOrderIter<'a, K, L> {
    fn new(root: NodeId, arena: &'a Arena<NodeData<K, L>>) -> Self {
        Self {
            arena,
            stack: vec![(root, false)],
        }
    }
}

impl<'a, K, L> Iterator for PostOrderIter<'a, K, L> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((id, children_visited)) = self.stack.pop() {
            if children_visited {
                return Some(id);
            }
            // Push this node back with children_visited = true
            self.stack.push((id, true));
            // Push children in reverse order so they come out in order
            let children: Vec<_> = id.children(self.arena).collect();
            for child in children.into_iter().rev() {
                self.stack.push((child, false));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_basics() {
        let mut tree: Tree<&str, String> = Tree::new(NodeData::new(0, "root"));

        let child1 = tree.add_child(tree.root, NodeData::leaf(1, "leaf", "a".to_string()));
        let child2 = tree.add_child(tree.root, NodeData::leaf(2, "leaf", "b".to_string()));

        assert_eq!(tree.child_count(tree.root), 2);
        assert_eq!(tree.position(child1), 0);
        assert_eq!(tree.position(child2), 1);
        assert_eq!(tree.parent(child1), Some(tree.root));
        assert_eq!(tree.height(tree.root), 1);
    }

    #[test]
    fn test_post_order() {
        let mut tree: Tree<&str, String> = Tree::new(NodeData::new(0, "root"));

        let child1 = tree.add_child(tree.root, NodeData::new(1, "node"));
        let _leaf1 = tree.add_child(child1, NodeData::leaf(2, "leaf", "a".to_string()));
        let _leaf2 = tree.add_child(child1, NodeData::leaf(3, "leaf", "b".to_string()));
        let _child2 = tree.add_child(tree.root, NodeData::leaf(4, "leaf", "c".to_string()));

        let order: Vec<_> = tree.post_order().collect();
        // Post-order: leaves first, then parents
        // leaf1, leaf2, child1, child2, root
        assert_eq!(order.len(), 5);
        assert_eq!(order.last(), Some(&tree.root));
    }
}
