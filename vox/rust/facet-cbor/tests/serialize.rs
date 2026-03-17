use facet::Facet;
use facet_cbor::to_vec;
use std::collections::HashMap;

// =============================================================================
// Primitive scalars
// =============================================================================

#[test]
fn test_u32() {
    // 42 → major 0, value 42 (0x18 0x2a)
    let bytes = to_vec(&42u32).unwrap();
    assert_eq!(bytes, vec![0x18, 0x2a]);
}

#[test]
fn test_u32_small() {
    // 0 → 0x00
    assert_eq!(to_vec(&0u32).unwrap(), vec![0x00]);
    // 23 → 0x17
    assert_eq!(to_vec(&23u32).unwrap(), vec![0x17]);
    // 24 → 0x18, 0x18
    assert_eq!(to_vec(&24u32).unwrap(), vec![0x18, 0x18]);
}

#[test]
fn test_u64_large() {
    // 1_000_000 = 0x000F4240 → major 0, additional 26 (4 bytes), big-endian
    let bytes = to_vec(&1_000_000u64).unwrap();
    assert_eq!(bytes, vec![0x1a, 0x00, 0x0f, 0x42, 0x40]);
}

#[test]
fn test_i64_positive() {
    // 100 → major 0, value 100 (0x18, 0x64)
    let bytes = to_vec(&100i64).unwrap();
    assert_eq!(bytes, vec![0x18, 0x64]);
}

#[test]
fn test_i64_negative() {
    // -1 → major 1, value 0 (0x20)
    assert_eq!(to_vec(&-1i64).unwrap(), vec![0x20]);
    // -10 → major 1, value 9 (0x29)
    assert_eq!(to_vec(&-10i64).unwrap(), vec![0x29]);
    // -100 → major 1, value 99 (0x38, 0x63)
    assert_eq!(to_vec(&-100i64).unwrap(), vec![0x38, 0x63]);
}

#[test]
fn test_string() {
    // "hello" → major 3, length 5, then UTF-8 bytes
    let bytes = to_vec(&String::from("hello")).unwrap();
    assert_eq!(bytes, vec![0x65, b'h', b'e', b'l', b'l', b'o']);
}

#[test]
fn test_empty_string() {
    let bytes = to_vec(&String::from("")).unwrap();
    assert_eq!(bytes, vec![0x60]); // major 3, length 0
}

#[test]
fn test_bool() {
    assert_eq!(to_vec(&true).unwrap(), vec![0xf5]);
    assert_eq!(to_vec(&false).unwrap(), vec![0xf4]);
}

#[test]
fn test_f64() {
    // 1.0 as f64 → 0xfb + 8 bytes big-endian IEEE 754
    let bytes = to_vec(&1.0f64).unwrap();
    assert_eq!(bytes.len(), 9);
    assert_eq!(bytes[0], 0xfb);
    assert_eq!(&bytes[1..], &1.0f64.to_be_bytes());
}

#[test]
fn test_f32() {
    let bytes = to_vec(&1.0f32).unwrap();
    assert_eq!(bytes.len(), 5);
    assert_eq!(bytes[0], 0xfa);
    assert_eq!(&bytes[1..], &1.0f32.to_be_bytes());
}

#[test]
fn test_unit() {
    assert_eq!(to_vec(&()).unwrap(), vec![0xf6]); // null
}

// =============================================================================
// Structs
// =============================================================================

#[derive(Facet)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn test_struct() {
    let p = Point { x: 1, y: 2 };
    let bytes = to_vec(&p).unwrap();
    // map(2) { "x": 1, "y": 2 }
    let mut expected = Vec::new();
    expected.push(0xa2); // map of 2 items
    // "x"
    expected.push(0x61); // text(1)
    expected.push(b'x');
    // 1
    expected.push(0x01);
    // "y"
    expected.push(0x61); // text(1)
    expected.push(b'y');
    // 2
    expected.push(0x02);
    assert_eq!(bytes, expected);
}

#[derive(Facet)]
struct Nested {
    name: String,
    point: Point,
}

#[test]
fn test_nested_struct() {
    let n = Nested {
        name: String::from("A"),
        point: Point { x: 10, y: 20 },
    };
    let bytes = to_vec(&n).unwrap();
    // map(2) { "name": "A", "point": map(2) { "x": 10, "y": 20 } }
    let mut expected = Vec::new();
    expected.push(0xa2); // map(2)
    // "name"
    expected.extend_from_slice(&[0x64, b'n', b'a', b'm', b'e']);
    // "A"
    expected.extend_from_slice(&[0x61, b'A']);
    // "point"
    expected.extend_from_slice(&[0x65, b'p', b'o', b'i', b'n', b't']);
    // map(2) { "x": 10, "y": 20 }
    expected.push(0xa2);
    expected.extend_from_slice(&[0x61, b'x']);
    expected.push(0x0a); // 10
    expected.extend_from_slice(&[0x61, b'y']);
    expected.push(0x14); // 20
    assert_eq!(bytes, expected);
}

