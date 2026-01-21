//! Tests for HTML diff path translation.
//!
//! All tests verify the core invariant: `apply(A, diff(A, B)) == B`

use facet_html_diff::apply::{Node, apply_patches};
use facet_testhelpers::test;

/// Assert that diffing old -> new and applying patches produces the expected result.
#[track_caller]
fn assert_roundtrip(old: &str, new: &str) {
    let patches = facet_html_diff::diff_html(old, new).unwrap();
    tracing::debug!("Patches for {old} -> {new}:");
    for patch in &patches {
        tracing::debug!("  {patch:?}");
    }

    let mut tree = Node::parse(old).unwrap();
    apply_patches(&mut tree, &patches).unwrap();
    let result = tree.to_html();

    let expected_tree = Node::parse(new).unwrap();
    let expected = expected_tree.to_html();

    assert_eq!(
        result, expected,
        "Roundtrip failed!\nOld: {old}\nNew: {new}\nPatches: {patches:?}\nResult: {result}\nExpected: {expected}"
    );
}

#[test]
fn simple_text_change() {
    assert_roundtrip(
        r#"<html><body><p>Hello</p></body></html>"#,
        r#"<html><body><p>Goodbye</p></body></html>"#,
    );
}

#[test]
fn insert_element() {
    assert_roundtrip(
        r#"<html><body><p>First</p></body></html>"#,
        r#"<html><body><p>First</p><p>Second</p></body></html>"#,
    );
}

#[test]
fn remove_element() {
    assert_roundtrip(
        r#"<html><body><p>First</p><p>Second</p></body></html>"#,
        r#"<html><body><p>First</p></body></html>"#,
    );
}

#[test]
fn attribute_change() {
    assert_roundtrip(
        r#"<html><body><div class="old">Content</div></body></html>"#,
        r#"<html><body><div class="new">Content</div></body></html>"#,
    );
}

#[test]
fn mixed_changes() {
    assert_roundtrip(
        r#"<html><body><div class="box"><p>One</p><p>Two</p></div></body></html>"#,
        r#"<html><body><div class="container"><p>One</p><p>Modified</p><p>Three</p></div></body></html>"#,
    );
}

#[test]
fn nested_text_change() {
    assert_roundtrip(
        r#"<html><body><div><span>Hello</span></div></body></html>"#,
        r#"<html><body><div><span>World</span></div></body></html>"#,
    );
}

#[test]
fn add_attribute() {
    assert_roundtrip(
        r#"<html><body><div>Content</div></body></html>"#,
        r#"<html><body><div id="main">Content</div></body></html>"#,
    );
}

#[test]
fn remove_attribute() {
    assert_roundtrip(
        r#"<html><body><div id="main">Content</div></body></html>"#,
        r#"<html><body><div>Content</div></body></html>"#,
    );
}

#[test]
fn remove_attribute_and_change_text() {
    assert_roundtrip(
        r#"<html><body><div id="a"><p class="a">A</p></div></body></html>"#,
        r#"<html><body><div><p>a</p></div></body></html>"#,
    );
}

#[test]
fn identical_documents() {
    let html = r#"<html><body><p>Same content</p></body></html>"#;
    assert_roundtrip(html, html);
}

#[test]
fn replace_element_type() {
    assert_roundtrip(
        r#"<html><body><p>a</p></body></html>"#,
        r#"<html><body><div></div></body></html>"#,
    );
}

#[test]
fn add_nested_element() {
    assert_roundtrip(
        r#"<html><body><div>a</div></body></html>"#,
        r#"<html><body><div><p>a</p></div></body></html>"#,
    );
}

#[test]
fn body_text_change() {
    assert_roundtrip(
        r#"<html><body>0</body></html>"#,
        r#"<html><body>A</body></html>"#,
    );
}

#[test]
fn add_id_and_child_to_empty_div() {
    assert_roundtrip(
        r#"<html><body><div></div></body></html>"#,
        r#"<html><body><div id="a"><p>a</p></div></body></html>"#,
    );
}

