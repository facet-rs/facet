//! Tree representation for diffing.
//!
//! Uses `indextree` as the arena backend for efficient tree storage.

use core::fmt::{self, Display};
use core::hash::Hash;
use indextree::{Arena, NodeId};

/// Trait that bundles all the type parameters for a tree.
///
/// This simplifies function signatures by replacing multiple generic parameters
/// with a single `T: TreeTypes` bound.
///
/// # Example
///
/// ```ignore
/// struct MyTreeTypes;
///
/// impl TreeTypes for MyTreeTypes {
///     type Kind = NodeKind;
///     type Label = NodeLabel;
///     type Props = HtmlProperties;
/// }
///
/// // Now use Tree<MyTreeTypes> instead of Tree<NodeKind, NodeLabel, HtmlProperties>
/// ```
pub trait TreeTypes {
    /// The kind/type of nodes (e.g., "div", "span" for HTML).
    /// Used during matching: only nodes of the same kind can match.
    type Kind: Clone + Eq + Hash + Display + Send + Sync;

    /// The label type for leaf nodes (the actual value).
    type Label: Clone + Eq + Display + Send + Sync;

    /// The properties type for key-value pairs attached to nodes.
    type Props: Properties + Send + Sync;
}

/// A structural hash of a node and all its descendants (Merkle-tree style).
/// Two nodes with the same hash are structurally identical.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NodeHash(pub u64);

impl fmt::Debug for NodeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeHash({:#018x})", self.0)
    }
}

impl fmt::Display for NodeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#018x}", self.0)
    }
}

impl From<u64> for NodeHash {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<NodeHash> for u64 {
    fn from(value: NodeHash) -> Self {
        value.0
    }
}

/// A property change detected between matched nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyChange<K, V> {
    /// The property key
    pub key: K,
    /// The old value (None if property was added)
    pub old_value: Option<V>,
    /// The new value (None if property was removed)
    pub new_value: Option<V>,
}

/// Trait for node properties (key-value pairs that are NOT tree children).
///
/// Properties are compared field-by-field when nodes match, generating
/// granular update operations. This avoids the cross-matching problem
/// where identical values (like None) get matched across different fields.
pub trait Properties: Clone {
    /// The key type for properties (e.g., &'static str for attribute names)
    type Key: Clone + Eq + Hash + Display;
    /// The value type for properties
    type Value: Clone + Eq + Display;

    /// Compute similarity between two property sets (0.0 to 1.0).
    /// Used during bottom-up matching to prefer nodes with similar properties.
    fn similarity(&self, other: &Self) -> f64;

    /// Find all property differences between self and other.
    /// Returns changes needed to transform self into other.
    fn diff(&self, other: &Self) -> Vec<PropertyChange<Self::Key, Self::Value>>;

    /// Check if this property set is empty (no properties defined).
    fn is_empty(&self) -> bool;
}

/// A placeholder type for "no key" that implements Display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct NoKey;

impl Display for NoKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(none)")
    }
}

/// A placeholder type for "no value" that implements Display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoVal;

impl Display for NoVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(none)")
    }
}

/// Default "no properties" type for backward compatibility.
///
/// Nodes without properties behave exactly as before.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoProps;

impl Properties for NoProps {
    type Key = NoKey;
    type Value = NoVal;

    fn similarity(&self, _other: &Self) -> f64 {
        1.0 // No properties = perfect match
    }

    fn diff(&self, _other: &Self) -> Vec<PropertyChange<Self::Key, Self::Value>> {
        vec![] // No properties = no changes
    }

    fn is_empty(&self) -> bool {
        true
    }
}

/// A simple tree types marker for trees with specific K, L, P types.
///
/// This allows using `Tree<SimpleTypes<K, L, P>>` which is equivalent to
/// the old `Tree<K, L, P>` signature.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimpleTypes<K, L, P = NoProps>(core::marker::PhantomData<(K, L, P)>);

impl<K, L, P> TreeTypes for SimpleTypes<K, L, P>
where
    K: Clone + Eq + Hash + Display + Send + Sync,
    L: Clone + Eq + Display + Send + Sync,
    P: Properties + Send + Sync,
{
    type Kind = K;
    type Label = L;
    type Props = P;
}

/// Data stored in each tree node.
///
/// This is the minimal information needed for the GumTree/Chawathe algorithms:
/// - A structural hash for fast equality checking
/// - A "kind" for type-based matching (nodes of different kinds don't match)
/// - An optional label for leaf nodes (the actual value)
/// - Properties: key-value pairs that are NOT tree children
#[derive(Debug)]
pub struct NodeData<T: TreeTypes> {
    /// Structural hash of this node and all its descendants (Merkle-tree style).
    /// Two nodes with the same hash are structurally identical.
    pub hash: NodeHash,

    /// The kind/type of this node.
    /// Used during matching: only nodes of the same kind can match.
    pub kind: T::Kind,

    /// Optional label for leaf nodes.
    /// For internal nodes, this might be None or a type name.
    /// For leaf nodes, this is the actual value (as a string or comparable form).
    pub label: Option<T::Label>,

    /// Properties: key-value pairs attached to this node.
    /// Unlike children, properties are diffed field-by-field when nodes match.
    pub properties: T::Props,
}

impl<T: TreeTypes> Clone for NodeData<T> {
    fn clone(&self) -> Self {
        Self {
            hash: self.hash,
            kind: self.kind.clone(),
            label: self.label.clone(),
            properties: self.properties.clone(),
        }
    }
}

