use super::*;
use facet::Facet;
use facet_format::DeserializeErrorKind;
use facet_testhelpers::test;
use styx_testhelpers::{ActualError, assert_annotated_errors, source_without_annotations};

#[derive(Facet, Debug, PartialEq)]
struct Simple {
    name: String,
    value: i32,
}

#[derive(Facet, Debug, PartialEq)]
struct WithOptional {
    required: String,
    optional: Option<i32>,
}

#[derive(Facet, Debug, PartialEq)]
struct Nested {
    inner: Simple,
}

fn deserialize_error_kind_name(kind: &DeserializeErrorKind) -> &'static str {
    match kind {
        DeserializeErrorKind::MissingField { .. } => "MissingField",
        DeserializeErrorKind::UnknownField { .. } => "UnknownField",
        DeserializeErrorKind::TypeMismatch { .. } => "TypeMismatch",
        DeserializeErrorKind::Reflect { .. } => "Reflect",
        DeserializeErrorKind::UnexpectedEof { .. } => "UnexpectedEof",
        DeserializeErrorKind::Unsupported { .. } => "Unsupported",
        DeserializeErrorKind::CannotBorrow { .. } => "CannotBorrow",
        DeserializeErrorKind::UnexpectedToken { .. } => "UnexpectedToken",
        DeserializeErrorKind::InvalidValue { .. } => "InvalidValue",
        _ => "DeserializeError",
    }
}

fn assert_deserialize_errors(annotated_source: &str, error: &facet_format::DeserializeError) {
    let span = error
        .span
        .as_ref()
        .map(|span| {
            let start = span.offset as usize;
            let end = start + span.len as usize;
            start..end
        })
        .unwrap_or(0..1);

    let actual_errors = vec![ActualError {
        span,
        kind: deserialize_error_kind_name(&error.kind).to_string(),
    }];

    assert_annotated_errors(annotated_source, actual_errors);
}

#[test]
fn test_simple_struct() {
    let input = "name hello\nvalue 42";
    let result: Simple = from_str(input).unwrap();
    assert_eq!(result.name, "hello");
    assert_eq!(result.value, 42);
}

#[test]
fn test_deserialize_type_mismatch_span() {
    #[derive(Facet, Debug, PartialEq)]
    struct IntOnly {
        value: i32,
    }

    let annotated = r#"
value "hello"
^^^^^ Reflect
"#;
    let source = source_without_annotations(annotated);
    let err = from_str::<IntOnly>(&source).unwrap_err();
    assert_deserialize_errors(annotated, &err);
}

#[test]
fn test_quoted_string() {
    let input = r#"name "hello world"
value 123"#;
    let result: Simple = from_str(input).unwrap();
    assert_eq!(result.name, "hello world");
    assert_eq!(result.value, 123);
}

#[test]
fn test_optional_present() {
    let input = "required hello\noptional 42";
    let result: WithOptional = from_str(input).unwrap();
    assert_eq!(result.required, "hello");
    assert_eq!(result.optional, Some(42));
}

#[test]
fn test_optional_absent() {
    let input = "required hello";
    let result: WithOptional = from_str(input).unwrap();
    assert_eq!(result.required, "hello");
    assert_eq!(result.optional, None);
}

#[test]
fn test_bool_values() {
    #[derive(Facet, Debug, PartialEq)]
    struct Flags {
        enabled: bool,
        debug: bool,
    }

    let input = "enabled true\ndebug false";
    let result: Flags = from_str(input).unwrap();
    assert!(result.enabled);
    assert!(!result.debug);
}

#[test]
fn test_vec() {
    #[derive(Facet, Debug, PartialEq)]
    struct WithVec {
        items: Vec<i32>,
    }

    let input = "items (1 2 3)";
    let result: WithVec = from_str(input).unwrap();
    assert_eq!(result.items, vec![1, 2, 3]);
}

#[test]
fn test_schema_directive_skipped() {
    // @schema directive should be skipped during deserialization
    // See: https://github.com/bearcove/styx/issues/3
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        name: String,
        port: u16,
    }

    let input = r#"@schema {source crate:test@1, cli test}

name myapp
port 8080"#;
    let result: Config = from_str(input).unwrap();
    assert_eq!(result.name, "myapp");
    assert_eq!(result.port, 8080);
}

