//! Integration test harness and runner for LSP extensions.
//!
//! This module provides tools for testing LSP extensions through real IPC
//! (roam over stdio), using the actual `StyxLspHostImpl` implementation.
//!
//! # Quick Start
//!
//! Write test cases in a `.styx` file:
//!
//! ```styx
//! tests [
//!     @test {
//!         name "table completions after from"
//!         input "from |"
//!         completions { has (product user order) }
//!     }
//! ]
//! ```
//!
//! Then run them in a Rust test:
//!
//! ```ignore
//! #[tokio::test]
//! async fn test_dibs_completions() {
//!     styx_lsp::testing::assert_test_file(
//!         env!("CARGO_BIN_EXE_dibs"),
//!         &["lsp-extension"],
//!         "tests/completions.styx",
//!         "crate:dibs-queries@1",
//!     ).await;
//! }
//! ```

mod harness;
mod runner;

pub use harness::{CursorInfo, HarnessError, TestDocument, TestHarness};
pub use runner::{
    RunnerError, TestFileResult, TestResult, assert_test_file, assert_test_file_with_uri,
    run_test_file, run_test_file_with_uri,
};
