//! QA harness for the vox Cranelift JIT.
//!
//! Provides:
//! - `DifferentialHarness`: runs same bytes through oracle and candidate,
//!   asserts they agree on output and error class.
//! - `ErrorClass`: canonical error bucket for differential comparison.
//! - Test fixtures: typed inputs with known encodings.
//! - Failure-mode corpus: hand-crafted malformed byte sequences.

pub mod corpus;
pub mod differential;
pub mod fixtures;
pub mod fuzz;

#[cfg(test)]
mod tests;
