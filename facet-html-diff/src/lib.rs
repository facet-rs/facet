//! HTML diffing with DOM patch generation.
//!
//! This crate translates facet-diff EditOps (from GumTree/Chawathe) into DOM Patches
//! that can be applied to update an HTML document incrementally.

#[macro_use]
mod macros;

pub mod apply;

// Re-export for convenience
pub use apply::{Node, apply_patches, parse_html};

use facet_core::{Def, Field, Type, UserType};
use facet_diff::{EditOp, PathSegment, tree_diff};
use facet_dom::naming::to_element_name;
use facet_html_dom::*;
use facet_reflect::{HasFields, Peek, PeekStruct};

/// A path to a node in the DOM tree.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(transparent)]
pub struct NodePath(pub Vec<usize>);

/// Reference to a node - either by path or by slot number.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum NodeRef {
    /// Node at a path in the DOM
    Path(NodePath),
    /// Node in a slot (previously detached).
    /// The optional path is relative to the slot root - used when the target
    /// is nested inside the detached subtree.
    Slot(u32, Option<NodePath>),
}

/// Operations to transform the DOM.
///
/// These follow Chawathe semantics: Insert/Move operations do NOT shift siblings.
/// Instead, they displace whatever is at the target position to a slot.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum Patch {
    /// Insert an empty element at position within parent.
    /// If `detach_to_slot` is Some, the node at that position is detached and stored in that slot.
    /// Children are NOT included - they will be placed via separate Move/InsertElement/InsertText ops.
    InsertElement {
        parent: NodeRef,
        position: usize,
        tag: String,
        detach_to_slot: Option<u32>,
    },

    /// Insert a text node at position within parent.
    /// If `detach_to_slot` is Some, the node at that position is detached and stored in that slot.
    InsertText {
        parent: NodeRef,
        position: usize,
        text: String,
        detach_to_slot: Option<u32>,
    },

    /// Remove a node (either at a path or in a slot)
    Remove { node: NodeRef },

    /// Update text content of a text node at path.
    /// Path points to the text node itself, not the parent element.
    SetText { path: NodePath, text: String },

    /// Set attribute on element at path
    SetAttribute {
        path: NodePath,
        name: String,
        value: String,
    },

    /// Remove attribute from element at path
    RemoveAttribute { path: NodePath, name: String },

    /// Move a node from one location to another.
    /// If `detach_to_slot` is Some, the node currently at `to` is detached and stored in that slot.
    Move {
        from: NodeRef,
        to: NodePath,
        detach_to_slot: Option<u32>,
    },
}

/// Diff two HTML documents and return DOM patches.
pub fn diff_html(old_html: &str, new_html: &str) -> Result<Vec<Patch>, String> {
    let old_doc: Html =
        facet_html::from_str(old_html).map_err(|e| format!("Failed to parse old HTML: {e}"))?;
    let new_doc: Html =
        facet_html::from_str(new_html).map_err(|e| format!("Failed to parse new HTML: {e}"))?;

    let edit_ops = tree_diff(&old_doc, &new_doc);

    debug!(count = edit_ops.len(), "Edit ops from facet-diff");
    for _op in &edit_ops {
        debug!(?_op, "edit op");
    }

    let patches = translate_to_patches(&edit_ops, &new_doc);

    debug!(count = patches.len(), "Translated patches");
    for _patch in &patches {
        debug!(?_patch, "patch");
    }

    Ok(patches)
}

#[cfg(not(feature = "tracing"))]
pub fn diff_html_debug(old_html: &str, new_html: &str) -> Result<Vec<Patch>, String> {
    diff_html(old_html, new_html)
}

/// Translate facet-diff EditOps into DOM Patches.
pub fn translate_to_patches(edit_ops: &[EditOp], new_doc: &Html) -> Vec<Patch> {
    edit_ops
        .iter()
        .flat_map(|op| translate_op(op, new_doc))
        .collect()
}

/// What does this path point to?
#[derive(Debug, Clone, PartialEq)]
enum PathTarget {
    /// Text content (html::text field or variant)
    Text,
    /// An attribute (html::attribute field or map key in extra)
    Attribute(String),
    /// An element node
    Element,
    /// A flattened struct containing attributes
    FlattenedAttributeStruct,
    /// A flattened list containing children
    FlattenedChildrenList,
    /// Something structural
    Other,
}

