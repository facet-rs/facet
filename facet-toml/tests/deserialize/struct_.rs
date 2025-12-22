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
        )
        .unwrap(),
        Root {
            value: 1,
            table: Table { value: 2 },
        },
    );

    assert!(matches!(
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
    ));
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
        .unwrap(),
        Root {
            value: 1,
            unit: Unit(2),
        },
    );

    assert!(matches!(
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
    ));
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
        .unwrap(),
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
        )
        .unwrap(),
        Root {
            a: 1,
            b: true,
            c: "::1".parse().unwrap()
        },
    );

    assert!(matches!(
        facet_toml::from_str::<Root>(
            r#"
            b = true
            c = '::1'
            "#
        )
        .unwrap_err()
        .kind,
        TomlDeErrorKind::ExpectedFieldWithName("a")
    ));
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
        )
        .unwrap(),
        Root {
            nested: Nested {
                a: 1,
                b: true,
                c: "::1".parse().unwrap()
            }
        },
    );

    assert!(matches!(
        facet_toml::from_str::<Root>("a = 1").unwrap_err().kind,
        TomlDeErrorKind::ExpectedFieldWithName("nested")
    ));
    assert!(matches!(
        facet_toml::from_str::<Root>("nested = true")
            .unwrap_err()
            .kind,
        TomlDeErrorKind::ParseSingleValueAsMultipleFieldStruct
    ));
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
        )
        .unwrap(),
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
        )
        .unwrap(),
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
        )
        .unwrap(),
        Root {
            a: i32::default(),
            b: bool::default(),
            c: "hi".to_owned()
        },
    );
}

#[test]
fn test_root_struct_deserialize_individual_defaults() {
    fn default_string() -> String {
        "hi".to_string()
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        #[facet(default = 42)]
        a: i32,
        #[facet(default = Some(true))]
        b: Option<bool>,
        #[facet(default = default_string())]
        c: String,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("").unwrap(),
        Root {
            a: 42,
            b: Some(true),
            c: "hi".to_string()
        },
    );
}

#[test]
fn test_root_struct_deserialize_container_defaults() {
    fn default_string() -> String {
        "hi".to_string()
    }

    #[derive(Debug, Facet, PartialEq)]
    #[facet(default)]
    struct Root {
        a: i32,
        b: Option<bool>,
        c: String,
    }

    impl Default for Root {
        fn default() -> Self {
            Self {
                a: 42,
                b: Some(true),
                c: default_string(),
            }
        }
    }

    assert_eq!(
        facet_toml::from_str::<Root>("").unwrap(),
        Root {
            a: 42,
            b: Some(true),
            c: "hi".to_string()
        },
    );
}

#[test]
fn test_ignore_unknown_table_keys() {
    // Reproduction test from issue #1344
    // facet-toml should ignore unknown table keys during deserialization
    #[derive(Debug, Facet, PartialEq)]
    struct Manifest {
        pkg: PkgTable,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct PkgTable {
        rustc: Option<Package>,
        #[facet(rename = "rust-std")]
        rust_std: Option<Package>,
        // Should ignore "cargo" and other unknown fields
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Package {
        version: String,
    }

    let toml = r#"
        [pkg.rustc]
        version = "1.75.0"

        [pkg.rust-std]
        version = "1.75.0"

        [pkg.cargo]
        version = "1.75.0"

        [pkg.rust-docs]
        version = "1.75.0"
    "#;

    let result = facet_toml::from_str::<Manifest>(toml).unwrap();
    assert_eq!(
        result,
        Manifest {
            pkg: PkgTable {
                rustc: Some(Package {
                    version: "1.75.0".to_string()
                }),
                rust_std: Some(Package {
                    version: "1.75.0".to_string()
                }),
            }
        }
    );
}

// Unknown fields are silently ignored by default (matching serde behavior).
// Use #[facet(deny_unknown_fields)] to enable strict mode.
// See tests/issue_1363.rs for comprehensive test coverage.
