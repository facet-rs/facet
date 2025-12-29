//! Tests for TOML values to lists.

use facet::Facet;
use facet_testhelpers::test;
use facet_toml_legacy::TomlDeErrorKind;

#[test]
fn test_scalar_list() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<i32>,
    }

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = []").unwrap(),
        Root { values: Vec::new() },
    );

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = [2]").unwrap(),
        Root { values: vec![2] },
    );

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = [1, -1, 0, 100]").unwrap(),
        Root {
            values: vec![1, -1, 0, 100],
        },
    );

    assert!(matches!(
        facet_toml_legacy::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    ));
}

#[test]
fn test_unit_struct_list() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item(i32);

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = []").unwrap(),
        Root { values: Vec::new() },
    );

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = [2]").unwrap(),
        Root {
            values: vec![Item(2)]
        },
    );

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = [1, -1, 0, 100]").unwrap(),
        Root {
            values: vec![Item(1), Item(-1), Item(0), Item(100)],
        },
    );

    assert!(matches!(
        facet_toml_legacy::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    ));
    assert!(matches!(
        facet_toml_legacy::from_str::<Root>("values = [true]")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    ));
    assert!(matches!(
        facet_toml_legacy::from_str::<Root>("values = [1, true]")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    ));
}

#[test]
fn test_nested_lists() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<Vec<i32>>,
    }

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = []").unwrap(),
        Root { values: Vec::new() },
    );
    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = [[], []]").unwrap(),
        Root {
            values: vec![Vec::new(); 2]
        },
    );

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = [[2]]").unwrap(),
        Root {
            values: vec![vec![2]]
        },
    );

    assert_eq!(
        facet_toml_legacy::from_str::<Root>("values = [[1, -1], [0], [100], []]").unwrap(),
        Root {
            values: vec![vec![1, -1], vec![0], vec![100], vec![]],
        },
    );

    assert!(matches!(
        facet_toml_legacy::from_str::<Root>("values = true")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    ));
    assert!(matches!(
        facet_toml_legacy::from_str::<Root>("values = [true]")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    ));
    assert!(matches!(
        facet_toml_legacy::from_str::<Root>("values = [[1], true]")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ExpectedType {
            expected: "array",
            got: "boolean"
        }
    ));
}
