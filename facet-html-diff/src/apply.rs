//! Apply patches to HTML documents.
//!
//! For property testing: apply(A, diff(A, B)) == B

use crate::Patch;
use std::collections::HashMap;

/// A mutable DOM node for patch application.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element {
        tag: String,
        attrs: HashMap<String, String>,
        children: Vec<Node>,
    },
    Text(String),
}

impl Node {
    /// Parse an HTML string into a Node tree.
    pub fn parse(html: &str) -> Result<Node, String> {
        // Use facet-html to parse, then convert to our mutable tree
        let doc: facet_html_dom::Html =
            facet_html::from_str(html).map_err(|e| format!("Parse error: {e}"))?;

        // Convert the body to our Node structure
        match &doc.body {
            Some(body) => Ok(convert_body(body)),
            None => Ok(Node::Element {
                tag: "body".to_string(),
                attrs: HashMap::new(),
                children: vec![],
            }),
        }
    }

    /// Serialize back to HTML.
    pub fn to_html(&self) -> String {
        let mut out = String::new();
        self.write_html(&mut out);
        out
    }

    fn write_html(&self, out: &mut String) {
        match self {
            Node::Text(s) => out.push_str(s),
            Node::Element {
                tag,
                attrs,
                children,
            } => {
                out.push('<');
                out.push_str(tag);
                // Sort attrs for deterministic output
                let mut attr_list: Vec<_> = attrs.iter().collect();
                attr_list.sort_by_key(|(k, _)| *k);
                for (k, v) in attr_list {
                    out.push(' ');
                    out.push_str(k);
                    out.push_str("=\"");
                    out.push_str(&html_escape(v));
                    out.push('"');
                }
                out.push('>');
                for child in children {
                    child.write_html(out);
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
        }
    }

    /// Get a mutable reference to a child node by path.
    fn get_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        if path.is_empty() {
            return Some(self);
        }

        match self {
            Node::Element { children, .. } => {
                let idx = path[0];
                if idx < children.len() {
                    children[idx].get_mut(&path[1..])
                } else {
                    None
                }
            }
            Node::Text(_) => None,
        }
    }

    /// Get the children vec of a node at path.
    fn children_mut(&mut self, path: &[usize]) -> Option<&mut Vec<Node>> {
        let node = self.get_mut(path)?;
        match node {
            Node::Element { children, .. } => Some(children),
            Node::Text(_) => None,
        }
    }

    /// Get the attrs of a node at path.
    fn attrs_mut(&mut self, path: &[usize]) -> Option<&mut HashMap<String, String>> {
        let node = self.get_mut(path)?;
        match node {
            Node::Element { attrs, .. } => Some(attrs),
            Node::Text(_) => None,
        }
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Convert facet-html-dom Body to our Node.
fn convert_body(body: &facet_html_dom::Body) -> Node {
    let mut children = Vec::new();
    for child in &body.children {
        if let Some(node) = convert_flow_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "body".to_string(),
        attrs: HashMap::new(),
        children,
    }
}

/// Convert FlowContent to Node.
fn convert_flow_content(content: &facet_html_dom::FlowContent) -> Option<Node> {
    use facet_html_dom::FlowContent::*;
    match content {
        Text(s) => Some(Node::Text(s.clone())),
        Div(div) => Some(convert_div(div)),
        P(p) => Some(convert_p(p)),
        Span(span) => Some(convert_span(span)),
        A(a) => Some(convert_a(a)),
        Ul(ul) => Some(convert_ul(ul)),
        Ol(ol) => Some(convert_ol(ol)),
        H1(h) => Some(convert_h1(h)),
        H2(h) => Some(convert_h2(h)),
        H3(h) => Some(convert_h3(h)),
        H4(h) => Some(convert_h4(h)),
        H5(h) => Some(convert_h5(h)),
        H6(h) => Some(convert_h6(h)),
        Pre(pre) => Some(convert_pre(pre)),
        Code(code) => Some(convert_code(code)),
        Em(em) => Some(convert_em(em)),
        Strong(strong) => Some(convert_strong(strong)),
        Img(img) => Some(convert_img(img)),
        Blockquote(bq) => Some(convert_blockquote(bq)),
        Hr(hr) => Some(Node::Element {
            tag: "hr".to_string(),
            attrs: convert_attrs(&hr.attrs),
            children: vec![],
        }),
        Br(br) => Some(Node::Element {
            tag: "br".to_string(),
            attrs: convert_attrs(&br.attrs),
            children: vec![],
        }),
        _ => None, // Skip unknown elements for now
    }
}

/// Convert PhrasingContent to Node.
fn convert_phrasing_content(content: &facet_html_dom::PhrasingContent) -> Option<Node> {
    use facet_html_dom::PhrasingContent::*;
    match content {
        Text(s) => Some(Node::Text(s.clone())),
        Span(span) => Some(convert_span(span)),
        A(a) => Some(convert_a(a)),
        Code(code) => Some(convert_code(code)),
        Em(em) => Some(convert_em(em)),
        Strong(strong) => Some(convert_strong(strong)),
        Img(img) => Some(convert_img(img)),
        Br(br) => Some(Node::Element {
            tag: "br".to_string(),
            attrs: convert_attrs(&br.attrs),
            children: vec![],
        }),
        _ => None,
    }
}

fn convert_div(div: &facet_html_dom::Div) -> Node {
    let mut children = Vec::new();
    for child in &div.children {
        if let Some(node) = convert_flow_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "div".to_string(),
        attrs: convert_attrs(&div.attrs),
        children,
    }
}

fn convert_p(p: &facet_html_dom::P) -> Node {
    let mut children = Vec::new();
    for child in &p.children {
        if let Some(node) = convert_phrasing_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "p".to_string(),
        attrs: convert_attrs(&p.attrs),
        children,
    }
}

fn convert_span(span: &facet_html_dom::Span) -> Node {
    let mut children = Vec::new();
    for child in &span.children {
        if let Some(node) = convert_phrasing_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "span".to_string(),
        attrs: convert_attrs(&span.attrs),
        children,
    }
}

fn convert_a(a: &facet_html_dom::A) -> Node {
    let mut children = Vec::new();
    for child in &a.children {
        if let Some(node) = convert_phrasing_content(child) {
            children.push(node);
        }
    }
    let mut attrs = convert_attrs(&a.attrs);
    if let Some(href) = &a.href {
        attrs.insert("href".to_string(), href.clone());
    }
    Node::Element {
        tag: "a".to_string(),
        attrs,
        children,
    }
}

fn convert_ul(ul: &facet_html_dom::Ul) -> Node {
    let mut children = Vec::new();
    for li in &ul.li {
        children.push(convert_li(li));
    }
    Node::Element {
        tag: "ul".to_string(),
        attrs: convert_attrs(&ul.attrs),
        children,
    }
}

fn convert_ol(ol: &facet_html_dom::Ol) -> Node {
    let mut children = Vec::new();
    for li in &ol.li {
        children.push(convert_li(li));
    }
    Node::Element {
        tag: "ol".to_string(),
        attrs: convert_attrs(&ol.attrs),
        children,
    }
}

fn convert_li(li: &facet_html_dom::Li) -> Node {
    let mut children = Vec::new();
    for child in &li.children {
        if let Some(node) = convert_flow_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "li".to_string(),
        attrs: convert_attrs(&li.attrs),
        children,
    }
}

macro_rules! heading_converter {
    ($name:ident, $tag:expr, $ty:ident) => {
        fn $name(h: &facet_html_dom::$ty) -> Node {
            let mut children = Vec::new();
            for child in &h.children {
                if let Some(node) = convert_phrasing_content(child) {
                    children.push(node);
                }
            }
            Node::Element {
                tag: $tag.to_string(),
                attrs: convert_attrs(&h.attrs),
                children,
            }
        }
    };
}

heading_converter!(convert_h1, "h1", H1);
heading_converter!(convert_h2, "h2", H2);
heading_converter!(convert_h3, "h3", H3);
heading_converter!(convert_h4, "h4", H4);
heading_converter!(convert_h5, "h5", H5);
heading_converter!(convert_h6, "h6", H6);

fn convert_pre(pre: &facet_html_dom::Pre) -> Node {
    let mut children = Vec::new();
    for child in &pre.children {
        if let Some(node) = convert_phrasing_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "pre".to_string(),
        attrs: convert_attrs(&pre.attrs),
        children,
    }
}

fn convert_code(code: &facet_html_dom::Code) -> Node {
    let mut children = Vec::new();
    for child in &code.children {
        if let Some(node) = convert_phrasing_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "code".to_string(),
        attrs: convert_attrs(&code.attrs),
        children,
    }
}

fn convert_em(em: &facet_html_dom::Em) -> Node {
    let mut children = Vec::new();
    for child in &em.children {
        if let Some(node) = convert_phrasing_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "em".to_string(),
        attrs: convert_attrs(&em.attrs),
        children,
    }
}

fn convert_strong(strong: &facet_html_dom::Strong) -> Node {
    let mut children = Vec::new();
    for child in &strong.children {
        if let Some(node) = convert_phrasing_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "strong".to_string(),
        attrs: convert_attrs(&strong.attrs),
        children,
    }
}

fn convert_img(img: &facet_html_dom::Img) -> Node {
    let mut attrs = convert_attrs(&img.attrs);
    if let Some(src) = &img.src {
        attrs.insert("src".to_string(), src.clone());
    }
    if let Some(alt) = &img.alt {
        attrs.insert("alt".to_string(), alt.clone());
    }
    Node::Element {
        tag: "img".to_string(),
        attrs,
        children: vec![],
    }
}

fn convert_blockquote(bq: &facet_html_dom::Blockquote) -> Node {
    let mut children = Vec::new();
    for child in &bq.children {
        if let Some(node) = convert_flow_content(child) {
            children.push(node);
        }
    }
    Node::Element {
        tag: "blockquote".to_string(),
        attrs: convert_attrs(&bq.attrs),
        children,
    }
}

fn convert_attrs(attrs: &facet_html_dom::GlobalAttrs) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(class) = &attrs.class {
        map.insert("class".to_string(), class.clone());
    }
    if let Some(id) = &attrs.id {
        map.insert("id".to_string(), id.clone());
    }
    if let Some(style) = &attrs.style {
        map.insert("style".to_string(), style.clone());
    }
    if let Some(title) = &attrs.tooltip {
        map.insert("title".to_string(), title.clone());
    }
    map
}

/// Apply a list of patches to a Node tree in order.
pub fn apply_patches(root: &mut Node, patches: &[Patch]) -> Result<(), String> {
    let mut slots: HashMap<u32, Node> = HashMap::new();
    for patch in patches {
        apply_patch(root, patch, &mut slots)?;
    }
    Ok(())
}

/// Apply a single patch.
fn apply_patch(
    root: &mut Node,
    patch: &Patch,
    slots: &mut HashMap<u32, Node>,
) -> Result<(), String> {
    match patch {
        Patch::Replace { path, html } => {
            let new_node = parse_html_fragment(html)?;
            if path.0.is_empty() {
                *root = new_node;
            } else {
                let parent_path = &path.0[..path.0.len() - 1];
                let idx = path.0[path.0.len() - 1];
                let children = root
                    .children_mut(parent_path)
                    .ok_or_else(|| format!("Replace: parent not found at {parent_path:?}"))?;
                if idx < children.len() {
                    children[idx] = new_node;
                } else {
                    return Err(format!("Replace: index {idx} out of bounds"));
                }
            }
        }
        Patch::InsertAt {
            parent,
            position,
            html,
            detach_to_slot,
        } => {
            use crate::NodeRef;
            let new_node = parse_html_fragment(html)?;

            match parent {
                NodeRef::Path(path) => {
                    let children = root
                        .children_mut(&path.0)
                        .ok_or_else(|| format!("InsertAt: parent not found at {:?}", path.0))?;

                    // In Chawathe semantics, Insert does NOT shift - it places at position
                    // and whatever was there gets displaced (detached to a slot).
                    if let Some(slot) = detach_to_slot {
                        if *position < children.len() {
                            let occupant = std::mem::replace(
                                &mut children[*position],
                                Node::Text(String::new()),
                            );
                            slots.insert(*slot, occupant);
                        }
                    }

                    // Grow the array with empty text placeholders if needed
                    while children.len() <= *position {
                        children.push(Node::Text(String::new()));
                    }
                    children[*position] = new_node;
                }
                NodeRef::Slot(parent_slot) => {
                    // Parent is in a slot - inserting into a detached subtree
                    // First handle displacement if needed
                    if let Some(slot) = detach_to_slot {
                        let slot_node = slots
                            .get_mut(parent_slot)
                            .ok_or_else(|| format!("InsertAt: slot {parent_slot} not found"))?;
                        let children = match slot_node {
                            Node::Element { children, .. } => children,
                            Node::Text(_) => {
                                return Err("InsertAt: cannot insert into text node".to_string());
                            }
                        };
                        if *position < children.len() {
                            let occupant = std::mem::replace(
                                &mut children[*position],
                                Node::Text(String::new()),
                            );
                            slots.insert(*slot, occupant);
                        }
                    }

                    // Now insert the new node
                    let slot_node = slots
                        .get_mut(parent_slot)
                        .ok_or_else(|| format!("InsertAt: slot {parent_slot} not found"))?;
                    let children = match slot_node {
                        Node::Element { children, .. } => children,
                        Node::Text(_) => {
                            return Err("InsertAt: cannot insert into text node".to_string());
                        }
                    };

                    // Grow the array with empty text placeholders if needed
                    while children.len() <= *position {
                        children.push(Node::Text(String::new()));
                    }
                    children[*position] = new_node;
                }
            }
        }
        Patch::AppendChild { path, html } => {
            let new_node = parse_html_fragment(html)?;
            let children = root
                .children_mut(&path.0)
                .ok_or_else(|| format!("AppendChild: node not found at {:?}", path.0))?;
            children.push(new_node);
        }
        Patch::InsertAfter { path, html } => {
            let new_node = parse_html_fragment(html)?;
            if path.0.is_empty() {
                return Err("InsertAfter: cannot insert after root".to_string());
            }
            let parent_path = &path.0[..path.0.len() - 1];
            let idx = path.0[path.0.len() - 1];
            let children = root
                .children_mut(parent_path)
                .ok_or_else(|| format!("InsertAfter: parent not found at {parent_path:?}"))?;
            if idx < children.len() {
                children.insert(idx + 1, new_node);
            } else {
                return Err(format!("InsertAfter: index {idx} out of bounds"));
            }
        }
        Patch::Remove { node } => {
            use crate::NodeRef;
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
                        children[idx] = Node::Text(String::new());
                    } else {
                        return Err(format!("Remove: index {idx} out of bounds"));
                    }
                }
                NodeRef::Slot(slot) => {
                    // Just remove from slots - the node was already detached
                    slots.remove(slot);
                }
            }
        }
        Patch::SetText { path, text } => {
            let node = root
                .get_mut(&path.0)
                .ok_or_else(|| format!("SetText: node not found at {:?}", path.0))?;
            match node {
                Node::Element { children, .. } => {
                    // Replace all children with a single text node
                    *children = vec![Node::Text(text.clone())];
                }
                Node::Text(s) => {
                    *s = text.clone();
                }
            }
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
            use crate::NodeRef;

            // Get the node to move (either from a path or from a slot)
            // When taking from a path, swap with placeholder (no shifting!)
            let node = match from {
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
                    std::mem::replace(&mut from_children[from_idx], Node::Text(String::new()))
                }
                NodeRef::Slot(slot) => slots
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
                // Detach the occupant by swapping with empty placeholder (no shifting!)
                let to_children = root.children_mut(to_parent_path).ok_or_else(|| {
                    format!("Move: target parent not found at {to_parent_path:?}")
                })?;
                if to_idx < to_children.len() {
                    let occupant =
                        std::mem::replace(&mut to_children[to_idx], Node::Text(String::new()));
                    slots.insert(*slot, occupant);
                }
            }

