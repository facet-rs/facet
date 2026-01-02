//! Integration tests that parse real-world HTML fixture files.
//!
//! These tests verify that the parser can handle complex, real-world HTML
//! without panicking or producing errors.

use facet::Facet;
use facet_format::FormatDeserializer;
use facet_html::HtmlParser;
use std::fs;
use std::path::Path;

/// A minimal HTML document structure for parsing tests.
#[derive(Debug, Facet)]
struct MinimalDoc {
    #[facet(default)]
    head: Option<MinimalHead>,
    #[facet(default)]
    body: Option<MinimalBody>,
}

#[derive(Debug, Facet)]
struct MinimalHead {
    #[facet(default)]
    title: Option<String>,
}

#[derive(Debug, Facet)]
struct MinimalBody {
    #[facet(facet_xml::text, default)]
    text: String,
}

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Test that we can at least parse (tokenize) all fixture files without panicking.
#[test]
fn parse_all_fixtures_without_panic() {
    let fixtures = fixtures_dir();
    if !fixtures.exists() {
        eprintln!("Fixtures directory doesn't exist, skipping test");
        return;
    }

    let mut count = 0;
    let mut errors = Vec::new();

    for entry in fs::read_dir(&fixtures).expect("Failed to read fixtures directory") {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "html") {
            count += 1;
            let filename = path.file_name().unwrap().to_string_lossy();
            let content = fs::read(&path).expect("Failed to read fixture file");

            // Test that parsing doesn't panic
            let result = std::panic::catch_unwind(|| {
                let parser = HtmlParser::new(&content);
                let mut deserializer = FormatDeserializer::new(parser);
                // Try to deserialize - we don't care about the result, just that it doesn't panic
                let _: Result<MinimalDoc, _> = deserializer.deserialize();
            });

            if result.is_err() {
                errors.push(format!("Panic while parsing: {}", filename));
            }
        }
    }

    assert!(count > 0, "No fixture files found in {:?}", fixtures);
    assert!(
        errors.is_empty(),
        "Errors parsing fixtures:\n{}",
        errors.join("\n")
    );
    println!("Successfully parsed {} fixture files", count);
}

/// Test that all fixtures produce valid ParseEvent streams.
#[test]
fn all_fixtures_produce_valid_events() {
    use facet_format::FormatParser;

    let fixtures = fixtures_dir();
    if !fixtures.exists() {
        return;
    }

    let mut count = 0;

    for entry in fs::read_dir(&fixtures).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "html") {
            count += 1;
            let content = fs::read(&path).unwrap();

            let mut parser = HtmlParser::new(&content);

            // Consume all events - this will error if any event is malformed
            let mut event_count = 0;
            loop {
                match parser.next_event() {
                    Ok(Some(_)) => event_count += 1,
                    Ok(None) => break,
                    Err(e) => {
                        panic!(
                            "Error parsing {}: {:?}",
                            path.file_name().unwrap().to_string_lossy(),
                            e
                        );
                    }
                }
            }

            // Each file should produce at least one event
            assert!(
                event_count > 0,
                "No events from {}",
                path.file_name().unwrap().to_string_lossy()
            );
        }
    }

    assert!(count > 0);
}

/// Regression test for issue #1568: facet-html panics during error cleanup.
///
/// This test verifies that when the HTML parser encounters a type mismatch,
/// it returns an error cleanly instead of panicking with SIGABRT during the
/// Drop implementation of Partial<true>.
///
/// TODO: Remove #[ignore] once the issue is fixed.
#[test]
#[ignore = "SIGABRT crash - see issue #1568"]
fn issue_1568_no_panic_on_error_cleanup() {
    use facet_html::elements::Html;

    // Load the HTML fixture that triggers the bug
    let fixture_path = fixtures_dir().join("issue-1568.html");
    let html = fs::read(&fixture_path).expect("Failed to read issue-1568.html fixture");

    // For now, just test that it doesn't panic
    // TODO: This should succeed once we fix the parsing issue
    let result = std::panic::catch_unwind(|| {
        facet_html::from_str::<Html>(std::str::from_utf8(&html).unwrap())
    });

    // Verify we didn't panic
    assert!(
        result.is_ok(),
        "Parser panicked during error cleanup (issue #1568)"
    );
}
