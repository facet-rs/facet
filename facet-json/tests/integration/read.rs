// use std::num::NonZero;

use std::num::NonZero;

use facet::Facet;
use facet_json::from_str;
use insta::assert_snapshot;

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
fn json_read_struct_twofields() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct TestStruct {
        name: String,
        age: u64,
    }
    let json = r#"{"name": "Alice", "age": 30}"#;

    let s: TestStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };
    assert_eq!(s.name, "Alice");
    assert_eq!(s.age, 30);
}

#[test]
fn json_read_struct_threefields() {
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

#[test]
fn json_read_bool() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct BoolStruct {
        yes: bool,
        no: bool,
    }

    let json = r#"{"yes": true, "no": false}"#;

    let s: BoolStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(
        s,
        BoolStruct {
            yes: true,
            no: false
        }
    );
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

/// Basic deserialization with renamed fields
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

/// Round-trip serialization then deserialization with a renamed field
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

/// Deserialization with common naming conventions (kebab-case, snake_case, camelCase)
#[test]
fn test_field_rename_common_case_styles() {
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

/// Serialization and deserialization with special symbol characters in field name
#[test]
#[cfg(feature = "std")]
fn test_field_rename_with_symbol_chars_name() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct SpecialCharsName {
        #[facet(rename = "@#$%^&")]
        special_chars: String,
    }

    let test_struct = SpecialCharsName {
        special_chars: "special value".to_string(),
    };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"@#$%^&":"special value"}"#);

    let roundtrip: SpecialCharsName = facet_json::from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Serialization and deserialization with Unicode characters in field name (emoji)
#[test]
#[cfg(feature = "std")]
fn test_field_rename_with_unicode_name_emoji() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct EmojiCharsName {
        #[facet(rename = "🏀")]
        ball: String,
    }

    let test_struct = EmojiCharsName {
        ball: "🏆".to_string(),
    };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"🏀":"🏆"}"#);

    let roundtrip: EmojiCharsName = facet_json::from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Serialization and deserialization with Unicode characters in field name (Euro sign)
#[test]
#[cfg(feature = "std")]
fn test_field_rename_with_unicode_name_special_signs() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct EmojiCharsName {
        #[facet(rename = "€℮↑→↓↔↕")]
        special_chars: String,
    }

    let test_struct = EmojiCharsName {
        special_chars: "...".to_string(),
    };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"€℮↑→↓↔↕":"..."}"#);

    let roundtrip: EmojiCharsName = facet_json::from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Serialization and deserialization with numeric field name
#[cfg(feature = "std")]
#[test]
fn test_field_rename_with_numeric_name() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct NumericName {
        #[facet(rename = "123")]
        numeric_name: i32,
    }

    let test_struct = NumericName { numeric_name: 42 };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"123":42}"#);

    let roundtrip: NumericName = facet_json::from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Serialization and deserialization with empty field name
#[cfg(feature = "std")]
#[test]
#[ignore]
fn test_field_rename_with_empty_name() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct EmptyName {
        #[facet(rename = "")]
        empty_name: bool,
    }

    let test_struct = EmptyName { empty_name: true };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"":true}"#);

    let roundtrip: EmptyName = facet_json::from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Serialization and deserialization of renamed enum variants (unit and tuple variants)
#[cfg(feature = "std")]
#[test]
#[ignore]
fn test_enum_variant_rename() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Color {
        #[facet(rename = "lime")]
        Green,

        #[facet(rename = "cyan")]
        Blue(u8),
    }

    // Test unit variant with rename
    let green = Color::Green;
    let json = facet_json::to_string(&green);
    assert_eq!(json, r#""lime""#);
    let roundtrip: Color = facet_json::from_str(&json).unwrap();
    assert_eq!(green, roundtrip);

    // Test tuple variant with rename
    let blue = Color::Blue(255);
    let json = facet_json::to_string(&blue);
    assert_eq!(json, r#"{"cyan":255}"#);
    let roundtrip: Color = facet_json::from_str(&json).unwrap();
    assert_eq!(blue, roundtrip);
}

/// Serialization and deserialization of renamed fields in struct enum variants
#[cfg(feature = "std")]
#[test]
#[ignore]
fn test_enum_struct_variant_field_rename() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    #[repr(u8)]
    enum Message {
        #[facet(rename = "success")]
        Success {
            #[facet(rename = "message")]
            msg: String,

            #[facet(rename = "code")]
            status_code: u16,
        },

        #[facet(rename = "error")]
        Error {
            #[facet(rename = "errorMessage")]
            msg: String,

            #[facet(rename = "errorCode")]
            code: u16,
        },
    }

    // Test struct variant with renamed fields
    let success = Message::Success {
        msg: "Operation completed".to_string(),
        status_code: 200,
    };

    let json = facet_json::to_string(&success);
    assert_eq!(
        json,
        r#"{"success":{"message":"Operation completed","code":200}}"#
    );

    let roundtrip: Message = facet_json::from_str(&json).unwrap();
    assert_eq!(success, roundtrip);

    // Test error variant
    let error = Message::Error {
        msg: "Not found".to_string(),
        code: 404,
    };

    let json = facet_json::to_string(&error);
    assert_eq!(
        json,
        r#"{"error":{"errorMessage":"Not found","errorCode":404}}"#
    );

    let roundtrip: Message = facet_json::from_str(&json).unwrap();
    assert_eq!(error, roundtrip);
}

