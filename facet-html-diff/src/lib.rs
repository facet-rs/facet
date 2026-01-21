//! HTML diffing with DOM patch generation.
//!
//! This crate translates facet-diff EditOps (from GumTree/Chawathe) into DOM Patches
//! that can be applied to update an HTML document incrementally.
//!
//! # Example
//!
//! ```
//! use facet_html_diff::{diff_html, Patch, NodePath};
//!
//! let old = "<html><body><p>Hello</p></body></html>";
//! let new = "<html><body><p>Goodbye</p></body></html>";
//!
//! let patches = diff_html(old, new).unwrap();
//! // patches will contain SetText operations to update "Hello" -> "Goodbye"
//! ```

#[macro_use]
mod macros;

pub mod apply;

use facet_diff::{EditOp, PathSegment, tree_diff};
use facet_html_dom::*;
use facet_reflect::{HasFields, Peek};

/// A path to a node in the DOM tree.
///
/// e.g., `[0, 2, 1]` means: body's child 0, then child 2, then child 1
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct NodePath(pub Vec<usize>);

/// Operations to transform the DOM.
///
/// These patches describe atomic operations that can be applied to an HTML DOM
/// to transform it from one state to another.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum Patch {
    /// Replace node at path with new HTML
    Replace { path: NodePath, html: String },

    /// Replace all children of node at path with new HTML (innerHTML replacement).
    /// The node itself and its attributes are preserved.
    ReplaceInnerHtml { path: NodePath, html: String },

    /// Insert HTML before the node at path
    InsertBefore { path: NodePath, html: String },

    /// Insert HTML after the node at path
    InsertAfter { path: NodePath, html: String },

    /// Append HTML as last child of node at path
    AppendChild { path: NodePath, html: String },

    /// Remove the node at path
    Remove { path: NodePath },

    /// Update text content of node at path
    SetText { path: NodePath, text: String },

    /// Set attribute on node at path
    SetAttribute {
        path: NodePath,
        name: String,
        value: String,
    },

    /// Remove attribute from node at path
    RemoveAttribute { path: NodePath, name: String },

    /// Move a node from one location to another.
    /// The node is removed from `from` and inserted at `to`.
    Move { from: NodePath, to: NodePath },
}

/// Diff two HTML documents and return DOM patches.
pub fn diff_html(old_html: &str, new_html: &str) -> Result<Vec<Patch>, String> {
    let old_doc: Html =
        facet_html::from_str(old_html).map_err(|e| format!("Failed to parse old HTML: {e}"))?;
    let new_doc: Html =
        facet_html::from_str(new_html).map_err(|e| format!("Failed to parse new HTML: {e}"))?;

    let edit_ops = tree_diff(&old_doc, &new_doc);

    debug!(count = edit_ops.len(), "Edit ops from facet-diff");
    for op in &edit_ops {
        debug!(?op, "edit op");
    }

    let patches = translate_to_patches(&edit_ops, &new_doc);

    debug!(count = patches.len(), "Translated patches");
    for patch in &patches {
        debug!(?patch, "patch");
    }

    Ok(patches)
}

/// Diff with debug tracing of raw edit ops.
#[cfg(not(feature = "tracing"))]
pub fn diff_html_debug(old_html: &str, new_html: &str) -> Result<Vec<Patch>, String> {
    diff_html(old_html, new_html)
}

/// Translate facet-diff EditOps into DOM Patches.
///
/// Simple approach: translate each op directly into a patch. No filtering, no deduplication.
/// The ops from cinereus describe how to get from A to B - just apply them.
pub fn translate_to_patches(edit_ops: &[EditOp], new_doc: &Html) -> Vec<Patch> {
    edit_ops
        .iter()
        .flat_map(|op| translate_op_multi(op, new_doc))
        .collect()
}

