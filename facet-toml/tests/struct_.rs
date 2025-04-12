//! Tests for TOML table values.

use facet::Facet;

#[test]
fn test_table_to_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
        table: Table,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Table {
        value: i32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            value = 1
            table.value = 2
            "#
        )
        .expect("Failed to parse TOML"),
        Root {
            value: 1,
            table: Table { value: 2 },
        },
    );
}

#[test]
fn test_unit_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
        unit: Unit,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Unit(i32);

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            value = 1
            unit = 2
            "#
        )
        .expect("Failed to parse TOML"),
        Root {
            value: 1,
            unit: Unit(2),
        },
    );
}

#[test]
fn test_nested_unit_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
        unit: NestedUnit,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct NestedUnit(Unit);

    #[derive(Debug, Facet, PartialEq)]
    struct Unit(i32);

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            value = 1
            unit = 2
            "#
        )
        .expect("Failed to parse TOML"),
        Root {
            value: 1,
            unit: NestedUnit(Unit(2)),
        },
    );
}
