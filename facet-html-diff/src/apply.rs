//! Apply patches to HTML documents.
//!
//! For property testing: apply(A, diff(A, B)) == B

use crate::{InsertContent, NodePath, Patch};
use facet_xml_node::{Content, Element};
use std::collections::HashMap;

/// Parse an HTML string into an Element tree, returning the body.
pub fn parse_html(html: &str) -> Result<Element, String> {
    // Use facet-html to parse directly into Element
    let doc: Element = facet_html::from_str(html).map_err(|e| format!("Parse error: {e}"))?;

    // Return the body element if this is an <html> document
    if doc.tag == "html" {
        for child in &doc.children {
            if let Content::Element(e) = child {
                if e.tag == "body" {
                    return Ok(e.clone());
                }
            }
        }
    }

    // Otherwise return as-is
    Ok(doc)
}

/// Navigate within an element using a relative path and return the children vec.
/// Used for operations on nodes within detached slots.
fn navigate_to_children_in_slot<'a>(
    slot_node: &'a mut Element,
    rel_path: Option<&NodePath>,
) -> Result<&'a mut Vec<Content>, String> {
    let mut current = slot_node;
    if let Some(path) = rel_path {
        for &idx in &path.0 {
            let child = current
                .children
                .get_mut(idx)
                .ok_or_else(|| format!("path index {idx} out of bounds in slot"))?;
            current = match child {
                Content::Element(e) => e,
                Content::Text(_) => {
                    return Err("cannot navigate through text node".to_string());
                }
            };
        }
    }
    Ok(&mut current.children)
}

/// Apply a list of patches to an Element tree in order.
pub fn apply_patches(root: &mut Element, patches: &[Patch]) -> Result<(), String> {
    // Slots hold Content (either Element or Text) that was displaced during edits
    let mut slots: HashMap<u32, Content> = HashMap::new();
    for patch in patches {
        apply_patch(root, patch, &mut slots)?;
    }
    Ok(())
}

/// Apply a single patch.
fn apply_patch(
    root: &mut Element,
    patch: &Patch,
    slots: &mut HashMap<u32, Content>,
) -> Result<(), String> {
    use crate::NodeRef;

    match patch {
        Patch::InsertElement {
            parent,
            position,
            tag,
            attrs,
            children,
            detach_to_slot,
        } => {
            // Create element with its attrs and children
            let new_element = Element {
                tag: tag.clone(),
                attrs: attrs.iter().cloned().collect(),
                children: children.iter().map(insert_content_to_content).collect(),
            };
            let new_content = Content::Element(new_element);

            insert_at_position(root, slots, parent, *position, new_content, *detach_to_slot)?;
        }
        Patch::InsertText {
            parent,
            position,
            text,
            detach_to_slot,
        } => {
            let new_content = Content::Text(text.clone());
            insert_at_position(root, slots, parent, *position, new_content, *detach_to_slot)?;
        }
        Patch::Remove { node } => {
            match node {
                NodeRef::Path(path) => {
                    if path.0.is_empty() {
                        return Err("Remove: cannot remove root".to_string());
                    }
                    let parent_path = &path.0[..path.0.len() - 1];
                    let idx = path.0[path.0.len() - 1];
                    let children = root
                        .children_mut(parent_path)
                        .ok_or_else(|| format!("Remove: parent not found at {parent_path:?}"))?;
                    if idx < children.len() {
                        // Swap with placeholder instead of remove (no shifting!)
                        children[idx] = Content::Text(String::new());
                    } else {
                        return Err(format!("Remove: index {idx} out of bounds"));
                    }
                }
                NodeRef::Slot(slot, _relative_path) => {
                    // Just remove from slots - the node was already detached
                    slots.remove(slot);
                }
            }
        }
        Patch::SetText { path, text } => {
            // Path points to a specific text node (e.g., [0, 1] = element at 0, text child at 1).
            // Navigate to the parent and replace just that child.
            if path.0.is_empty() {
                return Err("SetText: cannot set text on root".to_string());
            }
            let parent_path = &path.0[..path.0.len() - 1];
            let text_idx = path.0[path.0.len() - 1];
            let children = root
                .children_mut(parent_path)
                .ok_or_else(|| format!("SetText: parent not found at {parent_path:?}"))?;
            if text_idx >= children.len() {
                return Err(format!(
                    "SetText: index {text_idx} out of bounds (len={})",
                    children.len()
                ));
            }
            children[text_idx] = Content::Text(text.clone());
        }
        Patch::SetAttribute { path, name, value } => {
            let attrs = root
                .attrs_mut(&path.0)
                .ok_or_else(|| format!("SetAttribute: node not found at {:?}", path.0))?;
            attrs.insert(name.clone(), value.clone());
        }
        Patch::RemoveAttribute { path, name } => {
            let attrs = root
                .attrs_mut(&path.0)
                .ok_or_else(|| format!("RemoveAttribute: node not found at {:?}", path.0))?;
            attrs.remove(name);
        }
        Patch::Move {
            from,
            to,
            detach_to_slot,
        } => {
            debug!(?from, ?to, ?detach_to_slot, "apply Move");
            debug!(
                slots_before = ?slots.keys().collect::<Vec<_>>(),
                "apply Move slots state"
            );

            // Get the content to move (either from a path or from a slot)
            let content = match from {
                NodeRef::Path(from_path) => {
                    if from_path.0.is_empty() {
                        return Err("Move: cannot move root".to_string());
                    }
                    let from_parent_path = &from_path.0[..from_path.0.len() - 1];
                    let from_idx = from_path.0[from_path.0.len() - 1];
                    let from_children = root.children_mut(from_parent_path).ok_or_else(|| {
                        format!("Move: source parent not found at {from_parent_path:?}")
                    })?;
                    if from_idx >= from_children.len() {
                        return Err(format!("Move: source index {from_idx} out of bounds"));
                    }
                    // Swap with placeholder instead of remove (no shifting!)
                    std::mem::replace(&mut from_children[from_idx], Content::Text(String::new()))
                }
                NodeRef::Slot(slot, _relative_path) => slots
                    .remove(slot)
                    .ok_or_else(|| format!("Move: slot {slot} not found"))?,
            };

            // Check if we need to detach the occupant at the target position
            if to.0.is_empty() {
                return Err("Move: cannot move to root".to_string());
            }
            let to_parent_path = &to.0[..to.0.len() - 1];
            let to_idx = to.0[to.0.len() - 1];

            if let Some(slot) = detach_to_slot {
                let to_children = root.children_mut(to_parent_path).ok_or_else(|| {
                    format!("Move: target parent not found at {to_parent_path:?}")
                })?;
                debug!(
                    to_idx,
                    to_children_len = to_children.len(),
                    "Move detach check"
                );
                if to_idx < to_children.len() {
                    let occupant =
                        std::mem::replace(&mut to_children[to_idx], Content::Text(String::new()));
                    debug!(slot, ?occupant, "Move detach: inserting occupant into slot");
                    slots.insert(*slot, occupant);
                } else {
                    debug!(
                        to_idx,
                        to_children_len = to_children.len(),
                        slot,
                        "Move detach: to_idx >= len, NOT inserting to slot"
                    );
                }
            }

            // Place the content at the target location
            let to_children = root
                .children_mut(to_parent_path)
                .ok_or_else(|| format!("Move: target parent not found at {to_parent_path:?}"))?;
            // Grow the array with empty text placeholders if needed
            while to_children.len() <= to_idx {
                to_children.push(Content::Text(String::new()));
            }
            to_children[to_idx] = content;
        }
    }
    Ok(())
}

