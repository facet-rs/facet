//! Tests for TOML values to lists.

use facet::Facet;
use facet_toml::error::TomlErrorKind;

#[test]
fn test_scalar_list() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<i32>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("values = []").expect("Failed to parse TOML"),
        Root { values: Vec::new() },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [2]").expect("Failed to parse TOML"),
        Root { values: vec![2] },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [1, -1, 0, 100]").expect("Failed to parse TOML"),
        Root {
            values: vec![1, -1, 0, 100],
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );
}

#[test]
fn test_unit_struct_list() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item(i32);

    assert_eq!(
        facet_toml::from_str::<Root>("values = []").expect("Failed to parse TOML"),
        Root { values: Vec::new() },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [2]").expect("Failed to parse TOML"),
        Root {
            values: vec![Item(2)]
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [1, -1, 0, 100]").expect("Failed to parse TOML"),
        Root {
            values: vec![Item(1), Item(-1), Item(0), Item(100)],
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [true]")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [1, true]")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_nested_lists() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<Vec<i32>>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("values = []").expect("Failed to parse TOML"),
        Root { values: Vec::new() },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [[], []]").expect("Failed to parse TOML"),
        Root {
            values: vec![Vec::new(); 2]
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [[2]]").expect("Failed to parse TOML"),
        Root {
            values: vec![vec![2]]
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = [[1, -1], [0], [100], []]")
            .expect("Failed to parse TOML"),
        Root {
            values: vec![vec![1, -1], vec![0], vec![100], vec![]],
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [true]")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("values = [[1], true]")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    );
}
