//! Regression tests for <https://github.com/facet-rs/facet/issues/1987>
//!
//! Untagged enums with multiple fixed-size array variants should pick the
//! matching variant by array length, independent of declaration order.

use facet::Facet;

#[derive(Debug, Facet, PartialEq)]
#[facet(untagged)]
#[repr(C)]
enum TwoThenThree {
    Single(i32),
    MinMaxList([i32; 2]),
    MinMaxStepList([i32; 3]),
}

#[derive(Debug, Facet, PartialEq)]
#[facet(untagged)]
#[repr(C)]
enum ThreeThenTwo {
    Single(i32),
    MinMaxStepList([i32; 3]),
    MinMaxList([i32; 2]),
}

#[test]
fn test_issue_1987_two_element_array_matches_two_element_variant() {
    let input = r#"[1,5]"#;

    let a: TwoThenThree = facet_json::from_str(input).unwrap();
    assert_eq!(a, TwoThenThree::MinMaxList([1, 5]));

    let b: ThreeThenTwo = facet_json::from_str(input).unwrap();
    assert_eq!(b, ThreeThenTwo::MinMaxList([1, 5]));
}

#[test]
fn test_issue_1987_three_element_array_matches_three_element_variant() {
    let input = r#"[1,5,1]"#;

    let a: TwoThenThree = facet_json::from_str(input).unwrap();
    assert_eq!(a, TwoThenThree::MinMaxStepList([1, 5, 1]));

    let b: ThreeThenTwo = facet_json::from_str(input).unwrap();
    assert_eq!(b, ThreeThenTwo::MinMaxStepList([1, 5, 1]));
}
