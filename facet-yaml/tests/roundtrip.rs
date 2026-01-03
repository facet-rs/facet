//! Roundtrip tests for YAML fixtures.
//!
//! These tests verify that YAML files can be deserialized into typed structs
//! and then serialized back to produce semantically equivalent YAML.

use facet::Facet;
use std::path::Path;

// ============================================================================
// Test structs for fixtures
// ============================================================================

/// Struct for nested_struct.yaml
#[derive(Debug, Facet, PartialEq)]
struct NestedStruct {
    id: u64,
    child: NestedChild,
    tags: Vec<String>,
}

#[derive(Debug, Facet, PartialEq)]
struct NestedChild {
    code: String,
    active: bool,
}

/// Struct for deeply_nested.yaml
#[derive(Debug, Facet, PartialEq)]
struct DeeplyNested {
    config: Config,
    name: String,
}

#[derive(Debug, Facet, PartialEq)]
struct Config {
    database: DatabaseConfig,
    cache: CacheConfig,
}

#[derive(Debug, Facet, PartialEq)]
struct DatabaseConfig {
    host: String,
    port: u16,
}

#[derive(Debug, Facet, PartialEq)]
struct CacheConfig {
    enabled: bool,
    ttl: u64,
}

/// Struct for nested_sequences.yaml
#[derive(Debug, Facet, PartialEq)]
struct NestedSequences {
    matrix: Vec<Vec<i64>>,
}

/// Struct for mixed_nesting.yaml
#[derive(Debug, Facet, PartialEq)]
struct MixedNesting {
    items: Vec<Item>,
    count: u64,
}

#[derive(Debug, Facet, PartialEq)]
struct Item {
    name: String,
    values: Vec<i64>,
}

/// Struct for struct_with_vec.yaml
#[derive(Debug, Facet, PartialEq)]
struct StructWithVec {
    id: u64,
    tags: Vec<String>,
    active: bool,
}

/// Struct for simple_struct.yaml
#[derive(Debug, Facet, PartialEq)]
struct SimpleStruct {
    name: String,
    version: u64,
}

// ============================================================================
// Roundtrip test function
// ============================================================================

fn yaml_roundtrip_test(path: &Path) -> datatest_stable::Result<()> {
    let yaml_str = std::fs::read_to_string(path)?;
    let filename = path.file_stem().unwrap().to_str().unwrap();

    // Dispatch based on filename to use appropriate type
    match filename {
        "nested_struct" => roundtrip::<NestedStruct>(&yaml_str, path)?,
        "deeply_nested" => roundtrip::<DeeplyNested>(&yaml_str, path)?,
        "nested_sequences" => roundtrip::<NestedSequences>(&yaml_str, path)?,
        "mixed_nesting" => roundtrip::<MixedNesting>(&yaml_str, path)?,
        "struct_with_vec" => roundtrip::<StructWithVec>(&yaml_str, path)?,
        "simple_struct" => roundtrip::<SimpleStruct>(&yaml_str, path)?,
        _ => {
            return Err(format!("Unknown fixture: {}", filename).into());
        }
    }

    Ok(())
}

fn roundtrip<T>(yaml_str: &str, path: &Path) -> datatest_stable::Result<()>
where
    T: for<'a> Facet<'a> + std::fmt::Debug + PartialEq,
{
    // First parse
    let value1: T = facet_yaml::from_str(yaml_str)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

    // Serialize back to YAML
    let serialized1 = facet_yaml::to_string(&value1)
        .map_err(|e| format!("Failed to serialize {}: {}", path.display(), e))?;

    println!("\n=== {} ===", path.display());
    println!("Original:\n{}", yaml_str);
    println!("Serialized:\n{}", serialized1);

    // Parse the serialized output
    let value2: T = facet_yaml::from_str(&serialized1)
        .map_err(|e| format!("Failed to re-parse serialized YAML: {}", e))?;

    // Values should be equal
    assert_eq!(
        value1,
        value2,
        "Values differ after roundtrip for {}",
        path.display()
    );

    // Serialize again
    let serialized2 =
        facet_yaml::to_string(&value2).map_err(|e| format!("Failed to serialize again: {}", e))?;

    // The two serializations should be identical
    assert_eq!(
        serialized1,
        serialized2,
        "Serialized YAML should be identical after second roundtrip for {}",
        path.display()
    );

    println!("=== PASSED ===\n");

    Ok(())
}

datatest_stable::harness! {
    { test = yaml_roundtrip_test, root = "tests/fixtures", pattern = r".*\.yaml$" },
}
