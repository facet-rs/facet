//! GumTree-style tree diffing algorithm.
//!
//! This module implements a tree diff algorithm based on the GumTree paper
//! (ICSE 2014). It works in phases:
//!
//! 1. **Build trees**: Convert Peek values into a tree representation with hashes
//! 2. **Top-down matching**: Match nodes with identical hashes (identical subtrees)
//! 3. **Bottom-up matching**: Match remaining nodes by similarity
//! 4. **Edit script**: Generate insert/delete/update/move operations

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hasher};

use facet_reflect::Peek;

/// Unique identifier for a node in the tree
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(usize);

/// A path segment describing how to reach a child
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// A named field in a struct
    Field(&'static str),
    /// An index in a list/array
    Index(usize),
    /// A key in a map
    Key(String),
    /// An enum variant
    Variant(&'static str),
}

/// A path from root to a node
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Path(pub Vec<PathSegment>);

impl Path {
    /// Create a new empty path
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Append a segment to this path
    pub fn push(&mut self, segment: PathSegment) {
        self.0.push(segment);
    }

    /// Create a new path with an additional segment
    pub fn with(&self, segment: PathSegment) -> Self {
        let mut new = self.clone();
        new.push(segment);
        new
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, segment) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            match segment {
                PathSegment::Field(name) => write!(f, "{}", name)?,
                PathSegment::Index(idx) => write!(f, "[{}]", idx)?,
                PathSegment::Key(key) => write!(f, "[{:?}]", key)?,
                PathSegment::Variant(name) => write!(f, "::{}", name)?,
            }
        }
        Ok(())
    }
}

/// The kind of node (for type-based matching)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A struct with the given type name
    Struct(&'static str),
    /// An enum variant
    EnumVariant(&'static str, &'static str), // (enum_name, variant_name)
    /// A list/array element
    ListElement,
    /// A map entry
    MapEntry,
    /// A scalar value
    Scalar(&'static str),
    /// Root node
    Root,
}

/// A node in the tree representation
#[derive(Debug)]
pub struct TreeNode {
    /// Unique identifier
    pub id: NodeId,
    /// The kind of this node
    pub kind: NodeKind,
    /// Structural hash of this node and all descendants
    pub hash: u64,
    /// Height of this node (leaves = 0, internal = max child height + 1)
    pub height: usize,
    /// Path from root to this node
    pub path: Path,
    /// Children of this node
    pub children: Vec<NodeId>,
    /// Parent of this node (None for root)
    pub parent: Option<NodeId>,
}

/// A tree built from a Peek value
#[derive(Debug)]
pub struct Tree {
    /// All nodes in the tree
    nodes: Vec<TreeNode>,
    /// The root node ID
    pub root: NodeId,
}

impl Tree {
    /// Get a node by ID
    pub fn get(&self, id: NodeId) -> &TreeNode {
        &self.nodes[id.0]
    }

    /// Get all nodes
    pub fn nodes(&self) -> impl Iterator<Item = &TreeNode> {
        self.nodes.iter()
    }

    /// Get nodes in post-order (children before parents)
    pub fn post_order(&self) -> impl Iterator<Item = &TreeNode> {
        PostOrderIter::new(self)
    }

    /// Get the number of nodes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get all descendants of a node (including the node itself)
    pub fn descendants(&self, id: NodeId) -> HashSet<NodeId> {
        let mut result = HashSet::new();
        self.collect_descendants(id, &mut result);
        result
    }

    fn collect_descendants(&self, id: NodeId, result: &mut HashSet<NodeId>) {
        result.insert(id);
        for &child_id in &self.nodes[id.0].children {
            self.collect_descendants(child_id, result);
        }
    }
}

/// Post-order iterator over tree nodes
struct PostOrderIter<'a> {
    tree: &'a Tree,
    stack: Vec<(NodeId, bool)>, // (node_id, children_visited)
}

