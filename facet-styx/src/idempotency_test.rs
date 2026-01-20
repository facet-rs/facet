//! Idempotency tests for StyxWriter output.
//!
//! These tests verify that the output from facet-styx serialization is already
//! in canonical form - i.e., running it through the CST formatter produces
//! no changes. This ensures StyxWriter follows the same formatting rules as
//! the CST formatter.

use std::collections::HashMap;

use crate::schema_types::Documented;
use crate::to_string;
use facet::Facet;
use styx_format::{FormatOptions, format_source};

/// Assert that serializing a value produces canonical output.
/// The output should be unchanged when run through the CST formatter.
fn assert_idempotent<'facet, T: Facet<'facet>>(value: &T, description: &str) {
    let serialized = to_string(value).expect("serialization should succeed");
    let formatted = format_source(&serialized, FormatOptions::default());

    assert_eq!(
        serialized, formatted,
        "StyxWriter output is not canonical for {}!\n\nOriginal:\n{}\n\nAfter formatting:\n{}",
        description, serialized, formatted
    );
}


// =============================================================================
// Basic struct tests
// =============================================================================

#[test]
fn idempotent_simple_struct() {
    #[derive(Facet)]
    struct Config {
        name: String,
        port: u16,
    }

    let config = Config {
        name: "myapp".into(),
        port: 8080,
    };
    assert_idempotent(&config, "simple struct");
}

#[test]
fn idempotent_struct_with_string_requiring_quotes() {
    #[derive(Facet)]
    struct Message {
        text: String,
    }

    let msg = Message {
        text: "hello world".into(),
    };
    assert_idempotent(&msg, "struct with quoted string");
}

#[test]
fn idempotent_nested_struct() {
    #[derive(Facet)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[derive(Facet)]
    struct Config {
        name: String,
        origin: Point,
    }

    let config = Config {
        name: "test".into(),
        origin: Point { x: 10, y: 20 },
    };
    assert_idempotent(&config, "nested struct");
}

#[test]
fn idempotent_deeply_nested_struct() {
    #[derive(Facet)]
    struct Inner {
        value: i32,
    }

    #[derive(Facet)]
    struct Middle {
        inner: Inner,
    }

    #[derive(Facet)]
    struct Outer {
        middle: Middle,
    }

    let outer = Outer {
        middle: Middle {
            inner: Inner { value: 42 },
        },
    };
    assert_idempotent(&outer, "deeply nested struct");
}

// =============================================================================
// Optional fields
// =============================================================================

#[test]
fn idempotent_optional_none() {
    #[derive(Facet)]
    struct Config {
        required: String,
        optional: Option<i32>,
    }

    let config = Config {
        required: "hello".into(),
        optional: None,
    };
    assert_idempotent(&config, "optional None");
}

#[test]
fn idempotent_optional_some() {
    #[derive(Facet)]
    struct Config {
        required: String,
        optional: Option<i32>,
    }

    let config = Config {
        required: "hello".into(),
        optional: Some(42),
    };
    assert_idempotent(&config, "optional Some");
}

#[test]
fn idempotent_nested_optional() {
    #[derive(Facet)]
    struct Inner {
        value: i32,
    }

    #[derive(Facet)]
    struct Config {
        inner: Option<Inner>,
    }

    let config = Config {
        inner: Some(Inner { value: 42 }),
    };
    assert_idempotent(&config, "nested optional");
}

// =============================================================================
// Sequences (Vec)
// =============================================================================

#[test]
fn idempotent_vec_of_ints() {
    #[derive(Facet)]
    struct Data {
        items: Vec<i32>,
    }

    let data = Data {
        items: vec![1, 2, 3, 4, 5],
    };
    assert_idempotent(&data, "vec of ints");
}

#[test]
fn idempotent_vec_of_strings() {
    #[derive(Facet)]
    struct Data {
        names: Vec<String>,
    }

    let data = Data {
        names: vec!["alice".into(), "bob".into(), "charlie".into()],
    };
    assert_idempotent(&data, "vec of strings");
}

#[test]
fn idempotent_vec_of_structs() {
    #[derive(Facet)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[derive(Facet)]
    struct Data {
        points: Vec<Point>,
    }

    let data = Data {
        points: vec![Point { x: 1, y: 2 }, Point { x: 3, y: 4 }],
    };
    assert_idempotent(&data, "vec of structs");
}

