use facet_html_diff::{InsertContent, NodePath, NodeRef, Patch, apply_patches, parse_html};
use facet_testhelpers::test;
use facet_xml_node::Content;

#[test]
fn test_parse_and_serialize_roundtrip() {
    let html = "<body><p>Hello</p></body>";
    let node = parse_html("<html><body><p>Hello</p></body></html>").unwrap();
    assert_eq!(node.to_html(), html);
}

#[test]
fn test_apply_set_text() {
    // Body has <p>Hello</p>, the text "Hello" is at path [0, 0] (p's first child)
    let mut node = parse_html("<html><body><p>Hello</p></body></html>").unwrap();
    apply_patches(
        &mut node,
        &[Patch::SetText {
            path: NodePath(vec![0, 0]), // path to the text node inside <p>
            text: "Goodbye".to_string(),
        }],
    )
    .unwrap();
    assert_eq!(node.to_html(), "<body><p>Goodbye</p></body>");
}

#[test]
fn test_apply_set_attribute() {
    let mut node = parse_html("<html><body><div>Content</div></body></html>").unwrap();
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
    let mut node = parse_html("<html><body><p>First</p><p>Second</p></body></html>").unwrap();
    apply_patches(
        &mut node,
        &[Patch::Remove {
            node: NodeRef::Path(NodePath(vec![1])),
        }],
    )
    .unwrap();
    assert_eq!(node.to_html(), "<body><p>First</p></body>");
}

#[test]
fn test_apply_insert_element() {
    let mut node = parse_html("<html><body><p>First</p></body></html>").unwrap();
    apply_patches(
        &mut node,
        &[Patch::InsertElement {
            parent: NodeRef::Path(NodePath(vec![])),
            position: 0,
            tag: "p".to_string(),
            attrs: vec![],
            children: vec![],
            detach_to_slot: Some(0), // Chawathe: displace First to slot 0
        }],
    )
    .unwrap();
    // After insert with displacement, First is in slot 0, only empty <p> is in tree
    assert_eq!(node.to_html(), "<body><p></p></body>");
}

#[test]
fn test_apply_insert_element_no_displacement() {
    // Insert at end (no occupant) - no displacement needed
    let mut node = parse_html("<html><body><p>First</p></body></html>").unwrap();
    apply_patches(
        &mut node,
        &[Patch::InsertElement {
            parent: NodeRef::Path(NodePath(vec![])),
            position: 1, // Insert at index 1 (past last element)
            tag: "p".to_string(),
            attrs: vec![],
            children: vec![],
            detach_to_slot: None,
        }],
    )
    .unwrap();
    assert_eq!(node.to_html(), "<body><p>First</p><p></p></body>");
}

#[test]
fn test_apply_insert_element_with_children() {
    // Insert element with text content
    let mut node = parse_html("<html><body><p>First</p></body></html>").unwrap();
    apply_patches(
        &mut node,
        &[Patch::InsertElement {
            parent: NodeRef::Path(NodePath(vec![])),
            position: 1,
            tag: "p".to_string(),
            attrs: vec![],
            children: vec![InsertContent::Text("Second".to_string())],
            detach_to_slot: None,
        }],
    )
    .unwrap();
    assert_eq!(node.to_html(), "<body><p>First</p><p>Second</p></body>");
}

#[test]
fn test_apply_insert_element_with_attrs() {
    // Insert element with attribute
    let mut node = parse_html("<html><body><p>First</p></body></html>").unwrap();
    apply_patches(
        &mut node,
        &[Patch::InsertElement {
            parent: NodeRef::Path(NodePath(vec![])),
            position: 1,
            tag: "p".to_string(),
            attrs: vec![("class".to_string(), "highlight".to_string())],
            children: vec![InsertContent::Text("Second".to_string())],
            detach_to_slot: None,
        }],
    )
    .unwrap();
    assert_eq!(
        node.to_html(),
        "<body><p>First</p><p class=\"highlight\">Second</p></body>"
    );
}

#[test]
fn test_apply_insert_text() {
    let mut node = parse_html("<html><body><p>First</p></body></html>").unwrap();
    apply_patches(
        &mut node,
        &[Patch::InsertText {
            parent: NodeRef::Path(NodePath(vec![])),
            position: 1,
            text: "Hello".to_string(),
            detach_to_slot: None,
        }],
    )
    .unwrap();
    assert_eq!(node.to_html(), "<body><p>First</p>Hello</body>");
}

#[test]
fn test_parse_invalid_html_nesting() {
    // This HTML has a <div> inside a <strong> which is invalid per HTML spec
    // (block element inside inline element), but facet_xml_node::Element
    // should handle it fine since it doesn't enforce content models.
    let html = r#"<html><body><strong><div>nested div</div></strong></body></html>"#;
    let node = parse_html(html).unwrap();

    // The strong should contain the div
    assert_eq!(node.tag, "body");
    let strong = node.children.first().unwrap();
    if let Content::Element(strong_elem) = strong {
        assert_eq!(strong_elem.tag, "strong");
        let div = strong_elem.children.first().unwrap();
        if let Content::Element(div_elem) = div {
            assert_eq!(div_elem.tag, "div");
            assert_eq!(div_elem.text_content(), "nested div");
        } else {
            panic!("Expected element, got text");
        }
    } else {
        panic!("Expected element, got text");
    }
}
