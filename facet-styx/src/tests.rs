use super::*;
use facet::Facet;
use facet_format::DeserializeErrorKind;
use facet_testhelpers::test;
use styx_testhelpers::{ActualError, assert_annotated_errors, source_without_annotations};

mod metadata;

mod event_assert {
    use ariadne::{Color, Label, Report, ReportKind, Source};
    use facet_format::{ContainerKind, FormatParser, ParseEvent, ParseEventKind, ScalarValue};
    use similar::{ChangeTag, TextDiff};

    use crate::StyxParser;

    /// Format a ParseEvent to a string representation, ignoring spans.
    fn format_event(event: &ParseEvent<'_>) -> String {
        match &event.kind {
            ParseEventKind::StructStart(ContainerKind::Object) => "StructStart".to_string(),
            ParseEventKind::StructStart(kind) => format!("StructStart({:?})", kind),
            ParseEventKind::StructEnd => "StructEnd".to_string(),
            ParseEventKind::SequenceStart(ContainerKind::Array) => "SequenceStart".to_string(),
            ParseEventKind::SequenceStart(kind) => format!("SequenceStart({:?})", kind),
            ParseEventKind::SequenceEnd => "SequenceEnd".to_string(),
            ParseEventKind::FieldKey(key) => {
                if let Some(name) = key.name() {
                    format!("FieldKey({:?})", name.as_ref())
                } else if let Some(tag) = key.tag() {
                    if tag.is_empty() {
                        "FieldKey(@)".to_string()
                    } else {
                        format!("FieldKey(@{})", tag)
                    }
                } else {
                    "FieldKey(unit)".to_string()
                }
            }
            ParseEventKind::OrderedField => "OrderedField".to_string(),
            ParseEventKind::Scalar(ScalarValue::Unit) => "Scalar(unit)".to_string(),
            ParseEventKind::Scalar(ScalarValue::Null) => "Scalar(null)".to_string(),
            ParseEventKind::Scalar(ScalarValue::Bool(b)) => format!("Scalar({})", b),
            ParseEventKind::Scalar(ScalarValue::Char(c)) => format!("Scalar({:?})", c),
            ParseEventKind::Scalar(ScalarValue::I64(n)) => format!("Scalar({})", n),
            ParseEventKind::Scalar(ScalarValue::U64(n)) => format!("Scalar({}u)", n),
            ParseEventKind::Scalar(ScalarValue::I128(n)) => format!("Scalar({}i128)", n),
            ParseEventKind::Scalar(ScalarValue::U128(n)) => format!("Scalar({}u128)", n),
            ParseEventKind::Scalar(ScalarValue::F64(f)) => format!("Scalar({}f)", f),
            ParseEventKind::Scalar(ScalarValue::Str(s)) => format!("Scalar({:?})", s.as_ref()),
            ParseEventKind::Scalar(ScalarValue::Bytes(_)) => "Scalar(bytes)".to_string(),
            ParseEventKind::VariantTag(Some(name)) => format!("VariantTag({})", name),
            ParseEventKind::VariantTag(None) => "VariantTag(@)".to_string(),
        }
    }

    /// Collected event with owned data for storage.
    struct CollectedEvent {
        label: String,
        start: usize,
        end: usize,
    }

    /// Collect all events from a source string.
    fn collect_events(source: &str) -> Vec<CollectedEvent> {
        let mut parser = StyxParser::new(source);
        let mut events = Vec::new();
        loop {
            match parser.next_event() {
                Ok(Some(event)) => {
                    let label = format_event(&event);
                    let start = event.span.offset as usize;
                    let end = start + event.span.len as usize;
                    events.push(CollectedEvent { label, start, end });
                }
                Ok(None) => break,
                Err(e) => {
                    panic!("Parser error: {:?}", e);
                }
            }
        }
        events
    }