            // Place the node at the target location
            let to_children = root
                .children_mut(to_parent_path)
                .ok_or_else(|| format!("Move: target parent not found at {to_parent_path:?}"))?;
            // Grow the array with empty text placeholders if needed
            while to_children.len() <= to_idx {
                to_children.push(Node::Text(String::new()));
            }
            // Overwrite the placeholder at the target position
            to_children[to_idx] = node;
        }
    }
    Ok(())
}

/// Parse an HTML fragment into a Node.
fn parse_html_fragment(html: &str) -> Result<Node, String> {
    // Check if the fragment is a body element itself
    if html.trim().starts_with("<body") {
        // Parse as full document
        let full = format!("<html>{html}</html>");
        let doc: facet_html_dom::Html =
            facet_html::from_str(&full).map_err(|e| format!("Fragment parse error: {e}"))?;

        // Convert body to our Node
        if let Some(body) = &doc.body {
            return Ok(convert_body(body));
        }
        return Err("No body in fragment".to_string());
    }

    // Wrap in a minimal document to parse
    let full = format!("<html><body>{html}</body></html>");
    let doc: facet_html_dom::Html =
        facet_html::from_str(&full).map_err(|e| format!("Fragment parse error: {e}"))?;

    // Return the first child of body
    if let Some(body) = &doc.body
        && let Some(first) = body.children.first()
    {
        return convert_flow_content(first).ok_or_else(|| "Could not convert fragment".to_string());
    }
    // Might be just text
    Ok(Node::Text(html.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodePath;

    #[test]
    fn test_parse_and_serialize_roundtrip() {
        let html = "<body><p>Hello</p></body>";
        let node = Node::parse("<html><body><p>Hello</p></body></html>").unwrap();
        assert_eq!(node.to_html(), html);
    }

    #[test]
    fn test_apply_set_text() {
        let mut node = Node::parse("<html><body><p>Hello</p></body></html>").unwrap();
        apply_patches(
            &mut node,
            &[Patch::SetText {
                path: NodePath(vec![0]),
                text: "Goodbye".to_string(),
            }],
        )
        .unwrap();
        assert_eq!(node.to_html(), "<body><p>Goodbye</p></body>");
    }

    #[test]
    fn test_apply_set_attribute() {
        let mut node = Node::parse("<html><body><div>Content</div></body></html>").unwrap();
        apply_patches(
            &mut node,
            &[Patch::SetAttribute {
                path: NodePath(vec![0]),
                name: "class".to_string(),
                value: "highlight".to_string(),
            }],
        )
        .unwrap();
        assert_eq!(
            node.to_html(),
            "<body><div class=\"highlight\">Content</div></body>"
        );
    }

    #[test]
    fn test_apply_remove() {
        let mut node = Node::parse("<html><body><p>First</p><p>Second</p></body></html>").unwrap();
        apply_patches(
            &mut node,
            &[Patch::Remove {
                node: crate::NodeRef::Path(NodePath(vec![1])),
            }],
        )
        .unwrap();
        assert_eq!(node.to_html(), "<body><p>First</p></body>");
    }

    #[test]
    fn test_apply_insert_at() {
        let mut node = Node::parse("<html><body><p>First</p></body></html>").unwrap();
        apply_patches(
            &mut node,
            &[Patch::InsertAt {
                parent: crate::NodeRef::Path(NodePath(vec![])),
                position: 0,
                html: "<p>Zero</p>".to_string(),
                detach_to_slot: Some(0), // Chawathe: displace First to slot 0
            }],
        )
        .unwrap();
        // After insert with displacement, First is in slot 0, only Zero is in tree
        assert_eq!(node.to_html(), "<body><p>Zero</p></body>");
    }

    #[test]
    fn test_apply_insert_at_no_displacement() {
        // Insert at end (no occupant) - no displacement needed
        let mut node = Node::parse("<html><body><p>First</p></body></html>").unwrap();
        apply_patches(
            &mut node,
            &[Patch::InsertAt {
                parent: crate::NodeRef::Path(NodePath(vec![])),
                position: 1, // Insert at index 1 (past last element)
                html: "<p>Second</p>".to_string(),
                detach_to_slot: None,
            }],
        )
        .unwrap();
        assert_eq!(node.to_html(), "<body><p>First</p><p>Second</p></body>");
    }

    #[test]
    fn test_apply_append_child() {
        let mut node = Node::parse("<html><body><p>First</p></body></html>").unwrap();
        apply_patches(
            &mut node,
            &[Patch::AppendChild {
                path: NodePath(vec![]),
                html: "<p>Second</p>".to_string(),
            }],
        )
        .unwrap();
        assert_eq!(node.to_html(), "<body><p>First</p><p>Second</p></body>");
    }
}