#[test]
fn idempotent_empty_vec() {
    #[derive(Facet)]
    struct Data {
        items: Vec<i32>,
    }

    let data = Data { items: vec![] };
    assert_idempotent(&data, "empty vec");
}

// =============================================================================
// HashMaps
// =============================================================================

#[test]
fn idempotent_hashmap_string_int() {
    let mut map: HashMap<String, i32> = HashMap::new();
    map.insert("one".into(), 1);
    map.insert("two".into(), 2);
    assert_idempotent(&map, "hashmap string->int");
}

#[test]
fn idempotent_hashmap_string_struct() {
    #[derive(Facet)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut map: HashMap<String, Point> = HashMap::new();
    map.insert("origin".into(), Point { x: 0, y: 0 });
    map.insert("corner".into(), Point { x: 10, y: 10 });
    assert_idempotent(&map, "hashmap string->struct");
}

// =============================================================================
// Enums
// =============================================================================

#[test]
fn idempotent_unit_enum_variant() {
    #[derive(Facet)]
    #[repr(C)]
    enum Status {
        Active,
        Inactive,
    }

    #[derive(Facet)]
    struct Config {
        status: Status,
    }

    let config = Config {
        status: Status::Active,
    };
    assert_idempotent(&config, "unit enum variant");
}

#[test]
fn idempotent_newtype_enum_variant() {
    #[derive(Facet)]
    #[repr(C)]
    enum Value {
        Int(i32),
        String(String),
    }

    #[derive(Facet)]
    struct Config {
        value: Value,
    }

    let config = Config {
        value: Value::Int(42),
    };
    assert_idempotent(&config, "newtype enum variant (int)");

    let config2 = Config {
        value: Value::String("hello".into()),
    };
    assert_idempotent(&config2, "newtype enum variant (string)");
}

#[test]
fn idempotent_struct_enum_variant() {
    #[derive(Facet)]
    #[repr(C)]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    #[derive(Facet)]
    struct Config {
        shape: Shape,
    }

    let config = Config {
        shape: Shape::Circle { radius: 5.0 },
    };
    assert_idempotent(&config, "struct enum variant (circle)");

    let config2 = Config {
        shape: Shape::Rectangle {
            width: 10.0,
            height: 20.0,
        },
    };
    assert_idempotent(&config2, "struct enum variant (rectangle)");
}

// =============================================================================
// Documented fields (doc comments)
// =============================================================================

#[test]
fn idempotent_documented_fields() {
    #[derive(Facet)]
    struct Config {
        name: Documented<String>,
        port: Documented<u16>,
    }

    let config = Config {
        name: Documented::with_doc_line("myapp".into(), "The application name"),
        port: Documented::with_doc_line(8080, "Port to listen on"),
    };
    assert_idempotent(&config, "documented fields");
}

#[test]
fn idempotent_documented_nested_struct() {
    #[derive(Facet)]
    struct Server {
        host: Documented<String>,
        port: Documented<u16>,
    }

    #[derive(Facet)]
    struct Config {
        server: Documented<Server>,
    }

    let config = Config {
        server: Documented::with_doc_line(
            Server {
                host: Documented::with_doc_line("localhost".into(), "The hostname"),
                port: Documented::with_doc_line(8080, "The port"),
            },
            "Server configuration",
        ),
    };
    assert_idempotent(&config, "documented nested struct");
}

#[test]
fn idempotent_hashmap_documented_keys() {
    let mut map: HashMap<Documented<String>, i32> = HashMap::new();
    map.insert(
        Documented::with_doc_line("port".into(), "The port to listen on"),
        8080,
    );
    map.insert(
        Documented::with_doc_line("timeout".into(), "Timeout in seconds"),
        30,
    );
    assert_idempotent(&map, "hashmap with documented keys");
}

// =============================================================================
// Complex combinations
// =============================================================================

#[test]
fn idempotent_complex_nested() {
    #[derive(Facet)]
    struct Inner {
        value: i32,
    }

    #[derive(Facet)]
    struct Middle {
        items: Vec<Inner>,
        optional: Option<String>,
    }

    #[derive(Facet)]
    struct Outer {
        name: String,
        middles: Vec<Middle>,
    }

    let outer = Outer {
        name: "test".into(),
        middles: vec![
            Middle {
                items: vec![Inner { value: 1 }, Inner { value: 2 }],
                optional: Some("hello".into()),
            },
            Middle {
                items: vec![],
                optional: None,
            },
        ],
    };
    assert_idempotent(&outer, "complex nested structure");
}

