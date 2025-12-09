//! Tree diffing for Facet types using the cinereus algorithm.
//!
//! This module provides the bridge between facet-reflect's `Peek` and
//! cinereus's tree diffing algorithm.

use core::hash::Hasher;
use std::borrow::Cow;
use std::hash::DefaultHasher;

use cinereus::{EditOp as CinereusEditOp, MatchingConfig, NodeData, Tree, diff_trees};
use facet_core::{Def, StructKind, Type, UserType};
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

/// A path segment describing how to reach a child.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// A named field in a struct
    Field(Cow<'static, str>),
    /// An index in a list/array
    Index(usize),
    /// A key in a map
    Key(Cow<'static, str>),
    /// An enum variant
    Variant(Cow<'static, str>),
}

/// A path from root to a node.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct Path(pub Vec<PathSegment>);

impl Path {
    /// Create a new empty path.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Append a segment to this path.
    pub fn push(&mut self, segment: PathSegment) {
        self.0.push(segment);
    }

    /// Create a new path with an additional segment.
    pub fn with(&self, segment: PathSegment) -> Self {
        let mut new = self.clone();
        new.push(segment);
        new
    }
}

impl core::fmt::Display for Path {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
