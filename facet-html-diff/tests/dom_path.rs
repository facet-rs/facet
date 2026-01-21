//! Tests for DOM path extraction from facet-diff paths.
//!
//! These tests verify that navigate_path correctly converts facet-diff paths
//! into DOM paths (sequences of child indices).

use facet_html_diff::{NodePath, Patch, diff_html};
use facet_testhelpers::test;

fn get_dom_paths(old: &str, new: &str) -> Vec<(String, NodePath)> {
    let patches = diff_html(old, new).unwrap();
    patches
        .into_iter()
        .map(|p| {
            let (kind, path) = match &p {
                Patch::Replace { path, .. } => ("Replace", path.clone()),
                Patch::ReplaceInnerHtml { path, .. } => ("ReplaceInnerHtml", path.clone()),
                Patch::InsertBefore { path, .. } => ("InsertBefore", path.clone()),
                Patch::InsertAfter { path, .. } => ("InsertAfter", path.clone()),
                Patch::AppendChild { path, .. } => ("AppendChild", path.clone()),
                Patch::Remove { path } => ("Remove", path.clone()),
                Patch::SetText { path, .. } => ("SetText", path.clone()),
                Patch::SetAttribute { path, .. } => ("SetAttribute", path.clone()),
                Patch::RemoveAttribute { path, .. } => ("RemoveAttribute", path.clone()),
                Patch::Move { to, .. } => ("Move", to.clone()),
            };
            (kind.to_string(), path)
        })
        .collect()
}

// =============================================================================
// SIMPLE TEXT CHANGES
// =============================================================================

#[test]
fn body_text_dom_path() {
    // <body>Hello</body> -> <body>World</body>
    // DOM path should be [0] - first child of body
    let paths = get_dom_paths(
        "<html><body>Hello</body></html>",
        "<html><body>World</body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(set_text.unwrap().1.0, vec![0], "Text should be at [0]");
}

#[test]
fn p_text_dom_path() {
    // <body><p>A</p></body> -> <body><p>B</p></body>
    // DOM path should be [0, 0] - first child of body, first child of P
    let paths = get_dom_paths(
        "<html><body><p>A</p></body></html>",
        "<html><body><p>B</p></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![0, 0],
        "Text inside P should be at [0, 0]"
    );
}

#[test]
fn second_p_text_dom_path() {
    // <body><p>A</p><p>B</p></body> -> <body><p>A</p><p>X</p></body>
    // DOM path should be [1, 0]
    let paths = get_dom_paths(
        "<html><body><p>A</p><p>B</p></body></html>",
        "<html><body><p>A</p><p>X</p></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![1, 0],
        "Text inside second P should be at [1, 0]"
    );
}

// =============================================================================
// NESTED ELEMENTS
// =============================================================================

#[test]
fn nested_div_p_text_dom_path() {
    // <body><div><p>A</p></div></body> -> <body><div><p>B</p></div></body>
    // DOM path should be [0, 0, 0]
    let paths = get_dom_paths(
        "<html><body><div><p>A</p></div></body></html>",
        "<html><body><div><p>B</p></div></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![0, 0, 0],
        "Text inside nested P should be at [0, 0, 0]"
    );
}

#[test]
fn second_child_of_div_dom_path() {
    // <body><div><p>A</p><p>B</p></div></body> -> <body><div><p>A</p><p>X</p></div></body>
    // DOM path should be [0, 1, 0]
    let paths = get_dom_paths(
        "<html><body><div><p>A</p><p>B</p></div></body></html>",
        "<html><body><div><p>A</p><p>X</p></div></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![0, 1, 0],
        "Text in second P of div should be at [0, 1, 0]"
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
// MIXED CONTENT
// =============================================================================

#[test]
fn text_before_element_dom_path() {
    // <body>Hello<p>World</p></body> -> <body>Hi<p>World</p></body>
    // Text is at [0]
    let paths = get_dom_paths(
        "<html><body>Hello<p>World</p></body></html>",
        "<html><body>Hi<p>World</p></body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![0],
        "Text before P should be at [0]"
    );
}

#[test]
fn text_after_element_dom_path() {
    // <body><p>First</p>Second</body> -> <body><p>First</p>Changed</body>
    // Text is at [1]
    let paths = get_dom_paths(
        "<html><body><p>First</p>Second</body></html>",
        "<html><body><p>First</p>Changed</body></html>",
    );

    let set_text = paths.iter().find(|(k, _)| k == "SetText");
    assert!(set_text.is_some(), "Should have SetText patch");
    assert_eq!(
        set_text.unwrap().1.0,
        vec![1],
        "Text after P should be at [1]"
    );
}
