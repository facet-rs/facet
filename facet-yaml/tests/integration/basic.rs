//! Basic deserialization tests for facet-yaml.
//!
//! These tests cover fundamental YAML parsing functionality.

use facet::Facet;
use facet_yaml::{from_slice, from_slice_borrowed, from_str, from_str_borrowed, to_string};
use std::sync::Arc;

// ============================================================================
// Basic struct deserialization
// ============================================================================

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[test]
fn test_deserialize_simple_struct() {
    let yaml = r#"
name: Alice
age: 30
"#;

    let person: Person = from_str(yaml).unwrap();
    assert_eq!(
        person,
        Person {
            name: "Alice".to_string(),
            age: 30
        }
    );
}

#[test]
fn test_deserialize_indented_yaml() {
    let yaml = r#"
        name: Bob
        age: 25
    "#;

    let person: Person = from_str(yaml).unwrap();
    assert_eq!(person.name, "Bob");
    assert_eq!(person.age, 25);
}

// ============================================================================
// Owned vs borrowed deserialization
// ============================================================================

#[derive(Debug, Facet, PartialEq)]
struct Config {
    name: String,
    port: u16,
}

fn load_config_from_temp_buffer() -> Config {
    // Simulate reading a config file into a temporary buffer
    let yaml_content = String::from("name: myapp\nport: 8080");
    from_str(&yaml_content).unwrap()
}

#[test]
fn test_owned_deserialization_from_temp_buffer() {
    let config = load_config_from_temp_buffer();
    assert_eq!(config.name, "myapp");
    assert_eq!(config.port, 8080);
}

#[test]
fn test_from_str_vs_from_str_borrowed() {
    let yaml = "name: test\nport: 3000";

    let config_owned: Config = from_str(yaml).unwrap();
    let config_borrowed: Config = from_str_borrowed(yaml).unwrap();

    assert_eq!(config_owned, config_borrowed);
}

#[test]
fn test_from_slice_utf8() {
    let yaml = b"name: test\nport: 3000";

    let config: Config = from_slice(yaml).unwrap();
    assert_eq!(config.name, "test");
    assert_eq!(config.port, 3000);
}

#[test]
fn test_from_slice_borrowed_utf8() {
    let yaml = b"name: test\nport: 3000";

    let config: Config = from_slice_borrowed(yaml).unwrap();
    assert_eq!(config.name, "test");
    assert_eq!(config.port, 3000);
}

// ============================================================================
// List deserialization
// ============================================================================

