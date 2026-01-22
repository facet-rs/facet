//! HTML diffing with DOM patch generation.
//!
//! This crate translates facet-diff EditOps (from GumTree/Chawathe) into DOM Patches
//! that can be applied to update an HTML document incrementally.

#[macro_use]
mod tracing_macros;

pub mod apply;

// Re-export for convenience
pub use apply::{apply_patches, parse_html};
pub use facet_xml_node::Element;

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

/// Content that can be inserted as part of a new subtree.
/// This is used for the `children` field in `InsertElement` when inserting
/// a completely new subtree that has no match in the old document.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum InsertContent {
    /// An element with its tag, attributes, and nested children
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<InsertContent>,
    },
    /// A text node
    Text(String),
}

/// A single property change within an UpdateProps operation.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct PropChange {
    /// The property name (field name)
    pub name: String,
    /// The new value (None if property is being removed)
    pub value: Option<String>,
}

/// Operations to transform the DOM.
///
/// These follow Chawathe semantics: Insert/Move operations do NOT shift siblings.
/// Instead, they displace whatever is at the target position to a slot.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum Patch {
    /// Insert an element at position within parent.
    /// If `detach_to_slot` is Some, the node at that position is detached and stored in that slot.
    ///
    /// `attrs` and `children` contain the initial content:
    /// - Empty if this is a "shell" insert (content will be added via separate ops)
    /// - Populated if this is a new subtree with no matches in the old document
    InsertElement {
        parent: NodeRef,
        position: usize,
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<InsertContent>,
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
    /// If `detach_to_slot` is Some, the node at the target is detached and stored in that slot.
    Move {
        from: NodeRef,
        to: NodeRef,
        detach_to_slot: Option<u32>,
    },

    /// Update multiple properties on an element.
    /// The `_text` property is handled specially: it updates the text content of the element.
    /// Other properties are applied as HTML attributes.
    UpdateProps {
        path: NodePath,
        changes: Vec<PropChange>,
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

    let patches =
        translate_to_patches(&edit_ops, &new_doc).map_err(|e| format!("Translation error: {e}"))?;

    debug!(count = patches.len(), "Translated patches");
    for _patch in &patches {
        debug!(?_patch, "patch");
    }

    Ok(patches)
}

/// Translate facet-diff EditOps into DOM Patches.
///
/// Returns an error if any operation fails to translate.
pub fn translate_to_patches(
    edit_ops: &[EditOp],
    new_doc: &Html,
) -> Result<Vec<Patch>, TranslateError> {
    let mut patches = Vec::new();
    for op in edit_ops {
        let op_patches = translate_op(op, new_doc)?;
        patches.extend(op_patches);
    }
    Ok(patches)
}

/// What does this path point to?
#[derive(Debug, Clone, PartialEq)]
pub enum PathTarget {
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
    /// What the path points to
    target: PathTarget,
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
                if let Some((list_elem_shape, _is_children)) = find_flattened_list(current_shape) {
                    current_shape = list_elem_shape;
                    if is_last {
                        target = PathTarget::Element;
                    }
                } else if let Def::List(list_def) = &current_shape.def {
                    // Direct list access
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

    PathNavigation { target }
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

/// Error type for translation failures.
#[derive(Debug)]
pub enum TranslateError {
    /// Insert operation could not be translated
    InsertFailed {
        parent: facet_diff::NodeRef,
        position: usize,
        label_path: Vec<PathSegment>,
        target: PathTarget,
        reason: String,
    },
    /// UpdateAttribute operation could not be translated
    UpdateAttributeFailed {
        path: Vec<PathSegment>,
        attr_name: String,
        reason: String,
    },
    /// Unexpected operation type
    UnexpectedOp { op: String },
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranslateError::InsertFailed {
                parent,
                position,
                label_path,
                target,
                reason,
            } => write!(
                f,
                "Insert failed: parent={parent:?}, position={position}, label_path={label_path:?}, target={target:?}, reason={reason}"
            ),
            TranslateError::UpdateAttributeFailed {
                path,
                attr_name,
                reason,
            } => write!(
                f,
                "UpdateAttribute failed: path={path:?}, attr_name={attr_name}, reason={reason}"
            ),
            TranslateError::UnexpectedOp { op } => write!(f, "Unexpected op: {op}"),
        }
    }
}

impl std::error::Error for TranslateError {}

/// Translate a single EditOp to DOM Patches.
fn translate_op(op: &EditOp, new_doc: &Html) -> Result<Vec<Patch>, TranslateError> {
    trace!("translate_op: op={op:?}");
    match op {
        EditOp::Insert {
            parent,
            position,
            label_path,
            value,
            detach_to_slot,
            ..
        } => {
            let patch = translate_insert(
                parent,
                *position,
                &label_path.0,
                value.as_deref(),
                *detach_to_slot,
                new_doc,
            )?;
            Ok(vec![patch])
        }
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
            Ok(vec![Patch::Remove { node: node_ref }])
        }
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
            let to_ref = match to {
                facet_diff::NodeRef::Path(p) => NodeRef::Path(NodePath(extract_dom_indices(&p.0))),
                facet_diff::NodeRef::Slot(s, rel_path) => NodeRef::Slot(
                    *s,
                    rel_path
                        .as_ref()
                        .map(|p| NodePath(extract_dom_indices(&p.0))),
                ),
            };
            Ok(vec![Patch::Move {
                from: from_ref,
                to: to_ref,
                detach_to_slot: *detach_to_slot,
            }])
        }
        EditOp::UpdateAttributes { path, changes } => {
            // Convert directly to UpdateProps - _text handling happens during apply
            let dom_path = NodePath(extract_dom_indices(&path.0));
            let prop_changes: Vec<PropChange> = changes
                .iter()
                .map(|c| PropChange {
                    name: c.attr_name.to_string(),
                    value: c.new_value.clone(),
                })
                .collect();
            Ok(vec![Patch::UpdateProps {
                path: dom_path,
                changes: prop_changes,
            }])
        }
        #[allow(unreachable_patterns)]
        _ => Err(TranslateError::UnexpectedOp {
            op: format!("{op:?}"),
        }),
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
) -> Result<Patch, TranslateError> {
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

    // Clone target for use in error messages (before we match and move out of it)
    let target_for_error = nav.target.clone();
    let make_error = |reason: &str| TranslateError::InsertFailed {
        parent: parent.clone(),
        position,
        label_path: label_segments.to_vec(),
        target: target_for_error.clone(),
        reason: reason.to_string(),
    };

    match nav.target {
        PathTarget::Element => {
            // Navigate to the actual node to determine its type
            let peek = Peek::new(new_doc);
            let node_peek = navigate_peek(peek, label_segments)
                .ok_or_else(|| make_error("could not navigate to node in new_doc"))?;

            // Check if this is actually a text variant in the enum
            // (navigate_path may return Element for Index into enum lists where it can't know the variant)
            if let Ok(enum_peek) = node_peek.into_enum()
                && let Ok(variant) = enum_peek.active_variant()
                && variant.is_text()
            {
                // This is a text variant - extract the text value and emit InsertText
                let text = enum_peek
                    .field(0)
                    .ok()
                    .flatten()
                    .and_then(|p| p.as_str().map(|s| s.to_string()))
                    .unwrap_or_default();
                return Ok(Patch::InsertText {
                    parent: parent_ref,
                    position,
                    text,
                    detach_to_slot,
                });
            }

            // Not a text variant - insert element with its attrs and children
            let peek2 = Peek::new(new_doc);
            let node_peek2 = navigate_peek(peek2, label_segments)
                .ok_or_else(|| make_error("could not navigate to node in new_doc (second pass)"))?;
            let tag = get_element_tag(node_peek2);
            let (attrs, children) = extract_attrs_and_children(node_peek2);

            Ok(Patch::InsertElement {
                parent: parent_ref,
                position,
                tag,
                attrs,
                children,
                detach_to_slot,
            })
        }
        PathTarget::Attribute(name) => {
            // Attributes go on the parent element
            let element_path = match &parent_ref {
                NodeRef::Path(p) => p.clone(),
                NodeRef::Slot(..) => {
                    return Err(make_error("cannot set attribute on slot directly"));
                }
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
                        });
                        return match attr_value {
                            Some(v) => Ok(Patch::SetAttribute {
                                path: element_path,
                                name,
                                value: v,
                            }),
                            None => Err(make_error("attribute value is None")),
                        };
                    } else {
                        return Ok(Patch::RemoveAttribute {
                            path: element_path,
                            name,
                        });
                    }
                } else if let Some(s) = attr_peek.as_str() {
                    return Ok(Patch::SetAttribute {
                        path: element_path,
                        name,
                        value: s.to_string(),
                    });
                }
            }

            match value {
                Some(v) => Ok(Patch::SetAttribute {
                    path: element_path,
                    name: name.clone(),
                    value: v.to_string(),
                }),
                None => Err(make_error("attribute value is None and could not navigate")),
            }
        }
        PathTarget::Text => {
            // Insert a text node at the given position
            let text = value
                .ok_or_else(|| make_error("text value is None"))?
                .to_string();
            Ok(Patch::InsertText {
                parent: parent_ref,
                position,
                text,
                detach_to_slot,
            })
        }
        PathTarget::FlattenedAttributeStruct => {
            let element_path = match &parent_ref {
                NodeRef::Path(p) => p.0.clone(),
                NodeRef::Slot(..) => {
                    return Err(make_error(
                        "cannot handle flattened attribute struct on slot",
                    ));
                }
            };
            let patches = sync_attrs_from_new_doc(&element_path, label_segments, new_doc);
            patches
                .into_iter()
                .next()
                .ok_or_else(|| make_error("flattened attribute struct produced no patches"))
        }
        PathTarget::FlattenedChildrenList => {
            // This is intentionally a no-op: individual children will be inserted separately by cinereus.
            // However, we shouldn't receive Insert ops for flattened children lists - they're structural.
            Err(make_error(
                "received Insert for FlattenedChildrenList - this should not happen",
            ))
        }
        PathTarget::Other => Err(make_error("PathTarget::Other is not supported for Insert")),
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

    // If it's an enum, get the inner struct and check for a tag field
    if let Ok(enum_peek) = peek.into_enum()
        && let Ok(variant) = enum_peek.active_variant()
    {
        // First, check if the inner struct has a tag field (for Custom* elements)
        if let Some(inner) = enum_peek.field(0).ok().flatten()
            && let Some(tag) = get_tag_from_struct(inner)
        {
            return tag;
        }
        // Otherwise use the variant's rename or name
        let variant_name: Cow<'_, str> = variant
            .get_builtin_attr("rename")
            .and_then(|a| a.get_as::<&str>().copied())
            .map(Cow::Borrowed)
            .unwrap_or_else(|| to_element_name(variant.name));
        return variant_name.into_owned();
    }

    // For structs, first check for a tag field
    if let Some(tag) = get_tag_from_struct(peek) {
        return tag;
    }

    // Fall back to rename attribute on shape, then type identifier
    if let Some(rename) = peek.shape().get_builtin_attr_value::<&str>("rename") {
        rename.to_string()
    } else {
        to_element_name(peek.shape().type_identifier).into_owned()
    }
}

