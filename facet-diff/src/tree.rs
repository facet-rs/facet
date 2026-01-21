//! Tree diffing for Facet types using the cinereus algorithm.
//!
//! This module provides the bridge between facet-reflect's `Peek` and
//! cinereus's tree diffing algorithm.

#[cfg(feature = "matching-stats")]
pub use cinereus::matching::{get_stats as get_matching_stats, reset_stats as reset_matching_stats};

#[cfg(feature = "tracing")]
use tracing::debug;

#[cfg(not(feature = "tracing"))]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

use core::hash::{Hash, Hasher};
use std::borrow::Cow;
use std::hash::DefaultHasher;

use cinereus::{
    EditOp as CinereusEditOp, Matching, MatchingConfig, NodeData, Tree, diff_trees_with_matching,
    indextree::{self, NodeId},
    tree::{Properties, PropertyChange},
};
use std::collections::HashMap;
use facet_core::{Def, StructKind, Type, UserType};
use facet_diff_core::{Path, PathSegment};
use facet_reflect::{HasFields, Peek};

/// The kind of a node in the tree (for type-based matching).
#[derive(Debug, Clone, PartialEq, Eq, Hash, facet::Facet)]
#[repr(u8)]
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
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct NodeLabel {
    /// The path to this node from the root.
    pub path: Path,
}

/// An edit operation in the diff.
///
/// Each operation is self-contained with all information needed to apply it.
/// Consumers do not have access to the original trees.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditOp {
    /// A value was updated (matched but content differs).
    Update {
        /// The path to the updated node
        path: Path,
        /// The old value (for display/verification), None for containers
        old_value: Option<String>,
        /// The new value to set, None for containers
        new_value: Option<String>,
        /// Hash of the old value
        old_hash: u64,
        /// Hash of the new value
        new_hash: u64,
    },
    /// An attribute (property) was updated on a matched node.
    UpdateAttribute {
        /// The path to the node containing the attribute
        path: Path,
        /// The attribute name (field name)
        attr_name: &'static str,
        /// The old value (None if attribute was absent)
        old_value: Option<String>,
        /// The new value (None if attribute is being removed)
        new_value: Option<String>,
    },
    /// A node was inserted in tree B.
    Insert {
        /// The path where the node was inserted (shadow tree coordinates for DOM operations)
        path: Path,
        /// The path in tree_b coordinates (for navigating new_doc to get content)
        label_path: Path,
        /// The value to insert (for leaf nodes), None for containers
        value: Option<String>,
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

/// Properties for HTML/XML nodes: attribute key-value pairs.
///
/// These are fields marked with `#[facet(html::attribute)]` or `#[facet(xml::attribute)]`.
/// They are diffed field-by-field when nodes match, avoiding the cross-matching problem
/// where identical Option values (like None) get matched across different fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HtmlProperties {
    /// Attribute values keyed by field name.
    /// Values are stored as Option<String> to handle both present and absent attributes.
    pub attrs: HashMap<&'static str, Option<String>>,
}

impl HtmlProperties {
    /// Create empty properties.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an attribute value.
    pub fn set(&mut self, key: &'static str, value: Option<String>) {
        self.attrs.insert(key, value);
    }
}

impl Properties for HtmlProperties {
    type Key = &'static str;
    type Value = Option<String>;

    fn similarity(&self, other: &Self) -> f64 {
        // Count matching attributes
        let all_keys: std::collections::HashSet<_> = self
            .attrs
            .keys()
            .chain(other.attrs.keys())
            .collect();

        if all_keys.is_empty() {
            return 1.0; // Both empty = perfect match
        }

        let mut matches = 0;
        for key in &all_keys {
            if self.attrs.get(*key) == other.attrs.get(*key) {
                matches += 1;
            }
        }

        matches as f64 / all_keys.len() as f64
    }

