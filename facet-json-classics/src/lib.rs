//! Classic JSON benchmark fixtures from nativejson-benchmark.
//!
//! Provides compressed fixtures and helper functions for benchmarking JSON parsers.
//! Files are stored as brotli-compressed data and decompressed on demand.

use std::sync::LazyLock;

/// Decompress a brotli-compressed fixture
fn decompress(compressed: &[u8]) -> Vec<u8> {
    let mut decompressed = Vec::new();
    brotli::BrotliDecompress(&mut std::io::Cursor::new(compressed), &mut decompressed)
        .expect("Failed to decompress fixture");
    decompressed
}

/// citm_catalog.json - Event catalog data (~1.7MB uncompressed)
///
/// Contains event/performance data with many nested structs, hashmaps, and arrays.
pub static CITM_CATALOG: LazyLock<String> = LazyLock::new(|| {
    let compressed = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/citm_catalog.json.br"));
    String::from_utf8(decompress(compressed)).expect("citm_catalog.json should be valid UTF-8")
});

/// twitter.json - Social media API response (~632KB uncompressed)
///
/// Contains tweet data with nested user objects, entities, and optional fields.
pub static TWITTER: LazyLock<String> = LazyLock::new(|| {
    let compressed = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/twitter.json.br"));
    String::from_utf8(decompress(compressed)).expect("twitter.json should be valid UTF-8")
});

/// canada.json - GeoJSON polygon coordinates (~2.3MB uncompressed)
///
/// Contains deeply nested arrays of floating-point coordinates.
pub static CANADA: LazyLock<String> = LazyLock::new(|| {
    let compressed = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/canada.json.br"));
    String::from_utf8(decompress(compressed)).expect("canada.json should be valid UTF-8")
});