/// Translate a single EditOp to a DOM Patch.
fn translate_op(op: &EditOp, new_doc: &Html) -> Option<Patch> {
    trace!("translate_op: op={op:?}");
    match op {
        EditOp::Insert {
            path,
            label_path,
            value,
            ..
        } => translate_insert(&path.0, &label_path.0, value.as_deref(), new_doc),
        EditOp::Delete { path, .. } => translate_delete(&path.0, new_doc),
        EditOp::Update {
            path, new_value, ..
        } => translate_update(&path.0, new_value.as_deref()),
        EditOp::Move {
            old_path, new_path, ..
        } => translate_move(&old_path.0, &new_path.0, new_doc),
        EditOp::UpdateAttribute {
            path,
            attr_name,
            old_value: _,
            new_value,
        } => translate_update_attribute(&path.0, attr_name, new_value.as_deref()),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// Convert a facet path to a DOM path.
///
/// Facet path: [Field("body"), Field("children"), Index(1), Variant("Div"), Index(0), Field("children"), Index(2)]
/// DOM path: [1, 2] (just the indices from children arrays)
fn to_dom_path(segments: &[PathSegment]) -> Vec<usize> {
    let mut dom_path = Vec::new();
    let mut i = 0;

    while i < segments.len() {
        // Look for Field("children") followed by Index(n)
        if let PathSegment::Field(name) = &segments[i]
            && (name == "children" || name == "li")
            && let Some(PathSegment::Index(idx)) = segments.get(i + 1)
        {
            dom_path.push(*idx);
            i += 2;
            continue;
        }
        i += 1;
    }

    dom_path
}

/// What does this path point to?
#[derive(Debug, Clone, PartialEq)]
enum PathTarget {
    /// Text node content
    Text,
    /// An attribute (with name)
    Attribute(String),
    /// An element node
    Element,
    /// Something else (children array, etc.)
    Other,
}

/// Analyze what a path targets.
fn path_target(segments: &[PathSegment]) -> PathTarget {
    let last = segments.last();
    let second_last = segments.len().checked_sub(2).and_then(|i| segments.get(i));

    // Check for text: ends with Variant("Text") or Variant("_text")
    if let Some(PathSegment::Variant(name)) = last
        && (name == "Text" || name == "_text")
    {
        return PathTarget::Text;
    }
    // Also check Index(0) after Variant("Text")
    if let (Some(PathSegment::Variant(name)), Some(PathSegment::Index(0))) = (second_last, last)
        && (name == "Text" || name == "_text")
    {
        return PathTarget::Text;
    }

    // Check for attribute: Field("attrs") followed by Field(attr_name)
    if let (Some(PathSegment::Field(parent)), Some(PathSegment::Field(attr))) = (second_last, last)
        && parent == "attrs"
    {
        return PathTarget::Attribute(attr.to_string());
    }

    // Check for direct attributes (href, src, etc. flattened from attrs)
    if let Some(PathSegment::Field(name)) = last {
        if is_direct_attribute(name) {
            return PathTarget::Attribute(name.to_string());
        }
        // If it ends with "children" or other structural fields, it's Other
        if name == "children" || name == "attrs" || name == "body" || name == "head" {
            return PathTarget::Other;
        }
    }

    // Check for element: ends with Variant(ElementName) followed by Index(0)
    if let (Some(PathSegment::Variant(name)), Some(PathSegment::Index(0))) = (second_last, last)
        && name != "Text"
        && name != "_text"
    {
        return PathTarget::Element;
    }

    // If it ends with Index after children, it's an element in a children array
    if let Some(PathSegment::Index(_)) = last
        && let Some(PathSegment::Field(name)) = second_last
        && (name == "children" || name == "li")
    {
        return PathTarget::Element;
    }

    PathTarget::Other
}

fn is_direct_attribute(name: &str) -> bool {
    matches!(
        name,
        "href"
            | "src"
            | "alt"
            | "target"
            | "rel"
            | "download"
            | "type"
            | "action"
            | "method"
            | "name"
            | "value"
            | "placeholder"
            | "class"
            | "id"
            | "style"
    )
}

/// Translate an Insert operation.
/// `segments` is the shadow tree path (for DOM operations)
/// `label_segments` is the tree_b path (for navigating new_doc)
fn translate_insert(
    segments: &[PathSegment],
    label_segments: &[PathSegment],
    value: Option<&str>,
    new_doc: &Html,
) -> Option<Patch> {
    let target = path_target(segments);
    let dom_path = to_dom_path(segments);

    trace!(
        "translate_insert: segments={segments:?}, label_segments={label_segments:?}, dom_path={dom_path:?}, target={target:?}, value={value:?}"
    );

    match target {
        PathTarget::Element => {
            // For elements, navigate using label_segments (tree_b coordinates)
            let peek = Peek::new(new_doc);
            let node_peek = navigate_peek(peek, label_segments)?;
            let html = serialize_to_html(node_peek)?;

            if dom_path.is_empty() {
                // Inserting at body level - append to body
                Some(Patch::AppendChild {
                    path: NodePath(vec![]),
                    html,
                })
            } else {
                // Insert before the node at this path
                Some(Patch::InsertBefore {
                    path: NodePath(dom_path),
                    html,
                })
            }
        }
        PathTarget::Attribute(name) => {
            // DOM path is the element, not the attribute
            let elem_path = to_dom_path(&segments[..segments.len().saturating_sub(2)]);

            // Check the actual value in new_doc using label_segments (tree_b coordinates)
            let peek = Peek::new(new_doc);
            if let Some(attr_peek) = navigate_peek(peek, label_segments)
                && let Ok(opt) = attr_peek.into_option()
            {
                if opt.value().is_some() {
                    // Attribute has a value in new_doc - use the EditOp value or lookup
                    let attr_value = value.map(|s| s.to_string()).or_else(|| {
                        let p2 = Peek::new(new_doc);
                        navigate_peek(p2, label_segments)
                            .and_then(|peek| peek.into_option().ok())
                            .and_then(|opt| opt.value())
                            .and_then(|inner| inner.as_str().map(|s| s.to_string()))
                    })?;
                    return Some(Patch::SetAttribute {
                        path: NodePath(elem_path),
                        name,
                        value: attr_value,
                    });
                } else {
                    // Attribute is None in new_doc - emit RemoveAttribute
                    return Some(Patch::RemoveAttribute {
                        path: NodePath(elem_path),
                        name,
                    });
                }
            }

            // Fallback: use the EditOp value if available
            value.map(|attr_value| Patch::SetAttribute {
                path: NodePath(elem_path),
                name,
                value: attr_value.to_string(),
            })
        }
        PathTarget::Text => {
            // Use the value directly from the EditOp
            let text = value?.to_string();
            // DOM path is the parent element
            let parent_path = if dom_path.is_empty() {
                vec![]
            } else {
                dom_path[..dom_path.len() - 1].to_vec()
            };
            Some(Patch::SetText {
                path: NodePath(parent_path),
                text,
            })
        }
        PathTarget::Other => {
            // Check if this is an Insert at a structural field - replace inner HTML
            if let Some(PathSegment::Field(name)) = segments.last() {
                // Handle body field - this means the body content changed
                if name == "body" {
                    trace!("handling body insert");
                    let peek = Peek::new(new_doc);
                    // Use label_segments (tree_b coordinates) to navigate new_doc
                    if let Some(body_peek) = navigate_peek(peek, label_segments) {
                        // Unwrap Option<Body>
                        if let Ok(opt) = body_peek.into_option()
                            && let Some(inner) = opt.value()
                            && let Ok(s) = inner.into_struct()
                            && let Ok(children) = s.field_by_name("children")
                            && let Ok(list) = children.into_list()
                        {
                            let mut children_html = String::new();
                            for child in list.iter() {
                                if let Some(html) = serialize_to_html(child) {
                                    children_html.push_str(&html);
                                }
                            }
                            trace!("body children_html={children_html:?}");
                            return Some(Patch::ReplaceInnerHtml {
                                path: NodePath(vec![]),
                                html: children_html,
                            });
                        }
                    }
                }
                if name == "children" {
                    trace!("handling children insert");
                    // Get the parent element's children and serialize them
                    // Use label_segments (tree_b coordinates) to navigate new_doc
                    let parent_label_segments =
                        &label_segments[..label_segments.len().saturating_sub(1)];
                    trace!("parent_label_segments={parent_label_segments:?}");
                    let peek = Peek::new(new_doc);
                    if let Some(parent_peek) = navigate_peek(peek, parent_label_segments) {
                        trace!("navigated to parent, shape={:?}", parent_peek.shape());
                        // Handle Option<Body> or similar by unwrapping
                        let struct_peek = if let Ok(opt) = parent_peek.into_option() {
                            opt.value()
                        } else {
                            let peek2 = Peek::new(new_doc);
                            navigate_peek(peek2, parent_label_segments)
                        };
                        if let Some(struct_peek) = struct_peek {
                            if let Ok(s) = struct_peek.into_struct() {
                                trace!("parent is struct");
                                if let Ok(children) = s.field_by_name("children") {
                                    trace!("got children field, shape={:?}", children.shape());
                                    if let Ok(list) = children.into_list() {
                                        trace!("children is list with {} items", list.len());
                                        let mut children_html = String::new();
                                        for child in list.iter() {
                                            if let Some(html) = serialize_to_html(child) {
                                                children_html.push_str(&html);
                                            }
                                        }
                                        trace!("serialized children_html={children_html:?}");
                                        return Some(Patch::ReplaceInnerHtml {
                                            path: NodePath(dom_path),
                                            html: children_html,
                                        });
                                    } else {
                                        trace!("children is NOT a list");
                                    }
                                } else {
                                    trace!("no children field in struct");
                                }
                            } else {
                                trace!("unwrapped parent is NOT a struct");
                            }
                        } else {
                            trace!("parent Option is None or couldn't unwrap");
                        }
                    } else {
                        trace!("failed to navigate to parent - path may be in wrong coordinates");
                        // If we can't navigate using the path (likely shadow tree coordinates after Move),
                        // emit ReplaceInnerHtml with empty content. This handles cases where the element
                        // was moved and its children need to be cleared.
                        return Some(Patch::ReplaceInnerHtml {
                            path: NodePath(dom_path),
                            html: String::new(),
                        });
                    }
                }
            }
            None
        }
    }
}

/// Translate a Delete operation.
///
/// Takes new_doc to verify attribute deletions - we only delete if the
/// attribute doesn't exist in new_doc (to handle matching algorithm quirks).
fn translate_delete(segments: &[PathSegment], new_doc: &Html) -> Option<Patch> {
    let target = path_target(segments);
    let dom_path = to_dom_path(segments);

    trace!("translate_delete: segments={segments:?}");
    trace!("  dom_path={dom_path:?}, target={target:?}");

    match target {
        PathTarget::Element => {
            if dom_path.is_empty() {
                None // Can't delete body
            } else {
                Some(Patch::Remove {
                    path: NodePath(dom_path),
                })
            }
        }
        PathTarget::Attribute(name) => {
            // Check if this attribute exists in new_doc - if so, don't delete it.
            // This handles cases where the matching algorithm matched values across
            // different attribute fields (e.g., old.id matched to new.class).
            let elem_path = to_dom_path(&segments[..segments.len().saturating_sub(2)]);

            // Navigate to the attribute in new_doc to check if it exists
            let peek = Peek::new(new_doc);
            if let Some(attr_peek) = navigate_peek(peek, segments) {
                // Try to get the Option value
                if let Ok(opt) = attr_peek.into_option()
                    && opt.value().is_some()
                {
                    // Attribute exists in new_doc, don't delete it
                    trace!("  skipping delete - attribute exists in new_doc");
                    return None;
                }
            }

            Some(Patch::RemoveAttribute {
                path: NodePath(elem_path),
                name,
            })
        }
        PathTarget::Text => {
            // Text deletion - handled by SetText on parent
            None
        }
        PathTarget::Other => {
            // Check if this is deleting a "children" field
            if let Some(PathSegment::Field(name)) = segments.last()
                && name == "children"
            {
                // Same pattern as attributes: check if new_doc has content at this path.
                // If it does, skip - the Insert op will handle setting the content.
                let peek = Peek::new(new_doc);
                if let Some(children_peek) = navigate_peek(peek, segments)
                    && let Ok(list) = children_peek.into_list()
                    && !list.is_empty()
                {
                    // new_doc has children - Insert will handle it
                    trace!("  skipping delete - children exist in new_doc");
                    return None;
                }

                // Element is empty in new_doc - clear it
                return Some(Patch::ReplaceInnerHtml {
                    path: NodePath(dom_path),
                    html: String::new(),
                });
            }
            None
        }
    }
}

/// Translate a Move operation.
///
/// For attribute field moves (e.g., value moving from id to class field),
/// we translate to SetAttribute or RemoveAttribute based on the new value.
fn translate_move(
    old_segments: &[PathSegment],
    new_segments: &[PathSegment],
    new_doc: &Html,
) -> Option<Patch> {
    let old_target = path_target(old_segments);
    let new_target = path_target(new_segments);

    trace!("translate_move: old={old_segments:?} -> new={new_segments:?}");
    trace!("  old_target={old_target:?}, new_target={new_target:?}");

    // For attribute moves, look up the value at new_path and emit SetAttribute or RemoveAttribute
    if let PathTarget::Attribute(attr_name) = new_target {
        let elem_path = to_dom_path(&new_segments[..new_segments.len().saturating_sub(2)]);

        // Navigate to the attribute value in new_doc
        let peek = Peek::new(new_doc);
        if let Some(attr_peek) = navigate_peek(peek, new_segments) {
            // Try to get the Option value
            if let Ok(opt) = attr_peek.into_option() {
                if let Some(inner) = opt.value() {
                    // Has a value - SetAttribute
                    if let Some(s) = inner.as_str() {
                        return Some(Patch::SetAttribute {
                            path: NodePath(elem_path),
                            name: attr_name,
                            value: s.to_string(),
                        });
                    }
                } else {
                    // None - RemoveAttribute
                    return Some(Patch::RemoveAttribute {
                        path: NodePath(elem_path),
                        name: attr_name,
                    });
                }
            }
        }
        return None;
    }

    // For element moves, translate to DOM Move
    if old_target == PathTarget::Element && new_target == PathTarget::Element {
        let from = to_dom_path(old_segments);
        let to = to_dom_path(new_segments);
        return Some(Patch::Move {
            from: NodePath(from),
            to: NodePath(to),
        });
    }

    // Other structural moves don't translate directly
    None
}

/// Translate facet-diff EditOps into DOM Patches, returning multiple patches if needed.
///
/// Some ops (like attrs struct inserts/moves/deletes) need to generate multiple patches.
fn translate_op_multi(op: &EditOp, new_doc: &Html) -> Vec<Patch> {
    // Check for attrs struct inserts that need special handling
    if let EditOp::Insert { path, .. } = op
        && let Some(PathSegment::Field(name)) = path.0.last()
        && name == "attrs"
    {
        // Inserting an attrs struct - sync all attributes from new_doc
        return sync_attrs_from_new_doc(&path.0, &path.0, new_doc);
    }

    // For Move of attrs structs, skip entirely.
    // Moves indicate the attrs are being reused/matched between elements.
    // The actual attribute changes are handled by Insert and Delete ops.
    if let EditOp::Move { new_path, .. } = op
        && let Some(PathSegment::Field(name)) = new_path.0.last()
        && name == "attrs"
    {
        return vec![];
    }

    // Check for attrs struct deletes that need special handling
    if let EditOp::Delete { path, .. } = op
        && let Some(PathSegment::Field(name)) = path.0.last()
        && name == "attrs"
    {
        // Deleting an attrs struct - sync all attributes from new_doc
        // The element at this position should have whatever attrs new_doc specifies
        return sync_attrs_from_new_doc(&path.0, &path.0, new_doc);
    }

    // For all other ops, delegate to the single-patch translation
    translate_op(op, new_doc).into_iter().collect()
}

/// Sync all attributes from new_doc for an element.
///
/// - `old_attrs_path`: Path to attrs in OLD tree (for DOM position)
/// - `new_attrs_path`: Path to attrs in NEW tree (for looking up values)
fn sync_attrs_from_new_doc(
    old_attrs_path: &[PathSegment],
    new_attrs_path: &[PathSegment],
    new_doc: &Html,
) -> Vec<Patch> {
    use crate::HasFields;

    let mut patches = Vec::new();

    // Compute DOM path from OLD attrs path (where element currently is)
    let elem_path = &old_attrs_path[..old_attrs_path.len().saturating_sub(1)];
    let dom_path = to_dom_path(elem_path);

    // Navigate to the attrs struct in new_doc using NEW path (for values)
    let peek = Peek::new(new_doc);
    if let Some(attrs_peek) = navigate_peek(peek, new_attrs_path)
        && let Ok(s) = attrs_peek.into_struct()
    {
        // Iterate over ALL fields in the attrs struct
        for (field, field_peek) in s.fields() {
            let attr_name = field.name;
            if let Ok(opt) = field_peek.into_option() {
                if let Some(inner) = opt.value() {
                    // Has a value - SetAttribute
                    if let Some(v) = inner.as_str() {
                        patches.push(Patch::SetAttribute {
                            path: NodePath(dom_path.clone()),
                            name: attr_name.to_string(),
                            value: v.to_string(),
                        });
                    }
                } else {
                    // None - RemoveAttribute
                    patches.push(Patch::RemoveAttribute {
                        path: NodePath(dom_path.clone()),
                        name: attr_name.to_string(),
                    });
                }
            }
        }
    }

    patches
}

/// Translate an Update operation.
///
/// Uses the new_value directly from the EditOp.
fn translate_update(segments: &[PathSegment], new_value: Option<&str>) -> Option<Patch> {
    let target = path_target(segments);
    let dom_path = to_dom_path(segments);

    trace!(
        "translate_update: segments={segments:?}, dom_path={dom_path:?}, target={target:?}, new_value={new_value:?}"
    );

    match target {
        PathTarget::Text => {
            // Use the value directly from the EditOp
            let text = new_value?.to_string();
            // SetText on the node at dom_path - if it's a text node, update it directly;
            // if it's an element, replace its children with the text
            Some(Patch::SetText {
                path: NodePath(dom_path),
                text,
            })
        }
        PathTarget::Attribute(name) => {
            // Use the value directly from the EditOp
            let value = new_value?.to_string();
            let elem_path = to_dom_path(&segments[..segments.len().saturating_sub(2)]);
            Some(Patch::SetAttribute {
                path: NodePath(elem_path),
                name,
                value,
            })
        }
        PathTarget::Element | PathTarget::Other => {
            // Structural updates don't translate to DOM patches directly
            // The leaf changes (text, attributes) are what matter
            None
        }
    }
}

/// Translate an UpdateAttribute op directly to a DOM patch.
/// The path points to the element node, and attr_name specifies which attribute.
fn translate_update_attribute(
    segments: &[PathSegment],
    attr_name: &str,
    new_value: Option<&str>,
) -> Option<Patch> {
    let dom_path = to_dom_path(segments);

    trace!(
        "translate_update_attribute: segments={segments:?}, dom_path={dom_path:?}, attr={attr_name}, value={new_value:?}"
    );

    match new_value {
        Some(value) => {
            // Set or update the attribute
            Some(Patch::SetAttribute {
                path: NodePath(dom_path),
                name: attr_name.to_string(),
                value: value.to_string(),
            })
        }
        None => {
            // Remove the attribute
            Some(Patch::RemoveAttribute {
                path: NodePath(dom_path),
                name: attr_name.to_string(),
            })
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
                    s.field_by_name(name).ok()?
                } else if let Ok(opt) = peek.into_option() {
                    let inner = opt.value()?;
                    if let Ok(s) = inner.into_struct() {
                        s.field_by_name(name).ok()?
                    } else {
                        return None;
                    }
                } else {
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
                        return None;
                    }
                } else if let Ok(e) = peek.into_enum() {
                    e.field(*idx).ok()??
                } else {
                    return None;
                }
            }
            PathSegment::Variant(_) => {
                // Enum variant - value already IS that variant, continue
                peek
            }
            PathSegment::Key(key) => {
                if let Ok(map) = peek.into_map() {
                    for (k, v) in map.iter() {
                        if let Some(s) = k.as_str()
                            && s == key
                        {
                            return Some(v);
                        }
                    }
                    return None;
                } else {
                    return None;
                }
            }
        };
    }
    Some(peek)
}

/// Serialize a Peek value to HTML.
fn serialize_to_html(peek: Peek<'_, '_>) -> Option<String> {
    let mut serializer = facet_html::HtmlSerializer::new();
    facet_dom::serialize(&mut serializer, peek).ok()?;
    let bytes = serializer.finish();
    String::from_utf8(bytes).ok()
}
