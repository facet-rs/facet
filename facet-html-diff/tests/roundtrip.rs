//! Roundtrip tests for HTML diff.
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

#[test]
fn replace_elements_with_text() {
    assert_roundtrip(
        r#"<html><body><p>a</p><p>0</p></body></html>"#,
        r#"<html><body>A</body></html>"#,
    );
}

#[test]
fn remove_text_and_modify_sibling() {
    assert_roundtrip(
        r#"<html><body><div>a<p>A</p></div></body></html>"#,
        r#"<html><body><div><p>a</p></div></body></html>"#,
    );
}

#[test]
fn wrap_in_div() {
    assert_roundtrip(
        r#"<html><body><p>A</p></body></html>"#,
        r#"<html><body><div><p>a</p><p>A</p></div></body></html>"#,
    );
}

#[test]
fn attribute_swap() {
    assert_roundtrip(
        r#"<html><body><div id="a"></div></body></html>"#,
        r#"<html><body><div class="a"></div></body></html>"#,
    );
}

#[test]
fn change_class_remove_id() {
    assert_roundtrip(
        r#"<html><body><div class="a" id="a"></div></body></html>"#,
        r#"<html><body><div class="a-"></div></body></html>"#,
    );
}

#[test]
fn replace_p_and_text_with_text() {
    assert_roundtrip(
        r#"<html><body><p>0</p>0</body></html>"#,
        r#"<html><body> </body></html>"#,
    );
}

#[test]
fn remove_class_and_sibling() {
    assert_roundtrip(
        r#"<html><body><span class="a">0</span><span> </span></body></html>"#,
        r#"<html><body><span>A</span></body></html>"#,
    );
}

#[test]
fn text_and_spans_to_span() {
    assert_roundtrip(
        r#"<html><body> <span>0</span><span> </span></body></html>"#,
        r#"<html><body><span>A</span></body></html>"#,
    );
}

#[test]
fn add_class_and_sibling() {
    assert_roundtrip(
        r#"<html><body><p>A</p></body></html>"#,
        r#"<html><body><p class="a">A</p><p> </p></body></html>"#,
    );
}

#[test]
fn remove_div_text() {
    assert_roundtrip(
        r#"<html><body><div>0</div></body></html>"#,
        r#"<html><body><div></div></body></html>"#,
    );
}

#[test]
fn text_and_div_to_empty_div() {
    assert_roundtrip(
        r#"<html><body>A<div>0</div></body></html>"#,
        r#"<html><body><div></div></body></html>"#,
    );
}

#[test]
fn text_and_div_to_empty_div_and_text() {
    assert_roundtrip(
        r#"<html><body>A<div>0</div></body></html>"#,
        r#"<html><body><div></div> </body></html>"#,
    );
}

#[test]
fn text_moves_into_div() {
    assert_roundtrip(
        r#"<html><body>0<div></div></body></html>"#,
        r#"<html><body><div>0</div></body></html>"#,
    );
}

#[test]
fn insert_around_existing() {
    // Proptest found this case: inserting elements before and after an existing element
    assert_roundtrip(
        r#"<html><body> <span>A</span></body></html>"#,
        r#"<html><body><p>a</p><span>A</span><p>a</p></body></html>"#,
    );
}

// ============================================================================
// Simple structural tests - one operation at a time
// ============================================================================

#[test]
fn simple_swap_two_elements() {
    // [A, B] -> [B, A]
    // Just two moves, no inserts or deletes
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>B</p><p>A</p></body></html>"#,
    );
}

#[test]
fn simple_move_to_end() {
    // [A, B, C] -> [B, C, A]
    // Move first element to end
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p><p>C</p></body></html>"#,
        r#"<html><body><p>B</p><p>C</p><p>A</p></body></html>"#,
    );
}

