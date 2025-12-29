use std::borrow::Cow;
use std::collections::HashMap;

use facet::Facet;
use facet_json_legacy::{from_slice, from_str, from_str_borrowed};
use facet_testhelpers::test;

#[derive(Debug, Facet)]
struct BorrowedStr<'a> {
    name: &'a str,
}

#[test]
fn test_borrowed_str_deser() {
    let json = r#"{"name":"hello"}"#;
    let result: BorrowedStr = from_str_borrowed(json).unwrap();
    assert_eq!(result.name, "hello");
}

#[test]
fn test_borrowed_str_escaped_fails() {
    // String with escape sequence cannot be borrowed
    let json = r#"{"name":"hello\nworld"}"#;
    let result: Result<BorrowedStr, _> = from_str_borrowed(json);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("escape sequences"));
}

#[derive(Debug, Facet)]
struct CowStr<'a> {
    name: Cow<'a, str>,
}

#[test]
fn test_cow_str_borrowed() {
    // Unescaped string should be Cow::Borrowed
    let json = r#"{"name":"hello"}"#;
    let result: CowStr = from_str_borrowed(json).unwrap();
    assert!(matches!(result.name, Cow::Borrowed(_)));
    assert_eq!(&*result.name, "hello");
}

#[test]
fn test_cow_str_owned() {
    // Escaped string should be Cow::Owned
    let json = r#"{"name":"hello\nworld"}"#;
    let result: CowStr = from_str_borrowed(json).unwrap();
    assert!(matches!(result.name, Cow::Owned(_)));
    assert_eq!(&*result.name, "hello\nworld");
}

// Map key tests

#[test]
fn test_map_borrowed_str_keys() {
    let json = r#"{"foo":"value1","bar":"value2"}"#;
    let result: HashMap<&str, String> = from_str_borrowed(json).unwrap();
    assert_eq!(result.get("foo"), Some(&"value1".to_string()));
    assert_eq!(result.get("bar"), Some(&"value2".to_string()));
}

#[test]
fn test_map_cow_str_keys_borrowed() {
    let json = r#"{"foo":"value1","bar":"value2"}"#;
    let result: HashMap<Cow<str>, String> = from_str_borrowed(json).unwrap();
    // Keys should be borrowed since no escaping
    for key in result.keys() {
        assert!(
            matches!(key, Cow::Borrowed(_)),
            "key {key:?} should be borrowed"
        );
    }
}

#[test]
fn test_map_cow_str_keys_escaped() {
    let json = r#"{"foo\nbar":"value"}"#;
    let result: HashMap<Cow<str>, String> = from_str_borrowed(json).unwrap();
    // Key should be owned since it has escape sequence
    let key = result.keys().next().unwrap();
    assert!(matches!(key, Cow::Owned(_)));
    assert_eq!(&**key, "foo\nbar");
}

// ============================================================================
// Owned deserialization tests (from_slice / from_str - the new defaults)
// ============================================================================

/// An owned struct that can be deserialized without borrowing from input
#[derive(Debug, PartialEq, Facet)]
struct OwnedPerson {
    name: String,
    age: u32,
}

#[test]
fn test_from_str_basic() {
    let json = r#"{"name":"Alice","age":30}"#;
    let result: OwnedPerson = from_str(json).unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.age, 30);
}

#[test]
fn test_from_slice_basic() {
    let json = br#"{"name":"Bob","age":25}"#;
    let result: OwnedPerson = from_slice(json).unwrap();
    assert_eq!(result.name, "Bob");
    assert_eq!(result.age, 25);
}

#[test]
fn test_from_str_with_escapes() {
    // Escape sequences should work fine with owned deserialization
    let json = r#"{"name":"Hello\nWorld","age":42}"#;
    let result: OwnedPerson = from_str(json).unwrap();
    assert_eq!(result.name, "Hello\nWorld");
    assert_eq!(result.age, 42);
}

#[test]
fn test_from_slice_temporary_buffer() {
    // This simulates the axum use case: deserializing from a temporary buffer
    fn parse_request_body(body: &[u8]) -> OwnedPerson {
        from_slice(body).unwrap()
    }

    let body = br#"{"name":"Charlie","age":35}"#.to_vec();
    let person = parse_request_body(&body);
    // body is dropped here, but person survives because it's fully owned
    drop(body);
    assert_eq!(person.name, "Charlie");
    assert_eq!(person.age, 35);
}

#[test]
fn test_from_str_cow_works() {
    // Cow<'static, str> fields work with from_str
    #[derive(Debug, Facet)]
    struct WithCow {
        value: Cow<'static, str>,
    }

    let json = r#"{"value":"test"}"#;
    let result: WithCow = from_str(json).unwrap();
    // Cow<str> works fine - it can hold either borrowed or owned data
    // The 'static bound means it doesn't borrow from the input lifetime
    assert_eq!(&*result.value, "test");
}

#[test]
fn test_from_str_nested_struct() {
    #[derive(Debug, PartialEq, Facet)]
    struct Address {
        street: String,
        city: String,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct PersonWithAddress {
        name: String,
        address: Address,
    }

    let json = r#"{"name":"Dave","address":{"street":"123 Main St","city":"Springfield"}}"#;
    let result: PersonWithAddress = from_str(json).unwrap();
    assert_eq!(result.name, "Dave");
    assert_eq!(result.address.street, "123 Main St");
    assert_eq!(result.address.city, "Springfield");
}

#[test]
fn test_from_str_vec() {
    #[derive(Debug, PartialEq, Facet)]
    struct Names {
        items: Vec<String>,
    }

    let json = r#"{"items":["one","two","three"]}"#;
    let result: Names = from_str(json).unwrap();
    assert_eq!(result.items, vec!["one", "two", "three"]);
}
