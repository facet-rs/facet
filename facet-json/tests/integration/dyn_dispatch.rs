//! Tests for dynamic dispatch deserializer.
//!
//! Verifies that `&mut dyn DynParser` can be used with `FormatDeserializer`
//! to reduce monomorphization.

use facet::Facet;
use facet_format::{DynDeserializeError, DynParser, FormatDeserializer};
use facet_json::JsonParser;

#[derive(Debug, PartialEq, Facet)]
struct Person {
    name: String,
    age: u32,
}

#[test]
fn dyn_dispatch_basic() {
    let input = r#"{"name": "Alice", "age": 30}"#;

    // Create a concrete parser
    let mut parser = JsonParser::new(input.as_bytes());

    // Erase to dyn DynParser
    let dyn_parser: &mut dyn DynParser = &mut parser;

    // Use the dyn parser with FormatDeserializer
    let mut de = FormatDeserializer::new(dyn_parser);

    let person: Result<Person, DynDeserializeError> = de.deserialize();
    let person = person.expect("deserialization should succeed");

    assert_eq!(person.name, "Alice");
    assert_eq!(person.age, 30);
}

#[derive(Debug, PartialEq, Facet)]
struct Nested {
    inner: Person,
    tag: String,
}

#[test]
fn dyn_dispatch_nested() {
    let input = r#"{"inner": {"name": "Bob", "age": 25}, "tag": "test"}"#;

    let mut parser = JsonParser::new(input.as_bytes());
    let dyn_parser: &mut dyn DynParser = &mut parser;
    let mut de = FormatDeserializer::new(dyn_parser);

    let nested: Result<Nested, DynDeserializeError> = de.deserialize();
    let nested = nested.expect("deserialization should succeed");

    assert_eq!(nested.inner.name, "Bob");
    assert_eq!(nested.inner.age, 25);
    assert_eq!(nested.tag, "test");
}

#[derive(Debug, PartialEq, Facet)]
struct WithVec {
    items: Vec<i32>,
}

#[test]
fn dyn_dispatch_vec() {
    let input = r#"{"items": [1, 2, 3, 4, 5]}"#;

    let mut parser = JsonParser::new(input.as_bytes());
    let dyn_parser: &mut dyn DynParser = &mut parser;
    let mut de = FormatDeserializer::new(dyn_parser);

    let result: Result<WithVec, DynDeserializeError> = de.deserialize();
    let result = result.expect("deserialization should succeed");

    assert_eq!(result.items, vec![1, 2, 3, 4, 5]);
}

#[derive(Debug, PartialEq, Facet)]
#[facet(tag = "type")]
#[repr(C)]
enum Message {
    Text { content: String },
    Number { value: i32 },
}

#[test]
fn dyn_dispatch_enum() {
    let input = r#"{"type": "Text", "content": "hello"}"#;

    let mut parser = JsonParser::new(input.as_bytes());
    let dyn_parser: &mut dyn DynParser = &mut parser;
    let mut de = FormatDeserializer::new(dyn_parser);

    let msg: Result<Message, DynDeserializeError> = de.deserialize();
    let msg = msg.expect("deserialization should succeed");

    assert_eq!(
        msg,
        Message::Text {
            content: "hello".to_string()
        }
    );
}

/// Helper function that demonstrates using dyn dispatch in a generic context.
///
/// This function only needs one monomorphization regardless of how many
/// parser types call it.
fn deserialize_with_dyn<'de, T: facet_core::Facet<'de>>(
    parser: &mut dyn DynParser<'de>,
) -> Result<T, DynDeserializeError> {
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}

#[test]
fn dyn_dispatch_helper_function() {
    let input = r#"{"name": "Charlie", "age": 35}"#;
    let mut parser = JsonParser::new(input.as_bytes());

    // Call through a helper that uses dyn dispatch internally
    let person: Person = deserialize_with_dyn(&mut parser).expect("should work");

    assert_eq!(person.name, "Charlie");
    assert_eq!(person.age, 35);
}