#[test]
fn simple_move_to_start() {
    // [A, B, C] -> [C, A, B]
    // Move last element to start
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p><p>C</p></body></html>"#,
        r#"<html><body><p>C</p><p>A</p><p>B</p></body></html>"#,
    );
}

#[test]
fn simple_insert_at_start() {
    // [A, B] -> [X, A, B]
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>X</p><p>A</p><p>B</p></body></html>"#,
    );
}

#[test]
fn simple_insert_at_end() {
    // [A, B] -> [A, B, X]
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>A</p><p>B</p><p>X</p></body></html>"#,
    );
}

#[test]
fn simple_insert_in_middle() {
    // [A, B] -> [A, X, B]
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>A</p><p>X</p><p>B</p></body></html>"#,
    );
}

#[test]
fn simple_delete_from_start() {
    // [A, B, C] -> [B, C]
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p><p>C</p></body></html>"#,
        r#"<html><body><p>B</p><p>C</p></body></html>"#,
    );
}

#[test]
fn simple_delete_from_end() {
    // [A, B, C] -> [A, B]
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p><p>C</p></body></html>"#,
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
    );
}

#[test]
fn simple_delete_from_middle() {
    // [A, B, C] -> [A, C]
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p><p>C</p></body></html>"#,
        r#"<html><body><p>A</p><p>C</p></body></html>"#,
    );
}

// ============================================================================
// Two operations combined
// ============================================================================

#[test]
fn insert_then_move() {
    // [A, B] -> [X, B, A]
    // Insert X at start, move A to end
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>X</p><p>B</p><p>A</p></body></html>"#,
    );
}

#[test]
fn move_then_insert() {
    // [A, B] -> [B, X, A]
    // Move A to end, insert X in middle
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>B</p><p>X</p><p>A</p></body></html>"#,
    );
}

#[test]
fn delete_then_insert() {
    // [A, B, C] -> [X, B, C]
    // Delete A, insert X at start
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p><p>C</p></body></html>"#,
        r#"<html><body><p>X</p><p>B</p><p>C</p></body></html>"#,
    );
}

#[test]
fn insert_then_delete() {
    // [A, B, C] -> [A, X, C]
    // Delete B, insert X in its place
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p><p>C</p></body></html>"#,
        r#"<html><body><p>A</p><p>X</p><p>C</p></body></html>"#,
    );
}

#[test]
fn two_inserts() {
    // [A] -> [X, A, Y]
    assert_roundtrip(
        r#"<html><body><p>A</p></body></html>"#,
        r#"<html><body><p>X</p><p>A</p><p>Y</p></body></html>"#,
    );
}

#[test]
fn two_deletes() {
    // [A, B, C, D] -> [B, C]
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p><p>C</p><p>D</p></body></html>"#,
        r#"<html><body><p>B</p><p>C</p></body></html>"#,
    );
}

// ============================================================================
// Complex - known failing tests
// ============================================================================

#[test]
fn swap_only() {
    // [A, B] -> [B, A] - just swap, no inserts
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>B</p><p>A</p></body></html>"#,
    );
}

#[test]
fn swap_with_insert_at_end() {
    // [A, B] -> [B, A, C] - swap + insert at end
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>B</p><p>A</p><p>C</p></body></html>"#,
    );
}

#[test]
fn swap_with_insert_at_start() {
    // [A, B] -> [C, B, A] - insert at start + swap
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>C</p><p>B</p><p>A</p></body></html>"#,
    );
}

#[test]
fn swap_with_insert_in_middle() {
    // [A, B] -> [B, C, A] - swap with insert between
    assert_roundtrip(
        r#"<html><body><p>A</p><p>B</p></body></html>"#,
        r#"<html><body><p>B</p><p>C</p><p>A</p></body></html>"#,
    );
}

#[test]
fn swap_different_elements() {
    // [Span, Div] -> [Div, Span] - swap with different element types
    assert_roundtrip(
        r#"<html><body><span>A</span><div>B</div></body></html>"#,
        r#"<html><body><div>B</div><span>A</span></body></html>"#,
    );
}

