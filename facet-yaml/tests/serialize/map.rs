//! Tests for TOML values to maps.

use std::collections::HashMap;

use eyre::Result;
use facet::Facet;
use facet_yaml::YamlSerError;

use crate::assert_serialize;

#[test]
fn test_scalar_map() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: HashMap<String, i32>,
    }

    assert_serialize!(
        Root,
        Root {
            values: HashMap::new()
        },
    );

    assert_serialize!(
        Root,
        Root {
            values: [("a".to_string(), 0), ("b".to_string(), -1)].into()
        },
    );

    Ok(())
}

#[test]
#[ignore = "must be fixed in deserialize"]
fn test_optional_scalar_map() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Option<HashMap<String, i32>>,
    }

    assert_serialize!(Root, Root { values: None },);

    assert_serialize!(
        Root,
        Root {
            values: Some(HashMap::new())
        },
    );

    assert_serialize!(
        Root,
        Root {
            values: Some([("a".to_string(), 0), ("b".to_string(), -1)].into())
        },
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

    assert_serialize!(
        Root,
        Root {
            values: HashMap::new(),
            other: 1,
        },
    );

    assert_serialize!(
        Root,
        Root {
            values: [("a".to_string(), 0), ("b".to_string(), -1)].into(),
            other: 2,
        },
    );

    Ok(())
}

#[test]
#[ignore = "must be fixed in deserialize"]
fn test_unit_struct_map() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: HashMap<String, Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item(bool);

    assert_serialize!(
        Root,
        Root {
            values: HashMap::new()
        },
    );

    assert_serialize!(
        Root,
        Root {
            values: [
                ("a".to_string(), Item(true)),
                ("b".to_string(), Item(false))
            ]
            .into()
        },
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

    assert_serialize!(
        Root,
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

#[test]
#[ignore = "Must be fixed in facet-yaml deserialize"]
fn test_optional_struct_map() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        dependencies: HashMap<String, Dependency>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Dependency {
        version: Option<String>,
        optional: Option<bool>,
    }

    assert_serialize!(
        Root,
        Root {
            dependencies: [
                (
                    "syn".to_string(),
                    Dependency {
                        version: Some("1".to_string()),
                        optional: None,
                    }
                ),
                (
                    "paste".to_string(),
                    Dependency {
                        version: None,
                        optional: Some(true),
                    }
                ),
                (
                    "serde".to_string(),
                    Dependency {
                        version: None,
                        optional: None,
                    }
                )
            ]
            .into()
        },
    );

    Ok(())
}

#[test]
fn test_invalid_map_key() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: HashMap<bool, i32>,
    }

    assert!(matches!(
        facet_yaml::to_string(&Root {
            value: [(true, 0)].into()
        })
        .unwrap_err(),
        YamlSerError::InvalidKeyConversion { .. }
    ));

    Ok(())
}
