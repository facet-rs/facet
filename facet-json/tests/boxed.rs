// Tests for Box<T> serialization and deserialization in facet-json

use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;

#[derive(Debug, PartialEq, Facet)]
struct SimpleStruct {
    value: i32,
}

#[derive(Debug, PartialEq, Facet)]
struct WrapperBox {
    boxed: Box<SimpleStruct>,
}

#[derive(Debug, PartialEq, Facet)]
struct NestedBox {
    inner: Box<Box<String>>,
}

#[derive(Debug, PartialEq, Facet)]
struct BoxedPrimitive {
    num: Box<f64>,
}

#[derive(Debug, PartialEq, Facet)]
struct BoxedOption {
    maybe: Box<Option<String>>,
}

#[derive(Debug, PartialEq, Facet)]
struct BoxedVec {
    items: Box<Vec<i32>>,
}

#[test]
fn test_deserialize_boxed_struct() {
    let json = r#"{"boxed":{"value":42}}"#;
    let wrapper: WrapperBox = from_str(json)?;

    let expected = WrapperBox {
        boxed: Box::new(SimpleStruct { value: 42 }),
    };

    assert_eq!(wrapper, expected);
}

#[test]
fn test_deserialize_nested_box() {
    let json = r#"{"inner":"hello"}"#;
    let nested: NestedBox = from_str(json)?;

    let expected = NestedBox {
        inner: Box::new(Box::new("hello".to_string())),
    };

    assert_eq!(nested, expected);
}

#[test]
fn test_deserialize_boxed_primitive() {
    let json = r#"{"num":3.14}"#;
    let boxed: BoxedPrimitive = from_str(json)?;

    let expected = BoxedPrimitive {
        num: Box::new(3.14),
    };

    assert_eq!(boxed, expected);
}

#[test]
fn test_deserialize_boxed_option_some() {
    let json = r#"{"maybe":"something"}"#;
    let boxed: BoxedOption = from_str(json)?;

    let expected = BoxedOption {
        maybe: Box::new(Some("something".to_string())),
    };

    assert_eq!(boxed, expected);
}

// TODO: This test is commented out because Box<Option<T>> doesn't support
// deserializing null values yet (Box doesn't implement Default)
// #[test]
// fn test_deserialize_boxed_option_none() {
//     let json = r#"{"maybe":null}"#;
//     let boxed: BoxedOption = from_str(json)?;
//
//     let expected = BoxedOption {
//         maybe: Box::new(None),
//     };
//
//     assert_eq!(boxed, expected);
// }

#[test]
fn test_deserialize_boxed_vec() {
    let json = r#"{"items":[1,2,3,4,5]}"#;
    let boxed: BoxedVec = from_str(json)?;

    let expected = BoxedVec {
        items: Box::new(vec![1, 2, 3, 4, 5]),
    };

    assert_eq!(boxed, expected);
}

#[test]
fn test_serialize_boxed_struct() {
    let wrapper = WrapperBox {
        boxed: Box::new(SimpleStruct { value: 42 }),
    };

    let json = to_string(&wrapper);
    assert_eq!(json, r#"{"boxed":{"value":42}}"#);
}

#[test]
fn test_serialize_nested_box() {
    let nested = NestedBox {
        inner: Box::new(Box::new("hello".to_string())),
    };

    let json = to_string(&nested);
    assert_eq!(json, r#"{"inner":"hello"}"#);
}

#[test]
fn test_serialize_boxed_primitive() {
    let boxed = BoxedPrimitive {
        num: Box::new(3.14),
    };

    let json = to_string(&boxed);
    assert_eq!(json, r#"{"num":3.14}"#);
}

#[test]
fn test_serialize_boxed_option_some() {
    let boxed = BoxedOption {
        maybe: Box::new(Some("something".to_string())),
    };

    let json = to_string(&boxed);
    assert_eq!(json, r#"{"maybe":"something"}"#);
}

#[test]
fn test_serialize_boxed_option_none() {
    let boxed = BoxedOption {
        maybe: Box::new(None),
    };

    let json = to_string(&boxed);
    assert_eq!(json, r#"{"maybe":null}"#);
}

#[test]
fn test_serialize_boxed_vec() {
    let boxed = BoxedVec {
        items: Box::new(vec![1, 2, 3, 4, 5]),
    };

    let json = to_string(&boxed);
    assert_eq!(json, r#"{"items":[1,2,3,4,5]}"#);
}