#[test]
fn test_deserialize_primitive_list() {
    let yaml = r#"
- 1
- 2
- 3
- 4
- 5
"#;

    let numbers: Vec<u64> = from_str(yaml).unwrap();
    assert_eq!(numbers, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_deserialize_struct_list() {
    let yaml = r#"
- name: Alice
  age: 30
- name: Bob
  age: 25
- name: Charlie
  age: 35
"#;

    let people: Vec<Person> = from_str(yaml).unwrap();
    assert_eq!(people.len(), 3);
    assert_eq!(people[0].name, "Alice");
    assert_eq!(people[1].name, "Bob");
    assert_eq!(people[2].name, "Charlie");
}

#[test]
fn test_deserialize_empty_list() {
    let yaml = r#"[]"#;

    let empty_list: Vec<u64> = from_str(yaml).unwrap();
    assert!(empty_list.is_empty());
}

#[test]
fn test_deserialize_nested_lists() {
    let yaml = r#"
-
  - 1
  - 2
-
  - 3
  - 4
"#;

    let nested: Vec<Vec<u64>> = from_str(yaml).unwrap();
    assert_eq!(nested, vec![vec![1, 2], vec![3, 4]]);
}

#[test]
fn test_deserialize_flow_list() {
    let yaml = "[1, 2, 3, 4, 5]";

    let numbers: Vec<u64> = from_str(yaml).unwrap();
    assert_eq!(numbers, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_deserialize_arc_slice() {
    let yaml = "[1, 2, 3, 4, 5]";

    let arc_slice: Arc<[i32]> = from_str(yaml).unwrap();
    assert_eq!(arc_slice.as_ref(), &[1, 2, 3, 4, 5]);
}

// ============================================================================
// Map deserialization
// ============================================================================

use std::collections::HashMap;

#[test]
fn test_deserialize_string_to_string_map() {
    let yaml = r#"
key1: value1
key2: value2
key3: value3
"#;

    let map: HashMap<String, String> = from_str(yaml).unwrap();
    assert_eq!(map.get("key1").unwrap(), "value1");
    assert_eq!(map.get("key2").unwrap(), "value2");
    assert_eq!(map.get("key3").unwrap(), "value3");
}

#[test]
fn test_deserialize_string_to_int_map() {
    let yaml = r#"
one: 1
two: 2
three: 3
"#;

    let map: HashMap<String, i32> = from_str(yaml).unwrap();
    assert_eq!(*map.get("one").unwrap(), 1);
    assert_eq!(*map.get("two").unwrap(), 2);
    assert_eq!(*map.get("three").unwrap(), 3);
}

#[test]
fn test_deserialize_flow_map() {
    let yaml = "{a: 1, b: 2, c: 3}";

    let map: HashMap<String, i32> = from_str(yaml).unwrap();
    assert_eq!(map.len(), 3);
    assert_eq!(*map.get("a").unwrap(), 1);
}

#[test]
fn test_deserialize_empty_map() {
    let yaml = "{}";

    let map: HashMap<String, String> = from_str(yaml).unwrap();
    assert!(map.is_empty());
}

// ============================================================================
// Nested struct deserialization
// ============================================================================

#[derive(Debug, Facet, PartialEq)]
struct Address {
    street: String,
    city: String,
}

#[derive(Debug, Facet, PartialEq)]
struct PersonWithAddress {
    name: String,
    address: Address,
}

#[test]
fn test_deserialize_nested_struct() {
    let yaml = r#"
name: Alice
address:
  street: "123 Main St"
  city: New York
"#;

    let person: PersonWithAddress = from_str(yaml).unwrap();
    assert_eq!(person.name, "Alice");
    assert_eq!(person.address.street, "123 Main St");
    assert_eq!(person.address.city, "New York");
}

// ============================================================================
// Optional fields
// ============================================================================

#[derive(Debug, Facet, PartialEq)]
struct OptionalFields {
    required: String,
    optional: Option<String>,
}

#[test]
fn test_deserialize_with_optional_present() {
    let yaml = r#"
required: hello
optional: world
"#;

    let obj: OptionalFields = from_str(yaml).unwrap();
    assert_eq!(obj.required, "hello");
    assert_eq!(obj.optional, Some("world".to_string()));
}

#[test]
fn test_deserialize_with_optional_missing() {
    let yaml = r#"
required: hello
"#;

    let obj: OptionalFields = from_str(yaml).unwrap();
    assert_eq!(obj.required, "hello");
    assert_eq!(obj.optional, None);
}

#[test]
fn test_deserialize_with_optional_null() {
    let yaml = r#"
required: hello
optional: null
"#;

    let obj: OptionalFields = from_str(yaml).unwrap();
    assert_eq!(obj.required, "hello");
    assert_eq!(obj.optional, None);
}

// ============================================================================
// Scalar types
// ============================================================================

#[test]
fn test_deserialize_bool_true() {
    let yaml = "true";
    let value: bool = from_str(yaml).unwrap();
    assert!(value);
}

#[test]
fn test_deserialize_bool_false() {
    let yaml = "false";
    let value: bool = from_str(yaml).unwrap();
    assert!(!value);
}

#[test]
fn test_deserialize_integers() {
    assert_eq!(from_str::<i32>("42").unwrap(), 42);
    assert_eq!(from_str::<i32>("-42").unwrap(), -42);
    // Large integers might be parsed as floats by YAML, so test within safe range
    assert_eq!(
        from_str::<i64>("9007199254740991").unwrap(),
        9007199254740991i64
    );
}

#[test]
fn test_deserialize_floats() {
    assert!((from_str::<f64>("3.25").unwrap() - 3.25).abs() < 0.001);
    assert!((from_str::<f64>("-2.5").unwrap() - (-2.5)).abs() < 0.001);
}

#[test]
fn test_deserialize_string() {
    assert_eq!(
        from_str::<String>(r#""hello world""#).unwrap(),
        "hello world"
    );
    assert_eq!(from_str::<String>("hello").unwrap(), "hello");
}

#[test]
fn test_deserialize_null_to_option() {
    // YAML null deserializes to None
    let opt: Option<String> = from_str("null").unwrap();
    assert_eq!(opt, None);
}

// ============================================================================
// Empty struct serialization
// ============================================================================

#[test]
fn test_empty_struct_roundtrip() {
    #[derive(Debug, Facet, PartialEq)]
    struct EmptyStruct {}

    #[derive(Debug, Facet, PartialEq)]
    struct ConfigWithEmpty {
        name: String,
        empty_field: Option<EmptyStruct>,
    }

    let config = ConfigWithEmpty {
        name: "test".to_string(),
        empty_field: Some(EmptyStruct {}),
    };

    let yaml = facet_yaml::to_string(&config).unwrap();

    // The empty struct should be inline: `empty_field: {}`
    // Not on a new line like:
    // empty_field:
    // {}
    assert!(
        yaml.contains("empty_field: {}"),
        "Expected 'empty_field: {{}}' inline, got:\n{}",
        yaml
    );

    // Should be able to parse it back
    let parsed: ConfigWithEmpty = facet_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed, config);
}

#[test]
fn test_empty_seq_roundtrip() {
    #[derive(Debug, Facet, PartialEq)]
    struct ConfigWithEmptySeq {
        name: String,
        items: Vec<String>,
    }

    let config = ConfigWithEmptySeq {
        name: "test".to_string(),
        items: vec![],
    };

    let yaml = facet_yaml::to_string(&config).unwrap();

    // The empty sequence should be inline: `items: []`
    assert!(
        yaml.contains("items: []"),
        "Expected 'items: []' inline, got:\n{}",
        yaml
    );

    // Should be able to parse it back
    let parsed: ConfigWithEmptySeq = facet_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed, config);
}

// ============================================================================
// Vec field serialization (issue #1588)
// ============================================================================

#[test]
fn test_serialize_struct_with_vec_field() {
    #[derive(Debug, Facet, PartialEq)]
    struct Workflow {
        name: String,
        steps: Vec<Step>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Step {
        name: String,
        run: Option<String>,
    }

    let workflow = Workflow {
        name: "Test Workflow".into(),
        steps: vec![
            Step {
                name: "First step".into(),
                run: Some("echo hello".into()),
            },
            Step {
                name: "Second step".into(),
                run: Some("echo world".into()),
            },
        ],
    };

    let yaml = facet_yaml::to_string(&workflow).unwrap();
    eprintln!("Generated YAML:\n{}", yaml);

    // Should have proper list markers
    assert!(
        yaml.contains("- name: First step"),
        "Expected '- name: First step', got:\n{}",
        yaml
    );
    assert!(
        yaml.contains("- name: Second step"),
        "Expected '- name: Second step', got:\n{}",
        yaml
    );

    // Should be able to round-trip
    let parsed: Workflow = facet_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed, workflow);
}

#[test]
fn test_serialize_vec_of_scalars() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
        tags: Vec<String>,
    }

    let config = Config {
        name: "myapp".into(),
        tags: vec!["rust".into(), "yaml".into(), "facet".into()],
    };

    let yaml = facet_yaml::to_string(&config).unwrap();
    eprintln!("Generated YAML:\n{}", yaml);

    // Each tag should be on its own line with a list marker
    assert!(yaml.contains("- rust"), "Expected '- rust', got:\n{}", yaml);
    assert!(yaml.contains("- yaml"), "Expected '- yaml', got:\n{}", yaml);
    assert!(
        yaml.contains("- facet"),
        "Expected '- facet', got:\n{}",
        yaml
    );

    // Should be able to round-trip
    let parsed: Config = facet_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed, config);
}