// Proptest failure case 1: Replace two P elements with just text
#[test]
fn replace_elements_with_text() {
    assert_roundtrip(
        r#"<html><body><p>a</p><p>0</p></body></html>"#,
        r#"<html><body>A</body></html>"#,
    );
}

// Proptest failure case 2: Delete text node, change P text
#[test]
fn remove_text_and_modify_sibling() {
    assert_roundtrip(
        r#"<html><body><div>a<p>A</p></div></body></html>"#,
        r#"<html><body><div><p>a</p></div></body></html>"#,
    );
}

// Proptest failure case 3: Wrap content in div - produces empty patches!
#[test]
fn wrap_in_div() {
    assert_roundtrip(
        r#"<html><body><p>A</p></body></html>"#,
        r#"<html><body><div><p>a</p><p>A</p></div></body></html>"#,
    );
}

// Proptest failure case 4: Attribute swap - remove one attr, add another
#[test]
fn attribute_swap() {
    assert_roundtrip(
        r#"<html><body><div id="a"></div></body></html>"#,
        r#"<html><body><div class="a"></div></body></html>"#,
    );
}

// Proptest failure case 5: Both attrs present, change class and remove id
#[test]
fn change_class_remove_id() {
    assert_roundtrip(
        r#"<html><body><div class="a" id="a"></div></body></html>"#,
        r#"<html><body><div class="a-"></div></body></html>"#,
    );
}

// Proptest failure case 6: Remove P and text, replace with different text
#[test]
fn replace_p_and_text_with_text() {
    assert_roundtrip(
        r#"<html><body><p>0</p>0</body></html>"#,
        r#"<html><body> </body></html>"#,
    );
}

// Proptest failure case 7: Remove class from span, change text, remove sibling
#[test]
fn remove_class_and_sibling() {
    assert_roundtrip(
        r#"<html><body><span class="a">0</span><span> </span></body></html>"#,
        r#"<html><body><span>A</span></body></html>"#,
    );
}

// Proptest failure: Text + two Spans -> one Span
#[test]
fn text_and_spans_to_span() {
    assert_roundtrip(
        r#"<html><body> <span>0</span><span> </span></body></html>"#,
        r#"<html><body><span>A</span></body></html>"#,
    );
}

// Proptest failure: Add class to P and add another P
#[test]
fn add_class_and_sibling() {
    assert_roundtrip(
        r#"<html><body><p>A</p></body></html>"#,
        r#"<html><body><p class="a">A</p><p> </p></body></html>"#,
    );
}

// Proptest failure: Remove text from div
#[test]
fn remove_div_text() {
    assert_roundtrip(
        r#"<html><body><div>0</div></body></html>"#,
        r#"<html><body><div></div></body></html>"#,
    );
}

// Proptest failure: Text + Div with text -> empty Div
#[test]
fn text_and_div_to_empty_div() {
    assert_roundtrip(
        r#"<html><body>A<div>0</div></body></html>"#,
        r#"<html><body><div></div></body></html>"#,
    );
}

// Proptest failure: Text + Div with text -> empty Div + Text
#[test]
fn text_and_div_to_empty_div_and_text() {
    assert_roundtrip(
        r#"<html><body>A<div>0</div></body></html>"#,
        r#"<html><body><div></div> </body></html>"#,
    );
}

// Proptest failure: Text moves into div (sibling becomes child)
#[test]
fn text_moves_into_div() {
    assert_roundtrip(
        r#"<html><body>0<div></div></body></html>"#,
        r#"<html><body><div>0</div></body></html>"#,
    );
}

