// use std::num::NonZero;

use std::num::NonZero;

use facet::Facet;
use facet_json::from_str;

#[test]
fn json_read_simple_struct() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {
        name: String,
        age: u64,
        hobbies: Vec<String>,
    }
    let json = r#"{"name": "Alice", "age": 30, "hobbies": ["reading", "coding"]}"#;

    let s: TestStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);
    assert_eq!(s.hobbies.len(), 2);
    assert_eq!(s.hobbies[0], "reading");
    assert_eq!(s.hobbies[1], "coding");
}

#[test]
fn json_read_empty_struct() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {}
    let json = r#"{}"#;

    let _: TestStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
}

#[test]
fn json_read_nonzero() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Foo {
        foo: NonZero<u8>,
    }
    let json = r#"{"foo": 1}"#;
    let s: Foo = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(s.foo, { const { NonZero::new(1).unwrap() } });
}

#[test]
fn json_read_vec() {
    facet_testhelpers::setup();

    let json = r#"[1, 2, 3, 4, 5]"#;

    let v: Vec<i32> = match from_str(json) {
        Ok(v) => v,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(v, vec![1, 2, 3, 4, 5]);
}

#[test]
fn json_read_empty_vec() {
    facet_testhelpers::setup();

    let json = r#"[]"#;

    let v: Vec<i32> = match from_str(json) {
        Ok(v) => v,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(v, vec![]);
}

// #[test]
// fn json_read_hashmap() {
//     facet_testhelpers::setup();

//     let json = r#"{"key1": "value1", "key2": "value2", "key3": "value3"}"#;

//     let m: std::collections::HashMap<String, String> = match from_str(json) {
//         Ok(m) => m,
//         Err(e) => panic!("Error deserializing JSON: {}", e),
//     };
//     assert_eq!(m.get("key1").unwrap(), "value1");
//     assert_eq!(m.get("key2").unwrap(), "value2");
//     assert_eq!(m.get("key3").unwrap(), "value3");
// }

#[test]
fn json_read_more_types() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStructWithMoreTypes {
        u8_val: u8,
        u16_val: u16,
        i8_val: i8,
        i16_val: i16,
        u32_val: u32,
        i32_val: i32,
        u64_val: u64,
        i64_val: i64,
        f32_val: f32,
        f64_val: f64,
    }

    let json = r#"{
        "u8_val": 255,
        "u16_val": 65535,
        "i8_val": -128,
        "i16_val": -32768,
        "u32_val": 4294967295,
        "i32_val": -2147483648,
        "u64_val": 18446744073709551615,
        "i64_val": -9223372036854775808,
        "f32_val": 3.141592653589793,
        "f64_val": 3.141592653589793
    }"#;

    let test_struct: TestStructWithMoreTypes = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(test_struct.u8_val, 255);
    assert_eq!(test_struct.u16_val, 65535);
    assert_eq!(test_struct.i8_val, -128);
    assert_eq!(test_struct.i16_val, -32768);
    assert_eq!(test_struct.u32_val, 4294967295);
    assert_eq!(test_struct.i32_val, -2147483648);
    assert_eq!(test_struct.u64_val, 18446744073709551615);
    assert_eq!(test_struct.i64_val, -9223372036854775808);
    assert!((test_struct.f32_val - std::f32::consts::PI).abs() < f32::EPSILON);
    assert!((test_struct.f64_val - std::f64::consts::PI).abs() < f64::EPSILON);
}

#[test]
fn test_from_json_with_nested_structs() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct InnerStruct {
        value: i32,
    }

    #[derive(Facet)]
    struct OuterStruct {
        name: String,
        inner: InnerStruct,
    }

    let json = r#"{
        "name": "Outer",
        "inner": {
            "value": 42
        }
    }"#;

    let test_struct: OuterStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(test_struct.name, "Outer");
    assert_eq!(test_struct.inner.value, 42);
}

#[test]
fn test_from_json_with_option() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Options {
        name: Option<String>,
        age: Option<u32>,
        inner: Option<Inner>,
    }

    #[derive(Facet)]
    struct Inner {
        foo: i32,
    }

    let json = r#"{
    "name": "Alice",
    "age": null,
    "inner": {
        "foo": 42
    }
}"#;

    let test_struct: Options = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(test_struct.name.as_deref(), Some("Alice"));
    assert_eq!(test_struct.age, None);
    assert_eq!(test_struct.inner.as_ref().map(|i| i.foo), Some(42));
}

#[test]
fn test_field_rename_deserialization() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct Greetings {
        #[facet(rename = "bonjour")]
        hello: String,

        #[facet(rename = "au_revoir")]
        goodbye: String,
    }

    let json = r#"{"bonjour":"monde","au_revoir":"world"}"#;

    let result: Greetings = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(result.hello, "monde");
    assert_eq!(result.goodbye, "world");
}

#[cfg(feature = "std")]
#[test]
fn test_field_rename_roundtrip() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct Greetings {
        #[facet(rename = "bonjour")]
        hello: String,
    }

    let original = Greetings {
        hello: "monde".to_string(),
    };

    let json = facet_json::to_string(&original);
    assert_eq!(json, r#"{"bonjour":"monde"}"#);

    let roundtrip: Greetings = from_str(&json).unwrap();
    assert_eq!(original, roundtrip);
}

#[test]
fn test_field_rename_with_special_chars() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct SpecialNames {
        #[facet(rename = "kebab-case")]
        kebab_case: String,

        #[facet(rename = "snake_case")]
        original_snake: String,

        #[facet(rename = "camelCase")]
        camel_case: String,
    }

    let json = r#"{"kebab-case":"dash","snake_case":"underscore","camelCase":"hump"}"#;

    let result: SpecialNames = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(result.kebab_case, "dash");
    assert_eq!(result.original_snake, "underscore");
    assert_eq!(result.camel_case, "hump");
}
