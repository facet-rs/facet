//! UI tests for compile-time error diagnostics.
//!
//! These tests verify that macro error messages point to the correct source locations,
//! enabling IDE features like hover, go-to-definition, and proper error highlighting.

#[test]
fn ui_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
