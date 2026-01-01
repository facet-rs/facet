//! Test for #[facet(error::from)] and #[facet(error::source)] attributes

use facet::Facet;
use facet_error as error; // Import to enable error:: syntax in facet attributes
use std::error::Error;
use std::fmt;

/// A mock IO error for testing
#[derive(Debug, Facet)]
pub struct MockIoError {
    message: String,
}

impl fmt::Display for MockIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IO error: {}", self.message)
    }
}

impl Error for MockIoError {}

/// Test error with error::from attribute
#[derive(Facet, Debug)]
#[facet(derive(Error))]
#[repr(u8)]
pub enum TestError {
    /// data store disconnected
    Disconnect(#[facet(error::from)] MockIoError),

    /// invalid header (expected {expected}, found {found})
    InvalidHeader { expected: String, found: String },

    /// unknown error
    Unknown,
}

#[test]
fn test_from_impl_generated() {
    let io_err = MockIoError {
        message: "connection refused".to_string(),
    };
    let err: TestError = io_err.into();
    match err {
        TestError::Disconnect(_) => {}
        _ => panic!("Expected Disconnect variant"),
    }
}

#[test]
fn test_source_impl() {
    let io_err = MockIoError {
        message: "connection refused".to_string(),
    };

    let err = TestError::Disconnect(io_err);

    // Should have a source
    assert!(err.source().is_some());

    // Unknown should not have a source
    let err2 = TestError::Unknown;
    assert!(err2.source().is_none());
}

#[test]
fn test_display_with_from() {
    let io_err = MockIoError {
        message: "connection refused".to_string(),
    };

    let err = TestError::Disconnect(io_err);
    assert_eq!(format!("{err}"), "data store disconnected");
}

#[test]
fn test_display_with_struct_variant() {
    let err = TestError::InvalidHeader {
        expected: "application/json".to_string(),
        found: "text/html".to_string(),
    };
    let _display = format!("{err}");
}
