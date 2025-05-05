//! Tests for TOML values to lists.

use eyre::Result;
use facet::Facet;
use facet_toml::TomlDeErrorKind;

#[test]
fn test_scalar_list() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<i32>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("values = []")?,
        Root { values: Vec::new() },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [2]")?,
        Root { values: vec![2] },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [1, -1, 0, 100]")?,
        Root {
            values: vec![1, -1, 0, 100],
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );

    Ok(())
}

#[test]
fn test_unit_struct_list() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item(i32);

    assert_eq!(
        facet_toml::from_str::<Root>("values = []")?,
        Root { values: Vec::new() },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [2]")?,
        Root {
            values: vec![Item(2)]
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [1, -1, 0, 100]")?,
        Root {
            values: vec![Item(1), Item(-1), Item(0), Item(100)],
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [true]")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [1, true]")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );

    Ok(())
}

#[test]
fn test_nested_lists() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<Vec<i32>>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("values = []")?,
        Root { values: Vec::new() },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [[], []]")?,
        Root {
            values: vec![Vec::new(); 2]
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [[2]]")?,
        Root {
            values: vec![vec![2]]
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [[1, -1], [0], [100], []]")?,
        Root {
            values: vec![vec![1, -1], vec![0], vec![100], vec![]],
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [true]")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [[1], true]")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );

    Ok(())
}
