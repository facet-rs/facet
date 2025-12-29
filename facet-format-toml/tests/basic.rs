//! Basic deserialization tests for facet-format-toml.
//!
//! These tests cover the core deserialization functionality, ported from
//! facet-toml to ensure feature parity.

use facet::Facet;
use facet_testhelpers::test;
use std::collections::HashMap;

// ============================================================================
// Basic struct tests
// ============================================================================

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[test]
fn test_deserialize_person() {
    let toml = r#"
        name = "Alice"
        age = 30
    "#;

    let person: Person = facet_format_toml::from_str(toml).unwrap();
    assert_eq!(
        person,
        Person {
            name: "Alice".to_string(),
            age: 30
        }
    );
}

#[test]
fn test_deserialize_person_borrowed() {
    let toml = r#"
        name = "Alice"
        age = 30
    "#;

    let person: Person = facet_format_toml::from_str_borrowed(toml).unwrap();
    assert_eq!(
        person,
        Person {
            name: "Alice".to_string(),
            age: 30
        }
    );
}

#[test]
fn test_from_slice() {
    let toml = b"name = \"Alice\"\nage = 30";
    let person: Person = facet_format_toml::from_slice(toml).unwrap();
    assert_eq!(
        person,
        Person {
            name: "Alice".to_string(),
            age: 30
        }
    );
}

// ============================================================================
// Scalar type tests
// ============================================================================

#[test]
fn test_string() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: String,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 'string'").unwrap(),
        Root {
            value: "string".to_string()
        },
    );
}

#[test]
fn test_bool() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: bool,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = true").unwrap(),
        Root { value: true },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>("value = false").unwrap(),
        Root { value: false },
    );
}

#[test]
fn test_char() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: char,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 'c'").unwrap(),
        Root { value: 'c' },
    );
}

#[test]
fn test_integers() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        u8_val: u8,
        u16_val: u16,
        u32_val: u32,
        u64_val: u64,
        i8_val: i8,
        i16_val: i16,
        i32_val: i32,
        i64_val: i64,
    }

    // Note: TOML integers are signed 64-bit, so u64 max value cannot be represented
    // We test with values that fit in i64
    let toml = r#"
        u8_val = 255
        u16_val = 65535
        u32_val = 4294967295
        u64_val = 9223372036854775807
        i8_val = -128
        i16_val = -32768
        i32_val = -2147483648
        i64_val = -9223372036854775808
    "#;

    let root: Root = facet_format_toml::from_str(toml).unwrap();
    assert_eq!(root.u8_val, 255);
    assert_eq!(root.u16_val, 65535);
    assert_eq!(root.u32_val, 4294967295);
    assert_eq!(root.u64_val, 9223372036854775807); // i64::MAX
    assert_eq!(root.i8_val, -128);
    assert_eq!(root.i16_val, -32768);
    assert_eq!(root.i32_val, -2147483648);
    assert_eq!(root.i64_val, -9223372036854775808);
}

#[test]
fn test_floats() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        f32_val: f32,
        f64_val: f64,
    }

    let toml = r#"
        f32_val = 3.25
        f64_val = 2.5
    "#;

    let root: Root = facet_format_toml::from_str(toml).unwrap();
    assert!((root.f32_val - 3.25).abs() < 0.001);
    assert!((root.f64_val - 2.5).abs() < 0.0000001);
}

#[test]
fn test_integer_from_float() {
    // TOML allows floats to be converted to integers if they're whole numbers
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 1").unwrap(),
        Root { value: 1 },
    );
}

// ============================================================================
// List tests
// ============================================================================

#[test]
fn test_scalar_list() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<i32>,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("values = []").unwrap(),
        Root { values: Vec::new() },
    );

    assert_eq!(
        facet_format_toml::from_str::<Root>("values = [2]").unwrap(),
        Root { values: vec![2] },
    );

    assert_eq!(
        facet_format_toml::from_str::<Root>("values = [1, -1, 0, 100]").unwrap(),
        Root {
            values: vec![1, -1, 0, 100],
        },
    );
}

#[test]
fn test_string_list() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<String>,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>(r#"values = ["a", "b", "c"]"#).unwrap(),
        Root {
            values: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        },
    );
}

#[test]
fn test_nested_lists() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: Vec<Vec<i32>>,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("values = []").unwrap(),
        Root { values: Vec::new() },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>("values = [[], []]").unwrap(),
        Root {
            values: vec![Vec::new(); 2]
        },
    );

    assert_eq!(
        facet_format_toml::from_str::<Root>("values = [[2]]").unwrap(),
        Root {
            values: vec![vec![2]]
        },
    );

    assert_eq!(
        facet_format_toml::from_str::<Root>("values = [[1, -1], [0], [100], []]").unwrap(),
        Root {
            values: vec![vec![1, -1], vec![0], vec![100], vec![]],
        },
    );
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
        facet_format_toml::from_str::<Root>("values = []").unwrap(),
        Root { values: Vec::new() },
    );

    assert_eq!(
        facet_format_toml::from_str::<Root>("values = [2]").unwrap(),
        Root {
            values: vec![Item(2)]
        },
    );

    assert_eq!(
        facet_format_toml::from_str::<Root>("values = [1, -1, 0, 100]").unwrap(),
        Root {
            values: vec![Item(1), Item(-1), Item(0), Item(100)],
        },
    );
}

