use std::borrow::Cow;
use std::collections::HashMap;

use facet::Facet;
use facet_json::from_str;
use facet_testhelpers::test;

#[derive(Debug, Facet)]
struct BorrowedStr<'a> {
    name: &'a str,
}

#[test]
fn test_borrowed_str_deser() {
    let json = r#"{"name":"hello"}"#;
    let result: BorrowedStr = from_str(json).unwrap();
    assert_eq!(result.name, "hello");
}

#[test]
fn test_borrowed_str_escaped_fails() {
    // String with escape sequence cannot be borrowed
    let json = r#"{"name":"hello\nworld"}"#;
    let result: Result<BorrowedStr, _> = from_str(json);
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
    let result: CowStr = from_str(json).unwrap();
    assert!(matches!(result.name, Cow::Borrowed(_)));
    assert_eq!(&*result.name, "hello");
}

#[test]
fn test_cow_str_owned() {
    // Escaped string should be Cow::Owned
    let json = r#"{"name":"hello\nworld"}"#;
    let result: CowStr = from_str(json).unwrap();
    assert!(matches!(result.name, Cow::Owned(_)));
    assert_eq!(&*result.name, "hello\nworld");
}

// Map key tests

#[test]
fn test_map_borrowed_str_keys() {
    let json = r#"{"foo":"value1","bar":"value2"}"#;
    let result: HashMap<&str, String> = from_str(json).unwrap();
    assert_eq!(result.get("foo"), Some(&"value1".to_string()));
    assert_eq!(result.get("bar"), Some(&"value2".to_string()));
}

#[test]
fn test_map_cow_str_keys_borrowed() {
    let json = r#"{"foo":"value1","bar":"value2"}"#;
    let result: HashMap<Cow<str>, String> = from_str(json).unwrap();
    // Keys should be borrowed since no escaping
    for key in result.keys() {
        assert!(
            matches!(key, Cow::Borrowed(_)),
            "key {:?} should be borrowed",
            key
        );
    }
}

#[test]
fn test_map_cow_str_keys_escaped() {
    let json = r#"{"foo\nbar":"value"}"#;
    let result: HashMap<Cow<str>, String> = from_str(json).unwrap();
    // Key should be owned since it has escape sequence
    let key = result.keys().next().unwrap();
    assert!(matches!(key, Cow::Owned(_)));
    assert_eq!(&**key, "foo\nbar");
}