#[test]
fn test_nested_struct_with_vec_serialization() {
    // This mirrors NestedParent from format_suite
    #[derive(Debug, Facet, PartialEq)]
    struct NestedParent {
        id: u64,
        child: NestedChild,
        tags: Vec<String>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct NestedChild {
        code: String,
        active: bool,
    }

    let value = NestedParent {
        id: 42,
        child: NestedChild {
            code: "alpha".into(),
            active: true,
        },
        tags: vec!["core".into(), "json".into()],
    };

    let yaml = facet_yaml::to_string(&value).unwrap();
    eprintln!("Generated YAML:\n{}", yaml);

    // Verify correct structure - nested struct fields properly indented
    assert!(yaml.contains("id: 42"), "Expected 'id: 42', got:\n{}", yaml);
    assert!(yaml.contains("child:"), "Expected 'child:', got:\n{}", yaml);
    // Nested fields should be indented
    assert!(
        yaml.contains("  code: alpha"),
        "Expected '  code: alpha' (indented), got:\n{}",
        yaml
    );
    assert!(
        yaml.contains("  active: true"),
        "Expected '  active: true' (indented), got:\n{}",
        yaml
    );
    assert!(yaml.contains("tags:"), "Expected 'tags:', got:\n{}", yaml);
    // List items should be indented
    assert!(
        yaml.contains("  - core"),
        "Expected '  - core' (indented), got:\n{}",
        yaml
    );
    assert!(
        yaml.contains("  - json"),
        "Expected '  - json' (indented), got:\n{}",
        yaml
    );

    // Roundtrip test - deserialize and verify we get the same value back
    let parsed: NestedParent = facet_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed, value, "Roundtrip failed");
}

// ============================================================================
// YAML-specific features
// ============================================================================

#[test]
fn test_yaml_multiline_string() {
    let yaml = r#"
text: |
  This is a
  multiline string
  with newlines preserved
"#;

    #[derive(Debug, Facet)]
    struct Doc {
        text: String,
    }

    let doc: Doc = from_str(yaml).unwrap();
    assert!(doc.text.contains("This is a"));
    assert!(doc.text.contains("\n"));
}

