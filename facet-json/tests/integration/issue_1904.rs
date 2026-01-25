//! Test for issue #1904: Fails to parse valid json with nested enum
//!
//! When deserializing a struct with a flattened internally-tagged enum,
//! extra fields in the JSON (that belong to other variants or are unknown)
//! should be ignored rather than causing parse failures.

use facet::Facet;
use facet_json::from_str;

#[derive(Facet, Clone, Debug, PartialEq, PartialOrd)]
#[facet(tag = "type")]
#[repr(C)]
pub enum NestedEnum {
    NestedVariant { value: f64, label: String },
}

#[derive(Facet, Clone, Debug, PartialEq, PartialOrd)]
pub struct VariantA {
    pub foo: String,
    pub bar: String,
    pub count: f64,
    pub nested: Option<NestedEnum>,
}

#[derive(Facet, Clone, Debug, PartialEq, PartialOrd)]
pub struct VariantB {
    pub name: String,
    pub extra: String,
}

#[derive(Facet, Clone, Debug, PartialEq, PartialOrd)]
#[facet(tag = "type")]
#[repr(C)]
pub enum InnerEnum {
    VariantA(VariantA),
    VariantB(VariantB),
}

#[derive(Facet, Clone, Debug, PartialEq, PartialOrd)]
pub struct Outer {
    #[facet(flatten)]
    pub inner: InnerEnum,
    pub id: Option<String>,
}

/// Tests the original bug: extra field "extra" belongs to VariantB but we're
/// parsing VariantA. The solver should not eliminate VariantA just because
/// "extra" exists in VariantB's resolution.
#[test]
fn test_issue_1904_extra_field_from_other_variant() {
    let json =
        r#"{"type":"VariantA","foo":"a","bar":"b","count":1,"nested":null,"id":null,"extra":""}"#;
    let result: Result<Outer, _> = from_str(json);

    let outer = result.expect("should parse valid JSON with extra fields from other variant");

    match &outer.inner {
        InnerEnum::VariantA(a) => {
            assert_eq!(a.foo, "a");
            assert_eq!(a.bar, "b");
            assert_eq!(a.count, 1.0);
            assert!(a.nested.is_none());
        }
        InnerEnum::VariantB(_) => panic!("expected VariantA"),
    }
    assert!(outer.id.is_none());
}

// Types for testing the superset variant case
#[derive(Facet, Clone, Debug, PartialEq)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum SupersetEnum {
    Alpha { x: i32 },
    Beta { x: i32, y: i32 }, // Beta is a superset of Alpha
}

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct SupersetOuter {
    #[facet(flatten)]
    pub inner: SupersetEnum,
}

#[test]
fn test_issue_1904_extra_field_from_superset_variant() {
    let json = r#"{"kind":"Alpha","x":1,"y":2}"#;
    let result: Result<SupersetOuter, _> = from_str(json);

    let outer = result.expect("should select Alpha based on tag despite extra field from Beta");
    assert!(matches!(outer.inner, SupersetEnum::Alpha { x: 1 }));
}

// Types for testing deny_unknown_fields
#[derive(Facet, Clone, Debug, PartialEq)]
#[facet(tag = "type")]
#[repr(C)]
pub enum StrictInnerEnum {
    VariantA { foo: String },
    VariantB { bar: String },
}

#[derive(Facet, Clone, Debug, PartialEq)]
#[facet(deny_unknown_fields)]
pub struct StrictOuter {
    #[facet(flatten)]
    pub inner: StrictInnerEnum,
}

#[test]
fn test_issue_1904_deny_unknown_fields_still_rejects() {
    let json = r#"{"type":"VariantA","foo":"hello","extra":"rejected"}"#;
    let result: Result<StrictOuter, _> = from_str(json);

    assert!(
        result.is_err(),
        "deny_unknown_fields should reject extra fields even with the fix"
    );
}
