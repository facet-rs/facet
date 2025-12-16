//! Tree diffing for Facet types using the cinereus algorithm.
//!
//! This module provides the bridge between facet-reflect's `Peek` and
//! cinereus's tree diffing algorithm.

use core::hash::Hasher;
use std::borrow::Cow;
use std::hash::DefaultHasher;

use cinereus::{EditOp as CinereusEditOp, MatchingConfig, NodeData, Tree, diff_trees};
use facet_core::{Def, StructKind, Type, UserType};
use facet_diff_core::{Path, PathSegment};
use facet_reflect::{HasFields, Peek};

/// The kind of a node in the tree (for type-based matching).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A struct with the given type name
    Struct(&'static str),
    /// An enum variant
    EnumVariant(&'static str, &'static str), // (enum_name, variant_name)
    /// A list/array/slice
    List(&'static str),
    /// A map
    Map(&'static str),
    /// An option
    Option(&'static str),
    /// A scalar value
    Scalar(&'static str),
}

/// Label for a node (the actual value for leaves).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLabel {
    /// The path to this node from the root.
    pub path: Path,
}

/// An edit operation in the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditOp {
    /// A value was updated (matched but content differs).
    Update {
        /// The path to the updated node
        path: Path,
        /// Hash of the old value
        old_hash: u64,
        /// Hash of the new value
        new_hash: u64,
    },
    /// A node was inserted in tree B.
    Insert {
        /// The path where the node was inserted
        path: Path,
        /// Hash of the inserted value
        hash: u64,
    },
    /// A node was deleted from tree A.
    Delete {
        /// The path where the node was deleted
        path: Path,
        /// Hash of the deleted value
        hash: u64,
    },
    /// A node was moved from one location to another.
    Move {
        /// The original path
        old_path: Path,
        /// The new path
        new_path: Path,
        /// Hash of the moved value
        hash: u64,
    },
}

/// A tree built from a Peek value, ready for diffing.
pub type FacetTree = Tree<NodeKind, NodeLabel>;

/// Build a cinereus tree from a Peek value.
pub fn build_tree<'mem, 'facet>(peek: Peek<'mem, 'facet>) -> FacetTree {
    let mut builder = TreeBuilder::new();
    let root_id = builder.build_node(peek, Path::new());
    Tree {
        arena: builder.arena,
        root: root_id,
    }
}

struct TreeBuilder {
    arena: cinereus::indextree::Arena<NodeData<NodeKind, NodeLabel>>,
}

impl TreeBuilder {
    fn new() -> Self {
        Self {
            arena: cinereus::indextree::Arena::new(),
        }
    }

    fn build_node<'mem, 'facet>(
        &mut self,
        peek: Peek<'mem, 'facet>,
        path: Path,
    ) -> cinereus::indextree::NodeId {
        // Compute structural hash
        let mut hasher = DefaultHasher::new();
        peek.structural_hash(&mut hasher);
        let hash = hasher.finish();

        // Determine the node kind
        let kind = self.determine_kind(peek);

        // Create node data
        let data = NodeData {
            hash,
            kind,
            label: Some(NodeLabel { path: path.clone() }),
        };

        // Create the node
        let node_id = self.arena.new_node(data);

        // Build children based on type
        self.build_children(peek, node_id, path);

        node_id
    }

    fn determine_kind<'mem, 'facet>(&self, peek: Peek<'mem, 'facet>) -> NodeKind {
        match peek.shape().ty {
            Type::User(UserType::Struct(_)) => NodeKind::Struct(peek.shape().type_identifier),
            Type::User(UserType::Enum(_)) => {
                if let Ok(e) = peek.into_enum()
                    && let Ok(variant) = e.active_variant()
                {
                    return NodeKind::EnumVariant(peek.shape().type_identifier, variant.name);
                }
                NodeKind::Scalar(peek.shape().type_identifier)
            }
            _ => match peek.shape().def {
                Def::List(_) | Def::Array(_) | Def::Slice(_) => {
                    NodeKind::List(peek.shape().type_identifier)
                }
                Def::Map(_) => NodeKind::Map(peek.shape().type_identifier),
                Def::Option(_) => NodeKind::Option(peek.shape().type_identifier),
                _ => NodeKind::Scalar(peek.shape().type_identifier),
            },
        }
    }

    fn build_children<'mem, 'facet>(
        &mut self,
        peek: Peek<'mem, 'facet>,
        parent_id: cinereus::indextree::NodeId,
        path: Path,
    ) {
        match peek.shape().ty {
            Type::User(UserType::Struct(_)) => {
                if let Ok(s) = peek.into_struct() {
                    for (field, field_peek) in s.fields() {
                        // Skip metadata fields
                        if field.is_metadata() {
                            continue;
                        }
                        let child_path = path.with(PathSegment::Field(Cow::Borrowed(field.name)));
                        let child_id = self.build_node(field_peek, child_path);
                        parent_id.append(child_id, &mut self.arena);
                    }
                }
            }
            Type::User(UserType::Enum(_)) => {
                if let Ok(e) = peek.into_enum()
                    && let Ok(variant) = e.active_variant()
                {
                    let variant_path = path.with(PathSegment::Variant(Cow::Borrowed(variant.name)));
                    for (i, (field, field_peek)) in e.fields().enumerate() {
                        let child_path = if variant.data.kind == StructKind::Struct {
                            variant_path.with(PathSegment::Field(Cow::Borrowed(field.name)))
                        } else {
                            variant_path.with(PathSegment::Index(i))
                        };
                        let child_id = self.build_node(field_peek, child_path);
                        parent_id.append(child_id, &mut self.arena);
                    }
                }
            }
            _ => {
                match peek.shape().def {
                    Def::List(_) | Def::Array(_) | Def::Slice(_) => {
                        if let Ok(list) = peek.into_list_like() {
                            for (i, elem) in list.iter().enumerate() {
                                let child_path = path.with(PathSegment::Index(i));
                                let child_id = self.build_node(elem, child_path);
                                parent_id.append(child_id, &mut self.arena);
                            }
                        }
                    }
                    Def::Map(_) => {
                        if let Ok(map) = peek.into_map() {
                            for (key, value) in map.iter() {
                                let key_str = format!("{:?}", key);
                                let child_path = path.with(PathSegment::Key(Cow::Owned(key_str)));
                                let child_id = self.build_node(value, child_path);
                                parent_id.append(child_id, &mut self.arena);
                            }
                        }
                    }
                    Def::Option(_) => {
                        if let Ok(opt) = peek.into_option()
                            && let Some(inner) = opt.value()
                        {
                            // For options, the child keeps the same path
                            let child_id = self.build_node(inner, path);
                            parent_id.append(child_id, &mut self.arena);
                        }
                    }
                    _ => {
                        // Scalar/leaf node - no children
                    }
                }
            }
        }
    }
}

/// Compute the tree diff between two Facet values.
pub fn tree_diff<'a, 'f, A: facet_core::Facet<'f>, B: facet_core::Facet<'f>>(
    a: &'a A,
    b: &'a B,
) -> Vec<EditOp> {
    let peek_a = Peek::new(a);
    let peek_b = Peek::new(b);

    let tree_a = build_tree(peek_a);
    let tree_b = build_tree(peek_b);

    let config = MatchingConfig::default();
    let cinereus_ops = diff_trees(&tree_a, &tree_b, &config);

    // Convert cinereus ops to our EditOp format, filtering out no-op moves
    cinereus_ops
        .into_iter()
        .map(|op| convert_op(op, &tree_a, &tree_b))
        .filter(|op| {
            // Filter out MOVE operations where old and new paths are the same
            // (these are no-ops from the user's perspective)
            if let EditOp::Move {
                old_path, new_path, ..
            } = op
            {
                old_path != new_path
            } else {
                true
            }
        })
        .collect()
}

fn convert_op(
    op: CinereusEditOp<NodeKind, NodeLabel>,
    tree_a: &FacetTree,
    tree_b: &FacetTree,
) -> EditOp {
    match op {
        CinereusEditOp::Update {
            node_a,
            node_b,
            old_label,
            new_label: _,
        } => {
            let path = old_label.map(|l| l.path).unwrap_or_else(Path::new);
            EditOp::Update {
                path,
                old_hash: tree_a.get(node_a).hash,
                new_hash: tree_b.get(node_b).hash,
            }
        }
        CinereusEditOp::Insert { node_b, label, .. } => {
            let path = label.map(|l| l.path).unwrap_or_else(Path::new);
            EditOp::Insert {
                path,
                hash: tree_b.get(node_b).hash,
            }
        }
        CinereusEditOp::Delete { node_a } => {
            let data = tree_a.get(node_a);
            let path = data
                .label
                .as_ref()
                .map(|l| l.path.clone())
                .unwrap_or_default();
            EditOp::Delete {
                path,
                hash: data.hash,
            }
        }
        CinereusEditOp::Move { node_a, node_b, .. } => {
            let old_path = tree_a
                .get(node_a)
                .label
                .as_ref()
                .map(|l| l.path.clone())
                .unwrap_or_default();
            let new_path = tree_b
                .get(node_b)
                .label
                .as_ref()
                .map(|l| l.path.clone())
                .unwrap_or_default();
            EditOp::Move {
                old_path,
                new_path,
                hash: tree_b.get(node_b).hash,
            }
        }
    }
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
        assert!(!ops.is_empty(), "Changed values should have edits");
    }

    #[test]
    fn test_tree_building() {
        let person = Person {
            name: "Alice".into(),
            age: 30,
        };

        let peek = Peek::new(&person);
        let tree = build_tree(peek);

        // Should have root + 2 fields (at minimum)
        let node_count = tree.arena.count();
        assert!(
            node_count >= 3,
            "Tree should have root and field nodes, got {}",
            node_count
        );
    }
}

/// Result of computing similarity between two Peek values using tree diff.
#[derive(Debug, Clone)]
pub struct SimilarityResult<'mem, 'facet> {
    /// Similarity score between 0.0 and 1.0
    pub score: f64,
    /// The edit operations if similarity is above threshold
    pub edit_ops: Vec<EditOp>,
    /// The first Peek value (from)
    pub peek_a: Peek<'mem, 'facet>,
    /// The second Peek value (to)
    pub peek_b: Peek<'mem, 'facet>,
}

impl<'mem, 'facet> SimilarityResult<'mem, 'facet> {
    /// Check if the elements are similar enough to be considered a match
    pub fn is_similar(&self, threshold: f64) -> bool {
        self.score >= threshold
    }

    /// Check if the elements are identical (score = 1.0)
    pub fn is_identical(&self) -> bool {
        self.score >= 1.0 - f64::EPSILON
    }
}

/// Compute structural similarity between two Peek values using tree diff.
///
/// This uses the cinereus GumTree algorithm to:
/// 1. Build trees from both Peek values
/// 2. Compute a matching between nodes (hash-based + Dice coefficient)
/// 3. Return a similarity score based on how many nodes matched
///
/// The similarity score is: `matched_nodes / max(nodes_a, nodes_b)`
///
/// # Arguments
/// * `peek_a` - First value to compare
/// * `peek_b` - Second value to compare
/// * `config` - Optional matching configuration (uses defaults if None)
///
/// # Returns
/// A `SimilarityResult` containing the score and edit operations
pub fn compute_element_similarity<'mem, 'facet>(
    peek_a: Peek<'mem, 'facet>,
    peek_b: Peek<'mem, 'facet>,
    config: Option<&MatchingConfig>,
) -> SimilarityResult<'mem, 'facet> {
    let tree_a = build_tree(peek_a);
    let tree_b = build_tree(peek_b);

    let default_config = MatchingConfig::default();
    let config = config.unwrap_or(&default_config);

    let matching = cinereus::compute_matching(&tree_a, &tree_b, config);

    // Count nodes in each tree
    let nodes_a = tree_a.arena.count();
    let nodes_b = tree_b.arena.count();
    let max_nodes = nodes_a.max(nodes_b);

    // Similarity score: proportion of nodes that matched
    let score = if max_nodes == 0 {
        1.0 // Both empty = identical
    } else {
        matching.len() as f64 / max_nodes as f64
    };

    // Generate edit operations
    let cinereus_ops = diff_trees(&tree_a, &tree_b, config);
    let edit_ops = cinereus_ops
        .into_iter()
        .map(|op| convert_op(op, &tree_a, &tree_b))
        .filter(|op| {
            // Filter out no-op moves
            if let EditOp::Move {
                old_path, new_path, ..
            } = op
            {
                old_path != new_path
            } else {
                true
            }
        })
        .collect();

    SimilarityResult {
        score,
        edit_ops,
        peek_a,
        peek_b,
    }
}

/// Check if two sequence elements should be paired based on structural similarity.
///
/// This is a convenience function for sequence diffing that returns true
/// if the elements are similar enough to be shown as a modification rather
/// than a removal+addition.
///
/// # Arguments
/// * `peek_a` - First element
/// * `peek_b` - Second element
/// * `threshold` - Minimum similarity score (0.0 to 1.0), recommended 0.5-0.7
pub fn elements_are_similar<'mem, 'facet>(
    peek_a: Peek<'mem, 'facet>,
    peek_b: Peek<'mem, 'facet>,
    threshold: f64,
) -> bool {
    let result = compute_element_similarity(peek_a, peek_b, None);
    result.is_similar(threshold)
}
