//! Tests for TOML values to maps.

use std::collections::HashMap;

use eyre::Result;
use facet::Facet;
use facet_toml::error::TomlErrorKind;

#[test]
fn test_scalar_map() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: HashMap<String, i32>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("[values]")?,
        Root {
            values: HashMap::new()
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            [values]
            a = 0
            b = -1
            "#
        )?,
        Root {
            values: [("a".to_string(), 0), ("b".to_string(), -1)].into()
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "table like structure",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values.a = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("[values.a]").unwrap_err().kind,
        TomlErrorKind::ExpectedType {
            expected: "value",
            got: "table"
        }
    );

    Ok(())
}

#[test]
fn test_scalar_map_with_other_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: HashMap<String, i32>,
        other: i32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            other = 1
            [values]
            "#
        )?,
        Root {
            values: HashMap::new(),
            other: 1,
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            other = 2
            [values]
            a = 0
            b = -1
            "#
        )?,
        Root {
            values: [("a".to_string(), 0), ("b".to_string(), -1)].into(),
            other: 2,
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "table like structure",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values.a = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("[values.a]").unwrap_err().kind,
        TomlErrorKind::ExpectedType {
            expected: "value",
            got: "table"
        }
    );

    Ok(())
}

#[test]
fn test_unit_struct_map() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: HashMap<String, Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item(bool);

    assert_eq!(
        facet_toml::from_str::<Root>("[values]")?,
        Root {
            values: HashMap::new()
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            values.a = true
            values.b = false
            "#
        )?,
        Root {
            values: [
                ("a".to_string(), Item(true)),
                ("b".to_string(), Item(false))
            ]
            .into()
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "table like structure",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values.a = 10")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "boolean",
            got: "integer"
        }
    );

    Ok(())
}

#[test]
fn test_struct_map() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        dependencies: HashMap<String, Dependency>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Dependency {
        version: String,
        optional: bool,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("[dependencies]")?,
        Root {
            dependencies: HashMap::new()
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            [dependencies]
            syn = { version = "1", optional = false }
            paste = { version = "0.0.1", optional = true }
            "#
        )?,
        Root {
            dependencies: [
                (
                    "syn".to_string(),
                    Dependency {
                        version: "1".to_string(),
                        optional: false,
                    }
                ),
                (
                    "paste".to_string(),
                    Dependency {
                        version: "0.0.1".to_string(),
                        optional: true,
                    }
                )
            ]
            .into()
        },
    );

    Ok(())
}
