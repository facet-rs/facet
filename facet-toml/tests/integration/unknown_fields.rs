//! Tests for handling unknown fields in TOML.
//!
//! By default, facet-toml ignores unknown fields. This can be changed with
//! `#[facet(deny_unknown_fields)]`.

use facet::Facet;

#[test]
fn ignore_unknown_fields_by_default() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        version: Option<u32>,
        package: Option<Vec<Package>>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Package {
        name: String,
    }

    let toml = r#"
version = 3

[[package]]
name = "foo"

[metadata]
"checksum foo" = "abc123"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.version, Some(3));
    assert_eq!(config.package.as_ref().unwrap().len(), 1);
    assert_eq!(config.package.as_ref().unwrap()[0].name, "foo");
}

#[test]
fn deny_unknown_fields_when_explicitly_set() {
    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct Config {
        version: Option<u32>,
        package: Option<Vec<Package>>,
    }

    #[derive(Facet, Debug)]
    struct Package {
        name: String,
    }

    let toml = r#"
version = 3

[metadata]
"checksum foo" = "abc123"
"#;

    let result: Result<Config, _> = facet_toml::from_str(toml);
    assert!(
        result.is_err(),
        "Should error on unknown field with deny_unknown_fields"
    );
}

#[test]
fn ignore_unknown_table_with_nested_fields() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        name: String,
    }

    let toml = r#"
name = "test"

[unknown_section]
field1 = "value1"
field2 = 42

[unknown_section.nested]
field3 = true
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.name, "test");
}

#[test]
fn ignore_unknown_array_table() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        version: u32,
    }

    let toml = r#"
version = 1

[[unknown_array]]
name = "item1"

[[unknown_array]]
name = "item2"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.version, 1);
}

#[test]
fn mixed_known_and_unknown_fields() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        app: App,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct App {
        name: String,
        version: u32,
    }

    let toml = r#"
[app]
name = "myapp"
version = 1
unknown_field = "should be ignored"

[completely_unknown]
foo = "bar"
"#;

    let config: Config = facet_toml::from_str(toml).unwrap();
    assert_eq!(config.app.name, "myapp");
    assert_eq!(config.app.version, 1);
}
