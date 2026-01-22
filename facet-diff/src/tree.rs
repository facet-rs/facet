//! Tree diffing for Facet types using the cinereus algorithm.
//!
//! This module provides the bridge between facet-reflect's `Peek` and
//! cinereus's tree diffing algorithm.

#[cfg(feature = "matching-stats")]
pub use cinereus::matching::{
    get_stats as get_matching_stats, reset_stats as reset_matching_stats,
};

use crate::{debug, trace, trace_verbose};

use core::hash::{Hash, Hasher};
use std::borrow::Cow;
use std::hash::DefaultHasher;

use cinereus::{
    EditOp as CinereusEditOp, Matching, MatchingConfig, NodeData, Tree, diff_trees_with_matching,
    indextree::{self, NodeId},
    tree::{Properties, PropertyChange},
};
use facet_core::{Def, StructKind, Type, UserType};
use facet_diff_core::{Path, PathSegment};
use facet_reflect::{HasFields, Peek};
use std::collections::HashMap;

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
    /// Multiple attributes (properties) were updated on a matched node.
    UpdateAttributes {
        /// The path to the node containing the attributes
        path: Path,
        /// The attribute changes
        changes: Vec<AttributeChange>,
    },
    /// A node was inserted in tree B.
    /// In Chawathe semantics, Insert does NOT shift - it places at a position
    /// and whatever was there gets displaced (detached to a slot for later reinsertion).
    Insert {
        /// The parent node - either a path in the tree or a slot number
        parent: NodeRef,
        /// The position within the parent's children
        position: usize,
        /// The path in tree_b coordinates (for navigating new_doc to get content)
        label_path: Path,
        /// The value to insert (for leaf nodes), None for containers
        value: Option<String>,
        /// If Some, the displaced node goes to this slot
        detach_to_slot: Option<u32>,
        /// Hash of the inserted value
        hash: cinereus::NodeHash,
    },
    /// A node was deleted from tree A.
    Delete {
        /// The node to delete - either at a path or in a slot
        node: NodeRef,
        /// Hash of the deleted value
        hash: cinereus::NodeHash,
    },
    /// A node was moved from one location to another.
    /// If `detach_to_slot` is Some, the node at the target is detached and stored in that slot.
    Move {
        /// The source - either a path or a slot number
        from: NodeRef,
        /// The target - either a path in the tree or a slot-relative path
        to: NodeRef,
        /// If Some, the displaced node goes to this slot
        detach_to_slot: Option<u32>,
        /// Hash of the moved value
        hash: cinereus::NodeHash,
    },
}

/// Reference to a node - either by path or by slot number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeRef {
    /// Node at a path in the tree
    Path(Path),
    /// Node in a slot (previously detached).
    /// The optional path is relative to the slot root - used when the target
    /// is nested inside the detached subtree.
    Slot(u32, Option<Path>),
}

/// A single attribute change within an UpdateAttributes operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeChange {
    /// The attribute name (field name)
    pub attr_name: &'static str,
    /// The old value (None if attribute was absent)
    pub old_value: Option<String>,
    /// The new value (None if attribute is being removed)
    pub new_value: Option<String>,
}