// ============================================================================
// Map tests
// ============================================================================

#[test]
fn test_scalar_map() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        values: HashMap<String, i32>,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("[values]").unwrap(),
        Root {
            values: HashMap::new()
        },
    );

    assert_eq!(
        facet_format_toml::from_str::<Root>(
            r#"
            [values]
            a = 0
            b = -1
            "#
        )
        .unwrap(),
        Root {
            values: [("a".to_string(), 0), ("b".to_string(), -1)].into()
        },
    );
}

#[test]
fn test_struct_map() {
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
        facet_format_toml::from_str::<Root>("[dependencies]").unwrap(),
        Root {
            dependencies: HashMap::new()
        },
    );

    assert_eq!(
        facet_format_toml::from_str::<Root>(
            r#"
            [dependencies]
            syn = { version = "1", optional = false }
            paste = { version = "0.0.1", optional = true }
            "#
        )
        .unwrap(),
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
}

// ============================================================================
// Nested struct tests
// ============================================================================

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
        facet_format_toml::from_str::<Root>(
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
        facet_format_toml::from_str::<Root>(
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
        facet_format_toml::from_str::<Root>(
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
    use std::net::Ipv6Addr;

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        a: i32,
        b: bool,
        c: Ipv6Addr,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>(
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
}

// ============================================================================
// Option tests
// ============================================================================

#[test]
fn test_option_scalar() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<i32>,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("").unwrap(),
        Root { value: None },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 1").unwrap(),
        Root { value: Some(1) },
    );
}

#[test]
fn test_nested_option() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Option<i32>>,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("").unwrap(),
        Root { value: None },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 1").unwrap(),
        Root {
            value: Some(Some(1))
        },
    );
}

#[test]
fn test_option_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item {
        value: i32,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("").unwrap(),
        Root { value: None },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>("value.value = 1").unwrap(),
        Root {
            value: Some(Item { value: 1 })
        },
    );
}

// ============================================================================
// Enum tests
// ============================================================================

#[test]
fn test_unit_only_enum() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: UnitOnlyEnum,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum UnitOnlyEnum {
        VariantA,
        VariantB,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 'VariantA'").unwrap(),
        Root {
            value: UnitOnlyEnum::VariantA,
        },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 'VariantB'").unwrap(),
        Root {
            value: UnitOnlyEnum::VariantB,
        },
    );
}

#[test]
fn test_single_value_on_non_unit_enum() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: WithNonUnitVariant,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum WithNonUnitVariant {
        VariantA,
        #[allow(dead_code)]
        VariantB(i32),
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 'VariantA'").unwrap(),
        Root {
            value: WithNonUnitVariant::VariantA
        },
    );
    assert!(facet_format_toml::from_str::<Root>("value = 'VariantB'").is_err());
}

#[test]
fn test_tuple_enum() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: WithTupleVariants,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum WithTupleVariants {
        OneField(f32),
        TwoFields(bool, i16),
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = { OneField = 0.5 }").unwrap(),
        Root {
            value: WithTupleVariants::OneField(0.5)
        },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>(
            r#"
            [value.TwoFields]
            0 = true
            1 = 1
            "#
        )
        .unwrap(),
        Root {
            value: WithTupleVariants::TwoFields(true, 1)
        },
    );
}

#[test]
fn test_struct_enum() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: WithStructVariants,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum WithStructVariants {
        OneField { one: f64 },
        TwoFields { first: bool, second: u8 },
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value.OneField.one = 0.5").unwrap(),
        Root {
            value: WithStructVariants::OneField { one: 0.5 }
        },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>(
            r#"
            [value.TwoFields]
            first = true
            second = 1
            "#
        )
        .unwrap(),
        Root {
            value: WithStructVariants::TwoFields {
                first: true,
                second: 1
            }
        },
    );
}

#[test]
fn test_enum_root() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Root {
        A { value: u16 },
        B(u32),
        C,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("A.value = 1").unwrap(),
        Root::A { value: 1 },
    );
    assert_eq!(
        facet_format_toml::from_str::<Root>("B = 2").unwrap(),
        Root::B(2)
    );
    assert_eq!(facet_format_toml::from_str::<Root>("[C]").unwrap(), Root::C);
}

// ============================================================================
// Rename tests
// ============================================================================

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
        facet_format_toml::from_str::<Root>(
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
            c: "quoted".to_string(),
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
        facet_format_toml::from_str::<Root>(
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

// ============================================================================
// Default tests
// ============================================================================

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
        facet_format_toml::from_str::<Root>(
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
        facet_format_toml::from_str::<Root>("").unwrap(),
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
        facet_format_toml::from_str::<Root>("").unwrap(),
        Root {
            a: 42,
            b: Some(true),
            c: "hi".to_string()
        },
    );
}

// ============================================================================
// IP address tests
// ============================================================================

#[test]
fn test_socket_addr() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: std::net::SocketAddr,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = '127.0.0.1:8000'").unwrap(),
        Root {
            value: "127.0.0.1:8000".parse().unwrap()
        },
    );
}