impl<T: TreeTypes> NodeData<T> {
    /// Create a new node with the given hash and kind.
    pub fn new(hash: NodeHash, kind: T::Kind, properties: T::Props) -> Self {
        Self {
            hash,
            kind,
            label: None,
            properties,
        }
    }

    /// Create a new node with the given hash (as u64) and kind.
    pub fn new_u64(hash: u64, kind: T::Kind, properties: T::Props) -> Self {
        Self {
            hash: NodeHash(hash),
            kind,
            label: None,
            properties,
        }
    }

    /// Create a new leaf node with a label.
    pub fn leaf(hash: NodeHash, kind: T::Kind, label: T::Label, properties: T::Props) -> Self {
        Self {
            hash,
            kind,
            label: Some(label),
            properties,
        }
    }

    /// Create a new leaf node with a label, hash as u64.
    pub fn leaf_u64(hash: u64, kind: T::Kind, label: T::Label, properties: T::Props) -> Self {
        Self {
            hash: NodeHash(hash),
            kind,
            label: Some(label),
            properties,
        }
    }
}

/// Convenience constructors for trees without properties.
impl<K, L> NodeData<SimpleTypes<K, L>>
where
    K: Clone + Eq + Hash + Display + Send + Sync,
    L: Clone + Eq + Display + Send + Sync,
{
    /// Create a new node with no properties.
    pub fn simple(hash: NodeHash, kind: K) -> Self {
        Self {
            hash,
            kind,
            label: None,
            properties: NoProps,
        }
    }

    /// Create a new node with no properties, hash as u64.
    pub fn simple_u64(hash: u64, kind: K) -> Self {
        Self {
            hash: NodeHash(hash),
            kind,
            label: None,
            properties: NoProps,
        }
    }

    /// Create a new leaf node with no properties.
    pub fn simple_leaf(hash: NodeHash, kind: K, label: L) -> Self {
        Self {
            hash,
            kind,
            label: Some(label),
            properties: NoProps,
        }
    }

    /// Create a new leaf node with no properties, hash as u64.
    pub fn simple_leaf_u64(hash: u64, kind: K, label: L) -> Self {
        Self {
            hash: NodeHash(hash),
            kind,
            label: Some(label),
            properties: NoProps,
        }
    }
}

/// A tree structure for diffing.
///
/// Wraps an `indextree::Arena` with a designated root node.
pub struct Tree<T: TreeTypes> {
    /// The arena storing all nodes.
    pub arena: Arena<NodeData<T>>,
    /// The root node ID.
    pub root: NodeId,
}

impl<T: TreeTypes> fmt::Debug for Tree<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tree")
            .field("root", &self.root)
            .field("node_count", &self.arena.count())
            .finish()
    }
}

impl<T: TreeTypes> Tree<T> {
    /// Create a new tree with a single root node.
    pub fn new(root_data: NodeData<T>) -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(root_data);
        Self { arena, root }
    }

    /// Add a child node to a parent.
    pub fn add_child(&mut self, parent: NodeId, data: NodeData<T>) -> NodeId {
        let child = self.arena.new_node(data);
        parent.append(child, &mut self.arena);
        child
    }

    /// Get the data for a node.
    pub fn get(&self, id: NodeId) -> &NodeData<T> {
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
struct PostOrderIter<'a, T: TreeTypes> {
    arena: &'a Arena<NodeData<T>>,
    stack: Vec<(NodeId, bool)>, // (node_id, children_visited)
}

impl<'a, T: TreeTypes> PostOrderIter<'a, T> {
    fn new(root: NodeId, arena: &'a Arena<NodeData<T>>) -> Self {
        Self {
            arena,
            stack: vec![(root, false)],
        }
    }
}

impl<T: TreeTypes> Iterator for PostOrderIter<'_, T> {
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

    type TestTypes = SimpleTypes<&'static str, String>;

    #[test]
    fn test_tree_basics() {
        let mut tree: Tree<TestTypes> = Tree::new(NodeData::simple_u64(0, "root"));

        let child1 = tree.add_child(
            tree.root,
            NodeData::simple_leaf_u64(1, "leaf", "a".to_string()),
        );
        let child2 = tree.add_child(
            tree.root,
            NodeData::simple_leaf_u64(2, "leaf", "b".to_string()),
        );

        assert_eq!(tree.child_count(tree.root), 2);
        assert_eq!(tree.position(child1), 0);
        assert_eq!(tree.position(child2), 1);
        assert_eq!(tree.parent(child1), Some(tree.root));
        assert_eq!(tree.height(tree.root), 1);
    }

    #[test]
    fn test_post_order() {
        let mut tree: Tree<TestTypes> = Tree::new(NodeData::simple_u64(0, "root"));

        let child1 = tree.add_child(tree.root, NodeData::simple_u64(1, "node"));
        let _leaf1 = tree.add_child(
            child1,
            NodeData::simple_leaf_u64(2, "leaf", "a".to_string()),
        );
        let _leaf2 = tree.add_child(
            child1,
            NodeData::simple_leaf_u64(3, "leaf", "b".to_string()),
        );
        let _child2 = tree.add_child(
            tree.root,
            NodeData::simple_leaf_u64(4, "leaf", "c".to_string()),
        );

        let order: Vec<_> = tree.post_order().collect();
        // Post-order: leaves first, then parents
        // leaf1, leaf2, child1, child2, root
        assert_eq!(order.len(), 5);
        assert_eq!(order.last(), Some(&tree.root));
    }
}
