use facet::Facet;
use facet_json_legacy::{from_str, to_string};
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

    let wrapper: Wrapper = from_str(json).unwrap();

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

    let arc_foobar: Arc<Foobar> = from_str(json).unwrap();

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

    let arc_str: Arc<str> = from_str(json).unwrap();

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

    let rc_str: Rc<str> = from_str(json).unwrap();

    let expected: Rc<str> = Rc::from("hello world");

    assert_eq!(rc_str, expected);

    // Test round-trip serialization
    let serialized = to_string(&rc_str);
    assert_eq!(serialized, json);
}

#[test]
fn test_roundtrip_box_str() {
    let json = r#""hello world""#;

    let box_str: Box<str> = from_str(json).unwrap();

    let expected: Box<str> = Box::from("hello world");

    assert_eq!(box_str, expected);

    // Test round-trip serialization
    let serialized = to_string(&box_str);
    assert_eq!(serialized, json);
}

#[test]
fn test_roundtrip_arc_slice() {
    let json = r#"[1,2,3,4]"#;

    let arc_slice: Arc<[i32]> = from_str(json).unwrap();

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

    let arc_slice: Arc<[i32]> = from_str(json).unwrap();

    // Verify the deserialized values
    assert_eq!(arc_slice.len(), 5);
    assert_eq!(arc_slice[0], 1);
    assert_eq!(arc_slice[1], 2);
    assert_eq!(arc_slice[2], 3);
    assert_eq!(arc_slice[3], 4);
    assert_eq!(arc_slice[4], 5);

    // Also test with empty array
    let empty_json = r#"[]"#;
    let empty_arc: Arc<[i32]> = from_str(empty_json).unwrap();
    assert_eq!(empty_arc.len(), 0);
}

#[test]
fn test_serialize_shared_ref_foobar() {
    #[derive(Debug, PartialEq, Facet)]
    struct Outer<'a> {
        foobar: &'a FooBar,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct FooBar {
        foo: u32,
        bar: String,
    }

    let foobar = FooBar {
        foo: 7,
        bar: "abc".to_string(),
    };

    let outer = Outer { foobar: &foobar };

    let expected_json = r#"{"foobar":{"foo":7,"bar":"abc"}}"#;
    let serialized = to_string(&outer);
    assert_eq!(serialized, expected_json);
}

#[test]
fn test_serialize_exclusive_ref_foobar() {
    #[derive(Debug, PartialEq, Facet)]
    struct Outer<'a> {
        foobar: &'a mut FooBar,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct FooBar {
        foo: u32,
        bar: String,
    }

    let mut foobar = FooBar {
        foo: 7,
        bar: "abc".to_string(),
    };

    let outer = Outer {
        foobar: &mut foobar,
    };

    let expected_json = r#"{"foobar":{"foo":7,"bar":"abc"}}"#;
    let serialized = to_string(&outer);
    assert_eq!(serialized, expected_json);
}

#[test]
fn test_serialize_str_ref() {
    #[derive(Debug, PartialEq, Facet)]
    struct Outer<'a> {
        text: &'a str,
    }

    let text = "hello world";
    let outer = Outer { text };

    let expected_json = r#"{"text":"hello world"}"#;
    let serialized = to_string(&outer);
    assert_eq!(serialized, expected_json);
}

#[test]
fn test_serialize_str_mut_ref() {
    #[derive(Debug, PartialEq, Facet)]
    struct Outer<'a> {
        text: &'a mut str,
    }

    let mut text = String::from("hello world");
    let outer = Outer { text: &mut text };

    let expected_json = r#"{"text":"hello world"}"#;
    let serialized = to_string(&outer);
    assert_eq!(serialized, expected_json);
}

#[test]
fn test_serialize_slice_ref() {
    #[derive(Debug, PartialEq, Facet)]
    struct Outer<'a> {
        numbers: &'a [u32],
    }

    let numbers = [1, 2, 3, 4, 5];
    let outer = Outer { numbers: &numbers };

    let expected_json = r#"{"numbers":[1,2,3,4,5]}"#;
    let serialized = to_string(&outer);
    assert_eq!(serialized, expected_json);
}

#[test]
fn test_serialize_slice_mut_ref() {
    #[derive(Debug, PartialEq, Facet)]
    struct Outer<'a> {
        numbers: &'a mut [u32],
    }

    let mut numbers = [1, 2, 3, 4, 5];
    let outer = Outer {
        numbers: &mut numbers,
    };

    let expected_json = r#"{"numbers":[1,2,3,4,5]}"#;
    let serialized = to_string(&outer);
    assert_eq!(serialized, expected_json);
}