// =============================================================================
// Path Structure Understanding Tests
// =============================================================================
//
// These tests document the exact path structure generated by facet-diff for HTML.
// Understanding these paths is critical for facet-html-diff to correctly translate
// them into DOM operations.
//
// KEY INSIGHT: We walk the type structure alongside the path segments to determine
// what each segment means. The metadata on fields (is_attribute, is_text, is_flattened)
// tells us how to interpret the path.
//
// PATH ANATOMY for `<body><p>Hello</p></body>`:
// ┌─────────────────┬─────────────────────────────────────────────────────────┐
// │ Segment         │ Meaning                                                 │
// ├─────────────────┼─────────────────────────────────────────────────────────┤
// │ F(body)         │ Field access: Html.body (Option<Body>)                  │
// │ I(0)            │ List index: Body.children[0] (IS a DOM index)           │
// │ V(P)            │ Variant selection: FlowContent::P                       │
// │ I(0)            │ Tuple field: P struct inside enum (NOT a DOM index)     │
// │ I(0)            │ List index: P.children[0] (IS a DOM index - flattened)  │
// │ V(Text)         │ Variant selection: PhrasingContent::Text                │
// │ I(0)            │ Tuple field: String inside enum (NOT a DOM index)       │
// └─────────────────┴─────────────────────────────────────────────────────────┘
//
// NOTE: Option fields don't add an Index segment to the path! The path goes
// directly from F(body) to I(0) for the children index.
//
// CRITICAL: Only Index segments into FLATTENED LISTS are DOM indices.
// - Index after Variant -> tuple field access, NOT a DOM index
// - Index into Vec with #[facet(flatten)] -> IS a DOM index

mod path_structure {
    use facet_diff::{EditOp, PathSegment, tree_diff};
    use facet_html_dom::Html;
    use facet_testhelpers::test;