#[test]
fn idempotent_enum_with_optional_struct() {
    #[derive(Facet)]
    struct Details {
        x: i32,
        y: Option<i32>,
    }

    #[derive(Facet)]
    #[repr(C)]
    enum Value {
        Simple,
        WithDetails(Details),
    }

    #[derive(Facet)]
    struct Config {
        value: Value,
    }

    let config = Config {
        value: Value::WithDetails(Details { x: 10, y: Some(20) }),
    };
    assert_idempotent(&config, "enum with optional struct");
}

// Note: Compact mode tests are not included here because compact/inline mode
// is specifically for embedding values (not standalone documents). The CST
// formatter always adds trailing newlines which is not appropriate for embedded values.

// =============================================================================
// Schema-like structures (similar to what facet-styx generates)
// =============================================================================

#[test]
fn idempotent_schema_object() {
    use crate::schema_types::{ObjectSchema, Schema};

    let mut fields = HashMap::new();
    fields.insert(
        Documented::new(Some("name".to_string())),
        Schema::String(None),
    );
    fields.insert(Documented::new(Some("port".to_string())), Schema::Int(None));

    let schema = Schema::Object(ObjectSchema(fields));
    assert_idempotent(&schema, "schema object");
}

#[test]
fn idempotent_schema_with_doc_comments() {
    use crate::schema_types::{ObjectSchema, Schema};

    let mut fields = HashMap::new();
    fields.insert(
        Documented::with_doc_line(Some("name".to_string()), "The name field"),
        Schema::String(None),
    );
    fields.insert(
        Documented::with_doc_line(Some("port".to_string()), "The port number"),
        Schema::Int(None),
    );

    let schema = Schema::Object(ObjectSchema(fields));
    assert_idempotent(&schema, "schema with doc comments");
}

#[test]
fn idempotent_schema_optional_field() {
    use crate::schema_types::{ObjectSchema, OptionalSchema, Schema};

    let mut fields = HashMap::new();
    fields.insert(
        Documented::with_doc_line(Some("binary".to_string()), "Path to a pre-built binary"),
        Schema::Optional(OptionalSchema((Documented::new(Box::new(Schema::String(
            None,
        ))),))),
    );

    let schema = Schema::Object(ObjectSchema(fields));
    assert_idempotent(&schema, "schema optional field");
}

#[test]
fn idempotent_schema_enum() {
    use crate::schema_types::{EnumSchema, Schema};

    let mut variants = HashMap::new();
    variants.insert(Documented::new("active".to_string()), Schema::Unit);
    variants.insert(Documented::new("inactive".to_string()), Schema::Unit);

    let schema = Schema::Enum(EnumSchema(variants));
    assert_idempotent(&schema, "schema enum");
}

#[test]
fn idempotent_schema_enum_with_doc_comments() {
    use crate::schema_types::{EnumSchema, ObjectSchema, Schema};

    let mut variants = HashMap::new();
    variants.insert(
        Documented::with_doc_line("simple".to_string(), "A simple variant"),
        Schema::Unit,
    );

    let mut complex_fields = HashMap::new();
    complex_fields.insert(Documented::new(Some("x".to_string())), Schema::Int(None));
    complex_fields.insert(Documented::new(Some("y".to_string())), Schema::Int(None));
    variants.insert(
        Documented::with_doc_line("complex".to_string(), "A complex variant"),
        Schema::Object(ObjectSchema(complex_fields)),
    );

    let schema = Schema::Enum(EnumSchema(variants));
    assert_idempotent(&schema, "schema enum with doc comments");
}

#[test]
fn idempotent_schema_map() {
    use crate::schema_types::{MapSchema, Schema};

    let schema = Schema::Map(MapSchema(vec![
        Documented::new(Schema::String(None)),
        Documented::new(Schema::Int(None)),
    ]));
    assert_idempotent(&schema, "schema map");
}

#[test]
fn idempotent_schema_seq() {
    use crate::schema_types::{Schema, SeqSchema};

    let schema = Schema::Seq(SeqSchema((Documented::new(Box::new(Schema::String(None))),)));
    assert_idempotent(&schema, "schema seq");
}