/// Check for a field with the `html::tag` or `xml::tag` attribute and return its value.
fn get_tag_from_struct(peek: Peek<'_, '_>) -> Option<String> {
    if let Ok(s) = peek.into_struct() {
        for (field, field_peek) in s.fields() {
            if field.is_tag() {
                return field_peek.as_str().map(|s| s.to_string());
            }
        }
    }
    None
}

/// Extract attributes of an element (no children - cinereus handles children separately).
/// Cinereus always inserts "shells" - the children are populated by subsequent INSERT/MOVE ops.
fn extract_attrs_and_children(peek: Peek<'_, '_>) -> (Vec<(String, String)>, Vec<InsertContent>) {
    let mut attrs = Vec::new();
    let mut children = Vec::new();

    // For enums, get the inner struct
    let struct_peek = if let Ok(enum_peek) = peek.into_enum() {
        // Get the inner value (field 0 of the enum variant)
        enum_peek.field(0).ok().flatten()
    } else {
        Some(peek)
    };

    let Some(struct_peek) = struct_peek else {
        return (attrs, children);
    };

    // Try to get fields from a struct
    if let Ok(s) = struct_peek.into_struct() {
        extract_attrs_and_children_from_struct(s, &mut attrs, &mut children);
    }

    (attrs, children)
}

