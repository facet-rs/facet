//! Tests for #[facet(error::source)] attribute

use facet::Facet;
use facet_error as error;
use std::error::Error;
use std::fmt;

#[derive(Debug, Facet)]
pub struct MockError;

impl fmt::Display for MockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mock error")
    }
}

impl Error for MockError {}

// Test error::source on struct variant field
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum StructVariantError {
    /// IO error
    Io {
        #[facet(error::source)]
        cause: MockError,
    },
}

#[test]
fn test_source_struct_variant() {
    let err = StructVariantError::Io { cause: MockError };
    assert!(err.source().is_some());
}

// Test error::source on struct variant with multiple fields
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum MultiFieldError {
    /// error at line {line}
    Parse {
        line: usize,
        #[facet(error::source)]
        cause: MockError,
    },
}

#[test]
fn test_source_multi_field() {
    let err = MultiFieldError::Parse {
        line: 42,
        cause: MockError,
    };
    assert!(err.source().is_some());
}

// Test error::source on tuple variant with multiple fields
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum MultiTupleError {
    /// error
    Parse(String, #[facet(error::source)] MockError),
}

#[test]
fn test_source_multi_tuple() {
    let err = MultiTupleError::Parse("msg".to_string(), MockError);
    assert!(err.source().is_some());
}
