//! Roundtrip tests for HTML fixtures using datatest-stable.
//!
//! Each HTML fixture file becomes an individual test case, verifying that:
//! 1. The file can be parsed into an Html document
//! 2. The document can be serialized back to HTML
//! 3. The serialized HTML can be re-parsed
//! 4. Re-serialization produces identical output (idempotence)
//!
//! Note: We don't compare the original parsed structure with the reparsed structure
//! because serialization normalizes whitespace. The key property we test is that
//! once normalized, the roundtrip is stable (same output on each serialization).
//!
//! We also don't do structural comparison (assert_same!) because facet-diff's
//! recursive algorithm can cause stack overflow on deeply nested HTML documents.
//! See: https://github.com/facet-rs/facet/issues/XXXX

use facet_html_dom::Html;
use std::path::Path;

fn html_roundtrip_test(path: &Path) -> datatest_stable::Result<()> {
    let html_str = std::fs::read_to_string(path)?;

    // Step 1: Parse the original HTML
    let parsed: Html = facet_html::from_str(&html_str)
        .map_err(|e| format!("Failed to parse HTML from {}: {}", path.display(), e))?;

    // Step 2: Serialize back to HTML (this normalizes whitespace)
    let serialized =
        facet_html::to_string(&parsed).map_err(|e| format!("Failed to serialize HTML: {}", e))?;

    // Step 3: Re-parse the serialized output
    let _reparsed: Html = facet_html::from_str(&serialized)
        .map_err(|e| format!("Failed to re-parse serialized HTML: {}", e))?;

    // Step 4: Serialize again
    let reserialized = facet_html::to_string(&_reparsed)
        .map_err(|e| format!("Failed to serialize HTML again: {}", e))?;

    // Step 5: Verify serialization is idempotent - the key property we care about
    assert_eq!(
        serialized,
        reserialized,
        "Serialized HTML should be identical after roundtrip for {}",
        path.display()
    );

    Ok(())
}

datatest_stable::harness! {
    { test = html_roundtrip_test, root = "tests/fixtures", pattern = r".*\.html$" },
}
