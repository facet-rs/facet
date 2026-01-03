// Regression tests for GitHub issues
// Run with: cargo +nightly miri test -p facet-html --test minimal_repro

use facet_html_dom::Html;

// Issue #1568: Crash during error cleanup
#[test]
fn issue_1568_html_parse_error_cleanup() {
    // Simplified HTML that previously triggered a crash during error cleanup.
    let html = r#"<ul><li>text <code>code</code></li></ul>"#;

    // This should NOT crash during parsing or cleanup
    let result = facet_html::from_str::<Html>(html);

    // After the fix for #1575, this should parse successfully
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
}

// Issue #1575: facet-html crashes on <li> with parentheses
// Root cause: Vec<Li> fields in Ul/Ol structs were missing #[facet(xml::elements)]
// attribute, which is required to properly group repeated child elements into a Vec.
#[test]
fn issue_1575_li_with_parentheses() {
    // This HTML previously crashed with SIGABRT when parsing
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
<ul>
<li><code>index.html</code> - renders the root section (<code>/</code>)</li>
</ul>
</body>
</html>"#;

    // This should parse successfully (not just not crash)
    let result = facet_html::from_str::<Html>(html);
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
}

#[test]
fn issue_1575_simple_li_with_parentheses() {
    let html = r#"<ul><li>Some text (with parentheses)</li></ul>"#;

    let result = facet_html::from_str::<Html>(html);
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
}

#[test]
fn issue_1575_li_with_description_and_parentheses() {
    let html = r#"<ul><li>Item - description (detail)</li></ul>"#;

    let result = facet_html::from_str::<Html>(html);
    assert!(result.is_ok(), "Parsing should succeed: {:?}", result.err());
}

#[test]
fn issue_1575_li_with_mixed_content() {
    use facet_html_dom::{FlowContent, Ul};

    // Test that mixed content (text + elements) in <li> is preserved correctly
    let html = r#"<ul><li><code>a</code> text (<code>b</code>)</li></ul>"#;

    let result = facet_html::from_str::<Ul>(html).expect("should parse");
    assert_eq!(result.li.len(), 1);

    // Verify the mixed content is parsed correctly
    let children = &result.li[0].children;
    assert_eq!(children.len(), 4); // code, text, code, text

    // First child should be code element
    assert!(matches!(&children[0], FlowContent::Code(_)));
    // Second child should be text
    assert!(matches!(&children[1], FlowContent::Text(_)));
    // Third child should be code element
    assert!(matches!(&children[2], FlowContent::Code(_)));
    // Fourth child should be text
    assert!(matches!(&children[3], FlowContent::Text(_)));
}
