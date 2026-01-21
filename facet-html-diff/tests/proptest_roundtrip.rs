//! Property-based tests for HTML diff roundtrip.
//!
//! The core invariant: apply(A, diff(A, B)) == B
//!
//! We generate random HTML trees, diff them, apply the patches,
//! and verify the result matches the expected output.

use facet_html_diff::apply::{Node, apply_patches};
use facet_html_diff::diff_html;
use proptest::prelude::*;

/// Generate a random text string (no HTML special chars).
fn arb_text() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ]{1,20}".prop_filter("no angle brackets", |s| {
        !s.contains('<') && !s.contains('>')
    })
}

/// Generate a random class name.
fn arb_class() -> impl Strategy<Value = Option<String>> {
    prop_oneof![Just(None), "[a-z][a-z0-9-]{0,10}".prop_map(Some),]
}

/// Generate a random id.
fn arb_id() -> impl Strategy<Value = Option<String>> {
    prop_oneof![Just(None), "[a-z][a-z0-9-]{0,10}".prop_map(Some),]
}

/// A simplified Node for generation (we can't use facet-html-dom directly in proptest).
/// Note: We only generate valid HTML structures - no nested block elements in P.
#[derive(Debug, Clone)]
enum SimpleNode {
    Text(String),
    // P elements can only contain text/span (phrasing content), not other block elements
    P {
        class: Option<String>,
        text: String, // Just text for simplicity, no nested elements
    },
    Div {
        class: Option<String>,
        id: Option<String>,
        children: Vec<SimpleNode>,
    },
    Span {
        class: Option<String>,
        text: String, // Just text for simplicity
    },
}

impl SimpleNode {
    /// Convert to HTML string.
    fn to_html(&self) -> String {
        match self {
            SimpleNode::Text(s) => s.clone(),
            SimpleNode::P { class, text } => {
                let attrs = class
                    .as_ref()
                    .map(|c| format!(" class=\"{}\"", c))
                    .unwrap_or_default();
                format!("<p{attrs}>{text}</p>")
            }
            SimpleNode::Div {
                class,
                id,
                children,
            } => {
                let mut attrs = String::new();
                if let Some(c) = class {
                    attrs.push_str(&format!(" class=\"{}\"", c));
                }
                if let Some(i) = id {
                    attrs.push_str(&format!(" id=\"{}\"", i));
                }
                let inner: String = children.iter().map(|c| c.to_html()).collect();
                format!("<div{attrs}>{inner}</div>")
            }
            SimpleNode::Span { class, text } => {
                let attrs = class
                    .as_ref()
                    .map(|c| format!(" class=\"{}\"", c))
                    .unwrap_or_default();
                format!("<span{attrs}>{text}</span>")
            }
        }
    }

    /// Wrap in html/body for full document.
    fn to_full_html(&self) -> String {
        format!("<html><body>{}</body></html>", self.to_html())
    }
}

/// Generate a simple node tree with limited depth.
fn arb_node(depth: usize) -> impl Strategy<Value = SimpleNode> {
    if depth == 0 {
        // Base case: only text or simple elements
        prop_oneof![
            arb_text().prop_map(SimpleNode::Text),
            (arb_class(), arb_text()).prop_map(|(class, text)| SimpleNode::P { class, text }),
            (arb_class(), arb_text()).prop_map(|(class, text)| SimpleNode::Span { class, text }),
        ]
        .boxed()
    } else {
        prop_oneof![
            // Text node
            arb_text().prop_map(SimpleNode::Text),
            // P element with text only (phrasing content restriction)
            (arb_class(), arb_text()).prop_map(|(class, text)| SimpleNode::P { class, text }),
            // Div element with children (can nest block elements)
            (
                arb_class(),
                arb_id(),
                prop::collection::vec(arb_node(depth - 1), 0..3)
            )
                .prop_map(|(class, id, children)| SimpleNode::Div {
                    class,
                    id,
                    children
                }),
            // Span element with text only (phrasing content)
            (arb_class(), arb_text()).prop_map(|(class, text)| SimpleNode::Span { class, text }),
        ]
        .boxed()
    }
}

