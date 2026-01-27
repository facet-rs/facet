//! Integration tests for `from_value` deserialization.

use facet::Facet;
use facet_testhelpers::test;
use facet_value::{VString, Value, from_value, value};
use std::collections::{BTreeMap, HashMap};

#[test]
fn deserialize_simple_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    let v = value!({
        "name": "Alice",
        "age": 30
    });

    let person: Person = from_value(v).unwrap();
    assert_eq!(person.name, "Alice");
    assert_eq!(person.age, 30);
}

#[test]
fn deserialize_nested_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Address {
        street: String,
        city: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Person {
        name: String,
        address: Address,
    }

    let v = value!({
        "name": "Bob",
        "address": {
            "street": "123 Main St",
            "city": "Springfield"
        }
    });

    let person: Person = from_value(v).unwrap();
    assert_eq!(person.name, "Bob");
    assert_eq!(person.address.street, "123 Main St");
    assert_eq!(person.address.city, "Springfield");
}

#[test]
fn deserialize_struct_with_option() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
        description: Option<String>,
    }

    // With Some
    let v1 = value!({
        "name": "test",
        "description": "A test config"
    });
    let cfg1: Config = from_value(v1).unwrap();
    assert_eq!(cfg1.description, Some("A test config".to_string()));

    // With None
    let v2 = value!({
        "name": "test",
        "description": null
    });
    let cfg2: Config = from_value(v2).unwrap();
    assert_eq!(cfg2.description, None);
}

#[test]
fn deserialize_struct_with_vec() {
    #[derive(Debug, Facet, PartialEq)]
    struct Numbers {
        values: Vec<i32>,
    }

    let v = value!({
        "values": [1, 2, 3, 4, 5]
    });

    let nums: Numbers = from_value(v).unwrap();
    assert_eq!(nums.values, vec![1, 2, 3, 4, 5]);
}

#[test]
fn deserialize_unit_enum() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Color {
        Red,
        Green,
        Blue,
    }

    let v: Value = VString::new("Red").into();
    let color: Color = from_value(v).unwrap();
    assert_eq!(color, Color::Red);

    let v2: Value = VString::new("Blue").into();
    let color2: Color = from_value(v2).unwrap();
    assert_eq!(color2, Color::Blue);
}

#[test]
fn deserialize_tuple_enum() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Message {
        Text(String),
        Number(i32),
    }

    let v = value!({"Text": "Hello"});
    let msg: Message = from_value(v).unwrap();
    assert_eq!(msg, Message::Text("Hello".to_string()));

    let v2 = value!({"Number": 42});
    let msg2: Message = from_value(v2).unwrap();
    assert_eq!(msg2, Message::Number(42));
}

#[test]
fn deserialize_struct_enum() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    let v = value!({
        "Circle": {
            "radius": 5.0
        }
    });
    let shape: Shape = from_value(v).unwrap();
    assert_eq!(shape, Shape::Circle { radius: 5.0 });

    let v2 = value!({
        "Rectangle": {
            "width": 10.0,
            "height": 20.0
        }
    });
    let shape2: Shape = from_value(v2).unwrap();
    assert_eq!(
        shape2,
        Shape::Rectangle {
            width: 10.0,
            height: 20.0
        }
    );
}

#[test]
fn deserialize_hashmap() {
    let v = value!({
        "a": 1,
        "b": 2,
        "c": 3
    });

    let map: HashMap<String, i32> = from_value(v).unwrap();
    assert_eq!(map.get("a"), Some(&1));
    assert_eq!(map.get("b"), Some(&2));
    assert_eq!(map.get("c"), Some(&3));
}

#[test]
fn deserialize_btreemap() {
    let v = value!({
        "x": 10,
        "y": 20
    });

    let map: BTreeMap<String, i32> = from_value(v).unwrap();
    assert_eq!(map.get("x"), Some(&10));
    assert_eq!(map.get("y"), Some(&20));
}

#[test]
fn deserialize_box() {
    #[derive(Debug, Facet, PartialEq)]
    struct Node {
        value: i32,
        #[facet(recursive_type)]
        next: Option<Box<Node>>,
    }

    let v = value!({
        "value": 1,
        "next": {
            "value": 2,
            "next": null
        }
    });

    let node: Node = from_value(v).unwrap();
    assert_eq!(node.value, 1);
    assert_eq!(node.next.as_ref().unwrap().value, 2);
    assert!(node.next.as_ref().unwrap().next.is_none());
}

#[test]
fn deserialize_fixed_array() {
    let v = value!([1, 2, 3]);
    let arr: [i32; 3] = from_value(v).unwrap();
    assert_eq!(arr, [1, 2, 3]);
}

#[test]
fn deserialize_tuple() {
    let v = value!([1, "hello", true]);
    let tuple: (i32, String, bool) = from_value(v).unwrap();
    assert_eq!(tuple, (1, "hello".to_string(), true));
}

