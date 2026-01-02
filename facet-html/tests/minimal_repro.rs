// Minimal reproduction for issue #1568
// Run with: cargo +nightly miri test -p facet-html --test minimal_repro

use facet_html::elements::Html;

#[test]
fn minimal_html_parse_error_cleanup() {
    // Simplified HTML that previously triggered a crash during error cleanup.
    // The important thing is that it doesn't crash - whether it parses
    // successfully or returns an error is secondary.
    let html = r#"<ul><li>text <code>code</code></li></ul>"#;

    // This should NOT crash during parsing or cleanup
    let result = facet_html::from_str::<Html>(html);

    // Log the result for debugging
    match &result {
        Ok(_) => println!("Parsing succeeded"),
        Err(e) => println!("Parsing returned error (acceptable): {}", e),
    }
    // The test passes as long as we didn't crash
}