    fn diff(&self, other: &Self) -> Vec<PropertyChange<Self::Key, Self::Value>> {
        let mut changes = Vec::new();

        // Check all keys in self
        for (key, old_value) in &self.attrs {
            let new_value = other.attrs.get(key);
            if new_value != Some(old_value) {
                changes.push(PropertyChange {
                    key: *key,
                    old_value: Some(old_value.clone()),
                    new_value: new_value.cloned(),
                });
            }
        }

        // Check keys only in other (additions)
        for (key, new_value) in &other.attrs {
            if !self.attrs.contains_key(key) {
                changes.push(PropertyChange {
                    key: *key,
                    old_value: None,
                    new_value: Some(new_value.clone()),
                });
            }
        }

        changes
    }

    fn is_empty(&self) -> bool {
        self.attrs.is_empty()
    }
}

/// A tree built from a Peek value, ready for diffing.
pub type FacetTree = Tree<NodeKind, NodeLabel, HtmlProperties>;

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
    arena: cinereus::indextree::Arena<NodeData<NodeKind, NodeLabel, HtmlProperties>>,
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

        // Collect properties (attribute fields) for struct types
        let properties = self.collect_properties(peek);

        // Create node data with properties
        let data = NodeData {
            hash,
            kind,
            label: Some(NodeLabel { path: path.clone() }),
            properties,
        };

        // Create the node
        let node_id = self.arena.new_node(data);

        // Build children based on type (excluding attribute fields)
        self.build_children(peek, node_id, path);

        node_id
    }

    /// Collect attribute fields as properties.
    fn collect_properties<'mem, 'facet>(&self, peek: Peek<'mem, 'facet>) -> HtmlProperties {
        let mut props = HtmlProperties::new();

        // Only structs have attribute fields
        if let Type::User(UserType::Struct(_)) = peek.shape().ty {
            if let Ok(s) = peek.into_struct() {
                for (field, field_peek) in s.fields() {
                    // Check if this field is an attribute
                    if field.is_attribute() {
                        // Extract the value as a string
                        let value = self.extract_attribute_value(field_peek);
                        props.set(field.name, value);
                    }
                }
            }
        }

        props
    }

    /// Extract an attribute value as Option<String>.
    fn extract_attribute_value<'mem, 'facet>(&self, peek: Peek<'mem, 'facet>) -> Option<String> {
        // Handle Option<T> by unwrapping
        if let Ok(opt) = peek.into_option() {
            if let Some(inner) = opt.value() {
                return inner.as_str().map(|s| s.to_string());
            } else {
                return None;
            }
        }

        // Direct string value
        peek.as_str().map(|s| s.to_string())
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
                        // Skip attribute fields - they're stored as properties, not children
                        if field.is_attribute() {
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
    let (cinereus_ops, matching) = diff_trees_with_matching(&tree_a, &tree_b, &config);

    debug!(cinereus_ops_count = cinereus_ops.len(), "cinereus ops before conversion");
    for (i, op) in cinereus_ops.iter().enumerate() {
        debug!(i, ?op, "cinereus op");
    }

    // Convert cinereus ops to path-based EditOps using a shadow tree
    // to track index shifts as operations are applied.
    // We pass the original values so we can extract actual values for leaf nodes.
    let peek_a = Peek::new(a);
    let peek_b = Peek::new(b);
    let result = convert_ops_with_shadow(cinereus_ops, &tree_a, &tree_b, &matching, peek_a, peek_b);

    debug!(result_count = result.len(), "edit ops after conversion");
    result
}

/// Extract a scalar value from a Peek by navigating a path.
///
/// Returns Some(string representation) for leaf/scalar values, None for containers.
fn extract_value_at_path<'mem, 'facet>(
    mut peek: Peek<'mem, 'facet>,
    path: &Path,
) -> Option<String> {
    // Navigate to the node
    for (i, segment) in path.0.iter().enumerate() {
        debug!(i, ?segment, shape = ?peek.shape().type_identifier, "extract_value_at_path navigating");
        peek = match segment {
            PathSegment::Field(name) => {
                if let Ok(s) = peek.into_struct() {
                    s.field_by_name(name).ok()?
                } else if let Ok(opt) = peek.into_option() {
                    let inner = opt.value()?;
                    if let Ok(s) = inner.into_struct() {
                        s.field_by_name(name).ok()?
                    } else {
                        debug!("extract_value_at_path: option inner not a struct");
                        return None;
                    }
                } else {
                    debug!("extract_value_at_path: not a struct or option for Field");
                    return None;
                }
            }
            PathSegment::Index(idx) => {
                if let Ok(list) = peek.into_list() {
                    list.get(*idx)?
                } else if let Ok(opt) = peek.into_option() {
                    if *idx == 0 {
                        opt.value()?
                    } else {
                        debug!("extract_value_at_path: option index != 0");
                        return None;
                    }
                } else if let Ok(e) = peek.into_enum() {
                    e.field(*idx).ok()??
                } else {
                    debug!("extract_value_at_path: not a list, option, or enum for Index");
                    return None;
                }
            }
            PathSegment::Variant(_) => {
                // Variant just means we're already at that variant, continue
                peek
            }
            PathSegment::Key(key) => {
                if let Ok(map) = peek.into_map() {
                    for (k, v) in map.iter() {
                        if let Some(s) = k.as_str() {
                            if s == key {
                                peek = v;
                                break;
                            }
                        }
                    }
                    return None;
                } else {
                    debug!("extract_value_at_path: not a map for Key");
                    return None;
                }
            }
        };
    }

    // For now, we only extract string values.
    // This covers text content and attribute values in HTML.
    // Other scalar types can be added later if needed.

    // If we ended up at an Option<String>, unwrap it first
    if let Ok(opt) = peek.into_option() {
        if let Some(inner) = opt.value() {
            return inner.as_str().map(|s| s.to_string());
        } else {
            return None;
        }
    }

    peek.as_str().map(|s| s.to_string())
}

/// Convert cinereus ops to path-based EditOps.
///
/// This uses a "shadow tree" approach: we maintain a mutable copy of tree_a
/// and simulate applying each operation to it. This lets us compute correct
/// paths that account for index shifts from earlier operations.
///
/// The peeks are used to extract actual values for leaf nodes, making
/// the resulting EditOps self-contained.
fn convert_ops_with_shadow<'mem, 'facet>(
    ops: Vec<CinereusEditOp<NodeKind, NodeLabel, HtmlProperties>>,
    tree_a: &FacetTree,
    tree_b: &FacetTree,
    matching: &Matching,
    peek_a: Peek<'mem, 'facet>,
    peek_b: Peek<'mem, 'facet>,
) -> Vec<EditOp> {
    // Shadow tree: mutable clone of tree_a's structure
    let mut shadow_arena = tree_a.arena.clone();
    let shadow_root = tree_a.root;

    // Map from tree_b NodeIds to shadow tree NodeIds
    // Initially populated from matching (matched nodes)
    let mut b_to_shadow: HashMap<NodeId, NodeId> = HashMap::new();
    for (a_id, b_id) in matching.pairs() {
        b_to_shadow.insert(b_id, a_id);
    }

    let mut result = Vec::new();

    for op in ops {
        match op {
            CinereusEditOp::Update {
                node_a,
                node_b,
                old_label: _,
                new_label: _,
            } => {
                // Path for applying the patch comes from shadow tree (current position in DOM)
                let mut path = compute_path_in_shadow(&shadow_arena, shadow_root, node_a, tree_a);

                // Extract actual values using the stored paths in tree labels
                let old_path = tree_a
                    .get(node_a)
                    .label
                    .as_ref()
                    .map(|l| &l.path);
                let new_path_in_b = tree_b
                    .get(node_b)
                    .label
                    .as_ref()
                    .map(|l| &l.path);

                let old_value = old_path.and_then(|p| extract_value_at_path(peek_a, p));
                let new_value = new_path_in_b.and_then(|p| extract_value_at_path(peek_b, p));
                debug!(?old_path, ?new_path_in_b, ?old_value, ?new_value, "Update op values");

                // For struct field changes (e.g., attribute fields like id->class),
                // replace the last segment with the new field name.
                // This preserves the position but updates the field name.
                if let (Some(old_p), Some(new_p)) = (old_path, new_path_in_b) {
                    if let (Some(PathSegment::Field(old_field)), Some(PathSegment::Field(new_field))) =
                        (old_p.0.last(), new_p.0.last())
                    {
                        if old_field != new_field && !path.0.is_empty() {
                            // Replace the last field segment with the new field name
                            if let Some(PathSegment::Field(_)) = path.0.last() {
                                path.0.pop();
                                path.0.push(PathSegment::Field(new_field.clone()));
                            }
                        }
                    }
                }

                debug!(?path, ?old_value, ?new_value, "emitting EditOp::Update");
                result.push(EditOp::Update {
                    path,
                    old_value,
                    new_value,
                    old_hash: tree_a.get(node_a).hash,
                    new_hash: tree_b.get(node_b).hash,
                });
                // No structural change for Update
            }

            CinereusEditOp::UpdateProperty {
                node_a,
                node_b: _,
                key,
                old_value,
                new_value,
            } => {
                // Path to the node containing the attribute
                let path = compute_path_in_shadow(&shadow_arena, shadow_root, node_a, tree_a);

                // Flatten Option<Option<String>> to Option<String>
                let old_value = old_value.flatten();
                let new_value = new_value.flatten();

                debug!(?path, ?key, ?old_value, ?new_value, "emitting EditOp::UpdateAttribute");
                result.push(EditOp::UpdateAttribute {
                    path,
                    attr_name: key,
                    old_value,
                    new_value,
                });
                // No structural change for UpdateAttribute
            }

            CinereusEditOp::Insert {
                node_b,
                parent_b,
                position,
                label,
                ..
            } => {
                // Find the parent in our shadow tree
                let shadow_parent = b_to_shadow.get(&parent_b).copied().unwrap_or(shadow_root);

                // Compute the path for the insertion point
                let parent_path =
                    compute_path_in_shadow(&shadow_arena, shadow_root, shadow_parent, tree_a);
                let mut path = parent_path;

                // Add the position segment from the label (which has the full path info)
                if let Some(ref lbl) = label {
                    // The label's path contains the full path including the final segment
                    // We want to use its structure but with our computed parent path
                    if let Some(last_segment) = lbl.path.0.last() {
                        path.0.push(last_segment.clone());
                    }
                }

                // Extract value for leaf nodes using the tree_b label path
                let label_path = tree_b
                    .get(node_b)
                    .label
                    .as_ref()
                    .map(|l| l.path.clone())
                    .unwrap_or_else(|| Path(vec![]));
                let value = extract_value_at_path(peek_b, &label_path);

                debug!(?node_b, ?path, ?label_path, ?position, "INSERT op: computed path");
                result.push(EditOp::Insert {
                    path: path.clone(),
                    label_path,
                    value,
                    hash: tree_b.get(node_b).hash,
                });

                // Create a new node in shadow tree and add to b_to_shadow
                let new_data: NodeData<NodeKind, NodeLabel, HtmlProperties> = NodeData {
                    hash: 0,
                    kind: NodeKind::Scalar("inserted"),
                    label,
                    properties: HtmlProperties::new(),
                };
                let new_node = shadow_arena.new_node(new_data);

                // Insert at the correct position among siblings
                let children: Vec<_> = shadow_parent.children(&shadow_arena).collect();
                if position == 0 {
                    if let Some(&first_child) = children.first() {
                        first_child.insert_before(new_node, &mut shadow_arena);
                    } else {
                        shadow_parent.append(new_node, &mut shadow_arena);
                    }
                } else if position >= children.len() {
                    shadow_parent.append(new_node, &mut shadow_arena);
                } else {
                    children[position].insert_before(new_node, &mut shadow_arena);
                }

                b_to_shadow.insert(node_b, new_node);
            }

            CinereusEditOp::Delete { node_a } => {
                // Path is current location before deletion
                let path = compute_path_in_shadow(&shadow_arena, shadow_root, node_a, tree_a);
                debug!(?node_a, ?path, "DELETE op: computed path");
                result.push(EditOp::Delete {
                    path,
                    hash: tree_a.get(node_a).hash,
                });

                // Remove from shadow tree
                let parent_before = shadow_arena.get(node_a).and_then(|n| n.parent());
                if let Some(p) = parent_before {
                    let children_before: Vec<_> = p.children(&shadow_arena).collect();
                    debug!(?node_a, ?p, ?children_before, "before remove");
                }
                node_a.detach(&mut shadow_arena);
                let parent_after = shadow_arena.get(node_a).and_then(|n| n.parent());
                if let Some(p) = parent_before {
                    let children_after: Vec<_> = p.children(&shadow_arena).collect();
                    debug!(?node_a, ?parent_before, ?parent_after, ?children_after, "after remove");
                }
            }

            CinereusEditOp::Move {
                node_a,
                node_b,
                new_parent_b,
                new_position,
            } => {
                // Old path comes from tree_a's label (where the node was)
                let old_path = compute_path_in_shadow(&shadow_arena, shadow_root, node_a, tree_a);

                // New path comes from tree_b's label (where the node is going)
                // This is important for struct field moves where the field name changes
                let new_path = tree_b
                    .get(node_b)
                    .label
                    .as_ref()
                    .map(|l| l.path.clone())
                    .unwrap_or_default();

                // Detach from current parent
                node_a.detach(&mut shadow_arena);

                // Find new parent in shadow tree
                let shadow_new_parent = b_to_shadow.get(&new_parent_b).copied().unwrap_or(shadow_root);

                // Insert at new position
                let children: Vec<_> = shadow_new_parent.children(&shadow_arena).collect();
                if new_position == 0 {
                    if let Some(&first_child) = children.first() {
                        first_child.insert_before(node_a, &mut shadow_arena);
                    } else {
                        shadow_new_parent.append(node_a, &mut shadow_arena);
                    }
                } else if new_position >= children.len() {
                    shadow_new_parent.append(node_a, &mut shadow_arena);
                } else {
                    children[new_position].insert_before(node_a, &mut shadow_arena);
                }

                // Emit if paths differ (e.g., struct field moves like id -> class)
                debug!(?old_path, ?new_path, "Move op paths");
                if old_path != new_path {
                    debug!("emitting Move EditOp");
                    result.push(EditOp::Move {
                        old_path,
                        new_path,
                        hash: tree_b.get(node_b).hash,
                    });
                } else {
                    debug!("skipping Move - paths are equal");
                }

                // Update b_to_shadow
                b_to_shadow.insert(node_b, node_a);
            }
        }
    }

    result
}

/// Compute the path from root to a node in the shadow tree.
///
/// For nodes that have a label (original tree_a nodes), we use the stored path.
/// For inserted nodes, we compute the path by walking up and determining
/// the position at each level.
fn compute_path_in_shadow(
    shadow_arena: &indextree::Arena<NodeData<NodeKind, NodeLabel, HtmlProperties>>,
    shadow_root: NodeId,
    node: NodeId,
    _tree_a: &FacetTree,
) -> Path {
    // If this node has a label, it came from tree_a and we can use its stored path
    // But we need to account for any index shifts from insertions/deletions
    // For now, let's compute the path by walking up and using labels where available

    if let Some(node_ref) = shadow_arena.get(node) {
        if let Some(label) = &node_ref.get().label {
            // Original node from tree_a - use its stored path
            // But we need to update any indices that may have shifted
            return compute_adjusted_path(shadow_arena, shadow_root, node, &label.path);
        }
    }

    // Inserted node - compute path by walking up
    compute_path_by_walking(shadow_arena, shadow_root, node)
}

/// Compute an adjusted path for a node that may have shifted due to insertions/deletions.
///
/// Key insight: tree depth and path depth may NOT be 1:1 because Option types add a tree
/// node but not a path segment. We must use each node's stored path length to track depth.
fn compute_adjusted_path(
    shadow_arena: &indextree::Arena<NodeData<NodeKind, NodeLabel, HtmlProperties>>,
    shadow_root: NodeId,
    node: NodeId,
    original_path: &Path,
) -> Path {
    // Build a map from path depth -> actual position, but only for Index segments
    // We use each node's stored path to determine its depth, not tree traversal count
    let mut depth_to_position: HashMap<usize, usize> = HashMap::new();
    let mut current = node;

    debug!(?node, ?original_path, "compute_adjusted_path start");

    while current != shadow_root {
        // Get the current node's path depth from its stored label
        let current_path_len = shadow_arena
            .get(current)
            .and_then(|n| n.get().label.as_ref())
            .map(|l| l.path.0.len())
            .unwrap_or(0);

        // If this depth has an Index segment in the original path, record actual position
        if current_path_len > 0 {
            let depth = current_path_len - 1;
            if let Some(PathSegment::Index(_)) = original_path.0.get(depth) {
                if let Some(parent_id) = shadow_arena.get(current).and_then(|n| n.parent()) {
                    let children: Vec<_> = parent_id.children(shadow_arena).collect();
                    let pos = children.iter().position(|&c| c == current).unwrap_or(0);
                    debug!(?current, ?parent_id, ?depth, ?pos, num_children = children.len(), "recording position");
                    depth_to_position.insert(depth, pos);
                }
            }
        }

        // Move to parent
        if let Some(parent_id) = shadow_arena.get(current).and_then(|n| n.parent()) {
            current = parent_id;
        } else {
            break;
        }
    }

    // Build result path with adjusted indices
    let mut result_segments = Vec::new();
    for (i, segment) in original_path.0.iter().enumerate() {
        match segment {
            PathSegment::Index(_) => {
                if let Some(&pos) = depth_to_position.get(&i) {
                    result_segments.push(PathSegment::Index(pos));
                } else {
                    result_segments.push(segment.clone());
                }
            }
            _ => {
                result_segments.push(segment.clone());
            }
        }
    }

    Path(result_segments)
}

/// Compute path for a node by walking up the tree.
fn compute_path_by_walking(
    shadow_arena: &indextree::Arena<NodeData<NodeKind, NodeLabel, HtmlProperties>>,
    shadow_root: NodeId,
    node: NodeId,
) -> Path {
    let mut segments = Vec::new();
    let mut current = node;

    while current != shadow_root {
        if let Some(parent_id) = shadow_arena.get(current).and_then(|n| n.parent()) {
            // For inserted nodes, just use index
            let pos = parent_id
                .children(shadow_arena)
                .position(|c| c == current)
                .unwrap_or(0);
            segments.push(PathSegment::Index(pos));
            current = parent_id;
        } else {
            break;
        }
    }

    segments.reverse();
    Path(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use facet_testhelpers::test;

    #[derive(Debug, Clone, PartialEq, Facet)]
    struct Person {
        name: String,
        age: u32,
    }

    #[derive(Debug, Clone, PartialEq, Facet)]
    struct Container {
        items: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq, Facet)]
    struct Nested {
        name: String,
        children: Vec<Container>,
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

    /// Test that tree building produces correct paths for list elements
    #[test]
    fn test_tree_paths_for_list() {
        let container = Container {
            items: vec!["a".into(), "b".into(), "c".into()],
        };

        let peek = Peek::new(&container);
        let tree = build_tree(peek);

        // Collect all paths from the tree
        let mut paths: Vec<Path> = Vec::new();
        for node in tree.arena.iter() {
            if let Some(label) = &node.get().label {
                paths.push(label.path.clone());
            }
        }

        debug!(?paths, "all paths in tree");

        // Should have:
        // - [] (root)
        // - [Field("items")] (the vec field)
        // - [Field("items"), Index(0)] (first element)
        // - [Field("items"), Index(1)] (second element)
        // - [Field("items"), Index(2)] (third element)
        assert!(
            paths.iter().any(|p| p.0.is_empty()),
            "Should have root path"
        );
        assert!(
            paths.iter().any(|p| p.0 == vec![PathSegment::Field("items".into())]),
            "Should have items field path"
        );
        assert!(
            paths.iter().any(|p| p.0
                == vec![
                    PathSegment::Field("items".into()),
                    PathSegment::Index(0)
                ]),
            "Should have items[0] path"
        );
        assert!(
            paths.iter().any(|p| p.0
                == vec![
                    PathSegment::Field("items".into()),
                    PathSegment::Index(2)
                ]),
            "Should have items[2] path"
        );
    }

    /// Test that compute_adjusted_path correctly updates Index segments
    #[test]
    fn test_compute_adjusted_path_basic() {
        let container = Container {
            items: vec!["a".into(), "b".into(), "c".into()],
        };

        let peek = Peek::new(&container);
        let tree = build_tree(peek);

        // Find the node for items[1]
        let target_path = Path(vec![
            PathSegment::Field("items".into()),
            PathSegment::Index(1),
        ]);

        let mut target_node = None;
        for node_id in tree.root.descendants(&tree.arena) {
            if let Some(label) = &tree.arena.get(node_id).unwrap().get().label {
                if label.path == target_path {
                    target_node = Some(node_id);
                    break;
                }
            }
        }
        let target_node = target_node.expect("Should find items[1] node");

        // Without any modifications, the adjusted path should equal the original
        let adjusted = compute_adjusted_path(&tree.arena, tree.root, target_node, &target_path);
        assert_eq!(
            adjusted, target_path,
            "Unmodified tree should have unchanged paths"
        );
    }

    /// Test that after deleting an element, subsequent element paths shift
    #[test]
    fn test_path_adjustment_after_delete() {
        let container = Container {
            items: vec!["a".into(), "b".into(), "c".into()],
        };

        let peek = Peek::new(&container);
        let tree = build_tree(peek);

        // Clone the arena to simulate shadow tree
        let mut shadow_arena = tree.arena.clone();

        // Find the nodes
        let items_path = Path(vec![PathSegment::Field("items".into())]);
        let item0_path = Path(vec![
            PathSegment::Field("items".into()),
            PathSegment::Index(0),
        ]);
        let item1_path = Path(vec![
            PathSegment::Field("items".into()),
            PathSegment::Index(1),
        ]);
        let item2_path = Path(vec![
            PathSegment::Field("items".into()),
            PathSegment::Index(2),
        ]);

        let mut item0_node = None;
        let mut item1_node = None;
        let mut item2_node = None;

        for node_id in tree.root.descendants(&tree.arena) {
            if let Some(label) = &tree.arena.get(node_id).unwrap().get().label {
                if label.path == item0_path {
                    item0_node = Some(node_id);
                } else if label.path == item1_path {
                    item1_node = Some(node_id);
                } else if label.path == item2_path {
                    item2_node = Some(node_id);
                }
            }
        }

        let item0_node = item0_node.expect("Should find items[0]");
        let item1_node = item1_node.expect("Should find items[1]");
        let item2_node = item2_node.expect("Should find items[2]");

        // Delete item0 from shadow tree
        item0_node.remove(&mut shadow_arena);

        // Now item1 (originally at index 1) should be at index 0
        let adjusted1 =
            compute_adjusted_path(&shadow_arena, tree.root, item1_node, &item1_path);
        let expected1 = Path(vec![
            PathSegment::Field("items".into()),
            PathSegment::Index(0),
        ]);
        assert_eq!(
            adjusted1, expected1,
            "After deleting item[0], item[1] should become item[0]"
        );

        // And item2 (originally at index 2) should be at index 1
        let adjusted2 =
            compute_adjusted_path(&shadow_arena, tree.root, item2_node, &item2_path);
        let expected2 = Path(vec![
            PathSegment::Field("items".into()),
            PathSegment::Index(1),
        ]);
        assert_eq!(
            adjusted2, expected2,
            "After deleting item[0], item[2] should become item[1]"
        );
    }

    /// Test list element deletion produces some diff operations
    #[test]
    fn test_diff_list_delete() {
        let a = Container {
            items: vec!["a".into(), "b".into(), "c".into()],
        };
        let b = Container {
            items: vec!["a".into(), "c".into()], // removed "b"
        };

        let ops = tree_diff(&a, &b);
        debug!(?ops, "diff ops for list delete");

        // Should have some operations (the exact ops depend on cinereus's matching algorithm)
        // The algorithm may emit Delete, or Move+Delete, etc.
        assert!(!ops.is_empty(), "Should have some operations for deletion");

        // Should have at least one Delete or Move operation
        let has_structural_change = ops.iter().any(|op| {
            matches!(op, EditOp::Delete { .. } | EditOp::Move { .. })
        });
        assert!(
            has_structural_change,
            "Should have Delete or Move for structural change, got: {:?}",
            ops
        );
    }

    /// Test list element insertion produces correct Insert path
    #[test]
    fn test_diff_list_insert() {
        let a = Container {
            items: vec!["a".into(), "c".into()],
        };
        let b = Container {
            items: vec!["a".into(), "b".into(), "c".into()], // inserted "b" at index 1
        };

        let ops = tree_diff(&a, &b);
        debug!(?ops, "diff ops for list insert");

        // Should have an Insert for items[1]
        let has_insert_at_1 = ops.iter().any(|op| {
            if let EditOp::Insert { path, .. } = op {
                path.0
                    == vec![
                        PathSegment::Field("items".into()),
                        PathSegment::Index(1),
                    ]
            } else {
                false
            }
        });
        assert!(has_insert_at_1, "Should have Insert at items[1], got: {:?}", ops);
    }

    /// Test that nested structures produce diff operations
    #[test]
    fn test_nested_list_paths() {
        let a = Nested {
            name: "root".into(),
            children: vec![
                Container { items: vec!["a".into()] },
                Container { items: vec!["b".into()] },
            ],
        };
        let b = Nested {
            name: "root".into(),
            children: vec![
                Container { items: vec!["a".into()] },
                Container { items: vec!["modified".into()] }, // changed
            ],
        };

        let ops = tree_diff(&a, &b);
        debug!(?ops, "diff ops for nested change");

        // Should have some operations for the change
        assert!(!ops.is_empty(), "Should have operations for nested change");

        // At minimum, there should be something touching children
        let has_children_op = ops.iter().any(|op| {
            let path = match op {
                EditOp::Update { path, .. } => path,
                EditOp::Insert { path, .. } => path,
                EditOp::Delete { path, .. } => path,
                EditOp::Move { old_path, .. } => old_path,
                EditOp::UpdateAttribute { path, .. } => path,
            };
            path.0.first() == Some(&PathSegment::Field("children".into()))
        });
        assert!(
            has_children_op,
            "Should have operation touching children, got: {:?}",
            ops
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

    let (cinereus_ops, matching) = diff_trees_with_matching(&tree_a, &tree_b, config);

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

    // Generate edit operations using shadow tree
    let edit_ops = convert_ops_with_shadow(cinereus_ops, &tree_a, &tree_b, &matching, peek_a, peek_b);

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