impl<'a> PostOrderIter<'a> {
    fn new(tree: &'a Tree) -> Self {
        let mut stack = Vec::new();
        if !tree.nodes.is_empty() {
            stack.push((tree.root, false));
        }
        Self { tree, stack }
    }
}

impl<'a> Iterator for PostOrderIter<'a> {
    type Item = &'a TreeNode;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((id, children_visited)) = self.stack.pop() {
            if children_visited {
                return Some(&self.tree.nodes[id.0]);
            }
            // Push this node back with children_visited = true
            self.stack.push((id, true));
            // Push children (in reverse order so they come out in order)
            for &child_id in self.tree.nodes[id.0].children.iter().rev() {
                self.stack.push((child_id, false));
            }
        }
        None
    }
}

/// Builder for constructing a Tree from a Peek value
pub struct TreeBuilder {
    nodes: Vec<TreeNode>,
}

impl TreeBuilder {
    /// Build a tree from a Peek value
    pub fn build<'mem, 'facet>(peek: Peek<'mem, 'facet>) -> Tree {
        let mut builder = TreeBuilder { nodes: Vec::new() };
        let root = builder.build_node(peek, None, Path::new());
        Tree {
            nodes: builder.nodes,
            root,
        }
    }

    fn build_node<'mem, 'facet>(
        &mut self,
        peek: Peek<'mem, 'facet>,
        parent: Option<NodeId>,
        path: Path,
    ) -> NodeId {
        use facet_core::{Def, StructKind, Type, UserType};
        use facet_reflect::HasFields;

        let id = NodeId(self.nodes.len());

        // Determine the kind based on the shape
        let kind = match peek.shape().ty {
            Type::User(UserType::Struct(_)) => NodeKind::Struct(peek.shape().type_identifier),
            Type::User(UserType::Enum(_)) => {
                if let Ok(e) = peek.into_enum() {
                    if let Ok(variant) = e.active_variant() {
                        NodeKind::EnumVariant(peek.shape().type_identifier, variant.name)
                    } else {
                        NodeKind::Scalar(peek.shape().type_identifier)
                    }
                } else {
                    NodeKind::Scalar(peek.shape().type_identifier)
                }
            }
            _ => match peek.shape().def {
                Def::List(_) | Def::Array(_) | Def::Slice(_) => {
                    NodeKind::Scalar(peek.shape().type_identifier)
                }
                Def::Map(_) => NodeKind::Scalar(peek.shape().type_identifier),
                _ => NodeKind::Scalar(peek.shape().type_identifier),
            },
        };

        // Create placeholder node (we'll fill in hash and height after children)
        self.nodes.push(TreeNode {
            id,
            kind: kind.clone(),
            hash: 0,
            height: 0,
            path: path.clone(),
            children: Vec::new(),
            parent,
        });

        // Build children and collect their info
        let mut children = Vec::new();
        let mut max_child_height = 0;

        match peek.shape().ty {
            Type::User(UserType::Struct(_)) => {
                // Struct: add each field as a child
                if let Ok(s) = peek.into_struct() {
                    for (field, field_peek) in s.fields() {
                        let child_path = path.with(PathSegment::Field(field.name));
                        let child_id = self.build_node(field_peek, Some(id), child_path);
                        children.push(child_id);
                        max_child_height = max_child_height.max(self.nodes[child_id.0].height);
                    }
                }
            }
            Type::User(UserType::Enum(_)) => {
                // Enum: add variant fields as children
                if let Ok(e) = peek.into_enum()
                    && let Ok(variant) = e.active_variant()
                {
                    let variant_path = path.with(PathSegment::Variant(variant.name));
                    for (i, (field, field_peek)) in e.fields().enumerate() {
                        let child_path = if variant.data.kind == StructKind::Struct {
                            variant_path.with(PathSegment::Field(field.name))
                        } else {
                            variant_path.with(PathSegment::Index(i))
                        };
                        let child_id = self.build_node(field_peek, Some(id), child_path);
                        children.push(child_id);
                        max_child_height = max_child_height.max(self.nodes[child_id.0].height);
                    }
                }
            }
            _ => {
                // Handle Def-based types
                match peek.shape().def {
                    Def::List(_) | Def::Array(_) | Def::Slice(_) => {
                        if let Ok(list) = peek.into_list_like() {
                            for (i, elem) in list.iter().enumerate() {
                                let child_path = path.with(PathSegment::Index(i));
                                let child_id = self.build_node(elem, Some(id), child_path);
                                children.push(child_id);
                                max_child_height =
                                    max_child_height.max(self.nodes[child_id.0].height);
                            }
                        }
                    }
                    Def::Map(_) => {
                        if let Ok(map) = peek.into_map() {
                            for (key, value) in map.iter() {
                                // Use the key's string representation
                                let key_str = format!("{:?}", key);
                                let child_path = path.with(PathSegment::Key(key_str));
                                let child_id = self.build_node(value, Some(id), child_path);
                                children.push(child_id);
                                max_child_height =
                                    max_child_height.max(self.nodes[child_id.0].height);
                            }
                        }
                    }
                    Def::Option(_) => {
                        if let Ok(opt) = peek.into_option()
                            && let Some(inner) = opt.value()
                        {
                            let child_id = self.build_node(inner, Some(id), path.clone());
                            children.push(child_id);
                            max_child_height = max_child_height.max(self.nodes[child_id.0].height);
                        }
                    }
                    _ => {
                        // Scalar or other leaf node - no children
                    }
                }
            }
        }

        // Compute hash using structural_hash from Peek
        let mut hasher = DefaultHasher::new();
        peek.structural_hash(&mut hasher);
        let hash = hasher.finish();

        // Update node with computed values
        let node = &mut self.nodes[id.0];
        node.children = children;
        node.hash = hash;
        node.height = if node.children.is_empty() {
            0
        } else {
            max_child_height + 1
        };

        id
    }
}

