//! Generate test cases for WASM browser tests.
//!
//! Run with: cargo test -p facet-html-diff --test generate_wasm_tests -- --ignored --nocapture

use facet::Facet;
use facet_html_diff::diff_html;
use std::fs;
use std::path::Path;

#[derive(Facet)]
struct TestCase {
    name: String,
    old_body_html: String,
    new_body_html: String,
    patches_json: String,
}

/// Extract body innerHTML from full HTML document.
fn extract_body_inner(html: &str) -> String {
    let start = html.find("<body>").map(|i| i + 6).unwrap_or(0);
    let end = html.rfind("</body>").unwrap_or(html.len());
    html[start..end].to_string()
}

/// Generate a test case from old/new HTML.
fn make_test_case(name: &str, old_html: &str, new_html: &str) -> Option<TestCase> {
    let patches = diff_html(old_html, new_html).ok()?;
    let patches_json = facet_json::to_string(&patches).ok()?;

    Some(TestCase {
        name: name.to_string(),
        old_body_html: extract_body_inner(old_html),
        new_body_html: extract_body_inner(new_html),
        patches_json,
    })
}

#[test]
#[ignore] // Run manually to generate test cases
fn generate_test_cases() {
    let mut cases: Vec<TestCase> = Vec::new();

    // Simple text changes
    if let Some(tc) = make_test_case(
        "simple_text_change",
        "<html><body><p>Hello</p></body></html>",
        "<html><body><p>World</p></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "text_in_div",
        "<html><body><div>Old text</div></body></html>",
        "<html><body><div>New text</div></body></html>",
    ) {
        cases.push(tc);
    }

    // Attribute changes
    if let Some(tc) = make_test_case(
        "add_class",
        "<html><body><div>Content</div></body></html>",
        "<html><body><div class=\"highlight\">Content</div></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "change_class",
        "<html><body><div class=\"old\">Content</div></body></html>",
        "<html><body><div class=\"new\">Content</div></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "remove_class",
        "<html><body><div class=\"remove-me\">Content</div></body></html>",
        "<html><body><div>Content</div></body></html>",
    ) {
        cases.push(tc);
    }

    // Element insertion
    if let Some(tc) = make_test_case(
        "insert_element_at_end",
        "<html><body><p>First</p></body></html>",
        "<html><body><p>First</p><p>Second</p></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "insert_element_at_start",
        "<html><body><p>Second</p></body></html>",
        "<html><body><p>First</p><p>Second</p></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "insert_element_in_middle",
        "<html><body><p>First</p><p>Third</p></body></html>",
        "<html><body><p>First</p><p>Second</p><p>Third</p></body></html>",
    ) {
        cases.push(tc);
    }

    // Element removal
    if let Some(tc) = make_test_case(
        "remove_element_from_end",
        "<html><body><p>First</p><p>Second</p></body></html>",
        "<html><body><p>First</p></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "remove_element_from_start",
        "<html><body><p>First</p><p>Second</p></body></html>",
        "<html><body><p>Second</p></body></html>",
    ) {
        cases.push(tc);
    }

    // Empty element transitions
    if let Some(tc) = make_test_case(
        "fill_empty_div",
        "<html><body><div></div></body></html>",
        "<html><body><div>Content</div></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "drain_div_content",
        "<html><body><div>Content</div></body></html>",
        "<html><body><div></div></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "text_moves_into_div",
        "<html><body>Text<div></div></body></html>",
        "<html><body><div>Text</div></body></html>",
    ) {
        cases.push(tc);
    }

    // Nested changes
    if let Some(tc) = make_test_case(
        "nested_text_change",
        "<html><body><div><p>Old</p></div></body></html>",
        "<html><body><div><p>New</p></div></body></html>",
    ) {
        cases.push(tc);
    }

    if let Some(tc) = make_test_case(
        "deeply_nested",
        "<html><body><div><div><div>Deep</div></div></div></body></html>",
        "<html><body><div><div><div>Changed</div></div></div></body></html>",
    ) {
        cases.push(tc);
    }

    // Multiple changes
    if let Some(tc) = make_test_case(
        "multiple_text_changes",
        "<html><body><p>A</p><p>B</p><p>C</p></body></html>",
        "<html><body><p>X</p><p>Y</p><p>Z</p></body></html>",
    ) {
        cases.push(tc);
    }

    // Sibling reordering
    if let Some(tc) = make_test_case(
        "swap_siblings",
        "<html><body><p>First</p><p>Second</p></body></html>",
        "<html><body><p>Second</p><p>First</p></body></html>",
    ) {
        cases.push(tc);
    }

    // Mixed content
    if let Some(tc) = make_test_case(
        "text_and_elements",
        "<html><body>Text<span>Span</span></body></html>",
        "<html><body><span>Span</span>Text</body></html>",
    ) {
        cases.push(tc);
    }

    // Write to file
    let output_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("facet-html-diff-wasm/test-cases.json");

    let json = facet_json::to_string(&cases).expect("serialize test cases");
    fs::write(&output_path, json).expect("write test-cases.json");

    println!("Generated {} test cases to {:?}", cases.len(), output_path);
}