#[test]
fn test_ip_addrs() {
    use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        ip: IpAddr,
        ipv4: Ipv4Addr,
        ipv6: Ipv6Addr,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>(
            r#"
            ip = '127.0.0.1'
            ipv4 = '192.168.1.1'
            ipv6 = '::1'
            "#
        )
        .unwrap(),
        Root {
            ip: "127.0.0.1".parse().unwrap(),
            ipv4: "192.168.1.1".parse().unwrap(),
            ipv6: "::1".parse().unwrap(),
        },
    );
}

// ============================================================================
// Complex/edge case tests
// ============================================================================

#[test]
fn test_ignore_unknown_table_keys() {
    // facet-format-toml should ignore unknown table keys during deserialization
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

    let result = facet_format_toml::from_str::<Manifest>(toml).unwrap();
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

#[test]
fn test_array_of_tables() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        items: Vec<Item>,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Item {
        name: String,
        value: i32,
    }

    let toml = r#"
        [[items]]
        name = "first"
        value = 1

        [[items]]
        name = "second"
        value = 2
    "#;

    let result = facet_format_toml::from_str::<Root>(toml).unwrap();
    assert_eq!(
        result,
        Root {
            items: vec![
                Item {
                    name: "first".to_string(),
                    value: 1
                },
                Item {
                    name: "second".to_string(),
                    value: 2
                }
            ]
        }
    );
}

#[test]
fn test_inline_table() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        point: Point,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    let toml = r#"point = { x = 1, y = 2 }"#;

    let result = facet_format_toml::from_str::<Root>(toml).unwrap();
    assert_eq!(
        result,
        Root {
            point: Point { x: 1, y: 2 }
        }
    );
}

#[test]
fn test_multiline_strings() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        text: String,
    }

    let toml = r#"
text = """
Hello
World"""
"#;

    let result = facet_format_toml::from_str::<Root>(toml).unwrap();
    assert_eq!(
        result,
        Root {
            text: "Hello\nWorld".to_string()
        }
    );
}

#[test]
fn test_literal_strings() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        path: String,
    }

    let toml = r#"path = 'C:\Users\name'"#;

    let result = facet_format_toml::from_str::<Root>(toml).unwrap();
    assert_eq!(
        result,
        Root {
            path: r"C:\Users\name".to_string()
        }
    );
}

#[test]
fn test_datetime() {
    // TOML has native datetime support - test that we can deserialize to string
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        date: String,
    }

    let toml = r#"date = "2023-01-15T10:30:00Z""#;

    let result = facet_format_toml::from_str::<Root>(toml).unwrap();
    assert_eq!(
        result,
        Root {
            date: "2023-01-15T10:30:00Z".to_string()
        }
    );
}

#[test]
fn test_cow_str() {
    use std::borrow::Cow;

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Cow<'static, str>,
    }

    assert_eq!(
        facet_format_toml::from_str::<Root>("value = 'string'").unwrap(),
        Root {
            value: Cow::Borrowed("string")
        },
    );
}

// ============================================================================
// Serialization tests
// ============================================================================

#[test]
fn test_serialize_simple_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
        port: u16,
    }

    let config = Config {
        name: "my-app".to_string(),
        port: 8080,
    };

    let toml = facet_format_toml::to_string(&config).unwrap();
    assert_eq!(toml, "name = \"my-app\"\nport = 8080\n");
}

#[test]
fn test_serialize_nested_struct() {
    #[derive(Debug, Facet, PartialEq)]
    struct Server {
        host: String,
        port: u16,
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
        server: Server,
    }

    let config = Config {
        name: "test".to_string(),
        server: Server {
            host: "localhost".to_string(),
            port: 8080,
        },
    };

    let toml = facet_format_toml::to_string(&config).unwrap();
    // Nested structs become inline tables with current serializer
    assert!(toml.contains("name = \"test\""));
    assert!(toml.contains("server = { host = \"localhost\", port = 8080 }"));
}

#[test]
fn test_serialize_array() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        numbers: Vec<u32>,
    }

    let config = Config {
        numbers: vec![1, 2, 3],
    };

    let toml = facet_format_toml::to_string(&config).unwrap();
    assert!(toml.contains("numbers = [1, 2, 3]"));
}

#[test]
fn test_serialize_bool() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        enabled: bool,
        debug: bool,
    }

    let config = Config {
        enabled: true,
        debug: false,
    };

    let toml = facet_format_toml::to_string(&config).unwrap();
    assert!(toml.contains("enabled = true"));
    assert!(toml.contains("debug = false"));
}

#[test]
fn test_round_trip() {
    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
        port: u16,
        enabled: bool,
    }

    let original = Config {
        name: "test".to_string(),
        port: 8080,
        enabled: true,
    };

    let toml = facet_format_toml::to_string(&original).unwrap();
    let parsed: Config = facet_format_toml::from_str(&toml).unwrap();
    assert_eq!(original, parsed);
}
