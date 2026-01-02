// Minimal reproduction for issue #1568
// Run with: cargo +nightly miri test -p facet-html --test minimal_repro

use facet_html::elements::Html;

#[test]
fn minimal_html_parse_error_cleanup() {
    // Simplified HTML that triggers the same error path
    let html = r#"<ul><li>text <code>code</code></li></ul>"#;

    // This should return an error, not crash
    let result = facet_html::from_str::<Html>(html);

    // The parse should fail with an error
    assert!(result.is_err(), "Expected parse error");
}