#[test]
fn test_schema_directive_skipped_in_config_value() {
    // @schema at top level should be skipped even when parsing into ConfigValue
    use figue::ConfigValue;

    let input = r#"@schema {id crate:dibs@1, cli dibs}

db {
    crate reef-db
}
"#;
    let result: ConfigValue = from_str(input).unwrap();

    // Verify @schema was skipped, only db remains
    if let ConfigValue::Object(obj) = result {
        assert!(
            !obj.value.contains_key("@schema"),
            "Expected '@schema' to be skipped, got: {:?}",
            obj.value.keys().collect::<Vec<_>>()
        );
        assert!(
            obj.value.contains_key("db"),
            "Expected 'db' key, got: {:?}",
            obj.value.keys().collect::<Vec<_>>()
        );
    } else {
        panic!("Expected ConfigValue::Object, got: {:?}", result);
    }
}

// =========================================================================
// Expression mode tests
// =========================================================================

#[test]
fn test_from_str_expr_scalar() {
    let num: i32 = from_str_expr("42").unwrap();
    assert_eq!(num, 42);

    let s: String = from_str_expr("hello").unwrap();
    assert_eq!(s, "hello");

    let b: bool = from_str_expr("true").unwrap();
    assert!(b);
}

#[test]
fn test_from_str_expr_object() {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    let point: Point = from_str_expr("{x 10, y 20}").unwrap();
    assert_eq!(point.x, 10);
    assert_eq!(point.y, 20);
}

#[test]
fn test_from_str_expr_sequence() {
    let items: Vec<i32> = from_str_expr("(1 2 3)").unwrap();
    assert_eq!(items, vec![1, 2, 3]);
}

#[test]
fn test_expr_roundtrip() {
    // Serialize with expr mode, deserialize with expr mode
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        name: String,
        port: u16,
    }

    let original = Config {
        name: "test".into(),
        port: 8080,
    };

    // Serialize as expression (with braces)
    let serialized = to_string_compact(&original).unwrap();
    assert!(serialized.starts_with('{'));

    // Parse back as expression
    let parsed: Config = from_str_expr(&serialized).unwrap();
    assert_eq!(original, parsed);
}

// =========================================================================
// Documented<T> tests
// =========================================================================

#[test]
fn test_documented_basic() {
    // Documented<T> should have the metadata_container flag
    let shape = <Documented<String>>::SHAPE;
    assert!(shape.is_metadata_container());
}

#[test]
fn test_documented_helper_methods() {
    let doc = Documented::new(42);
    assert_eq!(*doc.value(), 42);
    assert!(doc.doc().is_none());

    let doc = Documented::with_doc(42, vec!["The answer".into()]);
    assert_eq!(*doc.value(), 42);
    assert_eq!(doc.doc(), Some(&["The answer".to_string()][..]));

    let doc = Documented::with_doc_line(42, "The answer");
    assert_eq!(doc.doc(), Some(&["The answer".to_string()][..]));
}

#[test]
fn test_documented_deref() {
    let doc = Documented::new("hello".to_string());
    // Deref should give us access to the inner value
    assert_eq!(doc.len(), 5);
    assert!(doc.starts_with("hel"));
}

#[test]
fn test_documented_from() {
    let doc: Documented<i32> = 42.into();
    assert_eq!(*doc.value(), 42);
    assert!(doc.doc().is_none());
}

#[test]
fn test_documented_map() {
    let doc = Documented::with_doc_line(42, "The answer");
    let mapped = doc.map(|x| x.to_string());
    assert_eq!(*mapped.value(), "42");
    assert_eq!(mapped.doc(), Some(&["The answer".to_string()][..]));
}

#[test]
fn test_unit_field_followed_by_another_field() {
    // When a field has unit value (no explicit value), followed by
    // another field on the next line, both should be parsed correctly.
    use std::collections::HashMap;

    #[derive(Facet, Debug, PartialEq)]
    struct Fields {
        #[facet(flatten)]
        fields: HashMap<String, Option<String>>,
    }

    let input = "foo\nbar baz";
    let result: Fields = from_str(input).unwrap();

    assert_eq!(result.fields.len(), 2);
    assert_eq!(result.fields.get("foo"), Some(&None));
    assert_eq!(result.fields.get("bar"), Some(&Some("baz".to_string())));
}

