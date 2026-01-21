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

#[test]
fn move_and_insert_interaction() {
    // Proptest found this case: moves and inserts interacting
    assert_roundtrip(
        r#"<html><body><span>A</span><div></div></body></html>"#,
        r#"<html><body><div> </div><span>0</span>0</body></html>"#,
    );
}
