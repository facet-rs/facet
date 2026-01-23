//! Tests for Spanned<T> deserialization in TOML.
//!
//! Spanned<T> wraps a value with source span information for diagnostics.

use facet::Facet;
use facet_reflect::Spanned;
use facet_toml::{self as toml, DeserializeError, TomlError};

// ============================================================================
// Basic Spanned types
// ============================================================================

#[test]
fn spanned_string() {
    #[derive(Facet, Debug)]
    struct Config {
        name: Spanned<String>,
    }

    let config: Config = toml::from_str(r#"name = "foo""#).unwrap();
    assert_eq!(config.name.value, "foo");
    assert!(config.name.span.is_unknown());
}

#[test]
fn spanned_vec() {
    #[derive(Facet, Debug)]
    struct Config {
        features: Spanned<Vec<String>>,
    }

    let config: Config = toml::from_str(r#"features = ["a", "b", "c"]"#).unwrap();
    assert_eq!(config.features.value, vec!["a", "b", "c"]);
    assert!(config.features.span.is_unknown());
}

#[test]
fn spanned_bool() {
    #[derive(Facet, Debug)]
    struct Config {
        enabled: Spanned<bool>,
    }

    let config: Config = toml::from_str(r#"enabled = true"#).unwrap();
    assert!(config.enabled.value);
    assert!(config.enabled.span.is_unknown());
}

#[test]
fn spanned_integer() {
    #[derive(Facet, Debug)]
    struct Config {
        version: Spanned<u32>,
    }

    let config: Config = toml::from_str(r#"version = 42"#).unwrap();
    assert_eq!(config.version.value, 42);
    assert!(config.version.span.is_unknown());
}

#[test]
fn multiple_spanned_fields() {
    #[derive(Facet, Debug)]
    struct Config {
        name: Spanned<String>,
        features: Spanned<Vec<String>>,
        enabled: Spanned<bool>,
    }

    let toml = r#"
name = "foo"
features = ["a", "b", "c"]
enabled = true
"#;

    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.name.value, "foo");
    assert_eq!(config.features.value, vec!["a", "b", "c"]);
    assert!(config.enabled.value);
}

// ============================================================================
// Spanned in nested structures
// ============================================================================

#[test]
fn nested_spanned_in_table() {
    #[derive(Facet, Debug)]
    struct Config {
        dependency: Dependency,
    }

    #[derive(Facet, Debug)]
    struct Dependency {
        git: Option<Spanned<String>>,
        features: Option<Spanned<Vec<String>>>,
        default_features: Option<Spanned<bool>>,
    }

    let toml = r#"
[dependency]
git = "https://github.com/user/repo"
features = ["a", "b"]
default_features = false
"#;

    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(
        config.dependency.git.as_ref().unwrap().value,
        "https://github.com/user/repo"
    );
    assert_eq!(
        config.dependency.features.as_ref().unwrap().value,
        vec!["a", "b"]
    );
    assert!(!config.dependency.default_features.as_ref().unwrap().value);
}

#[test]
fn spanned_in_array_of_tables() {
    #[derive(Facet, Debug)]
    struct Config {
        dependencies: Vec<Dependency>,
    }

    #[derive(Facet, Debug)]
    struct Dependency {
        name: Spanned<String>,
        version: Spanned<String>,
    }

    let toml = r#"
[[dependencies]]
name = "foo"
version = "1.0"

[[dependencies]]
name = "bar"
version = "2.0"
"#;

    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.dependencies.len(), 2);
    assert_eq!(config.dependencies[0].name.value, "foo");
    assert_eq!(config.dependencies[0].version.value, "1.0");
    assert_eq!(config.dependencies[1].name.value, "bar");
    assert_eq!(config.dependencies[1].version.value, "2.0");
}

// ============================================================================
// Spanned in untagged enums
// ============================================================================

#[derive(Facet, Debug)]
#[repr(u8)]
#[facet(untagged)]
pub enum DebugLevel {
    Bool(Spanned<bool>),
    Number(Spanned<u8>),
    String(Spanned<String>),
}

#[test]
fn spanned_untagged_enum_bool() {
    #[derive(Facet, Debug)]
    struct Config {
        value: DebugLevel,
    }

    let config: Config = toml::from_str(r#"value = true"#).unwrap();
    match config.value {
        DebugLevel::Bool(spanned_bool) => assert!(*spanned_bool),
        _ => panic!("Expected Bool variant"),
    }
}

#[test]
fn spanned_untagged_enum_number() {
    #[derive(Facet, Debug)]
    struct Config {
        value: DebugLevel,
    }

    let config: Config = toml::from_str(r#"value = 2"#).unwrap();
    match config.value {
        DebugLevel::Number(spanned_num) => assert_eq!(*spanned_num, 2),
        _ => panic!("Expected Number variant"),
    }
}

#[test]
fn spanned_untagged_enum_string() {
    #[derive(Facet, Debug)]
    struct Config {
        value: DebugLevel,
    }

    let config: Config = toml::from_str(r#"value = "full""#).unwrap();
    match config.value {
        DebugLevel::String(spanned_str) => assert_eq!(*spanned_str, "full"),
        _ => panic!("Expected String variant"),
    }
}

// ============================================================================
// Error diagnostics with spans
// ============================================================================

#[derive(Facet, Debug)]
struct PackageMetadata {
    name: String,
    version: String,
    readme: ReadmeValue,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[facet(untagged)]
enum ReadmeValue {
    Path(String),
    Workspace { workspace: bool },
}

#[test]
fn type_mismatch_preserves_span() {
    let toml_str = r#"
[package]
name = "test"
version = "0.1.0"
readme = false
"#;

    #[derive(Facet, Debug)]
    struct CargoManifest {
        package: PackageMetadata,
    }

    let result: Result<CargoManifest, DeserializeError<TomlError>> = toml::from_str(toml_str);
    assert!(result.is_err(), "Should fail with type mismatch");

    let error_msg = format!("{}", result.unwrap_err());
    assert!(
        error_msg.contains("reflection error")
            || error_msg.contains("Wrong shape")
            || error_msg.contains("Reflect"),
        "Error should mention reflection/shape issue: {}",
        error_msg
    );
}

#[test]
fn valid_readme_string() {
    let toml_str = r#"
[package]
name = "test"
version = "1.0.0"
readme = "README.md"
"#;

    #[derive(Facet, Debug)]
    struct CargoManifest {
        package: PackageMetadata,
    }

    let result: CargoManifest = toml::from_str(toml_str).unwrap();
    match result.package.readme {
        ReadmeValue::Path(path) => assert_eq!(path, "README.md"),
        _ => panic!("Expected Path variant"),
    }
}

#[test]
fn valid_readme_workspace() {
    let toml_str = r#"
[package]
name = "test"
version = "1.0.0"
readme = { workspace = true }
"#;

    #[derive(Facet, Debug)]
    struct CargoManifest {
        package: PackageMetadata,
    }

    let result: CargoManifest = toml::from_str(toml_str).unwrap();
    match result.package.readme {
        ReadmeValue::Workspace { workspace } => assert!(workspace),
        _ => panic!("Expected Workspace variant"),
    }
}