#[test]
fn test_map_schema_spacing() {
    // When serializing a map with a unit-payload tag key (like @string)
    // followed by another type, there should be proper spacing.
    // i.e., `@map(@string @enum{...})` NOT `@map(@string@enum{...})`
    use crate::schema_types::{Documented, EnumSchema, MapSchema, Schema};
    use std::collections::HashMap;

    let mut enum_variants = HashMap::new();
    enum_variants.insert(Documented::new("a".to_string()), Schema::Unit);
    enum_variants.insert(Documented::new("b".to_string()), Schema::Unit);

    let map_schema = Schema::Map(MapSchema(vec![
        Documented::new(Schema::String(None)), // Key type: @string (no payload)
        Documented::new(Schema::Enum(EnumSchema(enum_variants))), // Value type: @enum{...}
    ]));

    let output = to_string(&map_schema).unwrap();

    // Check that there's a space between @string and @enum
    assert!(
        output.contains("@string @enum"),
        "Expected space between @string and @enum, got: {}",
        output
    );
}

/// Test that Documented<String> works as a flattened map key (baseline).
#[test]
fn test_documented_as_flattened_map_key() {
    use indexmap::IndexMap;

    #[derive(Facet, Debug)]
    struct DocMap {
        #[facet(flatten)]
        items: IndexMap<Documented<String>, String>,
    }

    let source = r#"{foo bar, baz qux}"#;
    let result: Result<DocMap, _> = from_str(source);
    match &result {
        Ok(map) => {
            assert_eq!(map.items.len(), 2);
        }
        Err(e) => {
            panic!(
                "Documented<String> as map key failed: {}",
                e.render("<test>", source)
            );
        }
    }
}

/// Test that Spanned<String> works as a flattened map key.
///
/// This is a regression test for an issue where metadata containers with
/// span metadata failed to work as map keys in flattened maps.
#[test]
fn test_spanned_as_flattened_map_key() {
    use facet_reflect::Span;
    use indexmap::IndexMap;

    #[derive(Debug, Clone, Facet)]
    #[facet(metadata_container)]
    struct Spanned<T> {
        pub value: T,
        #[facet(metadata = "span")]
        pub span: Option<Span>,
    }

    impl<T: PartialEq> PartialEq for Spanned<T> {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }
    impl<T: Eq> Eq for Spanned<T> {}
    impl<T: std::hash::Hash> std::hash::Hash for Spanned<T> {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.value.hash(state);
        }
    }

    #[derive(Facet, Debug)]
    struct SpannedMap {
        #[facet(flatten)]
        items: IndexMap<Spanned<String>, String>,
    }

    let source = r#"{foo bar, baz qux}"#;
    let result: Result<SpannedMap, _> = from_str(source);
    match &result {
        Ok(map) => {
            assert_eq!(map.items.len(), 2);
            let keys: Vec<_> = map.items.keys().map(|k| k.value.as_str()).collect();
            assert!(keys.contains(&"foo"));
            assert!(keys.contains(&"baz"));
        }
        Err(e) => {
            panic!(
                "Spanned<String> as map key failed: {}",
                e.render("<test>", source)
            );
        }
    }
}

/// Test metadata container with both span and doc metadata as map key.
#[test]
fn test_metadata_container_with_span_and_doc_as_map_key() {
    use facet_reflect::Span;
    use indexmap::IndexMap;

    #[derive(Debug, Clone, Facet)]
    #[facet(metadata_container)]
    struct SpannedDoc<T> {
        pub value: T,
        #[facet(metadata = "span")]
        pub span: Option<Span>,
        #[facet(metadata = "doc")]
        pub doc: Option<Vec<String>>,
    }

    impl<T: PartialEq> PartialEq for SpannedDoc<T> {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }
    impl<T: Eq> Eq for SpannedDoc<T> {}
    impl<T: std::hash::Hash> std::hash::Hash for SpannedDoc<T> {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.value.hash(state);
        }
    }

    #[derive(Facet, Debug)]
    struct SpannedDocMap {
        #[facet(flatten)]
        items: IndexMap<SpannedDoc<String>, String>,
    }

    let source = r#"{foo bar, baz qux}"#;
    let result: Result<SpannedDocMap, _> = from_str(source);
    match &result {
        Ok(map) => {
            assert_eq!(map.items.len(), 2);
        }
        Err(e) => {
            panic!(
                "SpannedDoc<String> as map key failed: {}",
                e.render("<test>", source)
            );
        }
    }
}
