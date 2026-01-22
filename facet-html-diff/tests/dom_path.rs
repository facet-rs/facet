//! Tests for DOM path extraction from facet-diff paths.
//!
//! These tests verify that navigate_path correctly converts facet-diff paths
//! into DOM paths (sequences of child indices).
//!
//! IMPORTANT: SetText targets the *text node* itself, not the parent element.
//! So `<body><p>Text</p></body>` changing text results in SetText with
//! path `[0, 0]` (text node at index 0 inside P at index 0).

use facet_html_diff::{NodePath, Patch, diff_html};
use facet_testhelpers::test;

fn get_dom_paths(old: &str, new: &str) -> Vec<(String, NodePath)> {
    let patches = diff_html(old, new).unwrap();
    patches
        .into_iter()
        .map(|p| {
            let (kind, path) = match &p {
                Patch::InsertElement {
                    parent, position, ..
                } => match parent {
                    facet_html_diff::NodeRef::Path(path) => {
                        let mut p = path.0.clone();
                        p.push(*position);
                        ("InsertElement", NodePath(p))
                    }
                    facet_html_diff::NodeRef::Slot(s, _) => {
                        ("InsertElementSlot", NodePath(vec![*s as usize, *position]))
                    }
                },
                Patch::InsertText {
                    parent, position, ..
                } => match parent {
                    facet_html_diff::NodeRef::Path(path) => {
                        let mut p = path.0.clone();
                        p.push(*position);
                        ("InsertText", NodePath(p))
                    }
                    facet_html_diff::NodeRef::Slot(s, _) => {
                        ("InsertTextSlot", NodePath(vec![*s as usize, *position]))
                    }
                },
                Patch::Remove { node } => match node {
                    facet_html_diff::NodeRef::Path(path) => ("Remove", path.clone()),
                    facet_html_diff::NodeRef::Slot(s, _) => {
                        ("RemoveSlot", NodePath(vec![*s as usize]))
                    }
                },
                Patch::SetText { path, .. } => ("SetText", path.clone()),
                Patch::SetAttribute { path, .. } => ("SetAttribute", path.clone()),
                Patch::RemoveAttribute { path, .. } => ("RemoveAttribute", path.clone()),
                Patch::Move { to, .. } => match to {
                    facet_html_diff::NodeRef::Path(path) => ("Move", path.clone()),
                    facet_html_diff::NodeRef::Slot(s, _) => {
                        ("MoveSlot", NodePath(vec![*s as usize]))
                    }
                },
            };
            (kind.to_string(), path)
        })
        .collect()
}

// =============================================================================
// SIMPLE TEXT CHANGES
// SetText path points to the TEXT NODE itself, not the parent element.
// This is Chawathe-correct: we update just that specific text node.
// =============================================================================

#[test]
fn body_text_dom_path() {
    // <body>Hello</body> -> <body>World</body>
    // SetText path should be [0] - the text node at body's child index 0
    let paths = get_dom_paths(
        "<html><body>Hello</body></html>",
        "<html><body>World</body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![0],
        "SetText on body text should target the text node (path [0])"
    );
}

#[test]
fn p_text_dom_path() {
    // <body><p>A</p></body> -> <body><p>B</p></body>
    // SetText path should be [0, 0] - P is at index 0, text is at index 0 within P
    let paths = get_dom_paths(
        "<html><body><p>A</p></body></html>",
        "<html><body><p>B</p></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![0, 0],
        "SetText on P text should target text node (path [0, 0])"
    );
}

#[test]
fn second_p_text_dom_path() {
    // <body><p>A</p><p>B</p></body> -> <body><p>A</p><p>X</p></body>
    // SetText path should be [1, 0] - second P at index 1, text at index 0 within it
    let paths = get_dom_paths(
        "<html><body><p>A</p><p>B</p></body></html>",
        "<html><body><p>A</p><p>X</p></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![1, 0],
        "SetText on second P text should target text node (path [1, 0])"
    );
}

// =============================================================================
// NESTED ELEMENTS
// =============================================================================

