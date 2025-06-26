use facet::Facet;
use facet_json::{from_str, to_string};
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

#[test]
fn test_roundtrip_arc_foobar() {
    #[derive(Debug, PartialEq, Facet)]
    #[facet(deny_unknown_fields)]
    struct Foobar {
        foo: i32,
        bar: String,
    }

    let json = r#"{"foo":123,"bar":"baz"}"#;

    let arc_foobar: Arc<Foobar> = from_str(json)?;

    let expected = Arc::new(Foobar {
        foo: 123,
        bar: "baz".to_string(),
    });

    assert_eq!(arc_foobar, expected);

    // Test round-trip serialization
    let serialized = to_string(&arc_foobar);
    assert_eq!(serialized, json);
}

#[test]
fn test_roundtrip_arc_str() {
    let json = r#""hello world""#;

    let arc_str: Arc<str> = from_str(json)?;

    let expected: Arc<str> = Arc::from("hello world");

    assert_eq!(arc_str, expected);

    // Test round-trip serialization
    let serialized = to_string(&arc_str);
    assert_eq!(serialized, json);
}

#[test]
fn test_roundtrip_rc_str() {
    use std::rc::Rc;
    let json = r#""hello world""#;

    let rc_str: Rc<str> = from_str(json)?;

    let expected: Rc<str> = Rc::from("hello world");

    assert_eq!(rc_str, expected);

    // Test round-trip serialization
    let serialized = to_string(&rc_str);
    assert_eq!(serialized, json);
}

#[test]
fn test_roundtrip_box_str() {
    let json = r#""hello world""#;

    let box_str: Box<str> = from_str(json)?;

    let expected: Box<str> = Box::from("hello world");

    assert_eq!(box_str, expected);

    // Test round-trip serialization
    let serialized = to_string(&box_str);
    assert_eq!(serialized, json);
}

#[test]
fn test_roundtrip_arc_slice() {
    let json = r#"[1,2,3,4]"#;

    let arc_slice: Arc<[i32]> = from_str(json)?;

    let expected: Arc<[i32]> = Arc::from([1, 2, 3, 4]);

    assert_eq!(arc_slice, expected);

    // Test round-trip serialization
    let serialized = to_string(&arc_slice);
    assert_eq!(serialized, json);
}

#[test]
fn test_deserialize_arc_slice_only() {
    // Test only deserialization of Arc<[i32]> (not serialization)
    let json = r#"[1,2,3,4,5]"#;

    let arc_slice: Arc<[i32]> = from_str(json)?;

    // Verify the deserialized values
    assert_eq!(arc_slice.len(), 5);
    assert_eq!(arc_slice[0], 1);
    assert_eq!(arc_slice[1], 2);
    assert_eq!(arc_slice[2], 3);
    assert_eq!(arc_slice[3], 4);
    assert_eq!(arc_slice[4], 5);

    // Also test with empty array
    let empty_json = r#"[]"#;
    let empty_arc: Arc<[i32]> = from_str(empty_json)?;
    assert_eq!(empty_arc.len(), 0);
}