#[test]
fn swap_different_elements_with_insert() {
    // [Span, Div] -> [Div, Span, Text] - swap different types + insert
    assert_roundtrip(
        r#"<html><body><span>A</span><div>B</div></body></html>"#,
        r#"<html><body><div>B</div><span>A</span>C</body></html>"#,
    );
}

#[test]
fn swap_with_empty_div() {
    // [Span, Div(empty)] -> [Div(empty), Span] - empty div
    assert_roundtrip(
        r#"<html><body><span>A</span><div></div></body></html>"#,
        r#"<html><body><div></div><span>A</span></body></html>"#,
    );
}

#[test]
fn swap_with_div_content_change() {
    // [Span, Div(empty)] -> [Div(text), Span]
    assert_roundtrip(
        r#"<html><body><span>A</span><div></div></body></html>"#,
        r#"<html><body><div>X</div><span>A</span></body></html>"#,
    );
}

#[test]
fn swap_with_text_insert() {
    // [Span, Div] -> [Div, Span, Text] - no content changes
    assert_roundtrip(
        r#"<html><body><span>A</span><div>B</div></body></html>"#,
        r#"<html><body><div>B</div><span>A</span>C</body></html>"#,
    );
}

#[test]
fn swap_with_text_insert_empty_div() {
    // [Span, Div(empty)] -> [Div(empty), Span, Text]
    assert_roundtrip(
        r#"<html><body><span>A</span><div></div></body></html>"#,
        r#"<html><body><div></div><span>A</span>C</body></html>"#,
    );
}

#[test]
fn swap_with_all_text_changes() {
    // [Span(A), Div(empty)] -> [Div(X), Span(B)] - swap + both text changes
    assert_roundtrip(
        r#"<html><body><span>A</span><div></div></body></html>"#,
        r#"<html><body><div>X</div><span>B</span></body></html>"#,
    );
}

#[test]
fn swap_with_all_text_changes_and_insert() {
    // [Span(A), Div(empty)] -> [Div(X), Span(B), Text] - swap + changes + insert
    assert_roundtrip(
        r#"<html><body><span>A</span><div></div></body></html>"#,
        r#"<html><body><div>X</div><span>B</span>C</body></html>"#,
    );
}

#[test]
fn swap_with_insert_and_text_change() {
    // [Span, Div] -> [Div, Span, Text]
    // Div moves to front, Span stays, Text inserted at end
    // Plus content changes in Span and Div
    assert_roundtrip(
        r#"<html><body><span>A</span><div></div></body></html>"#,
        r#"<html><body><div> </div><span>0</span>0</body></html>"#,
    );
}

#[test]
fn proptest_minimal_failure() {
    // Minimal failing case from proptest
    // [Div, Text("A"), P("a")] -> [Div]
    assert_roundtrip(
        r#"<html><body><div></div>A<p>a</p></body></html>"#,
        r#"<html><body><div></div></body></html>"#,
    );
}

#[test]
fn proptest_minimal_failure_2() {
    // [Div(children: [Text("0")])] -> [Text("0"), Div(children: [])]
    assert_roundtrip(
        r#"<html><body><div>0</div></body></html>"#,
        r#"<html><body>0<div></div></body></html>"#,
    );
}

#[test]
fn proptest_minimal_failure_3() {
    // [Div(children: [Text("a")])] -> [Text("a"), Div(children: [P("a")])]
    assert_roundtrip(
        r#"<html><body><div>a</div></body></html>"#,
        r#"<html><body>a<div><p>a</p></div></body></html>"#,
    );
}

#[test]
fn proptest_minimal_failure_4() {
    // More complex: nested changes inside displaced div
    assert_roundtrip(
        r#"<html><body><div>a<p>0</p></div></body></html>"#,
        r#"<html><body>a<div><div></div></div></body></html>"#,
    );
}
