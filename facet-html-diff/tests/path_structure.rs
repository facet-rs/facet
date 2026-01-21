//! Tests for understanding facet-diff path structure.
//!
//! These tests verify how facet-diff generates paths for HTML documents.

use facet_diff::{EditOp, PathSegment, tree_diff};
use facet_html_dom::Html;
use facet_testhelpers::test;

fn get_raw_ops(old: &str, new: &str) -> Vec<EditOp> {
    let old_doc: Html = facet_html::from_str(old).unwrap();
    let new_doc: Html = facet_html::from_str(new).unwrap();
    tree_diff(&old_doc, &new_doc)
}

fn get_paths(old: &str, new: &str) -> Vec<Vec<PathSegment>> {
    get_raw_ops(old, new)
        .into_iter()
        .map(|op| match op {
            EditOp::Update { path, .. } => path.0,
            EditOp::Insert { path, .. } => path.0,
            EditOp::Delete { path, .. } => path.0,
            EditOp::Move { to, .. } => to.0,
            EditOp::UpdateAttribute { path, .. } => path.0,
            _ => vec![],
        })
        .collect()
}

fn get_ops(old: &str, new: &str) -> Vec<(String, String)> {
    let old_doc: Html = facet_html::from_str(old).unwrap();
    let new_doc: Html = facet_html::from_str(new).unwrap();
    let ops = tree_diff(&old_doc, &new_doc);
    ops.iter()
        .map(|op| {
            let (kind, path) = match op {
                EditOp::Update { path, .. } => ("Update", fmt_path(&path.0)),
                EditOp::Insert { path, .. } => ("Insert", fmt_path(&path.0)),
                EditOp::Delete { path, .. } => ("Delete", fmt_path(&path.0)),
                EditOp::Move { to, .. } => ("Move", fmt_path(&to.0)),
                EditOp::UpdateAttribute { path, .. } => ("UpdateAttr", fmt_path(&path.0)),
                _ => ("Other", String::new()),
            };
            (kind.to_string(), path)
        })
        .collect()
}

