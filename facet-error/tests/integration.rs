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
    InvalidHeader { expected: String, found: String },

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
        expected: "application/json".to_string(),
        found: "text/html".to_string(),
    };

    let display = format!("{err}");
    assert!(display.contains("expected application/json"));
    assert!(display.contains("found text/html"));
}

#[test]
fn test_unit_variant_with_custom_message() {
    let err = ErrorWithSource::Unknown;
    assert_eq!(format!("{err}"), "unknown error");
}

/// Test struct error (not enum)
#[derive(Facet, Debug)]
#[facet(derive(Error))]
pub struct StructError {
    pub code: i32,
    pub message: String,
}

#[test]
fn test_struct_error_display() {
    let err = StructError {
        code: 404,
        message: "Not Found".to_string(),
    };

    // Struct errors use the type name by default
    let display = format!("{err}");
    assert!(!display.is_empty());
}