/// Helper to insert content at a position, handling displacement to slots.
fn insert_at_position(
    root: &mut Element,
    slots: &mut HashMap<u32, Content>,
    parent: &crate::NodeRef,
    position: usize,
    new_content: Content,
    detach_to_slot: Option<u32>,
) -> Result<(), String> {
    use crate::NodeRef;

    match parent {
        NodeRef::Path(path) => {
            let children = root
                .children_mut(&path.0)
                .ok_or_else(|| format!("Insert: parent not found at {:?}", path.0))?;

            // In Chawathe semantics, Insert does NOT shift - it places at position
            // and whatever was there gets displaced (detached to a slot).
            if let Some(slot) = detach_to_slot {
                if position < children.len() {
                    let occupant =
                        std::mem::replace(&mut children[position], Content::Text(String::new()));
                    slots.insert(slot, occupant);
                }
            }

            // Grow the array with empty text placeholders if needed
            while children.len() <= position {
                children.push(Content::Text(String::new()));
            }
            children[position] = new_content;
        }
        NodeRef::Slot(parent_slot, relative_path) => {
            // Parent is in a slot - inserting into a detached subtree
            let slot_elem = match slots.get_mut(parent_slot) {
                Some(Content::Element(e)) => e,
                Some(Content::Text(_)) => {
                    return Err(format!(
                        "Insert: slot {parent_slot} contains text, not an element"
                    ));
                }
                None => return Err(format!("Insert: slot {parent_slot} not found")),
            };

            // First handle displacement if needed
            if let Some(slot) = detach_to_slot {
                let children = navigate_to_children_in_slot(slot_elem, relative_path.as_ref())?;
                if position < children.len() {
                    let occupant =
                        std::mem::replace(&mut children[position], Content::Text(String::new()));
                    slots.insert(slot, occupant);
                }
            }

            // Re-get the slot element (borrow was released)
            let slot_elem = match slots.get_mut(parent_slot) {
                Some(Content::Element(e)) => e,
                _ => return Err(format!("Insert: slot {parent_slot} not found")),
            };
            let children = navigate_to_children_in_slot(slot_elem, relative_path.as_ref())?;

            // Grow the array with empty text placeholders if needed
            while children.len() <= position {
                children.push(Content::Text(String::new()));
            }
            children[position] = new_content;
        }
    }
    Ok(())
}

/// Convert InsertContent to Content (facet_xml_node).
fn insert_content_to_content(ic: &InsertContent) -> Content {
    match ic {
        InsertContent::Element {
            tag,
            attrs,
            children,
        } => Content::Element(Element {
            tag: tag.clone(),
            attrs: attrs.iter().cloned().collect(),
            children: children.iter().map(insert_content_to_content).collect(),
        }),
        InsertContent::Text(s) => Content::Text(s.clone()),
    }
}
