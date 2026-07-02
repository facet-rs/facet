//! Regression test for https://github.com/facet-rs/facet/issues/2264
//!
//! `#[facet(flatten)]` on a field whose type is a `#[facet(transparent)]`
//! newtype wrapper should look through the wrapper and flatten the inner
//! type's fields as if the wrapper were not there.

use facet::Facet;
use facet_testhelpers::test;

#[derive(Facet, Clone, PartialEq, Debug)]
struct Fields {
    data: [String; 2],
}

#[derive(Facet, Clone, PartialEq, Debug)]
#[facet(transparent)]
struct Transparent<T>(T);

/// Without the wrapper — must keep working.
#[derive(Facet, Clone, PartialEq, Debug)]
struct Flatten {
    kind: String,
    #[facet(flatten)]
    fields: Fields,
}

/// With the transparent wrapper — this was the bug.
#[derive(Facet, Clone, PartialEq, Debug)]
struct WrappedFlatten {
    kind: String,
    #[facet(flatten)]
    fields: Transparent<Fields>,
}

const JSON: &str = r#"{"kind":"foo","data":["lorem","ipsum"]}"#;

#[test]
fn test_issue_2264_flatten_without_wrapper() {
    let v: Flatten = facet_json::from_str(JSON).unwrap();
    assert_eq!(v.kind, "foo");
    assert_eq!(v.fields.data, ["lorem", "ipsum"]);

    let roundtrip = facet_json::to_string(&v).unwrap();
    let back: Flatten = facet_json::from_str(&roundtrip).unwrap();
    assert_eq!(v, back);
}

#[test]
fn test_issue_2264_flatten_with_transparent_wrapper() {
    let v: WrappedFlatten = facet_json::from_str(JSON).unwrap();
    assert_eq!(v.kind, "foo");
    assert_eq!(v.fields.0.data, ["lorem", "ipsum"]);

    let roundtrip = facet_json::to_string(&v).unwrap();
    let back: WrappedFlatten = facet_json::from_str(&roundtrip).unwrap();
    assert_eq!(v, back);
}
