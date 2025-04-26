//! Tests for TOML table values.

use std::net::Ipv6Addr;

use eyre::Result;
use facet::Facet;
use facet_toml::error::TomlErrorKind;

#[test]
fn test_table_to_struct() -> Result<()> {
    facet_testhelpers::setup();

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
        )?,
        Root {
            value: 1,
            table: Table { value: 2 },
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            value = 1
            table.value.too-deep = 2
            "#
        )
        .unwrap_err()
        .kind,
        TomlErrorKind::ExpectedType {
            expected: "value",
            got: "table"
        }
    );

    Ok(())
}

#[test]
fn test_unit_struct() -> Result<()> {
    facet_testhelpers::setup();

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
        )?,
        Root {
            value: 1,
            unit: Unit(2),
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            value = 1
            unit = false
            "#
        )
        .unwrap_err()
        .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );

    Ok(())
}

#[test]
fn test_nested_unit_struct() -> Result<()> {
    facet_testhelpers::setup();

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
        )?,
        Root {
            value: 1,
            unit: NestedUnit(Unit(2)),
        },
    );

    Ok(())
}

#[test]
fn test_root_struct_multiple_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        a: i32,
        b: bool,
        c: Ipv6Addr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            a = 1
            b = true
            c = '::1'
            "#
        )?,
        Root {
            a: 1,
            b: true,
            c: "::1".parse().unwrap()
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            b = true
            c = '::1'
            "#
        )
        .unwrap_err()
        .kind,
        TomlErrorKind::ExpectedFieldWithName("a")
    );

    Ok(())
}

#[test]
fn test_nested_struct_multiple_fields() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        nested: Nested,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Nested {
        a: i32,
        b: bool,
        c: Ipv6Addr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            [nested]
            a = 1
            b = true
            c = '::1'
            "#
        )?,
        Root {
            nested: Nested {
                a: 1,
                b: true,
                c: "::1".parse().unwrap()
            }
        },
    );

    assert_eq!(
        facet_toml::from_str::<Root>("a = 1").unwrap_err().kind,
        TomlErrorKind::ExpectedFieldWithName("nested")
    );
    assert_eq!(
        facet_toml::from_str::<Root>("nested = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ParseSingleValueAsMultipleFieldStruct
    );

    Ok(())
}