#[test]
fn test_yaml_folded_string() {
    let yaml = r#"
text: >
  This is a
  folded string
  that becomes one line
"#;

    #[derive(Debug, Facet)]
    struct Doc {
        text: String,
    }

    let doc: Doc = from_str(yaml).unwrap();
    // Folded strings join lines with spaces
    assert!(doc.text.contains("This is a"));
}

// ============================================================================
// Multiline string serialization tests
// ============================================================================

#[test]
fn test_serialize_multiline_string_uses_block_scalar() {
    #[derive(Debug, Facet)]
    struct Doc {
        script: String,
    }

    let doc = Doc {
        script: "#!/bin/bash\nset -e\necho 'hello'".to_string(),
    };

    let yaml = to_string(&doc).unwrap();
    println!("Serialized YAML:\n{}", yaml);

    // Should use literal block scalar syntax, not escaped newlines
    assert!(
        yaml.contains("script: |-"),
        "Expected block scalar syntax (|-), got:\n{}",
        yaml
    );
    assert!(
        yaml.contains("#!/bin/bash"),
        "Should contain script content literally"
    );
    assert!(!yaml.contains("\\n"), "Should NOT contain escaped newlines");
}

#[test]
fn test_serialize_multiline_string_with_trailing_newline() {
    #[derive(Debug, Facet)]
    struct Doc {
        text: String,
    }

    // String with exactly one trailing newline
    let doc = Doc {
        text: "line1\nline2\n".to_string(),
    };

    let yaml = to_string(&doc).unwrap();
    println!("Serialized YAML:\n{}", yaml);

    // Should use | (clip) for single trailing newline
    assert!(yaml.contains("text: |"), "Expected block scalar syntax (|)");
    assert!(
        !yaml.contains("text: |-"),
        "Should use clip (|), not strip (|-)"
    );
    assert!(
        !yaml.contains("text: |+"),
        "Should use clip (|), not keep (|+)"
    );
}

#[test]
fn test_serialize_multiline_string_roundtrip() {
    #[derive(Debug, Facet, PartialEq)]
    struct Doc {
        script: String,
    }

    let original = Doc {
        script: "#!/bin/bash\nset -euo pipefail\n\necho 'Verifying binaries'\nmissing=0\n\nif [[ ! -x dist/app ]]; then\n  echo 'MISSING: app'\n  missing=1\nfi".to_string(),
    };

    let yaml = to_string(&original).unwrap();
    println!("Serialized YAML:\n{}", yaml);

    // Should be readable block scalar format
    assert!(yaml.contains("script: |-"), "Expected block scalar syntax");
    assert!(!yaml.contains("\\n"), "Should NOT contain escaped newlines");

    // Should roundtrip correctly
    let parsed: Doc = from_str(&yaml).unwrap();
    assert_eq!(
        original.script, parsed.script,
        "Multiline string should roundtrip exactly"
    );
}

#[test]
fn test_serialize_nested_struct_with_multiline() {
    #[derive(Debug, Facet, PartialEq)]
    struct Job {
        name: String,
        run: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Workflow {
        job: Job,
    }

    let workflow = Workflow {
        job: Job {
            name: "build".to_string(),
            run: "npm install\nnpm run build\nnpm test".to_string(),
        },
    };

    let yaml = to_string(&workflow).unwrap();
    println!("Serialized YAML:\n{}", yaml);

    // Check proper indentation in nested context
    assert!(
        yaml.contains("run: |-"),
        "Expected block scalar syntax for nested field"
    );

    // Roundtrip
    let parsed: Workflow = from_str(&yaml).unwrap();
    assert_eq!(workflow.job.run, parsed.job.run);
}

#[test]
fn test_serialize_single_line_string_unchanged() {
    #[derive(Debug, Facet)]
    struct Doc {
        name: String,
    }

    let doc = Doc {
        name: "simple value".to_string(),
    };

    let yaml = to_string(&doc).unwrap();
    println!("Serialized YAML:\n{}", yaml);

    // Single-line strings should NOT use block scalar
    assert!(
        !yaml.contains("|"),
        "Single-line string should NOT use block scalar"
    );
    assert!(
        yaml.contains("name: simple value"),
        "Should be plain inline string"
    );
}

#[test]
fn test_serialize_carriage_return_string_uses_quotes() {
    #[derive(Debug, Facet)]
    struct Doc {
        text: String,
    }

    let doc = Doc {
        text: "line1\r\nline2".to_string(),
    };

    let yaml = to_string(&doc).unwrap();
    println!("Serialized YAML:\n{}", yaml);

    // Strings with carriage returns should use quoted format, not block scalar
    assert!(
        !yaml.contains("|"),
        "String with \\r should NOT use block scalar"
    );
    assert!(yaml.contains("\\r"), "Should have escaped carriage return");
}