    /// Get edit ops with their paths (as formatted strings) for analysis
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
                    EditOp::Move { new_path, .. } => ("Move", fmt_path(&new_path.0)),
                    EditOp::UpdateAttribute { path, .. } => ("UpdateAttr", fmt_path(&path.0)),
                    _ => ("Other", String::new()),
                };
                (kind.to_string(), path)
            })
            .collect()
    }

    /// Format path for display
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

    /// Assert the deepest path ends with given suffix
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

    // =========================================================================
    // BODY DIRECT CHILDREN TESTS
    // =========================================================================

    #[test]
    fn body_text_only() {
        // Simplest case: text directly in body
        // Path: body -> Option unwrap -> children[0] -> Text variant -> inner String
        let ops = get_ops(
            "<html><body>Hello</body></html>",
            "<html><body>World</body></html>",
        );

        tracing::debug!("body_text_only ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // The Update should target the text content
        // Path ends with V(Text), I(0) - the Text variant's inner String
        assert_deepest_ends_with(&ops, "V(Text), I(0)");

        // The children index is I(0) - first child of body (no Option unwrap in path)
        assert!(
            ops.iter()
                .any(|(_, p)| p.contains("F(body), I(0), V(Text)")),
            "Should have path through body's first child to Text variant"
        );
    }

    #[test]
    fn single_p_text_change() {
        // <body><p>A</p></body> -> <body><p>B</p></body>
        // Path: body -> unwrap -> children[0] -> P variant -> inner P -> children[0] -> Text -> inner
        let ops = get_ops(
            "<html><body><p>A</p></body></html>",
            "<html><body><p>B</p></body></html>",
        );

        tracing::debug!("single_p_text_change ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Deepest path ends at the text content inside the P
        assert_deepest_ends_with(&ops, "V(Text), I(0)");

        // Path should go through P variant
        assert!(
            ops.iter().any(|(_, p)| p.contains("V(P)")),
            "Should have path through P variant"
        );
    }

    #[test]
    fn two_p_change_first() {
        // Changing first P - the children index should be I(0)
        let ops = get_ops(
            "<html><body><p>A</p><p>B</p></body></html>",
            "<html><body><p>X</p><p>B</p></body></html>",
        );

        tracing::debug!("two_p_change_first ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // First child of body is I(0) - NO Option unwrap index in path!
        // Pattern: F(body), I(0), V(P), ...
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
        // Changing second P - the children index should be I(1)
        let ops = get_ops(
            "<html><body><p>A</p><p>B</p></body></html>",
            "<html><body><p>A</p><p>Y</p></body></html>",
        );

        tracing::debug!("two_p_change_second ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Second child of body is I(1) - NO Option unwrap index in path!
        // Pattern: F(body), I(1), V(P), ...
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
        // Changing both Ps - should have paths with I(0) and I(1)
        let ops = get_ops(
            "<html><body><p>A</p><p>B</p></body></html>",
            "<html><body><p>X</p><p>Y</p></body></html>",
        );

        tracing::debug!("two_p_change_both ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        let has_first = ops.iter().any(|(_, p)| p.contains("F(body), I(0), V(P)"));
        let has_second = ops.iter().any(|(_, p)| p.contains("F(body), I(1), V(P)"));

        assert!(has_first, "Should have path through first child I(0)");
        assert!(has_second, "Should have path through second child I(1)");
    }

    // =========================================================================
    // NESTED ELEMENT TESTS
    // =========================================================================

    #[test]
    fn nested_div_p() {
        // <body><div><p>A</p></div></body> -> <body><div><p>B</p></div></body>
        // Two levels of flattened children: body.children[0] = Div, div.children[0] = P
        let ops = get_ops(
            "<html><body><div><p>A</p></div></body></html>",
            "<html><body><div><p>B</p></div></body></html>",
        );

        tracing::debug!("nested_div_p ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Should go through Div variant then P variant
        assert!(
            ops.iter()
                .any(|(_, p)| p.contains("V(Div)") && p.contains("V(P)")),
            "Should have path through both Div and P variants"
        );
    }

    #[test]
    fn span_inside_p() {
        // Span is PhrasingContent, P contains Vec<PhrasingContent>
        let ops = get_ops(
            "<html><body><p><span>A</span></p></body></html>",
            "<html><body><p><span>B</span></p></body></html>",
        );

        tracing::debug!("span_inside_p ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Should go through P, then Span variant
        assert!(
            ops.iter()
                .any(|(_, p)| p.contains("V(P)") && p.contains("V(Span)")),
            "Should have path through P and Span variants"
        );
    }

    #[test]
    fn multiple_spans_in_p() {
        // Two spans inside P - distinguished by index in P's children
        let ops = get_ops(
            "<html><body><p><span>A</span><span>B</span></p></body></html>",
            "<html><body><p><span>X</span><span>Y</span></p></body></html>",
        );

        tracing::debug!("multiple_spans_in_p ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Both spans should be updated
        assert!(
            ops.iter().filter(|(_, p)| p.contains("V(Span)")).count() >= 2,
            "Should have at least 2 paths through Span variant for both spans"
        );
    }

    // =========================================================================
    // MIXED CONTENT TESTS
    // =========================================================================

    #[test]
    fn text_before_p() {
        // <body>Hello<p>World</p></body> - text is child 0, P is child 1
        let ops = get_ops(
            "<html><body>Hello<p>World</p></body></html>",
            "<html><body>Hi<p>World</p></body></html>",
        );

        tracing::debug!("text_before_p ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Text is at index 0 in body's children
        // Path: F(body), I(0), V(Text), I(0)
        let has_text_at_0 = ops
            .iter()
            .any(|(_, p)| p.contains("F(body), I(0), V(Text)"));
        assert!(has_text_at_0, "Text should be at children index 0");
    }

    #[test]
    fn p_before_text() {
        // <body><p>First</p>Second</body> - P is child 0, text is child 1
        let ops = get_ops(
            "<html><body><p>First</p>Second</body></html>",
            "<html><body><p>First</p>Changed</body></html>",
        );

        tracing::debug!("p_before_text ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Text is at index 1 in body's children
        // Path: F(body), I(1), V(Text), I(0)
        let has_text_at_1 = ops
            .iter()
            .any(|(_, p)| p.contains("F(body), I(1), V(Text)"));
        assert!(has_text_at_1, "Text should be at children index 1");
    }

    // =========================================================================
    // ATTRIBUTE TESTS
    // =========================================================================

    #[test]
    fn add_class_to_p() {
        // Adding class attribute to P
        let ops = get_ops(
            "<html><body><p>Text</p></body></html>",
            r#"<html><body><p class="foo">Text</p></body></html>"#,
        );

        tracing::debug!("add_class_to_p ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Attribute paths go through P but don't include the children index
        // They should end at the P struct level (after V(P), I(0))
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
        // Attribute on P inside Div
        let ops = get_ops(
            "<html><body><div><p>Text</p></div></body></html>",
            r#"<html><body><div><p id="x">Text</p></div></body></html>"#,
        );

        tracing::debug!("attribute_on_nested_p ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Path goes through Div then P
        let has_nested_attr = ops
            .iter()
            .any(|(_, p)| p.contains("V(Div)") && p.contains("V(P)"));
        assert!(
            has_nested_attr,
            "Should have path through Div to P for attribute"
        );
    }

    // =========================================================================
    // INSERT/DELETE TESTS
    // =========================================================================

    #[test]
    fn insert_second_child() {
        // Insert a new P after existing one
        let ops = get_ops(
            "<html><body><p>A</p></body></html>",
            "<html><body><p>A</p><p>B</p></body></html>",
        );

        tracing::debug!("insert_second_child ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Should have Insert ops
        assert!(
            ops.iter().any(|(k, _)| k == "Insert"),
            "Should have Insert operation"
        );
    }

    #[test]
    fn delete_first_child() {
        // Delete first P, leaving second
        let ops = get_ops(
            "<html><body><p>A</p><p>B</p></body></html>",
            "<html><body><p>B</p></body></html>",
        );

        tracing::debug!("delete_first_child ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        // Should have Delete or Move ops
        assert!(
            ops.iter().any(|(k, _)| k == "Delete" || k == "Move"),
            "Should have Delete or Move operation for removed element"
        );
    }

    #[test]
    fn empty_to_single_child() {
        // Insert into empty body
        let ops = get_ops(
            "<html><body></body></html>",
            "<html><body><p>New</p></body></html>",
        );

        tracing::debug!("empty_to_single_child ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        assert!(
            ops.iter().any(|(k, _)| k == "Insert"),
            "Should have Insert operation"
        );
    }

    #[test]
    fn single_child_to_empty() {
        // Delete only child
        let ops = get_ops(
            "<html><body><p>Old</p></body></html>",
            "<html><body></body></html>",
        );

        tracing::debug!("single_child_to_empty ops:");
        for (kind, p) in &ops {
            tracing::debug!("  {kind}: {p}");
        }

        assert!(
            ops.iter().any(|(k, _)| k == "Delete"),
            "Should have Delete operation"
        );
    }
}

// =============================================================================
// DOM Path Extraction Tests
// =============================================================================
//
// These tests verify that we correctly extract DOM paths from facet-diff paths.
// A DOM path is a sequence of child indices: [0, 2, 1] means "first child, then
// third child, then second child".
//
// The key insight: we walk the TYPE STRUCTURE alongside the path segments.
// The current type's shape tells us what each segment means:
//
// - Index on Def::Option → unwrap, NOT a DOM index
// - Index on Def::List (flattened children) → IS a DOM index
// - Index on enum (after Variant) → tuple field access, NOT a DOM index
// - Field → navigate to field, check its attributes
// - Variant → select enum variant
//
// What makes a list "children" vs "attributes"? Look at what's INSIDE:
// - If items are elements (have structure/children) → DOM children
// - If items' fields are all html::attribute → attributes

mod dom_path_extraction {
    use facet_html_diff::{NodePath, Patch, diff_html};

    /// Extract DOM paths from patches for a given diff
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

    // =========================================================================
    // SIMPLE TEXT CHANGES
    // =========================================================================

    #[test]
    fn body_text_dom_path() {
        // <body>Hello</body> -> <body>World</body>
        // The text is body's first child (index 0)
        // DOM path should be [0] - first child of body
        let paths = get_dom_paths(
            "<html><body>Hello</body></html>",
            "<html><body>World</body></html>",
        );

        tracing::debug!("body_text_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        // Should have SetText with path [0] (first child of body)
        let set_text = paths.iter().find(|(k, _)| k == "SetText");
        assert!(set_text.is_some(), "Should have SetText patch");
        assert_eq!(
            set_text.unwrap().1.0,
            vec![0],
            "Text node should be at DOM path [0]"
        );
    }

    #[test]
    fn p_text_dom_path() {
        // <body><p>A</p></body> -> <body><p>B</p></body>
        // The P is body's first child (index 0)
        // The text is P's first child (index 0)
        // DOM path for text should be [0, 0]
        let paths = get_dom_paths(
            "<html><body><p>A</p></body></html>",
            "<html><body><p>B</p></body></html>",
        );

        tracing::debug!("p_text_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_text = paths.iter().find(|(k, _)| k == "SetText");
        assert!(set_text.is_some(), "Should have SetText patch");
        assert_eq!(
            set_text.unwrap().1.0,
            vec![0, 0],
            "Text inside P should be at DOM path [0, 0]"
        );
    }

    #[test]
    fn second_p_text_dom_path() {
        // <body><p>A</p><p>B</p></body> -> <body><p>A</p><p>X</p></body>
        // Second P is at index 1, its text is at index 0
        // DOM path should be [1, 0]
        let paths = get_dom_paths(
            "<html><body><p>A</p><p>B</p></body></html>",
            "<html><body><p>A</p><p>X</p></body></html>",
        );

        tracing::debug!("second_p_text_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_text = paths.iter().find(|(k, _)| k == "SetText");
        assert!(set_text.is_some(), "Should have SetText patch");
        assert_eq!(
            set_text.unwrap().1.0,
            vec![1, 0],
            "Text inside second P should be at DOM path [1, 0]"
        );
    }

    // =========================================================================
    // NESTED ELEMENTS
    // =========================================================================

    #[test]
    fn nested_div_p_text_dom_path() {
        // <body><div><p>A</p></div></body> -> <body><div><p>B</p></div></body>
        // Div is body[0], P is div[0], text is p[0]
        // DOM path should be [0, 0, 0]
        let paths = get_dom_paths(
            "<html><body><div><p>A</p></div></body></html>",
            "<html><body><div><p>B</p></div></body></html>",
        );

        tracing::debug!("nested_div_p_text_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_text = paths.iter().find(|(k, _)| k == "SetText");
        assert!(set_text.is_some(), "Should have SetText patch");
        assert_eq!(
            set_text.unwrap().1.0,
            vec![0, 0, 0],
            "Text inside nested P should be at DOM path [0, 0, 0]"
        );
    }

    #[test]
    fn second_child_of_div_dom_path() {
        // <body><div><p>A</p><p>B</p></div></body> -> <body><div><p>A</p><p>X</p></div></body>
        // Div is body[0], second P is div[1], text is p[0]
        // DOM path should be [0, 1, 0]
        let paths = get_dom_paths(
            "<html><body><div><p>A</p><p>B</p></div></body></html>",
            "<html><body><div><p>A</p><p>X</p></div></body></html>",
        );

        tracing::debug!("second_child_of_div_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_text = paths.iter().find(|(k, _)| k == "SetText");
        assert!(set_text.is_some(), "Should have SetText patch");
        assert_eq!(
            set_text.unwrap().1.0,
            vec![0, 1, 0],
            "Text inside second P of div should be at DOM path [0, 1, 0]"
        );
    }

    // =========================================================================
    // ATTRIBUTES
    // =========================================================================

    #[test]
    fn attribute_on_first_child_dom_path() {
        // <body><p>Text</p></body> -> <body><p class="foo">Text</p></body>
        // The P is at body[0], attribute is ON that element
        // DOM path for attribute should be [0]
        let paths = get_dom_paths(
            "<html><body><p>Text</p></body></html>",
            r#"<html><body><p class="foo">Text</p></body></html>"#,
        );

        tracing::debug!("attribute_on_first_child_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_attr = paths.iter().find(|(k, _)| k == "SetAttribute");
        assert!(set_attr.is_some(), "Should have SetAttribute patch");
        assert_eq!(
            set_attr.unwrap().1.0,
            vec![0],
            "Attribute on P should target DOM path [0]"
        );
    }

    #[test]
    fn attribute_on_second_child_dom_path() {
        // <body><p>A</p><p>B</p></body> -> <body><p>A</p><p id="x">B</p></body>
        // Second P is at body[1]
        // DOM path for attribute should be [1]
        let paths = get_dom_paths(
            "<html><body><p>A</p><p>B</p></body></html>",
            r#"<html><body><p>A</p><p id="x">B</p></body></html>"#,
        );

        tracing::debug!("attribute_on_second_child_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_attr = paths.iter().find(|(k, _)| k == "SetAttribute");
        assert!(set_attr.is_some(), "Should have SetAttribute patch");
        assert_eq!(
            set_attr.unwrap().1.0,
            vec![1],
            "Attribute on second P should target DOM path [1]"
        );
    }

    #[test]
    fn attribute_on_nested_element_dom_path() {
        // <body><div><p>Text</p></div></body> -> <body><div><p id="x">Text</p></div></body>
        // Div is body[0], P is div[0]
        // DOM path for attribute should be [0, 0]
        let paths = get_dom_paths(
            "<html><body><div><p>Text</p></div></body></html>",
            r#"<html><body><div><p id="x">Text</p></div></body></html>"#,
        );

        tracing::debug!("attribute_on_nested_element_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_attr = paths.iter().find(|(k, _)| k == "SetAttribute");
        assert!(set_attr.is_some(), "Should have SetAttribute patch");
        assert_eq!(
            set_attr.unwrap().1.0,
            vec![0, 0],
            "Attribute on nested P should target DOM path [0, 0]"
        );
    }

    // =========================================================================
    // MIXED CONTENT
    // =========================================================================

    #[test]
    fn text_before_element_dom_path() {
        // <body>Hello<p>World</p></body> -> <body>Hi<p>World</p></body>
        // Text is body[0], P is body[1]
        // Changed text should be at [0]
        let paths = get_dom_paths(
            "<html><body>Hello<p>World</p></body></html>",
            "<html><body>Hi<p>World</p></body></html>",
        );

        tracing::debug!("text_before_element_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_text = paths.iter().find(|(k, _)| k == "SetText");
        assert!(set_text.is_some(), "Should have SetText patch");
        assert_eq!(
            set_text.unwrap().1.0,
            vec![0],
            "Text before P should be at DOM path [0]"
        );
    }

    #[test]
    fn text_after_element_dom_path() {
        // <body><p>First</p>Second</body> -> <body><p>First</p>Changed</body>
        // P is body[0], text is body[1]
        // Changed text should be at [1]
        let paths = get_dom_paths(
            "<html><body><p>First</p>Second</body></html>",
            "<html><body><p>First</p>Changed</body></html>",
        );

        tracing::debug!("text_after_element_dom_path patches:");
        for (kind, path) in &paths {
            tracing::debug!("  {kind}: {:?}", path.0);
        }

        let set_text = paths.iter().find(|(k, _)| k == "SetText");
        assert!(set_text.is_some(), "Should have SetText patch");
        assert_eq!(
            set_text.unwrap().1.0,
            vec![1],
            "Text after P should be at DOM path [1]"
        );
    }
}
