//! Edge case tests ported from toml-rs serde tests.
//! See: https://github.com/toml-rs/toml/blob/main/crates/toml/tests/serde/general.rs

use facet::Facet;
use facet_testhelpers::test;

// =============================================================================
// Integer edge cases
// =============================================================================

#[test]
fn test_i64_min() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: i64,
    }

    let result = facet_toml_legacy::from_str::<Foo>(&format!("value = {}", i64::MIN)).unwrap();
    assert_eq!(result, Foo { value: i64::MIN });
}

#[test]
fn test_i64_max() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: i64,
    }

    let result = facet_toml_legacy::from_str::<Foo>(&format!("value = {}", i64::MAX)).unwrap();
    assert_eq!(result, Foo { value: i64::MAX });
}

#[test]
fn test_i64_roundtrip_min() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: i64,
    }

    let original = Foo { value: i64::MIN };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: Foo = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_i64_roundtrip_max() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: i64,
    }

    let original = Foo { value: i64::MAX };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: Foo = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_u64_large_values() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: u64,
    }

    // Values up to i64::MAX should work
    let result = facet_toml_legacy::from_str::<Foo>(&format!("value = {}", i64::MAX as u64)).unwrap();
    assert_eq!(
        result,
        Foo {
            value: i64::MAX as u64
        }
    );
}

#[test]
fn test_u64_max_from_string() {
    // u64::MAX exceeds TOML's native integer range (i64), but can be parsed from string
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: u64,
    }

    // TOML integers are i64, so u64::MAX cannot be represented directly
    // This tests that we handle the edge case appropriately
    let input = format!("value = {}", u64::MAX);
    let result = facet_toml_legacy::from_str::<Foo>(&input);
    // This may fail because u64::MAX > i64::MAX - that's expected behavior
    // The important thing is we don't panic
    let _ = result;
}

// =============================================================================
// Float edge cases
// =============================================================================

#[test]
fn test_f64_min() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: f64,
    }

    let original = Foo { value: f64::MIN };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: Foo = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_f64_max() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: f64,
    }

    let original = Foo { value: f64::MAX };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: Foo = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_f64_infinity() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: f64,
    }

    // TOML supports inf
    let result = facet_toml_legacy::from_str::<Foo>("value = inf").unwrap();
    assert!(result.value.is_infinite() && result.value.is_sign_positive());

    let result = facet_toml_legacy::from_str::<Foo>("value = +inf").unwrap();
    assert!(result.value.is_infinite() && result.value.is_sign_positive());

    let result = facet_toml_legacy::from_str::<Foo>("value = -inf").unwrap();
    assert!(result.value.is_infinite() && result.value.is_sign_negative());
}

#[test]
fn test_f64_nan() {
    #[derive(Debug, Facet)]
    struct Foo {
        value: f64,
    }

    // TOML supports nan
    let result = facet_toml_legacy::from_str::<Foo>("value = nan").unwrap();
    assert!(result.value.is_nan());

    let result = facet_toml_legacy::from_str::<Foo>("value = +nan").unwrap();
    assert!(result.value.is_nan());

    let result = facet_toml_legacy::from_str::<Foo>("value = -nan").unwrap();
    assert!(result.value.is_nan());
}

#[test]
fn test_f32_infinity() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        value: f32,
    }

    let result = facet_toml_legacy::from_str::<Foo>("value = inf").unwrap();
    assert!(result.value.is_infinite() && result.value.is_sign_positive());

    let result = facet_toml_legacy::from_str::<Foo>("value = -inf").unwrap();
    assert!(result.value.is_infinite() && result.value.is_sign_negative());
}

#[test]
fn test_f32_nan() {
    #[derive(Debug, Facet)]
    struct Foo {
        value: f32,
    }

    let result = facet_toml_legacy::from_str::<Foo>("value = nan").unwrap();
    assert!(result.value.is_nan());
}

// =============================================================================
// Tuple types
// =============================================================================

#[test]
fn test_homogeneous_tuple() {
    #[derive(Debug, Facet, PartialEq)]
    struct Collection {
        elems: (i64, i64, i64),
    }

    let result = facet_toml_legacy::from_str::<Collection>("elems = [0, 1, 2]").unwrap();
    assert_eq!(result, Collection { elems: (0, 1, 2) });
}

