//! Roundtrip tests for HTML diff using datatest-stable.
//!
//! Each test case is a file in `tests/roundtrip-cases/` with format:
//! ```
//! <old HTML>
//! ===
//! <new HTML>
//! ```
//!
//! The test verifies: apply(old, diff(old, new)) == new

use facet_html_diff::{apply_patches, diff_html, parse_html};
use std::path::Path;

fn run_roundtrip_test(path: &Path) -> datatest_stable::Result<()> {
    facet_testhelpers::setup();

    let content = std::fs::read_to_string(path)?;
    let parts: Vec<&str> = content.split("\n===\n").collect();

    if parts.len() != 2 {
        return Err(format!(
            "Test file must have exactly one '===' separator, found {} parts",
            parts.len()
        )
        .into());
    }

    let old = parts[0].trim();
    let new = parts[1].trim();

    tracing::info!(%old, %new, "Starting roundtrip test");

    let patches = diff_html(old, new).map_err(|e| format!("diff failed: {e}"))?;
    tracing::info!(?patches, "Generated patches");

    let mut tree = parse_html(old).map_err(|e| format!("parse old failed: {e}"))?;
    apply_patches(&mut tree, &patches).map_err(|e| format!("apply failed: {e}"))?;
    let result = tree.to_html();

    let expected_tree = parse_html(new).map_err(|e| format!("parse new failed: {e}"))?;
    let expected = expected_tree.to_html();

    if result != expected {
        return Err(format!(
            "Roundtrip failed!\nOld: {old}\nNew: {new}\nPatches: {patches:?}\nResult: {result}\nExpected: {expected}"
        )
        .into());
    }

    Ok(())
}

datatest_stable::harness! {
    { test = run_roundtrip_test, root = "tests/roundtrip-cases", pattern = r".*\.html$" },
}
