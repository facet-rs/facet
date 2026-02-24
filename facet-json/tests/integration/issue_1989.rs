//! Regression test for https://github.com/facet-rs/facet/issues/1989
//!
//! Untagged enum struct variants must error on missing required fields.

use facet::Facet;
use facet_json::from_str;
use facet_testhelpers::test;

#[derive(Debug, Facet, PartialEq)]
#[facet(untagged)]
#[repr(C)]
enum TestEnum {
    Single(i32),
    MinMaxStepList([i32; 3]),
    MinMaxList([i32; 2]),
    MinMax {
        min: i32,
        max: i32,
        #[facet(default)]
        step: Option<i32>,
    },
}

#[test]
fn test_issue_1989_untagged_missing_required_fields_error() {
    let input = r#"{}"#;
    let result: Result<TestEnum, _> = from_str(input);

    assert!(
        result.is_err(),
        "deserializing `{{}}` should fail for missing required fields, got: {:?}",
        result
    );
}