/// A mapping between nodes in two trees
#[derive(Debug, Default)]
pub struct Matching {
    /// Map from tree A node to tree B node
    a_to_b: HashMap<NodeId, NodeId>,
    /// Map from tree B node to tree A node
    b_to_a: HashMap<NodeId, NodeId>,
}

impl Matching {
    /// Create a new empty matching
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a match between two nodes
    pub fn add(&mut self, a: NodeId, b: NodeId) {
        self.a_to_b.insert(a, b);
        self.b_to_a.insert(b, a);
    }

    /// Check if a node from tree A is matched
    pub fn contains_a(&self, a: NodeId) -> bool {
        self.a_to_b.contains_key(&a)
    }

    /// Check if a node from tree B is matched
    pub fn contains_b(&self, b: NodeId) -> bool {
        self.b_to_a.contains_key(&b)
    }

    /// Get the match for a node from tree A
    pub fn get_b(&self, a: NodeId) -> Option<NodeId> {
        self.a_to_b.get(&a).copied()
    }

    /// Get the match for a node from tree B
    pub fn get_a(&self, b: NodeId) -> Option<NodeId> {
        self.b_to_a.get(&b).copied()
    }

    /// Get all matched pairs
    pub fn pairs(&self) -> impl Iterator<Item = (NodeId, NodeId)> + '_ {
        self.a_to_b.iter().map(|(&a, &b)| (a, b))
    }
}

