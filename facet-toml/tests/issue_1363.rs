//! Tests for issue #1363: Unknown fields should be ignored by default
//!
//! This test verifies that:
//! 1. Unknown fields are silently ignored by default (like serde)
//! 2. #[facet(deny_unknown_fields)] makes deserialization strict
//! 3. Works for both table headers and key-value pairs

use facet::Facet;

#[test]
fn unknown_fields_ignored_by_default_keyvalue() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        name: String,
    }

    let toml = r#"
        name = "foo"
        extra_field = "bar"
        another_unknown = 42
    "#;

    let result = facet_toml::from_str::<Config>(toml).unwrap();
    assert_eq!(
        result,
        Config {
            name: "foo".to_string()
        }
    );
}

#[test]
fn unknown_fields_ignored_by_default_table_header() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        known: Option<KnownSection>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct KnownSection {
        value: i32,
    }

    let toml = r#"
        [known]
        value = 123

        [unknown_section]
        foo = "bar"
        baz = 456
    "#;

    let result = facet_toml::from_str::<Config>(toml).unwrap();
    assert_eq!(
        result,
        Config {
            known: Some(KnownSection { value: 123 })
        }
    );
}

#[test]
fn deny_unknown_fields_rejects_keyvalue() {
    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct Config {
        name: String,
    }

    let toml = r#"
        name = "foo"
        extra_field = "bar"
    "#;

    let result = facet_toml::from_str::<Config>(toml);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("extra_field") || err.contains("Unknown field"),
        "Error should mention the unknown field: {err}"
    );
}

#[test]
fn deny_unknown_fields_rejects_table_header() {
    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct Config {
        known: Option<KnownSection>,
    }

    #[derive(Facet, Debug)]
    struct KnownSection {
        value: i32,
    }

    let toml = r#"
        [known]
        value = 123

        [unknown_section]
        foo = "bar"
    "#;

    let result = facet_toml::from_str::<Config>(toml);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("unknown_section") || err.contains("Unknown field"),
        "Error should mention the unknown field: {err}"
    );
}

#[test]
fn deny_unknown_fields_accepts_known_fields() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields)]
    struct Config {
        name: String,
        version: Option<String>,
    }

    let toml = r#"
        name = "foo"
        version = "1.0"
    "#;

    let result = facet_toml::from_str::<Config>(toml).unwrap();
    assert_eq!(
        result,
        Config {
            name: "foo".to_string(),
            version: Some("1.0".to_string()),
        }
    );
}

#[test]
fn cargo_toml_example_without_deny() {
    // This is the motivating example from the issue - parsing Cargo.toml
    // without having to enumerate all possible fields
    #[derive(Facet, Debug, PartialEq)]
    struct Manifest {
        package: Package,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Package {
        name: String,
        version: Option<String>,
    }

    let toml = r#"
        [package]
        name = "my-crate"
        version = "0.1.0"
        edition = "2021"
        authors = ["Someone"]
        license = "MIT"
        description = "A cool crate"
        repository = "https://github.com/example/example"
        keywords = ["example", "test"]
        categories = ["development-tools"]
    "#;

    let result = facet_toml::from_str::<Manifest>(toml).unwrap();
    assert_eq!(
        result.package,
        Package {
            name: "my-crate".to_string(),
            version: Some("0.1.0".to_string()),
        }
    );
}

#[test]
fn unknown_nested_fields_ignored() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        database: Database,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Database {
        host: String,
    }

    let toml = r#"
        [database]
        host = "localhost"
        port = 5432
        username = "admin"
        password = "secret"
        max_connections = 100
    "#;

    let result = facet_toml::from_str::<Config>(toml).unwrap();
    assert_eq!(
        result,
        Config {
            database: Database {
                host: "localhost".to_string()
            }
        }
    );
}

#[test]
fn deny_unknown_nested_fields() {
    #[derive(Facet, Debug)]
    struct Config {
        database: Database,
    }

    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct Database {
        host: String,
    }

    let toml = r#"
        [database]
        host = "localhost"
        port = 5432
    "#;

    let result = facet_toml::from_str::<Config>(toml);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("port") || err.contains("Unknown field"),
        "Error should mention the unknown field: {err}"
    );
}
