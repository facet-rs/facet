//! Compile-time error tests for the #[rapace::service] macro.
//!
//! These tests use trybuild to ensure the macro properly rejects invalid inputs
//! with helpful error messages.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/fail/*.rs");
}
