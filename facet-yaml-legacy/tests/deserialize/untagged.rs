//! Tests for untagged enum deserialization.

use facet::Facet;
use facet_solver::VariantsByFormat;
use facet_testhelpers::test;

// ===========================================================================
// Solver VariantsByFormat Detection Tests
// ===========================================================================

#[test]
fn test_variants_by_format_scalar_detection() {
    // Verify that scalar variants are properly detected
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum NumericValue {
        Small(u8),
        Medium(u16),
        Large(u32),
    }

    let vbf = VariantsByFormat::from_shape(NumericValue::SHAPE).unwrap();
    assert_eq!(
        vbf.scalar_variants.len(),
        3,
        "Should have 3 scalar variants"
    );
}

#[test]
fn test_variants_by_format_tuple_detection() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum Point {
        TwoD(f64, f64),
        ThreeD(f64, f64, f64),
    }

    let vbf = VariantsByFormat::from_shape(Point::SHAPE).unwrap();
    assert_eq!(vbf.tuple_variants.len(), 2, "Should have 2 tuple variants");
}

// ===========================================================================
// Struct Variants (existing tests)
// ===========================================================================

// Test case from the original issue: tuple variants with sequences
// Note: Tuple variants for untagged enums require sequence format in YAML
// This differs from struct variants which use mapping format
#[test]
fn test_untagged_tuple_variants_issue_1183() {
    // Struct-based equivalent that works:
    #[derive(Debug, Facet, PartialEq, Clone)]
    #[facet(untagged)]
    #[repr(C)]
    #[allow(dead_code)]
    pub enum CntStruct {
        Unit { value: String },
        Weight { value: String, weight: f64 },
    }

    // Unit variant
    let unit: CntStruct = facet_yaml_legacy::from_str("value: sugar").unwrap();
    assert_eq!(
        unit,
        CntStruct::Unit {
            value: "sugar".to_string()
        }
    );

    // Weight variant
    let weight: CntStruct = facet_yaml_legacy::from_str("value: flour\nweight: 2.5").unwrap();
    assert_eq!(
        weight,
        CntStruct::Weight {
            value: "flour".to_string(),
            weight: 2.5
        }
    );
}

#[test]
fn test_untagged_struct_variants() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    // Test Circle variant
    let circle: Shape = facet_yaml_legacy::from_str("radius: 5.0").unwrap();
    assert_eq!(circle, Shape::Circle { radius: 5.0 });

    // Test Rectangle variant
    let rect: Shape = facet_yaml_legacy::from_str("width: 10.0\nheight: 20.0").unwrap();
    assert_eq!(
        rect,
        Shape::Rectangle {
            width: 10.0,
            height: 20.0
        }
    );
}

#[test]
fn test_untagged_discriminating_field() {
    // Test case from the issue: distinguishing variants by field names
    #[derive(Debug, Facet, PartialEq, Clone)]
    #[facet(untagged)]
    #[repr(C)]
    #[allow(dead_code)]
    pub enum Cnt {
        Unit { name: String },
        Weight { name: String, weight: f64 },
    }

    // Unit variant (has only "name")
    let unit: Cnt = facet_yaml_legacy::from_str("name: sugar").unwrap();
    assert_eq!(
        unit,
        Cnt::Unit {
            name: "sugar".to_string()
        }
    );

    // Weight variant (has "name" and "weight")
    let weight: Cnt = facet_yaml_legacy::from_str("name: flour\nweight: 2.5").unwrap();
    assert_eq!(
        weight,
        Cnt::Weight {
            name: "flour".to_string(),
            weight: 2.5
        }
    );
}

#[test]
fn test_untagged_in_struct() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Value {
        Text { text: String },
        Number { num: i32 },
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Container {
        name: String,
        value: Value,
    }

    let yaml = r#"
name: test
value:
  text: hello
"#;
    let container: Container = facet_yaml_legacy::from_str(yaml).unwrap();
    assert_eq!(container.name, "test");
    assert_eq!(
        container.value,
        Value::Text {
            text: "hello".to_string()
        }
    );

    let yaml2 = r#"
name: test2
value:
  num: 42
"#;
    let container2: Container = facet_yaml_legacy::from_str(yaml2).unwrap();
    assert_eq!(container2.name, "test2");
    assert_eq!(container2.value, Value::Number { num: 42 });
}

