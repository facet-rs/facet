//! Tests for #[facet(flatten)] attribute in TOML.
//!
//! Flatten allows capturing unknown fields into a map or Value.

use std::collections::HashMap;

use facet::Facet;
use facet_value::Value;

// ============================================================================
// Flatten with Value type
// ============================================================================

#[derive(Facet, Debug, Clone)]
pub struct Badge {
    #[facet(flatten)]
    pub attributes: Value,
}

#[derive(Facet, Debug)]
pub struct Config {
    pub badges: HashMap<String, Badge>,
}

#[test]
fn flatten_value_with_only_known_fields() {
    let toml = r#"
[badges.appveyor]
repository = "user/repo"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert!(config.badges.contains_key("appveyor"));
}

#[test]
fn flatten_value_empty_table() {
    let toml = r#"
[badges.appveyor]
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert!(config.badges.contains_key("appveyor"));
    let badge = &config.badges["appveyor"];
    assert!(badge.attributes.is_object());
    assert!(badge.attributes.as_object().unwrap().is_empty());
}

#[test]
fn flatten_value_mixed_fields() {
    #[derive(Facet, Debug)]
    pub struct BadgeWithKnown {
        pub repository: Option<String>,
        #[facet(flatten)]
        pub attributes: Value,
    }

    #[derive(Facet, Debug)]
    pub struct ConfigWithKnown {
        pub badges: HashMap<String, BadgeWithKnown>,
    }

    let toml = r#"
[badges.appveyor]
repository = "user/repo"
branch = "main"
service = "appveyor"
"#;

    let config: ConfigWithKnown = facet_toml::from_str(toml).unwrap();
    let badge = &config.badges["appveyor"];

    // Known field
    assert_eq!(badge.repository.as_ref().unwrap(), "user/repo");

    // Flattened fields
    let attrs = badge.attributes.as_object().unwrap();
    assert!(attrs.contains_key("branch"));
    assert!(attrs.contains_key("service"));
}

// ============================================================================
// Flatten in nested array structures
// ============================================================================

#[derive(Facet, Debug)]
struct Root {
    pub item: Vec<Item>,
}

#[derive(Facet, Debug)]
struct Item {
    pub nested_item: NestedItem,
}

#[derive(Facet, Debug)]
struct NestedItem {
    #[facet(flatten)]
    pub extra: HashMap<String, Value>,
}

#[test]
fn table_in_array_with_flatten() {
    let toml = r#"
        [[item]]

        [item.nested_item]
        foo = 1
        bar = 2
    "#;

    let result: Root = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.item.len(), 1);
    assert_eq!(result.item[0].nested_item.extra.len(), 2);
    assert!(result.item[0].nested_item.extra.contains_key("foo"));
    assert!(result.item[0].nested_item.extra.contains_key("bar"));
}

#[test]
fn multiple_array_elements_with_flatten() {
    let toml = r#"
        [[item]]

        [item.nested_item]
        foo = 1
        bar = 2

        [[item]]

        [item.nested_item]
        baz = 3
    "#;

    let result: Root = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.item.len(), 2);
    assert_eq!(result.item[0].nested_item.extra.len(), 2);
    assert_eq!(result.item[1].nested_item.extra.len(), 1);
}

#[test]
fn flatten_with_single_key_works() {
    let toml = r#"
        [[item]]

        [item.nested_item]
        foo = 1
    "#;

    let result: Root = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.item.len(), 1);
    assert_eq!(result.item[0].nested_item.extra.len(), 1);
}