#[test]
fn idempotent_schema_nested_optionals() {
    use crate::schema_types::{OptionalSchema, Schema};

    let schema = Schema::Optional(OptionalSchema((Documented::new(Box::new(
        Schema::Optional(OptionalSchema((Documented::new(Box::new(Schema::String(
            None,
        ))),))),
    )),)));
    assert_idempotent(&schema, "schema nested optionals");
}

#[test]
fn idempotent_schema_complex_map_enum() {
    use crate::schema_types::{EnumSchema, MapSchema, Schema};

    // This mimics the dibs schema pattern: @map(@string @enum{...})
    let mut enum_variants = HashMap::new();
    enum_variants.insert(
        Documented::with_doc_line("uuid".to_string(), "UUID type"),
        Schema::Unit,
    );
    enum_variants.insert(
        Documented::with_doc_line("string".to_string(), "String type"),
        Schema::Unit,
    );
    enum_variants.insert(
        Documented::with_doc_line("int".to_string(), "Integer type"),
        Schema::Unit,
    );

    let schema = Schema::Map(MapSchema(vec![
        Documented::new(Schema::String(None)),
        Documented::new(Schema::Enum(EnumSchema(enum_variants))),
    ]));
    assert_idempotent(&schema, "schema map with enum");
}

#[test]
fn idempotent_schema_deeply_nested() {
    use crate::schema_types::{EnumSchema, MapSchema, ObjectSchema, OptionalSchema, Schema};

    // Deeply nested: @object{field @optional(@map(@string @enum{...}))}
    let mut enum_variants = HashMap::new();
    enum_variants.insert(Documented::new("a".to_string()), Schema::Unit);
    enum_variants.insert(Documented::new("b".to_string()), Schema::Unit);

    let map_schema = Schema::Map(MapSchema(vec![
        Documented::new(Schema::String(None)),
        Documented::new(Schema::Enum(EnumSchema(enum_variants))),
    ]));

    let optional_schema =
        Schema::Optional(OptionalSchema((Documented::new(Box::new(map_schema)),)));

    let mut fields = HashMap::new();
    fields.insert(
        Documented::with_doc_line(Some("field".to_string()), "A complex field"),
        optional_schema,
    );

    let schema = Schema::Object(ObjectSchema(fields));
    assert_idempotent(&schema, "schema deeply nested");
}

#[test]
fn idempotent_dibs_like_schema() {
    use crate::schema_types::{Meta, ObjectSchema, OptionalSchema, Schema, SchemaFile};

    // This test mimics the actual dibs schema structure:
    // meta {id "crate:dibs@1", cli dibs, description "..."}
    // schema {@ @object{
    //     /// Database crate configuration.
    //     db @object{
    //         /// Path to a pre-built binary...
    //         binary @optional(@string)
    //         /// Name of the crate...
    //         crate @optional(@string)
    //     }
    // }}

    // Build the inner 'db' object schema
    let mut db_fields = HashMap::new();
    db_fields.insert(
        Documented::with_doc(
            Some("binary".to_string()),
            vec![
                "Path to a pre-built binary (for faster iteration).".to_string(),
                "If not specified, we'll use `cargo run -p <crate_name>`.".to_string(),
            ],
        ),
        Schema::Optional(OptionalSchema((Documented::new(Box::new(Schema::String(
            None,
        ))),))),
    );
    db_fields.insert(
        Documented::with_doc_line(
            Some("crate".to_string()),
            "Name of the crate containing schema definitions (e.g., \"my-app-db\").",
        ),
        Schema::Optional(OptionalSchema((Documented::new(Box::new(Schema::String(
            None,
        ))),))),
    );

    // Build the root object schema with 'db' field
    let mut root_fields = HashMap::new();
    root_fields.insert(
        Documented::with_doc_line(Some("db".to_string()), "Database crate configuration."),
        Schema::Object(ObjectSchema(db_fields)),
    );

    // Build the schema map with @ key pointing to the root object
    let mut schema_map = HashMap::new();
    schema_map.insert(None, Schema::Object(ObjectSchema(root_fields)));

    // Build the complete SchemaFile
    let schema_file = SchemaFile {
        meta: Meta {
            id: "crate:dibs@1".to_string(),
            version: None,
            cli: Some("dibs".to_string()),
            description: Some("Configuration loaded from `dibs.styx`.".to_string()),
        },
        imports: None,
        schema: schema_map,
    };

    assert_idempotent(&schema_file, "dibs-like schema file");
}