/// Result of navigating a path through the type structure.
struct PathNavigation {
    /// DOM path - indices into flattened children lists only
    dom_path: Vec<usize>,
    /// What the path points to
    target: PathTarget,
    /// DOM path to the containing element (for attribute/text targets)
    element_dom_path: Vec<usize>,
}

/// Navigate a path through the type structure, using metadata to build DOM path.
///
/// facet-diff generates paths that SKIP Options and flattened field names.
/// So `F(body), I(1), V(P)` means: body field → children\[1\] → P variant
/// The Index(1) is NOT an Option unwrap - it's the children list index.
///
/// The rules:
/// - Field: navigate to that field's type
/// - Index at Option with flattened list inside: unwrap Option, use index on the list (DOM index)
/// - Index at struct with flattened list: use index on the list (DOM index)
/// - Index at enum (after Variant): tuple field access (NOT a DOM index)
/// - Index at List: list access (DOM index if the list is children)
/// - Variant at enum: select variant
/// - Variant at struct: find flattened list containing that variant's enum
fn navigate_path(
    segments: &[PathSegment],
    root_shape: &'static facet_core::Shape,
) -> PathNavigation {
    let mut dom_path = Vec::new();
    let mut element_dom_path = Vec::new();
    let mut target = PathTarget::Other;
    let mut current_shape = root_shape;
    let mut after_variant = false; // Track if previous segment was Variant

    for (i, segment) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        trace!(
            i,
            ?segment,
            shape = current_shape.type_identifier,
            ?after_variant,
            "navigate_path"
        );

        match segment {
            PathSegment::Field(name) => {
                after_variant = false;
                // Navigate through Option if needed
                let struct_shape = unwrap_option(current_shape);

                if let Type::User(UserType::Struct(struct_def)) = &struct_shape.ty {
                    if let Some((field, field_shape)) = find_field_in_struct(struct_def, name) {
                        current_shape = field_shape;

                        if is_last {
                            if field.is_attribute() {
                                target = PathTarget::Attribute(name.to_string());
                            } else if field.is_text() {
                                target = PathTarget::Text;
                            } else if field.is_flattened() {
                                if matches!(field_shape.ty, Type::User(UserType::Struct(_))) {
                                    target = PathTarget::FlattenedAttributeStruct;
                                } else if is_list_type(field_shape) {
                                    target = PathTarget::FlattenedChildrenList;
                                }
                            } else {
                                target = PathTarget::Other;
                            }
                        }

                        if !field.is_attribute() && !field.is_text() {
                            element_dom_path = dom_path.clone();
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            PathSegment::Index(idx) => {
                if after_variant {
                    // Index after Variant = tuple field access, NOT a DOM index
                    after_variant = false;
                    if let Type::User(UserType::Enum(enum_def)) = &current_shape.ty {
                        // Get the variant from the previous segment
                        if let Some(PathSegment::Variant(var_name)) =
                            segments.get(i.wrapping_sub(1))
                            && let Some(variant) =
                                enum_def.variants.iter().find(|v| v.name == var_name)
                            && let Some(field) = variant.data.fields.get(*idx)
                        {
                            current_shape = field.shape();
                            if is_last {
                                if variant.is_text() {
                                    target = PathTarget::Text;
                                } else if is_transparent_element_struct(current_shape) {
                                    // Struct with only flattened fields (attrs + children)
                                    // This is the "content" of the element, not a DOM node
                                    // Inserting this = replacing innerHTML
                                    target = PathTarget::FlattenedChildrenList;
                                } else {
                                    // Landing on struct/enum inside variant = element
                                    target = PathTarget::Element;
                                }
                            }
                        }
                    }
                    continue;
                }

                // Try to find a flattened children list to index into
                if let Some((list_elem_shape, is_children)) = find_flattened_list(current_shape) {
                    if is_children {
                        dom_path.push(*idx);
                        element_dom_path = dom_path.clone();
                    }
                    current_shape = list_elem_shape;
                    if is_last {
                        target = PathTarget::Element;
                    }
                } else if let Def::List(list_def) = &current_shape.def {
                    // Direct list access
                    dom_path.push(*idx);
                    element_dom_path = dom_path.clone();
                    current_shape = list_def.t;
                    if is_last {
                        target = PathTarget::Element;
                    }
                } else {
                    break;
                }
            }

            PathSegment::Variant(name) => {
                after_variant = true;

                // Find the enum - might be directly here or in a flattened list
                let enum_shape = if let Type::User(UserType::Enum(_)) = &current_shape.ty {
                    current_shape
                } else if let Some((list_elem_shape, _)) = find_flattened_list(current_shape) {
                    list_elem_shape
                } else {
                    break;
                };

                if let Type::User(UserType::Enum(enum_def)) = &enum_shape.ty
                    && let Some(variant) = enum_def.variants.iter().find(|v| v.name == name)
                {
                    current_shape = enum_shape;
                    if is_last {
                        if variant.is_text() {
                            target = PathTarget::Text;
                        } else {
                            target = PathTarget::Element;
                        }
                    }
                }
            }

            PathSegment::Key(key) => {
                after_variant = false;
                if let Def::Map(map_def) = &current_shape.def {
                    current_shape = map_def.v;
                    if is_last {
                        target = PathTarget::Attribute(key.to_string());
                    }
                }
            }
        }
    }

    PathNavigation {
        dom_path,
        target,
        element_dom_path,
    }
}

/// Unwrap Option type if present, otherwise return the shape as-is.
fn unwrap_option(shape: &'static facet_core::Shape) -> &'static facet_core::Shape {
    if let Def::Option(opt_def) = &shape.def {
        opt_def.t
    } else {
        shape
    }
}

/// Find a flattened list field in a shape (unwrapping Option if needed).
/// Returns (element_shape, is_children) where is_children is true if the list
/// contains DOM children (elements with structure, not just attributes).
fn find_flattened_list(
    shape: &'static facet_core::Shape,
) -> Option<(&'static facet_core::Shape, bool)> {
    let struct_shape = unwrap_option(shape);

    if let Type::User(UserType::Struct(struct_def)) = &struct_shape.ty {
        for field in struct_def.fields.iter() {
            if field.is_flattened() {
                let field_shape = field.shape();
                if let Def::List(list_def) = &field_shape.def {
                    // Check if items are children (elements) vs attributes
                    // Children = items that have their own structure (not just attribute fields)
                    let is_children = is_element_type(list_def.t);
                    return Some((list_def.t, is_children));
                }
            }
        }
    }
    None
}

/// Check if a type represents a DOM element (has structure beyond just attributes).
/// An element type is a struct/enum that can have children or text content,
/// not just a struct whose fields are all attributes.
fn is_element_type(shape: &facet_core::Shape) -> bool {
    match &shape.ty {
        Type::User(UserType::Enum(_)) => {
            // Enums like FlowContent, PhrasingContent are element containers
            true
        }
        Type::User(UserType::Struct(struct_def)) => {
            // A struct is an element if it has any non-attribute fields
            // (i.e., it can have children or text content)
            struct_def.fields.iter().any(|f| !f.is_attribute())
        }
        _ => false,
    }
}

/// Check if a struct is "transparent" - all fields are flattened.
/// These structs (like Div, Span, P) contain attrs + children but don't
/// represent a DOM node themselves. The DOM element is the enum variant.
fn is_transparent_element_struct(shape: &facet_core::Shape) -> bool {
    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        // All fields must be flattened for it to be transparent
        struct_def.fields.iter().all(|f| f.is_flattened())
    } else {
        false
    }
}

/// Find a field in a struct, including checking flattened structs recursively.
fn find_field_in_struct(
    struct_def: &facet_core::StructType,
    name: &str,
) -> Option<(&'static Field, &'static facet_core::Shape)> {
    for field in struct_def.fields.iter() {
        if field.name == name {
            return Some((field, field.shape()));
        }
        // Check flattened structs
        if field.is_flattened()
            && let Type::User(UserType::Struct(inner_struct)) = &field.shape().ty
            && let Some(result) = find_field_in_struct(inner_struct, name)
        {
            return Some(result);
        }
    }
    None
}

/// Check if a shape is a list type.
fn is_list_type(shape: &facet_core::Shape) -> bool {
    matches!(shape.def, Def::List(_))
}

/// Convert a facet path to a DOM path.
fn to_dom_path(segments: &[PathSegment]) -> Vec<usize> {
    let html_shape = <Html as facet_core::Facet>::SHAPE;
    navigate_path(segments, html_shape).dom_path
}

/// Extract DOM indices from path segments.
///
/// Index segments that follow a Variant are tuple field accesses (not DOM indices).
/// All other Index segments are DOM child indices.
fn extract_dom_indices(segments: &[PathSegment]) -> Vec<usize> {
    let mut result = Vec::new();
    let mut after_variant = false;

    for seg in segments {
        match seg {
            PathSegment::Index(idx) => {
                if after_variant {
                    // Tuple field access, not a DOM index
                    after_variant = false;
                } else {
                    // DOM child index
                    result.push(*idx);
                }
            }
            PathSegment::Variant(_) => {
                after_variant = true;
            }
            _ => {
                after_variant = false;
            }
        }
    }

    result
}

/// Translate a single EditOp to DOM Patches.
fn translate_op(op: &EditOp, new_doc: &Html) -> Vec<Patch> {
    trace!("translate_op: op={op:?}");
    match op {
        EditOp::Insert {
            parent,
            position,
            label_path,
            value,
            detach_to_slot,
            ..
        } => translate_insert(
            parent,
            *position,
            &label_path.0,
            value.as_deref(),
            *detach_to_slot,
            new_doc,
        )
        .into_iter()
        .collect(),
        EditOp::Delete { node, .. } => {
            let node_ref = match node {
                facet_diff::NodeRef::Path(p) => NodeRef::Path(NodePath(extract_dom_indices(&p.0))),
                facet_diff::NodeRef::Slot(s, rel_path) => NodeRef::Slot(
                    *s,
                    rel_path
                        .as_ref()
                        .map(|p| NodePath(extract_dom_indices(&p.0))),
                ),
            };
            vec![Patch::Remove { node: node_ref }]
        }
        EditOp::Update {
            path, new_value, ..
        } => translate_update(&path.0, new_value.as_deref())
            .into_iter()
            .collect(),
        EditOp::Move {
            from,
            to,
            detach_to_slot,
            ..
        } => {
            let from_ref = match from {
                facet_diff::NodeRef::Path(p) => NodeRef::Path(NodePath(extract_dom_indices(&p.0))),
                facet_diff::NodeRef::Slot(s, rel_path) => NodeRef::Slot(
                    *s,
                    rel_path
                        .as_ref()
                        .map(|p| NodePath(extract_dom_indices(&p.0))),
                ),
            };
            let to_path = NodePath(extract_dom_indices(&to.0));
            vec![Patch::Move {
                from: from_ref,
                to: to_path,
                detach_to_slot: *detach_to_slot,
            }]
        }
        EditOp::UpdateAttribute {
            path,
            attr_name,
            new_value,
            ..
        } => translate_update_attribute(&path.0, attr_name, new_value.as_deref())
            .into_iter()
            .collect(),
        #[allow(unreachable_patterns)]
        _ => vec![],
    }
}

/// Translate an Insert operation.
///
/// `segments` is the path from EditOp - DOM position with Variants stripped.
/// `label_segments` is the label_path - full type navigation path with Variants.
/// `detach_to_slot` - if Some, the displaced node goes to this slot.
fn translate_insert(
    parent: &facet_diff::NodeRef,
    position: usize,
    label_segments: &[PathSegment],
    value: Option<&str>,
    detach_to_slot: Option<u32>,
    new_doc: &Html,
) -> Option<Patch> {
    let html_shape = <Html as facet_core::Facet>::SHAPE;

    // Use label_segments for type navigation (has Variant info)
    let nav = navigate_path(label_segments, html_shape);

    debug!(
        "translate_insert: parent={parent:?}, position={position}, label_segments={label_segments:?}, target={:?}, value={value:?}",
        nav.target
    );

    // Convert parent NodeRef to our NodeRef, and compute target path/ref
    let parent_ref = match parent {
        facet_diff::NodeRef::Path(p) => NodeRef::Path(NodePath(extract_dom_indices(&p.0))),
        facet_diff::NodeRef::Slot(s, rel_path) => NodeRef::Slot(
            *s,
            rel_path
                .as_ref()
                .map(|p| NodePath(extract_dom_indices(&p.0))),
        ),
    };

    match nav.target {
        PathTarget::Element => {
            // Navigate to the actual node to determine its type
            let peek = Peek::new(new_doc);
            let node_peek = navigate_peek(peek, label_segments)?;

            // Check if this is actually a text variant in the enum
            // (navigate_path may return Element for Index into enum lists where it can't know the variant)
            if let Ok(enum_peek) = node_peek.into_enum() {
                if let Ok(variant) = enum_peek.active_variant() {
                    if variant.is_text() {
                        // This is a text variant - extract the text value and emit InsertText
                        let text = enum_peek
                            .field(0)
                            .ok()
                            .flatten()
                            .and_then(|p| p.as_str().map(|s| s.to_string()))
                            .unwrap_or_default();
                        return Some(Patch::InsertText {
                            parent: parent_ref,
                            position,
                            text,
                            detach_to_slot,
                        });
                    }
                }
            }

            // Not a text variant - insert element
            let peek2 = Peek::new(new_doc);
            let node_peek2 = navigate_peek(peek2, label_segments)?;
            let tag = get_element_tag(node_peek2);

            Some(Patch::InsertElement {
                parent: parent_ref,
                position,
                tag,
                detach_to_slot,
            })
        }
        PathTarget::Attribute(name) => {
            // Attributes go on the parent element
            let element_path = match &parent_ref {
                NodeRef::Path(p) => p.clone(),
                NodeRef::Slot(..) => return None, // Can't set attr on slot directly
            };

            let peek = Peek::new(new_doc);
            if let Some(attr_peek) = navigate_peek(peek, label_segments) {
                if let Ok(opt) = attr_peek.into_option() {
                    if opt.value().is_some() {
                        let attr_value = value.map(|s| s.to_string()).or_else(|| {
                            let p2 = Peek::new(new_doc);
                            navigate_peek(p2, label_segments)
                                .and_then(|p| p.into_option().ok())
                                .and_then(|o| o.value())
                                .and_then(|inner| inner.as_str().map(|s| s.to_string()))
                        })?;
                        return Some(Patch::SetAttribute {
                            path: element_path,
                            name,
                            value: attr_value,
                        });
                    } else {
                        return Some(Patch::RemoveAttribute {
                            path: element_path,
                            name,
                        });
                    }
                } else if let Some(s) = attr_peek.as_str() {
                    return Some(Patch::SetAttribute {
                        path: element_path,
                        name,
                        value: s.to_string(),
                    });
                }
            }

            value.map(|v| Patch::SetAttribute {
                path: element_path,
                name: name.clone(),
                value: v.to_string(),
            })
        }
        PathTarget::Text => {
            // Insert a text node at the given position
            let text = value?.to_string();
            Some(Patch::InsertText {
                parent: parent_ref,
                position,
                text,
                detach_to_slot,
            })
        }
        PathTarget::FlattenedAttributeStruct => {
            let element_path = match &parent_ref {
                NodeRef::Path(p) => p.0.clone(),
                NodeRef::Slot(..) => return None,
            };
            let patches = sync_attrs_from_new_doc(&element_path, label_segments, new_doc);
            patches.into_iter().next()
        }
        PathTarget::FlattenedChildrenList => {
            // Individual children will be inserted separately by cinereus
            None
        }
        PathTarget::Other => None,
    }
}

/// Sync all attributes from new_doc for an element.
fn sync_attrs_from_new_doc(
    dom_path: &[usize],
    attrs_path: &[PathSegment],
    new_doc: &Html,
) -> Vec<Patch> {
    let mut patches = Vec::new();

    let peek = Peek::new(new_doc);
    if let Some(attrs_peek) = navigate_peek(peek, attrs_path) {
        collect_attributes_recursive(attrs_peek, dom_path, &mut patches);
    }

    patches
}

/// Recursively collect attributes from a peek, handling flattened structs.
fn collect_attributes_recursive(peek: Peek<'_, '_>, dom_path: &[usize], patches: &mut Vec<Patch>) {
    if let Ok(s) = peek.into_struct() {
        for (field, field_peek) in s.fields() {
            if field.is_attribute() {
                if let Ok(opt) = field_peek.into_option() {
                    if let Some(inner) = opt.value() {
                        if let Some(v) = inner.as_str() {
                            patches.push(Patch::SetAttribute {
                                path: NodePath(dom_path.to_vec()),
                                name: field.name.to_string(),
                                value: v.to_string(),
                            });
                        }
                    } else {
                        patches.push(Patch::RemoveAttribute {
                            path: NodePath(dom_path.to_vec()),
                            name: field.name.to_string(),
                        });
                    }
                }
            } else if field.is_flattened() {
                collect_attributes_recursive(field_peek, dom_path, patches);
            }
        }
    }
    // Handle flattened maps (BTreeMap<String, String> for extra attributes)
    if let Ok(map) = peek.into_map() {
        for (k, v) in map.iter() {
            if let Some(key) = k.as_str()
                && let Some(value) = v.as_str()
            {
                patches.push(Patch::SetAttribute {
                    path: NodePath(dom_path.to_vec()),
                    name: key.to_string(),
                    value: value.to_string(),
                });
            }
        }
    }
}

/// Translate an Update operation.
fn translate_update(segments: &[PathSegment], new_value: Option<&str>) -> Option<Patch> {
    let html_shape = <Html as facet_core::Facet>::SHAPE;
    let nav = navigate_path(segments, html_shape);

    debug!(
        "translate_update: segments={segments:?}, dom_path={:?}, target={:?}, value={new_value:?}",
        nav.dom_path, nav.target
    );

    match nav.target {
        PathTarget::Text => {
            let text = new_value?.to_string();
            // Update just the specific text node at dom_path.
            // dom_path points to the text node itself (e.g., [0, 1] means element at 0, text child at 1).
            Some(Patch::SetText {
                path: NodePath(nav.dom_path),
                text,
            })
        }
        PathTarget::Attribute(name) => {
            let value = new_value?.to_string();
            Some(Patch::SetAttribute {
                path: NodePath(nav.element_dom_path),
                name,
                value,
            })
        }
        _ => None,
    }
}

/// Translate an UpdateAttribute op.
fn translate_update_attribute(
    segments: &[PathSegment],
    attr_name: &str,
    new_value: Option<&str>,
) -> Option<Patch> {
    let dom_path = to_dom_path(segments);

    match new_value {
        Some(value) => Some(Patch::SetAttribute {
            path: NodePath(dom_path),
            name: attr_name.to_string(),
            value: value.to_string(),
        }),
        None => Some(Patch::RemoveAttribute {
            path: NodePath(dom_path),
            name: attr_name.to_string(),
        }),
    }
}

/// Navigate a Peek value following path segments.
fn navigate_peek<'mem, 'facet>(
    mut peek: Peek<'mem, 'facet>,
    segments: &[PathSegment],
) -> Option<Peek<'mem, 'facet>> {
    for segment in segments {
        peek = match segment {
            PathSegment::Field(name) => {
                if let Ok(s) = peek.into_struct() {
                    // Try direct field first
                    if let Ok(fp) = s.field_by_name(name) {
                        fp
                    } else {
                        // Check flattened fields
                        find_field_in_peek_struct(s, name)?
                    }
                } else if let Ok(opt) = peek.into_option() {
                    let inner = opt.value()?;
                    if let Ok(s) = inner.into_struct() {
                        if let Ok(fp) = s.field_by_name(name) {
                            fp
                        } else {
                            find_field_in_peek_struct(s, name)?
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            PathSegment::Index(idx) => {
                if let Ok(list) = peek.into_list_like() {
                    list.get(*idx)?
                } else if let Ok(opt) = peek.into_option() {
                    // Option might contain a struct with flattened list
                    if let Some(inner) = opt.value() {
                        if let Ok(s) = inner.into_struct() {
                            // Find flattened list field and index into it
                            let mut found = None;
                            for (field, field_peek) in s.fields() {
                                if field.is_flattened()
                                    && let Ok(list) = field_peek.into_list_like()
                                {
                                    found = list.get(*idx);
                                    break;
                                }
                            }
                            found?
                        } else if let Ok(list) = inner.into_list_like() {
                            list.get(*idx)?
                        } else if *idx == 0 {
                            inner
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                } else if let Ok(s) = peek.into_struct() {
                    // Struct with flattened list - find it and index
                    let mut found = None;
                    for (field, field_peek) in s.fields() {
                        if field.is_flattened()
                            && let Ok(list) = field_peek.into_list_like()
                        {
                            found = list.get(*idx);
                            break;
                        }
                    }
                    found?
                } else if let Ok(e) = peek.into_enum() {
                    e.field(*idx).ok()??
                } else {
                    return None;
                }
            }
            PathSegment::Variant(_) => {
                // Stay at current position - enum is already at the variant
                peek
            }
            PathSegment::Key(key) => {
                if let Ok(map) = peek.into_map() {
                    let mut found = None;
                    for (k, v) in map.iter() {
                        if k.as_str() == Some(key) {
                            found = Some(v);
                            break;
                        }
                    }
                    found?
                } else {
                    return None;
                }
            }
        };
    }
    Some(peek)
}

/// Find a field in a PeekStruct, checking flattened fields recursively.
fn find_field_in_peek_struct<'mem, 'facet>(
    s: PeekStruct<'mem, 'facet>,
    name: &str,
) -> Option<Peek<'mem, 'facet>> {
    for (field, field_peek) in s.fields() {
        if field.is_flattened()
            && let Ok(inner_s) = field_peek.into_struct()
        {
            if let Ok(fp) = inner_s.field_by_name(name) {
                return Some(fp);
            }
            // Recurse into nested flattened structs
            if let Some(fp) = find_field_in_peek_struct(inner_s, name) {
                return Some(fp);
            }
        }
    }
    None
}

/// Get the element tag name from a Peek value.
///
/// For enums (like FlowContent, PhrasingContent), this returns the variant's
/// effective name (respecting `#[facet(rename = "...")]`).
/// For structs, this returns the shape's rename or the type identifier with lowerCamelCase.
fn get_element_tag(peek: Peek<'_, '_>) -> String {
    use std::borrow::Cow;

    // If it's an enum, use the active variant's name
    if let Ok(enum_peek) = peek.into_enum() {
        if let Ok(variant) = enum_peek.active_variant() {
            // Check for rename attribute on the variant
            let variant_name: Cow<'_, str> = variant
                .get_builtin_attr("rename")
                .and_then(|a| a.get_as::<&str>().copied())
                .map(Cow::Borrowed)
                .unwrap_or_else(|| to_element_name(variant.name));
            return variant_name.into_owned();
        }
    }

    // For structs, check rename attribute on shape, then fall back to type identifier
    if let Some(rename) = peek.shape().get_builtin_attr_value::<&str>("rename") {
        rename.to_string()
    } else {
        to_element_name(peek.shape().type_identifier).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_element_with_text() {
        // This tests adding a new element that has text content
        let old = "<html><body><p>First</p></body></html>";
        let new = "<html><body><p>First</p><p>Second</p></body></html>";

        let old_doc: Html = facet_html::from_str(old).unwrap();
        let new_doc: Html = facet_html::from_str(new).unwrap();
        let ops = tree_diff(&old_doc, &new_doc);
        eprintln!("EditOps for add_element_with_text:");
        for op in &ops {
            eprintln!("  {:?}", op);
        }

        let patches = diff_html(old, new).unwrap();

        eprintln!("Patches:");
        for p in &patches {
            eprintln!("  {:?}", p);
        }

        // Apply patches and verify
        let mut tree = apply::parse_html(old).unwrap();
        apply::apply_patches(&mut tree, &patches).unwrap();
        let result = tree.to_html();

        assert_eq!(result, "<body><p>First</p><p>Second</p></body>");
    }

    #[test]
    fn test_text_insert_generates_insert_text() {
        // Old: empty div, New: div with text
        let old = "<html><body><div></div></body></html>";
        let new = "<html><body><div>a</div></body></html>";

        // First, let's see what EditOps are generated
        let old_doc: Html = facet_html::from_str(old).unwrap();
        let new_doc: Html = facet_html::from_str(new).unwrap();
        let ops = tree_diff(&old_doc, &new_doc);
        eprintln!("EditOps:");
        for op in &ops {
            eprintln!("  {:?}", op);
        }

        let patches = diff_html(old, new).unwrap();

        eprintln!("Patches:");
        for p in &patches {
            eprintln!("  {:?}", p);
        }

        // Should have InsertText, not InsertElement with tag "text"
        let has_text_element = patches
            .iter()
            .any(|p| matches!(p, Patch::InsertElement { tag, .. } if tag == "text"));
        assert!(
            !has_text_element,
            "Should not have InsertElement with tag 'text', got: {patches:?}"
        );

        let has_insert_text = patches
            .iter()
            .any(|p| matches!(p, Patch::InsertText { .. }));
        assert!(
            has_insert_text,
            "Should have InsertText patch, got: {patches:?}"
        );
    }
}