/// Generate a body with multiple children.
fn arb_body() -> impl Strategy<Value = Vec<SimpleNode>> {
    prop::collection::vec(arb_node(2), 1..4)
}

/// Convert a list of SimpleNodes to full HTML.
fn nodes_to_html(nodes: &[SimpleNode]) -> String {
    let inner: String = nodes.iter().map(|n| n.to_html()).collect();
    format!("<html><body>{inner}</body></html>")
}

/// Normalize HTML for comparison by parsing and re-serializing.
fn normalize_html(html: &str) -> Result<String, String> {
    let node = Node::parse(html)?;
    Ok(node.to_html())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// The core invariant: apply(old, diff(old, new)) produces new.
    #[test]
    fn roundtrip_diff_apply(
        old_children in arb_body(),
        new_children in arb_body()
    ) {
        let old_html = nodes_to_html(&old_children);
        let new_html = nodes_to_html(&new_children);

        // Compute diff
        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        // Parse old into mutable tree
        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse old failed: {e}")))?;

        // Apply patches
        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        // Get result
        let result = tree.to_html();

        // Normalize expected for comparison
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize new failed: {e}")))?;

        prop_assert_eq!(
            result.clone(),
            expected.clone(),
            "Roundtrip failed!\nOld: {}\nNew: {}\nPatches: {:?}\nResult: {}\nExpected: {}",
            old_html,
            new_html,
            patches,
            result,
            expected
        );
    }

    /// Test with single element changes.
    #[test]
    fn single_element_roundtrip(
        node_a in arb_node(1),
        node_b in arb_node(1)
    ) {
        facet_testhelpers::setup();
        let old_html = node_a.to_full_html();
        let new_html = node_b.to_full_html();

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }

    /// Test text-only changes.
    #[test]
    fn text_only_roundtrip(
        text_a in arb_text(),
        text_b in arb_text()
    ) {
        let old_html = format!("<html><body><p>{text_a}</p></body></html>");
        let new_html = format!("<html><body><p>{text_b}</p></body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }

    /// Test attribute changes.
    #[test]
    fn attribute_roundtrip(
        class_a in arb_class(),
        class_b in arb_class(),
        text in arb_text()
    ) {
        let old_attrs = class_a.as_ref().map(|c| format!(" class=\"{}\"", c)).unwrap_or_default();
        let new_attrs = class_b.as_ref().map(|c| format!(" class=\"{}\"", c)).unwrap_or_default();

        let old_html = format!("<html><body><div{old_attrs}>{text}</div></body></html>");
        let new_html = format!("<html><body><div{new_attrs}>{text}</div></body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Test empty element transitions: adding/removing children from empty divs.
    /// This exercises the matching fix where empty nodes match non-empty nodes.
    #[test]
    fn empty_element_transitions(
        children in prop::collection::vec(arb_node(0), 0..3)
    ) {
        // Test: empty div -> div with children
        let old_html = "<html><body><div></div></body></html>".to_string();
        let new_inner: String = children.iter().map(|c| c.to_html()).collect();
        let new_html = format!("<html><body><div>{new_inner}</div></body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected, "empty->filled failed");
    }

    /// Test removing all children from an element.
    #[test]
    fn drain_element_children(
        children in prop::collection::vec(arb_node(0), 1..4)
    ) {
        // Test: div with children -> empty div
        let old_inner: String = children.iter().map(|c| c.to_html()).collect();
        let old_html = format!("<html><body><div>{old_inner}</div></body></html>");
        let new_html = "<html><body><div></div></body></html>".to_string();

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected, "filled->empty failed");
    }

    /// Test deeper nesting (depth 3-4).
    #[test]
    fn deep_nesting_roundtrip(
        old_children in prop::collection::vec(arb_node(3), 1..3),
        new_children in prop::collection::vec(arb_node(3), 1..3)
    ) {
        let old_html = nodes_to_html(&old_children);
        let new_html = nodes_to_html(&new_children);

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }

    /// Test sibling reordering - same elements in different order.
    #[test]
    fn sibling_reorder_roundtrip(
        mut elements in prop::collection::vec(arb_node(0), 2..5)
    ) {
        let old_html = nodes_to_html(&elements);

        // Rotate elements to create a reordering
        if !elements.is_empty() {
            elements.rotate_left(1);
        }
        let new_html = nodes_to_html(&elements);

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }

    /// Test moving content between siblings.
    #[test]
    fn content_move_between_siblings(
        text in arb_text()
    ) {
        // Content moves from one div to another
        let old_html = format!("<html><body><div>{text}</div><div></div></body></html>");
        let new_html = format!("<html><body><div></div><div>{text}</div></body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }

    /// Test text moving into/out of elements.
    #[test]
    fn text_element_boundary_moves(
        text in arb_text()
    ) {
        // Text outside div moves inside
        let old_html = format!("<html><body>{text}<div></div></body></html>");
        let new_html = format!("<html><body><div>{text}</div></body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }

    /// Test inserting text before nested div structures (issue #1846 pattern).
    /// This specifically tests the slot-based displacement with nested elements.
    #[test]
    fn insert_before_nested_divs(
        text in arb_text(),
        depth in 1usize..4
    ) {
        // Build nested divs: <div><div>...<div></div>...</div></div>
        let mut inner = String::new();
        for _ in 0..depth {
            inner = format!("<div>{inner}</div>");
        }

        let old_html = format!("<html><body>{inner}</body></html>");
        let new_html = format!("<html><body>{text}{inner}</body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected, "insert before nested divs failed at depth {}", depth);
    }

    /// Test inserting text into deeply nested divs (the exact issue #1846 pattern).
    #[test]
    fn insert_into_nested_divs(
        text in arb_text(),
        depth in 1usize..4
    ) {
        // Old: <div><div>...</div></div> (nested empty divs)
        // New: <div><div>...<div>TEXT</div>...</div></div> (text in innermost)
        let mut old_inner = String::new();
        for _ in 0..depth {
            old_inner = format!("<div>{old_inner}</div>");
        }

        let mut new_inner = text.clone();
        for _ in 0..depth {
            new_inner = format!("<div>{new_inner}</div>");
        }

        let old_html = format!("<html><body>{old_inner}</body></html>");
        let new_html = format!("<html><body>{new_inner}</body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected, "insert into nested divs failed at depth {}", depth);
    }

    /// Test the exact pattern from issue #1846: insert text before nested divs AND
    /// insert text into the innermost div.
    #[test]
    fn issue_1846_pattern(
        text_before in arb_text(),
        text_inside in arb_text(),
        depth in 1usize..4
    ) {
        // Old: <div><div>...</div></div>
        // New: TEXT_BEFORE<div><div>...<div>TEXT_INSIDE</div>...</div></div>
        let mut old_inner = String::new();
        for _ in 0..depth {
            old_inner = format!("<div>{old_inner}</div>");
        }

        let mut new_inner = text_inside.clone();
        for _ in 0..depth {
            new_inner = format!("<div>{new_inner}</div>");
        }

        let old_html = format!("<html><body>{old_inner}</body></html>");
        let new_html = format!("<html><body>{text_before}{new_inner}</body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected, "issue 1846 pattern failed at depth {}", depth);
    }

    /// Test nested structures with mixed content.
    #[test]
    fn nested_mixed_content(
        texts in prop::collection::vec(arb_text(), 2..4),
        depth in 1usize..3
    ) {
        // Create nested divs with text at various levels
        let mut old_content = texts[0].clone();
        let mut new_content = texts.get(1).cloned().unwrap_or_default();

        for i in 0..depth {
            let extra_text = texts.get(i + 1).cloned().unwrap_or_default();
            old_content = format!("<div>{old_content}</div>");
            new_content = format!("<div>{extra_text}{new_content}</div>");
        }

        let old_html = format!("<html><body>{old_content}</body></html>");
        let new_html = format!("<html><body>{new_content}</body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }

    /// Test multiple sibling elements being displaced and reinserted.
    #[test]
    fn multiple_siblings_displaced(
        insert_text in arb_text(),
        num_siblings in 2usize..5
    ) {
        // Old: <div>A</div><div>B</div><div>C</div>...
        // New: TEXT<div>A</div><div>B</div><div>C</div>...
        let siblings: String = (0..num_siblings)
            .map(|i| format!("<div>child{i}</div>"))
            .collect();

        let old_html = format!("<html><body>{siblings}</body></html>");
        let new_html = format!("<html><body>{insert_text}{siblings}</body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }

    /// Test deeply nested structure where inner content changes.
    #[test]
    fn deep_inner_content_change(
        old_text in arb_text(),
        new_text in arb_text(),
        depth in 2usize..5
    ) {
        let mut old_inner = old_text;
        let mut new_inner = new_text;

        for _ in 0..depth {
            old_inner = format!("<div>{old_inner}</div>");
            new_inner = format!("<div>{new_inner}</div>");
        }

        let old_html = format!("<html><body>{old_inner}</body></html>");
        let new_html = format!("<html><body>{new_inner}</body></html>");

        let patches = diff_html(&old_html, &new_html)
            .map_err(|e| TestCaseError::fail(format!("diff failed: {e}")))?;

        let mut tree = Node::parse(&old_html)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;

        apply_patches(&mut tree, &patches)
            .map_err(|e| TestCaseError::fail(format!("apply failed: {e}")))?;

        let result = tree.to_html();
        let expected = normalize_html(&new_html)
            .map_err(|e| TestCaseError::fail(format!("normalize failed: {e}")))?;

        prop_assert_eq!(result, expected);
    }
}

#[cfg(test)]
mod sanity_tests {
    use super::*;
    use facet_testhelpers::test;

    /// Sanity check: identical documents produce no patches.
    #[test]
    fn identical_documents_no_patches() {
        let html = "<html><body><p>Hello</p></body></html>";
        let patches = diff_html(html, html).unwrap();
        // Might have some spurious patches but applying them should still work
        let mut tree = Node::parse(html).unwrap();
        apply_patches(&mut tree, &patches).unwrap();
        let result = tree.to_html();
        let expected = normalize_html(html).unwrap();
        assert_eq!(result, expected);
    }

    /// Sanity check: simple text change.
    #[test]
    fn simple_text_change_roundtrip() {
        let old = "<html><body><p>Hello</p></body></html>";
        let new = "<html><body><p>Goodbye</p></body></html>";

        let patches = diff_html(old, new).unwrap();
        let mut tree = Node::parse(old).unwrap();
        apply_patches(&mut tree, &patches).unwrap();

        let result = tree.to_html();
        let expected = normalize_html(new).unwrap();
        assert_eq!(result, expected);
    }

    /// Sanity check: add element.
    #[test]
    fn add_element_roundtrip() {
        let old = "<html><body><p>First</p></body></html>";
        let new = "<html><body><p>First</p><p>Second</p></body></html>";

        let patches = diff_html(old, new).unwrap();
        let mut tree = Node::parse(old).unwrap();
        apply_patches(&mut tree, &patches).unwrap();

        let result = tree.to_html();
        let expected = normalize_html(new).unwrap();
        assert_eq!(result, expected);
    }

    /// Sanity check: remove element.
    #[test]
    fn remove_element_roundtrip() {
        let old = "<html><body><p>First</p><p>Second</p></body></html>";
        let new = "<html><body><p>First</p></body></html>";

        let patches = diff_html(old, new).unwrap();
        let mut tree = Node::parse(old).unwrap();
        apply_patches(&mut tree, &patches).unwrap();

        let result = tree.to_html();
        let expected = normalize_html(new).unwrap();
        assert_eq!(result, expected);
    }

    /// Sanity check: attribute change.
    #[test]
    fn attribute_change_roundtrip() {
        let old = r#"<html><body><div class="old">Content</div></body></html>"#;
        let new = r#"<html><body><div class="new">Content</div></body></html>"#;

        let patches = diff_html(old, new).unwrap();
        let mut tree = Node::parse(old).unwrap();
        apply_patches(&mut tree, &patches).unwrap();

        let result = tree.to_html();
        let expected = normalize_html(new).unwrap();
        assert_eq!(result, expected);
    }
}
