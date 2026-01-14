//! Probing solver tests.

use facet::Facet;
use facet_solver::Schema;
use facet_testhelpers::test;

#[derive(Facet, Debug)]
struct SimpleStruct {
    name: String,
    value: i32,
}

#[test]
fn test_known_paths_simple_struct() {
    let schema = Schema::build(SimpleStruct::SHAPE).unwrap();
    let config = &schema.resolutions()[0];

    // Simple struct should have top-level paths
    assert!(config.has_key_path(&["name"]));
    assert!(config.has_key_path(&["value"]));
    assert!(!config.has_key_path(&["nonexistent"]));
}

#[test]
fn test_known_paths_nested_struct() {
    // Struct with non-flattened nested struct
    #[derive(Facet, Debug)]
    struct Inner {
        x: i32,
        y: i32,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        name: String,
        pos: Inner,
    }

    let schema = Schema::build(Outer::SHAPE).unwrap();
    let config = &schema.resolutions()[0];

    // Top-level paths
    assert!(config.has_key_path(&["name"]));
    assert!(config.has_key_path(&["pos"]));

    // Nested paths (for probing)
    assert!(config.has_key_path(&["pos", "x"]));
    assert!(config.has_key_path(&["pos", "y"]));

    // Nonexistent
    assert!(!config.has_key_path(&["pos", "z"]));
}

#[test]
fn test_probe_deeply_nested() {
    // Deep nesting: struct -> struct -> struct
    #[derive(Facet, Debug)]
    struct Level3 {
        deep: String,
    }

    #[derive(Facet, Debug)]
    struct Level2 {
        mid: Level3,
    }

    #[derive(Facet, Debug)]
    struct Level1 {
        top: Level2,
    }

    let schema = Schema::build(Level1::SHAPE).unwrap();
    let config = &schema.resolutions()[0];

    // All paths should be tracked
    assert!(config.has_key_path(&["top"]));
    assert!(config.has_key_path(&["top", "mid"]));
    assert!(config.has_key_path(&["top", "mid", "deep"]));
}
