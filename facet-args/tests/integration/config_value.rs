//! Minimal test for metadata_container with span tracking.

use facet::Facet;
use facet_testhelpers::test;
use tracing::info;

/// A span in source text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
pub struct Span {
    pub offset: usize,
    pub len: usize,
}

/// A string value with span tracking.
#[derive(Debug, Clone, Facet)]
#[facet(metadata_container)]
pub struct Spanned<T> {
    pub value: T,
    #[facet(metadata = "span")]
    pub span: Option<Span>,
}

#[test]
fn test_spanned_string_from_json() {
    // JSON:  {"name": "hello", "count": 42}
    // Bytes: 0123456789...
    //                 ^     ^         ^^
    //                 9     15        27 29
    let json = r#"{"name": "hello", "count": 42}"#;

    #[derive(Debug, Facet)]
    struct Config {
        name: Spanned<String>,
        count: Spanned<i64>,
    }

    let config: Config = facet_json::from_str(json).expect("should parse");

    assert_eq!(config.name.value, "hello");
    assert_eq!(config.count.value, 42);

    // Verify spans are populated (not None)
    let name_span = config.name.span.expect("name should have a span");
    let count_span = config.count.span.expect("count should have a span");

    info!(?name_span, "name span");
    info!(?count_span, "count span");

    // Verify the spans point to the right places in the JSON
    // "hello" starts at offset 9 and is 7 bytes (including quotes)
    assert_eq!(name_span.offset, 9);
    assert_eq!(name_span.len, 7);

    // 42 starts at offset 27 and is 2 bytes
    assert_eq!(count_span.offset, 27);
    assert_eq!(count_span.len, 2);

    // Verify we can extract the original text using the span
    let name_text = &json[name_span.offset..name_span.offset + name_span.len];
    assert_eq!(name_text, r#""hello""#);

    let count_text = &json[count_span.offset..count_span.offset + count_span.len];
    assert_eq!(count_text, "42");
}