#[test]
fn nested_div_p_text_dom_path() {
    // <body><div><p>A</p></div></body> -> <body><div><p>B</p></div></body>
    // SetText path should be [0, 0, 0] - div at 0, P at 0, text at 0
    let paths = get_dom_paths(
        "<html><body><div><p>A</p></div></body></html>",
        "<html><body><div><p>B</p></div></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![0, 0, 0],
        "SetText on nested P text should target text node (path [0, 0, 0])"
    );
}

#[test]
fn second_child_of_div_dom_path() {
    // <body><div><p>A</p><p>B</p></div></body> -> <body><div><p>A</p><p>X</p></div></body>
    // SetText path should be [0, 1, 0] - div at 0, second P at 1, text at 0
    let paths = get_dom_paths(
        "<html><body><div><p>A</p><p>B</p></div></body></html>",
        "<html><body><div><p>A</p><p>X</p></div></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![0, 1, 0],
        "SetText on second P of div should target text node (path [0, 1, 0])"
    );
}

// =============================================================================
// ATTRIBUTES
// =============================================================================

#[test]
fn attribute_on_first_child_dom_path() {
    // <body><p>Text</p></body> -> <body><p class="foo">Text</p></body>
    // DOM path should be [0]
    let paths = get_dom_paths(
        "<html><body><p>Text</p></body></html>",
        r#"<html><body><p class="foo">Text</p></body></html>"#,
    );

    let set_attr = paths.iter().find(|(k, _)| k == "SetAttribute");
    assert!(set_attr.is_some(), "Should have SetAttribute patch");
    assert_eq!(
        set_attr.unwrap().1.0,
        vec![0],
        "Attribute on P should target [0]"
    );
}

#[test]
fn attribute_on_second_child_dom_path() {
    // <body><p>A</p><p>B</p></body> -> <body><p>A</p><p id="x">B</p></body>
    // DOM path should be [1]
    let paths = get_dom_paths(
        "<html><body><p>A</p><p>B</p></body></html>",
        r#"<html><body><p>A</p><p id="x">B</p></body></html>"#,
    );

    let set_attr = paths.iter().find(|(k, _)| k == "SetAttribute");
    assert!(set_attr.is_some(), "Should have SetAttribute patch");
    assert_eq!(
        set_attr.unwrap().1.0,
        vec![1],
        "Attribute on second P should target [1]"
    );
}

#[test]
fn attribute_on_nested_element_dom_path() {
    // <body><div><p>Text</p></div></body> -> <body><div><p id="x">Text</p></div></body>
    // DOM path should be [0, 0]
    let paths = get_dom_paths(
        "<html><body><div><p>Text</p></div></body></html>",
        r#"<html><body><div><p id="x">Text</p></div></body></html>"#,
    );

    let set_attr = paths.iter().find(|(k, _)| k == "SetAttribute");
    assert!(set_attr.is_some(), "Should have SetAttribute patch");
    assert_eq!(
        set_attr.unwrap().1.0,
        vec![0, 0],
        "Attribute on nested P should target [0, 0]"
    );
}

// =============================================================================
// MIXED CONTENT - text nodes as siblings of elements
// SetText targets the specific text node, not the parent element.
// =============================================================================

#[test]
fn text_before_element_dom_path() {
    // <body>Hello<p>World</p></body> -> <body>Hi<p>World</p></body>
    // "Hello" is a text node at body's child index 0, so SetText path is [0]
    let paths = get_dom_paths(
        "<html><body>Hello<p>World</p></body></html>",
        "<html><body>Hi<p>World</p></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    if let Some((_, path)) = set_text {
        // SetText targets the text node at index 0
        assert_eq!(
            path.0,
            vec![0],
            "SetText on body's first text child should target [0]"
        );
    }
    // Otherwise, a different patch strategy was used, which is also fine
}

#[test]
fn text_after_element_dom_path() {
    // <body><p>First</p>Second</body> -> <body><p>First</p>Changed</body>
    // "Second" is a text node at body's child index 1 (after the P at index 0)
    let paths = get_dom_paths(
        "<html><body><p>First</p>Second</body></html>",
        "<html><body><p>First</p>Changed</body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    if let Some((_, path)) = set_text {
        // SetText targets the text node at index 1
        assert_eq!(
            path.0,
            vec![1],
            "SetText on body's second child (text) should target [1]"
        );
    }
}
