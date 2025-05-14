//! Tests for TOML table values.

use std::net::Ipv6Addr;

use facet::Facet;
use facet_testhelpers::test;
use facet_toml::TomlDeErrorKind;

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
        TomlDeErrorKind::ExpectedType {
            expected: "value",
            got: "table"
        }
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
        TomlDeErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
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
        )?,
        Root {
            value: 1,
            unit: NestedUnit(Unit(2)),
        },
    );
}

#[test]
fn test_root_struct_multiple_fields() {
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
        TomlDeErrorKind::ExpectedFieldWithName("a")
    );
}

#[test]
fn test_nested_struct_multiple_fields() {
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
        TomlDeErrorKind::ExpectedFieldWithName("nested")
    );
    assert_eq!(
        facet_toml::from_str::<Root>("nested = true")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ParseSingleValueAsMultipleFieldStruct
    );
}

#[test]
fn test_rename_single_struct_fields() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        #[facet(rename = "1")]
        a: i32,
        #[facet(rename = "with spaces")]
        b: bool,
        #[facet(rename = "'quoted'")]
        c: String,
        #[facet(rename = "")]
        d: usize,
    }

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            1 = 1
            "with spaces" = true
            "'quoted'" = 'quoted'
            "" = 2
            "#
        )?,
        Root {
            a: 1,
            b: true,
            c: "quoted".parse().unwrap(),
            d: 2
        },
    );
}

#[test]
fn test_rename_all_struct_fields() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(rename_all = "kebab-case")]
    struct Root {
        a_number: i32,
        another_bool: bool,
        #[facet(rename = "Overwrite")]
        shouldnt_matter: f32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            a-number = 1
            another-bool = true
            Overwrite = 1.0
            "#
        )?,
        Root {
            a_number: 1,
            another_bool: true,
            shouldnt_matter: 1.0
        },
    );
}

#[test]
fn test_default_struct_fields() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        #[facet(default)]
        a: i32,
        #[facet(default)]
        b: bool,
        #[facet(default)]
        c: String,
    }

    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            c = "hi"
            "#
        )?,
        Root {
            a: i32::default(),
            b: bool::default(),
            c: "hi".to_owned()
        },
    );
}