#[test]
fn test_untagged_list_of_variants() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Item {
        Simple { id: u32 },
        Complex { id: u32, data: String },
    }

    let yaml = r#"
- id: 1
- id: 2
  data: extra
- id: 3
"#;
    let items: Vec<Item> = facet_yaml_legacy::from_str(yaml).unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0], Item::Simple { id: 1 });
    assert_eq!(
        items[1],
        Item::Complex {
            id: 2,
            data: "extra".to_string()
        }
    );
    assert_eq!(items[2], Item::Simple { id: 3 });
}

// ===========================================================================
// Scalar (Newtype) Variants - Issue #1186
// ===========================================================================

#[test]
fn test_untagged_scalar_string_vs_int() {
    // Basic case: distinguish string from integer
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum StringOrInt {
        Int(i64),
        Str(String),
    }

    // Integer value
    let int_val: StringOrInt = facet_yaml_legacy::from_str("42").unwrap();
    assert_eq!(int_val, StringOrInt::Int(42));

    // Negative integer
    let neg_val: StringOrInt = facet_yaml_legacy::from_str("-100").unwrap();
    assert_eq!(neg_val, StringOrInt::Int(-100));

    // String value
    let str_val: StringOrInt = facet_yaml_legacy::from_str("hello").unwrap();
    assert_eq!(str_val, StringOrInt::Str("hello".to_string()));

    // Quoted string that looks like a number
    let quoted: StringOrInt = facet_yaml_legacy::from_str("\"42\"").unwrap();
    assert_eq!(quoted, StringOrInt::Str("42".to_string()));
}

#[test]
fn test_untagged_scalar_numeric_specificity() {
    // Test that smaller numeric types are preferred when the value fits
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum NumericValue {
        Small(u8),
        Medium(u16),
        Large(u32),
    }

    // Small value fits in u8
    let small: NumericValue = facet_yaml_legacy::from_str("42").unwrap();
    assert_eq!(small, NumericValue::Small(42));

    // Value too large for u8 but fits in u16
    let medium: NumericValue = facet_yaml_legacy::from_str("1000").unwrap();
    assert_eq!(medium, NumericValue::Medium(1000));

    // Value too large for u16 but fits in u32
    let large: NumericValue = facet_yaml_legacy::from_str("100000").unwrap();
    assert_eq!(large, NumericValue::Large(100000));
}

#[test]
fn test_untagged_scalar_signed_vs_unsigned() {
    // Test signed vs unsigned discrimination
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum SignedOrUnsigned {
        Unsigned(u32),
        Signed(i32),
    }

    // Positive value goes to unsigned (more specific for positive)
    let pos: SignedOrUnsigned = facet_yaml_legacy::from_str("100").unwrap();
    assert_eq!(pos, SignedOrUnsigned::Unsigned(100));

    // Negative value must go to signed
    let neg: SignedOrUnsigned = facet_yaml_legacy::from_str("-100").unwrap();
    assert_eq!(neg, SignedOrUnsigned::Signed(-100));
}

#[test]
fn test_untagged_scalar_float_vs_int() {
    // Test float vs integer discrimination
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum FloatOrInt {
        Int(i32),
        Float(f64),
    }

    // Integer value
    let int_val: FloatOrInt = facet_yaml_legacy::from_str("42").unwrap();
    assert_eq!(int_val, FloatOrInt::Int(42));

    // Float value (has decimal point)
    let float_val: FloatOrInt = facet_yaml_legacy::from_str("3.5").unwrap();
    assert_eq!(float_val, FloatOrInt::Float(3.5));
}

