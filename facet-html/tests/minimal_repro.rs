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

// Issue #1578: Round-trip serialization fails for script tags
// The problem was that script tags with `src` attribute were serializing attributes
// as child elements like `<script><src>/js/app.js</src></script>` instead of
// `<script src="/js/app.js"></script>`, which could not be re-parsed.
#[test]
fn issue_1578_script_roundtrip() {
    let input = r#"<html><body><script src="/js/app.js"></script></body></html>"#;

    // Parse it
    let doc: Html = facet_html::from_str(input).expect("Initial parse failed");

    // Serialize it
    let serialized = facet_html::to_string(&doc).expect("Serialization failed");

    // The serialized output should have src as an attribute, not a child element
    assert!(
        serialized.contains(r#"src="/js/app.js""#),
        "src should be serialized as an attribute, got: {}",
        serialized
    );
    assert!(
        !serialized.contains("<src>"),
        "src should NOT be serialized as a child element, got: {}",
        serialized
    );

    // Try to parse the serialized output (this is the core bug)
    let reparsed: Html =
        facet_html::from_str(&serialized).expect("Round-trip parse failed - this is issue #1578");

    // Verify the script tag data is preserved
    let body = reparsed.body.expect("body should exist");
    assert!(!body.children.is_empty(), "body should have children");
}

#[test]
fn issue_1578_script_with_inline_content() {
    let input = r#"<script>console.log("hello");</script>"#;

    // Parse it
    let doc: facet_html_dom::Script = facet_html::from_str(input).expect("Initial parse failed");
    assert_eq!(doc.text, r#"console.log("hello");"#);

    // Serialize it
    let serialized = facet_html::to_string(&doc).expect("Serialization failed");

    // Round-trip
    let reparsed: facet_html_dom::Script =
        facet_html::from_str(&serialized).expect("Round-trip failed");
    assert_eq!(reparsed.text, r#"console.log("hello");"#);
}
