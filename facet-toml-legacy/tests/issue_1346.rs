//! Test for issue #1346: Untagged enums fail to parse in facet-toml
//!
//! This test reproduces the bug where facet-toml fails to deserialize
//! enums marked with `#[facet(untagged)]`, returning the error:
//! "Operation failed on shape Dependency: No variant found with the given name"
//!
//! The use case is parsing Cargo.toml-style dependency declarations where
//! a field can be either a simple version string or a table with options.

use facet::Facet;

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
struct Test1 {
    dep: Dependency,
}

/// Test parsing a simple version string into an untagged enum
#[test]
fn test_untagged_enum_string_variant() {
    let toml = r#"dep = "1.0""#;

    let result = facet_toml_legacy::from_str::<Test1>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse string value into untagged enum Version variant"
    );

    let parsed = result.unwrap();
    assert_eq!(parsed.dep, Dependency::Version("1.0".to_string()));
}

/// Test parsing a table into an untagged enum
#[test]
fn test_untagged_enum_table_variant() {
    let toml = r#"
[dep]
path = "../util"
"#;

    let result = facet_toml_legacy::from_str::<Test1>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse table value into untagged enum Table variant"
    );

    let parsed = result.unwrap();
    assert_eq!(
        parsed.dep,
        Dependency::Table(DepTable {
            path: Some("../util".to_string()),
            version: None,
        })
    );
}

/// Test parsing a table with version field
#[test]
fn test_untagged_enum_table_with_version() {
    let toml = r#"
[dep]
version = "1.0"
path = "../util"
"#;

    let result = facet_toml_legacy::from_str::<Test1>(toml);
    if let Err(e) = &result {
        eprintln!("Error: {e}");
    }

    assert!(
        result.is_ok(),
        "Should parse table with both fields into untagged enum Table variant"
    );

    let parsed = result.unwrap();
    assert_eq!(
        parsed.dep,
        Dependency::Table(DepTable {
            path: Some("../util".to_string()),
            version: Some("1.0".to_string()),
        })
    );
}
