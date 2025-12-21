//! Test for issue #1353: Untagged enum in HashMap fails with table header syntax
//!
//! This test reproduces the bug where facet-toml fails to deserialize untagged
//! enums within HashMaps when using TOML table header syntax like `[dependencies.backtrace]`.
//!
//! The error is: "must select variant before selecting enum fields"
//!
//! The issue is that table headers require determining the enum variant before
//! seeing the fields, but untagged enums need to see fields to select the variant.

use facet::Facet;
use std::collections::HashMap;

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
#[facet(untagged)]
enum Dependency {
    Version(String),
    Table(DepTable),
}

#[derive(Facet, Debug, Default, PartialEq)]
struct DepTable {
    path: Option<String>,
    version: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct Manifest {
    dependencies: HashMap<String, Dependency>,
}

/// Test parsing untagged enum in HashMap with inline table syntax (this works)
#[test]
fn test_untagged_enum_hashmap_inline_syntax() {
    let toml = r#"
[dependencies]
backtrace = { path = "../.." }
"#;

    let result = facet_toml::from_str::<Manifest>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse inline table syntax into untagged enum in HashMap"
    );

    let parsed = result.unwrap();
    assert_eq!(
        parsed.dependencies.get("backtrace"),
        Some(&Dependency::Table(DepTable {
            path: Some("../..".to_string()),
            version: None,
        }))
    );
}

/// Test parsing untagged enum in HashMap with table header syntax
/// This tests when [dependencies.backtrace] is the FIRST table header
/// (no prior [dependencies] initialization).
#[test]
fn test_untagged_enum_hashmap_table_header_syntax() {
    let toml = r#"
[dependencies.backtrace]
path = "../.."
"#;

    let result = facet_toml::from_str::<Manifest>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse table header syntax into untagged enum in HashMap"
    );

    let parsed = result.unwrap();
    assert_eq!(
        parsed.dependencies.get("backtrace"),
        Some(&Dependency::Table(DepTable {
            path: Some("../..".to_string()),
            version: None,
        }))
    );
}

/// Test multiple dependencies with mixed table header and inline syntax
#[test]
fn test_untagged_enum_hashmap_mixed_syntax() {
    let toml = r#"
[dependencies]
simple = "1.0"

[dependencies.backtrace]
path = "../.."
version = "0.3"
"#;

    let result = facet_toml::from_str::<Manifest>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse mixed inline and table header syntax"
    );

    let parsed = result.unwrap();
    assert_eq!(
        parsed.dependencies.get("simple"),
        Some(&Dependency::Version("1.0".to_string()))
    );
    assert_eq!(
        parsed.dependencies.get("backtrace"),
        Some(&Dependency::Table(DepTable {
            path: Some("../..".to_string()),
            version: Some("0.3".to_string()),
        }))
    );
}