/// Phase 1 & 2: Top-down matching based on hash equality
pub fn top_down_match(tree_a: &Tree, tree_b: &Tree) -> Matching {
    let mut matching = Matching::new();

    // Build hash -> nodes index for tree B
    let mut b_by_hash: HashMap<u64, Vec<NodeId>> = HashMap::new();
    for node in tree_b.nodes() {
        b_by_hash.entry(node.hash).or_default().push(node.id);
    }

    // Priority queue: process higher nodes first (by height, descending)
    let mut queue: Vec<(NodeId, NodeId)> = vec![(tree_a.root, tree_b.root)];

    // Sort by height descending
    queue.sort_by(|a, b| {
        let ha = tree_a.get(a.0).height;
        let hb = tree_a.get(b.0).height;
        hb.cmp(&ha)
    });

    while let Some((a_id, b_id)) = queue.pop() {
        let a_node = tree_a.get(a_id);
        let b_node = tree_b.get(b_id);

        // If already matched, skip
        if matching.contains_a(a_id) || matching.contains_b(b_id) {
            continue;
        }

        // If hashes match, these subtrees are identical
        if a_node.hash == b_node.hash {
            // Match this node and all descendants
            match_subtrees(tree_a, tree_b, a_id, b_id, &mut matching);
        } else {
            // Hashes differ - try to match children
            for &a_child in &a_node.children {
                let a_child_node = tree_a.get(a_child);

                // Find candidates in B with matching hash
                if let Some(b_candidates) = b_by_hash.get(&a_child_node.hash) {
                    for &b_candidate in b_candidates {
                        if !matching.contains_b(b_candidate) {
                            queue.push((a_child, b_candidate));
                        }
                    }
                }

                // Also try matching with children of b_id that have same kind
                for &b_child in &b_node.children {
                    if !matching.contains_b(b_child) {
                        let b_child_node = tree_b.get(b_child);
                        if a_child_node.kind == b_child_node.kind {
                            queue.push((a_child, b_child));
                        }
                    }
                }
            }
        }
    }

    matching
}

/// Match two subtrees recursively (when hashes match)
fn match_subtrees(
    tree_a: &Tree,
    tree_b: &Tree,
    a_id: NodeId,
    b_id: NodeId,
    matching: &mut Matching,
) {
    matching.add(a_id, b_id);

    let a_node = tree_a.get(a_id);
    let b_node = tree_b.get(b_id);

    // Match children in order (they should be identical if hashes match)
    for (a_child, b_child) in a_node.children.iter().zip(b_node.children.iter()) {
        match_subtrees(tree_a, tree_b, *a_child, *b_child, matching);
    }
}

/// Phase 3: Bottom-up matching for unmatched nodes
pub fn bottom_up_match(tree_a: &Tree, tree_b: &Tree, matching: &mut Matching) {
    const SIMILARITY_THRESHOLD: f64 = 0.5;

    // Build kind -> nodes index for tree B (unmatched only)
    let mut b_by_kind: HashMap<NodeKind, Vec<NodeId>> = HashMap::new();
    for node in tree_b.nodes() {
        if !matching.contains_b(node.id) {
            b_by_kind
                .entry(node.kind.clone())
                .or_default()
                .push(node.id);
        }
    }

    // Process tree A in post-order (children before parents)
    for a_node in tree_a.post_order() {
        if matching.contains_a(a_node.id) {
            continue;
        }

        // Find candidates with same kind
        let candidates = b_by_kind.get(&a_node.kind).cloned().unwrap_or_default();

        // Score candidates by dice coefficient
        let mut best: Option<(NodeId, f64)> = None;
        for b_id in candidates {
            if matching.contains_b(b_id) {
                continue;
            }

            let score = dice_coefficient(tree_a, tree_b, a_node.id, b_id, matching);
            if score >= SIMILARITY_THRESHOLD && (best.is_none() || score > best.unwrap().1) {
                best = Some((b_id, score));
            }
        }

        if let Some((b_id, _)) = best {
            matching.add(a_node.id, b_id);
        }
    }
}

