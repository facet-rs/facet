use facet::Facet;
use facet_json::from_str;
use facet_testhelpers::test;
use std::sync::Arc;

#[derive(Debug, PartialEq, Facet)]
#[facet(deny_unknown_fields)]
struct SomeStruct {
    value: i32,
}

#[derive(Debug, PartialEq, Facet)]
#[facet(deny_unknown_fields)]
struct Wrapper {
    inner: Arc<SomeStruct>,
}

#[test]
fn test_deserialize_struct_with_arc_field() {
    let json = r#"{"inner":{"value":42}}"#;

    let wrapper: Wrapper = from_str(json)?;

    let expected = Wrapper {
        inner: Arc::new(SomeStruct { value: 42 }),
    };

    assert_eq!(wrapper, expected);
}