    /// Format a list of collected events to a multi-line string.
    fn format_events(events: &[CollectedEvent]) -> String {
        events
            .iter()
            .map(|e| e.label.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Normalize expected string: trim, dedent, remove empty lines at start/end.
    fn normalize_expected(expected: &str) -> String {
        let lines: Vec<&str> = expected.lines().collect();

        // Find minimum indentation (ignoring empty lines)
        let min_indent = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);

        // Dedent and filter empty lines at start/end
        let dedented: Vec<&str> = lines
            .iter()
            .map(|l| {
                if l.len() >= min_indent {
                    &l[min_indent..]
                } else {
                    l.trim()
                }
            })
            .collect();

        // Trim empty lines from start and end
        let start = dedented.iter().position(|l| !l.is_empty()).unwrap_or(0);
        let end = dedented
            .iter()
            .rposition(|l| !l.is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);

        dedented[start..end].join("\n")
    }

    /// Print each event with its span annotated on the source using ariadne.
    fn print_events_with_spans(source: &str, events: &[CollectedEvent]) {
        eprintln!("\n=== Events with spans ===\n");

        for (i, event) in events.iter().enumerate() {
            eprintln!("Event {}: {}", i + 1, event.label);

            if event.end > event.start {
                let mut buf = Vec::new();
                Report::build(
                    ReportKind::Custom("", Color::Cyan),
                    ("", event.start..event.end),
                )
                .with_label(
                    Label::new(("", event.start..event.end))
                        .with_message(&event.label)
                        .with_color(Color::Cyan),
                )
                .finish()
                .write(("", Source::from(source)), &mut buf)
                .unwrap();
                eprintln!("{}", String::from_utf8_lossy(&buf));
            } else {
                eprintln!("  (no span)\n");
            }
        }
    }

    fn indent(s: &str, prefix: &str) -> String {
        s.lines()
            .map(|l| format!("{}{}", prefix, l))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Assert that parse events match the expected string representation.
    /// On mismatch, shows expected, actual, and a diff.
    /// Set STYX_SHOW_SPANS=1 to see each event with its span annotated on the source.
    pub fn assert_events_eq_impl(source: &str, expected: &str) {
        let events = collect_events(source);
        let actual = format_events(&events);
        let expected_normalized = normalize_expected(expected);

        if actual == expected_normalized {
            return;
        }

        eprintln!("\n╭─ Events mismatch! ─────────────────────────────────────────╮\n");

        eprintln!("Expected:\n{}\n", indent(&expected_normalized, "    "));
        eprintln!("Actual:\n{}\n", indent(&actual, "    "));

        eprintln!("Diff:");
        let diff = TextDiff::from_lines(&expected_normalized, &actual);
        for change in diff.iter_all_changes() {
            let (sign, color) = match change.tag() {
                ChangeTag::Delete => ("-", "\x1b[31m"),
                ChangeTag::Insert => ("+", "\x1b[32m"),
                ChangeTag::Equal => (" ", ""),
            };
            let reset = if color.is_empty() { "" } else { "\x1b[0m" };
            eprint!("  {}{}{}{}", color, sign, change, reset);
        }
        eprintln!();

        if std::env::var("STYX_SHOW_SPANS").is_ok() {
            print_events_with_spans(source, &events);
        } else {
            eprintln!("Hint: rerun with STYX_SHOW_SPANS=1 to see spans annotated on source\n");
        }

        eprintln!("╰────────────────────────────────────────────────────────────╯\n");

        panic!("Events do not match expected");
    }
}

macro_rules! assert_events_eq {
    ($source:expr, $expected:expr) => {
        event_assert::assert_events_eq_impl($source, $expected)
    };
}

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

// =========================================================================
// Event assertion tests
// =========================================================================

#[test]
fn test_simple_key_value_events() {
    assert_events_eq!(
        "name hello",
        "
        StructStart
        FieldKey(\"name\")
        Scalar(\"hello\")
        StructEnd
        "
    );
}

#[test]
fn test_nested_object_events() {
    assert_events_eq!(
        "outer { inner value }",
        "
        StructStart
        FieldKey(\"outer\")
        StructStart
        FieldKey(\"inner\")
        Scalar(\"value\")
        StructEnd
        StructEnd
        "
    );
}

#[test]
fn test_explicit_root_object_only_one_structstart() {
    assert_events_eq!(
        "{key val}",
        "
        StructStart
        FieldKey(\"key\")
        Scalar(\"val\")
        StructEnd
        "
    );
}

#[test]
fn test_bare_true_is_string() {
    // In styx, all bare scalars are strings. "true" without quotes is the string "true",
    // not a boolean. Only the target type determines how it's interpreted.
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        active: String,
    }

    let input = "active true";
    let result: Config = from_str(input).unwrap();
    assert_eq!(result.active, "true");
}

#[test]
fn test_bare_false_is_string() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        active: String,
    }

    let input = "active false";
    let result: Config = from_str(input).unwrap();
    assert_eq!(result.active, "false");
}

#[test]
fn test_bare_number_is_string() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        port: String,
    }

    let input = "port 8080";
    let result: Config = from_str(input).unwrap();
    assert_eq!(result.port, "8080");
}

/// Test that @map(@TypeRef @optional(@OtherType)) has proper spacing between type references.
/// This is a regression test for a bug where type references (via #[facet(other)] variants)
/// didn't get proper spacing when serialized in maps.
#[test]
fn test_map_type_ref_spacing() {
    use crate::schema_types::{Documented, MapSchema, OptionalSchema, Schema};

    // Create a map with a type reference key and optional type reference value
    // This mimics: IndexMap<ColumnName, Option<FieldDef>> -> @map(@ColumnName @optional(@FieldDef))
    let map_schema = Schema::Map(MapSchema(vec![
        Documented::new(Schema::Type {
            name: Some("ColumnName".to_string()),
        }),
        Documented::new(Schema::Optional(OptionalSchema((Documented::new(
            Box::new(Schema::Type {
                name: Some("FieldDef".to_string()),
            }),
        ),)))),
    ]));

    let output = to_string(&map_schema).unwrap();
    eprintln!("Output: {}", output);

    // Check that there's a space between @ColumnName and @optional
    assert!(
        output.contains("@ColumnName @optional"),
        "Expected space between @ColumnName and @optional, got: {}",
        output
    );
}

/// Test that metadata containers with non-optional Span field work.
/// This is a regression test for issue #53 where Meta<String> with
/// `span: Span` (not Option<Span>) failed with "missing field `span`".
#[test]
fn test_metadata_container_non_optional_span() {
    use facet_reflect::Span;

    #[derive(Debug, Facet)]
    #[facet(metadata_container)]
    struct Meta<T> {
        pub value: T,
        #[facet(metadata = "span")]
        pub span: Span,
    }

    #[derive(Debug, Facet)]
    struct Config {
        name: Meta<String>,
    }

    let source = r#"{name "hello"}"#;
    let result: Result<Config, _> = from_str(source);
    match result {
        Ok(config) => {
            eprintln!("Success: {:?}", config);
            assert_eq!(config.name.value, "hello");
            // Span should cover the "hello" string (offset 6, len 7 including quotes)
            assert_eq!(config.name.span.offset, 6);
            assert_eq!(config.name.span.len, 7);
        }
        Err(e) => {
            panic!("Failed to parse: {}", e.render("<test>", source));
        }
    }
}

/// Test that metadata containers with non-optional Span work as map keys.
/// This is a more specific test for issue #53 where Meta<String> as a
/// flattened map key might fail.
#[test]
fn test_metadata_container_as_map_key() {
    use facet_reflect::Span;
    use indexmap::IndexMap;

    #[derive(Debug, Facet)]
    #[facet(metadata_container)]
    struct Meta<T> {
        pub value: T,
        #[facet(metadata = "span")]
        pub span: Span,
    }

    impl<T: PartialEq> PartialEq for Meta<T> {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }
    impl<T: Eq> Eq for Meta<T> {}
    impl<T: std::hash::Hash> std::hash::Hash for Meta<T> {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.value.hash(state);
        }
    }

    #[derive(Debug, Facet)]
    struct QueryFile {
        #[facet(flatten)]
        queries: IndexMap<Meta<String>, Decl>,
    }

    #[derive(Debug, Facet)]
    #[facet(rename_all = "lowercase")]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Decl {
        Select(Select),
    }

    #[derive(Debug, Facet)]
    struct Select {
        from: String,
    }

    let source = r#"{
        GetUsers @select{from users}
        GetPosts @select{from posts}
    }"#;

    let result: Result<QueryFile, _> = from_str(source);
    match result {
        Ok(file) => {
            eprintln!("Success: {:?}", file);
            assert_eq!(file.queries.len(), 2);

            let keys: Vec<_> = file.queries.keys().collect();
            assert_eq!(keys[0].value, "GetUsers");
            assert_eq!(keys[1].value, "GetPosts");

            // Check that spans were captured
            eprintln!("GetUsers span: {:?}", keys[0].span);
            eprintln!("GetPosts span: {:?}", keys[1].span);
        }
        Err(e) => {
            panic!("Failed to parse: {}", e.render("<test>", source));
        }
    }
}
