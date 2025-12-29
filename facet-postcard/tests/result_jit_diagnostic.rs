//! Test for Result<T, E> JIT diagnostics and support.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::{from_slice, to_vec};

#[derive(Debug, PartialEq, Facet)]
struct SimpleResult {
    value: Result<i32, String>,
}

#[derive(Debug, PartialEq, Facet)]
struct ComplexResult {
    data: Result<Vec<i32>, String>,
}

#[test]
fn test_result_simple_ok() {
    let input = SimpleResult { value: Ok(42) };
    let bytes = to_vec(&input).unwrap();
    let output: SimpleResult = from_slice(&bytes).unwrap();
    assert_eq!(input, output);
}

#[test]
fn test_result_simple_err() {
    let input = SimpleResult {
        value: Err("error message".to_string()),
    };
    let bytes = to_vec(&input).unwrap();
    let output: SimpleResult = from_slice(&bytes).unwrap();
    assert_eq!(input, output);
}

#[test]
fn test_result_complex_ok() {
    let input = ComplexResult {
        data: Ok(vec![1, 2, 3]),
    };
    let bytes = to_vec(&input).unwrap();
    let output: ComplexResult = from_slice(&bytes).unwrap();
    assert_eq!(input, output);
}

#[test]
fn test_result_complex_err() {
    let input = ComplexResult {
        data: Err("failed".to_string()),
    };
    let bytes = to_vec(&input).unwrap();
    let output: ComplexResult = from_slice(&bytes).unwrap();
    assert_eq!(input, output);
}