/// Properties for HTML/XML nodes: attribute key-value pairs.
///
/// These are fields marked with `#[facet(html::attribute)]` or `#[facet(xml::attribute)]`.
/// They are diffed field-by-field when nodes match, avoiding the cross-matching problem
/// where identical Option values (like None) get matched across different fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HtmlProperties {
    /// Attribute values keyed by field name.
    /// Values are stored as `Option<String>` to handle both present and absent attributes.
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
        let all_keys: std::collections::HashSet<_> =
            self.attrs.keys().chain(other.attrs.keys()).collect();

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
        let hash = cinereus::NodeHash(hasher.finish());

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
        self.collect_properties_recursive(peek, &mut props);
        trace_verbose!(
            shape = peek.shape().type_identifier,
            props_count = props.attrs.len(),
            "collect_properties result"
        );
        props
    }

    /// Recursively collect attribute fields, including from flattened structs and enum variants.
    fn collect_properties_recursive<'mem, 'facet>(
        &self,
        peek: Peek<'mem, 'facet>,
        props: &mut HtmlProperties,
    ) {
        trace_verbose!(
            shape = peek.shape().type_identifier,
            "collect_properties_recursive"
        );

        match &peek.shape().ty {
            Type::User(UserType::Struct(_)) => {
                if let Ok(s) = peek.into_struct() {
                    for (field, field_peek) in s.fields() {
                        if field.is_attribute() {
                            let value = self.extract_attribute_value(field_peek);
                            // trace!(field = field.name, ?value, "found attribute field");
                            props.set(field.name, value);
                        } else if field.is_text() {
                            // Text content stored as special _text property
                            let value = self.extract_attribute_value(field_peek);
                            props.set("_text", value);
                        } else if field.is_flattened() {
                            // trace!(field = field.name, "recursing into flattened field");
                            self.collect_properties_recursive(field_peek, props);
                        }
                    }
                }
            }
            Type::User(UserType::Enum(_)) => {
                // For enums, get the active variant's inner value and recurse
                if let Ok(e) = peek.into_enum() {
                    // Tuple variants have fields - get field 0 (the inner struct)
                    if let Ok(Some(inner)) = e.field(0) {
                        // trace!("recursing into enum variant inner value");
                        self.collect_properties_recursive(inner, props);
                    }
                }
            }
            _ => {
                // For scalar string values, store as _text property
                if let Some(s) = peek.as_str() {
                    props.set("_text", Some(s.to_string()));
                }
            }
        }
    }

    /// Extract an attribute value as `Option<String>`.
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
                        // For flattened fields, we need to decide based on what they contain:
                        // - Flattened structs (like GlobalAttrs) contain attributes -> skip as tree node
                        //   (their attributes are collected as properties on the parent)
                        // - Flattened lists (like children: Vec<FlowContent>) -> process their items
                        if field.is_flattened() {
                            // Check if the field is a struct type (like GlobalAttrs)
                            if let Type::User(UserType::Struct(_)) = field_peek.shape().ty {
                                // Flattened struct - skip it (attributes collected as properties)
                                continue;
                            }
                            // For flattened lists/arrays, process their items directly
                            // (don't create a tree node for the list field itself)
                            if let Ok(list) = field_peek.into_list_like() {
                                for (i, elem) in list.iter().enumerate() {
                                    let child_path = path.with(PathSegment::Index(i));
                                    let child_id = self.build_node(elem, child_path);
                                    parent_id.append(child_id, &mut self.arena);
                                }
                            }
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

    debug!(
        cinereus_ops_count = cinereus_ops.len(),
        "cinereus ops before conversion"
    );
    #[allow(clippy::unused_enumerate_index)]
    for (_i, _op) in cinereus_ops.iter().enumerate() {
        debug!(_i, %_op, "cinereus op");
    }

    // Convert cinereus ops to path-based EditOps using a shadow tree
    // to track index shifts as operations are applied.
    // We pass the original values so we can extract actual values for leaf nodes.
    let peek_a = Peek::new(a);
    let peek_b = Peek::new(b);
    let result = convert_ops_with_shadow(cinereus_ops, &tree_a, &tree_b, &matching, peek_a, peek_b);
    debug!(result_count = result.len(), "edit ops after conversion");

    #[allow(clippy::let_and_return)]
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
    #[allow(clippy::unused_enumerate_index)]
    for (_i, segment) in path.0.iter().enumerate() {
        debug!(_i, ?segment, shape = ?peek.shape().type_identifier, "extract_value_at_path navigating");
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
                    // Option might contain a struct with flattened list
                    if let Some(inner) = opt.value() {
                        if let Ok(s) = inner.into_struct() {
                            // Find flattened list field
                            let mut found = None;
                            for (field, field_peek) in s.fields() {
                                if field.is_flattened()
                                    && let Ok(list) = field_peek.into_list()
                                {
                                    found = list.get(*idx);
                                    break;
                                }
                            }
                            found?
                        } else if let Ok(list) = inner.into_list() {
                            list.get(*idx)?
                        } else if *idx == 0 {
                            inner
                        } else {
                            debug!(
                                "extract_value_at_path: option inner not struct/list, index != 0"
                            );
                            return None;
                        }
                    } else {
                        debug!("extract_value_at_path: option is None");
                        return None;
                    }
                } else if let Ok(s) = peek.into_struct() {
                    // Struct with flattened list - find it and index
                    let mut found = None;
                    for (field, field_peek) in s.fields() {
                        if field.is_flattened()
                            && let Ok(list) = field_peek.into_list()
                        {
                            found = list.get(*idx);
                            break;
                        }
                    }
                    found?
                } else if let Ok(e) = peek.into_enum() {
                    e.field(*idx).ok()??
                } else {
                    debug!("extract_value_at_path: not a list, option, struct, or enum for Index");
                    return None;
                }
            }
            PathSegment::Variant(_) => {
                // Variant just means we're already at that variant, continue
                peek
            }
            PathSegment::Key(key) => {
                if let Ok(map) = peek.into_map() {
                    let mut found = None;
                    for (k, v) in map.iter() {
                        if let Some(s) = k.as_str()
                            && s == key
                        {
                            found = Some(v);
                            break;
                        }
                    }
                    found?
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

/// Format the shadow tree for debugging.
#[allow(dead_code, clippy::only_used_in_recursion)]
fn format_shadow_tree(
    arena: &indextree::Arena<NodeData<NodeKind, NodeLabel, HtmlProperties>>,
    root: NodeId,
    node: NodeId,
    depth: usize,
) -> String {
    let mut out = String::new();
    let indent = "  ".repeat(depth);

    if let Some(node_ref) = arena.get(node) {
        let data = node_ref.get();
        let kind_str = match &data.kind {
            NodeKind::Struct(name) => format!("Struct({name})"),
            NodeKind::EnumVariant(e, v) => format!("Variant({e}::{v})"),
            NodeKind::List(name) => format!("List({name})"),
            NodeKind::Map(name) => format!("Map({name})"),
            NodeKind::Option(name) => format!("Option({name})"),
            NodeKind::Scalar(name) => format!("Scalar({name})"),
        };
        let label_str = data
            .label
            .as_ref()
            .map(|l| format!(" path={}", l.path))
            .unwrap_or_default();

        out.push_str(&format!(
            "{indent}[{id}] {kind}{label}\n",
            id = usize::from(node),
            kind = kind_str,
            label = label_str
        ));

        for child in node.children(arena) {
            out.push_str(&format_shadow_tree(arena, root, child, depth + 1));
        }
    }

    out
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
    _peek_a: Peek<'mem, 'facet>,
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

    // Track detached nodes: NodeId -> slot number
    // When a Move places a node at a position occupied by another node,
    // the occupant is detached and stored in a slot for later reinsertion.
    let mut detached_nodes: HashMap<NodeId, u32> = HashMap::new();
    let mut next_slot: u32 = 0;

    debug!(
        "SHADOW TREE INITIAL STATE:\n{}",
        format_shadow_tree(&shadow_arena, shadow_root, shadow_root, 0)
    );

    // Process operations in cinereus order.
    // For each op: update shadow tree first, THEN compute paths from updated tree.
    for op in ops {
        match op {
            CinereusEditOp::UpdateProperties {
                node_a,
                node_b: _,
                changes,
            } => {
                // Path to the node containing the attributes
                let path = compute_path_in_shadow(&shadow_arena, shadow_root, node_a, tree_a);

                // Convert cinereus PropertyChange to our AttributeChange
                let changes: Vec<AttributeChange> = changes
                    .into_iter()
                    .map(|c| AttributeChange {
                        attr_name: c.key,
                        // Flatten Option<Option<String>> to Option<String>
                        old_value: c.old_value.flatten(),
                        new_value: c.new_value.flatten(),
                    })
                    .collect();

                debug!(
                    %path,
                    num_changes = changes.len(),
                    "emitting EditOp::UpdateAttributes"
                );
                result.push(EditOp::UpdateAttributes { path, changes });
                // No structural change for UpdateAttributes
            }

            CinereusEditOp::Insert {
                node_b,
                parent_b,
                position,
                label,
                ..
            } => {
                debug!(
                    node_b = usize::from(node_b),
                    parent_b = usize::from(parent_b),
                    position,
                    "INSERT: starting"
                );

                // Find the parent in our shadow tree
                let shadow_parent = b_to_shadow.get(&parent_b).copied().unwrap_or(shadow_root);

                // Create a new node in shadow tree
                let new_data: NodeData<NodeKind, NodeLabel, HtmlProperties> = NodeData {
                    hash: cinereus::NodeHash(0),
                    kind: NodeKind::Scalar("inserted"),
                    label: label.clone(),
                    properties: HtmlProperties::new(),
                };
                let new_node = shadow_arena.new_node(new_data);

                // In Chawathe semantics, Insert does NOT shift - it places at position
                // and whatever was there gets displaced (detached to a slot).
                // We insert new_node before occupant, then detach occupant (swap, no shift).
                let children: Vec<_> = shadow_parent.children(&shadow_arena).collect();
                let detach_to_slot = if position < children.len() {
                    let occupant = children[position];
                    let occupant_slot = next_slot;
                    next_slot += 1;
                    debug!(
                        occupant = usize::from(occupant),
                        occupant_slot, "INSERT: will detach occupant to slot"
                    );
                    // Insert new_node before occupant, then detach occupant
                    occupant.insert_before(new_node, &mut shadow_arena);
                    occupant.detach(&mut shadow_arena);
                    detached_nodes.insert(occupant, occupant_slot);
                    Some(occupant_slot)
                } else {
                    // No occupant, just append
                    shadow_parent.append(new_node, &mut shadow_arena);
                    None
                };

                b_to_shadow.insert(node_b, new_node);

                // Determine the parent reference - either a path or a slot
                // We need to check if the parent OR any ancestor is detached
                let parent = if let Some(&slot) = detached_nodes.get(&shadow_parent) {
                    // Parent is directly in a slot
                    NodeRef::Slot(slot, None)
                } else if let Some((slot, relative_path)) =
                    find_detached_ancestor(&shadow_arena, shadow_parent, &detached_nodes)
                {
                    // An ancestor is in a slot - include the relative path to the parent
                    NodeRef::Slot(slot, relative_path)
                } else {
                    // Parent is in the tree - compute its path
                    let parent_path =
                        compute_path_in_shadow(&shadow_arena, shadow_root, shadow_parent, tree_a);
                    NodeRef::Path(parent_path)
                };

                // Extract value for leaf nodes using the tree_b label path
                let label_path = tree_b
                    .get(node_b)
                    .label
                    .as_ref()
                    .map(|l| l.path.clone())
                    .unwrap_or_else(|| Path(vec![]));
                let value = extract_value_at_path(peek_b, &label_path);

                let edit_op = EditOp::Insert {
                    parent,
                    position,
                    label_path,
                    value,
                    detach_to_slot,
                    hash: tree_b.get(node_b).hash,
                };
                debug!(?edit_op, "emitting Insert");
                result.push(edit_op);

                debug!(
                    "SHADOW TREE AFTER INSERT:\n{}",
                    format_shadow_tree(&shadow_arena, shadow_root, shadow_root, 0)
                );
            }

            CinereusEditOp::Delete { node_a } => {
                // Check if the node or any ancestor is currently detached (in a slot)
                let node = if let Some(slot) = detached_nodes.remove(&node_a) {
                    // Node is directly in a slot - delete from slot
                    NodeRef::Slot(slot, None)
                } else if let Some((slot, relative_path)) =
                    find_detached_ancestor(&shadow_arena, node_a, &detached_nodes)
                {
                    // An ancestor is in a slot - the node is inside a detached subtree
                    // It will be deleted when the slot is cleared or the subtree is replaced
                    NodeRef::Slot(slot, relative_path)
                } else {
                    // Node is in the tree - delete from path
                    let path = compute_path_in_shadow(&shadow_arena, shadow_root, node_a, tree_a);
                    // Swap with placeholder (insert placeholder before, then detach)
                    // This prevents shifting of siblings.
                    let placeholder_data: NodeData<NodeKind, NodeLabel, HtmlProperties> =
                        NodeData {
                            hash: cinereus::NodeHash(0),
                            kind: NodeKind::Scalar("placeholder"),
                            label: None,
                            properties: HtmlProperties::new(),
                        };
                    let placeholder = shadow_arena.new_node(placeholder_data);
                    node_a.insert_before(placeholder, &mut shadow_arena);
                    node_a.detach(&mut shadow_arena);
                    NodeRef::Path(path)
                };

                let edit_op = EditOp::Delete {
                    node,
                    hash: tree_a.get(node_a).hash,
                };
                debug!(?edit_op, "emitting Delete");
                result.push(edit_op);

                debug!(
                    "SHADOW TREE AFTER DELETE:\n{}",
                    format_shadow_tree(&shadow_arena, shadow_root, shadow_root, 0)
                );
            }

            CinereusEditOp::Move {
                node_a,
                node_b,
                new_parent_b,
                new_position,
            } => {
                debug!(
                    node_a = usize::from(node_a),
                    node_b = usize::from(node_b),
                    new_parent_b = usize::from(new_parent_b),
                    new_position,
                    "MOVE: starting"
                );

                // Find new parent in shadow tree
                let shadow_new_parent =
                    b_to_shadow.get(&new_parent_b).copied().unwrap_or_else(|| {
                        // new_parent_b is not in b_to_shadow - this shouldn't happen after matching fixes.
                        // If this occurs, cinereus should have skipped this Move because parent_b is unmatched.
                        debug!(
                            new_parent_b = usize::from(new_parent_b),
                            node_a = usize::from(node_a),
                            node_b = usize::from(node_b),
                            "WARNING: new_parent_b not in b_to_shadow, falling back to shadow_root"
                        );
                        shadow_root
                    });

                // Check if the node is currently detached (in limbo)
                let is_detached = detached_nodes.contains_key(&node_a);

                // Determine the source for the Move
                let from = if is_detached {
                    let slot = detached_nodes.remove(&node_a).unwrap();
                    NodeRef::Slot(slot, None)
                } else {
                    let old_path =
                        compute_path_in_shadow(&shadow_arena, shadow_root, node_a, tree_a);
                    // Swap node_a with a placeholder (insert placeholder before, then detach)
                    // This prevents shifting of siblings.
                    let placeholder_data: NodeData<NodeKind, NodeLabel, HtmlProperties> =
                        NodeData {
                            hash: cinereus::NodeHash(0),
                            kind: NodeKind::Scalar("placeholder"),
                            label: None,
                            properties: HtmlProperties::new(),
                        };
                    let placeholder = shadow_arena.new_node(placeholder_data);
                    node_a.insert_before(placeholder, &mut shadow_arena);
                    node_a.detach(&mut shadow_arena);
                    NodeRef::Path(old_path)
                };

                // Check if something is at the target position that needs to be detached
                // We insert node_a before occupant, then detach occupant (swap, no shift).
                let children: Vec<_> = shadow_new_parent.children(&shadow_arena).collect();
                let detach_to_slot = if new_position < children.len() {
                    let occupant = children[new_position];
                    // Don't detach ourselves (shouldn't happen since we already detached)
                    if occupant != node_a {
                        let occupant_slot = next_slot;
                        next_slot += 1;
                        debug!(
                            ?occupant,
                            occupant_slot, "MOVE: will detach occupant to slot"
                        );
                        // Insert node_a before occupant, then detach occupant
                        occupant.insert_before(node_a, &mut shadow_arena);
                        occupant.detach(&mut shadow_arena);
                        detached_nodes.insert(occupant, occupant_slot);
                        Some(occupant_slot)
                    } else {
                        // node_a is already at the target position, nothing to do
                        None
                    }
                } else {
                    // No occupant, just append
                    shadow_new_parent.append(node_a, &mut shadow_arena);
                    None
                };

                // Compute the target path: parent's path + new_position
                // We use new_position directly because that's the FINAL position in tree_b,
                // not the current shadow tree position (which may have gaps from detached nodes).
                //
                // Check if the parent is in a detached subtree (slot) - if so, we need
                // to use a slot-relative path, not a root-relative path.
                let to = if let Some(&slot) = detached_nodes.get(&shadow_new_parent) {
                    // Parent is directly in a slot
                    let mut rel_path = Path(vec![]);
                    rel_path.0.push(PathSegment::Index(new_position));
                    NodeRef::Slot(slot, Some(rel_path))
                } else if let Some((slot, relative_path)) =
                    find_detached_ancestor(&shadow_arena, shadow_new_parent, &detached_nodes)
                {
                    // An ancestor is in a slot - extend the relative path
                    let mut rel_path = relative_path.unwrap_or_else(|| Path(vec![]));
                    rel_path.0.push(PathSegment::Index(new_position));
                    NodeRef::Slot(slot, Some(rel_path))
                } else {
                    // Parent is in the tree - compute its path
                    let mut parent_path = compute_path_in_shadow(
                        &shadow_arena,
                        shadow_root,
                        shadow_new_parent,
                        tree_a,
                    );
                    parent_path.0.push(PathSegment::Index(new_position));
                    NodeRef::Path(parent_path)
                };

                // Emit Move
                debug!(?from, ?to, ?detach_to_slot, "MOVE: emitting");
                result.push(EditOp::Move {
                    from,
                    to,
                    detach_to_slot,
                    hash: tree_b.get(node_b).hash,
                });

                // Update b_to_shadow
                b_to_shadow.insert(node_b, node_a);

                debug!(
                    "SHADOW TREE AFTER MOVE:\n{}",
                    format_shadow_tree(&shadow_arena, shadow_root, shadow_root, 0)
                );
            }
        }
    }

    result
}

/// Check if any ancestor of a node is in the detached_nodes map.
/// Returns (slot_number, relative_path) if an ancestor is detached, None otherwise.
/// The relative_path is the path from the detached ancestor (slot root) to the node,
/// preserving the actual PathSegments (including Variant segments) from the node labels.
fn find_detached_ancestor(
    shadow_arena: &indextree::Arena<NodeData<NodeKind, NodeLabel, HtmlProperties>>,
    node: NodeId,
    detached_nodes: &HashMap<NodeId, u32>,
) -> Option<(u32, Option<Path>)> {
    let mut current = node;
    // Collect (child_node, parent_node) pairs as we traverse up
    let mut traversal: Vec<(NodeId, NodeId)> = Vec::new();
    trace!(node = usize::from(node), "find_detached_ancestor: starting");

    loop {
        trace!(
            current = usize::from(current),
            "find_detached_ancestor: checking"
        );
        // Check if current node is detached
        if let Some(&slot) = detached_nodes.get(&current) {
            trace!(
                current = usize::from(current),
                slot,
                traversal_len = traversal.len(),
                "find_detached_ancestor: found!"
            );
            // Build the relative path from slot root to the original node
            // by using the label paths from each node
            let relative_path = if traversal.is_empty() {
                None // Node is directly the slot root
            } else {
                // Get the slot root's path (current node's label)
                let slot_root_path_len = shadow_arena
                    .get(current)
                    .and_then(|n| n.get().label.as_ref())
                    .map(|l| l.path.0.len())
                    .unwrap_or(0);

                // Get the target node's full path (original node's label)
                let target_path = shadow_arena
                    .get(node)
                    .and_then(|n| n.get().label.as_ref())
                    .map(|l| l.path.0.clone());

                if let Some(full_path) = target_path {
                    // The relative path is the suffix after the slot root's path
                    if full_path.len() > slot_root_path_len {
                        let relative_segments = full_path[slot_root_path_len..].to_vec();
                        trace!(
                            ?relative_segments,
                            "find_detached_ancestor: relative path from labels"
                        );
                        Some(Path(relative_segments))
                    } else {
                        None
                    }
                } else {
                    // Fallback: use position indices if labels aren't available
                    // This preserves old behavior for nodes without labels
                    let mut path_indices: Vec<usize> = Vec::new();
                    for (child, parent) in traversal.iter().rev() {
                        let pos = parent
                            .children(shadow_arena)
                            .position(|c| c == *child)
                            .unwrap_or(0);
                        path_indices.push(pos);
                    }
                    Some(Path(
                        path_indices.into_iter().map(PathSegment::Index).collect(),
                    ))
                }
            };
            return Some((slot, relative_path));
        }
        // Move to parent, recording the traversal
        if let Some(parent_id) = shadow_arena.get(current).and_then(|n| n.parent()) {
            traversal.push((current, parent_id));
            current = parent_id;
        } else {
            trace!(
                current = usize::from(current),
                "find_detached_ancestor: no parent, stopping"
            );
            // No more parents
            break;
        }
    }
    None
}

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

    if let Some(node_ref) = shadow_arena.get(node)
        && let Some(label) = &node_ref.get().label
    {
        // Original node from tree_a - use its stored path
        // But we need to update any indices that may have shifted
        return compute_adjusted_path(shadow_arena, shadow_root, node, &label.path);
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

    debug!(node = usize::from(node), %original_path, "compute_adjusted_path start");

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
            if let Some(PathSegment::Index(_)) = original_path.0.get(depth)
                && let Some(parent_id) = shadow_arena.get(current).and_then(|n| n.parent())
            {
                let children: Vec<_> = parent_id.children(shadow_arena).collect();
                let pos = children.iter().position(|&c| c == current).unwrap_or(0);
                debug!(
                    current = usize::from(current),
                    parent = usize::from(parent_id),
                    depth,
                    pos,
                    num_children = children.len(),
                    "recording position"
                );
                depth_to_position.insert(depth, pos);
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
    let edit_ops =
        convert_ops_with_shadow(cinereus_ops, &tree_a, &tree_b, &matching, peek_a, peek_b);

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

#[cfg(test)]
mod tests;