/// Serialization and deserialization of renamed fields in nested data structures
#[cfg(feature = "std")]
#[test]
fn test_field_rename_nested_structures() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct Address {
        #[facet(rename = "streetName")]
        street: String,

        #[facet(rename = "zipCode")]
        postal_code: String,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct Person {
        #[facet(rename = "fullName")]
        name: String,

        #[facet(rename = "homeAddress")]
        address: Address,

        #[facet(rename = "contactInfo")]
        contacts: Vec<Contact>,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct Contact {
        #[facet(rename = "type")]
        contact_type: String,

        #[facet(rename = "value")]
        contact_value: String,
    }

    let person = Person {
        name: "John Doe".to_string(),
        address: Address {
            street: "Main St".to_string(),
            postal_code: "12345".to_string(),
        },
        contacts: vec![
            Contact {
                contact_type: "email".to_string(),
                contact_value: "john@example.com".to_string(),
            },
            Contact {
                contact_type: "phone".to_string(),
                contact_value: "555-1234".to_string(),
            },
        ],
    };

    let json = facet_json::to_string(&person);
    let expected = r#"{"fullName":"John Doe","homeAddress":{"streetName":"Main St","zipCode":"12345"},"contactInfo":[{"type":"email","value":"john@example.com"},{"type":"phone","value":"555-1234"}]}"#;
    assert_eq!(json, expected);

    let roundtrip: Person = facet_json::from_str(&json).unwrap();
    assert_eq!(person, roundtrip);
}

/// Serialization and deserialization of renamed optional fields (Some and None cases)
#[cfg(feature = "std")]
#[test]
fn test_field_rename_optional_values() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct OptionalFields {
        #[facet(rename = "requiredField")]
        required: String,

        #[facet(rename = "optionalString")]
        maybe_string: Option<String>,

        #[facet(rename = "optionalNumber")]
        maybe_number: Option<i32>,
    }

    // Test with all fields present
    let full = OptionalFields {
        required: "always here".to_string(),
        maybe_string: Some("optional value".to_string()),
        maybe_number: Some(42),
    };

    let json = facet_json::to_string(&full);
    assert_eq!(
        json,
        r#"{"requiredField":"always here","optionalString":"optional value","optionalNumber":42}"#
    );

    let roundtrip: OptionalFields = facet_json::from_str(&json).unwrap();
    assert_eq!(full, roundtrip);

    // Test with None fields
    let partial = OptionalFields {
        required: "always here".to_string(),
        maybe_string: None,
        maybe_number: None,
    };

    let json = facet_json::to_string(&partial);
    assert_eq!(
        json,
        r#"{"requiredField":"always here","optionalString":null,"optionalNumber":null}"#
    );

    let roundtrip: OptionalFields = facet_json::from_str(&json).unwrap();
    assert_eq!(partial, roundtrip);
}

/// Deserialization with extra fields in JSON that aren't in the target struct
#[test]
fn test_field_rename_ignore_extra_fields() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct User {
        #[facet(rename = "userId")]
        id: u64,

        #[facet(rename = "userName")]
        name: String,
    }

    // JSON with extra fields that aren't in our struct
    let json = r#"{"userId":123,"userName":"Alice","role":"admin","active":true}"#;

    // We should be able to deserialize this without error, ignoring extra fields
    let user: User = facet_json::from_str(json).unwrap();

    assert_eq!(user.id, 123);
    assert_eq!(user.name, "Alice");
}

/// Renamed fields have priority over original field names during serialization
#[cfg(feature = "std")]
#[test]
fn test_field_rename_serialization_priority() {
    facet_testhelpers::setup();

    // When serializing, the rename attribute should always be used instead of
    // the original field name
    #[derive(Debug, PartialEq, Facet)]
    struct DataModel {
        #[facet(rename = "data")]
        items: Vec<String>,
    }

    let model = DataModel {
        items: vec!["one".to_string(), "two".to_string()],
    };

    let json = facet_json::to_string(&model);
    assert_eq!(json, r#"{"data":["one","two"]}"#);
}

/// Proper errors are returned when required renamed fields are missing
#[test]
fn test_field_rename_missing_required_error() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Facet)]
    struct Required {
        #[facet(rename = "renamedField")]
        original_field: String,
    }

    // JSON missing the required field
    let json = r#"{}"#;

    // This should result in an error as the required field is missing
    let result = facet_json::from_str::<Required>(json);
    let e = result.unwrap_err();
    assert!(matches!(
        e.kind,
        facet_json::JsonErrorKind::MissingField(f) if f == "original_field"
    ));
    assert_snapshot!(e.to_string());
}