/// Extract attributes and children from a struct.
fn extract_attrs_and_children_from_struct(
    s: PeekStruct<'_, '_>,
    attrs: &mut Vec<(String, String)>,
    children: &mut Vec<InsertContent>,
) {
    for (field, field_peek) in s.fields() {
        // Handle attributes
        if field.is_attribute() {
            let attr_name = field
                .rename
                .map(|s| s.to_string())
                .unwrap_or_else(|| to_element_name(field.name).into_owned());

            if let Ok(opt) = field_peek.into_option() {
                if let Some(inner) = opt.value()
                    && let Some(val) = inner.as_str()
                {
                    attrs.push((attr_name, val.to_string()));
                }
            } else if let Some(val) = field_peek.as_str() {
                attrs.push((attr_name, val.to_string()));
            }
            continue;
        }

        // Handle flattened structs (like GlobalAttrs) - recurse to extract nested attrs
        if field.is_flattened() {
            if let Ok(inner_struct) = field_peek.into_struct() {
                // Flattened struct - only extract attrs, no children
                extract_attrs_only(inner_struct, attrs);
            } else if let Ok(list) = field_peek.into_list_like() {
                // Flattened list (like children: Vec<FlowContent>) - extract children
                for elem in list.iter() {
                    if let Some(content) = extract_insert_content(elem) {
                        children.push(content);
                    }
                }
            }
        }
    }
}

