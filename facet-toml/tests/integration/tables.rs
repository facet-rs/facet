//! Tests for TOML table handling.
//!
//! Includes empty tables, dotted keys, and nested table structures.

use facet::Facet;
use facet_value::Value;
use std::collections::HashMap;

// ============================================================================
// Empty tables
// ============================================================================

#[test]
fn empty_toml_table() {
    #[derive(Facet, Debug)]
    struct PackageProfile {
        opt_level: Option<i64>,
    }

    #[derive(Facet, Debug)]
    struct Profile {
        package: Option<HashMap<String, PackageProfile>>,
    }

    #[derive(Facet, Debug)]
    struct Config {
        profile: HashMap<String, Profile>,
    }

    let toml = r#"
[profile.release.package]
# zed = { codegen-units = 16 }
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert!(config.profile.contains_key("release"));
    let release_profile = &config.profile["release"];
    assert!(release_profile.package.is_some());
    assert!(release_profile.package.as_ref().unwrap().is_empty());
}

#[test]
fn empty_toml_table_at_root() {
    #[derive(Facet, Debug)]
    struct RootConfig {
        empty_section: Option<HashMap<String, String>>,
    }

    let toml = r#"
[empty_section]
# All fields commented out
"#;

    let config: RootConfig = facet_toml::from_str(toml).unwrap();
    assert!(config.empty_section.is_some());
    assert!(config.empty_section.as_ref().unwrap().is_empty());
}

#[test]
fn empty_toml_table_with_value_type() {
    #[derive(Facet, Debug)]
    struct Config {
        metadata: Option<Value>,
        other: Option<String>,
    }

    let toml = r#"
[metadata]
# All fields commented out

[other_section]
value = "test"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert!(config.metadata.is_some());
    let val = config.metadata.as_ref().unwrap();
    assert!(val.is_object());
    let obj = val.as_object().unwrap();
    assert!(obj.is_empty());
}

#[test]
fn empty_table_followed_by_sibling() {
    #[derive(Facet, Debug)]
    struct PackageProfile {
        opt_level: Option<i64>,
    }

    #[derive(Facet, Debug)]
    struct Profile {
        package: Option<HashMap<String, PackageProfile>>,
        inherits: Option<String>,
    }

    #[derive(Facet, Debug)]
    struct Config {
        profile: HashMap<String, Profile>,
    }

    let toml = r#"
[profile.release.package]
# zed = { codegen-units = 16 }

[profile.release-fast]
inherits = "release"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert!(config.profile.contains_key("release"));
    assert!(config.profile.contains_key("release-fast"));
}

// ============================================================================
// Dotted keys
// ============================================================================

#[derive(Facet, Debug)]
struct Workspace {
    members: Vec<String>,
    metadata: Option<Value>,
}

#[derive(Facet, Debug)]
struct Manifest {
    workspace: Option<Workspace>,
}

#[test]
fn dotted_key_with_string_value() {
    let toml = r#"
[workspace]
members = []

[workspace.metadata.typos]
default.simple = "value"
"#;

    let manifest: Manifest = facet_toml::from_str(toml).unwrap();
    assert!(manifest.workspace.is_some());
    let workspace = manifest.workspace.as_ref().unwrap();
    assert!(workspace.metadata.is_some());
}

#[test]
fn dotted_key_with_array_value() {
    let toml = r#"
[workspace]
members = []

[workspace.metadata.typos]
default.extend-ignore-re = ["clonable"]
"#;

    let manifest: Manifest = facet_toml::from_str(toml).unwrap();
    let workspace = manifest.workspace.as_ref().unwrap();
    let metadata = workspace.metadata.as_ref().unwrap();
    assert!(metadata.is_object());
    let obj = metadata.as_object().unwrap();
    assert!(obj.contains_key("typos"));
}

#[test]
fn dotted_key_with_various_types() {
    let toml = r#"
[workspace]
members = []

[workspace.metadata.test]
a.string = "value"
b.integer = 42
c.boolean = true
d.array = [1, 2, 3]
e.inline-table = { key = "value" }
"#;

    let manifest: Manifest = facet_toml::from_str(toml).unwrap();
    let workspace = manifest.workspace.as_ref().unwrap();
    assert!(workspace.metadata.is_some());
}