fn fmt_path(segments: &[PathSegment]) -> String {
    segments
        .iter()
        .map(|s| match s {
            PathSegment::Field(f) => format!("F({f})"),
            PathSegment::Index(i) => format!("I({i})"),
            PathSegment::Variant(v) => format!("V({v})"),
            PathSegment::Key(k) => format!("K({k})"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn assert_deepest_ends_with(ops: &[(String, String)], expected_suffix: &str) {
    let deepest = ops.iter().max_by_key(|(_, p)| p.len());
    if let Some((kind, path)) = deepest {
        assert!(
            path.ends_with(expected_suffix),
            "Deepest path ({kind}) should end with [{expected_suffix}], got [{path}]"
        );
    } else {
        panic!("No ops found");
    }
}

// =============================================================================
// PATH SEGMENT BEHAVIOR TESTS
// =============================================================================

#[test]
fn option_fields_do_not_add_index_segment() {
    // Html.body is Option<Body>, but the path should NOT have an Index(0) for unwrapping it.
    let paths = get_paths(
        "<html><body>Hello</body></html>",
        "<html><body>World</body></html>",
    );

    // The path F(body), I(0) should exist and I(0) should be children index
    let has_body_then_index = paths.iter().any(|p| {
        p.len() >= 2
            && matches!(&p[0], PathSegment::Field(f) if f == "body")
            && matches!(&p[1], PathSegment::Index(0))
    });
    assert!(
        has_body_then_index,
        "Should have path starting with F(body), I(0) - Option doesn't add segment"
    );
}

#[test]
fn index_after_variant_is_tuple_field_access() {
    // When we have V(P), I(0), that I(0) is accessing the P struct inside the enum,
    // NOT a DOM child index.
    let paths = get_paths(
        "<html><body><p>A</p></body></html>",
        "<html><body><p>B</p></body></html>",
    );

    let has_variant_then_index = paths.iter().any(|p| {
        p.windows(2).any(|w| {
            matches!(&w[0], PathSegment::Variant(v) if v == "P")
                && matches!(&w[1], PathSegment::Index(0))
        })
    });
    assert!(
        has_variant_then_index,
        "Should have V(P), I(0) sequence - Index accesses tuple field inside variant"
    );
}

#[test]
fn index_into_flattened_list_is_dom_index() {
    // Body.children is Vec<FlowContent> with #[facet(flatten)].
    // Index into this list IS a DOM index.
    let paths = get_paths(
        "<html><body><p>A</p><p>B</p></body></html>",
        "<html><body><p>A</p><p>X</p></body></html>",
    );

    let has_second_child_index = paths.iter().any(|p| {
        p.len() >= 2
            && matches!(&p[0], PathSegment::Field(f) if f == "body")
            && matches!(&p[1], PathSegment::Index(1))
    });
    assert!(
        has_second_child_index,
        "Changing second child should have F(body), I(1) - Index is DOM index"
    );
}

#[test]
fn add_id_and_child_generates_ops() {
    let ops = get_raw_ops(
        r#"<html><body><div></div></body></html>"#,
        r#"<html><body><div id="a"><p>a</p></div></body></html>"#,
    );

    for op in &ops {
        tracing::debug!("{op:?}");
    }

    assert!(
        !ops.is_empty(),
        "Should generate edit ops for adding id and child"
    );

    // Should have UpdateAttribute op for the id attribute
    let has_update_attr = ops
        .iter()
        .any(|op| matches!(op, EditOp::UpdateAttribute { attr_name, .. } if *attr_name == "id"));
    assert!(
        has_update_attr,
        "Should have UpdateAttribute op for id, got: {:?}",
        ops
    );
}

#[test]
fn update_op_contains_new_value_for_text() {
    let ops = get_raw_ops(
        "<html><body>Hello</body></html>",
        "<html><body>World</body></html>",
    );

    let text_update = ops.iter().find(|op| {
        if let EditOp::Update { path, .. } = op {
            path.0
                .iter()
                .any(|s| matches!(s, PathSegment::Variant(v) if v == "Text"))
        } else {
            false
        }
    });

    assert!(text_update.is_some(), "Should have Update op for text");

    if let Some(EditOp::Update { new_value, .. }) = text_update {
        assert!(
            new_value.is_some(),
            "Update op for text should have new_value populated"
        );
        assert_eq!(new_value.as_deref(), Some("World"));
    }
}

// =============================================================================
// BODY DIRECT CHILDREN TESTS
// =============================================================================

#[test]
fn body_text_only() {
    let ops = get_ops(
        "<html><body>Hello</body></html>",
        "<html><body>World</body></html>",
    );

    assert_deepest_ends_with(&ops, "V(Text), I(0)");
    assert!(
        ops.iter()
            .any(|(_, p)| p.contains("F(body), I(0), V(Text)")),
        "Should have path through body's first child to Text variant"
    );
}

#[test]
fn single_p_text_change() {
    let ops = get_ops(
        "<html><body><p>A</p></body></html>",
        "<html><body><p>B</p></body></html>",
    );

    assert_deepest_ends_with(&ops, "V(Text), I(0)");
    assert!(
        ops.iter().any(|(_, p)| p.contains("V(P)")),
        "Should have path through P variant"
    );
}

#[test]
fn two_p_change_first() {
    let ops = get_ops(
        "<html><body><p>A</p><p>B</p></body></html>",
        "<html><body><p>X</p><p>B</p></body></html>",
    );

    let has_first_child = ops
        .iter()
        .any(|(_, p)| p.starts_with("F(body), I(0), V(P)"));
    assert!(
        has_first_child,
        "Changed path should target body's first child"
    );
}

#[test]
fn two_p_change_second() {
    let ops = get_ops(
        "<html><body><p>A</p><p>B</p></body></html>",
        "<html><body><p>A</p><p>Y</p></body></html>",
    );

    let has_second_child = ops
        .iter()
        .any(|(_, p)| p.starts_with("F(body), I(1), V(P)"));
    assert!(
        has_second_child,
        "Changed path should target body's second child"
    );
}

#[test]
fn two_p_change_both() {
    let ops = get_ops(
        "<html><body><p>A</p><p>B</p></body></html>",
        "<html><body><p>X</p><p>Y</p></body></html>",
    );

    let has_first = ops.iter().any(|(_, p)| p.contains("F(body), I(0), V(P)"));
    let has_second = ops.iter().any(|(_, p)| p.contains("F(body), I(1), V(P)"));

    assert!(has_first, "Should have path through first child I(0)");
    assert!(has_second, "Should have path through second child I(1)");
}

// =============================================================================
// NESTED ELEMENT TESTS
// =============================================================================

#[test]
fn nested_div_p() {
    let ops = get_ops(
        "<html><body><div><p>A</p></div></body></html>",
        "<html><body><div><p>B</p></div></body></html>",
    );

    assert!(
        ops.iter()
            .any(|(_, p)| p.contains("V(Div)") && p.contains("V(P)")),
        "Should have path through both Div and P variants"
    );
}

#[test]
fn span_inside_p() {
    let ops = get_ops(
        "<html><body><p><span>A</span></p></body></html>",
        "<html><body><p><span>B</span></p></body></html>",
    );

    assert!(
        ops.iter()
            .any(|(_, p)| p.contains("V(P)") && p.contains("V(Span)")),
        "Should have path through P and Span variants"
    );
}

#[test]
fn multiple_spans_in_p() {
    let ops = get_ops(
        "<html><body><p><span>A</span><span>B</span></p></body></html>",
        "<html><body><p><span>X</span><span>Y</span></p></body></html>",
    );

    assert!(
        ops.iter().filter(|(_, p)| p.contains("V(Span)")).count() >= 2,
        "Should have at least 2 paths through Span variant"
    );
}

// =============================================================================
// MIXED CONTENT TESTS
// =============================================================================

#[test]
fn text_before_p() {
    let ops = get_ops(
        "<html><body>Hello<p>World</p></body></html>",
        "<html><body>Hi<p>World</p></body></html>",
    );

    let has_text_at_0 = ops
        .iter()
        .any(|(_, p)| p.contains("F(body), I(0), V(Text)"));
    assert!(has_text_at_0, "Text should be at children index 0");
}

#[test]
fn p_before_text() {
    let ops = get_ops(
        "<html><body><p>First</p>Second</body></html>",
        "<html><body><p>First</p>Changed</body></html>",
    );

    let has_text_at_1 = ops
        .iter()
        .any(|(_, p)| p.contains("F(body), I(1), V(Text)"));
    assert!(has_text_at_1, "Text should be at children index 1");
}

// =============================================================================
// ATTRIBUTE TESTS
// =============================================================================

#[test]
fn add_class_to_p() {
    let ops = get_ops(
        "<html><body><p>Text</p></body></html>",
        r#"<html><body><p class="foo">Text</p></body></html>"#,
    );

    let attr_ops: Vec<_> = ops
        .iter()
        .filter(|(_, p)| p.contains("V(P)") && !p.contains("V(Text)"))
        .collect();

    assert!(
        !attr_ops.is_empty(),
        "Should have attribute-related ops on P"
    );
}

#[test]
fn attribute_on_nested_p() {
    let ops = get_ops(
        "<html><body><div><p>Text</p></div></body></html>",
        r#"<html><body><div><p id="x">Text</p></div></body></html>"#,
    );

    let has_nested_attr = ops
        .iter()
        .any(|(_, p)| p.contains("V(Div)") && p.contains("V(P)"));
    assert!(
        has_nested_attr,
        "Should have path through Div to P for attribute"
    );
}

// =============================================================================
// INSERT/DELETE TESTS
// =============================================================================

#[test]
fn insert_second_child() {
    let ops = get_ops(
        "<html><body><p>A</p></body></html>",
        "<html><body><p>A</p><p>B</p></body></html>",
    );

    assert!(
        ops.iter().any(|(k, _)| k == "Insert"),
        "Should have Insert operation"
    );
}

#[test]
fn delete_first_child() {
    let ops = get_ops(
        "<html><body><p>A</p><p>B</p></body></html>",
        "<html><body><p>B</p></body></html>",
    );

    assert!(
        ops.iter().any(|(k, _)| k == "Delete" || k == "Move"),
        "Should have Delete or Move operation"
    );
}

#[test]
fn empty_to_single_child() {
    let ops = get_ops(
        "<html><body></body></html>",
        "<html><body><p>New</p></body></html>",
    );

    assert!(
        ops.iter().any(|(k, _)| k == "Insert"),
        "Should have Insert operation"
    );
}

#[test]
fn single_child_to_empty() {
    let ops = get_ops(
        "<html><body><p>Old</p></body></html>",
        "<html><body></body></html>",
    );

    assert!(
        ops.iter().any(|(k, _)| k == "Delete"),
        "Should have Delete operation"
    );
}
