//! Integration tests for facet-error

use facet::Facet;

/// A simple test error
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum SimpleError {
    /// something went wrong
    Unknown,

    /// invalid value: {0}
    InvalidValue(String),
}

#[test]
fn test_display_unit_variant() {
    let err = SimpleError::Unknown;
    assert_eq!(format!("{err}"), "something went wrong");
}

#[test]
fn test_display_tuple_variant() {
    let err = SimpleError::InvalidValue("bad".to_string());
    // Note: {0} interpolation requires the plugin to support it
    assert!(format!("{err}").contains("invalid value"));
}

/// Error with struct variant (for now, avoid opaque types until namespace attrs work)
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum ErrorWithSource {
    /// network error: {0}
    Network(String),

    /// invalid header (expected {expected}, found {found})
    InvalidHeader { _expected: String, _found: String },

    /// unknown error
    Unknown,
}

#[test]
fn test_tuple_variant_with_interpolation() {
    // Test tuple variant with {0} interpolation
    let err = ErrorWithSource::Network("connection refused".to_string());
    let display = format!("{err}");
    assert!(display.contains("network error"));
    assert!(display.contains("connection refused"));
}

#[test]
fn test_struct_variant_display() {
    let err = ErrorWithSource::InvalidHeader {
        _expected: "application/json".to_string(),
        _found: "text/html".to_string(),
    };

    // Use fields directly to avoid unused warnings
    if let ErrorWithSource::InvalidHeader { _expected, _found } = &err {
        let _ = (_expected, _found);
    }

    let display = format!("{err}");
    assert!(display.contains("expected application/json"));
    assert!(display.contains("found text/html"));
}

#[test]
fn test_unit_variant_with_custom_message() {
    let err = ErrorWithSource::Unknown;
    assert_eq!(format!("{err}"), "unknown error");
}

// TODO: Add struct error test once template supports structs
// Currently commenting out because the template generates a match
// statement that causes non-exhaustive pattern errors for structs