#[test]
fn test_heterogeneous_tuple() {
    #[derive(Debug, Facet, PartialEq)]
    struct Mixed {
        data: (i32, bool, String),
    }

    let result = facet_toml_legacy::from_str::<Mixed>(r#"data = [42, true, "hello"]"#).unwrap();
    assert_eq!(
        result,
        Mixed {
            data: (42, true, "hello".to_string())
        }
    );
}

#[test]
fn test_tuple_roundtrip() {
    #[derive(Debug, Facet, PartialEq)]
    struct Collection {
        elems: (i64, i64, i64),
    }

    let original = Collection {
        elems: (10, 20, 30),
    };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: Collection = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

// =============================================================================
// Fixed-size arrays
// =============================================================================

#[test]
fn test_fixed_size_array() {
    #[derive(Debug, Facet, PartialEq)]
    struct Entity {
        pos: [i32; 2],
    }

    let result = facet_toml_legacy::from_str::<Entity>("pos = [1, 2]").unwrap();
    assert_eq!(result, Entity { pos: [1, 2] });
}

#[test]
fn test_fixed_size_array_roundtrip() {
    #[derive(Debug, Facet, PartialEq)]
    struct Entity {
        pos: [i32; 3],
    }

    let original = Entity { pos: [10, 20, 30] };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: Entity = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_fixed_size_array_of_strings() {
    #[derive(Debug, Facet, PartialEq)]
    struct Names {
        values: [String; 2],
    }

    let result = facet_toml_legacy::from_str::<Names>(r#"values = ["alice", "bob"]"#).unwrap();
    assert_eq!(
        result,
        Names {
            values: ["alice".to_string(), "bob".to_string()]
        }
    );
}

// =============================================================================
// Newtype wrappers
// =============================================================================

#[test]
fn test_newtype_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct A {
        b: B,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct B(u32);

    let result = facet_toml_legacy::from_str::<A>("b = 42").unwrap();
    assert_eq!(result, A { b: B(42) });
}

#[test]
fn test_newtype_roundtrip() {
    #[derive(Debug, Facet, PartialEq)]
    struct A {
        b: B,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct B(u32);

    let original = A { b: B(123) };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: A = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

// =============================================================================
// Table arrays (array of tables)
// =============================================================================

#[test]
fn test_table_array() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        items: Vec<Bar>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Bar {
        value: i32,
    }

    let input = r#"
[[items]]
value = 1

[[items]]
value = 2

[[items]]
value = 3
"#;

    let result = facet_toml_legacy::from_str::<Foo>(input).unwrap();
    assert_eq!(
        result,
        Foo {
            items: vec![Bar { value: 1 }, Bar { value: 2 }, Bar { value: 3 }]
        }
    );
}

#[test]
fn test_table_array_roundtrip() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        items: Vec<Bar>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Bar {
        value: i32,
    }

    let original = Foo {
        items: vec![Bar { value: 10 }, Bar { value: 20 }],
    };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: Foo = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

// =============================================================================
// Empty arrays
// =============================================================================

#[test]
fn test_empty_array() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        items: Vec<i32>,
    }

    let result = facet_toml_legacy::from_str::<Foo>("items = []").unwrap();
    assert_eq!(result, Foo { items: vec![] });
}

#[test]
fn test_empty_array_roundtrip() {
    #[derive(Debug, Facet, PartialEq)]
    struct Foo {
        items: Vec<i32>,
    }

    let original = Foo { items: vec![] };
    let serialized = facet_toml_legacy::to_string(&original).unwrap();
    let deserialized: Foo = facet_toml_legacy::from_str(&serialized).unwrap();
    assert_eq!(original, deserialized);
}

// =============================================================================
// Dotted keys
// =============================================================================

#[test]
fn test_dotted_keys() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        database: Database,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Database {
        host: String,
        port: u16,
    }

    let input = r#"
database.host = "localhost"
database.port = 5432
"#;

    let result = facet_toml_legacy::from_str::<Config>(input).unwrap();
    assert_eq!(
        result,
        Config {
            database: Database {
                host: "localhost".to_string(),
                port: 5432,
            }
        }
    );
}

// =============================================================================
// Inline tables
// =============================================================================

#[test]
fn test_inline_table() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        point: Point,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    let result = facet_toml_legacy::from_str::<Config>("point = { x = 1, y = 2 }").unwrap();
    assert_eq!(
        result,
        Config {
            point: Point { x: 1, y: 2 }
        }
    );
}

// =============================================================================
// Mixed table styles
// =============================================================================

#[test]
fn test_mixed_table_styles() {
    #[derive(Debug, Facet, PartialEq)]
    struct Manifest {
        package: Package,
        dependencies: Dependencies,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Package {
        name: String,
        version: String,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Dependencies {
        #[facet(default)]
        serde: Option<String>,
    }

    let input = r#"
[package]
name = "my-crate"
version = "0.1.0"

[dependencies]
serde = "1.0"
"#;

    let result = facet_toml_legacy::from_str::<Manifest>(input).unwrap();
    assert_eq!(
        result,
        Manifest {
            package: Package {
                name: "my-crate".to_string(),
                version: "0.1.0".to_string(),
            },
            dependencies: Dependencies {
                serde: Some("1.0".to_string()),
            },
        }
    );
}
