//! Tests for facet-assert, facet-diff, and facet-pretty support for Value

use facet_assert::{Sameness, assert_same, assert_sameish, check_same, check_sameish};
use facet_diff::FacetDiff;
use facet_pretty::FacetPretty;
use facet_value::value;

// ============================================================================
// facet-assert tests
// ============================================================================

#[test]
fn test_value_vs_value_same() {
    let v1 = value!([1, 2, 3]);
    let v2 = value!([1, 2, 3]);
    assert_same!(v1, v2);
}

#[test]
fn test_value_vs_value_different() {
    let v1 = value!([1, 2, 3]);
    let v2 = value!([1, 2, 4]);
    match check_same(&v1, &v2) {
        Sameness::Different(_) => {} // expected
        Sameness::Same => panic!("expected Different, got Same"),
        Sameness::Opaque { type_name } => panic!("expected Different, got Opaque: {type_name}"),
    }
}

#[test]
fn test_value_array_vs_vec_same() {
    let v = value!([1, 2, 3]);
    let vec: Vec<i64> = vec![1, 2, 3];
    assert_sameish!(v, vec);
}

#[test]
fn test_vec_vs_value_array_same() {
    let vec: Vec<i64> = vec![1, 2, 3];
    let v = value!([1, 2, 3]);
    assert_sameish!(vec, v);
}

#[test]
fn test_value_array_vs_vec_different_length() {
    let v = value!([1, 2, 3, 4]);
    let vec: Vec<i64> = vec![1, 2, 3];
    match check_sameish(&v, &vec) {
        Sameness::Different(_) => {} // expected
        Sameness::Same => panic!("expected Different, got Same"),
        Sameness::Opaque { type_name } => panic!("expected Different, got Opaque: {type_name}"),
    }
}

#[test]
fn test_value_array_vs_vec_different_content() {
    let v = value!([1, 2, 99]);
    let vec: Vec<i64> = vec![1, 2, 3];
    match check_sameish(&v, &vec) {
        Sameness::Different(_) => {} // expected
        Sameness::Same => panic!("expected Different, got Same"),
        Sameness::Opaque { type_name } => panic!("expected Different, got Opaque: {type_name}"),
    }
}

#[test]
fn test_value_string_vs_string_same() {
    let v = value!("hello");
    let s = String::from("hello");
    assert_sameish!(v, s);
}

#[test]
fn test_value_string_vs_string_different() {
    let v = value!("hello");
    let s = String::from("world");
    match check_sameish(&v, &s) {
        Sameness::Different(_) => {} // expected
        Sameness::Same => panic!("expected Different, got Same"),
        Sameness::Opaque { type_name } => panic!("expected Different, got Opaque: {type_name}"),
    }
}

#[test]
fn test_value_number_vs_i64_same() {
    let v = value!(42);
    let n: i64 = 42;
    assert_sameish!(v, n);
}

#[test]
fn test_value_number_vs_i64_different() {
    let v = value!(42);
    let n: i64 = 43;
    match check_sameish(&v, &n) {
        Sameness::Different(_) => {} // expected
        Sameness::Same => panic!("expected Different, got Same"),
        Sameness::Opaque { type_name } => panic!("expected Different, got Opaque: {type_name}"),
    }
}

#[test]
fn test_value_bool_vs_bool_same() {
    let v = value!(true);
    let b = true;
    assert_sameish!(v, b);
}

#[test]
fn test_value_bool_vs_bool_different() {
    let v = value!(true);
    let b = false;
    match check_sameish(&v, &b) {
        Sameness::Different(_) => {} // expected
        Sameness::Same => panic!("expected Different, got Same"),
        Sameness::Opaque { type_name } => panic!("expected Different, got Opaque: {type_name}"),
    }
}

#[test]
fn test_nested_value_vs_nested_vec() {
    // Nested arrays
    let v = value!([[1, 2], [3, 4]]);
    let vec: Vec<Vec<i64>> = vec![vec![1, 2], vec![3, 4]];
    assert_sameish!(v, vec);
}

