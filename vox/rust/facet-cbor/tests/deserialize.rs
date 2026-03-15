use facet::Facet;
use facet_cbor::{from_slice, to_vec};
use std::collections::HashMap;

// =============================================================================
// Round-trip helpers
// =============================================================================

fn round_trip<T: Facet<'static> + std::fmt::Debug + PartialEq>(value: &T) {
    let bytes = to_vec(value).unwrap();
    let decoded: T = from_slice(&bytes).unwrap();
    assert_eq!(&decoded, value);
}

// =============================================================================
// Primitive scalars
// =============================================================================

#[test]
fn test_u32_round_trip() {
    round_trip(&0u32);
    round_trip(&23u32);
    round_trip(&24u32);
    round_trip(&255u32);
    round_trip(&256u32);
    round_trip(&65535u32);
    round_trip(&65536u32);
    round_trip(&42u32);
    round_trip(&u32::MAX);
}

#[test]
fn test_u64_round_trip() {
    round_trip(&0u64);
    round_trip(&1_000_000u64);
    round_trip(&u64::MAX);
}

#[test]
fn test_i64_round_trip() {
    round_trip(&0i64);
    round_trip(&42i64);
    round_trip(&-1i64);
    round_trip(&-100i64);
    round_trip(&i64::MIN);
    round_trip(&i64::MAX);
}

#[test]
fn test_i32_round_trip() {
    round_trip(&0i32);
    round_trip(&-1i32);
    round_trip(&i32::MIN);
    round_trip(&i32::MAX);
}

#[test]
fn test_negative_integers() {
    round_trip(&-1i8);
    round_trip(&-10i16);
    round_trip(&-1000i32);
    round_trip(&-1_000_000i64);
}

#[test]
fn test_bool_round_trip() {
    round_trip(&true);
    round_trip(&false);
}

#[test]
fn test_f64_round_trip() {
    round_trip(&0.0f64);
    round_trip(&3.14159f64);
    round_trip(&-1.5f64);
    round_trip(&f64::INFINITY);
    round_trip(&f64::NEG_INFINITY);
}

#[test]
fn test_f32_round_trip() {
    round_trip(&0.0f32);
    round_trip(&3.14f32);
    round_trip(&-1.5f32);
}

#[test]
fn test_string_round_trip() {
    round_trip(&String::new());
    round_trip(&"hello".to_string());
    round_trip(&"hello world 🌍".to_string());
}

// =============================================================================
// Structs with named fields
// =============================================================================

#[derive(Facet, Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn test_struct_round_trip() {
    round_trip(&Point { x: 10, y: 20 });
    round_trip(&Point { x: -5, y: 0 });
}

// =============================================================================
// Nested structs
// =============================================================================

#[derive(Facet, Debug, PartialEq)]
struct Line {
    start: Point,
    end: Point,
}

#[test]
fn test_nested_struct_round_trip() {
    round_trip(&Line {
        start: Point { x: 0, y: 0 },
        end: Point { x: 10, y: 20 },
    });
}

// =============================================================================
// Enums
// =============================================================================

#[derive(Facet, Debug, PartialEq)]
enum Shape {
    Circle(f64),
    Rectangle { width: f64, height: f64 },
    Nothing,
}

#[test]
fn test_enum_unit_variant() {
    round_trip(&Shape::Nothing);
}

#[test]
fn test_enum_newtype_variant() {
    round_trip(&Shape::Circle(5.0));
}

#[test]
fn test_enum_struct_variant() {
    round_trip(&Shape::Rectangle {
        width: 10.0,
        height: 20.0,
    });
}

// =============================================================================
// Vec<T> and Vec<u8>
// =============================================================================

#[test]
fn test_vec_i32_round_trip() {
    round_trip(&vec![1i32, 2, 3, 4, 5]);
}

#[test]
fn test_vec_string_round_trip() {
    round_trip(&vec!["hello".to_string(), "world".to_string()]);
}

#[test]
fn test_vec_u8_round_trip() {
    // Vec<u8> is serialized as a CBOR byte string
    round_trip(&vec![0u8, 1, 2, 255]);
}

#[test]
fn test_empty_vec() {
    round_trip(&Vec::<i32>::new());
    round_trip(&Vec::<u8>::new());
}

// =============================================================================
// Option<T>
// =============================================================================

#[test]
fn test_option_some() {
    round_trip(&Some(42i32));
    round_trip(&Some("hello".to_string()));
}

#[test]
fn test_option_none() {
    round_trip(&Option::<i32>::None);
    round_trip(&Option::<String>::None);
}

// =============================================================================
// HashMap<String, T>
// =============================================================================

#[test]
fn test_hashmap_round_trip() {
    let mut map = HashMap::new();
    map.insert("one".to_string(), 1i32);
    map.insert("two".to_string(), 2i32);
    round_trip(&map);
}

#[test]
fn test_empty_hashmap() {
    round_trip(&HashMap::<String, i32>::new());
}

// =============================================================================
// Empty containers
// =============================================================================

#[test]
fn test_empty_string() {
    round_trip(&String::new());
}

// =============================================================================
// Error cases
// =============================================================================

#[test]
fn test_truncated_input() {
    // A single byte that starts a u32 encoding but is truncated
    let result = from_slice::<u32>(&[0x19]); // 2-byte uint header, but no payload
    assert!(result.is_err());
}

#[test]
fn test_type_mismatch() {
    // CBOR text string where u32 is expected
    let text_bytes = to_vec(&"hello".to_string()).unwrap();
    let result = from_slice::<u32>(&text_bytes);
    assert!(result.is_err());
}

#[test]
fn test_empty_input() {
    let result = from_slice::<u32>(&[]);
    assert!(result.is_err());
}
