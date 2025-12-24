/// Test for issue #1431: Support Spanned<T> in untagged enum variants
///
/// This test reproduces the case where:
/// - An untagged enum has a variant containing `Spanned<String>`
/// - Deserializing a simple scalar string like "1.0" should work
/// - The Spanned wrapper should be transparently unwrapped during variant matching
///
/// The issue is that the solver treats Spanned<String> as a struct variant
/// instead of recognizing it should match scalar values.
use facet::Facet;
use facet_json as json;
use facet_reflect::Spanned;

#[derive(Facet, Debug, PartialEq)]
pub struct DependencyDetail {
    pub version: String,
    pub optional: Option<bool>,
}

/// Simplified version of Cargo.toml dependency syntax
#[derive(Facet, Debug)]
#[repr(u8)]
#[facet(untagged)]
pub enum Dependency {
    /// Simple version string: "aho-corasick = \"1.0\""
    Version(Spanned<String>),
    /// Detailed specification: "aho-corasick = { version = \"1.0\", optional = true }"
    Detailed(DependencyDetail),
}

#[test]
fn test_spanned_string_in_untagged_enum() {
    // This should deserialize as Dependency::Version with span info
    let json = r#""1.0""#;

    let result: Dependency = json::from_str(json).expect("should deserialize as Version variant");

    match result {
        Dependency::Version(spanned_version) => {
            assert_eq!(*spanned_version, "1.0");
            // The span should cover the string value
            assert!(spanned_version.span.offset > 0 || spanned_version.span.len > 0);
        }
        _ => panic!("Expected Version variant, got Detailed variant"),
    }
}

#[test]
fn test_detailed_variant_still_works() {
    let json = r#"{"version":"1.0","optional":true}"#;

    let result: Dependency = json::from_str(json).expect("should deserialize as Detailed variant");

    match result {
        Dependency::Detailed(detail) => {
            assert_eq!(detail.version, "1.0");
            assert_eq!(detail.optional, Some(true));
        }
        Dependency::Version(_) => panic!("Expected Detailed variant, got Version variant"),
    }
}

#[test]
fn test_spanned_preserves_span_info() {
    // Test that we actually capture meaningful span information
    let json = r#"  "2.5.1"  "#;

    let result: Dependency = json::from_str(json).expect("should deserialize with span");

    match result {
        Dependency::Version(spanned_version) => {
            assert_eq!(*spanned_version, "2.5.1");
            // Span should have non-zero length (the actual string content)
            assert!(
                spanned_version.span.len > 0,
                "Span length should be > 0, got {}",
                spanned_version.span.len
            );
        }
        _ => panic!("Expected Version variant"),
    }
}