#[test]
fn deserialize_primitives() {
    // bool
    let v = Value::TRUE;
    let b: bool = from_value(v).unwrap();
    assert!(b);

    // i8
    let v = Value::from(42i64);
    let n: i8 = from_value(v).unwrap();
    assert_eq!(n, 42);

    // i16
    let v = Value::from(1000i64);
    let n: i16 = from_value(v).unwrap();
    assert_eq!(n, 1000);

    // i32
    let v = Value::from(100000i64);
    let n: i32 = from_value(v).unwrap();
    assert_eq!(n, 100000);

    // i64
    let v = Value::from(i64::MAX);
    let n: i64 = from_value(v).unwrap();
    assert_eq!(n, i64::MAX);

    // u8
    let v = Value::from(255u64);
    let n: u8 = from_value(v).unwrap();
    assert_eq!(n, 255);

    // u16
    let v = Value::from(65535u64);
    let n: u16 = from_value(v).unwrap();
    assert_eq!(n, 65535);

    // u32
    let v = Value::from(u32::MAX as u64);
    let n: u32 = from_value(v).unwrap();
    assert_eq!(n, u32::MAX);

    // u64
    let v = Value::from(u64::MAX);
    let n: u64 = from_value(v).unwrap();
    assert_eq!(n, u64::MAX);

    // f32
    let v = Value::from(2.5f64);
    let n: f32 = from_value(v).unwrap();
    assert!((n - 2.5).abs() < 0.001);

    // f64
    let v = Value::from(2.5f64);
    let n: f64 = from_value(v).unwrap();
    assert!((n - 2.5).abs() < 0.0000001);

    // String
    let v: Value = VString::new("hello").into();
    let s: String = from_value(v).unwrap();
    assert_eq!(s, "hello");
}

#[test]
fn deserialize_with_default() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
        #[facet(default)]
        enabled: bool,
        #[facet(default)]
        count: Option<i32>,
    }

    // Missing fields should get defaults
    let v = value!({
        "name": "test"
    });
    let cfg: Config = from_value(v).unwrap();
    assert_eq!(cfg.name, "test");
    assert!(!cfg.enabled); // default is false
    assert_eq!(cfg.count, None); // Option defaults to None
}

#[test]
fn deserialize_value_into_value() {
    // Deserializing a Value into a Value should just clone
    let v = value!({
        "nested": {
            "array": [1, 2, 3]
        }
    });

    let v2: Value = from_value(v.clone()).unwrap();
    assert_eq!(v, v2);
}

#[test]
fn deserialize_numeric_enum() {
    /// Numeric enum with a u8 discriminant.
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[facet(is_numeric)]
    #[repr(u8)]
    enum NumericEnum {
        A,
        B,
        C,
    }

    // Deserialize from number value (discriminant 0 -> A)
    let v = Value::from(0i64);
    let e: NumericEnum = from_value(v).unwrap();
    assert_eq!(e, NumericEnum::A);

    // Deserialize from number value (discriminant 1 -> B)
    let v = Value::from(1i64);
    let e: NumericEnum = from_value(v).unwrap();
    assert_eq!(e, NumericEnum::B);

    // Deserialize from number value (discriminant 2 -> C)
    let v = Value::from(2i64);
    let e: NumericEnum = from_value(v).unwrap();
    assert_eq!(e, NumericEnum::C);
}

#[test]
fn deserialize_signed_numeric_enum() {
    /// Numeric enum with signed discriminant.
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[facet(is_numeric)]
    #[repr(i16)]
    enum SignedNumericEnum {
        Negative = -1,
        Zero,
        Positive,
    }

    // Deserialize from signed number (discriminant -1 -> Negative)
    let v = Value::from(-1i64);
    let e: SignedNumericEnum = from_value(v).unwrap();
    assert_eq!(e, SignedNumericEnum::Negative);

    // Deserialize from zero (discriminant 0 -> Zero)
    let v = Value::from(0i64);
    let e: SignedNumericEnum = from_value(v).unwrap();
    assert_eq!(e, SignedNumericEnum::Zero);

    // Deserialize from positive (discriminant 1 -> Positive)
    let v = Value::from(1i64);
    let e: SignedNumericEnum = from_value(v).unwrap();
    assert_eq!(e, SignedNumericEnum::Positive);
}

#[test]
fn deserialize_numeric_enum_from_string() {
    /// Numeric enum with a u8 discriminant.
    #[derive(Facet, Debug, Clone, PartialEq)]
    #[facet(is_numeric)]
    #[repr(u8)]
    enum NumericEnum {
        A,
        B,
    }

    // Deserialize from string that can be parsed as integer
    let v: Value = VString::new("0").into();
    let e: NumericEnum = from_value(v).unwrap();
    assert_eq!(e, NumericEnum::A);

    let v: Value = VString::new("1").into();
    let e: NumericEnum = from_value(v).unwrap();
    assert_eq!(e, NumericEnum::B);
}

#[test]
fn deserialize_struct_with_rename() {
    // Test that from_value respects #[facet(rename = "...")] attribute
    // This is a regression test for https://github.com/facet-rs/facet/issues/1940
    #[derive(Debug, Facet, PartialEq)]
    struct Outer {
        #[facet(rename = "other")]
        field: String,
    }

    let v = value!({
        "other": "hi"
    });

    let result: Outer = from_value(v).unwrap();
    assert_eq!(result.field, "hi");
}

#[test]
fn deserialize_struct_with_rename_and_alias() {
    // Test that both rename and alias work together
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        #[facet(rename = "primary_name", alias = "alt_name")]
        value: String,
    }

    // Using the renamed name should work
    let v1 = value!({
        "primary_name": "via rename"
    });
    let result1: Config = from_value(v1).unwrap();
    assert_eq!(result1.value, "via rename");

    // Using the alias should also work
    let v2 = value!({
        "alt_name": "via alias"
    });
    let result2: Config = from_value(v2).unwrap();
    assert_eq!(result2.value, "via alias");
}