/// Convert a Peek value to InsertContent (recursively).
fn extract_insert_content(peek: Peek<'_, '_>) -> Option<InsertContent> {
    // Check if this is an enum (like FlowContent)
    if let Ok(enum_peek) = peek.into_enum() {
        if let Ok(variant) = enum_peek.active_variant() {
            // Check if it's a text variant
            if variant.is_text() {
                // Extract the text value
                let text = enum_peek
                    .field(0)
                    .ok()
                    .flatten()
                    .and_then(|p| p.as_str().map(|s| s.to_string()))
                    .unwrap_or_default();
                return Some(InsertContent::Text(text));
            }

            // Not a text variant - it's an element
            // Get the inner struct (field 0)
            if let Ok(Some(inner)) = enum_peek.field(0) {
                let tag = get_element_tag(inner);
                let (attrs, children) = extract_attrs_and_children(inner);
                return Some(InsertContent::Element {
                    tag,
                    attrs,
                    children,
                });
            }
        }
    }

    // Direct struct (not wrapped in enum)
    if let Ok(s) = peek.into_struct() {
        let tag = get_tag_from_struct(peek).unwrap_or_else(|| "div".to_string());
        let mut attrs = Vec::new();
        let mut children = Vec::new();
        extract_attrs_and_children_from_struct(s, &mut attrs, &mut children);
        return Some(InsertContent::Element {
            tag,
            attrs,
            children,
        });
    }

    None
}

/// Helper to extract only attributes from a struct (no children).
fn extract_attrs_only(s: PeekStruct<'_, '_>, attrs: &mut Vec<(String, String)>) {
    for (field, field_peek) in s.fields() {
        // Handle attributes
        if field.is_attribute() {
            let attr_name = field
                .rename
                .map(|s| s.to_string())
                .unwrap_or_else(|| to_element_name(field.name).into_owned());

            if let Ok(opt) = field_peek.into_option() {
                if let Some(inner) = opt.value()
                    && let Some(val) = inner.as_str()
                {
                    attrs.push((attr_name, val.to_string()));
                }
            } else if let Some(val) = field_peek.as_str() {
                attrs.push((attr_name, val.to_string()));
            }
            continue;
        }
        // Handle flattened structs (like GlobalAttrs) - recurse to extract nested attrs
        if field.is_flattened()
            && let Ok(inner_struct) = field_peek.into_struct()
        {
            extract_attrs_only(inner_struct, attrs);
        }
        // Skip flattened children lists - cinereus handles children
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_testhelpers::test;

    /// Verify InsertText is used for text nodes, not InsertElement with tag "text"
    #[test]
    fn test_text_insert_generates_insert_text() {
        let old = "<html><body><div></div></body></html>";
        let new = "<html><body><div>a</div></body></html>";

        let patches = diff_html(old, new).unwrap();

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