/// Compute dice coefficient between two nodes based on matched descendants
fn dice_coefficient(
    tree_a: &Tree,
    tree_b: &Tree,
    a_id: NodeId,
    b_id: NodeId,
    matching: &Matching,
) -> f64 {
    let desc_a = tree_a.descendants(a_id);
    let desc_b = tree_b.descendants(b_id);

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

/// An edit operation in the diff
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditOp {
    /// A value was updated (matched but content differs)
    Update {
        /// The path to the updated node
        path: Path,
        /// Hash of the old value
        old_hash: u64,
        /// Hash of the new value
        new_hash: u64,
    },
    /// A node was inserted in tree B
    Insert {
        /// The path where the node was inserted
        path: Path,
        /// Hash of the inserted value
        hash: u64,
    },
    /// A node was deleted from tree A
    Delete {
        /// The path where the node was deleted
        path: Path,
        /// Hash of the deleted value
        hash: u64,
    },
    /// A node was moved from one location to another
    Move {
        /// The original path
        old_path: Path,
        /// The new path
        new_path: Path,
        /// Hash of the moved value
        hash: u64,
    },
}

/// Phase 4: Generate edit script from matching
pub fn generate_edit_script(tree_a: &Tree, tree_b: &Tree, matching: &Matching) -> Vec<EditOp> {
    let mut ops = Vec::new();

    // Deletions: nodes in A that are not matched
    for a_node in tree_a.nodes() {
        if !matching.contains_a(a_node.id) {
            ops.push(EditOp::Delete {
                path: a_node.path.clone(),
                hash: a_node.hash,
            });
        }
    }

    // Insertions: nodes in B that are not matched
    for b_node in tree_b.nodes() {
        if !matching.contains_b(b_node.id) {
            ops.push(EditOp::Insert {
                path: b_node.path.clone(),
                hash: b_node.hash,
            });
        }
    }

    // Updates and Moves: matched nodes where something changed
    for (a_id, b_id) in matching.pairs() {
        let a_node = tree_a.get(a_id);
        let b_node = tree_b.get(b_id);

        // Check if path changed (move)
        if a_node.path != b_node.path {
            ops.push(EditOp::Move {
                old_path: a_node.path.clone(),
                new_path: b_node.path.clone(),
                hash: b_node.hash,
            });
        }
        // Check if hash changed (update) - note: if subtrees matched by hash, they're identical
        // But if matched by similarity, content may differ
        else if a_node.hash != b_node.hash {
            ops.push(EditOp::Update {
                path: a_node.path.clone(),
                old_hash: a_node.hash,
                new_hash: b_node.hash,
            });
        }
    }

    ops
}

/// Compute the tree diff between two values
pub fn tree_diff<'a, 'f, A: facet_core::Facet<'f>, B: facet_core::Facet<'f>>(
    a: &'a A,
    b: &'a B,
) -> Vec<EditOp> {
    let peek_a = Peek::new(a);
    let peek_b = Peek::new(b);

    let tree_a = TreeBuilder::build(peek_a);
    let tree_b = TreeBuilder::build(peek_b);

    let mut matching = top_down_match(&tree_a, &tree_b);
    bottom_up_match(&tree_a, &tree_b, &mut matching);

    generate_edit_script(&tree_a, &tree_b, &matching)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Debug, Clone, PartialEq, Facet)]
    struct Person {
        name: String,
        age: u32,
    }

    #[test]
    fn test_identical_trees() {
        let a = Person {
            name: "Alice".into(),
            age: 30,
        };
        let b = a.clone();

        let ops = tree_diff(&a, &b);
        assert!(ops.is_empty(), "Identical trees should have no edits");
    }

    #[test]
    fn test_simple_update() {
        let a = Person {
            name: "Alice".into(),
            age: 30,
        };
        let b = Person {
            name: "Alice".into(),
            age: 31,
        };

        let ops = tree_diff(&a, &b);
        // Should have some update operations
        assert!(!ops.is_empty(), "Changed values should have edits");
    }

    #[test]
    fn test_tree_building() {
        let person = Person {
            name: "Alice".into(),
            age: 30,
        };

        let peek = Peek::new(&person);
        let tree = TreeBuilder::build(peek);

        // Should have root + 2 fields
        assert!(tree.len() >= 3, "Tree should have root and field nodes");
    }
}