#[test]
fn test_untagged_scalar_bool() {
    // Test boolean discrimination with all YAML boolean spellings
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum BoolOrString {
        Bool(bool),
        Str(String),
    }

    // Boolean true - various YAML spellings
    let t: BoolOrString = facet_yaml_legacy::from_str("true").unwrap();
    assert_eq!(t, BoolOrString::Bool(true));

    let yes: BoolOrString = facet_yaml_legacy::from_str("yes").unwrap();
    assert_eq!(yes, BoolOrString::Bool(true));

    let on: BoolOrString = facet_yaml_legacy::from_str("on").unwrap();
    assert_eq!(on, BoolOrString::Bool(true));

    let y: BoolOrString = facet_yaml_legacy::from_str("y").unwrap();
    assert_eq!(y, BoolOrString::Bool(true));

    // Boolean false - various YAML spellings
    let f: BoolOrString = facet_yaml_legacy::from_str("false").unwrap();
    assert_eq!(f, BoolOrString::Bool(false));

    let no: BoolOrString = facet_yaml_legacy::from_str("no").unwrap();
    assert_eq!(no, BoolOrString::Bool(false));

    let off: BoolOrString = facet_yaml_legacy::from_str("off").unwrap();
    assert_eq!(off, BoolOrString::Bool(false));

    let n: BoolOrString = facet_yaml_legacy::from_str("n").unwrap();
    assert_eq!(n, BoolOrString::Bool(false));

    // Regular string
    let s: BoolOrString = facet_yaml_legacy::from_str("hello").unwrap();
    assert_eq!(s, BoolOrString::Str("hello".to_string()));
}

#[test]
fn test_untagged_scalar_in_struct() {
    // Test scalar variants nested in a struct
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum Value {
        Text(String),
        Number(i32),
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Config {
        name: String,
        value: Value,
    }

    let yaml = r#"
name: test
value: 42
"#;
    let config: Config = facet_yaml_legacy::from_str(yaml).unwrap();
    assert_eq!(config.name, "test");
    assert_eq!(config.value, Value::Number(42));

    let yaml2 = r#"
name: test2
value: hello
"#;
    let config2: Config = facet_yaml_legacy::from_str(yaml2).unwrap();
    assert_eq!(config2.name, "test2");
    assert_eq!(config2.value, Value::Text("hello".to_string()));
}

#[test]
fn test_untagged_scalar_list() {
    // Test list of scalar variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum Item {
        Num(i32),
        Text(String),
    }

    let yaml = r#"
- 1
- hello
- 2
- world
"#;
    let items: Vec<Item> = facet_yaml_legacy::from_str(yaml).unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(items[0], Item::Num(1));
    assert_eq!(items[1], Item::Text("hello".to_string()));
    assert_eq!(items[2], Item::Num(2));
    assert_eq!(items[3], Item::Text("world".to_string()));
}

// ===========================================================================
// Tuple Variants - Issue #1186
// ===========================================================================

#[test]
fn test_untagged_tuple_basic() {
    // Basic tuple variant with sequence
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Point {
        Point2D(f64, f64),
    }

    let point: Point = facet_yaml_legacy::from_str("[1.0, 2.0]").unwrap();
    assert_eq!(point, Point::Point2D(1.0, 2.0));
}

#[test]
fn test_untagged_tuple_different_arities() {
    // Tuple variants with different arities
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum Data {
        Single(i32),
        Pair(i32, i32),
        Triple(i32, i32, i32),
    }

    // Single element (arity 1) - this is actually a newtype, not a tuple
    // In untagged mode, newtype scalars are direct values, not sequences
    // So [42] would match Pair/Triple arities, not Single

    // Pair (arity 2)
    let pair: Data = facet_yaml_legacy::from_str("[1, 2]").unwrap();
    assert_eq!(pair, Data::Pair(1, 2));

    // Triple (arity 3)
    let triple: Data = facet_yaml_legacy::from_str("[1, 2, 3]").unwrap();
    assert_eq!(triple, Data::Triple(1, 2, 3));
}

#[test]
fn test_untagged_tuple_color() {
    // RGB color as tuple variant
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Color {
        Rgb(u8, u8, u8),
    }

    let red: Color = facet_yaml_legacy::from_str("[255, 0, 0]").unwrap();
    assert_eq!(red, Color::Rgb(255, 0, 0));

    let white: Color = facet_yaml_legacy::from_str("[255, 255, 255]").unwrap();
    assert_eq!(white, Color::Rgb(255, 255, 255));
}

#[test]
fn test_untagged_tuple_in_struct() {
    // Tuple variants nested in a struct
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Coord {
        XY(f64, f64),
    }

    #[derive(Debug, Facet, PartialEq)]
    struct Location {
        name: String,
        position: Coord,
    }

    let yaml = r#"
name: origin
position: [0.0, 0.0]
"#;
    let loc: Location = facet_yaml_legacy::from_str(yaml).unwrap();
    assert_eq!(loc.name, "origin");
    assert_eq!(loc.position, Coord::XY(0.0, 0.0));
}

