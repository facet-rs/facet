//! Test case metadata.
//!
//! This module defines the test result type used across all test modules.

/// Result of running a test case.
pub struct TestResult {
    /// Whether the test passed.
    pub passed: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

impl TestResult {
    /// Create a passing result.
    pub fn pass() -> Self {
        Self {
            passed: true,
            error: None,
        }
    }

    /// Create a failing result with an error message.
    pub fn fail(msg: impl Into<String>) -> Self {
        Self {
            passed: false,
            error: Some(msg.into()),
        }
    }
}