#[test]
fn test_value_object_vs_value_object_same() {
    let v1 = value!({"name": "Alice", "age": 30});
    let v2 = value!({"name": "Alice", "age": 30});
    assert_same!(v1, v2);
}

#[test]
fn test_value_object_vs_value_object_different() {
    let v1 = value!({"name": "Alice", "age": 30});
    let v2 = value!({"name": "Bob", "age": 30});
    match check_same(&v1, &v2) {
        Sameness::Different(_) => {} // expected
        Sameness::Same => panic!("expected Different, got Same"),
        Sameness::Opaque { type_name } => panic!("expected Different, got Opaque: {type_name}"),
    }
}

// ============================================================================
// facet-diff tests
// ============================================================================

#[test]
fn test_diff_value_equal() {
    let v1 = value!([1, 2, 3]);
    let v2 = value!([1, 2, 3]);
    let diff = v1.diff(&v2);
    assert!(diff.is_equal());
}

#[test]
fn test_diff_value_not_equal() {
    let v1 = value!([1, 2, 3]);
    let v2 = value!([1, 2, 4]);
    let diff = v1.diff(&v2);
    assert!(!diff.is_equal());
}

#[test]
fn test_diff_value_object_equal() {
    let v1 = value!({"a": 1, "b": 2});
    let v2 = value!({"a": 1, "b": 2});
    let diff = v1.diff(&v2);
    assert!(diff.is_equal());
}

#[test]
fn test_diff_value_object_different_value() {
    let v1 = value!({"a": 1, "b": 2});
    let v2 = value!({"a": 1, "b": 3});
    let diff = v1.diff(&v2);
    assert!(!diff.is_equal());
}

#[test]
fn test_diff_value_object_different_keys() {
    let v1 = value!({"a": 1, "b": 2});
    let v2 = value!({"a": 1, "c": 2});
    let diff = v1.diff(&v2);
    assert!(!diff.is_equal());
}

// ============================================================================
// facet-pretty tests
// ============================================================================

#[test]
fn test_pretty_value_null() {
    let v = value!(null);
    let pretty = v.pretty().to_string();
    assert!(pretty.contains("null"), "pretty output: {pretty}");
}

#[test]
fn test_pretty_value_bool() {
    let v = value!(true);
    let pretty = v.pretty().to_string();
    assert!(pretty.contains("true"), "pretty output: {pretty}");
}

#[test]
fn test_pretty_value_number() {
    let v = value!(42);
    let pretty = v.pretty().to_string();
    assert!(pretty.contains("42"), "pretty output: {pretty}");
}

#[test]
fn test_pretty_value_string() {
    let v = value!("hello");
    let pretty = v.pretty().to_string();
    assert!(pretty.contains("hello"), "pretty output: {pretty}");
}

#[test]
fn test_pretty_value_array() {
    let v = value!([1, 2, 3]);
    let pretty = v.pretty().to_string();
    // Should contain the array elements
    assert!(pretty.contains("1"), "pretty output: {pretty}");
    assert!(pretty.contains("2"), "pretty output: {pretty}");
    assert!(pretty.contains("3"), "pretty output: {pretty}");
}

#[test]
fn test_pretty_value_object() {
    let v = value!({"name": "Alice", "age": 30});
    let pretty = v.pretty().to_string();
    // Should contain the keys and values
    assert!(pretty.contains("name"), "pretty output: {pretty}");
    assert!(pretty.contains("Alice"), "pretty output: {pretty}");
    assert!(pretty.contains("age"), "pretty output: {pretty}");
    assert!(pretty.contains("30"), "pretty output: {pretty}");
}

#[test]
fn test_pretty_nested_value() {
    let v = value!({
        "users": [
            {"name": "Alice"},
            {"name": "Bob"}
        ]
    });
    let pretty = v.pretty().to_string();
    assert!(pretty.contains("users"), "pretty output: {pretty}");
    assert!(pretty.contains("Alice"), "pretty output: {pretty}");
    assert!(pretty.contains("Bob"), "pretty output: {pretty}");
}