#[test]
fn test_untagged_tuple_list() {
    // List of tuple variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    #[allow(dead_code)]
    enum Point {
        XY(i32, i32),
    }

    let yaml = r#"
- [0, 0]
- [1, 2]
- [3, 4]
"#;
    let points: Vec<Point> = facet_yaml_legacy::from_str(yaml).unwrap();
    assert_eq!(points.len(), 3);
    assert_eq!(points[0], Point::XY(0, 0));
    assert_eq!(points[1], Point::XY(1, 2));
    assert_eq!(points[2], Point::XY(3, 4));
}

#[test]
fn test_untagged_newtype_tuple_variant_issue_1189() {
    // Regression test for tuple values nested inside a newtype variant
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum Counter {
        Unit(String),
        Weight((String, f64)),
    }

    let yaml = r#"
- [AGRICIBPAR, 1.0]
- [BARCLAYLDN, 2.5]
"#;

    let counters: Vec<Counter> = facet_yaml_legacy::from_str(yaml).unwrap();
    assert_eq!(
        counters,
        vec![
            Counter::Weight(("AGRICIBPAR".to_string(), 1.0)),
            Counter::Weight(("BARCLAYLDN".to_string(), 2.5)),
        ]
    );
}

// ===========================================================================
// Multiple String-Parseable Types (Trial Parsing)
// ===========================================================================

#[test]
fn test_untagged_scalar_multiple_string_types() {
    use std::net::IpAddr;

    // Multiple variants that all parse from strings, but with different validity
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    enum NetworkOrText {
        // IpAddr parses from string but only accepts valid IP addresses
        Ip(IpAddr),
        // String accepts anything
        Text(String),
    }

    // Valid IP address -> should pick Ip variant
    let ip: NetworkOrText = facet_yaml_legacy::from_str("192.168.1.1").unwrap();
    assert_eq!(ip, NetworkOrText::Ip("192.168.1.1".parse().unwrap()));

    // Another valid IP
    let ip6: NetworkOrText = facet_yaml_legacy::from_str("::1").unwrap();
    assert_eq!(ip6, NetworkOrText::Ip("::1".parse().unwrap()));

    // Invalid IP address -> should fall back to Text variant
    let text: NetworkOrText = facet_yaml_legacy::from_str("hello world").unwrap();
    assert_eq!(text, NetworkOrText::Text("hello world".to_string()));

    // Something that looks like IP but isn't valid
    let not_ip: NetworkOrText = facet_yaml_legacy::from_str("999.999.999.999").unwrap();
    assert_eq!(not_ip, NetworkOrText::Text("999.999.999.999".to_string()));
}

#[test]
fn test_untagged_scalar_multiple_string_types_ordering() {
    use std::net::Ipv4Addr;

    // Test that more specific types are tried before String
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    enum Value {
        // String is first in declaration order, but should be tried last
        Text(String),
        // Ipv4Addr is more specific
        Ip(Ipv4Addr),
    }

    // Valid IP should still pick Ip even though Text is declared first
    let ip: Value = facet_yaml_legacy::from_str("10.0.0.1").unwrap();
    assert_eq!(ip, Value::Ip("10.0.0.1".parse().unwrap()));

    // Invalid IP falls back to Text
    let text: Value = facet_yaml_legacy::from_str("not-an-ip").unwrap();
    assert_eq!(text, Value::Text("not-an-ip".to_string()));
}

// ===========================================================================
// Mixed Variants (Scalar + Struct + Tuple)
// ===========================================================================

#[test]
fn test_untagged_mixed_scalar_and_struct() {
    // Mix of scalar and struct variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[facet(untagged)]
    enum Config {
        Simple(String),
        Detailed { host: String, port: u16 },
    }

    // Simple string value
    let simple: Config = facet_yaml_legacy::from_str("localhost:8080").unwrap();
    assert_eq!(simple, Config::Simple("localhost:8080".to_string()));

    // Detailed struct value
    let detailed: Config = facet_yaml_legacy::from_str("host: localhost\nport: 8080").unwrap();
    assert_eq!(
        detailed,
        Config::Detailed {
            host: "localhost".to_string(),
            port: 8080
        }
    );
}