#[test]
fn test_two_empty_arrays() {
    facet_testhelpers::setup();

    #[derive(Facet, Clone, Default)]
    pub struct RevisionConfig {
        pub one: Vec<String>,
        pub two: Vec<String>,
    }

    let markup = r#"
    {
      "one": [],
      "two": []
    }
    "#;

    let config: RevisionConfig = match from_str(markup) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Failed to parse RevisionConfig: {}", e),
    };
    assert!(config.one.is_empty());
    assert!(config.two.is_empty());
}

#[test]
fn test_one_empty_one_nonempty_array() {
    facet_testhelpers::setup();

    #[derive(Facet, Clone, Default)]
    pub struct RevisionConfig {
        pub one: Vec<String>,
        pub two: Vec<String>,
    }

    let markup = r#"
    {
      "one": [],
      "two": ["a", "b", "c"]
    }
    "#;

    let config: RevisionConfig = match from_str(markup) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Failed to parse RevisionConfig: {}", e),
    };
    assert!(config.one.is_empty());
    assert_eq!(config.two, vec!["a", "b", "c"]);
}

#[test]
fn test_one_nonempty_one_empty_array() {
    facet_testhelpers::setup();

    #[derive(Facet, Clone, Default)]
    pub struct RevisionConfig {
        pub one: Vec<String>,
        pub two: Vec<String>,
    }

    let markup = r#"
    {
      "one": ["x", "y"],
      "two": []
    }
    "#;

    let config: RevisionConfig = match from_str(markup) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Failed to parse RevisionConfig: {}", e),
    };
    assert_eq!(config.one, vec!["x", "y"]);
    assert!(config.two.is_empty());
}

#[test]
fn test_nested_arrays() {
    facet_testhelpers::setup();

    #[derive(Facet, Clone, Default)]
    pub struct NestedArrays {
        pub matrix: Vec<Vec<i32>>,
    }

    let markup = r#"
    {
      "matrix": [
        [1, 2, 3],
        [],
        [4, 5]
      ]
    }
    "#;

    let nested: NestedArrays = match from_str(markup) {
        Ok(cfg) => cfg,
        Err(e) => panic!("Failed to parse NestedArrays: {}", e),
    };
    assert_eq!(nested.matrix.len(), 3);
    assert_eq!(nested.matrix[0], vec![1, 2, 3]);
    assert_eq!(nested.matrix[1], vec![]);
    assert_eq!(nested.matrix[2], vec![4, 5]);
}

#[test]
fn test_deny_unknown_fields() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct StrictStruct {
        foo: String,
        bar: i32,
    }

    // JSON with only expected fields
    let json_ok = r#"{"foo":"abc","bar":42}"#;
    let result_ok: Result<StrictStruct, _> = from_str(json_ok);
    assert!(result_ok.is_ok());

    // JSON with an unexpected extra field should generate an error
    let json_extra = r#"{"foo":"abc","bar":42,"baz":true}"#;
    let result_extra: Result<StrictStruct, _> = from_str(json_extra);
    assert!(result_extra.is_err());
}

#[test]
fn json_read_struct_level_default_unset_field() {
    facet_testhelpers::setup();

    #[derive(Facet, Default, Debug)]
    #[facet(default)]
    struct DefaultStruct {
        foo: i32,
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    let json = r#"{"foo": 123}"#;

    let s: DefaultStruct = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    // bar should be the default String ("")
    assert_eq!(s.foo, 123);
    assert_eq!(s.bar, "");
}

#[test]
fn json_read_field_level_default_no_function() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefault {
        foo: i32,
        #[facet(default)]
        bar: String,
    }

    // Only set foo, leave bar missing - should use Default for String
    let json = r#"{"foo": 789}"#;

    let s: FieldDefault = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(s.foo, 789);
    assert_eq!(s.bar, "");
}

fn default_number() -> i32 {
    12345
}

#[test]
fn json_read_field_level_default_function() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct FieldDefaultFn {
        #[facet(default = "default_number")]
        foo: i32,
        bar: String,
    }

    // Only set bar, leave foo missing - should use default_number()
    let json = r#"{"bar": "hello"}"#;

    let s: FieldDefaultFn = match from_str(json) {
        Ok(s) => s,
        Err(e) => panic!("Error deserializing JSON: {}", e),
    };

    assert_eq!(s.foo, 12345);
    assert_eq!(s.bar, "hello");
}

#[test]
fn test_allow_unknown_fields() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct PermissiveStruct {
        foo: String,
        bar: i32,
    }

    // JSON with only expected fields
    let json_ok = r#"{"foo":"abc","bar":42}"#;
    let result_ok: Result<PermissiveStruct, _> = from_str(json_ok);
    result_ok.unwrap();

    // JSON with an unexpected extra field should NOT generate an error
    let json_extra = r#"{"foo":"abc","bar":42,"baz":[]}"#;
    let result_extra: Result<PermissiveStruct, _> = from_str(json_extra);
    result_extra.unwrap();
}

#[test]
fn test_str_escaped() {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct S {
        foo: String,
    }

    let json_ok = r#"{"foo":"\"\\abc"}"#;
    let result_ok: Result<S, _> = from_str(json_ok);
    assert_eq!(&result_ok.unwrap().foo, "\"\\abc");
}