// =============================================================================
// Enums
// =============================================================================

#[derive(Facet)]
#[repr(u8)]
enum Color {
    Red,
    Green,
    Blue,
}

#[test]
fn test_unit_variant() {
    let bytes = to_vec(&Color::Red).unwrap();
    // map(1) { "Red": null }
    let mut expected = Vec::new();
    expected.push(0xa1); // map(1)
    expected.extend_from_slice(&[0x63, b'R', b'e', b'd']); // "Red"
    expected.push(0xf6); // null
    assert_eq!(bytes, expected);
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}

#[test]
fn test_struct_variant() {
    let s = Shape::Circle { radius: 1.5 };
    let bytes = to_vec(&s).unwrap();
    // map(1) { "Circle": map(1) { "radius": 1.5 } }
    let mut expected = Vec::new();
    expected.push(0xa1); // map(1)
    expected.extend_from_slice(&[0x66, b'C', b'i', b'r', b'c', b'l', b'e']); // "Circle"
    expected.push(0xa1); // map(1)
    expected.extend_from_slice(&[0x66, b'r', b'a', b'd', b'i', b'u', b's']); // "radius"
    expected.push(0xfb); // float64
    expected.extend_from_slice(&1.5f64.to_be_bytes());
    assert_eq!(bytes, expected);
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Value {
    Num(i64),
    Text(String),
}

#[test]
fn test_newtype_variant() {
    let v = Value::Num(42);
    let bytes = to_vec(&v).unwrap();
    // map(1) { "Num": 42 }
    let mut expected = Vec::new();
    expected.push(0xa1);
    expected.extend_from_slice(&[0x63, b'N', b'u', b'm']); // "Num"
    expected.extend_from_slice(&[0x18, 0x2a]); // 42
    assert_eq!(bytes, expected);
}

// =============================================================================
// Containers
// =============================================================================

#[test]
fn test_vec() {
    let v = vec![1u32, 2, 3];
    let bytes = to_vec(&v).unwrap();
    // array(3) [1, 2, 3]
    assert_eq!(bytes, vec![0x83, 0x01, 0x02, 0x03]);
}

#[test]
fn test_vec_u8_as_bytes() {
    let v: Vec<u8> = vec![0xde, 0xad, 0xbe, 0xef];
    let bytes = to_vec(&v).unwrap();
    // byte string(4)
    assert_eq!(bytes, vec![0x44, 0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn test_option_some() {
    let v: Option<u32> = Some(42);
    let bytes = to_vec(&v).unwrap();
    // Just the inner value: 42
    assert_eq!(bytes, vec![0x18, 0x2a]);
}

#[test]
fn test_option_none() {
    let v: Option<u32> = None;
    let bytes = to_vec(&v).unwrap();
    // null
    assert_eq!(bytes, vec![0xf6]);
}

#[test]
fn test_hashmap() {
    let mut m = HashMap::new();
    m.insert(String::from("a"), 1u32);
    let bytes = to_vec(&m).unwrap();
    // map(1) { "a": 1 }
    let mut expected = Vec::new();
    expected.push(0xa1); // map(1)
    expected.extend_from_slice(&[0x61, b'a']); // "a"
    expected.push(0x01); // 1
    assert_eq!(bytes, expected);
}

// =============================================================================
// Deterministic encoding
// =============================================================================

#[test]
fn test_deterministic() {
    let p = Point { x: 42, y: -7 };
    let bytes1 = to_vec(&p).unwrap();
    let bytes2 = to_vec(&p).unwrap();
    assert_eq!(
        bytes1, bytes2,
        "same input must produce identical CBOR bytes"
    );
}

// =============================================================================
// Negative integer encoding
// =============================================================================

#[test]
fn test_negative_integers_use_major_1() {
    // -1 → 0x20 (major 1, additional 0)
    let bytes = to_vec(&-1i32).unwrap();
    assert_eq!(bytes[0] >> 5, 1, "first byte should have major type 1");
    assert_eq!(bytes, vec![0x20]);

    // -24 → 0x37 (major 1, additional 23)
    let bytes = to_vec(&-24i32).unwrap();
    assert_eq!(bytes, vec![0x37]);

    // -25 → 0x38, 0x18 (major 1, additional 24, value 24)
    let bytes = to_vec(&-25i32).unwrap();
    assert_eq!(bytes, vec![0x38, 0x18]);
}
