use facet::Facet;
use facet_deserialize::DeserErrorKind;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;
use insta::assert_snapshot;

/// Basic deserialization with renamed fields
#[test]
fn test_field_rename_deserialization() {
    #[derive(Facet, Debug, PartialEq)]
    struct Greetings {
        #[facet(rename = "bonjour")]
        hello: String,

        #[facet(rename = "au_revoir")]
        goodbye: String,
    }

    let json = r#"{"bonjour":"monde","au_revoir":"world"}"#;

    let result: Greetings = from_str(json)?;

    assert_eq!(result.hello, "monde");
    assert_eq!(result.goodbye, "world");
}

/// Round-trip serialization then deserialization with a renamed field
#[cfg(feature = "std")]
#[test]
fn test_field_rename_roundtrip() {
    #[derive(Facet, Debug, PartialEq)]
    struct Greetings {
        #[facet(rename = "bonjour")]
        hello: String,
    }

    let original = Greetings {
        hello: "monde".to_string(),
    };

    let json = to_string(&original);
    assert_eq!(json, r#"{"bonjour":"monde"}"#);

    let roundtrip: Greetings = from_str(&json).unwrap();
    assert_eq!(original, roundtrip);
}

/// Deserialization with common naming conventions (kebab-case, snake_case, camelCase)
#[test]
fn test_field_rename_common_case_styles() {
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

    let result: SpecialNames = from_str(json)?;
    assert_eq!(result.kebab_case, "dash");
    assert_eq!(result.original_snake, "underscore");
    assert_eq!(result.camel_case, "hump");
}

/// Serialization and deserialization with special symbol characters in field name
#[test]
#[cfg(feature = "std")]
fn test_field_rename_with_symbol_chars_name() {
    #[derive(Debug, PartialEq, Facet)]
    struct SpecialCharsName {
        #[facet(rename = "@#$%^&")]
        special_chars: String,
    }

    let test_struct = SpecialCharsName {
        special_chars: "special value".to_string(),
    };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"@#$%^&":"special value"}"#);

    let roundtrip: SpecialCharsName = from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Serialization and deserialization with Unicode characters in field name (emoji)
#[test]
#[cfg(feature = "std")]
fn test_field_rename_with_unicode_name_emoji() {
    #[derive(Debug, PartialEq, Facet)]
    struct EmojiCharsName {
        #[facet(rename = "🏀")]
        ball: String,
    }

    let test_struct = EmojiCharsName {
        ball: "🏆".to_string(),
    };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"🏀":"🏆"}"#);

    let roundtrip: EmojiCharsName = from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Round-trip serialization/deserialization with raw identifiers as field names
#[cfg(feature = "std")]
#[test]
fn test_raw_identifier_fields_roundtrip() {
    #[derive(Facet, Debug, PartialEq)]
    struct RawIdentifiers {
        // Use rename because the JSON key won't have the r# prefix
        #[facet(rename = "type")]
        r#type: String,

        #[facet(rename = "match")]
        r#match: bool,
    }

    let original = RawIdentifiers {
        r#type: "keyword_value".to_string(),
        r#match: false,
    };

    // Serialization should use the renamed keys
    let json = to_string(&original);
    assert_eq!(json, r#"{"type":"keyword_value","match":false}"#);

    // Deserialization should correctly map back to raw identifiers
    let roundtrip: RawIdentifiers = from_str(&json).unwrap();
    assert_eq!(original, roundtrip);
}

/// Serialization and deserialization with Unicode characters in field name (Euro sign)
#[test]
#[cfg(feature = "std")]
fn test_field_rename_with_unicode_name_special_signs() {
    #[derive(Debug, PartialEq, Facet)]
    struct EmojiCharsName {
        #[facet(rename = "€℮↑→↓↔↕")]
        special_chars: String,
    }

    let test_struct = EmojiCharsName {
        special_chars: "...".to_string(),
    };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"€℮↑→↓↔↕":"..."}"#);

    let roundtrip: EmojiCharsName = from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Serialization and deserialization with numeric field name
#[cfg(feature = "std")]
#[test]
fn test_field_rename_with_numeric_name() {
    #[derive(Debug, PartialEq, Facet)]
    struct NumericName {
        #[facet(rename = "123")]
        numeric_name: i32,
    }

    let test_struct = NumericName { numeric_name: 42 };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"123":42}"#);

    let roundtrip: NumericName = from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}

/// Serialization and deserialization of renamed enum variants (unit and tuple variants)
#[cfg(feature = "std")]
#[test]
#[ignore]
fn test_enum_variant_rename() {
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
    let json = to_string(&green);
    assert_eq!(json, r#""lime""#);
    let roundtrip: Color = from_str(&json).unwrap();
    assert_eq!(green, roundtrip);

    // Test tuple variant with rename
    let blue = Color::Blue(255);
    let json = to_string(&blue);
    assert_eq!(json, r#"{"cyan":255}"#);
    let roundtrip: Color = from_str(&json).unwrap();
    assert_eq!(blue, roundtrip);
}

/// Serialization and deserialization of renamed fields in struct enum variants
#[cfg(feature = "std")]
#[test]
#[ignore]
fn test_enum_struct_variant_field_rename() {
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

    let json = to_string(&success);
    assert_eq!(
        json,
        r#"{"success":{"message":"Operation completed","code":200}}"#
    );

    let roundtrip: Message = from_str(&json).unwrap();
    assert_eq!(success, roundtrip);

    // Test error variant
    let error = Message::Error {
        msg: "Not found".to_string(),
        code: 404,
    };

    let json = to_string(&error);
    assert_eq!(
        json,
        r#"{"error":{"errorMessage":"Not found","errorCode":404}}"#
    );

    let roundtrip: Message = from_str(&json).unwrap();
    assert_eq!(error, roundtrip);
}

/// Serialization and deserialization of renamed fields in nested data structures
#[cfg(feature = "std")]
#[test]
fn test_field_rename_nested_structures() {
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

    let json = to_string(&person);
    let expected = r#"{"fullName":"John Doe","homeAddress":{"streetName":"Main St","zipCode":"12345"},"contactInfo":[{"type":"email","value":"john@example.com"},{"type":"phone","value":"555-1234"}]}"#;
    assert_eq!(json, expected);

    let roundtrip: Person = from_str(&json).unwrap();
    assert_eq!(person, roundtrip);
}

/// Serialization and deserialization of renamed optional fields (Some and None cases)
#[cfg(feature = "std")]
#[test]
fn test_field_rename_optional_values() {
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

    let json = to_string(&full);
    assert_eq!(
        json,
        r#"{"requiredField":"always here","optionalString":"optional value","optionalNumber":42}"#
    );

    let roundtrip: OptionalFields = from_str(&json).unwrap();
    assert_eq!(full, roundtrip);

    // Test with None fields
    let partial = OptionalFields {
        required: "always here".to_string(),
        maybe_string: None,
        maybe_number: None,
    };

    let json = to_string(&partial);
    assert_eq!(
        json,
        r#"{"requiredField":"always here","optionalString":null,"optionalNumber":null}"#
    );

    let roundtrip: OptionalFields = from_str(&json).unwrap();
    assert_eq!(partial, roundtrip);
}

/// Deserialization with extra fields in JSON that aren't in the target struct
#[test]
fn test_field_rename_ignore_extra_fields() {
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
#[ignore]
fn test_field_rename_serialization_priority() {
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

    let json = to_string(&model);
    assert_eq!(json, r#"{"data":["one","two"]}"#);
}

/// Proper errors are returned when required renamed fields are missing
#[test]
#[ignore]
fn test_field_rename_missing_required_error() {
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
        DeserErrorKind::MissingField(f) if f == "original_field"
    ));
    #[cfg(not(miri))]
    assert_snapshot!(e.to_string());
}

/// Rename to verify it's not an accidental alias
#[test]
fn test_field_rename_not_alias() {
    #[derive(Facet, Debug, PartialEq)]
    struct ABTesting {
        #[facet(rename = "b")]
        a: String,

        #[facet(rename = "c")]
        b: String,
    }

    let json = r#"{"b":"focus group 1","c":"focus group 2"}"#;

    let result: ABTesting = from_str(json)?;

    assert_eq!(result.a, "focus group 1");
    assert_eq!(result.b, "focus group 2");
}

/// Empty string rename test (which is valid in JSON)
#[test]
#[cfg(feature = "std")]
fn test_field_empty_string_rename() {
    #[derive(Debug, PartialEq, Facet)]
    struct EmptyStringField {
        #[facet(rename = "")]
        empty_key: String,

        normal_field: i32,
    }

    // Test with empty string key
    let test_struct = EmptyStringField {
        empty_key: "value for empty key".to_string(),
        normal_field: 42,
    };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"":"value for empty key","normal_field":42}"#);

    let roundtrip: EmptyStringField = from_str(&json).unwrap();
    assert_eq!(test_struct, roundtrip);
}
