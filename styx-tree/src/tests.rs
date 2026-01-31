use super::*;
use facet_testhelpers::test;

#[test]
fn test_parse_simple() {
    let doc = Document::parse("name Alice\nage 30").unwrap();
    assert_eq!(doc.get("name").and_then(|v| v.as_str()), Some("Alice"));
    assert_eq!(doc.get("age").and_then(|v| v.as_str()), Some("30"));
}

#[test]
fn test_parse_empty() {
    let doc = Document::parse("").unwrap();
    assert!(doc.root.is_empty());
}

#[test]
fn test_convenience_parse() {
    let value = parse("greeting hello").unwrap();
    assert_eq!(
        value.get("greeting").and_then(|v| v.as_str()),
        Some("hello")
    );
}

#[test]
fn test_schema_tree_structure() {
    // Parse a schema-like document to understand the tree structure
    // Structure:
    //   schema {
    //     @ @object{         // @ is unit key, @object{...} is the value (tag with object payload)
    //       name @string
    //     }
    //   }
    let source = r#"schema {
  @ @object{
    name @string
  }
}"#;
    let value = parse(source).unwrap();

    // Root is an object with one entry: "schema"
    let obj = value.as_object().expect("root should be object");
    assert_eq!(obj.len(), 1);

    // "schema" value is an object
    let schema = obj.get("schema").expect("should have schema key");
    let schema_obj = schema.as_object().expect("schema should be object");

    // schema has one entry with a unit key
    assert_eq!(schema_obj.len(), 1);
    let entry = &schema_obj.entries[0];

    // Key is unit (@ as a key means unit key)
    assert!(
        entry.key.is_unit(),
        "key should be unit, got {:?}",
        entry.key
    );

    // Value is @object{...} - a tagged value with tag "object" and object payload
    assert_eq!(
        entry.value.tag_name(),
        Some("object"),
        "value should have tag 'object'"
    );

    // The payload of @object{...} is the inner object { name @string }
    let payload = entry
        .value
        .payload
        .as_ref()
        .expect("@object should have payload");
    let payload_obj = match payload {
        value::Payload::Object(obj) => obj,
        _ => panic!("payload should be object, got {:?}", payload),
    };
    assert_eq!(payload_obj.len(), 1);

    // "name" entry
    let name_entry = &payload_obj.entries[0];
    assert_eq!(name_entry.key.as_str(), Some("name"));

    // Value is tagged with "string", no payload
    assert_eq!(
        name_entry.value.tag_name(),
        Some("string"),
        "@string should have tag 'string'"
    );
    assert!(
        name_entry.value.payload.is_none(),
        "@string should have no payload"
    );
}
